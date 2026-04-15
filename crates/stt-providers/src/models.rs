use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::SttError;

// ---------------------------------------------------------------------------
// WhisperModelId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhisperModelId {
    Base,
    Small,
    Medium,
    LargeV3Turbo,
}

impl WhisperModelId {
    pub fn as_str(&self) -> &'static str {
        match self {
            WhisperModelId::Base => "base",
            WhisperModelId::Small => "small",
            WhisperModelId::Medium => "medium",
            WhisperModelId::LargeV3Turbo => "large-v3-turbo",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "base" => Some(WhisperModelId::Base),
            "small" => Some(WhisperModelId::Small),
            "medium" => Some(WhisperModelId::Medium),
            "large-v3-turbo" => Some(WhisperModelId::LargeV3Turbo),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ModelInfo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub filename: String,
    pub size_bytes: u64,
    pub download_url: String,
    pub description: String,
    pub downloaded: bool,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

pub fn models_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models")
}

pub fn whisper_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models").join("whisper")
}

pub fn pyannote_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models").join("pyannote")
}

pub fn whisper_model_path(app_data_dir: &Path, filename: &str) -> PathBuf {
    whisper_dir(app_data_dir).join(filename)
}

pub fn pyannote_model_path(app_data_dir: &Path, filename: &str) -> PathBuf {
    pyannote_dir(app_data_dir).join(filename)
}

// ---------------------------------------------------------------------------
// Filename mapping
// ---------------------------------------------------------------------------

