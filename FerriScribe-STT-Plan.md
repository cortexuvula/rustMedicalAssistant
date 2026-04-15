# FerriScribe Local STT Pipeline — Implementation Plan

## Context
FerriScribe is a Rust application that needs local, offline speech-to-text transcription with speaker diarization. The goal is to transcribe doctor-patient encounters with speaker labels (e.g., "Doctor:", "Patient:").

## Architecture Overview

```
Microphone/File → [Audio Capture] → [Whisper Transcription] → [Speaker Diarization] → Structured Output
                                                                    (pyannote)
```

Two-step pipeline:
1. **Transcription:** whisper.cpp (via Rust bindings)
2. **Speaker Diarization:** pyannote (via Rust ONNX bindings)

## Crate Selection

### Transcription: `whisper-rs` (v0.16.0)
- **Repo:** https://codeberg.org/tazz4843/whisper-rs
- **What it is:** Idiomatic Rust bindings for whisper.cpp
- **Features needed:**
  - `metal` — Apple Silicon GPU acceleration (critical for Mac)
  - Time-aligned segments with timestamps
  - Beam search and greedy sampling strategies
- **Model:** `large-v3-turbo` (already cached at `~/.cache/whisper/large-v3-turbo.pt`, ~1.5GB)
- **Note:** There's also `whisper-cpp-plus` which adds streaming PCM transcription and VAD — consider if real-time streaming is needed later.

### Diarization: `pyannote-rs` (native-pyannote-rs)
- **Repo:** https://github.com/thewh1teagle/pyannote-rs
- **What it is:** Rust port of pyannote.audio using ONNX Runtime (no Python dependency!)
- **Models:**
  - `segmentation-3.0` — identifies speech boundaries
  - `wespeaker-voxceleb-*` — generates speaker embeddings for identification
- **Method:** Cosine similarity on speaker embeddings to match speakers across segments
- **Build deps:** Cargo, Clang, CMake

### Alternative: `whisper-cpp-plus`
- Adds streaming transcription and VAD on top of whisper-rs
- Use if you want real-time streaming (patient dictation mode)
- Can coexist with `whisper-rs` as it wraps the same whisper.cpp

## Implementation Steps

### Step 1: Add Dependencies to `Cargo.toml`
```toml
[dependencies]
whisper-rs = { version = "0.16.0", features = ["metal"] }
native-pyannote-rs = "0.1"  # check latest version on crates.io
# or pyannote-rs = { git = "https://github.com/thewh1teagle/pyannote-rs" }
```

### Step 2: Whisper Transcription Module
Create `src/stt/transcription.rs`:

```rust
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let ctx = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default(),
        )?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, audio_data: &[f32], language: &str) -> Result<Vec<Segment>, Box<dyn std::error::Error>> {
        let params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_print_special(false);
        params.set_print_progress(true);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let mut state = self.ctx.create_state()?;
        state.full(params, audio_data)?;

        let mut segments = Vec::new();
        for segment in state.as_iter() {
            segments.push(Segment {
                start: segment.start_timestamp() as f64 / 100.0,
                end: segment.end_timestamp() as f64 / 100.0,
                text: segment.to_string(),
            });
        }
        Ok(segments)
    }
}

pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}
```

### Step 3: Speaker Diarization Module
Create `src/stt/diarization.rs`:

```rust
// Uses pyannote-rs for speaker identification
// Takes audio + whisper segments, returns speaker-labeled segments

pub struct Diarizer {
    segmentation_model: /* pyannote ONNX model */,
    embedding_model: /* wespeaker ONNX model */,
}

impl Diarizer {
    pub fn new(segmentation_path: &str, embedding_path: &str) -> Result<Self, ...> { ... }

    pub fn diarize(&self, audio: &[f32]) -> Result<Vec<SpeakerTurn>, ...> {
        // 1. Run segmentation to find speech boundaries
        // 2. Extract speaker embeddings per turn
        // 3. Cluster/compare using cosine similarity
        // 4. Return speaker-labeled turns
    }
}

pub struct SpeakerTurn {
    pub speaker_id: String,  // e.g., "SPEAKER_00", "SPEAKER_01"
    pub start: f64,
    pub end: f64,
}
```

### Step 4: Merge Module
Create `src/stt/merge.rs`:

```rust
// Align whisper segments with diarization turns
// Use timestamp overlap to assign speakers to transcription segments

pub fn merge(
    segments: &[Segment],
    speakers: &[SpeakerTurn],
) -> Vec<LabeledSegment> {
    // For each whisper segment, find the speaker turn
    // with maximum timestamp overlap
}
```

### Step 5: Audio Input Module
Create `src/stt/audio.rs`:

```rust
// Load audio files (WAV, MP3, etc.) and convert to 16kHz mono f32
// Dependencies: hound (WAV), symphonia (multi-format)
// Resample to 16kHz using rubato or dasp
```

## Audio Requirements
- Whisper requires: **16kHz, mono, f32 samples**
- If recording from mic, capture at native rate then resample
- Use `cpal` crate for cross-platform mic capture (if needed later)

## Model Storage
- Whisper models: `~/.cache/whisper/` (large-v3-turbo already there)
- pyannote models: `~/.cache/pyannote/` (need to download ONNX versions)
- Download ONNX models at first run with progress indicator

## Configuration
Add to FerriScribe config:
```toml
[stt]
whisper_model = "large-v3-turbo"  # or base/small/medium/large-v3
whisper_model_path = "~/.cache/whisper/"
language = "en"
diarization_enabled = true
speaker_labels = { "SPEAKER_00" = "Doctor", "SPEAKER_01" = "Patient" }
```

## Testing Plan
1. Unit test transcription with a known audio file
2. Unit test diarization with a 2-speaker audio file
3. Integration test: end-to-end with a doctor-patient mock conversation
4. Benchmark: measure real-time factor (RTF) on Apple Silicon with Metal

## Future Enhancements
- **Real-time streaming:** Switch to `whisper-cpp-plus` for live transcription with VAD
- **Wake word:** Add "Hey Ferri" wake word detection
- **Noise filtering:** Add noise gate/suppression before transcription
- **Custom vocabulary:** Medical terminology boosting in whisper

## Dependencies Summary
```toml
[dependencies]
whisper-rs = { version = "0.16.0", features = ["metal"] }
native-pyannote-rs = "0.1"  # ONNX-based, no Python needed
hound = "0.5"           # WAV reading
cpal = "0.15"           # Microphone capture (optional)
rubato = "0.15"         # Audio resampling
```
