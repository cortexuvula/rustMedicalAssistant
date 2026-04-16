use std::time::Duration;

use tracing::info;

use crate::state::{self, AppState};

/// Re-read API keys from storage and rebuild AI + STT provider registries.
///
/// Returns the list of available AI provider names after reinitialization.
#[tauri::command]
pub async fn reinit_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // Load saved settings for provider config (host, port, active provider, whisper model)
    let config = {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        medical_db::settings::SettingsRepo::load_config(&conn)
            .map_err(|e| e.to_string())?
    };

    // Rebuild AI providers with current config (includes LM Studio host/port)
    let mut ai_registry = state::init_ai_providers(&state.keys, &config);

    // Restore the user's active provider preference from saved settings
    // so reinit doesn't silently switch to a random provider.
    ai_registry.set_active(&config.ai_provider);

    let available = ai_registry.list_available();
    {
        let mut guard = state.ai_providers.lock().await;
        *guard = ai_registry;
    }

    // Rebuild local STT provider with current whisper model setting
    let stt = state::init_stt_providers(&state.data_dir, &config.whisper_model);
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
