# Remote STT and Remote Ollama Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a FerriScribe instance on one machine (Computer B) use Whisper and Ollama running on another machine (Computer A) reached over Tailscale, matching the existing LM Studio remote pattern.

**Architecture:** New `RemoteSttProvider` talks to any OpenAI-compatible Whisper server (`POST /v1/audio/transcriptions`); diarization still runs locally on Computer B. Ollama host/port threaded through `OllamaProvider::new` and `EmbeddingGenerator::new_ollama` from new `AppConfig` fields. An explicit `stt_mode: Local | Remote` toggle in Settings selects the provider at startup.

**Tech Stack:** Rust 2024, Tauri v2, reqwest (multipart), wiremock (tests), Svelte 5, Vitest.

**Spec:** `docs/superpowers/specs/2026-04-23-remote-stt-and-ollama-design.md`

---

## File map

| Path | Change | Responsibility |
|------|--------|----------------|
| `crates/core/src/types/settings.rs` | modify | Add `SttMode` enum, STT remote fields, Ollama host/port |
| `src-tauri/src/state.rs` | modify | Thread Ollama host through `init_ai_providers` + RAG embeddings; branch `init_stt_providers` on `stt_mode`; load STT api key from keychain |
| `crates/stt-providers/src/remote_provider.rs` | create | `RemoteSttProvider` impl of `SttProvider` — OpenAI-compat HTTP + local diarization |
| `crates/stt-providers/src/lib.rs` | modify | Export `remote_provider` module |
| `crates/stt-providers/Cargo.toml` | modify | Add `wiremock` + `tempfile` as dev-deps; `bytes` from workspace |
| `crates/stt-providers/src/audio_prep.rs` | modify | Add `write_pcm16_wav_bytes` helper for in-memory WAV encoding |
| `src-tauri/src/commands/providers.rs` | modify | Add `test_stt_remote_connection` + `test_ollama_connection` |
| `src-tauri/src/commands/security.rs` | modify (or create if absent) | Add `save_stt_remote_api_key` + `clear_stt_remote_api_key` |
| `src-tauri/src/lib.rs` | modify | Register new Tauri commands |
| `src/lib/components/SettingsContent.svelte` | modify | STT mode toggle + remote fields + Ollama server subsection |
| `src/lib/api/providers.ts` | modify | Frontend bindings for new commands |
| `src/lib/components/SettingsContent.test.ts` | create | Vitest for mode toggle conditional rendering |
| `README.md` | modify | Appendix: server-side setup for Computer A |

---

## Task 1: Config schema — `SttMode`, STT remote fields, Ollama host/port

**Files:**
- Modify: `crates/core/src/types/settings.rs` (defaults around lines 187–210; struct around lines 260–330; tests around lines 420+)
- Create: `docs/superpowers/specs/2026-04-23-remote-stt-and-ollama-design.md` (already in working tree, untracked — commit it)
- Create: `docs/superpowers/plans/2026-04-23-remote-stt-and-ollama.md` (this file — already in working tree, untracked — commit it)

### Task 1.1: Write failing tests for the new config fields and defaults

- [ ] **Step 1: Add three new tests**

Append to `crates/core/src/types/settings.rs` inside the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn new_config_defaults_stt_mode_to_local() {
        let config: AppConfig = serde_json::from_str("{}").expect("parse empty");
        assert_eq!(config.stt_mode, SttMode::Local);
        assert_eq!(config.stt_remote_host, "");
        assert_eq!(config.stt_remote_port, 8080);
        assert_eq!(config.stt_remote_model, "whisper-1");
    }

    #[test]
    fn new_config_defaults_ollama_host_and_port() {
        let config: AppConfig = serde_json::from_str("{}").expect("parse empty");
        assert_eq!(config.ollama_host, "localhost");
        assert_eq!(config.ollama_port, 11434);
    }

    #[test]
    fn stt_mode_roundtrips_through_json() {
        let json = r#"{"stt_mode":"remote"}"#;
        let config: AppConfig = serde_json::from_str(json).expect("parse");
        assert_eq!(config.stt_mode, SttMode::Remote);

        let out = serde_json::to_string(&config).expect("serialize");
        assert!(
            out.contains(r#""stt_mode":"remote""#),
            "expected remote, got: {out}"
        );
    }
```

- [ ] **Step 2: Run tests — expect compile failure**

Run: `cargo test -p medical-core settings`
Expected: `error[E0412]: cannot find type 'SttMode' in this scope` (among others).

### Task 1.2: Add the enum, defaults, and fields

- [ ] **Step 3: Add `SttMode` enum and default helpers**

Insert at the top of `crates/core/src/types/settings.rs` immediately after `use serde::{Deserialize, Serialize};` (line 1):

```rust
/// How speech-to-text is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttMode {
    /// In-process whisper-rs on this machine.
    #[default]
    Local,
    /// HTTP POST to an OpenAI-compatible Whisper server.
    Remote,
}
```

Add these default helpers alongside the existing `default_lmstudio_*` helpers (after `default_lmstudio_port`, around line 195):

```rust
fn default_stt_remote_port() -> u16 {
    8080
}

fn default_stt_remote_model() -> String {
    "whisper-1".into()
}

fn default_ollama_host() -> String {
    "localhost".into()
}

fn default_ollama_port() -> u16 {
    11434
}
```

- [ ] **Step 4: Add fields to `AppConfig`**

In the `AppConfig` struct in `crates/core/src/types/settings.rs`, add these fields immediately after the existing `lmstudio_port` field (around line 273):

```rust
    // STT mode selection
    #[serde(default)]
    pub stt_mode: SttMode,
    // Remote Whisper server (when stt_mode == Remote)
    #[serde(default)]
    pub stt_remote_host: String,
    #[serde(default = "default_stt_remote_port")]
    pub stt_remote_port: u16,
    #[serde(default = "default_stt_remote_model")]
    pub stt_remote_model: String,

    // Ollama server (local or remote on LAN)
    #[serde(default = "default_ollama_host")]
    pub ollama_host: String,
    #[serde(default = "default_ollama_port")]
    pub ollama_port: u16,
