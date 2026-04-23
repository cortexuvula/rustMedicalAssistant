use std::time::Duration;

use tracing::info;

use crate::state::{self, AppState};

/// Rebuild AI + STT provider registries (e.g. after LM Studio host/port changes).
///
/// Returns the list of available AI provider names after reinitialization.
#[tauri::command]
pub async fn reinit_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // Load saved settings for provider config (host, port, active provider, whisper model)
    let config = {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        let mut cfg = medical_db::settings::SettingsRepo::load_config(&conn)
            .map_err(|e| e.to_string())?;
        cfg.migrate();
        cfg
    };

    // Rebuild AI providers with current config (includes LM Studio host/port)
    let mut ai_registry = state::init_ai_providers(&config);

    // Restore the user's active provider preference from saved settings
    // so reinit doesn't silently switch to a random provider.
    ai_registry.set_active(&config.ai_provider);

    let available = ai_registry.list_available();
    {
        let mut guard = state.ai_providers.lock().await;
        *guard = ai_registry;
    }

    // Rebuild STT provider based on current config (mode + whisper model + remote host/port/key).
    let stt = state::init_stt_providers_with_config(&state.data_dir, &config);
    {
        let mut guard = state.stt_providers.lock().await;
        *guard = stt;
    }

    info!(providers = ?available, "Providers reinitialized");

    Ok(available)
}

/// Test connectivity to an LM Studio server.
///
/// Makes a GET request to `http://{host}:{port}/v1/models` with a 5-second
/// timeout. Returns a success message with the model count, or an error.
#[tauri::command]
pub async fn test_lmstudio_connection(host: String, port: u16) -> Result<String, String> {
    let effective_host = if host.is_empty() { "localhost".to_string() } else { host };
    let url = format!("http://{}:{}/v1/models", effective_host, port);

    info!(url = %url, "Testing LM Studio connection");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            if e.is_connect() {
                format!("Connection refused — is LM Studio running at {}:{}?", effective_host, port)
            } else if e.is_timeout() {
                format!("Connection timed out — check that {}:{} is reachable", effective_host, port)
            } else {
                format!("Connection failed: {e}")
            }
        })?;

    if !response.status().is_success() {
        return Err(format!("Server returned HTTP {}", response.status()));
    }

    // Parse the OpenAI-compatible models response to count models
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from server: {e}"))?;

    let model_count = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(format!("Connected — {} model{} available", model_count, if model_count == 1 { "" } else { "s" }))
}

/// Test connectivity to a remote Whisper server (OpenAI-compatible).
///
/// Makes a GET request to `http://{host}:{port}/v1/models` with a 5-second
/// connect timeout and 10-second overall timeout. If `api_key` is present and
/// non-empty, an `Authorization: Bearer …` header is sent.
#[tauri::command]
pub async fn test_stt_remote_connection(
    host: String,
    port: u16,
    api_key: Option<String>,
) -> Result<String, String> {
    let effective_host = if host.is_empty() { "localhost".to_string() } else { host };
    let url = format!("http://{}:{}/v1/models", effective_host, port);

    info!(url = %url, "Testing Whisper server connection");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut req = client.get(&url);
    if let Some(key) = api_key.as_deref().filter(|s| !s.is_empty()) {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let response = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Connection refused — is the Whisper server running at {}:{}?", effective_host, port)
        } else if e.is_timeout() {
            format!("Connection timed out — check that {}:{} is reachable", effective_host, port)
        } else {
            format!("Connection failed: {e}")
        }
    })?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err("Authentication failed — check API key".into());
    }
    if !response.status().is_success() {
        return Err(format!("Server returned HTTP {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from server: {e}"))?;

    let model_count = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(format!(
        "Connected — {} model{} available",
        model_count,
        if model_count == 1 { "" } else { "s" }
    ))
}

/// Test connectivity to an Ollama server.
///
/// Makes a GET request to `http://{host}:{port}/api/tags` with a 5-second
/// timeout. Returns a success message including the installed-model count,
/// or a user-readable error.
#[tauri::command]
pub async fn test_ollama_connection(host: String, port: u16) -> Result<String, String> {
    let effective_host = if host.is_empty() { "localhost".to_string() } else { host };
    let url = format!("http://{}:{}/api/tags", effective_host, port);

    info!(url = %url, "Testing Ollama connection");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client.get(&url).send().await.map_err(|e| {
        if e.is_connect() {
            format!("Connection refused — is Ollama running at {}:{}?", effective_host, port)
        } else if e.is_timeout() {
            format!("Connection timed out — check that {}:{} is reachable", effective_host, port)
        } else {
            format!("Connection failed: {e}")
        }
    })?;

    if !response.status().is_success() {
        return Err(format!("Server returned HTTP {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from server: {e}"))?;

    let model_count = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(format!(
        "Connected — {} model{} installed",
        model_count,
        if model_count == 1 { "" } else { "s" }
    ))
}
