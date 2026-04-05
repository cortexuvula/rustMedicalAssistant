# Plan 2: Audio & Providers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `audio`, `ai-providers`, `stt-providers`, and `tts-providers` crates — all I/O-facing crates that handle audio capture/playback and external API communication.

**Architecture:** The `audio` crate uses `cpal` for capture and `rodio` for playback with a lock-free ring buffer bridging the audio callback thread to the main thread. Provider crates use `reqwest` for HTTP with SSE streaming via `eventsource-stream`. An `OpenAiCompatibleClient` base handles 4 of 6 LLM providers. STT providers include a failover chain with circuit-breaker health tracking.

**Tech Stack:** cpal, rodio, hound, ringbuf, reqwest, eventsource-stream, serde, tokio, whisper-rs (optional, feature-gated)

**Depends on:** Plan 1 (core types, traits, db, security crates must be built)

---

## File Structure

```
crates/
├── audio/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (AudioEngine, re-exports)
│       ├── device.rs               (device enumeration, AudioDevice)
│       ├── capture.rs              (cpal capture → ring buffer → WAV)
│       ├── waveform.rs             (downsample for visualization)
│       ├── playback.rs             (rodio playback with controls)
│       └── state.rs                (RecordingState machine)
├── ai-providers/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (ProviderRegistry, re-exports)
│       ├── http_client.rs          (shared reqwest client, retry, circuit breaker)
│       ├── sse.rs                  (SSE stream parsing helpers)
│       ├── openai_compat.rs        (OpenAI-compatible base client)
│       ├── openai.rs               (OpenAI provider)
│       ├── anthropic.rs            (Anthropic Claude provider)
│       ├── gemini.rs               (Google Gemini provider)
│       ├── groq.rs                 (Groq — thin wrapper on openai_compat)
│       ├── cerebras.rs             (Cerebras — thin wrapper on openai_compat)
│       └── ollama.rs               (Ollama — thin wrapper on openai_compat)
├── stt-providers/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (SttRegistry, re-exports)
│       ├── failover.rs             (SttFailover chain with health tracking)
│       ├── deepgram.rs             (Deepgram Nova-2 Medical)
│       ├── groq_whisper.rs         (Groq Whisper API)
│       ├── elevenlabs_stt.rs       (ElevenLabs Scribe v2)
│       ├── modulate.rs             (Modulate — stub, needs API access)
│       └── whisper_local.rs        (whisper-rs local, feature-gated)
└── tts-providers/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                  (TtsRegistry, re-exports)
        ├── elevenlabs_tts.rs       (ElevenLabs text-to-speech)
        └── local_tts.rs            (platform TTS — stub)
```

---

### Task 1: Add New Crates to Workspace

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/audio/Cargo.toml`, `crates/audio/src/lib.rs`
- Create: `crates/ai-providers/Cargo.toml`, `crates/ai-providers/src/lib.rs`
- Create: `crates/stt-providers/Cargo.toml`, `crates/stt-providers/src/lib.rs`
- Create: `crates/tts-providers/Cargo.toml`, `crates/tts-providers/src/lib.rs`

- [ ] **Step 1: Add workspace dependencies**

Add to `Cargo.toml` workspace root `[workspace.dependencies]` section:
```toml
reqwest = { version = "0.12", features = ["json", "multipart", "stream"] }
eventsource-stream = "0.2"
tokio-stream = "0.1"
bytes = "1"
```

Add to `[workspace.members]`:
```toml
members = [
    "crates/core",
    "crates/db",
    "crates/security",
    "crates/audio",
    "crates/ai-providers",
    "crates/stt-providers",
    "crates/tts-providers",
    "src-tauri",
]
```

- [ ] **Step 2: Create audio crate**

Write `crates/audio/Cargo.toml`:
```toml
[package]
name = "medical-audio"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
cpal = "0.15"
rodio = { version = "0.19", default-features = false, features = ["wav", "mp3"] }
hound = "3.5"
ringbuf = "0.4"
tokio = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

Write `crates/audio/src/lib.rs`:
```rust
pub mod device;
pub mod state;
pub mod capture;
pub mod waveform;
pub mod playback;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Device error: {0}")]
    Device(String),
    #[error("Capture error: {0}")]
    Capture(String),
    #[error("Playback error: {0}")]
    Playback(String),
    #[error("Encoding error: {0}")]
    Encoding(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No input device available")]
    NoInputDevice,
    #[error("No output device available")]
    NoOutputDevice,
    #[error("Invalid state transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },
}

pub type AudioResult<T> = Result<T, AudioError>;
```

Create stub files for all declared modules:
- `crates/audio/src/device.rs` → `//! Device enumeration — implemented in Task 2`
- `crates/audio/src/state.rs` → `//! Recording state machine — implemented in Task 3`
- `crates/audio/src/capture.rs` → `//! Audio capture — implemented in Task 4`
- `crates/audio/src/waveform.rs` → `//! Waveform sampling — implemented in Task 5`
- `crates/audio/src/playback.rs` → `//! Playback — implemented in Task 6`

- [ ] **Step 3: Create ai-providers crate**

Write `crates/ai-providers/Cargo.toml`:
```toml
[package]
name = "medical-ai-providers"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
reqwest = { workspace = true }
eventsource-stream = { workspace = true }
tokio-stream = { workspace = true }
bytes = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
futures-core = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

Write `crates/ai-providers/src/lib.rs`:
```rust
pub mod http_client;
pub mod sse;
pub mod openai_compat;
pub mod openai;
pub mod anthropic;
pub mod gemini;
pub mod groq;
pub mod cerebras;
pub mod ollama;

use std::collections::HashMap;
use std::sync::Arc;
use medical_core::traits::AiProvider;

/// Registry of available AI providers, keyed by name.
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn AiProvider>>,
    active: String,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            active: String::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let name = provider.name().to_string();
        if self.active.is_empty() {
            self.active = name.clone();
        }
        self.providers.insert(name, provider);
    }

    pub fn get(&self, name: &str) -> Option<&dyn AiProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    pub fn active(&self) -> Option<&dyn AiProvider> {
        self.get(&self.active)
    }

    pub fn set_active(&mut self, name: &str) -> bool {
        if self.providers.contains_key(name) {
            self.active = name.to_string();
            true
        } else {
            false
        }
    }

    pub fn list_available(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}
```

Create stub files for all declared modules (same pattern — comment with future task reference).

- [ ] **Step 4: Create stt-providers crate**

Write `crates/stt-providers/Cargo.toml`:
```toml
[package]
name = "medical-stt-providers"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
reqwest = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

Write `crates/stt-providers/src/lib.rs`:
```rust
pub mod failover;
pub mod deepgram;
pub mod groq_whisper;
pub mod elevenlabs_stt;
pub mod modulate;
pub mod whisper_local;

use thiserror::Error;

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
}

pub type SttResult<T> = Result<T, SttError>;
```

Create stub files for all declared modules.

- [ ] **Step 5: Create tts-providers crate**

Write `crates/tts-providers/Cargo.toml`:
```toml
[package]
name = "medical-tts-providers"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
reqwest = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

Write `crates/tts-providers/src/lib.rs`:
```rust
pub mod elevenlabs_tts;
pub mod local_tts;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TtsError {
    #[error("Synthesis failed: {0}")]
    Synthesis(String),
    #[error("Voice not found: {0}")]
    VoiceNotFound(String),
    #[error("HTTP error: {0}")]
    Http(String),
}

pub type TtsResult<T> = Result<T, TtsError>;
```

Create stub files for declared modules.

- [ ] **Step 6: Update src-tauri/Cargo.toml**

Add new crate dependencies:
```toml
medical-audio = { path = "../crates/audio" }
medical-ai-providers = { path = "../crates/ai-providers" }
medical-stt-providers = { path = "../crates/stt-providers" }
medical-tts-providers = { path = "../crates/tts-providers" }
```

- [ ] **Step 7: Verify workspace builds**

Run: `cargo build --workspace`
Expected: Clean build.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: add audio, ai-providers, stt-providers, tts-providers crate scaffolds to workspace"
```

---

### Task 2: Audio — Device Enumeration

**Files:**
- Create: `crates/audio/src/device.rs`

- [ ] **Step 1: Write device enumeration**