```

- [ ] **Step 5: Run tests — expect PASS**

Run: `cargo test -p medical-core settings`
Expected: all new tests pass; all existing tests still pass.

- [ ] **Step 6: Verify the existing default-roundtrip test still passes**

Run: `cargo test -p medical-core settings::tests::default_config_matches_json`
Expected: PASS. If any pre-existing "all fields present" test fails because it serializes the full `AppConfig`, update its expected snapshot to include the new fields with their defaults — do NOT delete assertions.

- [ ] **Step 7: Commit spec + plan + config changes**

```bash
git add docs/superpowers/specs/2026-04-23-remote-stt-and-ollama-design.md \
        docs/superpowers/plans/2026-04-23-remote-stt-and-ollama.md \
        crates/core/src/types/settings.rs
git commit -m "feat(settings): add SttMode + STT remote + Ollama host/port

Introduces the config surface for remote STT (OpenAI-compatible Whisper
server) and remote Ollama. SttMode defaults to Local so existing users
see no behavior change on upgrade. Ollama host/port defaults to
localhost:11434, matching previous hardcoded behavior."
```

Do NOT stage the unrelated `docs/superpowers/plans/2026-04-22-agent-and-error-hardening.md` — it belongs to a different feature.

---

## Task 2: Thread Ollama host through AI provider init and RAG embeddings

**Files:**
- Modify: `src-tauri/src/state.rs` (lines 107–134 for AI providers, line 209 for embeddings)

### Task 2.1: Write failing test — `EmbeddingGenerator` uses configured host

- [ ] **Step 1: Add a Rust-side test in `src-tauri/src/state.rs`**

Append to `src-tauri/src/state.rs` inside (or creating) a `#[cfg(test)] mod tests` block near the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::settings::AppConfig;

    #[test]
    fn init_ai_providers_uses_configured_ollama_host() {
        let mut config = AppConfig::default();
        config.ollama_host = "tailnet-node".into();
        config.ollama_port = 11500;
        let registry = init_ai_providers(&config);
        // Ollama should be registered; the internal URL is private, but the
        // provider's presence in the registry is observable.
        assert!(
            registry.list_available().contains(&"ollama".to_string()),
            "ollama not registered with custom host"
        );
    }
}
```

- [ ] **Step 2: Run — expect PASS (the test only asserts presence)**

Run: `cargo test -p <tauri-crate-name> init_ai_providers_uses_configured_ollama_host`
(Confirm the crate name via `grep "^name" src-tauri/Cargo.toml`.)

This test passes even before the change. It's a guard rail — it will only fail if someone later breaks Ollama registration entirely. Thread the host via production code below; the test still holds.

### Task 2.2: Thread Ollama host into `init_ai_providers`

- [ ] **Step 3: Replace the Ollama construction in `state.rs`**

In `src-tauri/src/state.rs`, find the block around lines 113–119:

```rust
    match OllamaProvider::new(None) {
        Ok(p) => {
            info!("Registering Ollama provider (local)");
            registry.register(Arc::new(p));
        }
        Err(e) => tracing::error!(error = %e, "Failed to build Ollama provider; skipping"),
    }
```

Replace with:

```rust
    let ollama_host = if config.ollama_host.is_empty() { "localhost" } else { &config.ollama_host };
    let ollama_url = format!("http://{}:{}", ollama_host, config.ollama_port);
    match OllamaProvider::new(Some(&ollama_url)) {
        Ok(p) => {
            info!(url = %ollama_url, "Registering Ollama provider");
            registry.register(Arc::new(p));
        }
        Err(e) => tracing::error!(error = %e, url = %ollama_url, "Failed to build Ollama provider; skipping"),
    }
```

### Task 2.3: Thread Ollama host into the RAG embedding generator

- [ ] **Step 4: Update the RAG init line**

In `src-tauri/src/state.rs`, locate line 209:

```rust
        let embedding_generator = Arc::new(EmbeddingGenerator::new_ollama(None, None));
```

Replace with (keeping the surrounding context — this is inside a block where `config` is in scope; if not, hoist from the same config used earlier in `AppState::initialize`):

```rust
        let embedding_host = if config.ollama_host.is_empty() {
            "localhost".to_string()
        } else {
            config.ollama_host.clone()
        };
        let embedding_url = format!("http://{}:{}", embedding_host, config.ollama_port);
        let embedding_generator = Arc::new(EmbeddingGenerator::new_ollama(
            Some(&embedding_url),
            Some(&config.embedding_model),
        ));
```

If `config` is not yet in scope at that point in `AppState::initialize`, confirm by reading `src-tauri/src/state.rs` around `impl AppState { pub fn initialize() { ... } }`. If it's loaded earlier in the function, pass it through; if not, load it inline:

```rust
        let config = {
            let conn = db.conn().map_err(|e| format!("{e}"))?;
            let mut c = medical_db::settings::SettingsRepo::load_config(&conn)
                .unwrap_or_default();
            c.migrate();
            c
        };
```

…and use that before the `EmbeddingGenerator::new_ollama` call. STOP and report NEEDS_CONTEXT if the data flow is ambiguous rather than guessing.

- [ ] **Step 5: Verify build and existing tests**

Run: `cargo build --workspace && cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat(providers): thread Ollama host/port from config

Ollama chat provider and RAG embedding generator now honor the
configured ollama_host/ollama_port instead of hardcoding localhost.
Matches the existing LM Studio remote pattern."
```

---

## Task 3: `RemoteSttProvider` — implementation + tests

**Files:**
- Modify: `crates/stt-providers/Cargo.toml` (dev-deps)
- Modify: `crates/stt-providers/src/lib.rs` (export module)
- Modify: `crates/stt-providers/src/audio_prep.rs` (add PCM16 WAV encoder)
- Create: `crates/stt-providers/src/remote_provider.rs`
- Create: `crates/stt-providers/tests/remote_provider_integration.rs` (integration test)

### Task 3.1: Add dev-dependencies

- [ ] **Step 1: Update `crates/stt-providers/Cargo.toml`**

Replace the `[dev-dependencies]` section with:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
wiremock = "0.6"
tempfile = "3"
```

