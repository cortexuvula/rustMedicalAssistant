use std::sync::Arc;

use medical_sharing::mdns::DiscoveredServer;
use medical_sharing::qr::{PairPayload, PairPorts, encode};
use medical_sharing::{SharingConfig, SharingService, SharingStatus};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SharingStatusDto {
    pub enabled: bool,
    pub ollama_ok: bool,
    pub whisper_ok: bool,
    pub mdns_ok: bool,
    pub pairing_ok: bool,
    pub paired_clients: u32,
}

impl From<SharingStatus> for SharingStatusDto {
    fn from(s: SharingStatus) -> Self {
        Self {
            enabled: s.enabled,
            ollama_ok: s.ollama_ok,
            whisper_ok: s.whisper_ok,
            mdns_ok: s.mdns_ok,
            pairing_ok: s.pairing_ok,
            paired_clients: s.paired_clients,
        }
    }
}

#[tauri::command]
pub async fn start_sharing(
    state: State<'_, AppState>,
    friendly_name: String,
) -> Result<(), String> {
    let cfg = build_sharing_config(&state, friendly_name)
        .await
        .map_err(|e| e.to_string())?;
    let service = Arc::new(SharingService::new(cfg).map_err(|e| e.to_string())?);
    service.start().await.map_err(|e| e.to_string())?;
    *state.sharing.write().await = Some(service);
    Ok(())
}

#[tauri::command]
pub async fn stop_sharing(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(s) = state.sharing.write().await.take() {
        s.stop().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn sharing_status(state: State<'_, AppState>) -> Result<SharingStatusDto, String> {
    if let Some(s) = state.sharing.read().await.as_ref() {
        Ok(s.status().await.into())
    } else {
        Ok(SharingStatusDto {
            enabled: false,
            ollama_ok: false,
            whisper_ok: false,
            mdns_ok: false,
            pairing_ok: false,
            paired_clients: 0,
        })
    }
}

#[tauri::command]
pub async fn pairing_qr(state: State<'_, AppState>) -> Result<String, String> {
    let svc = state.sharing.read().await;
    let svc = svc.as_ref().ok_or("sharing not running")?;
    let code = svc.pairing_state().issue_code().await;
    let cfg = svc.config();
    let lan = local_lan_address();
    let payload = PairPayload {
        host: cfg.friendly_name.clone(),
        lan,
        tailscale: tailscale_address().await,
        ports: PairPorts {
            ollama: cfg.ollama_proxy_port,
            whisper: cfg.whisper_proxy_port,
            pairing: cfg.pairing_port,
            lmstudio: cfg.lmstudio_port,
        },
        code,
    };
    Ok(encode(&payload))
}

#[derive(Debug, Serialize)]
pub struct ClientDto {
    pub id: i64,
    pub label: String,
}

#[tauri::command]
pub async fn list_paired_clients(state: State<'_, AppState>) -> Result<Vec<ClientDto>, String> {
    let svc = state.sharing.read().await;
    let svc = svc.as_ref().ok_or("sharing not running")?;
    let rows = svc.token_store().list().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| ClientDto {
            id: r.id,
            label: r.label,
        })
        .collect())
}

#[tauri::command]
pub async fn revoke_client(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let svc = state.sharing.read().await;
    let svc = svc.as_ref().ok_or("sharing not running")?;
    svc.token_store().revoke(id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn discover_servers(timeout_ms: u64) -> Result<Vec<DiscoveredServer>, String> {
    let mut rx =
        medical_sharing::mdns::browse(std::time::Duration::from_millis(timeout_ms))
            .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(d) = rx.recv().await {
        out.push(d);
    }
    Ok(out)
}

#[tauri::command]
pub async fn pair_with_server(
    server_url: String,
    code: String,
    label: String,
) -> Result<String, String> {
    let body = serde_json::json!({ "code": code, "label": label });
    let resp = reqwest::Client::new()
        .post(format!("{server_url}/pair/enroll"))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("server rejected pair: {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v.get("token")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string())
}

async fn build_sharing_config(
    _state: &AppState,
    friendly_name: String,
) -> Result<SharingConfig, String> {
    use medical_security::keychain;
    use rand::RngCore;

    // Reuse the SQLCipher DB key as the sharing-store key — same keychain
    // entry, no new secret to manage.
    let key = keychain::get_db_key()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "db key missing from keychain".to_string())?;

    let app_data = dirs::data_dir()
        .ok_or_else(|| "no app data dir".to_string())?
        .join("rust-medical-assistant");
    std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    let mut whisper_api = [0u8; 16];
    rand::thread_rng()
        .try_fill_bytes(&mut whisper_api)
        .map_err(|e| e.to_string())?;
    Ok(SharingConfig {
        enabled: true,
        friendly_name,
        ollama_proxy_port: 11435,
        whisper_proxy_port: 8081,
        pairing_port: 11436,
        whisper_internal_port: 8080,
        lmstudio_port: lmstudio_running_port().await,
        token_store_path: app_data.join("sharing.db"),
        token_store_key: key,
        binary_dir: app_data.join("bin"),
        whisper_model_path: app_data.join("models/whisper/ggml-large-v3-turbo.bin"),
        whisper_internal_api_key: hex::encode(whisper_api),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

fn local_lan_address() -> Option<String> {
    use std::net::UdpSocket;
    // Standard "connect to a public IP, read our outbound IP" trick. Doesn't actually transmit.
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

async fn tailscale_address() -> Option<String> {
    let out = tokio::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("Self")?
        .get("DNSName")?
        .as_str()
        .map(|s| s.trim_end_matches('.').to_string())
}

async fn lmstudio_running_port() -> Option<u16> {
    let resp = reqwest::Client::new()
        .get("http://127.0.0.1:1234/v1/models")
        .timeout(std::time::Duration::from_millis(300))
        .send()
        .await
        .ok()?;
    if resp.status().is_success() {
        Some(1234)
    } else {
        None
    }
}