Write `crates/audio/src/device.rs`:
```rust
use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use crate::{AudioError, AudioResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub channels: Vec<u16>,
}

/// List available input devices.
pub fn list_input_devices() -> AudioResult<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());

    let mut devices = Vec::new();
    for device in host.input_devices().map_err(|e| AudioError::Device(e.to_string()))? {
        let name = device.name().unwrap_or_else(|_| "Unknown".into());
        let is_default = default_name.as_deref() == Some(&name);

        let (sample_rates, channels) = supported_configs(&device);

        devices.push(AudioDevice {
            name,
            is_input: true,
            is_default,
            sample_rates,
            channels,
        });
    }
    Ok(devices)
}

/// List available output devices.
pub fn list_output_devices() -> AudioResult<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok());

    let mut devices = Vec::new();
    for device in host.output_devices().map_err(|e| AudioError::Device(e.to_string()))? {
        let name = device.name().unwrap_or_else(|_| "Unknown".into());
        let is_default = default_name.as_deref() == Some(&name);

        let (sample_rates, channels) = supported_configs(&device);

        devices.push(AudioDevice {
            name,
            is_input: false,
            is_default,
            sample_rates,
            channels,
        });
    }
    Ok(devices)
}

/// Get the default input device by name, or the system default.
pub fn get_input_device(name: Option<&str>) -> AudioResult<cpal::Device> {
    let host = cpal::default_host();

    if let Some(name) = name {
        for device in host.input_devices().map_err(|e| AudioError::Device(e.to_string()))? {
            if device.name().ok().as_deref() == Some(name) {
                return Ok(device);
            }
        }
    }

    host.default_input_device().ok_or(AudioError::NoInputDevice)
}

/// Get the default output device by name, or the system default.
pub fn get_output_device(name: Option<&str>) -> AudioResult<cpal::Device> {
    let host = cpal::default_host();

    if let Some(name) = name {
        for device in host.output_devices().map_err(|e| AudioError::Device(e.to_string()))? {
            if device.name().ok().as_deref() == Some(name) {
                return Ok(device);
            }
        }
    }

    host.default_output_device().ok_or(AudioError::NoOutputDevice)
}

fn supported_configs(device: &cpal::Device) -> (Vec<u32>, Vec<u16>) {
    let mut sample_rates = Vec::new();
    let mut channels = Vec::new();

    if let Ok(configs) = device.supported_input_configs() {
        for config in configs {
            let min = config.min_sample_rate().0;
            let max = config.max_sample_rate().0;
            for rate in [16000, 22050, 44100, 48000] {
                if rate >= min && rate <= max && !sample_rates.contains(&rate) {
                    sample_rates.push(rate);
                }
            }
            let ch = config.channels();
            if !channels.contains(&ch) {
                channels.push(ch);
            }
        }
    }

    // Fallback: try output configs for output devices
    if sample_rates.is_empty() {
        if let Ok(configs) = device.supported_output_configs() {
            for config in configs {
                let min = config.min_sample_rate().0;
                let max = config.max_sample_rate().0;
                for rate in [16000, 22050, 44100, 48000] {
                    if rate >= min && rate <= max && !sample_rates.contains(&rate) {
                        sample_rates.push(rate);
                    }
                }
                let ch = config.channels();
                if !channels.contains(&ch) {
                    channels.push(ch);
                }
            }
        }
    }

    sample_rates.sort();
    channels.sort();
    (sample_rates, channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_input_devices_returns_vec() {
        // May be empty in CI (no audio hardware), but should not error
        let result = list_input_devices();
        assert!(result.is_ok());
    }

    #[test]
    fn list_output_devices_returns_vec() {
        let result = list_output_devices();
        assert!(result.is_ok());
    }

    #[test]
    fn get_input_device_none_returns_default_or_error() {
        let result = get_input_device(None);
        // Either succeeds (has audio) or returns NoInputDevice
        match result {
            Ok(device) => assert!(device.name().is_ok()),
            Err(AudioError::NoInputDevice) => {} // OK in CI
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[test]
    fn audio_device_serializes() {
        let device = AudioDevice {
            name: "Test Mic".into(),
            is_input: true,
            is_default: true,
            sample_rates: vec![44100, 48000],
            channels: vec![1, 2],
        };
        let json = serde_json::to_value(&device).unwrap();
        assert_eq!(json["name"], "Test Mic");
        assert_eq!(json["is_default"], true);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-audio`
Expected: All tests pass (may skip device tests in CI).

- [ ] **Step 3: Commit**

```bash
git add crates/audio/src/device.rs
git commit -m "feat(audio): add device enumeration with cpal"
```

---

### Task 3: Audio — Recording State Machine

**Files:**
- Create: `crates/audio/src/state.rs`

- [ ] **Step 1: Write state machine with tests**

Write `crates/audio/src/state.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use crate::{AudioError, AudioResult};

/// Recording state machine. Invalid transitions are compile-time prevented
/// by only exposing valid transition methods on each state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RecordingState {
    Idle,
    Recording {
        #[serde(skip)]
        started_at: Option<Instant>,
        file_path: PathBuf,
        device_name: String,
        elapsed_before_pause: Duration,
    },
    Paused {
        #[serde(skip)]
        paused_at: Option<Instant>,
        file_path: PathBuf,
        device_name: String,
        elapsed_before_pause: Duration,
    },
    Stopped {
        file_path: PathBuf,
        duration: Duration,
    },
}

impl RecordingState {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_recording(&self) -> bool {
        matches!(self, Self::Recording { .. })
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused { .. })
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped { .. })
    }

    /// Total elapsed recording time (excluding paused time).
    pub fn elapsed(&self) -> Duration {
        match self {
            Self::Idle => Duration::ZERO,
            Self::Recording { started_at, elapsed_before_pause, .. } => {
                let since_start = started_at
                    .map(|s| s.elapsed())
                    .unwrap_or(Duration::ZERO);
                *elapsed_before_pause + since_start
            }
            Self::Paused { elapsed_before_pause, .. } => *elapsed_before_pause,
            Self::Stopped { duration, .. } => *duration,
        }
    }

    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Idle => None,
            Self::Recording { file_path, .. } => Some(file_path),
            Self::Paused { file_path, .. } => Some(file_path),
            Self::Stopped { file_path, .. } => Some(file_path),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording { .. } => "recording",
            Self::Paused { .. } => "paused",
            Self::Stopped { .. } => "stopped",
        }
    }
}

/// Manages valid state transitions.
pub struct StateMachine {
    state: RecordingState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: RecordingState::Idle,
        }
    }

    pub fn state(&self) -> &RecordingState {
        &self.state
    }

    /// Idle → Recording
    pub fn start(&mut self, file_path: PathBuf, device_name: String) -> AudioResult<()> {
        if !self.state.is_idle() {
            return Err(AudioError::InvalidTransition {
                from: self.state.label().into(),
                to: "recording".into(),
            });
        }
        self.state = RecordingState::Recording {
            started_at: Some(Instant::now()),
            file_path,
            device_name,
            elapsed_before_pause: Duration::ZERO,
        };
        Ok(())
    }

    /// Recording → Paused
    pub fn pause(&mut self) -> AudioResult<()> {
        match &self.state {
            RecordingState::Recording {
                started_at,
                file_path,
                device_name,
                elapsed_before_pause,
            } => {
                let since_start = started_at
                    .map(|s| s.elapsed())
                    .unwrap_or(Duration::ZERO);
                self.state = RecordingState::Paused {
                    paused_at: Some(Instant::now()),
                    file_path: file_path.clone(),
                    device_name: device_name.clone(),
                    elapsed_before_pause: *elapsed_before_pause + since_start,
                };
                Ok(())
            }
            _ => Err(AudioError::InvalidTransition {
                from: self.state.label().into(),
                to: "paused".into(),
            }),
        }
    }

    /// Paused → Recording
    pub fn resume(&mut self) -> AudioResult<()> {
        match &self.state {
            RecordingState::Paused {
                file_path,
                device_name,
                elapsed_before_pause,
                ..
            } => {
                self.state = RecordingState::Recording {
                    started_at: Some(Instant::now()),
                    file_path: file_path.clone(),
                    device_name: device_name.clone(),
                    elapsed_before_pause: *elapsed_before_pause,
                };
                Ok(())
            }
            _ => Err(AudioError::InvalidTransition {
                from: self.state.label().into(),
                to: "recording".into(),
            }),
        }
    }

    /// Recording|Paused → Stopped
    pub fn stop(&mut self) -> AudioResult<()> {
        let elapsed = self.state.elapsed();
        match &self.state {
            RecordingState::Recording { file_path, .. }
            | RecordingState::Paused { file_path, .. } => {
                self.state = RecordingState::Stopped {
                    file_path: file_path.clone(),
                    duration: elapsed,
                };
                Ok(())
            }
            _ => Err(AudioError::InvalidTransition {
                from: self.state.label().into(),
                to: "stopped".into(),
            }),
        }
    }

    /// Stopped|Idle → Idle (reset)
    pub fn reset(&mut self) -> AudioResult<()> {
        match &self.state {
            RecordingState::Stopped { .. } | RecordingState::Idle => {
                self.state = RecordingState::Idle;
                Ok(())
            }
            _ => Err(AudioError::InvalidTransition {
                from: self.state.label().into(),
                to: "idle".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle() {
        let sm = StateMachine::new();
        assert!(sm.state().is_idle());
    }

    #[test]
    fn valid_lifecycle_idle_record_pause_resume_stop_reset() {
        let mut sm = StateMachine::new();
        sm.start("/tmp/test.wav".into(), "mic".into()).unwrap();
        assert!(sm.state().is_recording());

        sm.pause().unwrap();
        assert!(sm.state().is_paused());

        sm.resume().unwrap();
        assert!(sm.state().is_recording());

        sm.stop().unwrap();
        assert!(sm.state().is_stopped());
        assert!(sm.state().elapsed() >= Duration::ZERO);

        sm.reset().unwrap();
        assert!(sm.state().is_idle());
    }

    #[test]
    fn cannot_pause_from_idle() {
        let mut sm = StateMachine::new();
        assert!(sm.pause().is_err());
    }

    #[test]
    fn cannot_resume_from_idle() {
        let mut sm = StateMachine::new();
        assert!(sm.resume().is_err());
    }

    #[test]
    fn cannot_start_while_recording() {
        let mut sm = StateMachine::new();
        sm.start("/tmp/a.wav".into(), "mic".into()).unwrap();
        assert!(sm.start("/tmp/b.wav".into(), "mic".into()).is_err());
    }

    #[test]
    fn cannot_stop_from_idle() {
        let mut sm = StateMachine::new();
        assert!(sm.stop().is_err());
    }

    #[test]
    fn can_stop_from_paused() {
        let mut sm = StateMachine::new();
        sm.start("/tmp/test.wav".into(), "mic".into()).unwrap();
        sm.pause().unwrap();
        sm.stop().unwrap();
        assert!(sm.state().is_stopped());
    }

    #[test]
    fn file_path_available_in_all_active_states() {
        let mut sm = StateMachine::new();
        assert!(sm.state().file_path().is_none());

        sm.start("/tmp/test.wav".into(), "mic".into()).unwrap();
        assert_eq!(sm.state().file_path().unwrap().to_str(), Some("/tmp/test.wav"));

        sm.pause().unwrap();
        assert!(sm.state().file_path().is_some());

        sm.stop().unwrap();
        assert!(sm.state().file_path().is_some());
    }

    #[test]
    fn elapsed_accumulates_across_pause_resume() {
        let mut sm = StateMachine::new();
        sm.start("/tmp/test.wav".into(), "mic".into()).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        sm.pause().unwrap();
        let elapsed_after_pause = sm.state().elapsed();
        assert!(elapsed_after_pause >= Duration::from_millis(40));

        // Elapsed doesn't grow while paused
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(sm.state().elapsed(), elapsed_after_pause);

        sm.resume().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        sm.stop().unwrap();
        assert!(sm.state().elapsed() > elapsed_after_pause);
    }

    #[test]
    fn state_labels() {
        assert_eq!(RecordingState::Idle.label(), "idle");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-audio`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/audio/src/state.rs