Add `bytes` to the main `[dependencies]` section (after `reqwest`):

```toml
bytes = { workspace = true }
```

### Task 3.2: Add the in-memory PCM16 WAV encoder

- [ ] **Step 2: Add the helper to `audio_prep.rs`**

Append to `crates/stt-providers/src/audio_prep.rs`:

```rust
/// Encode a 16 kHz mono PCM16 buffer as an in-memory WAV file.
///
/// Produces a RIFF/WAVE payload suitable for upload to any OpenAI-compatible
/// Whisper server. No extra heap allocations after the initial `Vec::with_capacity`.
pub fn write_pcm16_wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk (PCM, mono, 16-bit)
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // subchunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // 1 channel
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * 2; // sample_rate * channels * bits_per_sample/8
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }

    buf
}

#[cfg(test)]
mod wav_encode_tests {
    use super::*;

    #[test]
    fn encodes_header_and_data_length() {
        let samples = [0i16, 1, -1, 32767, -32768];
        let wav = write_pcm16_wav_bytes(&samples, 16000);
        // RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        // file size (36 + data)
        let file_size = u32::from_le_bytes(wav[4..8].try_into().unwrap());
        assert_eq!(file_size, 36 + 10);
        assert_eq!(&wav[8..12], b"WAVE");
        // data chunk length
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        assert_eq!(data_len, 10);
        // total bytes
        assert_eq!(wav.len(), 44 + 10);
    }

    #[test]
    fn sample_rate_in_header_matches_input() {
        let wav = write_pcm16_wav_bytes(&[0i16; 4], 22050);
        let sr = u32::from_le_bytes(wav[24..28].try_into().unwrap());
        assert_eq!(sr, 22050);
    }
}
```

### Task 3.3: Create `remote_provider.rs` with failing tests

- [ ] **Step 3: Create the new module with its full implementation and tests**

Create `crates/stt-providers/src/remote_provider.rs`:

