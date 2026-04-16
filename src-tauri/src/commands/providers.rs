use crate::state::{self, AppState};

/// Re-read API keys from storage and rebuild AI + STT provider registries.
///
/// Returns the list of available AI provider names after reinitialization.
#[tauri::command]
pub async fn reinit_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // Rebuild AI providers
    let mut ai_registry = state::init_ai_providers(&state.keys);

    // Restore the user's active provider preference from saved settings
    // so reinit doesn't silently switch to a random provider.
    {
        let conn = state.db.conn().ok();
        if let Some(provider_name) = conn
            .and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
            .map(|cfg| cfg.ai_provider)
        {
            ai_registry.set_active(&provider_name);
        }
    }

    let available = ai_registry.list_available();
    {
        let mut guard = state.ai_providers.lock().await;
        *guard = ai_registry;
    }

    // Rebuild local STT provider with current whisper model setting
    let whisper_model = {
        let conn = state.db.conn().ok();
        conn.and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
            .map(|cfg| cfg.whisper_model)
            .unwrap_or_else(|| "large-v3-turbo".into())
    };

    let stt = state::init_stt_providers(&state.data_dir, &whisper_model);
    {
        let mut guard = state.stt_providers.lock().await;
        *guard = stt;
    }

    Ok(available)
}