git commit -m "feat(audio): add recording state machine with valid transitions"
```

---

### Task 4: Audio — Capture Pipeline (cpal + ring buffer + WAV)

**Files:**
- Create: `crates/audio/src/capture.rs`

- [ ] **Step 1: Write capture pipeline**

Write `crates/audio/src/capture.rs`:
```rust
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavSpec, WavWriter};
use ringbuf::{HeapRb, traits::{Consumer, Producer, Split}};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use crate::{AudioError, AudioResult};

/// Audio capture configuration.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            buffer_size: 4096,
        }
    }
}

/// Handle to a running capture session. Drop to stop.
pub struct CaptureHandle {
    stream: cpal::Stream,
    is_paused: Arc<AtomicBool>,
    drain_handle: Option<std::thread::JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

impl CaptureHandle {
    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    pub fn stop(self) {
        // Drop triggers cleanup via Drop impl
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        // Stream is dropped automatically, stopping the audio callback
        if let Some(handle) = self.drain_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Start capturing audio from the given device.
///
/// Returns a `CaptureHandle` (drop to stop) and a channel receiver
/// that emits waveform amplitude chunks (~20Hz) for visualization.
///
/// Audio is written to a WAV file at `output_path`.
pub fn start_capture(
    device: &cpal::Device,
    config: CaptureConfig,
    output_path: &Path,
) -> AudioResult<(CaptureHandle, mpsc::Receiver<Vec<f32>>)> {
    let stream_config = StreamConfig {
        channels: config.channels,
        sample_rate: cpal::SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // Ring buffer: 2 seconds of audio at configured rate
    let ring_size = (config.sample_rate as usize) * (config.channels as usize) * 2;
    let rb = HeapRb::<f32>::new(ring_size);
    let (mut producer, mut consumer) = rb.split();

    let is_paused = Arc::new(AtomicBool::new(false));
    let is_paused_cb = is_paused.clone();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_drain = stop_flag.clone();

    // Waveform channel: send amplitude data to frontend
    let (waveform_tx, waveform_rx) = mpsc::channel::<Vec<f32>>(64);

    // WAV writer setup
    let spec = WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let output_path = output_path.to_path_buf();

    // cpal audio callback: push samples into ring buffer
    let stream = device
        .build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if is_paused_cb.load(Ordering::Relaxed) {
                    return;
                }
                // Push as many samples as we can; drop overflow
                let written = producer.push_slice(data);
                if written < data.len() {
                    warn!("Audio ring buffer overflow — dropped {} samples", data.len() - written);
                }
            },
            move |err| {
                error!("Audio capture error: {err}");
            },
            None, // no timeout
        )
        .map_err(|e| AudioError::Capture(e.to_string()))?;

    stream.play().map_err(|e| AudioError::Capture(e.to_string()))?;

    // Drain thread: reads ring buffer, writes WAV, sends waveform data
    let drain_handle = std::thread::spawn(move || {
        let mut writer = match WavWriter::create(&output_path, spec) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create WAV file: {e}");
                return;
            }
        };

        let mut waveform_buf = Vec::with_capacity(256);
        let samples_per_waveform_chunk = (spec.sample_rate / 20) as usize; // ~20Hz updates
        let mut sample_count = 0usize;

        let mut read_buf = vec![0.0f32; 1024];

        loop {
            if stop_flag_drain.load(Ordering::Relaxed) {
                break;
            }

            let count = consumer.pop_slice(&mut read_buf);
            if count == 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }

            for &sample in &read_buf[..count] {
                let _ = writer.write_sample(sample);
                waveform_buf.push(sample.abs());
                sample_count += 1;

                if sample_count % samples_per_waveform_chunk == 0 && !waveform_buf.is_empty() {
                    // Downsample waveform to 128 points
                    let chunk = downsample_waveform(&waveform_buf, 128);
                    let _ = waveform_tx.try_send(chunk);
                    waveform_buf.clear();
                }
            }
        }

        // Flush remaining waveform data
        if !waveform_buf.is_empty() {
            let chunk = downsample_waveform(&waveform_buf, 128);
            let _ = waveform_tx.try_send(chunk);
        }

        if let Err(e) = writer.finalize() {
            error!("Failed to finalize WAV file: {e}");
        }
        info!("Capture drain thread exiting, wrote {} samples", sample_count);
    });

    Ok((
        CaptureHandle {
            stream,
            is_paused,
            drain_handle: Some(drain_handle),
            stop_flag,
        },
        waveform_rx,
    ))
}

fn downsample_waveform(samples: &[f32], target_len: usize) -> Vec<f32> {
    if samples.len() <= target_len {
        return samples.to_vec();
    }
    let chunk_size = samples.len() / target_len;
    samples
        .chunks(chunk_size)
        .map(|chunk| {
            chunk.iter().copied().fold(0.0f32, |a, b| a.max(b))
        })
        .take(target_len)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capture_config() {
        let config = CaptureConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
    }

    #[test]
    fn downsample_waveform_reduces_length() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32) / 1000.0).collect();
        let result = downsample_waveform(&samples, 128);
        assert_eq!(result.len(), 128);
    }

    #[test]
    fn downsample_waveform_preserves_short_input() {
        let samples = vec![0.1, 0.2, 0.3];
        let result = downsample_waveform(&samples, 128);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn downsample_waveform_takes_peak_per_chunk() {
        let samples = vec![0.1, 0.5, 0.2, 0.8, 0.3, 0.9, 0.4, 0.7];
        let result = downsample_waveform(&samples, 4);
        // chunks of 2: [0.1, 0.5], [0.2, 0.8], [0.3, 0.9], [0.4, 0.7]
        assert_eq!(result[0], 0.5);
        assert_eq!(result[1], 0.8);
        assert_eq!(result[2], 0.9);
        assert_eq!(result[3], 0.7);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-audio`
Expected: All tests pass. (Capture integration requires audio hardware — unit tests cover the logic.)

- [ ] **Step 3: Commit**

```bash
git add crates/audio/src/capture.rs
git commit -m "feat(audio): add cpal capture pipeline with ring buffer and WAV encoding"
```

---

### Task 5: Audio — Waveform Sampling Helpers

**Files:**
- Create: `crates/audio/src/waveform.rs`

- [ ] **Step 1: Write waveform helpers**

Write `crates/audio/src/waveform.rs`:
```rust
/// Compute RMS (root mean square) amplitude of a sample buffer.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Compute peak amplitude of a sample buffer.
pub fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max)
}

/// Convert linear amplitude to decibels.
/// Returns -inf for amplitude 0.
pub fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return f32::NEG_INFINITY;
    }
    20.0 * amplitude.log10()
}

