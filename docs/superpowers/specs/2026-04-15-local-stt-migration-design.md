# Local STT Migration

**Date:** 2026-04-15
**Status:** Approved

## Problem

FerriScribe currently relies on cloud STT providers (Deepgram, ElevenLabs, Groq, Modulate) for speech-to-text transcription with speaker diarization. This requires API keys, internet connectivity, and sends sensitive medical audio to third-party servers. The goal is to replace all cloud STT with a fully local pipeline using whisper.cpp (via whisper-rs) for transcription and pyannote (via pyannote-rs ONNX bindings) for speaker diarization.

## Design

### Overview

Remove all cloud STT providers, the failover chain, and STT-related API key management. Replace with a single `LocalSttProvider` that runs a two-stage pipeline: whisper-rs transcription followed by pyannote-rs diarization. Models are downloaded on first use from HuggingFace, stored locally, and all subsequent transcription is fully offline.

The existing `SttProvider` trait is preserved — `LocalSttProvider` implements it, so the rest of the app (pipeline, SOAP generation, UI) doesn't need to change.

### Architecture

**Crate structure** (`crates/stt-providers/src/`):

| File | Responsibility |
|------|---------------|
| `lib.rs` | Exports `LocalSttProvider` |
| `local_provider.rs` | `SttProvider` impl, orchestrates the two-stage pipeline |
| `whisper.rs` | whisper-rs transcription wrapper |
| `diarization.rs` | pyannote-rs speaker diarization |
| `merge.rs` | Align whisper segments with speaker turns by timestamp overlap |
| `models.rs` | Model download, storage path resolution, validation |
| `audio_prep.rs` | Resample to 16kHz mono for whisper |

**Dependencies change:**
- Remove: `reqwest` (no more HTTP calls to STT APIs)
- Remove: `local-stt` feature flag (whisper-rs is now always required)
- Add: `whisper-rs` with `metal` feature (Apple Silicon GPU acceleration)
- Add: `pyannote-rs` / `native-pyannote-rs` (ONNX-based, no Python dependency)
- Add: `reqwest` stays only in `models.rs` for one-time model downloads (or use a lighter HTTP client)

### Transcription Pipeline

**Stage 1 — Whisper transcription:**
1. Receive `AudioData` (f32 PCM, any sample rate/channels)
2. Resample to 16kHz mono if needed (linear interpolation)
3. Run whisper-rs with `SamplingStrategy::Greedy { best_of: 1 }` on `spawn_blocking`
4. Extract segments with timestamps (start, end, text)

**Stage 2 — Pyannote diarization:**
1. Run segmentation model on 16kHz mono audio → speech boundary timestamps
2. Extract speaker embeddings per speech segment
3. Cluster embeddings using cosine similarity → assign speaker IDs
4. Produce speaker turns (speaker_id, start, end)
5. Run on `spawn_blocking`

**Merge:**
- For each whisper segment, find the speaker turn with maximum timestamp overlap
- Assign speaker ID → `TranscriptSegment.speaker = Some("Speaker 1")`
- Return standard `Transcript` struct — identical to what cloud providers returned

**Error handling:**
- Model not downloaded → descriptive error directing user to Settings
- Whisper inference fails → return error with details
- Diarization fails → fall back gracefully: return transcript without speaker labels (log warning, don't fail the transcription)

### Model Management

**Storage:** `{app_data_dir}/models/`
```
models/
  whisper/
    ggml-large-v3-turbo.bin
    ggml-small.bin
  pyannote/
    segmentation-3.0.onnx
    wespeaker-voxceleb-resnet34.onnx
```

**Available whisper models:**

| ID | File | Size | Description |
|----|------|------|-------------|
| `base` | `ggml-base.bin` | ~150MB | Fast, basic accuracy |
| `small` | `ggml-small.bin` | ~500MB | Good balance |
| `medium` | `ggml-medium.bin` | ~1.5GB | High accuracy |
| `large-v3-turbo` | `ggml-large-v3-turbo.bin` | ~1.6GB | Best accuracy, medical-grade |

**Pyannote models** (always required, ~45MB total):
- `segmentation-3.0.onnx` — speech boundary detection
- `wespeaker-voxceleb-resnet34.onnx` — speaker embeddings

**Download flow:**
1. User triggers transcription (or clicks download in Settings)
2. `LocalSttProvider` checks if selected whisper model + both pyannote models exist on disk
3. If missing: emits `model-download-progress` events and downloads from HuggingFace
4. Progress reported to frontend (percentage + model name)
5. Once complete, transcription proceeds
6. Subsequent runs skip download

**New Tauri commands:**
- `list_whisper_models` — returns available models with download status (size, downloaded boolean)
- `download_model` — triggers model download, emits `model-download-progress` events
- `delete_model` — removes a downloaded model from disk to reclaim space

### Speaker Labels

Generic labels only: "Speaker 1", "Speaker 2", etc. Pyannote assigns speaker IDs arbitrarily per recording so no attempt is made to map speakers to roles (Doctor/Patient). The SOAP generation step already understands multi-speaker transcripts without needing role labels.

### Settings Changes

**Remove:**
- `stt_provider` field from `AppConfig`
- STT provider dropdown in Settings UI
- STT-related API key fields (deepgram, elevenlabs, groq, modulate keys)

**Add:**
- `whisper_model: String` to `AppConfig` (default: `"large-v3-turbo"`)
- STT settings section in UI with:
  - Whisper model size dropdown (base / small / medium / large-v3-turbo)
  - Per-model status indicator: "Downloaded" / "Not downloaded" / "Downloading 42%"
  - Download button for undownloaded models
  - Delete button per model to reclaim disk space
  - Language selector (passed to whisper)

### State Initialization Changes

`init_stt_providers` no longer takes API keys. It:
1. Reads `whisper_model` from settings
2. Resolves model paths in `{app_data_dir}/models/`
3. Creates `LocalSttProvider` with model paths
4. Returns the provider directly (no failover chain)

If the model files don't exist yet, the provider is still created — it will trigger download on first `transcribe()` call or return an error directing users to Settings.

### Files Deleted

- `crates/stt-providers/src/deepgram.rs`
- `crates/stt-providers/src/elevenlabs_stt.rs`
- `crates/stt-providers/src/groq_whisper.rs`
- `crates/stt-providers/src/modulate.rs`
- `crates/stt-providers/src/failover.rs`
- `crates/stt-providers/src/whisper_local.rs` (replaced by new files)

### Files Modified

- `crates/stt-providers/src/lib.rs` — export `LocalSttProvider` only
- `crates/stt-providers/Cargo.toml` — swap dependencies
- `crates/core/src/types/settings.rs` — remove `stt_provider`, add `whisper_model`
- `src-tauri/src/state.rs` — simplify `init_stt_providers`
- `src-tauri/src/commands/transcription.rs` — call provider directly, no failover
- `src-tauri/src/commands/providers.rs` — remove STT from `reinit_providers`
- `src-tauri/src/lib.rs` — register new model management commands
- `src/lib/types/index.ts` — update `AppConfig` type
- `src/lib/stores/settings.ts` — update defaults
- `src/lib/components/SettingsContent.svelte` — replace STT section with model management UI

### What Doesn't Change

- `SttProvider` trait in `crates/core`
- `AudioData`, `SttConfig`, `Transcript`, `TranscriptSegment` types
- `transcribe_recording` Tauri command signature (still takes recording_id, language, diarize)
- Frontend transcription flow and pipeline integration
- SOAP generation pipeline
- AI provider system (completely separate)