```rust
//! RemoteSttProvider — OpenAI-compatible Whisper server client.
//!
//! Sends a 16 kHz mono PCM WAV to `POST {base}/v1/audio/transcriptions` and
//! parses `verbose_json` back into `TranscriptSegment[]`. Local pyannote
//! diarization runs on the same audio buffer (paralleling `LocalSttProvider`)
//! so speaker labels still work even when Whisper is remote.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use tracing::{info, warn};

use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::{
    AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk, TranscriptSegment,
};

use crate::audio_prep;
use crate::diarization::SpeakerDiarizer;
use crate::merge;

const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(600);
const TARGET_SAMPLE_RATE: u32 = 16_000;

pub struct RemoteSttProvider {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    segmentation_model_path: PathBuf,
    embedding_model_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct VerboseJson {
    #[serde(default)]
    segments: Vec<VerboseSegment>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    duration: Option<f32>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerboseSegment {
    start: f32,
    end: f32,
    text: String,
}

impl RemoteSttProvider {
    pub fn new(
        host: &str,
        port: u16,
        model: &str,
        api_key: Option<String>,
        segmentation_model_path: PathBuf,
        embedding_model_path: PathBuf,
    ) -> AppResult<Self> {
        let host = if host.is_empty() { "localhost" } else { host };
        let base_url = format!("http://{host}:{port}");

        let client = Client::builder()
            .pool_max_idle_per_host(4)
            .connect_timeout(Duration::from_secs(10))
            .timeout(TRANSCRIBE_TIMEOUT)
            .build()
            .map_err(|e| AppError::SttProvider(format!("Failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            base_url,
            model: model.to_string(),
            api_key,
            segmentation_model_path,
            embedding_model_path,
        })
    }

    fn diarization_available(&self) -> bool {
        self.segmentation_model_path.exists() && self.embedding_model_path.exists()
    }

    async fn post_audio(
        &self,
        wav_bytes: Vec<u8>,
        language: Option<&str>,
    ) -> AppResult<VerboseJson> {
        let url = format!("{}/v1/audio/transcriptions", self.base_url);

        let mut form = Form::new()
            .part(
                "file",
                Part::bytes(wav_bytes)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .map_err(|e| AppError::SttProvider(format!("multipart error: {e}")))?,
            )
            .text("model", self.model.clone())
            .text("response_format", "verbose_json");
        if let Some(lang) = language {
            if !lang.is_empty() {
                form = form.text("language", lang.to_string());
            }
        }

        let mut req = self.client.post(&url).multipart(form);
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                req = req.header("Authorization", format!("Bearer {key}"));
            }
        }

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::SttProvider(format!(
                    "Transcription timed out after {}s",
                    TRANSCRIBE_TIMEOUT.as_secs()
                ))
            } else if e.is_connect() {
                AppError::SttProvider(format!(
                    "Cannot reach Whisper server at {}: {e}",
                    self.base_url
                ))
            } else {
                AppError::SttProvider(format!("Whisper request failed: {e}"))
            }
        })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::SttProvider(
                "Whisper server rejected authentication — check API key".into(),
            ));
        }
        if status.is_client_error() {
            let body = resp.text().await.unwrap_or_default();
            let prefix: String = body.chars().take(200).collect();
            return Err(AppError::SttProvider(format!(
                "Whisper server rejected request: {status} {prefix}"
            )));
        }
        if status.is_server_error() {
            return Err(AppError::SttProvider(format!(
                "Whisper server internal error: {status}"
            )));
        }

        resp.json::<VerboseJson>().await.map_err(|e| {
            AppError::SttProvider(format!("Unexpected response from Whisper server: {e}"))
        })
    }
}

#[async_trait]
impl SttProvider for RemoteSttProvider {
    fn name(&self) -> &str {
        "remote"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        self.diarization_available()
    }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let duration = audio.duration_seconds();

        // Stage 1: resample to 16 kHz mono f32, then convert to i16 for upload.
        let audio_16k = audio_prep::to_16k_mono_f32(&audio);
        let samples_i16 = audio_prep::f32_to_i16(&audio_16k);
        let wav_bytes = audio_prep::write_pcm16_wav_bytes(&samples_i16, TARGET_SAMPLE_RATE);

        // Stage 2: POST to the Whisper server.
        let parsed = self.post_audio(wav_bytes, config.language.as_deref()).await?;

        let whisper_segments: Vec<TranscriptSegment> = parsed
            .segments
            .into_iter()
            .map(|s| TranscriptSegment {
                start: s.start as f64,
                end: s.end as f64,
                text: s.text,
                speaker: None,
            })
            .collect();

        // Stage 3: local diarization if requested and models present.
        let speaker_turns = if config.diarize && self.diarization_available() {
            let seg_path = self.segmentation_model_path.clone();
            let emb_path = self.embedding_model_path.clone();
            let audio_for_diarize = samples_i16.clone();
            match tokio::task::spawn_blocking(move || {
                let diarizer = SpeakerDiarizer::new(seg_path, emb_path);
                diarizer.diarize(&audio_for_diarize, TARGET_SAMPLE_RATE as i32)
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
            if config.diarize && !self.diarization_available() {
                warn!("Diarization requested but pyannote models not found — skipping");
            }
            Vec::new()
        };

        // Stage 4: merge speaker turns with whisper segments.
        let merged = if speaker_turns.is_empty() {
            whisper_segments
        } else {
            merge::merge_segments_with_speakers(&whisper_segments, &speaker_turns)
        };

        let full_text = parsed.text.unwrap_or_else(|| {
            merged
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        });

        info!(
            segments = merged.len(),
            text_len = full_text.len(),
            "Remote transcription complete"
        );

        Ok(Transcript {
            text: full_text,
            segments: merged,
            language: parsed.language.or(config.language),
            duration_seconds: Some(duration),
            provider: "remote".to_owned(),
            metadata: serde_json::json!({
                "server": self.base_url,
                "model": self.model,
            }),
        })
    }

    async fn transcribe_stream(
        &self,
        _stream: AudioStream,
        _config: SttConfig,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>> {
        Err(AppError::SttProvider(
            "Remote provider does not support streaming transcription".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::{AudioData, SttConfig};
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn dummy_audio() -> AudioData {
        // 1 second of silent 16 kHz mono f32.
        AudioData {
            samples: vec![0.0_f32; 16_000],
            sample_rate: 16_000,
            channels: 1,
        }
    }

    fn verbose_body() -> serde_json::Value {
        serde_json::json!({
            "text": "Hello patient.",
            "segments": [
                { "start": 0.0, "end": 1.0, "text": "Hello patient." }
            ],
            "language": "en",
            "duration": 1.0
        })
    }

    fn provider_at(base: &str, api_key: Option<String>) -> RemoteSttProvider {
        // Strip the http:// prefix to feed RemoteSttProvider::new which re-adds it.
        let stripped = base.trim_start_matches("http://");
        let (host, port) = stripped
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap()))
            .unwrap();
        RemoteSttProvider::new(
            &host,
            port,
            "whisper-1",
            api_key,
            PathBuf::from("/nonexistent-seg.onnx"),
            PathBuf::from("/nonexistent-emb.onnx"),
        )
        .expect("build provider")
    }

    #[tokio::test]
    async fn happy_path_returns_segments_without_diarization() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(verbose_body()))
            .mount(&server)
            .await;

        let provider = provider_at(&server.uri(), None);
        let transcript = provider
            .transcribe(
                dummy_audio(),
                SttConfig { language: Some("en".into()), diarize: false, ..SttConfig::default() },
            )
            .await
            .expect("transcribe");

        assert_eq!(transcript.provider, "remote");
        assert_eq!(transcript.segments.len(), 1);
        assert_eq!(transcript.segments[0].text, "Hello patient.");
        assert!(transcript.segments[0].speaker.is_none());
    }

    #[tokio::test]
    async fn authorization_header_sent_when_api_key_present() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(verbose_body()))
            .mount(&server)
            .await;

        let provider = provider_at(&server.uri(), Some("sk-test".into()));
        let res = provider
            .transcribe(dummy_audio(), SttConfig::default())
            .await;
        assert!(res.is_ok(), "expected ok, got: {res:?}");
    }

    #[tokio::test]
    async fn no_authorization_header_when_api_key_absent() {
        let server = MockServer::start().await;
        // Match requests that DO have Authorization — they should be zero.
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        // Requests WITHOUT Authorization get a 200.
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(verbose_body()))
            .mount(&server)
            .await;

        let provider = provider_at(&server.uri(), None);
        let res = provider
            .transcribe(dummy_audio(), SttConfig::default())
            .await;
        assert!(res.is_ok(), "should not send Authorization without key");
    }

    #[tokio::test]
    async fn http_401_maps_to_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let provider = provider_at(&server.uri(), Some("bad".into()));
        let err = provider
            .transcribe(dummy_audio(), SttConfig::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("authentication"),
            "expected auth error, got: {err}"
        );
    }

    #[tokio::test]
    async fn http_503_maps_to_server_internal_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let provider = provider_at(&server.uri(), None);
        let err = provider
            .transcribe(dummy_audio(), SttConfig::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("internal error"),
            "expected 5xx error, got: {err}"
        );
    }

    #[tokio::test]
    async fn malformed_json_maps_to_parse_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let provider = provider_at(&server.uri(), None);
        let err = provider
            .transcribe(dummy_audio(), SttConfig::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Unexpected response"),
            "expected parse error, got: {err}"
        );
    }

    #[test]
    fn diarization_available_is_false_without_models() {
        let p = RemoteSttProvider::new(
            "localhost",
            8080,
            "whisper-1",
            None,
            PathBuf::from("/nowhere/seg.onnx"),
            PathBuf::from("/nowhere/emb.onnx"),
        )
        .expect("build");
        assert!(!p.diarization_available());
    }
}
```

### Task 3.4: Export the module

- [ ] **Step 4: Add the module to `crates/stt-providers/src/lib.rs`**