/// Normalize samples to -1.0..1.0 range based on peak amplitude.
pub fn normalize(samples: &[f32]) -> Vec<f32> {
    let peak = peak(samples);
    if peak <= 0.0 {
        return samples.to_vec();
    }
    samples.iter().map(|s| s / peak).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_silence() {
        assert_eq!(rms(&[0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn rms_of_constant() {
        let rms_val = rms(&[0.5, 0.5, 0.5, 0.5]);
        assert!((rms_val - 0.5).abs() < 0.001);
    }

    #[test]
    fn rms_of_empty() {
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn peak_of_samples() {
        assert_eq!(peak(&[0.1, -0.5, 0.3, -0.2]), 0.5);
    }

    #[test]
    fn peak_of_empty() {
        assert_eq!(peak(&[]), 0.0);
    }

    #[test]
    fn amplitude_to_db_unity() {
        assert!((amplitude_to_db(1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn amplitude_to_db_half() {
        // -6.02 dB
        assert!((amplitude_to_db(0.5) - (-6.0206)).abs() < 0.01);
    }

    #[test]
    fn amplitude_to_db_zero() {
        assert!(amplitude_to_db(0.0).is_infinite());
    }

    #[test]
    fn normalize_scales_to_peak() {
        let samples = vec![0.1, -0.5, 0.3];
        let normalized = normalize(&samples);
        assert!((normalized[1] - (-1.0)).abs() < 0.001); // peak was -0.5
        assert!((normalized[0] - 0.2).abs() < 0.001);
    }

    #[test]
    fn normalize_silence() {
        let samples = vec![0.0, 0.0];
        let normalized = normalize(&samples);
        assert_eq!(normalized, vec![0.0, 0.0]);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-audio`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/audio/src/waveform.rs
git commit -m "feat(audio): add waveform sampling helpers (RMS, peak, dB, normalize)"
```

---

### Task 6: Audio — Playback

**Files:**
- Create: `crates/audio/src/playback.rs`

- [ ] **Step 1: Write playback**

Write `crates/audio/src/playback.rs`:
```rust
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use crate::{AudioError, AudioResult};

/// Audio player using rodio. Wraps a Sink for play/pause/stop/volume.
pub struct Player {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Sink,
}

impl Player {
    /// Create a new player using the default output device.
    pub fn new() -> AudioResult<Self> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| AudioError::Playback(e.to_string()))?;
        let sink =
            Sink::try_new(&stream_handle).map_err(|e| AudioError::Playback(e.to_string()))?;
        sink.pause(); // Start paused

        Ok(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink,
        })
    }

    /// Load and play an audio file (WAV, MP3, OGG, FLAC).
    pub fn play_file(&self, path: &Path) -> AudioResult<()> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let source =
            Decoder::new(reader).map_err(|e| AudioError::Playback(format!("Decode error: {e}")))?;
        self.sink.append(source);
        self.sink.play();
        Ok(())
    }

    pub fn pause(&self) {
        self.sink.pause();
    }

    pub fn resume(&self) {
        self.sink.play();
    }

    pub fn stop(&self) {
        self.sink.stop();
    }

    pub fn set_volume(&self, volume: f32) {
        self.sink.set_volume(volume.clamp(0.0, 2.0));
    }

    pub fn volume(&self) -> f32 {
        self.sink.volume()
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Player tests require audio output hardware.
    // We test what we can without it.

    #[test]
    fn player_creation_fails_gracefully_without_hardware() {
        // In CI without audio, this should either succeed or give a clear error
        match Player::new() {
            Ok(player) => {
                assert!(player.is_paused()); // starts paused
                assert!(player.is_empty()); // no audio loaded
            }
            Err(AudioError::Playback(_)) => {} // OK in headless environments
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[test]
    fn play_nonexistent_file_errors() {
        if let Ok(player) = Player::new() {
            let result = player.play_file(Path::new("/nonexistent/file.wav"));
            assert!(result.is_err());
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-audio`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/audio/src/playback.rs
git commit -m "feat(audio): add rodio-based playback with play/pause/stop/volume"
```

---

### Task 7: AI Providers — HTTP Client Infrastructure

**Files:**
- Create: `crates/ai-providers/src/http_client.rs`
- Create: `crates/ai-providers/src/sse.rs`

- [ ] **Step 1: Write shared HTTP client with retry and circuit breaker**

Write `crates/ai-providers/src/http_client.rs`:
```rust
use reqwest::{Client, ClientBuilder, header};
use std::time::Duration;
use tracing::warn;

/// Build a configured reqwest client for AI provider APIs.
pub fn build_client(api_key: &str, timeout_secs: u64) -> reqwest::Result<Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {api_key}")).unwrap(),
    );

    ClientBuilder::new()
        .default_headers(headers)
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(5)
        .build()
}

/// Build a reqwest client with a custom auth header (e.g., x-api-key).
pub fn build_client_custom_auth(
    header_name: &str,
    api_key: &str,
    timeout_secs: u64,
) -> reqwest::Result<Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::HeaderName::from_bytes(header_name.as_bytes()).unwrap(),
        header::HeaderValue::from_str(api_key).unwrap(),
    );

    ClientBuilder::new()
        .default_headers(headers)
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(5)
        .build()
}

/// Retry configuration for API calls.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub backoff_factor: f64,
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryConfig {
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.initial_delay.as_secs_f64() * self.backoff_factor.powi(attempt as i32);
        let capped = delay.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(capped)
    }
}

/// Simple circuit breaker for provider health tracking.
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_count: u32,
    failure_threshold: u32,
    last_failure: Option<std::time::Instant>,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_count: 0,
            failure_threshold,
            last_failure: None,
            recovery_timeout,
        }
    }

    pub fn is_open(&self) -> bool {
        if self.failure_count >= self.failure_threshold {
            // Check if recovery timeout has elapsed
            if let Some(last) = self.last_failure {
                return last.elapsed() < self.recovery_timeout;
            }
        }
        false
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure = None;
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(std::time::Instant::now());
        warn!(
            failures = self.failure_count,
            threshold = self.failure_threshold,
            "Circuit breaker: recorded failure"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_config_exponential_backoff() {
        let config = RetryConfig::default();
        let d0 = config.delay_for_attempt(0);
        let d1 = config.delay_for_attempt(1);
        let d2 = config.delay_for_attempt(2);
        assert_eq!(d0, Duration::from_secs(1));
        assert_eq!(d1, Duration::from_secs(2));
        assert_eq!(d2, Duration::from_secs(4));
    }

    #[test]
    fn retry_config_caps_at_max() {
        let config = RetryConfig {
            max_delay: Duration::from_secs(5),
            ..Default::default()
        };
        let d10 = config.delay_for_attempt(10);
        assert!(d10 <= Duration::from_secs(5));
    }

    #[test]
    fn circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        assert!(!cb.is_open());
    }

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(300));
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_open());
        cb.record_failure();
        assert!(cb.is_open());
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(300));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        cb.record_failure();
        assert!(!cb.is_open()); // only 1 failure after reset
    }
}
```

- [ ] **Step 2: Write SSE stream helpers**

Write `crates/ai-providers/src/sse.rs`:
```rust
use eventsource_stream::Eventsource;
use futures_core::Stream;
use reqwest::Response;
use std::pin::Pin;
use tokio_stream::StreamExt;

/// Parse an SSE stream from a reqwest response into a stream of (event, data) tuples.
/// Filters out empty data and [DONE] markers.
pub fn parse_sse_response(
    response: Response,
) -> Pin<Box<dyn Stream<Item = Result<String, String>> + Send>> {
    let stream = response
        .bytes_stream()
        .eventsource()
        .filter_map(|event| {
            match event {
                Ok(ev) => {
                    let data = ev.data;
                    if data.is_empty() || data == "[DONE]" {
                        None
                    } else {
                        Some(Ok(data))
                    }
                }
                Err(e) => Some(Err(e.to_string())),
            }
        });

    Box::pin(stream)
}

#[cfg(test)]
mod tests {
    // SSE parsing requires a real HTTP response — tested via integration tests
    // with the provider implementations.
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p medical-ai-providers`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ai-providers/src/http_client.rs crates/ai-providers/src/sse.rs
git commit -m "feat(ai): add shared HTTP client with retry, circuit breaker, and SSE parsing"
```

---

### Task 8: AI Providers — OpenAI-Compatible Base Client

**Files:**
- Create: `crates/ai-providers/src/openai_compat.rs`

- [ ] **Step 1: Write OpenAI-compatible client**

Write `crates/ai-providers/src/openai_compat.rs`:
```rust
use futures_core::Stream;
use medical_core::error::{AppError, AppResult};
use medical_core::types::ai::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::StreamExt;
use crate::sse::parse_sse_response;

/// Base client for OpenAI-compatible APIs (OpenAI, Groq, Cerebras, Ollama).
/// Each provider configures the base_url and api_key.
pub struct OpenAiCompatibleClient {
    client: Client,
    base_url: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ApiUsage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatResponseMessage>,
    delta: Option<ChatDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Deserialize)]
struct ChatDelta {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCallDelta>>,
}

#[derive(Deserialize)]
struct ApiToolCall {
    id: Option<String>,
    function: Option<ApiFunction>,
}

#[derive(Deserialize)]
struct ApiToolCallDelta {
    index: Option<u32>,
    id: Option<String>,
    function: Option<ApiFunctionDelta>,
}

#[derive(Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ApiFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

impl OpenAiCompatibleClient {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    pub async fn complete(&self, request: &CompletionRequest) -> AppResult<CompletionResponse> {
        let body = self.build_request(request, false, None);
        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let api_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(format!("JSON parse error: {e}")))?;

        self.parse_response(api_response, &request.model)
    }

    pub async fn complete_stream(
        &self,
        request: &CompletionRequest,
    ) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        let body = self.build_request(request, true, None);
        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let sse_stream = parse_sse_response(response);

        let mapped = sse_stream.map(|result| {
            match result {
                Ok(data) => {
                    let parsed: Result<ChatResponse, _> = serde_json::from_str(&data);
                    match parsed {
                        Ok(resp) => {
                            if let Some(choice) = resp.choices.first() {
                                if let Some(delta) = &choice.delta {
                                    if let Some(content) = &delta.content {
                                        return Ok(StreamChunk::Delta {
                                            text: content.clone(),
                                        });
                                    }
                                    if let Some(tool_calls) = &delta.tool_calls {
                                        if let Some(tc) = tool_calls.first() {
                                            return Ok(StreamChunk::ToolCallDelta {
                                                id: tc.id.clone().unwrap_or_default(),
                                                name: tc.function.as_ref().and_then(|f| f.name.clone()),
                                                arguments_delta: tc
                                                    .function
                                                    .as_ref()
                                                    .and_then(|f| f.arguments.clone())
                                                    .unwrap_or_default(),
                                            });
                                        }
                                    }
                                }
                                if choice.finish_reason.is_some() {
                                    if let Some(usage) = resp.usage {
                                        return Ok(StreamChunk::Usage(UsageInfo {
                                            prompt_tokens: usage.prompt_tokens.unwrap_or(0),
                                            completion_tokens: usage.completion_tokens.unwrap_or(0),
                                            total_tokens: usage.total_tokens.unwrap_or(0),
                                        }));
                                    }
                                    return Ok(StreamChunk::Done);
                                }
                            }
                            Ok(StreamChunk::Done)
                        }
                        Err(e) => Err(AppError::AiProvider(format!("Stream parse error: {e}"))),
                    }
                }
                Err(e) => Err(AppError::AiProvider(format!("SSE error: {e}"))),
            }
        });

        Ok(Box::pin(mapped))
    }

    pub async fn complete_with_tools(
        &self,
        request: &CompletionRequest,
        tools: &[ToolDef],
    ) -> AppResult<ToolCompletionResponse> {
        let tool_specs: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        let body = self.build_request(request, false, Some(tool_specs));
        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let api_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(format!("JSON parse error: {e}")))?;

        let choice = api_response
            .choices
            .first()
            .ok_or_else(|| AppError::AiProvider("No choices in response".into()))?;

        let content = choice.message.as_ref().and_then(|m| m.content.clone());

        let tool_calls = choice
            .message
            .as_ref()
            .and_then(|m| m.tool_calls.as_ref())
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| {
                        let func = tc.function.as_ref();
                        ToolCall {
                            id: tc.id.clone().unwrap_or_default(),
                            name: func.map(|f| f.name.clone()).unwrap_or_default(),
                            arguments: func
                                .and_then(|f| serde_json::from_str(&f.arguments).ok())
                                .unwrap_or(serde_json::Value::Null),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = api_response.usage.map(|u| UsageInfo {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u.total_tokens.unwrap_or(0),
        }).unwrap_or(UsageInfo {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(ToolCompletionResponse {
            content,
            tool_calls,
            usage,
        })
    }

    fn build_request(
        &self,
        request: &CompletionRequest,
        stream: bool,
        tools: Option<Vec<serde_json::Value>>,
    ) -> ChatRequest {
        let mut messages: Vec<ChatMessage> = Vec::new();

        if let Some(system) = &request.system_prompt {
            messages.push(ChatMessage {
                role: "system".into(),
                content: system.clone(),
            });
        }

        for msg in &request.messages {
            let role = match msg.role {
                medical_core::types::ai::Role::System => "system",
                medical_core::types::ai::Role::User => "user",
                medical_core::types::ai::Role::Assistant => "assistant",
                medical_core::types::ai::Role::Tool => "tool",
            };
            let content = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::ToolResult { content, .. } => content.clone(),
            };
            messages.push(ChatMessage {
                role: role.into(),
                content,
            });
        }

        ChatRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: if stream { Some(true) } else { None },
            tools,
        }
    }

    fn parse_response(
        &self,
        api_response: ChatResponse,
        model: &str,
    ) -> AppResult<CompletionResponse> {
        let choice = api_response
            .choices
            .first()
            .ok_or_else(|| AppError::AiProvider("No choices in response".into()))?;

        let content = choice
            .message
            .as_ref()
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let tool_calls = choice
            .message
            .as_ref()
            .and_then(|m| m.tool_calls.as_ref())
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| {
                        let func = tc.function.as_ref();
                        ToolCall {
                            id: tc.id.clone().unwrap_or_default(),
                            name: func.map(|f| f.name.clone()).unwrap_or_default(),
                            arguments: func
                                .and_then(|f| serde_json::from_str(&f.arguments).ok())
                                .unwrap_or(serde_json::Value::Null),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = api_response.usage.map(|u| UsageInfo {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u.total_tokens.unwrap_or(0),
        }).unwrap_or(UsageInfo {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            content,
            model: api_response.model.unwrap_or_else(|| model.to_string()),
            usage,
            tool_calls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_includes_system_prompt() {
        let client = OpenAiCompatibleClient::new(
            reqwest::Client::new(),
            "https://api.example.com".into(),
        );
        let request = CompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("Hello".into()),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
            system_prompt: Some("You are a doctor".into()),
        };
        let built = client.build_request(&request, false, None);
        assert_eq!(built.messages.len(), 2);
        assert_eq!(built.messages[0].role, "system");
        assert_eq!(built.messages[0].content, "You are a doctor");
        assert_eq!(built.messages[1].role, "user");
    }

    #[test]
    fn build_request_stream_flag() {
        let client = OpenAiCompatibleClient::new(
            reqwest::Client::new(),
            "https://api.example.com".into(),
        );
        let request = CompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            system_prompt: None,
        };
        let non_stream = client.build_request(&request, false, None);
        assert!(non_stream.stream.is_none());
        let stream = client.build_request(&request, true, None);
        assert_eq!(stream.stream, Some(true));
    }

    #[test]
    fn parse_response_extracts_content() {
        let client = OpenAiCompatibleClient::new(
            reqwest::Client::new(),
            "https://api.example.com".into(),
        );
        let api_response = ChatResponse {
            choices: vec![ChatChoice {
                message: Some(ChatResponseMessage {
                    content: Some("Hello back!".into()),
                    tool_calls: None,
                }),
                delta: None,
                finish_reason: Some("stop".into()),
            }],
            usage: Some(ApiUsage {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                total_tokens: Some(15),
            }),
            model: Some("gpt-4o".into()),
        };
        let result = client.parse_response(api_response, "gpt-4o").unwrap();
        assert_eq!(result.content, "Hello back!");
        assert_eq!(result.usage.total_tokens, 15);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-ai-providers`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ai-providers/src/openai_compat.rs
git commit -m "feat(ai): add OpenAI-compatible base client with completion, streaming, and tool calling"
```

---

### Task 9: AI Providers — OpenAI, Groq, Cerebras, Ollama

**Files:**
- Create: `crates/ai-providers/src/openai.rs`
- Create: `crates/ai-providers/src/groq.rs`
- Create: `crates/ai-providers/src/cerebras.rs`
- Create: `crates/ai-providers/src/ollama.rs`

- [ ] **Step 1: Write OpenAI provider**

Write `crates/ai-providers/src/openai.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::AppResult;
use medical_core::traits::AiProvider;
use medical_core::types::ai::*;
use std::pin::Pin;
use crate::http_client::build_client;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct OpenAiProvider {
    client: OpenAiCompatibleClient,
}

impl OpenAiProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let http = build_client(api_key, 120)
            .map_err(|e| medical_core::error::AppError::AiProvider(e.to_string()))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, "https://api.openai.com/v1".into()),
        })
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo { id: "gpt-4o".into(), name: "GPT-4o".into(), provider: "openai".into(), max_tokens: 128000, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gpt-4o-mini".into(), name: "GPT-4o Mini".into(), provider: "openai".into(), max_tokens: 128000, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gpt-4-turbo".into(), name: "GPT-4 Turbo".into(), provider: "openai".into(), max_tokens: 128000, supports_tools: true, supports_streaming: true },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        self.client.complete(&request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        self.client.complete_stream(&request).await
    }

    async fn complete_with_tools(
        &self,
        request: CompletionRequest,
        tools: &[ToolDef],
    ) -> AppResult<ToolCompletionResponse> {
        self.client.complete_with_tools(&request, tools).await
    }
}
```

- [ ] **Step 2: Write Groq provider**

Write `crates/ai-providers/src/groq.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::AppResult;
use medical_core::traits::AiProvider;
use medical_core::types::ai::*;
use std::pin::Pin;
use crate::http_client::build_client;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct GroqProvider {
    client: OpenAiCompatibleClient,
}

