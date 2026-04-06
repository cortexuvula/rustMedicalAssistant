use crate::state::{self, AppState};

/// Re-read API keys from storage and rebuild AI + STT provider registries.
///
/// Returns the list of available AI provider names after reinitialization.
#[tauri::command]
pub async fn reinit_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // Rebuild AI providers
    let ai_registry = state::init_ai_providers(&state.keys);
    let available = ai_registry.list_available();
    {
        let mut guard = state.ai_providers.lock().await;
        *guard = ai_registry;
    }

    // Rebuild STT failover chain
    let stt = state::init_stt_providers(&state.keys);
    {
        let mut guard = state.stt_providers.lock().await;
        *guard = stt;
    }

    Ok(available)
}
