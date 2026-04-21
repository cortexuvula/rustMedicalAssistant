pub mod audio;
pub mod chat;
pub mod context_templates;
pub mod export;
pub mod generation;
pub mod logging;
pub mod models;
pub mod pipeline;
pub mod providers;
pub mod rag;
pub mod recordings;
pub mod settings;
pub mod transcription;
pub mod vocabulary;

use std::path::PathBuf;

use medical_db::Database;

/// Resolve the recordings directory from settings.
///
/// If the user has configured a custom `storage_path`, use it.
/// Otherwise fall back to `{data_dir}/recordings`.
pub fn resolve_recordings_dir(db: &Database, data_dir: &PathBuf) -> Result<PathBuf, String> {
    let dir = if let Ok(conn) = db.conn() {
        medical_db::settings::SettingsRepo::load_config(&conn)
            .ok()
            .map(|mut c| { c.migrate(); c })
            .and_then(|cfg| cfg.storage_path.filter(|s| !s.is_empty()))
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("recordings"))
    } else {
        data_dir.join("recordings")
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create recordings dir: {e}"))?;
    Ok(dir)
}