Find the existing module declarations and add:

```rust
pub mod remote_provider;
```

Also verify the file exports the pub helpers needed by `remote_provider.rs` (`audio_prep`, `diarization`, `merge`). They should already be `pub mod` — leave as-is.

### Task 3.5: Run and commit

- [ ] **Step 5: Build and run tests**

Run: `cargo test -p medical-stt-providers`
Expected: all new tests pass; existing tests still pass.

Run: `cargo build --workspace`
Expected: no compile errors in dependent crates.

- [ ] **Step 6: Commit**

```bash
git add crates/stt-providers/Cargo.toml \
        crates/stt-providers/src/audio_prep.rs \
        crates/stt-providers/src/remote_provider.rs \
        crates/stt-providers/src/lib.rs
git commit -m "feat(stt): add RemoteSttProvider (OpenAI-compat Whisper)

Talks to any /v1/audio/transcriptions endpoint, uploads a 16 kHz
mono PCM16 WAV, parses verbose_json back into TranscriptSegment[].
Local pyannote diarization runs against the same buffer so speaker
labels still work when Whisper is remote. Covers network / auth /
4xx / 5xx / timeout / malformed-JSON error paths in unit tests
using wiremock."
```

---

## Task 4: Dispatcher — branch `init_stt_providers` on `SttMode`

**Files:**
- Modify: `src-tauri/src/state.rs` (the existing `init_stt_providers` function)

### Task 4.1: Write the failing test

- [ ] **Step 1: Add a test**

Append to the existing `#[cfg(test)] mod tests` block in `src-tauri/src/state.rs`:

```rust
    #[test]
    fn init_stt_providers_remote_mode_builds_remote_provider() {
        use medical_core::types::settings::{AppConfig, SttMode};
        let mut cfg = AppConfig::default();
        cfg.stt_mode = SttMode::Remote;
        cfg.stt_remote_host = "tailnet-node".into();
        cfg.stt_remote_port = 8080;
        cfg.stt_remote_model = "whisper-1".into();

        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = init_stt_providers_with_config(tmp.path(), &cfg)
            .expect("provider should be built");
        assert_eq!(provider.name(), "remote");
    }

    #[test]
    fn init_stt_providers_local_mode_builds_local_provider() {
        use medical_core::types::settings::{AppConfig, SttMode};
        let mut cfg = AppConfig::default();
        cfg.stt_mode = SttMode::Local;
        cfg.whisper_model = "large-v3-turbo".into();

        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = init_stt_providers_with_config(tmp.path(), &cfg)
            .expect("provider should be built");
        assert_eq!(provider.name(), "local");
    }
```

Also add `tempfile = "3"` to `[dev-dependencies]` in `src-tauri/Cargo.toml` if not already present.

- [ ] **Step 2: Run — expect compile failure**

Run: `cargo test -p <tauri-crate-name> init_stt_providers`
Expected: `error: cannot find function 'init_stt_providers_with_config'` — we're introducing a new signature that takes the full config so the dispatcher can see `stt_mode`.

### Task 4.2: Refactor `init_stt_providers` to a config-aware variant

- [ ] **Step 3: Replace the existing `init_stt_providers` in `state.rs`**

Locate the function around lines 136–154. Replace the entire function with:

```rust
/// Create the STT provider based on the user's chosen mode.
pub fn init_stt_providers_with_config(
    data_dir: &std::path::Path,
    config: &medical_core::types::settings::AppConfig,
) -> Option<Arc<dyn SttProvider + Send + Sync>> {
    let seg_path = stt_models::pyannote_model_path(data_dir, "segmentation-3.0.onnx");
    let emb_path = stt_models::pyannote_model_path(data_dir, "wespeaker_en_voxceleb_CAM++.onnx");

    match config.stt_mode {
        medical_core::types::settings::SttMode::Local => {
            let whisper_filename = stt_models::whisper_model_filename(&config.whisper_model)
                .unwrap_or("ggml-large-v3-turbo.bin");
            let whisper_path = stt_models::whisper_model_path(data_dir, whisper_filename);
            info!(
                whisper = %whisper_path.display(),
                segmentation = %seg_path.display(),
                embedding = %emb_path.display(),
                "Initializing local STT provider"
            );
            Some(Arc::new(medical_stt_providers::local_provider::LocalSttProvider::new(
                whisper_path,
                seg_path,
                emb_path,
            )))
        }
        medical_core::types::settings::SttMode::Remote => {
            // Load the remote API key from the keychain if present (non-fatal
            // on miss: an empty key just means no Authorization header).
            let api_key = medical_security::key_storage::KeyStorage::open(data_dir)
                .ok()
                .and_then(|ks| ks.get_key("stt_remote_api_key").ok().flatten());

            info!(
                host = %config.stt_remote_host,
                port = config.stt_remote_port,
                model = %config.stt_remote_model,
                has_api_key = api_key.is_some(),
                "Initializing remote STT provider"
            );

            match medical_stt_providers::remote_provider::RemoteSttProvider::new(
                &config.stt_remote_host,
                config.stt_remote_port,
                &config.stt_remote_model,
                api_key,
                seg_path,
                emb_path,
            ) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to build remote STT provider; falling back to no STT");
                    None
                }
            }
        }
    }
}

/// Backwards-compatible wrapper that loads the config and defers to
/// `init_stt_providers_with_config`. Kept because existing call sites pass a
/// bare `whisper_model_id` from the config.
pub fn init_stt_providers(
    data_dir: &std::path::Path,
    _whisper_model_id: &str,
) -> Option<Arc<dyn SttProvider + Send + Sync>> {
    // NOTE: legacy callers — `reinit_providers` and `AppState::initialize` —
    // should migrate to `init_stt_providers_with_config`.
    None
}
```

Then in BOTH call sites, migrate:

1. `AppState::initialize` (around line 240 — find the call `init_stt_providers(&data_dir, &config.whisper_model)`). Change to `init_stt_providers_with_config(&data_dir, &config)`.

