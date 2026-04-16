use std::path::PathBuf;

use medical_core::types::recording::{ProcessingStatus, Recording, RecordingSummary};
use medical_db::recordings::RecordingsRepo;
use medical_db::search::SearchRepo;
use medical_db::vectors::VectorsRepo;
use uuid::Uuid;

use crate::state::AppState;
use super::resolve_recordings_dir;

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

#[tauri::command]
pub fn delete_all_recordings(
    state: tauri::State<'_, AppState>,
) -> Result<u32, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;

    // Delete all RAG vectors
    let _ = conn.execute("DELETE FROM vectors", []);

    // Delete all recordings and get audio paths for file cleanup
    let paths = RecordingsRepo::delete_all(&conn).map_err(|e| e.to_string())?;
    let count = paths.len() as u32;

    // Remove audio files from disk
    for path in &paths {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    Ok(count)
}

/// Import an audio file from the filesystem into the recordings library.
///
/// Non-WAV files (MP3, FLAC, OGG, M4A, AAC) are automatically converted to
/// WAV so the transcription pipeline can process them.  Creates a Recording
/// entry in the database and returns the new recording ID.
#[tauri::command]
pub fn import_audio_file(
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<String, String> {
    let source = PathBuf::from(&file_path);
    if !source.exists() {
        return Err(format!("File not found: {file_path}"));
    }

    // Resolve recordings directory from settings (custom path or default).
    let recordings_dir = resolve_recordings_dir(&state.db, &state.data_dir)?;

    let original_name = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "imported".to_string());

    let recording_id = Uuid::new_v4();
    let short_id = &recording_id.to_string()[..8];

    // Determine if we need to convert to WAV.
    let dest_path = if medical_audio::convert::is_wav_file(&source) {
        // Already WAV — just copy.
        let dest_filename = format!("{original_name}_{short_id}.wav");
        let dest = recordings_dir.join(&dest_filename);
        std::fs::copy(&source, &dest)
            .map_err(|e| format!("Failed to copy file: {e}"))?;
        dest
    } else {
        // Non-WAV — convert to WAV.
        let dest_filename = format!("{original_name}_{short_id}.wav");
        let dest = recordings_dir.join(&dest_filename);
        medical_audio::convert::convert_to_wav(&source, &dest)
            .map_err(|e| format!("Failed to convert audio: {e}"))?;
        dest
    };

    // Read duration and file size from the resulting WAV.
    let file_size = std::fs::metadata(&dest_path).map(|m| m.len()).ok();
    let duration = hound::WavReader::open(&dest_path)
        .ok()
        .map(|reader| {
            let spec = reader.spec();
            let total_samples = reader.len() as f64;
            if spec.sample_rate > 0 && spec.channels > 0 {
                total_samples / (spec.sample_rate as f64 * spec.channels as f64)
            } else {
                0.0
            }
        });

    let dest_filename = dest_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{original_name}_{short_id}.wav"));

    // Create the Recording entry.
    let mut recording = Recording::new(dest_filename, dest_path);
    recording.id = recording_id;
    recording.duration_seconds = duration;
    recording.file_size_bytes = file_size;
    recording.status = ProcessingStatus::Pending;

    let conn = state.db.conn().map_err(|e| e.to_string())?;
    RecordingsRepo::insert(&conn, &recording).map_err(|e| e.to_string())?;

    Ok(recording_id.to_string())
}
