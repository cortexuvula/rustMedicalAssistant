# Local STT Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all cloud STT providers (Deepgram, ElevenLabs, Groq, Modulate) and the failover chain with a fully local speech-to-text pipeline using whisper-rs and pyannote-rs for speaker diarization.

**Architecture:** Two-stage pipeline — whisper-rs transcription (Metal GPU-accelerated) followed by pyannote-rs ONNX-based speaker diarization, merged by timestamp overlap. Models downloaded from HuggingFace on first use, stored in `{app_data_dir}/models/`. The existing `SttProvider` trait is preserved — `LocalSttProvider` implements it so the rest of the app doesn't change.

**Tech Stack:** whisper-rs 0.16 (Metal), pyannote-rs 0.3.4 (ONNX via ort), reqwest (model downloads)

**Note on diarization quality:** pyannote-rs uses a simple segmentation+embedding pipeline. For higher accuracy, `speakrs` (v0.4) implements the full pyannote community pipeline with PLDA+VBx clustering (~7% DER vs ~80%). The `diarization.rs` module wraps diarization behind a clean interface, making it straightforward to swap implementations later. Per the spec, diarization failure falls back gracefully (transcript without speaker labels).

---

## File Structure

### New files (create)

| File | Responsibility |
|------|---------------|
| `crates/stt-providers/src/audio_prep.rs` | Resample any AudioData to 16kHz mono f32 (and i16 for pyannote) |
| `crates/stt-providers/src/models.rs` | Model info, path resolution, download with progress, validation |
| `crates/stt-providers/src/whisper.rs` | whisper-rs transcription wrapper returning timestamped segments |
| `crates/stt-providers/src/diarization.rs` | pyannote-rs speaker diarization wrapper |
| `crates/stt-providers/src/merge.rs` | Align whisper segments with speaker turns by timestamp overlap |
| `crates/stt-providers/src/local_provider.rs` | `SttProvider` impl orchestrating the two-stage pipeline |
| `src-tauri/src/commands/models.rs` | Tauri commands: list_whisper_models, download_model, delete_model |
| `src/lib/api/models.ts` | Frontend invoke wrappers for model management commands |

### Modified files

| File | Change |
|------|--------|
| `crates/stt-providers/Cargo.toml` | Remove `local-stt` feature, make whisper-rs mandatory with `metal`, add pyannote-rs |
| `crates/stt-providers/src/lib.rs` | Remove old modules, export new modules and `LocalSttProvider` |
| `crates/core/src/types/settings.rs` | Remove `stt_provider`/`stt_failover_chain`, add `whisper_model` |
| `src-tauri/src/state.rs` | Remove cloud STT imports, simplify `init_stt_providers` to create `LocalSttProvider` |
| `src-tauri/src/commands/transcription.rs` | Use `LocalSttProvider` directly instead of `SttFailover` |
| `src-tauri/src/commands/providers.rs` | Remove STT rebuild from `reinit_providers` |
| `src-tauri/src/commands/mod.rs` | Add `pub mod models;` |
| `src-tauri/src/lib.rs` | Register new model management commands |
| `src/lib/types/index.ts` | Remove `stt_provider`, add `whisper_model` to `AppConfig` |
| `src/lib/stores/settings.ts` | Remove `stt_provider` default, add `whisper_model` default |
| `src/lib/components/SettingsContent.svelte` | Replace STT provider dropdown with model management UI |

### Deleted files

| File | Reason |
|------|--------|
| `crates/stt-providers/src/deepgram.rs` | Cloud provider removed |
| `crates/stt-providers/src/elevenlabs_stt.rs` | Cloud provider removed |
| `crates/stt-providers/src/groq_whisper.rs` | Cloud provider removed |
| `crates/stt-providers/src/modulate.rs` | Cloud provider removed |
| `crates/stt-providers/src/failover.rs` | Failover chain removed |
| `crates/stt-providers/src/whisper_local.rs` | Replaced by new whisper.rs + local_provider.rs |

---

### Task 1: Update Cargo.toml Dependencies

**Files:**
- Modify: `crates/stt-providers/Cargo.toml`

- [ ] **Step 1: Replace stt-providers Cargo.toml**

Replace the entire contents of `crates/stt-providers/Cargo.toml` with:

```toml
[package]
name = "medical-stt-providers"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
futures-core = { workspace = true }
reqwest = { workspace = true }

# Local STT
whisper-rs = { version = "0.16", features = ["metal"] }
pyannote-rs = "0.3"

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

Key changes:
- Removed: `local-stt` feature flag entirely
- Removed: `hound` (WAV encoding was used by cloud providers; WAV loading stays in src-tauri)
- Changed: `whisper-rs` from optional to mandatory, added `metal` feature
- Added: `pyannote-rs` for speaker diarization (brings in `ort`, `ndarray`, `knf-rs`)
- Kept: `reqwest` (now for model downloads instead of API calls)

- [ ] **Step 2: Verify the project compiles with new deps**

Run: `cd /Users/cortexuvula/Development/rustMedicalAssistant && cargo check -p medical-stt-providers 2>&1 | tail -20`

Expected: Compilation errors about missing modules (the old ones we haven't deleted yet and the new ones we haven't created). That's fine — we're validating that dependency resolution works. If you see dependency resolution errors (version conflicts, missing features), fix them before proceeding.

**Troubleshooting:**
- If `whisper-rs` `metal` feature fails: try without it first (`whisper-rs = "0.16"`) and add Metal back after confirming the base builds.
- If `pyannote-rs` has ort version conflicts: check if it needs a specific ort version pinned in workspace deps.
- If pyannote-rs pulls in `ort` with `load-dynamic` feature (requiring ONNX Runtime dylib at runtime), note this — we may need to bundle the dylib or override features.

- [ ] **Step 3: Commit**

```bash
git add crates/stt-providers/Cargo.toml
git commit -m "build: update stt-providers dependencies for local STT migration

Remove local-stt feature gate, make whisper-rs mandatory with Metal
acceleration, add pyannote-rs for speaker diarization."
```

---

### Task 2: Create audio_prep Module

**Files:**
- Create: `crates/stt-providers/src/audio_prep.rs`

This module extracts the resampling logic from the existing `whisper_local.rs` into a shared utility. Both whisper (needs f32) and pyannote (needs i16) will use it.

- [ ] **Step 1: Write tests for audio_prep**

Create `crates/stt-providers/src/audio_prep.rs` with tests at the bottom:

```rust
//! Audio preprocessing: resample to 16 kHz mono for whisper and pyannote.

use medical_core::types::AudioData;

/// Resample audio to 16 kHz mono f32.
///
/// Uses linear interpolation — good enough for speech. For production quality
/// consider a proper resampler (e.g. rubato).
pub fn to_16k_mono_f32(audio: &AudioData) -> Vec<f32> {
    let channels = audio.channels.max(1) as usize;

    // Mix down to mono by averaging channels.
    let mono: Vec<f32> = if channels > 1 {
        audio
            .samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        audio.samples.clone()
    };

    let src_rate = audio.sample_rate as f64;
    let dst_rate = 16_000.0_f64;

    if (src_rate - dst_rate).abs() < 1.0 {
        return mono;
    }

    let ratio = src_rate / dst_rate;
    let out_len = (mono.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_idx = i as f64 * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(mono.len().saturating_sub(1));
        let frac = (src_idx - idx0 as f64) as f32;
        out.push(mono[idx0] * (1.0 - frac) + mono[idx1] * frac);
    }

    out
}

