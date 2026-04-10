use medical_core::types::recording::{Recording, RecordingSummary};
use medical_db::recordings::RecordingsRepo;
use medical_db::search::SearchRepo;
use medical_db::vectors::VectorsRepo;
use uuid::Uuid;

use crate::state::AppState;

#[tauri::command]
pub fn list_recordings(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<RecordingSummary>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    RecordingsRepo::list_all(&conn, limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recording(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Recording, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_recordings(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<Recording>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SearchRepo::search_recordings(&conn, &query, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_recording(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let conn = state.db.conn().map_err(|e| e.to_string())?;

    // Get the recording first so we can clean up the WAV file
    let recording = RecordingsRepo::get_by_id(&conn, &uuid).ok();

    // Delete RAG chunks indexed from this recording
    let _ = VectorsRepo::delete_by_document(&conn, &id);

    // Delete the DB row (transcript, SOAP, referral, letter are all columns)
    RecordingsRepo::delete(&conn, &uuid).map_err(|e| e.to_string())?;

    // Delete the WAV file from disk
    if let Some(rec) = recording {
        if rec.audio_path.exists() {
            let _ = std::fs::remove_file(&rec.audio_path);
        }
    }

    Ok(())
}