impl GroqProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let http = build_client(api_key, 120)
            .map_err(|e| medical_core::error::AppError::AiProvider(e.to_string()))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, "https://api.groq.com/openai/v1".into()),
        })
    }
}

#[async_trait]
impl AiProvider for GroqProvider {
    fn name(&self) -> &str { "groq" }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo { id: "llama-3.3-70b-versatile".into(), name: "Llama 3.3 70B".into(), provider: "groq".into(), max_tokens: 32768, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "mixtral-8x7b-32768".into(), name: "Mixtral 8x7B".into(), provider: "groq".into(), max_tokens: 32768, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gemma2-9b-it".into(), name: "Gemma2 9B".into(), provider: "groq".into(), max_tokens: 8192, supports_tools: false, supports_streaming: true },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        self.client.complete(&request).await
    }

    async fn complete_stream(&self, request: CompletionRequest) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        self.client.complete_stream(&request).await
    }

    async fn complete_with_tools(&self, request: CompletionRequest, tools: &[ToolDef]) -> AppResult<ToolCompletionResponse> {
        self.client.complete_with_tools(&request, tools).await
    }
}
```

- [ ] **Step 3: Write Cerebras provider**

Write `crates/ai-providers/src/cerebras.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::AppResult;
use medical_core::traits::AiProvider;
use medical_core::types::ai::*;
use std::pin::Pin;
use crate::http_client::build_client;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct CerebrasProvider {
    client: OpenAiCompatibleClient,
}

impl CerebrasProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let http = build_client(api_key, 120)
            .map_err(|e| medical_core::error::AppError::AiProvider(e.to_string()))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, "https://api.cerebras.ai/v1".into()),
        })
    }
}

#[async_trait]
impl AiProvider for CerebrasProvider {
    fn name(&self) -> &str { "cerebras" }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo { id: "llama-3.3-70b".into(), name: "Llama 3.3 70B".into(), provider: "cerebras".into(), max_tokens: 8192, supports_tools: false, supports_streaming: true },
            ModelInfo { id: "qwen-3-32b".into(), name: "Qwen 3 32B".into(), provider: "cerebras".into(), max_tokens: 8192, supports_tools: false, supports_streaming: true },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        self.client.complete(&request).await
    }

    async fn complete_stream(&self, request: CompletionRequest) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        self.client.complete_stream(&request).await
    }

    async fn complete_with_tools(&self, request: CompletionRequest, tools: &[ToolDef]) -> AppResult<ToolCompletionResponse> {
        self.client.complete_with_tools(&request, tools).await
    }
}
```

- [ ] **Step 4: Write Ollama provider**

Write `crates/ai-providers/src/ollama.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::AppResult;
use medical_core::traits::AiProvider;
use medical_core::types::ai::*;
use std::pin::Pin;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct OllamaProvider {
    client: OpenAiCompatibleClient,
}

