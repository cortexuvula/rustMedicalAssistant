use medical_core::types::settings::AppConfig;
use medical_db::settings::SettingsRepo;

use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppConfig, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SettingsRepo::load_config(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SettingsRepo::save_config(&conn, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_api_key(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> Result<Option<String>, String> {
    state.keys.get_key(&provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_api_key(
    state: tauri::State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<(), String> {
    state.keys.store_key(&provider, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_api_keys(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    state.keys.list_providers().map_err(|e| e.to_string())
}