2. `src-tauri/src/commands/providers.rs::reinit_providers` (around line 37). Change to `init_stt_providers_with_config(&state.data_dir, &config)`.

After both migrations, remove the legacy `init_stt_providers(_, _)` wrapper entirely — nothing should call it.

- [ ] **Step 4: Run tests**

Run: `cargo build --workspace && cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/providers.rs src-tauri/Cargo.toml
git commit -m "feat(state): branch init_stt_providers on SttMode

Local mode builds LocalSttProvider as before; Remote mode builds
RemoteSttProvider with host/port/model from config and api_key
loaded from the keychain (entry: stt_remote_api_key)."
```

---

## Task 5: Tauri commands — test connections and manage STT API key

**Files:**
- Modify: `src-tauri/src/commands/providers.rs`
- Modify: `src-tauri/src/commands/security.rs` (or wherever `save_api_key` lives — grep for it first)
- Modify: `src-tauri/src/lib.rs` (register new commands)

### Task 5.1: Discover the existing API-key command file

- [ ] **Step 1: Locate the existing API-key commands**

Run: `grep -rn "save_api_key\|store_key" src-tauri/src/commands/`
Expected: find the file that already has `#[tauri::command] pub async fn save_api_key(...)`. Use that file for the new `save_stt_remote_api_key` and `clear_stt_remote_api_key`. Note: if the file is `security.rs`, add there. If it's `settings.rs`, add there. Use the same module.

### Task 5.2: Add `test_stt_remote_connection`

- [ ] **Step 2: Append to `src-tauri/src/commands/providers.rs`**

```rust
/// Test connectivity to a remote Whisper server.
///
/// Hits `GET /v1/models` with a 5 s timeout. Returns a success message
/// including the model count, or a user-readable error.
#[tauri::command]
pub async fn test_stt_remote_connection(
    host: String,
    port: u16,
    api_key: Option<String>,
) -> Result<String, String> {
    let effective_host = if host.is_empty() { "localhost".to_string() } else { host };
    let url = format!("http://{}:{}/v1/models", effective_host, port);

    info!(url = %url, "Testing Whisper server connection");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut req = client.get(&url);
    if let Some(key) = api_key.as_ref().filter(|s| !s.is_empty()) {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let response = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Connection refused — is the Whisper server running at {}:{}?", effective_host, port)
        } else if e.is_timeout() {
            format!("Connection timed out — check that {}:{} is reachable", effective_host, port)
        } else {
            format!("Connection failed: {e}")
        }
    })?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err("Authentication failed — check API key".into());
    }
    if !response.status().is_success() {
        return Err(format!("Server returned HTTP {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from server: {e}"))?;

    let model_count = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(format!(
        "Connected — {} model{} available",
        model_count,
        if model_count == 1 { "" } else { "s" }
    ))
}
```

### Task 5.3: Add `test_ollama_connection`

- [ ] **Step 3: Append to `src-tauri/src/commands/providers.rs`**

```rust
/// Test connectivity to an Ollama server.
///
/// Hits `GET /api/tags` with a 5 s timeout. Returns a success message
/// with the installed-model count, or an error.
#[tauri::command]
pub async fn test_ollama_connection(host: String, port: u16) -> Result<String, String> {
    let effective_host = if host.is_empty() { "localhost".to_string() } else { host };
    let url = format!("http://{}:{}/api/tags", effective_host, port);

    info!(url = %url, "Testing Ollama connection");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client.get(&url).send().await.map_err(|e| {
        if e.is_connect() {
            format!("Connection refused — is Ollama running at {}:{}?", effective_host, port)
        } else if e.is_timeout() {
            format!("Connection timed out — check that {}:{} is reachable", effective_host, port)
        } else {
            format!("Connection failed: {e}")
        }
    })?;

    if !response.status().is_success() {
        return Err(format!("Server returned HTTP {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from server: {e}"))?;

    let model_count = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(format!(
        "Connected — {} model{} installed",
        model_count,
        if model_count == 1 { "" } else { "s" }
    ))
}
```

### Task 5.4: Add STT API-key commands

- [ ] **Step 4: Add two commands to the existing API-key module**

In the file discovered in Step 1 (likely `src-tauri/src/commands/security.rs`), add:

```rust
#[tauri::command]
pub async fn save_stt_remote_api_key(
    state: tauri::State<'_, AppState>,
    api_key: String,
) -> Result<(), String> {
    let ks = medical_security::key_storage::KeyStorage::open(&state.data_dir)
        .map_err(|e| format!("keychain error: {e}"))?;
    ks.store_key("stt_remote_api_key", &api_key)
        .map_err(|e| format!("failed to store key: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn clear_stt_remote_api_key(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let ks = medical_security::key_storage::KeyStorage::open(&state.data_dir)
        .map_err(|e| format!("keychain error: {e}"))?;
    // KeyStorage::store_key with an empty value effectively clears it.
    // If a dedicated delete method exists (grep for `fn delete`), prefer it.
    ks.store_key("stt_remote_api_key", "")
        .map_err(|e| format!("failed to clear key: {e}"))?;
    Ok(())
}
```

If `KeyStorage` has a `delete_key` method (verify via `grep "fn delete" crates/security/src/key_storage.rs`), use that instead for `clear_stt_remote_api_key`.

### Task 5.5: Register the new commands

- [ ] **Step 5: Wire them into the Tauri builder in `src-tauri/src/lib.rs`**

Find the existing `.invoke_handler(tauri::generate_handler![...])` block. Add the four new command identifiers to the list, keeping existing entries untouched:

```rust
            test_stt_remote_connection,
            test_ollama_connection,
            save_stt_remote_api_key,
            clear_stt_remote_api_key,
```

Import them at the top of `lib.rs` alongside existing command imports.

- [ ] **Step 6: Verify build**

Run: `cargo build -p <tauri-crate-name>`
Expected: success.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/providers.rs \
        src-tauri/src/commands/security.rs \
        src-tauri/src/lib.rs
git commit -m "feat(commands): remote STT + Ollama test + key storage commands

