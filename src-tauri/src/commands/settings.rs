use medical_core::error::{AppError, AppResult};
use medical_core::types::settings::AppConfig;
use medical_db::settings::SettingsRepo;

use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> AppResult<AppConfig> {
    let conn = state.db.conn().map_err(|e| AppError::Database(e.to_string()))?;
    let mut config = SettingsRepo::load_config(&conn)
        .map_err(|e| AppError::Database(e.to_string()))?;
    config.migrate();
    Ok(config)
}

#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> AppResult<()> {
    let conn = state.db.conn().map_err(|e| AppError::Database(e.to_string()))?;
    SettingsRepo::save_config(&conn, &config).map_err(|e| AppError::Database(e.to_string()))
}

#[tauri::command]
pub fn get_api_key(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> AppResult<Option<String>> {
    state
        .keys
        .get_key(&provider)
        .map_err(|e| AppError::Security(e.to_string()))
}

#[tauri::command]
pub fn set_api_key(
    state: tauri::State<'_, AppState>,
    provider: String,
    key: String,
) -> AppResult<()> {
    state
        .keys
        .store_key(&provider, &key)
        .map_err(|e| AppError::Security(e.to_string()))
}

#[tauri::command]
pub fn list_api_keys(state: tauri::State<'_, AppState>) -> AppResult<Vec<String>> {
    state
        .keys
        .list_providers()
        .map_err(|e| AppError::Security(e.to_string()))
}

/// Return the built-in default system prompt for the given document type.
///
/// `doc_type` must be one of: "soap", "referral", "letter", "synopsis".
#[tauri::command]
pub fn get_default_prompt(doc_type: String) -> AppResult<String> {
    use medical_processing::document_generator::{
        default_letter_prompt, default_referral_prompt, default_synopsis_prompt,
    };
    use medical_processing::soap_generator::default_soap_prompt;

    match doc_type.as_str() {
        "soap" => Ok(default_soap_prompt().to_string()),
        "referral" => Ok(default_referral_prompt().to_string()),
        "letter" => Ok(default_letter_prompt().to_string()),
        "synopsis" => Ok(default_synopsis_prompt().to_string()),
        _ => Err(AppError::Config(format!("Unknown doc_type: {}", doc_type))),
    }
}