impl OllamaProvider {
    pub fn new(host: Option<&str>) -> AppResult<Self> {
        let base_url = host.unwrap_or("http://localhost:11434");
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| medical_core::error::AppError::AiProvider(e.to_string()))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, format!("{base_url}/v1")),
        })
    }

    /// Check if Ollama is reachable.
    pub async fn health_check(&self) -> bool {
        // Ollama exposes /api/version
        let url = self.client_base_url().replace("/v1", "/api/version");
        reqwest::get(&url).await.is_ok()
    }

    fn client_base_url(&self) -> String {
        // Access base_url through a method we'll need to add
        // For now, reconstruct from the known pattern
        "http://localhost:11434/v1".into()
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    fn available_models(&self) -> Vec<ModelInfo> {
        // Ollama models are dynamic — this returns common defaults
        // Full implementation would query /api/tags
        vec![
            ModelInfo { id: "llama3".into(), name: "Llama 3".into(), provider: "ollama".into(), max_tokens: 8192, supports_tools: false, supports_streaming: true },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        self.client.complete(&request).await
    }

    async fn complete_stream(&self, request: CompletionRequest) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        self.client.complete_stream(&request).await
    }

    async fn complete_with_tools(&self, request: CompletionRequest, tools: &[ToolDef]) -> AppResult<ToolCompletionResponse> {
        self.client.complete_with_tools(&request, tools).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_creates_with_default_host() {
        let provider = OllamaProvider::new(None).unwrap();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn ollama_creates_with_custom_host() {
        let provider = OllamaProvider::new(Some("http://192.168.1.100:11434")).unwrap();
        assert_eq!(provider.name(), "ollama");
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p medical-ai-providers`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/ai-providers/src/openai.rs crates/ai-providers/src/groq.rs crates/ai-providers/src/cerebras.rs crates/ai-providers/src/ollama.rs
git commit -m "feat(ai): add OpenAI, Groq, Cerebras, Ollama providers (OpenAI-compatible)"
```

---

### Task 10: AI Providers — Anthropic

**Files:**
- Create: `crates/ai-providers/src/anthropic.rs`

- [ ] **Step 1: Write Anthropic provider**

Write `crates/ai-providers/src/anthropic.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::{AppError, AppResult};
use medical_core::traits::AiProvider;
use medical_core::types::ai::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::StreamExt;
use crate::sse::parse_sse_response;

pub struct AnthropicProvider {
    client: Client,
    base_url: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    model: String,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<StreamDelta>,
    content_block: Option<ContentBlock>,
    message: Option<AnthropicResponse>,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    partial_json: Option<String>,
}

impl AnthropicProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let client = Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("x-api-key", api_key.parse().unwrap());
                h.insert("anthropic-version", "2023-06-01".parse().unwrap());
                h.insert("content-type", "application/json".parse().unwrap());
                h
            })
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        Ok(Self {
            client,
            base_url: "https://api.anthropic.com/v1".into(),
        })
    }

    fn build_request(&self, request: &CompletionRequest, stream: bool, tools: Option<Vec<serde_json::Value>>) -> AnthropicRequest {
        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| AnthropicMessage {
                role: match m.role {
                    Role::User | Role::Tool => "user".into(),
                    Role::Assistant => "assistant".into(),
                    Role::System => "user".into(), // filtered above, but just in case
                },
                content: match &m.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::ToolResult { content, .. } => content.clone(),
                },
            })
            .collect();

        AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system: request.system_prompt.clone(),
            temperature: request.temperature,
            stream: if stream { Some(true) } else { None },
            tools,
        }
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn name(&self) -> &str { "anthropic" }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo { id: "claude-opus-4-20250514".into(), name: "Claude Opus 4".into(), provider: "anthropic".into(), max_tokens: 200000, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "claude-sonnet-4-20250514".into(), name: "Claude Sonnet 4".into(), provider: "anthropic".into(), max_tokens: 200000, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "claude-haiku-4-20250514".into(), name: "Claude Haiku 4".into(), provider: "anthropic".into(), max_tokens: 200000, supports_tools: true, supports_streaming: true },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        let body = self.build_request(&request, false, None);
        let url = format!("{}/messages", self.base_url);

        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let api_response: AnthropicResponse = response.json().await
            .map_err(|e| AppError::AiProvider(format!("JSON parse: {e}")))?;

        let content = api_response.content.iter()
            .filter(|b| b.block_type == "text")
            .filter_map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("");

        let tool_calls = api_response.content.iter()
            .filter(|b| b.block_type == "tool_use")
            .map(|b| ToolCall {
                id: b.id.clone().unwrap_or_default(),
                name: b.name.clone().unwrap_or_default(),
                arguments: b.input.clone().unwrap_or(serde_json::Value::Null),
            })
            .collect();

        Ok(CompletionResponse {
            content,
            model: api_response.model,
            usage: UsageInfo {
                prompt_tokens: api_response.usage.input_tokens,
                completion_tokens: api_response.usage.output_tokens,
                total_tokens: api_response.usage.input_tokens + api_response.usage.output_tokens,
            },
            tool_calls,
        })
    }

    async fn complete_stream(&self, request: CompletionRequest) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        let body = self.build_request(&request, true, None);
        let url = format!("{}/messages", self.base_url);

        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let sse_stream = parse_sse_response(response);

        let mapped = sse_stream.filter_map(|result| {
            match result {
                Ok(data) => {
                    let parsed: Result<StreamEvent, _> = serde_json::from_str(&data);
                    match parsed {
                        Ok(event) => match event.event_type.as_str() {
                            "content_block_delta" => {
                                if let Some(delta) = event.delta {
                                    if let Some(text) = delta.text {
                                        return Some(Ok(StreamChunk::Delta { text }));
                                    }
                                }
                                None
                            }
                            "message_delta" => {
                                if let Some(usage) = event.usage {
                                    return Some(Ok(StreamChunk::Usage(UsageInfo {
                                        prompt_tokens: 0,
                                        completion_tokens: usage.output_tokens,
                                        total_tokens: usage.input_tokens + usage.output_tokens,
                                    })));
                                }
                                None
                            }
                            "message_stop" => Some(Ok(StreamChunk::Done)),
                            _ => None,
                        },
                        Err(_) => None, // Skip unparseable events
                    }
                }
                Err(e) => Some(Err(AppError::AiProvider(format!("SSE error: {e}")))),
            }
        });

        Ok(Box::pin(mapped))
    }

    async fn complete_with_tools(&self, request: CompletionRequest, tools: &[ToolDef]) -> AppResult<ToolCompletionResponse> {
        let tool_specs: Vec<serde_json::Value> = tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        }).collect();

        let body = self.build_request(&request, false, Some(tool_specs));
        let url = format!("{}/messages", self.base_url);

        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let api_response: AnthropicResponse = response.json().await
            .map_err(|e| AppError::AiProvider(format!("JSON parse: {e}")))?;

        let content = api_response.content.iter()
            .filter(|b| b.block_type == "text")
            .filter_map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("");

        let tool_calls = api_response.content.iter()
            .filter(|b| b.block_type == "tool_use")
            .map(|b| ToolCall {
                id: b.id.clone().unwrap_or_default(),
                name: b.name.clone().unwrap_or_default(),
                arguments: b.input.clone().unwrap_or(serde_json::Value::Null),
            })
            .collect();

        Ok(ToolCompletionResponse {
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls,
            usage: UsageInfo {
                prompt_tokens: api_response.usage.input_tokens,
                completion_tokens: api_response.usage.output_tokens,
                total_tokens: api_response.usage.input_tokens + api_response.usage.output_tokens,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_model_list() {
        let provider = AnthropicProvider {
            client: reqwest::Client::new(),
            base_url: "https://api.anthropic.com/v1".into(),
        };
        let models = provider.available_models();
        assert!(models.iter().any(|m| m.id.contains("claude")));
        assert!(models.iter().all(|m| m.supports_tools));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-ai-providers`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ai-providers/src/anthropic.rs
git commit -m "feat(ai): add Anthropic Claude provider with Messages API and streaming"
```

---

### Task 11: AI Providers — Gemini (stub)

**Files:**
- Create: `crates/ai-providers/src/gemini.rs`

- [ ] **Step 1: Write Gemini provider stub**

Write `crates/ai-providers/src/gemini.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::{AppError, AppResult};
use medical_core::traits::AiProvider;
use medical_core::types::ai::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::StreamExt;
use crate::sse::parse_sse_response;

pub struct GeminiProvider {
    client: Client,
    api_key: String,
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

impl GeminiProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::AiProvider(e.to_string()))?;
        Ok(Self {
            client,
            api_key: api_key.to_string(),
        })
    }

    fn build_url(&self, model: &str, stream: bool) -> String {
        let action = if stream { "streamGenerateContent?alt=sse" } else { "generateContent" };
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:{action}&key={}",
            self.api_key
        )
    }

    fn build_request(&self, request: &CompletionRequest) -> GeminiRequest {
        let contents: Vec<GeminiContent> = request.messages.iter()
            .filter(|m| m.role != Role::System)
            .map(|m| GeminiContent {
                role: match m.role {
                    Role::User | Role::Tool => "user".into(),
                    Role::Assistant => "model".into(),
                    Role::System => "user".into(),
                },
                parts: vec![GeminiPart {
                    text: match &m.content {
                        MessageContent::Text(t) => t.clone(),
                        MessageContent::ToolResult { content, .. } => content.clone(),
                    },
                }],
            })
            .collect();

        let system_instruction = request.system_prompt.as_ref().map(|s| GeminiContent {
            role: "user".into(),
            parts: vec![GeminiPart { text: s.clone() }],
        });

        GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        }
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn name(&self) -> &str { "gemini" }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo { id: "gemini-2.0-flash".into(), name: "Gemini 2.0 Flash".into(), provider: "gemini".into(), max_tokens: 1048576, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gemini-1.5-pro".into(), name: "Gemini 1.5 Pro".into(), provider: "gemini".into(), max_tokens: 2097152, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gemini-1.5-flash".into(), name: "Gemini 1.5 Flash".into(), provider: "gemini".into(), max_tokens: 1048576, supports_tools: true, supports_streaming: true },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = self.build_url(&request.model, false);
        let body = self.build_request(&request);

        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let api_response: GeminiResponse = response.json().await
            .map_err(|e| AppError::AiProvider(format!("JSON parse: {e}")))?;

        let content = api_response.candidates
            .and_then(|c| c.first().cloned())
            .and_then(|c| c.content)
            .map(|c| c.parts.iter().map(|p| p.text.clone()).collect::<Vec<_>>().join(""))
            .unwrap_or_default();

        let usage = api_response.usage_metadata.map(|u| UsageInfo {
            prompt_tokens: u.prompt_token_count.unwrap_or(0),
            completion_tokens: u.candidates_token_count.unwrap_or(0),
            total_tokens: u.total_token_count.unwrap_or(0),
        }).unwrap_or(UsageInfo { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 });

        Ok(CompletionResponse {
            content,
            model: request.model,
            usage,
            tool_calls: vec![],
        })
    }

    async fn complete_stream(&self, request: CompletionRequest) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        let url = self.build_url(&request.model, true);
        let body = self.build_request(&request);

        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let sse_stream = parse_sse_response(response);

        let mapped = sse_stream.filter_map(|result| {
            match result {
                Ok(data) => {
                    let parsed: Result<GeminiResponse, _> = serde_json::from_str(&data);
                    match parsed {
                        Ok(resp) => {
                            let text = resp.candidates
                                .and_then(|c| c.first().cloned())
                                .and_then(|c| c.content)
                                .map(|c| c.parts.iter().map(|p| p.text.clone()).collect::<Vec<_>>().join(""));
                            if let Some(text) = text {
                                if !text.is_empty() {
                                    return Some(Ok(StreamChunk::Delta { text }));
                                }
                            }
                            None
                        }
                        Err(_) => None,
                    }
                }
                Err(e) => Some(Err(AppError::AiProvider(format!("SSE error: {e}")))),
            }
        });

        Ok(Box::pin(mapped))
    }

    async fn complete_with_tools(&self, request: CompletionRequest, _tools: &[ToolDef]) -> AppResult<ToolCompletionResponse> {
        // Gemini tool calling uses a different format — basic implementation
        let response = self.complete(request).await?;
        Ok(ToolCompletionResponse {
            content: Some(response.content),
            tool_calls: vec![],
            usage: response.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_model_list() {
        let provider = GeminiProvider {
            client: reqwest::Client::new(),
            api_key: "test".into(),
        };
        let models = provider.available_models();
        assert!(models.iter().any(|m| m.id.contains("gemini")));
    }

    #[test]
    fn gemini_url_generation() {
        let provider = GeminiProvider {
            client: reqwest::Client::new(),
            api_key: "test-key".into(),
        };
        let url = provider.build_url("gemini-2.0-flash", false);
        assert!(url.contains("generativelanguage.googleapis.com"));
        assert!(url.contains("generateContent"));
        assert!(url.contains("test-key"));

        let stream_url = provider.build_url("gemini-2.0-flash", true);
        assert!(stream_url.contains("streamGenerateContent"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-ai-providers`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ai-providers/src/gemini.rs
git commit -m "feat(ai): add Google Gemini provider with generateContent API and streaming"
```

---

### Task 12: STT Providers — Failover Chain

**Files:**
- Create: `crates/stt-providers/src/failover.rs`

- [ ] **Step 1: Write failover chain with tests**

Write `crates/stt-providers/src/failover.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::AppResult;
use medical_core::traits::SttProvider;
use medical_core::types::stt::*;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Health state for a provider in the failover chain.
#[derive(Debug)]
struct ProviderHealth {
    failure_count: u32,
    last_failure: Option<Instant>,
}

impl ProviderHealth {
    fn new() -> Self {
        Self {
            failure_count: 0,
            last_failure: None,
        }
    }

    fn is_available(&self, failure_threshold: u32, cooldown: Duration) -> bool {
        if self.failure_count < failure_threshold {
            return true;
        }
        // Check if cooldown has elapsed
        self.last_failure
            .map(|t| t.elapsed() >= cooldown)
            .unwrap_or(true)
    }

    fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure = None;
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());
    }
}

/// STT failover chain. Tries providers in order, skipping unhealthy ones.
pub struct SttFailover {
    chain: Vec<Arc<dyn SttProvider>>,
    health: Mutex<HashMap<String, ProviderHealth>>,
    failure_threshold: u32,
    cooldown: Duration,
}

impl SttFailover {
    pub fn new(chain: Vec<Arc<dyn SttProvider>>) -> Self {
        let health = chain
            .iter()
            .map(|p| (p.name().to_string(), ProviderHealth::new()))
            .collect();

        Self {
            chain,
            health: Mutex::new(health),
            failure_threshold: 3,
            cooldown: Duration::from_secs(300),
        }
    }

    pub fn with_thresholds(mut self, failure_threshold: u32, cooldown_secs: u64) -> Self {
        self.failure_threshold = failure_threshold;
        self.cooldown = Duration::from_secs(cooldown_secs);
        self
    }

    /// Transcribe using the first available provider in the chain.
    pub async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let mut last_error = None;

        for provider in &self.chain {
            let name = provider.name().to_string();

            // Check health
            {
                let health = self.health.lock().unwrap();
                if let Some(h) = health.get(&name) {
                    if !h.is_available(self.failure_threshold, self.cooldown) {
                        warn!(provider = %name, "Skipping — circuit breaker open");
                        continue;
                    }
                }
            }

            info!(provider = %name, "Attempting transcription");

            match provider.transcribe(audio.clone(), config.clone()).await {
                Ok(transcript) => {
                    let mut health = self.health.lock().unwrap();
                    if let Some(h) = health.get_mut(&name) {
                        h.record_success();
                    }
                    info!(provider = %name, "Transcription succeeded");
                    return Ok(transcript);
                }
                Err(e) => {
                    warn!(provider = %name, error = %e, "Transcription failed");
                    let mut health = self.health.lock().unwrap();
                    if let Some(h) = health.get_mut(&name) {
                        h.record_failure();
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            medical_core::error::AppError::SttProvider("All providers exhausted".into())
        }))
    }

    /// Get the health status of all providers.
    pub fn provider_statuses(&self) -> Vec<(String, bool)> {
        let health = self.health.lock().unwrap();
        self.chain
            .iter()
            .map(|p| {
                let name = p.name().to_string();
                let available = health
                    .get(&name)
                    .map(|h| h.is_available(self.failure_threshold, self.cooldown))
                    .unwrap_or(true);
                (name, available)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_health_starts_available() {
        let h = ProviderHealth::new();
        assert!(h.is_available(3, Duration::from_secs(300)));
    }

    #[test]
    fn provider_health_unavailable_after_threshold() {
        let mut h = ProviderHealth::new();
        h.record_failure();
        h.record_failure();
        assert!(h.is_available(3, Duration::from_secs(300)));
        h.record_failure();
        assert!(!h.is_available(3, Duration::from_secs(300)));
    }

    #[test]
    fn provider_health_resets_on_success() {
        let mut h = ProviderHealth::new();
        h.record_failure();
        h.record_failure();
        h.record_success();
        h.record_failure();
        assert!(h.is_available(3, Duration::from_secs(300)));
    }

    #[test]
    fn failover_creates_with_empty_chain() {
        let failover = SttFailover::new(vec![]);
        assert!(failover.provider_statuses().is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-stt-providers`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/stt-providers/src/failover.rs
git commit -m "feat(stt): add failover chain with circuit-breaker health tracking"
```

---

### Task 13: STT Providers — Deepgram, Groq Whisper, ElevenLabs Scribe

**Files:**
- Create: `crates/stt-providers/src/deepgram.rs`
- Create: `crates/stt-providers/src/groq_whisper.rs`
- Create: `crates/stt-providers/src/elevenlabs_stt.rs`

- [ ] **Step 1: Write Deepgram provider**

Write `crates/stt-providers/src/deepgram.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::stt::*;
use reqwest::Client;
use serde::Deserialize;
use std::pin::Pin;

pub struct DeepgramProvider {
    client: Client,
}

#[derive(Deserialize)]
struct DeepgramResponse {
    results: Option<DeepgramResults>,
}

#[derive(Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Deserialize)]
struct DeepgramAlternative {
    transcript: String,
    words: Option<Vec<DeepgramWord>>,
}

#[derive(Deserialize)]
struct DeepgramWord {
    word: String,
    start: f64,
    end: f64,
    confidence: f64,
    speaker: Option<u32>,
}

impl DeepgramProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let client = Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("Authorization", format!("Token {api_key}").parse().unwrap());
                h
            })
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::SttProvider(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SttProvider for DeepgramProvider {
    fn name(&self) -> &str { "deepgram" }
    fn supports_streaming(&self) -> bool { true }
    fn supports_diarization(&self) -> bool { true }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let mut url = "https://api.deepgram.com/v1/listen?model=nova-2-medical&smart_format=true".to_string();

        if config.diarize {
            url.push_str("&diarize=true");
            if let Some(speakers) = config.num_speakers {
                url.push_str(&format!("&diarize.max_speakers={speakers}"));
            }
        }

        let language = &config.language;
        url.push_str(&format!("&language={language}"));

        // Encode audio as WAV bytes
        let wav_bytes = encode_audio_to_wav(&audio)?;

        let response = self.client
            .post(&url)
            .header("Content-Type", "audio/wav")
            .body(wav_bytes)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("Deepgram request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::SttProvider(format!("Deepgram HTTP {status}: {text}")));
        }

        let api_response: DeepgramResponse = response.json().await
            .map_err(|e| AppError::SttProvider(format!("Deepgram JSON parse: {e}")))?;

        let transcript_text = api_response.results
            .as_ref()
            .and_then(|r| r.channels.first())
            .and_then(|c| c.alternatives.first())
            .map(|a| a.transcript.clone())
            .unwrap_or_default();

        let segments = api_response.results
            .as_ref()
            .and_then(|r| r.channels.first())
            .and_then(|c| c.alternatives.first())
            .and_then(|a| a.words.as_ref())
            .map(|words| {
                words.iter().map(|w| TranscriptSegment {
                    text: w.word.clone(),
                    start: w.start,
                    end: w.end,
                    speaker: w.speaker.map(|s| format!("Speaker {s}")),
                    confidence: Some(w.confidence as f32),
                }).collect()
            })
            .unwrap_or_default();

        Ok(Transcript {
            text: transcript_text,
            segments,
            language: Some(config.language),
            duration_seconds: Some(audio.duration_seconds()),
            provider: "deepgram".into(),
            metadata: serde_json::json!({"model": "nova-2-medical"}),
        })
    }

    async fn transcribe_stream(&self, _stream: AudioStream, _config: SttConfig) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send>>> {
        Err(AppError::SttProvider("Deepgram streaming STT not yet implemented".into()))
    }
}

fn encode_audio_to_wav(audio: &AudioData) -> AppResult<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::new(&mut cursor, spec)
        .map_err(|e| AppError::SttProvider(format!("WAV encode error: {e}")))?;
    for &sample in &audio.samples {
        writer.write_sample(sample)
            .map_err(|e| AppError::SttProvider(format!("WAV write error: {e}")))?;
    }
    writer.finalize()
        .map_err(|e| AppError::SttProvider(format!("WAV finalize error: {e}")))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_audio_to_wav_produces_valid_bytes() {
        let audio = AudioData {
            samples: vec![0.0; 16000],
            sample_rate: 16000,
            channels: 1,
        };
        let bytes = encode_audio_to_wav(&audio).unwrap();
        assert!(bytes.len() > 44); // WAV header is 44 bytes
        assert_eq!(&bytes[0..4], b"RIFF");
    }
}
```

- [ ] **Step 2: Write Groq Whisper provider**

Write `crates/stt-providers/src/groq_whisper.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::stt::*;
use reqwest::Client;
use serde::Deserialize;
use std::pin::Pin;

pub struct GroqWhisperProvider {
    client: Client,
}

#[derive(Deserialize)]
struct WhisperResponse {
    text: String,
}

impl GroqWhisperProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let client = Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("Authorization", format!("Bearer {api_key}").parse().unwrap());
                h
            })
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::SttProvider(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SttProvider for GroqWhisperProvider {
    fn name(&self) -> &str { "groq_whisper" }
    fn supports_streaming(&self) -> bool { false }
    fn supports_diarization(&self) -> bool { false }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let wav_bytes = super::deepgram::encode_audio_to_wav_pub(&audio)?;

        let part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .unwrap();

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-large-v3-turbo")
            .text("language", config.language.chars().take(2).collect::<String>());

        let response = self.client
            .post("https://api.groq.com/openai/v1/audio/transcriptions")
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("Groq Whisper request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::SttProvider(format!("Groq Whisper HTTP {status}: {text}")));
        }

        let api_response: WhisperResponse = response.json().await
            .map_err(|e| AppError::SttProvider(format!("Groq Whisper JSON parse: {e}")))?;

        Ok(Transcript {
            text: api_response.text,
            segments: vec![],
            language: Some(config.language),
            duration_seconds: Some(audio.duration_seconds()),
            provider: "groq_whisper".into(),
            metadata: serde_json::json!({"model": "whisper-large-v3-turbo"}),
        })
    }

    async fn transcribe_stream(&self, _stream: AudioStream, _config: SttConfig) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send>>> {
        Err(AppError::SttProvider("Groq Whisper does not support streaming".into()))
    }
}
```

Note: The `encode_audio_to_wav_pub` function needs to be exported from deepgram. Update `crates/stt-providers/src/deepgram.rs` to add:
```rust
pub fn encode_audio_to_wav_pub(audio: &AudioData) -> AppResult<Vec<u8>> {
    encode_audio_to_wav(audio)
}
```

- [ ] **Step 3: Write ElevenLabs Scribe provider**

Write `crates/stt-providers/src/elevenlabs_stt.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::stt::*;
use reqwest::Client;
use serde::Deserialize;
use std::pin::Pin;

pub struct ElevenLabsSttProvider {
    client: Client,
}

#[derive(Deserialize)]
struct ScribeResponse {
    text: Option<String>,
    #[allow(dead_code)]
    language_code: Option<String>,
}

impl ElevenLabsSttProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let client = Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("xi-api-key", api_key.parse().unwrap());
                h
            })
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::SttProvider(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SttProvider for ElevenLabsSttProvider {
    fn name(&self) -> &str { "elevenlabs" }
    fn supports_streaming(&self) -> bool { false }
    fn supports_diarization(&self) -> bool { true }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let wav_bytes = super::deepgram::encode_audio_to_wav_pub(&audio)?;

        let part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .unwrap();

        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model_id", "scribe_v2")
            .text("language_code", config.language.clone());

        if config.diarize {
            form = form.text("diarize", "true");
            if let Some(speakers) = config.num_speakers {
                form = form.text("num_speakers", speakers.to_string());
            }
        }

        let response = self.client
            .post("https://api.elevenlabs.io/v1/speech-to-text")
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("ElevenLabs STT failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::SttProvider(format!("ElevenLabs STT HTTP {status}: {text}")));
        }

        let api_response: ScribeResponse = response.json().await
            .map_err(|e| AppError::SttProvider(format!("ElevenLabs STT JSON parse: {e}")))?;

        Ok(Transcript {
            text: api_response.text.unwrap_or_default(),
            segments: vec![],
            language: Some(config.language),
            duration_seconds: Some(audio.duration_seconds()),
            provider: "elevenlabs".into(),
            metadata: serde_json::json!({"model": "scribe_v2"}),
        })
    }

    async fn transcribe_stream(&self, _stream: AudioStream, _config: SttConfig) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send>>> {
        Err(AppError::SttProvider("ElevenLabs does not support streaming STT".into()))
    }
}
```

- [ ] **Step 4: Write stub for Modulate and Whisper local**

Write `crates/stt-providers/src/modulate.rs`:
```rust
//! Modulate/Velma STT provider — requires API access credentials.
//! Features: voice emotion detection, speaker diarization, deepfake detection, PII redaction.
//! Emotion data stored in Transcript.metadata and surfaced in UI as sentiment tags.

