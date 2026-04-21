use medical_core::types::settings::AppConfig;
use medical_db::settings::SettingsRepo;

use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppConfig, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let mut config = SettingsRepo::load_config(&conn).map_err(|e| e.to_string())?;
    config.migrate();
    Ok(config)
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

/// Return the built-in default system prompt for the given document type.
///
/// `doc_type` must be one of: "soap", "referral", "letter", "synopsis".
#[tauri::command]
pub fn get_default_prompt(doc_type: String) -> Result<String, String> {
    use medical_processing::document_generator::{
        default_letter_prompt, default_referral_prompt, default_synopsis_prompt,
    };
    use medical_processing::soap_generator::default_soap_prompt;

    match doc_type.as_str() {
        "soap" => Ok(default_soap_prompt().to_string()),
        "referral" => Ok(default_referral_prompt().to_string()),
        "letter" => Ok(default_letter_prompt().to_string()),
        "synopsis" => Ok(default_synopsis_prompt().to_string()),
        _ => Err(format!("Unknown doc_type: {}", doc_type)),
    }
}