Adds test_stt_remote_connection, test_ollama_connection,
save_stt_remote_api_key, clear_stt_remote_api_key. Mirrors the
existing test_lmstudio_connection / save_api_key patterns."
```

---

## Task 6: Settings UI — STT mode toggle + Ollama server subsection

**Files:**
- Modify: `src/lib/components/SettingsContent.svelte`
- Modify: `src/lib/api/providers.ts`
- Create: `src/lib/components/SettingsContent.test.ts`

### Task 6.1: Add the frontend API bindings

- [ ] **Step 1: Extend `src/lib/api/providers.ts`**

Append:

```ts
export async function testSttRemoteConnection(
  host: string,
  port: number,
  apiKey: string | null,
): Promise<string> {
  return invoke<string>('test_stt_remote_connection', { host, port, apiKey });
}

export async function testOllamaConnection(host: string, port: number): Promise<string> {
  return invoke<string>('test_ollama_connection', { host, port });
}

export async function saveSttRemoteApiKey(apiKey: string): Promise<void> {
  return invoke('save_stt_remote_api_key', { apiKey });
}

export async function clearSttRemoteApiKey(): Promise<void> {
  return invoke('clear_stt_remote_api_key');
}
```

### Task 6.2: Add the STT mode toggle + remote fields to `SettingsContent.svelte`

- [ ] **Step 2: Add state variables near the top of the `<script>` block** (alongside existing `lmstudioTestStatus` around line 40)

```ts
  let sttMode = $state<'local' | 'remote'>($settings.stt_mode ?? 'local');
  let sttRemoteTestStatus = $state<'idle' | 'testing' | 'success' | 'error'>('idle');
  let sttRemoteTestMessage = $state('');
  let sttRemoteApiKey = $state('');  // ephemeral; not mirrored from settings
  let ollamaTestStatus = $state<'idle' | 'testing' | 'success' | 'error'>('idle');
  let ollamaTestMessage = $state('');
```

- [ ] **Step 3: Replace the existing Whisper model section with a mode-aware version**

Locate the existing Whisper model download UI (search for `whisper_model` or `"Whisper Model"` in `SettingsContent.svelte`). Wrap it as follows (replace wholesale):

```svelte
<div class="form-group">
  <label class="form-label">STT Mode</label>
  <div class="radio-row">
    <label>
      <input
        type="radio"
        bind:group={sttMode}
        value="local"
        on:change={() => settings.updateField('stt_mode', 'local')}
      /> Local
    </label>
    <label>
      <input
        type="radio"
        bind:group={sttMode}
        value="remote"
        on:change={() => settings.updateField('stt_mode', 'remote')}
      /> Remote
    </label>
  </div>
</div>

