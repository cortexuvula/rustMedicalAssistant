use std::sync::Arc;

use medical_db::settings::SettingsRepo;

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

    // Read preferred STT provider from saved settings
    let preferred_stt = {
        let conn = state.db.conn().ok();
        conn.and_then(|c| SettingsRepo::load_config(&c).ok())
            .map(|cfg| cfg.stt_provider)
            .unwrap_or_else(|| "deepgram".into())
    };

    // Rebuild STT failover chain with preferred provider first
    let stt = state::init_stt_providers(&state.keys, &preferred_stt);
    {
        let mut guard = state.stt_providers.lock().await;
        *guard = stt.map(Arc::new);
    }

    Ok(available)
}