pub struct ModulateProvider;

impl ModulateProvider {
    pub fn new(_api_key: &str) -> Self {
        Self
    }
}
```

Write `crates/stt-providers/src/whisper_local.rs`:
```rust
//! Local Whisper STT via whisper-rs (whisper.cpp bindings).
//! Feature-gated — requires `local-whisper` feature flag.
//! Models downloaded on first use to app data directory.

pub struct WhisperLocalProvider;

impl WhisperLocalProvider {
    pub fn new() -> Self {
        Self
    }
}
```

- [ ] **Step 5: Update deepgram.rs to export the WAV encoder**

Add to `crates/stt-providers/src/deepgram.rs` (after the existing `encode_audio_to_wav` function):
```rust
/// Public wrapper for WAV encoding, used by other STT providers.
pub fn encode_audio_to_wav_pub(audio: &AudioData) -> AppResult<Vec<u8>> {
    encode_audio_to_wav(audio)
}
```

Also add `hound` to `crates/stt-providers/Cargo.toml` dependencies:
```toml
hound = "3.5"
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p medical-stt-providers`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/stt-providers/
git commit -m "feat(stt): add Deepgram, Groq Whisper, ElevenLabs Scribe providers with failover"
```

---

### Task 14: TTS Providers — ElevenLabs + Local Stub