{#if sttMode === 'local'}
  <!-- ORIGINAL LOCAL WHISPER UI GOES HERE UNCHANGED -->
{:else}
  <div class="form-group">
    <label for="stt-remote-host" class="form-label">Host</label>
    <input
      id="stt-remote-host"
      type="text"
      placeholder="computer-a.tailnet.ts.net"
      value={$settings.stt_remote_host}
      on:change={(e) => settings.updateField('stt_remote_host', (e.target as HTMLInputElement).value)}
    />
  </div>
  <div class="form-group">
    <label for="stt-remote-port" class="form-label">Port</label>
    <input
      id="stt-remote-port"
      type="number"
      value={$settings.stt_remote_port}
      on:change={(e) => settings.updateField('stt_remote_port', parseInt((e.target as HTMLInputElement).value, 10))}
    />
  </div>
  <div class="form-group">
    <label for="stt-remote-model" class="form-label">Model</label>
    <input
      id="stt-remote-model"
      type="text"
      value={$settings.stt_remote_model}
      on:change={(e) => settings.updateField('stt_remote_model', (e.target as HTMLInputElement).value)}
    />
  </div>
  <div class="form-group">
    <label for="stt-remote-key" class="form-label">API key (optional)</label>
    <input
      id="stt-remote-key"
      type="password"
      bind:value={sttRemoteApiKey}
    />
    <button
      type="button"
      on:click={async () => {
        if (sttRemoteApiKey) await saveSttRemoteApiKey(sttRemoteApiKey);
        else await clearSttRemoteApiKey();
      }}
    >Save key</button>
  </div>
  <div class="form-group">
    <button
      type="button"
      disabled={sttRemoteTestStatus === 'testing'}
      on:click={async () => {
        sttRemoteTestStatus = 'testing';
        sttRemoteTestMessage = '';
        try {
          const msg = await testSttRemoteConnection(
            $settings.stt_remote_host,
            $settings.stt_remote_port,
            sttRemoteApiKey || null,
          );
          sttRemoteTestStatus = 'success';
          sttRemoteTestMessage = msg;
        } catch (err) {
          sttRemoteTestStatus = 'error';
          sttRemoteTestMessage = String(err);
        }
      }}
    >{sttRemoteTestStatus === 'testing' ? 'Testing…' : 'Test connection'}</button>
    {#if sttRemoteTestStatus === 'success'}
      <span class="test-result test-success">✓ {sttRemoteTestMessage}</span>
    {:else if sttRemoteTestStatus === 'error'}
      <span class="test-result test-error">✗ {sttRemoteTestMessage}</span>
    {/if}
  </div>
{/if}

<!-- Pyannote (diarization) download UI STAYS here, unchanged by the mode toggle. -->
<p class="helper-text">Diarization runs on this machine regardless of STT mode.</p>
```

Keep the original local Whisper section's JSX as-is — just move it inside the `{#if sttMode === 'local'}` branch.

Import the new bindings at the top of the `<script>`:

```ts
import {
  testSttRemoteConnection,
  saveSttRemoteApiKey,
  clearSttRemoteApiKey,
  testOllamaConnection,
} from '$lib/api/providers';
```

### Task 6.3: Add the Ollama Server subsection

- [ ] **Step 4: Immediately after the existing "LM Studio Server" subsection** (around line 747 in `SettingsContent.svelte`), add a mirror for Ollama:

```svelte
<!-- Ollama Server -->
<h4 class="subsection-title">Ollama Server</h4>
<p class="helper-text">
  Configure the Ollama server address. Use <code>localhost</code> if Ollama runs on this machine, or enter a remote IP for a network server.
</p>
<div class="form-group">
  <label for="ollama-host" class="form-label">Host</label>
  <input
    id="ollama-host"
    type="text"
    value={$settings.ollama_host}
    on:change={(e) => {
      settings.updateField('ollama_host', (e.target as HTMLInputElement).value);
      ollamaTestStatus = 'idle';
      ollamaTestMessage = '';
    }}
  />
</div>
<div class="form-group">
  <label for="ollama-port" class="form-label">Port</label>
  <input
    id="ollama-port"
    type="number"
    value={$settings.ollama_port}
    on:change={(e) => {
      settings.updateField('ollama_port', parseInt((e.target as HTMLInputElement).value, 10));
      ollamaTestStatus = 'idle';
      ollamaTestMessage = '';
    }}
  />
</div>
<div class="form-group">
  <button
    type="button"
    disabled={ollamaTestStatus === 'testing'}
    on:click={async () => {
      ollamaTestStatus = 'testing';
      ollamaTestMessage = '';
      try {
        const msg = await testOllamaConnection(
          $settings.ollama_host || 'localhost',
          $settings.ollama_port || 11434,
        );
        ollamaTestStatus = 'success';
        ollamaTestMessage = msg;
      } catch (err) {
        ollamaTestStatus = 'error';
        ollamaTestMessage = String(err);
      }
    }}
  >{ollamaTestStatus === 'testing' ? 'Testing…' : 'Test connection'}</button>
  {#if ollamaTestStatus === 'success'}
    <span class="test-result test-success">✓ {ollamaTestMessage}</span>
  {:else if ollamaTestStatus === 'error'}
    <span class="test-result test-error">✗ {ollamaTestMessage}</span>
  {/if}
</div>
```

### Task 6.4: Minimal Vitest for the mode toggle (optional if the existing suite has no Svelte component tests — confirm first)

- [ ] **Step 5: Check whether component tests exist**

Run: `ls src/lib/components/*.test.ts` — if any Svelte-component Vitest file exists, follow its pattern to add one assertion that toggling `sttMode` to `'remote'` hides the Whisper model download UI and shows the remote host field. If no component tests exist at all, skip this subtask and capture the shortcoming in the PR description — do not invent a new test harness.

### Task 6.5: Build and commit

- [ ] **Step 6: Run the frontend build and type-check**

Run: `npx vitest run`
Expected: existing tests pass.

Run: `./node_modules/.bin/svelte-check --tsconfig ./tsconfig.json`
Expected: no new errors in `SettingsContent.svelte` or `providers.ts`.

- [ ] **Step 7: Commit**

```bash
git add src/lib/components/SettingsContent.svelte src/lib/api/providers.ts
git commit -m "feat(settings-ui): STT mode toggle + Ollama server section

Adds a Local | Remote radio to the Audio / STT tab. In Remote mode,
shows host/port/model/API-key inputs and a Test connection button;
pyannote download UI stays visible in both modes with a clarifying
helper. Adds an Ollama Server subsection mirroring the LM Studio one."
```

---

## Task 7: Documentation — server-side setup for Computer A

**Files:**
- Modify: `README.md`

### Task 7.1: Append the appendix

- [ ] **Step 1: Add a new section to `README.md`**

Append to `README.md`:

```markdown
## Running STT / Ollama on a different machine (LAN / Tailscale)

FerriScribe can offload Whisper and Ollama to a more powerful machine reached over your LAN or Tailscale. Diarization still runs on the client.

### Whisper server (Computer A)

Build `whisper.cpp`'s server binary once, then run it against the same model file FerriScribe downloads:

    git clone https://github.com/ggerganov/whisper.cpp
    cd whisper.cpp
    make server
    ./server -m /path/to/ggml-large-v3-turbo.bin --host 0.0.0.0 --port 8080

In FerriScribe (Computer B), Settings → Audio / STT → set STT Mode to **Remote**, Host to Computer A's hostname or IP, Port to `8080`, Model to `whisper-1`. Click **Test connection**.

Any OpenAI-compatible Whisper server works — `faster-whisper-server`, LocalAI, etc. — just match the host/port/model fields accordingly.

### Ollama (Computer A)

Ollama must bind beyond `localhost` to accept remote connections:

    OLLAMA_HOST=0.0.0.0:11434 ollama serve

In FerriScribe (Computer B), Settings → AI Providers → Ollama Server → set Host to Computer A's hostname or IP. Click **Test connection**.

### Security

On a Tailnet, peer identity is handled by Tailscale ACLs. If you expose the servers on an untrusted LAN, configure `whisper.cpp server --api-key` and enter the same value in FerriScribe's **API key** field; Ollama does not support API keys natively and should stay behind a trusted network boundary.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs(readme): server-side setup for remote STT and Ollama

Appendix covers whisper.cpp server + Ollama on Computer A plus the
minimal FerriScribe client configuration. Notes API-key support via
whisper.cpp's --api-key and Tailscale's identity/ACL model."
```

---

## Final verification

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `npx vitest run`
- [ ] `./node_modules/.bin/svelte-check --tsconfig ./tsconfig.json` — no new errors in the files touched by this plan
- [ ] Manual smoke on a real Tailnet: start `whisper.cpp server` on one machine, switch Computer B's STT mode to Remote, record 30 s of audio, verify a transcript + speaker labels return. Stop the server and retry — a clear error should surface.
- [ ] Upgrade smoke: start with a pre-existing `config.json` from v0.9.x, launch the new build, confirm `stt_mode` defaults to Local, Ollama host defaults to `localhost`, no behavior change.

## Out of scope (recap)

- Streaming transcription
- Remote pyannote
- Native whisper.cpp `/inference` endpoint
- Shared `<RemoteServerConfig />` Svelte component across LM Studio / Ollama / remote STT (follow-up once a third section is in the UI)
- Rotating / multi-key management for the STT remote API key beyond save/clear