pub fn whisper_model_filename(model_id: &str) -> Option<&'static str> {
    match model_id {
        "base" => Some("ggml-base.bin"),
        "small" => Some("ggml-small.bin"),
        "medium" => Some("ggml-medium.bin"),
        "large-v3-turbo" => Some("ggml-large-v3-turbo.bin"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Available models
// ---------------------------------------------------------------------------

pub fn available_whisper_models(app_data_dir: &Path) -> Vec<ModelInfo> {
    let models_raw = [
        (
            "base",
            "ggml-base.bin",
            147_951_465u64,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            "Whisper Base (~148 MB) — fast, lower accuracy",
        ),
        (
            "small",
            "ggml-small.bin",
            487_601_905u64,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            "Whisper Small (~488 MB) — balanced speed and accuracy",
        ),
        (
            "medium",
            "ggml-medium.bin",
            1_533_774_081u64,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
            "Whisper Medium (~1.5 GB) — high accuracy",
        ),
        (
            "large-v3-turbo",
            "ggml-large-v3-turbo.bin",
            1_622_081_537u64,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
            "Whisper Large-v3-Turbo (~1.6 GB) — best accuracy",
        ),
    ];

    models_raw
        .iter()
        .map(|(id, filename, size_bytes, url, description)| {
            let path = whisper_model_path(app_data_dir, filename);
            ModelInfo {
                id: id.to_string(),
                filename: filename.to_string(),
                size_bytes: *size_bytes,
                download_url: url.to_string(),
                description: description.to_string(),
                downloaded: path.exists(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Check required models
// ---------------------------------------------------------------------------

pub fn check_required_models(app_data_dir: &Path, whisper_model_id: &str) -> Vec<String> {
    let mut missing = Vec::new();

    // Check requested whisper model
    if let Some(filename) = whisper_model_filename(whisper_model_id) {
        let path = whisper_model_path(app_data_dir, filename);
        if !path.exists() {
            missing.push(format!(
                "Whisper model '{}' ({})",
                whisper_model_id, filename
            ));
        }
    }

    // Pyannote stub models (diarization — currently not available but reserved)
    let pyannote_stubs = [
        ("segmentation-3.0.onnx", "Pyannote segmentation model"),
        ("embedding.onnx", "Pyannote speaker embedding model"),
    ];
    for (filename, description) in &pyannote_stubs {
        let path = pyannote_model_path(app_data_dir, filename);
        if !path.exists() {
            missing.push(description.to_string());
        }
    }

    missing
}

// ---------------------------------------------------------------------------
// Download / delete
// ---------------------------------------------------------------------------

pub async fn download_model<F>(
    url: &str,
    dest_path: &Path,
    on_progress: F,
) -> Result<(), SttError>
where
    F: Fn(u64, u64) + Send + 'static,
{
    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;

    // Ensure parent directory exists
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            SttError::ModelDownload(format!("Failed to create model directory: {e}"))
        })?;
    }

    let tmp_path = dest_path.with_extension("tmp");

    let response = reqwest::get(url).await.map_err(|e| {
        SttError::ModelDownload(format!("Failed to start download from {url}: {e}"))
    })?;

    if !response.status().is_success() {
        return Err(SttError::ModelDownload(format!(
            "HTTP {} downloading {url}",
            response.status()
        )));
    }

    let total_bytes = response.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&tmp_path).await.map_err(|e| {
        SttError::ModelDownload(format!(
            "Failed to create temporary file {}: {e}",
            tmp_path.display()
        ))
    })?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| {
            SttError::ModelDownload(format!("Stream error while downloading {url}: {e}"))
        })?;

        file.write_all(&bytes).await.map_err(|e| {
            SttError::ModelDownload(format!("Failed to write to {}: {e}", tmp_path.display()))
        })?;

        downloaded += bytes.len() as u64;
        on_progress(downloaded, total_bytes);
    }

    file.flush().await.map_err(|e| {
        SttError::ModelDownload(format!("Failed to flush {}: {e}", tmp_path.display()))
    })?;

    // Atomic rename
    tokio::fs::rename(&tmp_path, dest_path).await.map_err(|e| {
        SttError::ModelDownload(format!(
            "Failed to rename {} -> {}: {e}",
            tmp_path.display(),
            dest_path.display()
        ))
    })?;

    Ok(())
}

pub async fn delete_model(path: &Path) -> Result<(), SttError> {
    if !path.exists() {
        return Err(SttError::ModelNotFound(format!(
            "Model file not found: {}",
            path.display()
        )));
    }

    tokio::fs::remove_file(path).await.map_err(|e| {
        SttError::ModelDownload(format!("Failed to delete {}: {e}", path.display()))
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn whisper_model_filenames() {
        assert_eq!(whisper_model_filename("base"), Some("ggml-base.bin"));
        assert_eq!(whisper_model_filename("small"), Some("ggml-small.bin"));
        assert_eq!(whisper_model_filename("medium"), Some("ggml-medium.bin"));
        assert_eq!(
            whisper_model_filename("large-v3-turbo"),
            Some("ggml-large-v3-turbo.bin")
        );
        assert_eq!(whisper_model_filename("unknown"), None);
    }

    #[test]
    fn path_resolution() {
        let base = Path::new("/tmp/app_data");
        assert_eq!(models_dir(base), Path::new("/tmp/app_data/models"));
        assert_eq!(whisper_dir(base), Path::new("/tmp/app_data/models/whisper"));
        assert_eq!(
            pyannote_dir(base),
            Path::new("/tmp/app_data/models/pyannote")
        );
        assert_eq!(
            whisper_model_path(base, "ggml-base.bin"),
            Path::new("/tmp/app_data/models/whisper/ggml-base.bin")
        );
        assert_eq!(
            pyannote_model_path(base, "segmentation.onnx"),
            Path::new("/tmp/app_data/models/pyannote/segmentation.onnx")
        );
    }

    #[test]
    fn available_models_list() {
        // Use a path that definitely does not exist so downloaded = false for all
        let base = Path::new("/tmp/__nonexistent_ferriscribe_test_dir__");
        let models = available_whisper_models(base);
        assert_eq!(models.len(), 4);
        assert!(models.iter().all(|m| !m.downloaded));
    }

    #[test]
    fn check_missing_models() {
        let base = Path::new("/tmp/__nonexistent_ferriscribe_test_dir__");
        let missing = check_required_models(base, "base");
        // whisper base + 2 pyannote stubs = 3 missing
        assert_eq!(missing.len(), 3);
    }

    #[test]
    fn whisper_model_id_roundtrip() {
        let ids = [
            WhisperModelId::Base,
            WhisperModelId::Small,
            WhisperModelId::Medium,
            WhisperModelId::LargeV3Turbo,
        ];
        for id in &ids {
            let s = id.as_str();
            let roundtripped = WhisperModelId::from_str(s).expect("from_str failed");
            assert_eq!(*id, roundtripped);
        }
        assert!(WhisperModelId::from_str("nonexistent").is_none());
    }
}
