//! Tauri commands for model management: list, download, delete.

use serde::Serialize;
use tauri::Emitter;

use medical_stt_providers::models as stt_models;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
struct ModelDownloadProgress {
    model_id: String,
    downloaded_bytes: u64,
    total_bytes: u64,
}

/// List all available whisper models with download status.
#[tauri::command]
pub async fn list_whisper_models(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<stt_models::ModelInfo>, String> {
    Ok(stt_models::available_whisper_models(&state.data_dir))
}

/// Download a model by ID.
///
/// Emits `model-download-progress` events with `{ model_id, downloaded_bytes, total_bytes }`.
#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let data_dir = state.data_dir.clone();

    // Find the model info from available whisper models
    let all_models = stt_models::available_whisper_models(&data_dir);

    let model = all_models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Unknown model ID: {model_id}"))?
        .clone();

    // Determine destination path
    let dest_path = stt_models::whisper_model_path(&data_dir, &model.filename);

    if dest_path.exists() {
        return Ok(()); // Already downloaded
    }

    let mid = model_id.clone();
    let app_clone = app.clone();

    stt_models::download_model(&model.download_url, &dest_path, move |downloaded, total| {
        let _ = app_clone.emit(
            "model-download-progress",
            ModelDownloadProgress {
                model_id: mid.clone(),
                downloaded_bytes: downloaded,
                total_bytes: total,
            },
        );
    })
    .await
    .map_err(|e| e.to_string())?;

    // After downloading a whisper model, reinitialize the STT provider so it picks up the new model
    let whisper_model = {
        let conn = state.db.conn().ok();
        conn.and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
            .map(|cfg| cfg.whisper_model)
            .unwrap_or_else(|| "large-v3-turbo".into())
    };
    let stt = crate::state::init_stt_providers(&state.data_dir, &whisper_model);
    {
        let mut guard = state.stt_providers.lock().await;
        *guard = stt;
    }

    Ok(())
}

/// Delete a downloaded model to reclaim disk space.
#[tauri::command]
pub async fn delete_model(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let data_dir = state.data_dir.clone();

    let all_models = stt_models::available_whisper_models(&data_dir);

    let model = all_models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Unknown model ID: {model_id}"))?;

    let path = stt_models::whisper_model_path(&data_dir, &model.filename);

    stt_models::delete_model(&path)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