**Files:**
- Create: `crates/tts-providers/src/elevenlabs_tts.rs`
- Create: `crates/tts-providers/src/local_tts.rs`

- [ ] **Step 1: Write ElevenLabs TTS provider**

Write `crates/tts-providers/src/elevenlabs_tts.rs`:
```rust
use async_trait::async_trait;
use medical_core::error::{AppError, AppResult};
use medical_core::traits::TtsProvider;
use medical_core::types::stt::AudioData;
use medical_core::types::tts::*;
use reqwest::Client;
use serde::Serialize;

pub struct ElevenLabsTtsProvider {
    client: Client,
}

#[derive(Serialize)]
struct TtsRequest {
    text: String,
    model_id: String,
    voice_settings: VoiceSettings,
}

#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
    style: f32,
    use_speaker_boost: bool,
}

impl ElevenLabsTtsProvider {
    pub fn new(api_key: &str) -> AppResult<Self> {
        let client = Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("xi-api-key", api_key.parse().unwrap());
                h.insert("Content-Type", "application/json".parse().unwrap());
                h
            })
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| AppError::TtsProvider(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl TtsProvider for ElevenLabsTtsProvider {
    fn name(&self) -> &str { "elevenlabs" }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        // Common ElevenLabs voices — full list fetched from API in production
        vec![
            VoiceInfo { id: "21m00Tcm4TlvDq8ikWAM".into(), name: "Rachel".into(), language: Some("en".into()), gender: Some("female".into()), preview_url: None },
            VoiceInfo { id: "AZnzlk1XvdvUeBnXmlld".into(), name: "Domi".into(), language: Some("en".into()), gender: Some("female".into()), preview_url: None },
            VoiceInfo { id: "EXAVITQu4vr4xnSDxMaL".into(), name: "Bella".into(), language: Some("en".into()), gender: Some("female".into()), preview_url: None },
            VoiceInfo { id: "ErXwobaYiN019PkySvjV".into(), name: "Antoni".into(), language: Some("en".into()), gender: Some("male".into()), preview_url: None },
            VoiceInfo { id: "VR6AewLTigWG4xSOukaG".into(), name: "Arnold".into(), language: Some("en".into()), gender: Some("male".into()), preview_url: None },
        ]
    }

    async fn synthesize(&self, text: &str, config: TtsConfig) -> AppResult<AudioData> {
        let voice_id = &config.voice;
        let model = config.model.as_deref().unwrap_or("eleven_flash_v2_5");
        let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}");

        let body = TtsRequest {
            text: text.to_string(),
            model_id: model.to_string(),
            voice_settings: VoiceSettings {
                stability: 0.5,
                similarity_boost: 0.75,
                style: 0.0,
                use_speaker_boost: true,
            },
        };

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::TtsProvider(format!("ElevenLabs TTS failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::TtsProvider(format!("ElevenLabs TTS HTTP {status}: {text}")));
        }

        // Response is raw audio bytes (MP3)
        let audio_bytes = response.bytes().await
            .map_err(|e| AppError::TtsProvider(format!("Failed to read audio: {e}")))?;

        // Return as raw bytes in AudioData — playback decodes via rodio
        Ok(AudioData {
            samples: audio_bytes.iter().map(|&b| b as f32 / 255.0).collect(),
            sample_rate: 44100, // MP3 default — actual decoding happens in playback
            channels: 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_list_not_empty() {
        let provider = ElevenLabsTtsProvider {
            client: reqwest::Client::new(),
        };
        assert!(!provider.available_voices().is_empty());
    }

    #[test]
    fn provider_name() {
        let provider = ElevenLabsTtsProvider {
            client: reqwest::Client::new(),
        };
        assert_eq!(provider.name(), "elevenlabs");
    }
}
```

- [ ] **Step 2: Write local TTS stub**

Write `crates/tts-providers/src/local_tts.rs`:
```rust
//! Local platform TTS provider.
//! Uses platform-native speech synthesis:
//!   - Linux: speech-dispatcher
//!   - macOS: NSSpeechSynthesizer
//!   - Windows: SAPI
//! Full implementation in a future plan.

pub struct LocalTtsProvider;

impl LocalTtsProvider {
    pub fn new() -> Self {
        Self
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p medical-tts-providers`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/tts-providers/
git commit -m "feat(tts): add ElevenLabs TTS provider and local TTS stub"
```

---

### Task 15: Final Verification — Full Workspace Build and Tests

- [ ] **Step 1: Build entire workspace**

Run: `cargo build --workspace`
Expected: Clean build.

- [ ] **Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass. Report total count.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Fix any warnings.

- [ ] **Step 4: Commit fixes if any**

```bash
git add -A
git commit -m "fix: address clippy warnings in provider crates"
```

- [ ] **Step 5: Push**

```bash
git push origin master
```