/// Convert f32 PCM samples to i16 (for pyannote-rs which expects i16 input).
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s * 32_767.0).clamp(-32_768.0, 32_767.0) as i16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_16k_mono() {
        let audio = AudioData {
            samples: vec![0.5, -0.5, 0.25],
            sample_rate: 16000,
            channels: 1,
        };
        let result = to_16k_mono_f32(&audio);
        assert_eq!(result.len(), 3);
        assert!((result[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stereo_to_mono() {
        // Stereo: L=1.0, R=0.0 → mono=0.5
        let audio = AudioData {
            samples: vec![1.0, 0.0, 0.6, 0.4],
            sample_rate: 16000,
            channels: 2,
        };
        let result = to_16k_mono_f32(&audio);
        assert_eq!(result.len(), 2);
        assert!((result[0] - 0.5).abs() < 1e-6);
        assert!((result[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn downsample_44100_to_16000() {
        let n = 44100; // 1 second at 44.1 kHz
        let audio = AudioData {
            samples: vec![0.0; n],
            sample_rate: 44100,
            channels: 1,
        };
        let result = to_16k_mono_f32(&audio);
        // Should produce ~16000 samples (1 second at 16 kHz)
        assert!(result.len() >= 15999 && result.len() <= 16001);
    }

    #[test]
    fn f32_to_i16_conversion() {
        let samples = vec![1.0, -1.0, 0.0, 0.5];
        let result = f32_to_i16(&samples);
        assert_eq!(result[0], 32767);
        assert_eq!(result[1], -32767); // clamped to -32768 range
        assert_eq!(result[2], 0);
        assert_eq!(result[3], 16383); // 0.5 * 32767 ≈ 16383
    }

    #[test]
    fn empty_audio() {
        let audio = AudioData {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let result = to_16k_mono_f32(&audio);
        assert!(result.is_empty());
    }
}
```

- [ ] **Step 2: Temporarily add module to lib.rs to run tests**

Add `pub mod audio_prep;` to `crates/stt-providers/src/lib.rs` (keep existing modules for now — they'll be removed later).

- [ ] **Step 3: Run tests**

Run: `cargo test -p medical-stt-providers audio_prep -- --nocapture`

Expected: All 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/stt-providers/src/audio_prep.rs crates/stt-providers/src/lib.rs
git commit -m "feat(stt): add audio_prep module for 16kHz mono resampling"
```

---

### Task 3: Create Model Management Module

**Files:**
- Create: `crates/stt-providers/src/models.rs`

This module handles model metadata, path resolution, download with progress callbacks, and deletion. It manages both whisper and pyannote models.

- [ ] **Step 1: Create models.rs**

```rust
//! Model management: metadata, paths, download, validation.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::SttError;

// ── Model metadata ──────────────────────────────────────────────────────────

/// Identifies a whisper model variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhisperModelId {
    Base,
    Small,
    Medium,
    LargeV3Turbo,
}

impl WhisperModelId {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::LargeV3Turbo => "large-v3-turbo",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "base" => Some(Self::Base),
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large-v3-turbo" => Some(Self::LargeV3Turbo),
            _ => None,
        }
    }
}

/// Information about a downloadable model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub filename: String,
    pub size_bytes: u64,
    pub download_url: String,
    pub description: String,
    pub downloaded: bool,
}

/// Returns metadata for all available whisper models.
pub fn available_whisper_models(app_data_dir: &Path) -> Vec<ModelInfo> {
    let models = vec![
        ("base", "ggml-base.bin", 147_951_465, "Fast, basic accuracy"),
        ("small", "ggml-small.bin", 487_601_905, "Good balance of speed and accuracy"),
        ("medium", "ggml-medium.bin", 1_533_774_081, "High accuracy"),
        ("large-v3-turbo", "ggml-large-v3-turbo.bin", 1_622_081_537, "Best accuracy, recommended for medical use"),
    ];

    models
        .into_iter()
        .map(|(id, filename, size, desc)| {
            let path = whisper_model_path(app_data_dir, filename);
            ModelInfo {
                id: id.to_string(),
                filename: filename.to_string(),
                size_bytes: size,
                download_url: format!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{filename}"
                ),
                description: desc.to_string(),
                downloaded: path.exists(),
            }
        })
        .collect()
}

/// Returns metadata for required pyannote models.
pub fn pyannote_models(app_data_dir: &Path) -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "segmentation".to_string(),
            filename: "segmentation-3.0.onnx".to_string(),
            size_bytes: 17_000_000, // ~17 MB
            download_url: "https://huggingface.co/pyannote/segmentation-3.0/resolve/main/onnx/model.onnx".to_string(),
            description: "Speech boundary detection".to_string(),
            downloaded: pyannote_model_path(app_data_dir, "segmentation-3.0.onnx").exists(),
        },
        ModelInfo {
            id: "embedding".to_string(),
            filename: "wespeaker_en_voxceleb_CAM++.onnx".to_string(),
            size_bytes: 28_000_000, // ~28 MB
            download_url: "https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM/resolve/main/onnx/model.onnx".to_string(),
            description: "Speaker embeddings".to_string(),
            downloaded: pyannote_model_path(app_data_dir, "wespeaker_en_voxceleb_CAM++.onnx").exists(),
        },
    ]
}

// ── Path resolution ─────────────────────────────────────────────────────────

pub fn models_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models")
}

pub fn whisper_dir(app_data_dir: &Path) -> PathBuf {
    models_dir(app_data_dir).join("whisper")
}

pub fn pyannote_dir(app_data_dir: &Path) -> PathBuf {
    models_dir(app_data_dir).join("pyannote")
}

pub fn whisper_model_path(app_data_dir: &Path, filename: &str) -> PathBuf {
    whisper_dir(app_data_dir).join(filename)
}

pub fn pyannote_model_path(app_data_dir: &Path, filename: &str) -> PathBuf {
    pyannote_dir(app_data_dir).join(filename)
}

/// Returns the filename for a whisper model ID.
pub fn whisper_model_filename(model_id: &str) -> Option<&'static str> {
    match model_id {
        "base" => Some("ggml-base.bin"),
        "small" => Some("ggml-small.bin"),
        "medium" => Some("ggml-medium.bin"),
        "large-v3-turbo" => Some("ggml-large-v3-turbo.bin"),
        _ => None,
    }
}

// ── Download ────────────────────────────────────────────────────────────────

/// Download a model file from `url` to `dest_path`.
///
/// The `on_progress` callback receives `(bytes_downloaded, total_bytes)`.
/// `total_bytes` is 0 if the server doesn't provide Content-Length.
///
/// Creates parent directories as needed. Downloads to a `.tmp` file and
/// renames on completion to avoid partial files.
pub async fn download_model<F>(
    url: &str,
    dest_path: &Path,
    on_progress: F,
) -> Result<(), SttError>
where
    F: Fn(u64, u64) + Send + 'static,
{
    use tokio::io::AsyncWriteExt;

    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| SttError::ModelDownload(format!("Failed to create directory: {e}")))?;
    }

    let tmp_path = dest_path.with_extension("tmp");

    info!(url = %url, dest = %dest_path.display(), "Starting model download");

    let response = reqwest::get(url)
        .await
        .map_err(|e| SttError::ModelDownload(format!("HTTP request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(SttError::ModelDownload(format!(
            "Download failed with status {}. The model URL may require authentication — \
             see https://huggingface.co for model access.",
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| SttError::ModelDownload(format!("Failed to create file: {e}")))?;

    let mut stream = response.bytes_stream();
    use futures_core::Stream;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    // Use futures_util if available, otherwise manual poll
    while let Some(chunk) = {
        use tokio_stream::StreamExt;
        tokio_stream::StreamExt::next(&mut stream).await
    } {
        let chunk = chunk.map_err(|e| SttError::ModelDownload(format!("Download stream error: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| SttError::ModelDownload(format!("Write error: {e}")))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    file.flush()
        .await
        .map_err(|e| SttError::ModelDownload(format!("Flush error: {e}")))?;
    drop(file);

    // Atomic rename
    tokio::fs::rename(&tmp_path, dest_path)
        .await
        .map_err(|e| SttError::ModelDownload(format!("Rename error: {e}")))?;

    info!(dest = %dest_path.display(), bytes = downloaded, "Model download complete");
    Ok(())
}

/// Delete a model file from disk.
pub async fn delete_model(path: &Path) -> Result<(), SttError> {
    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| SttError::ModelDownload(format!("Failed to delete model: {e}")))?;
        info!(path = %path.display(), "Model deleted");
    }
    Ok(())
}

/// Check that all required models exist for transcription.
///
/// Returns a list of missing model descriptions (empty = all present).
pub fn check_required_models(
    app_data_dir: &Path,
    whisper_model_id: &str,
) -> Vec<String> {
    let mut missing = Vec::new();

    let whisper_filename = whisper_model_filename(whisper_model_id)
        .unwrap_or("ggml-large-v3-turbo.bin");
    if !whisper_model_path(app_data_dir, whisper_filename).exists() {
        missing.push(format!("Whisper model '{whisper_model_id}' not downloaded"));
    }

    if !pyannote_model_path(app_data_dir, "segmentation-3.0.onnx").exists() {
        missing.push("Pyannote segmentation model not downloaded".to_string());
    }

    if !pyannote_model_path(app_data_dir, "wespeaker_en_voxceleb_CAM++.onnx").exists() {
        missing.push("Pyannote speaker embedding model not downloaded".to_string());
    }

    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn whisper_model_filenames() {
        assert_eq!(whisper_model_filename("base"), Some("ggml-base.bin"));
        assert_eq!(whisper_model_filename("large-v3-turbo"), Some("ggml-large-v3-turbo.bin"));
        assert_eq!(whisper_model_filename("unknown"), None);
    }

    #[test]
    fn path_resolution() {
        let data = Path::new("/tmp/test");
        assert_eq!(whisper_model_path(data, "ggml-base.bin"), PathBuf::from("/tmp/test/models/whisper/ggml-base.bin"));
        assert_eq!(pyannote_model_path(data, "seg.onnx"), PathBuf::from("/tmp/test/models/pyannote/seg.onnx"));
    }

    #[test]
    fn available_models_list() {
        let data = Path::new("/nonexistent");
        let models = available_whisper_models(data);
        assert_eq!(models.len(), 4);
        assert_eq!(models[0].id, "base");
        assert_eq!(models[3].id, "large-v3-turbo");
        // None should be marked as downloaded since path doesn't exist
        assert!(models.iter().all(|m| !m.downloaded));
    }

    #[test]
    fn check_missing_models() {
        let data = Path::new("/nonexistent");
        let missing = check_required_models(data, "large-v3-turbo");
        assert_eq!(missing.len(), 3); // whisper + 2 pyannote models
    }

    #[test]
    fn whisper_model_id_roundtrip() {
        for id in ["base", "small", "medium", "large-v3-turbo"] {
            let parsed = WhisperModelId::from_str(id).unwrap();
            assert_eq!(parsed.as_str(), id);
        }
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod models;` to `crates/stt-providers/src/lib.rs`.

- [ ] **Step 3: Add streaming dependencies to lib.rs**

The download function uses `bytes_stream()` from reqwest which returns a `futures_core::Stream`. We need `tokio-stream` to iterate it. Add to `crates/stt-providers/Cargo.toml`:

```toml
tokio-stream = { version = "0.1" }
```

Also add to the imports in models.rs — the download function uses `tokio_stream::StreamExt`. Adjust the download function to compile cleanly. The exact stream iteration pattern may need adjustment depending on the reqwest version — the implementer should ensure the `bytes_stream()` → `StreamExt::next()` pattern works with the workspace's reqwest version.

- [ ] **Step 4: Update SttError enum**

In `crates/stt-providers/src/lib.rs`, add a new variant to `SttError`:

```rust
#[derive(Error, Debug)]
pub enum SttError {
    #[error("Transcription failed: {0}")]
    Transcription(String),
    #[error("Provider unavailable: {0}")]
    Unavailable(String),
    #[error("All providers exhausted")]
    AllProvidersExhausted,
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Audio format error: {0}")]
    AudioFormat(String),
    #[error("Model download error: {0}")]
    ModelDownload(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p medical-stt-providers models -- --nocapture`

Expected: All 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/stt-providers/src/models.rs crates/stt-providers/src/lib.rs crates/stt-providers/Cargo.toml
git commit -m "feat(stt): add model management module

Model metadata, path resolution, async download with progress callbacks,
and validation for whisper and pyannote ONNX models."
```

---

### Task 4: Create Whisper Transcription Wrapper

**Files:**
- Create: `crates/stt-providers/src/whisper.rs`

Extracted from `whisper_local.rs` — runs whisper.cpp inference via whisper-rs on a blocking thread and returns timestamped segments.

- [ ] **Step 1: Create whisper.rs**

```rust
//! Whisper transcription via whisper-rs.

use std::path::PathBuf;

use tracing::{info, instrument};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use medical_core::error::{AppError, AppResult};

/// A timestamped segment from whisper transcription.
#[derive(Debug, Clone)]
pub struct WhisperSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Wrapper around whisper-rs for local transcription.
pub struct WhisperTranscriber {
    model_path: PathBuf,
}

impl WhisperTranscriber {
    pub fn new(model_path: PathBuf) -> Self {
        Self { model_path }
    }

    /// Transcribe 16 kHz mono f32 audio.
    ///
    /// Must be called on a blocking thread (or via `spawn_blocking`).
    /// Returns a list of timestamped text segments.
    #[instrument(skip(self, audio_16k_mono), fields(provider = "whisper_local"))]
    pub fn transcribe(
        &self,
        audio_16k_mono: &[f32],
        language: Option<&str>,
    ) -> AppResult<Vec<WhisperSegment>> {
        let ctx = WhisperContext::new_with_params(
            self.model_path.to_str().ok_or_else(|| {
                AppError::SttProvider("Model path is not valid UTF-8".into())
            })?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| AppError::SttProvider(format!("Failed to load Whisper model: {e}")))?;

        let mut state = ctx.create_state().map_err(|e| {
            AppError::SttProvider(format!("Failed to create Whisper state: {e}"))
        })?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Language hint (ISO-639-1 two-letter code).
        let lang_code: Option<String> = language.map(|l| l.chars().take(2).collect());
        params.set_language(lang_code.as_deref());

        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_translate(false);
        params.set_no_timestamps(false);

        info!(
            samples = audio_16k_mono.len(),
            duration_s = audio_16k_mono.len() as f64 / 16_000.0,
            "Running local Whisper inference"
        );

        state.full(params, audio_16k_mono).map_err(|e| {
            AppError::SttProvider(format!("Whisper inference failed: {e}"))
        })?;

        let num_segments = state.full_n_segments();
        let mut segments = Vec::with_capacity(num_segments as usize);

        for i in 0..num_segments {
            let segment = state.get_segment(i).ok_or_else(|| {
                AppError::SttProvider(format!("Segment {i} out of bounds"))
            })?;

            let text = segment.to_str_lossy().map_err(|e| {
                AppError::SttProvider(format!("Failed to get segment {i} text: {e}"))
            })?;

            // whisper.cpp timestamps are in centiseconds.
            let start = segment.start_timestamp() as f64 / 100.0;
            let end = segment.end_timestamp() as f64 / 100.0;

            let text_trimmed = text.trim().to_owned();
            if !text_trimmed.is_empty() {
                segments.push(WhisperSegment {
                    text: text_trimmed,
                    start,
                    end,
                });
            }
        }

        info!(segments = segments.len(), "Whisper transcription complete");
        Ok(segments)
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod whisper;` to `crates/stt-providers/src/lib.rs`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p medical-stt-providers 2>&1 | tail -10`

Expected: Should compile (may have warnings about unused modules from old providers). If whisper-rs API has changed from what's shown, adjust the segment extraction to match the current API.

- [ ] **Step 4: Commit**

```bash
git add crates/stt-providers/src/whisper.rs crates/stt-providers/src/lib.rs
git commit -m "feat(stt): add whisper transcription wrapper

Wraps whisper-rs inference with Metal GPU acceleration. Returns
timestamped segments from 16kHz mono audio."
```

---

### Task 5: Create Speaker Diarization Module

**Files:**
- Create: `crates/stt-providers/src/diarization.rs`

Wraps pyannote-rs to run speaker diarization: segmentation → embedding → speaker assignment. Returns speaker turns with timestamps.

- [ ] **Step 1: Create diarization.rs**

```rust
//! Speaker diarization via pyannote-rs.
//!
//! Runs a two-stage pipeline:
//! 1. Segmentation model → detects speech boundaries
//! 2. Embedding model → extracts speaker vectors per segment
//! 3. EmbeddingManager → assigns speaker IDs by cosine similarity

use std::path::PathBuf;

use tracing::{info, warn};

use medical_core::error::{AppError, AppResult};

/// A speaker turn: a contiguous time range attributed to one speaker.
#[derive(Debug, Clone)]
pub struct SpeakerTurn {
    pub speaker_id: usize,
    pub start: f64,
    pub end: f64,
}

/// Speaker diarization using pyannote-rs ONNX models.
pub struct SpeakerDiarizer {
    segmentation_path: PathBuf,
    embedding_path: PathBuf,
}

impl SpeakerDiarizer {
    pub fn new(segmentation_path: PathBuf, embedding_path: PathBuf) -> Self {
        Self {
            segmentation_path,
            embedding_path,
        }
    }

    /// Run speaker diarization on 16 kHz mono audio.
    ///
    /// Takes i16 samples (as required by pyannote-rs).
    /// Must be called on a blocking thread.
    ///
    /// Returns speaker turns. On failure, returns an empty vec (graceful
    /// degradation — the caller proceeds without speaker labels).
    pub fn diarize(
        &self,
        samples_i16: &[i16],
        sample_rate: u32,
    ) -> AppResult<Vec<SpeakerTurn>> {
        info!(
            samples = samples_i16.len(),
            sample_rate,
            seg_model = %self.segmentation_path.display(),
            emb_model = %self.embedding_path.display(),
            "Running pyannote speaker diarization"
        );

        // Step 1: Segment audio into speech regions
        let segments: Vec<pyannote_rs::Segment> =
            match pyannote_rs::get_segments(samples_i16, sample_rate, &self.segmentation_path) {
                Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
                Err(e) => {
                    warn!(error = %e, "Segmentation failed — returning empty diarization");
                    return Ok(Vec::new());
                }
            };

        if segments.is_empty() {
            info!("No speech segments detected");
            return Ok(Vec::new());
        }

        info!(segments = segments.len(), "Speech segments detected");

        // Step 2: Extract embeddings and assign speakers
        let mut extractor = match pyannote_rs::EmbeddingExtractor::new(&self.embedding_path) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to load embedding model — returning empty diarization");
                return Ok(Vec::new());
            }
        };

        // Max 6 speakers (medical encounters rarely have more than 2-3)
        let mut manager = pyannote_rs::EmbeddingManager::new(6);
        let threshold = 0.5_f32; // Cosine similarity threshold for speaker matching

        let mut turns = Vec::with_capacity(segments.len());

        for seg in &segments {
            let embedding: Vec<f32> = match extractor.compute(&seg.samples) {
                Ok(iter) => iter.collect(),
                Err(e) => {
                    warn!(
                        start = seg.start,
                        end = seg.end,
                        error = %e,
                        "Failed to compute embedding for segment — skipping"
                    );
                    continue;
                }
            };

            let speaker_id = manager
                .search_speaker(embedding, threshold)
                .unwrap_or(0);

            turns.push(SpeakerTurn {
                speaker_id,
                start: seg.start,
                end: seg.end,
            });
        }

        info!(
            turns = turns.len(),
            speakers = manager.get_all_speakers().len(),
            "Diarization complete"
        );

        Ok(turns)
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod diarization;` to `crates/stt-providers/src/lib.rs`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p medical-stt-providers 2>&1 | tail -10`

Expected: Should compile. If pyannote-rs API differs (e.g., `get_segments` returns `Vec` instead of `impl Iterator`, or `EmbeddingExtractor::compute` has a different signature), adjust accordingly. The API was documented from v0.3.4 — check the actual crate's public API with `cargo doc -p pyannote-rs --open` if there are issues.

**Troubleshooting:**
- If `pyannote_rs::get_segments` returns `Result<impl Iterator<Item = Result<Segment>>>`, unwrap both layers as shown.
- If `pyannote_rs::get_segments` returns `Result<Vec<Segment>>` instead, adjust: `let segments = pyannote_rs::get_segments(...)?.into_iter().collect();`
- If `EmbeddingExtractor::compute` takes `&mut self`, add `mut` to the binding.
- If ort initialization fails at runtime, ensure ONNX Runtime is available (ort may need the `download` feature to auto-fetch it during build).

- [ ] **Step 4: Commit**

```bash
git add crates/stt-providers/src/diarization.rs crates/stt-providers/src/lib.rs
git commit -m "feat(stt): add speaker diarization module via pyannote-rs

ONNX-based segmentation + embedding pipeline. Assigns speaker IDs by
cosine similarity. Fails gracefully with empty results on error."
```

---

### Task 6: Create Segment Merge Module

**Files:**
- Create: `crates/stt-providers/src/merge.rs`

Aligns whisper transcription segments with speaker diarization turns by maximum timestamp overlap.

- [ ] **Step 1: Create merge.rs with tests**

```rust
//! Merge whisper segments with speaker turns by timestamp overlap.

use medical_core::types::TranscriptSegment;

use crate::diarization::SpeakerTurn;
use crate::whisper::WhisperSegment;

/// Merge whisper text segments with speaker turns.
///
/// For each whisper segment, finds the speaker turn with the greatest
/// timestamp overlap and assigns that speaker's ID as "Speaker N".
///
/// If `speaker_turns` is empty, returns segments without speaker labels.
pub fn merge_segments_with_speakers(
    whisper_segments: &[WhisperSegment],
    speaker_turns: &[SpeakerTurn],
) -> Vec<TranscriptSegment> {
    whisper_segments
        .iter()
        .map(|ws| {
            let speaker = if speaker_turns.is_empty() {
                None
            } else {
                best_speaker_for_range(ws.start, ws.end, speaker_turns)
            };

            TranscriptSegment {
                text: ws.text.clone(),
                start: ws.start,
                end: ws.end,
                speaker,
                confidence: None,
            }
        })
        .collect()
}

/// Find the speaker with the most overlap for the given time range.
fn best_speaker_for_range(
    start: f64,
    end: f64,
    turns: &[SpeakerTurn],
) -> Option<String> {
    let mut best_id: Option<usize> = None;
    let mut best_overlap: f64 = 0.0;

    for turn in turns {
        let overlap_start = start.max(turn.start);
        let overlap_end = end.min(turn.end);
        let overlap = (overlap_end - overlap_start).max(0.0);

        if overlap > best_overlap {
            best_overlap = overlap;
            best_id = Some(turn.speaker_id);
        }
    }

    // Only assign speaker if there's meaningful overlap (> 10ms)
    if best_overlap > 0.01 {
        best_id.map(|id| format!("Speaker {}", id + 1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(text: &str, start: f64, end: f64) -> WhisperSegment {
        WhisperSegment {
            text: text.to_string(),
            start,
            end,
        }
    }

    fn st(speaker_id: usize, start: f64, end: f64) -> SpeakerTurn {
        SpeakerTurn {
            speaker_id,
            start,
            end,
        }
    }

    #[test]
    fn no_speaker_turns_returns_none_labels() {
        let segments = vec![ws("Hello", 0.0, 1.0)];
        let result = merge_segments_with_speakers(&segments, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Hello");
        assert!(result[0].speaker.is_none());
    }

    #[test]
    fn single_speaker_assigns_label() {
        let segments = vec![ws("Hello", 0.0, 1.0), ws("World", 1.0, 2.0)];
        let turns = vec![st(0, 0.0, 2.0)];
        let result = merge_segments_with_speakers(&segments, &turns);
        assert_eq!(result[0].speaker.as_deref(), Some("Speaker 1"));
        assert_eq!(result[1].speaker.as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn two_speakers_assigned_correctly() {
        let segments = vec![
            ws("How are you?", 0.0, 2.0),
            ws("Not great.", 2.5, 4.0),
            ws("Tell me more.", 4.5, 6.0),
        ];
        let turns = vec![
            st(0, 0.0, 2.0),  // Speaker 1 speaks first
            st(1, 2.5, 4.0),  // Speaker 2 responds
            st(0, 4.5, 6.0),  // Speaker 1 again
        ];
        let result = merge_segments_with_speakers(&segments, &turns);
        assert_eq!(result[0].speaker.as_deref(), Some("Speaker 1"));
        assert_eq!(result[1].speaker.as_deref(), Some("Speaker 2"));
        assert_eq!(result[2].speaker.as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn partial_overlap_picks_best_match() {
        // Whisper segment spans two speaker turns — picks the one with more overlap
        let segments = vec![ws("Overlapping", 1.0, 3.0)];
        let turns = vec![
            st(0, 0.0, 1.5),  // 0.5s overlap
            st(1, 1.5, 4.0),  // 1.5s overlap — should win
        ];
        let result = merge_segments_with_speakers(&segments, &turns);
        assert_eq!(result[0].speaker.as_deref(), Some("Speaker 2"));
    }

    #[test]
    fn no_overlap_returns_none() {
        let segments = vec![ws("Gap", 5.0, 6.0)];
        let turns = vec![st(0, 0.0, 1.0)]; // No overlap with segment
        let result = merge_segments_with_speakers(&segments, &turns);
        assert!(result[0].speaker.is_none());
    }

    #[test]
    fn timestamps_preserved() {
        let segments = vec![ws("Test", 1.5, 3.7)];
        let turns = vec![st(0, 0.0, 5.0)];
        let result = merge_segments_with_speakers(&segments, &turns);
        assert!((result[0].start - 1.5).abs() < 1e-6);
        assert!((result[0].end - 3.7).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod merge;` to `crates/stt-providers/src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p medical-stt-providers merge -- --nocapture`

Expected: All 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/stt-providers/src/merge.rs crates/stt-providers/src/lib.rs
git commit -m "feat(stt): add segment merge module

Aligns whisper transcription segments with speaker diarization turns
by maximum timestamp overlap."
```

---

### Task 7: Create LocalSttProvider and Clean Up lib.rs

**Files:**
- Create: `crates/stt-providers/src/local_provider.rs`
- Modify: `crates/stt-providers/src/lib.rs`
- Delete: `crates/stt-providers/src/deepgram.rs`
- Delete: `crates/stt-providers/src/elevenlabs_stt.rs`
- Delete: `crates/stt-providers/src/groq_whisper.rs`
- Delete: `crates/stt-providers/src/modulate.rs`
- Delete: `crates/stt-providers/src/failover.rs`
- Delete: `crates/stt-providers/src/whisper_local.rs`

- [ ] **Step 1: Create local_provider.rs**

```rust
//! LocalSttProvider — the single SttProvider implementation for local inference.
//!
//! Orchestrates the two-stage pipeline:
//! 1. Whisper transcription (whisper-rs, Metal GPU)
//! 2. Pyannote speaker diarization (pyannote-rs, ONNX)
//! 3. Merge segments with speaker labels

use std::path::PathBuf;

use async_trait::async_trait;
use futures_core::Stream;
use tracing::{info, warn};

use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::{
    AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk, TranscriptSegment,
};

use crate::audio_prep;
use crate::diarization::SpeakerDiarizer;
use crate::merge::merge_segments_with_speakers;
use crate::whisper::WhisperTranscriber;

/// Local speech-to-text provider using whisper-rs + pyannote-rs.
///
/// Created with paths to model files. If diarization model files are not
/// present, diarization is skipped and transcripts are returned without
/// speaker labels.
pub struct LocalSttProvider {
    whisper_model_path: PathBuf,
    segmentation_model_path: PathBuf,
    embedding_model_path: PathBuf,
}

impl LocalSttProvider {
    pub fn new(
        whisper_model_path: PathBuf,
        segmentation_model_path: PathBuf,
        embedding_model_path: PathBuf,
    ) -> Self {
        Self {
            whisper_model_path,
            segmentation_model_path,
            embedding_model_path,
        }
    }
}

#[async_trait]
impl SttProvider for LocalSttProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        // Supported when pyannote models are present
        self.segmentation_model_path.exists() && self.embedding_model_path.exists()
    }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        if !self.whisper_model_path.exists() {
            return Err(AppError::SttProvider(format!(
                "Whisper model not found at {}. Download a model in Settings → Audio / STT.",
                self.whisper_model_path.display()
            )));
        }

        let duration = audio.duration_seconds();

        // ── Stage 1: Resample ───────────────────────────────────────────
        let audio_16k = audio_prep::to_16k_mono_f32(&audio);

        // ── Stage 2: Whisper transcription ──────────────────────────────
        let whisper_path = self.whisper_model_path.clone();
        let language = config.language.clone();
        let audio_for_whisper = audio_16k.clone();

        let whisper_segments = tokio::task::spawn_blocking(move || {
            let transcriber = WhisperTranscriber::new(whisper_path);
            transcriber.transcribe(&audio_for_whisper, language.as_deref())
        })
        .await
        .map_err(|e| AppError::SttProvider(format!("Whisper task panicked: {e}")))?
        ?;

        // ── Stage 3: Speaker diarization (optional) ─────────────────────
        let speaker_turns = if config.diarize && self.supports_diarization() {
            let seg_path = self.segmentation_model_path.clone();
            let emb_path = self.embedding_model_path.clone();
            let audio_i16 = audio_prep::f32_to_i16(&audio_16k);

            match tokio::task::spawn_blocking(move || {
                let diarizer = SpeakerDiarizer::new(seg_path, emb_path);
                diarizer.diarize(&audio_i16, 16000)
            })
            .await
            {
                Ok(Ok(turns)) => turns,
                Ok(Err(e)) => {
                    warn!(error = %e, "Diarization failed — proceeding without speaker labels");
                    Vec::new()
                }
                Err(e) => {
                    warn!(error = %e, "Diarization task panicked — proceeding without speaker labels");
                    Vec::new()
                }
            }
        } else {
            if config.diarize && !self.supports_diarization() {
                warn!("Diarization requested but pyannote models not found — skipping");
            }
            Vec::new()
        };

        // ── Stage 4: Merge ──────────────────────────────────────────────
        let segments: Vec<TranscriptSegment> =
            merge_segments_with_speakers(&whisper_segments, &speaker_turns);

        let full_text: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        info!(
            segments = segments.len(),
            speakers = speaker_turns.iter().map(|t| t.speaker_id).collect::<std::collections::HashSet<_>>().len(),
            text_len = full_text.len(),
            "Local transcription complete"
        );

        Ok(Transcript {
            text: full_text,
            segments,
            language: config.language.clone(),
            duration_seconds: Some(duration),
            provider: "local".to_owned(),
            metadata: serde_json::json!({
                "whisper_model": self.whisper_model_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                "diarization": !speaker_turns.is_empty(),
            }),
        })
    }

    async fn transcribe_stream(
        &self,
        _stream: AudioStream,
        _config: SttConfig,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>> {
        Err(AppError::SttProvider(
            "Local provider does not support streaming transcription".to_owned(),
        ))
    }
}
```

- [ ] **Step 2: Delete old provider files**

```bash
cd /Users/cortexuvula/Development/rustMedicalAssistant
rm crates/stt-providers/src/deepgram.rs
rm crates/stt-providers/src/elevenlabs_stt.rs
rm crates/stt-providers/src/groq_whisper.rs
rm crates/stt-providers/src/modulate.rs
rm crates/stt-providers/src/failover.rs
rm crates/stt-providers/src/whisper_local.rs
```

- [ ] **Step 3: Replace lib.rs contents**

Replace `crates/stt-providers/src/lib.rs` entirely:

```rust
pub mod audio_prep;
pub mod models;
pub mod whisper;
pub mod diarization;
pub mod merge;
pub mod local_provider;

pub use local_provider::LocalSttProvider;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("Transcription failed: {0}")]
    Transcription(String),
    #[error("Provider unavailable: {0}")]
    Unavailable(String),
    #[error("Audio format error: {0}")]
    AudioFormat(String),
    #[error("Model download error: {0}")]
    ModelDownload(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

pub type SttResult<T> = Result<T, SttError>;
```

Note: `AllProvidersExhausted` and `Http` variants are removed (no failover chain, no HTTP API calls).

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p medical-stt-providers 2>&1 | tail -20`

Expected: The crate should compile. You may see errors in downstream crates (src-tauri) that still reference deleted types (`SttFailover`, `DeepgramProvider`, etc.) — those are fixed in Task 8.

- [ ] **Step 5: Commit**

```bash
git add -A crates/stt-providers/src/
git commit -m "feat(stt): add LocalSttProvider, remove cloud providers

Replace Deepgram, ElevenLabs, Groq, Modulate providers and failover
chain with a single LocalSttProvider using whisper-rs + pyannote-rs.
Diarization degrades gracefully when models are missing."
```

---

### Task 8: Update Backend Settings, State, and Transcription

**Files:**
- Modify: `crates/core/src/types/settings.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands/transcription.rs`
- Modify: `src-tauri/src/commands/providers.rs`

- [ ] **Step 1: Update settings.rs — remove STT provider fields, add whisper_model**

In `crates/core/src/types/settings.rs`:

Remove these default helper functions:
```rust
fn default_stt_provider() -> String {
    "groq".into()
}

fn default_stt_failover_chain() -> Vec<String> {
    vec!["groq".into(), "deepgram".into(), "whisper".into()]
}
```

Add this new default helper:
```rust
fn default_whisper_model() -> String {
    "large-v3-turbo".into()
}
```

In the `AppConfig` struct, replace these fields:
```rust
    #[serde(default = "default_stt_provider")]
    pub stt_provider: String,
    #[serde(default = "default_stt_failover_chain")]
    pub stt_failover_chain: Vec<String>,
```

With:
```rust
    #[serde(default = "default_whisper_model")]
    pub whisper_model: String,
```

- [ ] **Step 2: Update settings tests**

In the `default_config_values` test, replace:
```rust
        assert_eq!(config.stt_provider, "groq");
```
With:
```rust
        assert_eq!(config.whisper_model, "large-v3-turbo");
```

Remove the `stt_failover_chain_default` test entirely.

- [ ] **Step 3: Update state.rs**

Replace the entire contents of `src-tauri/src/state.rs`. The key changes:
- Remove all cloud STT provider imports
- Remove `SttFailover` import
- Change `stt_providers` field type from `Arc<Mutex<Option<Arc<SttFailover>>>>` to `Arc<Mutex<Option<Arc<dyn SttProvider + Send + Sync>>>>`
- Simplify `init_stt_providers` to create `LocalSttProvider` with model paths

Here's the updated file (showing only the changed sections — leave everything else identical):

**Imports** — remove these lines:
```rust
use medical_stt_providers::deepgram::DeepgramProvider;
use medical_stt_providers::elevenlabs_stt::ElevenLabsSttProvider;
use medical_stt_providers::failover::SttFailover;
use medical_stt_providers::groq_whisper::GroqWhisperProvider;
use medical_stt_providers::modulate::ModulateProvider;
```

Add this import:
```rust
use medical_stt_providers::LocalSttProvider;
use medical_stt_providers::models as stt_models;
```

**AppState struct** — change the `stt_providers` field:
```rust
    pub stt_providers: Arc<Mutex<Option<Arc<dyn SttProvider + Send + Sync>>>>,
```

**Replace `init_stt_providers` function entirely:**
```rust
/// Create the local STT provider with model paths from the app data directory.
///
/// Returns the provider directly — no API keys needed, no failover chain.
/// If model files don't exist yet, the provider is still created; it will
/// return a descriptive error on first transcribe() call.
pub fn init_stt_providers(data_dir: &Path, whisper_model_id: &str) -> Option<Arc<dyn SttProvider + Send + Sync>> {
    let whisper_filename = stt_models::whisper_model_filename(whisper_model_id)
        .unwrap_or("ggml-large-v3-turbo.bin");

    let whisper_path = stt_models::whisper_model_path(data_dir, whisper_filename);
    let seg_path = stt_models::pyannote_model_path(data_dir, "segmentation-3.0.onnx");
    let emb_path = stt_models::pyannote_model_path(data_dir, "wespeaker_en_voxceleb_CAM++.onnx");

    info!(
        whisper = %whisper_path.display(),
        segmentation = %seg_path.display(),
        embedding = %emb_path.display(),
        "Initializing local STT provider"
    );

    let provider = LocalSttProvider::new(whisper_path, seg_path, emb_path);
    Some(Arc::new(provider))
}
```

**In `AppState::initialize()`** — update the STT initialization:

Replace:
```rust
        let preferred_stt = config.as_ref()
            .map(|c| c.stt_provider.as_str())
            .unwrap_or("deepgram");
        let stt_providers = init_stt_providers(&keys, preferred_stt);
```

With:
```rust
        let whisper_model = config.as_ref()
            .map(|c| c.whisper_model.as_str())
            .unwrap_or("large-v3-turbo");
        let stt_providers = init_stt_providers(&data_dir, whisper_model);
```

And update the `stt_providers` field initialization:
```rust
            stt_providers: Arc::new(Mutex::new(stt_providers)),
```

(Remove the `.map(Arc::new)` since `init_stt_providers` already returns `Option<Arc<...>>`.)

- [ ] **Step 4: Update transcription.rs**

In `src-tauri/src/commands/transcription.rs`, update the STT provider usage.

Replace the STT provider lock section (lines ~146-167):
```rust
    // Clone the Arc<dyn SttProvider> so we release the mutex before the long-running transcribe await.
    let stt: Arc<dyn medical_core::traits::SttProvider + Send + Sync> = {
        let guard = state.stt_providers.lock().await;
        match guard.as_ref() {
            Some(stt) => stt.clone(),
            None => {
                tracing::error!("No STT provider configured — cannot transcribe");
                return Err(
                    "No STT provider configured. Download a Whisper model in Settings → Audio / STT.".to_string()
                );
            }
        }
    };
    let transcript = stt.transcribe(audio, config)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "STT transcription failed");
            format!("Transcription failed: {e}")
        })?;
```

Also remove the `list_stt_providers` command entirely (or replace it with a stub returning the local provider name). Replace:

```rust
#[tauri::command]
pub async fn list_stt_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, bool)>, String> {
    let guard = state.stt_providers.lock().await;
    match guard.as_deref() {
        Some(stt) => Ok(stt.provider_statuses()),
        None => Ok(vec![]),
    }
}
```

With:
```rust
#[tauri::command]
pub async fn list_stt_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, bool)>, String> {
    let guard = state.stt_providers.lock().await;
    match guard.as_ref() {
        Some(provider) => Ok(vec![(provider.name().to_string(), true)]),
        None => Ok(vec![]),
    }
}
```

- [ ] **Step 5: Update providers.rs**

In `src-tauri/src/commands/providers.rs`, remove the STT rebuild section.

Replace the entire function body:

```rust
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
```

Remove the old imports that reference `Arc` (if no longer needed) or STT-specific types.

- [ ] **Step 6: Verify the full project compiles**

Run: `cargo check 2>&1 | tail -20`

Expected: Should compile. Fix any remaining references to `SttFailover`, `DeepgramProvider`, etc. in the codebase. Common issues:
- `state.rs` may need `use std::path::Path;` for the new function signature
- `providers.rs` may need the `state::init_stt_providers` import path updated
- The `stt_providers` type change from `Arc<SttFailover>` to `Arc<dyn SttProvider + Send + Sync>` may require `as_deref()` calls to be changed to `as_ref()`

- [ ] **Step 7: Run existing tests**

Run: `cargo test 2>&1 | tail -30`

Expected: All tests pass. The settings tests now check `whisper_model` instead of `stt_provider`.

- [ ] **Step 8: Commit**

```bash
git add crates/core/src/types/settings.rs src-tauri/src/state.rs src-tauri/src/commands/transcription.rs src-tauri/src/commands/providers.rs
git commit -m "refactor(stt): wire LocalSttProvider into backend

Remove cloud STT imports and failover chain from state initialization.
Replace stt_provider/stt_failover_chain settings with whisper_model.
Transcription now uses LocalSttProvider directly."
```

---

### Task 9: Create Tauri Model Management Commands

**Files:**
- Create: `src-tauri/src/commands/models.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create commands/models.rs**

```rust
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
/// Supports both whisper model IDs ("base", "small", "medium", "large-v3-turbo")
/// and pyannote model IDs ("segmentation", "embedding").
///
/// Emits `model-download-progress` events with `{ model_id, downloaded_bytes, total_bytes }`.
#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let data_dir = state.data_dir.clone();

    // Find the model info
    let all_models: Vec<stt_models::ModelInfo> = stt_models::available_whisper_models(&data_dir)
        .into_iter()
        .chain(stt_models::pyannote_models(&data_dir))
        .collect();

    let model = all_models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Unknown model ID: {model_id}"))?
        .clone();

    // Determine destination path
    let dest_path = if stt_models::whisper_model_filename(&model_id).is_some() {
        stt_models::whisper_model_path(&data_dir, &model.filename)
    } else {
        stt_models::pyannote_model_path(&data_dir, &model.filename)
    };

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

    let all_models: Vec<stt_models::ModelInfo> = stt_models::available_whisper_models(&data_dir)
        .into_iter()
        .chain(stt_models::pyannote_models(&data_dir))
        .collect();

    let model = all_models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Unknown model ID: {model_id}"))?;

    let path = if stt_models::whisper_model_filename(&model_id).is_some() {
        stt_models::whisper_model_path(&data_dir, &model.filename)
    } else {
        stt_models::pyannote_model_path(&data_dir, &model.filename)
    };

    stt_models::delete_model(&path)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
```

- [ ] **Step 2: Add module to commands/mod.rs**

In `src-tauri/src/commands/mod.rs`, add:

```rust
pub mod models;
```

- [ ] **Step 3: Register commands in lib.rs**

In `src-tauri/src/lib.rs`, add to the `invoke_handler` list:

```rust
            commands::models::list_whisper_models,
            commands::models::download_model,
            commands::models::delete_model,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -10`

Expected: Full project compiles.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/models.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(stt): add Tauri model management commands

list_whisper_models, download_model, delete_model with progress events.
Reinitializes STT provider after model download."
```

---

### Task 10: Update Frontend Types, Stores, and Settings UI

**Files:**
- Create: `src/lib/api/models.ts`
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/stores/settings.ts`
- Modify: `src/lib/components/SettingsContent.svelte`

- [ ] **Step 1: Create api/models.ts**

```typescript
import { invoke } from '@tauri-apps/api/core';

export interface ModelInfo {
  id: string;
  filename: string;
  size_bytes: number;
  download_url: string;
  description: string;
  downloaded: boolean;
}

export async function listWhisperModels(): Promise<ModelInfo[]> {
  return invoke('list_whisper_models');
}

export async function downloadModel(modelId: string): Promise<void> {
  return invoke('download_model', { modelId });
}

export async function deleteModel(modelId: string): Promise<void> {
  return invoke('delete_model', { modelId });
}
```

- [ ] **Step 2: Update types/index.ts**

In `src/lib/types/index.ts`, update the `AppConfig` interface.

Remove:
```typescript
  stt_provider: string;
```

Add:
```typescript
  whisper_model: string;
```

- [ ] **Step 3: Update stores/settings.ts**

In `src/lib/stores/settings.ts`, update the defaults object.

Remove:
```typescript
  stt_provider: 'groq',
```

Add:
```typescript
  whisper_model: 'large-v3-turbo',
```

- [ ] **Step 4: Update SettingsContent.svelte — Audio / STT section**

Replace the Audio / STT section in `src/lib/components/SettingsContent.svelte`. This replaces the STT provider dropdown with a whisper model selector, download status indicators, and download/delete buttons.

**Add imports** at the top of the script:
```typescript
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onDestroy } from 'svelte';
  import { listWhisperModels, downloadModel, deleteModel, type ModelInfo } from '../api/models';
```

**Add state variables** after the existing state declarations:
```typescript
  let whisperModels = $state<ModelInfo[]>([]);
  let modelsRefreshing = $state(false);
  let downloadingModel = $state<string | null>(null);
  let downloadProgress = $state<Record<string, { downloaded: number; total: number }>>({});
  let progressUnlisten: UnlistenFn | null = null;
```

**Add model management functions:**
```typescript
  async function fetchWhisperModels() {
    modelsRefreshing = true;
    try {
      whisperModels = await listWhisperModels();
    } catch (e) {
      console.error('Failed to list whisper models:', e);
    } finally {
      modelsRefreshing = false;
    }
  }

  async function handleDownloadModel(modelId: string) {
    downloadingModel = modelId;
    try {
      await downloadModel(modelId);
      await fetchWhisperModels();
    } catch (e) {
      console.error(`Failed to download model ${modelId}:`, e);
    } finally {
      downloadingModel = null;
    }
  }

  async function handleDeleteModel(modelId: string) {
    try {
      await deleteModel(modelId);
      await fetchWhisperModels();
    } catch (e) {
      console.error(`Failed to delete model ${modelId}:`, e);
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(0)} MB`;
    return `${(bytes / 1073741824).toFixed(1)} GB`;
  }

  async function handleWhisperModelChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await settings.updateField('whisper_model', value);
  }
```

**Update onMount** to fetch whisper models and listen for download progress:
```typescript
  // Add to the onMount Promise.allSettled array:
  fetchWhisperModels(),
```

After the `onMount`, add a progress listener setup:
```typescript
  // Listen for model download progress events
  listen<{ model_id: string; downloaded_bytes: number; total_bytes: number }>(
    'model-download-progress',
    (event) => {
      downloadProgress = {
        ...downloadProgress,
        [event.payload.model_id]: {
          downloaded: event.payload.downloaded_bytes,
          total: event.payload.total_bytes,
        },
      };
    }
  ).then((unlisten) => {
    progressUnlisten = unlisten;
  });

  onDestroy(() => {
    progressUnlisten?.();
  });
```

**Remove** `handleSttProviderChange` function.

**Replace the Audio / STT section markup.** Replace everything inside `{:else if activeSection === 'audio'}`:

```svelte
    {:else if activeSection === 'audio'}
      <section class="settings-section">
        <h3 class="section-title">Audio / STT</h3>

        <div class="form-group">
          <label for="input-device" class="form-label">Input Device</label>
          <select
            id="input-device"
            value={$settings.input_device ?? ''}
            onchange={handleInputDeviceChange}
            disabled={devicesLoading}
          >
            {#if devicesLoading}
              <option value="">Loading devices...</option>
            {:else}
              <option value="">System Default</option>
              {#each audioDevices as device}
                <option value={device.name}>
                  {device.name}{device.is_default ? ' (Default)' : ''}
                </option>
              {/each}
            {/if}
          </select>
        </div>

        <div class="form-group">
          <label for="whisper-model" class="form-label">Whisper Model</label>
          <select
            id="whisper-model"
            value={$settings.whisper_model}
            onchange={handleWhisperModelChange}
          >
            {#each whisperModels as model}
              <option value={model.id}>
                {model.id} ({formatBytes(model.size_bytes)}) {model.downloaded ? '' : '- not downloaded'}
              </option>
            {/each}
          </select>
          <span class="form-hint">Larger models are more accurate but use more memory and take longer.</span>
        </div>

        <div class="form-group">
          <span class="form-label">Model Management</span>
          <div class="model-list">
            {#each whisperModels as model}
              <div class="model-row">
                <div class="model-info">
                  <span class="model-name">{model.id}</span>
                  <span class="model-desc">{model.description}</span>
                  <span class="model-size">{formatBytes(model.size_bytes)}</span>
                </div>
                <div class="model-actions">
                  {#if model.downloaded}
                    <span class="badge-downloaded">Downloaded</span>
                    <button
                      class="btn-delete-model"
                      onclick={() => handleDeleteModel(model.id)}
                      disabled={model.id === $settings.whisper_model}
                      title={model.id === $settings.whisper_model ? 'Cannot delete the active model' : 'Delete to free disk space'}
                    >
                      Delete
                    </button>
                  {:else if downloadingModel === model.id}
                    <span class="download-progress">
                      {#if downloadProgress[model.id]}
                        {Math.round((downloadProgress[model.id].downloaded / (downloadProgress[model.id].total || 1)) * 100)}%
                      {:else}
                        Starting...
                      {/if}
                    </span>
                  {:else}
                    <button
                      class="btn-download-model"
                      onclick={() => handleDownloadModel(model.id)}
                      disabled={downloadingModel !== null}
                    >
                      Download
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>

        <div class="form-group">
          <label for="sample-rate" class="form-label">Sample Rate</label>
          <select
            id="sample-rate"
            value={$settings.sample_rate}
            onchange={handleSampleRateChange}
          >
            <option value={16000}>16000 Hz</option>
            <option value={44100}>44100 Hz</option>
            <option value={48000}>48000 Hz</option>
          </select>
        </div>

        <div class="form-group">
          <label class="form-label checkbox-label">
            <input
              type="checkbox"
              checked={$settings.auto_generate_soap}
              onchange={(e: Event) => {
                const checked = (e.target as HTMLInputElement).checked;
                settings.updateField('auto_generate_soap', checked);
              }}
            />
            <span>Auto-generate SOAP after recording</span>
          </label>
          <span class="form-hint">When enabled, transcription and SOAP generation start automatically after you stop recording.</span>
        </div>
      </section>
```

**Remove** the STT-related entries from `API_PROVIDERS` array. Remove these objects:
```typescript
    { id: 'deepgram', label: 'Deepgram' },
    { id: 'elevenlabs', label: 'ElevenLabs' },
    { id: 'modulate', label: 'Modulate' },
```

These were only needed for STT API key management. The remaining API keys (openai, anthropic, gemini, groq, cerebras) are for AI providers, not STT.

**Add styles** to the `<style>` block:

```css
  .model-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    gap: 12px;
  }

  .model-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .model-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .model-desc {
    font-size: 11px;
    color: var(--text-muted);
  }

  .model-size {
    font-size: 11px;
    color: var(--text-muted);
  }

  .model-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .badge-downloaded {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--success);
    background-color: color-mix(in srgb, var(--success) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
  }

  .download-progress {
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
  }

  .btn-download-model {
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 500;
    background-color: var(--accent);
    color: var(--text-inverse);
    border-radius: var(--radius-sm);
    transition: background-color 0.15s ease;
  }

  .btn-download-model:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-download-model:disabled {
    opacity: 0.5;
  }

  .btn-delete-model {
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 500;
    color: var(--danger, #ef4444);
    background-color: transparent;
    border: 1px solid var(--danger, #ef4444);
    border-radius: var(--radius-sm);
    transition: background-color 0.15s ease;
  }

  .btn-delete-model:hover:not(:disabled) {
    background-color: rgba(239, 68, 68, 0.1);
  }

  .btn-delete-model:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
```

- [ ] **Step 5: Verify frontend builds**

Run: `cd /Users/cortexuvula/Development/rustMedicalAssistant && npm run check 2>&1 | tail -20`

Expected: TypeScript type checking passes. If there are errors about the `stt_provider` property being missing from `AppConfig`, check all frontend files that reference it and update to `whisper_model`.

- [ ] **Step 6: Start dev server and verify Settings UI**

Run: `npm run tauri dev`

Manual verification:
1. Open Settings → Audio / STT section
2. Verify the STT provider dropdown is gone
3. Verify the Whisper model dropdown shows (base, small, medium, large-v3-turbo)
4. Verify model management section shows download/delete buttons
5. Verify the auto-generate SOAP checkbox still works
6. Verify the API Keys section no longer shows Deepgram, ElevenLabs, Modulate

- [ ] **Step 7: Commit**

```bash
git add src/lib/api/models.ts src/lib/types/index.ts src/lib/stores/settings.ts src/lib/components/SettingsContent.svelte
git commit -m "feat(ui): replace STT provider settings with model management UI

Whisper model selector, per-model download status, download/delete
buttons with progress. Remove cloud STT provider dropdown and API key
entries for Deepgram, ElevenLabs, Modulate."
```

---

## Post-Implementation Notes

### Testing the full pipeline

After all tasks are complete:
1. Download the `base` whisper model (smallest, fastest download for testing)
2. Record or import a short audio file
3. Verify transcription completes successfully
4. If pyannote models are available, verify speaker labels appear
5. If pyannote models are not available, verify transcription works without speaker labels

### Pyannote model availability

The pyannote ONNX models may require accepting license terms on HuggingFace. If automatic download fails with a 401/403 error, users can:
1. Visit the model pages on HuggingFace and accept the license
2. Download the ONNX files manually
3. Place them in `{app_data_dir}/models/pyannote/`

The download error message in `models.rs` includes guidance about HuggingFace authentication.

### Future improvements

- **Higher quality diarization:** Replace pyannote-rs with `speakrs` (v0.4) for production-grade PLDA+VBx clustering (~7% DER vs ~80%). Requires BLAS backend (openblas or Accelerate). The `diarization.rs` module wraps diarization behind a clean interface making this swap straightforward.
- **Language selector:** The whisper `language` parameter is passed through from `SttConfig.language`. Add a language dropdown to the Settings UI.
- **Model download on first transcription:** Currently, if models aren't downloaded, `LocalSttProvider::transcribe` returns an error. Could trigger auto-download with progress events instead.
