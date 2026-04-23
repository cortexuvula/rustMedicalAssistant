# Remote STT and Remote Ollama — Design

**Status:** Approved for plan authoring
**Date:** 2026-04-23
**Owner:** devlead@andrehugo.ca

## Problem

FerriScribe currently assumes Whisper and Ollama run on the same machine as the app. A user running the app on a low-powered laptop (Computer B) cannot use those AI features unless they also run the model servers locally. The user wants to keep Computer B as the client and offload compute to a capable machine (Computer A) reached over Tailscale. LM Studio already supports remote hosting (shipped in v0.9.x); Whisper and Ollama do not.

## Goal

1. Let Computer B transcribe audio by POSTing it to a Whisper server running on Computer A.
2. Let Computer B use an Ollama instance running on Computer A for chat and for RAG embeddings.
3. Keep the UX pattern consistent with the existing LM Studio remote configuration: explicit host/port in Settings, a "Test connection" button, hard errors when the server is unreachable.

## Non-Goals

- **Remote pyannote/diarization.** Diarization stays on Computer B. Pyannote models are small (~34 MB) and CPU-light; splitting them over the network isn't worth the complexity or bandwidth.
- **Streaming transcription.** Neither mode supports it today. Out of scope.
- **whisper.cpp-native `/inference` endpoint.** We target the OpenAI-compatible protocol only for maximum server interoperability.
- **Automatic fallback** from Remote → Local if the remote call fails. Explicit mode toggle, hard errors — per Q2 decision.
- **Discovery / mDNS / Tailnet auto-config.** Users configure host manually.
- **Multi-server load balancing or failover.**

## Current State

- `LmStudioProvider` already takes a host and port; `AppConfig.lmstudio_host` / `AppConfig.lmstudio_port` flow into `state.rs::init_ai_providers`. Settings UI has a host/port input + test button.
- `OllamaProvider::new(host: Option<&str>)` already accepts a host, but `state.rs:113` calls it with `None` — hardcoded to localhost.
- `EmbeddingGenerator::new_ollama(host: Option<&str>, model: Option<&str>)` also accepts a host, also called with `None` in the RAG init path.
- `LocalSttProvider` is the only `SttProvider` impl. Single file at `crates/stt-providers/src/local_provider.rs`. Does Whisper + pyannote + merge in-process.
- `SttProvider` trait at `crates/core/src/traits/stt_provider.rs` is five methods (`name`, `supports_streaming`, `supports_diarization`, `transcribe`, `transcribe_stream`). A second impl slots in without a trait change.

## Architecture

### Remote STT

New crate module: `crates/stt-providers/src/remote_provider.rs`. Defines `RemoteSttProvider` implementing `SttProvider`. Same crate as `LocalSttProvider`; reuses `audio_prep`, `diarization`, `merge`, and the transcript/segment types.

Per-transcription flow:

```
WAV on disk
  → audio_prep::to_16k_mono_f32 (existing)
  → audio_prep::f32_to_i16 (existing, also used by local diarization)
  → wrap as 16-bit PCM WAV in-memory
  → multipart POST {host}:{port}/v1/audio/transcriptions
        fields: file=audio.wav, model, response_format=verbose_json,
                language (if Some), Authorization: Bearer <api_key> (if Some)
  → parse verbose_json → whisper_segments with {start, end, text}
  → if config.diarize && pyannote models present on Computer B:
        run SpeakerDiarizer locally on the same i16 buffer
        (reuses spawn_blocking pattern from LocalSttProvider)
  → merge::merge_segments_with_speakers(whisper_segments, speaker_turns)
  → Transcript { provider: "remote", segments, metadata: { server_url, model } }
```

Dispatcher: `init_stt_providers` in `src-tauri/src/state.rs` switches on `config.stt_mode`. `Local` constructs `LocalSttProvider` as today; `Remote` constructs `RemoteSttProvider` with host/port/model and the API key pulled from the keychain (same path as OpenAI/Anthropic keys). The rest of the app holds `Arc<dyn SttProvider>` and is oblivious to the choice.

### Remote Ollama (chat + embeddings)

One host, one port, one Ollama process — it serves `/api/chat` and `/api/embeddings` from the same daemon. Symmetric to LM Studio:

- `AppConfig.ollama_host: String` (default `"localhost"`)
- `AppConfig.ollama_port: u16` (default `11434`)
- `state.rs:113`: `OllamaProvider::new(Some(&format!("http://{host}:{port}")))` instead of `None`.
- RAG init: `EmbeddingGenerator::new_ollama(Some(&ollama_url), None)`.

The signature threading is already supported by both constructors; this is an invocation change.

### Config additions

In `crates/core/src/types/settings.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttMode {
    #[default]
    Local,
    Remote,
}

// In AppConfig:
#[serde(default)]
pub stt_mode: SttMode,
#[serde(default)]
pub stt_remote_host: String,          // default "" — shown as empty in UI
#[serde(default = "default_stt_remote_port")]
pub stt_remote_port: u16,             // default 8080 (whisper.cpp server default)
#[serde(default = "default_stt_remote_model")]
pub stt_remote_model: String,         // default "whisper-1"

#[serde(default = "default_ollama_host")]
pub ollama_host: String,              // default "localhost"
#[serde(default = "default_ollama_port")]
pub ollama_port: u16,                 // default 11434
```

Existing users upgrading: serde defaults populate the new fields, `stt_mode` defaults to `Local`, `ollama_host`/`ollama_port` to `localhost:11434` — zero behavior change on upgrade.

The STT remote API key is NOT a config field. It lives in the keychain under service `rust-medical-assistant`, key `stt_remote_api_key`. Loaded at provider-construction time by `init_stt_providers`. Matches how OpenAI/Anthropic/Gemini keys are already handled.

### Settings UI

**Audio / STT tab** gets a mode toggle at the top of the Whisper section:

- `STT Mode: ( ) Local    (•) Remote`
- If Local: show the existing Whisper-model-download UI. Hide the remote fields.
- If Remote: hide Whisper-model UI. Show `Host`, `Port`, `Model`, `API key` (password input), `[Test connection]`. Test calls a new Tauri command.
- Pyannote (diarization) download UI stays visible in BOTH modes with helper text: "Diarization runs on this machine regardless of STT mode."

**AI Providers tab** gets an Ollama Server subsection, mirror of the LM Studio one already present: `Host`, `Port`, `[Test connection]`.

### New Tauri commands

- `test_stt_remote_connection(host, port, api_key)` → `Result<SttTestResult, AppError>` where `SttTestResult` includes the list of models returned by `GET /v1/models`. Surface to the UI as "✓ Connected — N models available: [names]".
- `test_ollama_connection(host, port)` → `Result<OllamaTestResult, AppError>` — GET `/api/tags`, return the list of installed models.
- `save_stt_remote_api_key(key)` / `clear_stt_remote_api_key()` for keychain writes (mirror of existing `save_api_key` commands).

## Error handling

Categories and UI surfacing:

| Condition | AppError variant (today, strings; structured after the 2026-04-22 plan lands) | UX |
|-----------|-------------------------------------------------------------------------------|----|
| DNS / TCP / TLS failure | `SttProvider("Cannot reach Whisper server at {host}:{port}: {e}")` | Toast + inline error in Record tab; "failed" pipeline event |
| HTTP 401 / 403 | `SttProvider("Whisper server rejected authentication — check API key")` | Same |
| HTTP 4xx (other) | `SttProvider("Whisper server rejected request: {status} {body-prefix}")` | Same |
| HTTP 5xx | `SttProvider("Whisper server internal error: {status}")` | Same |
| Timeout (>600 s for transcribe, >10 s for test-connection) | `SttProvider("Transcription timed out after 600s")` | Same |
| Malformed `verbose_json` | `SttProvider("Unexpected response from Whisper server")` | Same |

The `Authorization` header is redacted from reqwest tracing spans via `set_sensitive` or a custom middleware; the raw API key never appears in logs.

Remote-mode failure does NOT fall back to Local. The user picked Remote explicitly; silent fallback would mask configuration errors.

## Testing

### Rust

- **`remote_provider.rs` unit tests.** Use `wiremock` (async, tokio-friendly) to stand up a mock `/v1/audio/transcriptions` endpoint. Cases:
  1. Happy path — canned `verbose_json` → segments parsed, no diarization requested.
  2. Happy path with diarization — same response, `config.diarize = true`, local pyannote models present (use `tempdir` with mock `.onnx` files? — actually just skip diarization assertion if models absent, and test the diarization integration separately with existing local_provider tests that already run on real models). Prefer: test segment-mapping only; trust merge tests.
  3. HTTP 401 → returns `SttProvider` error with "authentication" in the message.
  4. HTTP 503 → surfaces "server internal error".
  5. Network timeout — configure wiremock to delay > timeout; assert the right error.
  6. Malformed JSON → parse error surfaced cleanly, not a panic.
  7. API key absence — no `Authorization` header on the request (verify via wiremock request recorder).
  8. API key presence — `Authorization: Bearer <key>` present.

- **Config migration test.** Deserialize a v0.9.x JSON (without new fields) and assert all new fields populate to defaults with `stt_mode == Local`.

- **`state.rs::init_stt_providers` branch test.** With `SttMode::Local`, constructs `LocalSttProvider`. With `SttMode::Remote`, constructs `RemoteSttProvider` with host/port/model from config and api_key from the (mocked) keychain.

### Svelte / Vitest

- Mode toggle conditional rendering (Local fields vs Remote fields).
- "Test connection" button invokes the right Tauri command with the right payload.
- Pyannote download UI is visible in both modes.

### Manual smoke

- Run `whisper.cpp server` on Computer A, point Computer B at its Tailscale name, record 30 s of audio on Computer B, verify transcript + diarization both come back correctly.
- Stop the server; trigger another transcription; verify a clean error message names the unreachable host.
- Switch to Local mode with a downloaded Whisper model; verify nothing about local behavior changed.

## Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| 40 MB+ uploads on slow links | Pre-resample to 16 kHz mono and downsample to i16 PCM (≈ 19 MB for 10 min); acceptable on Tailscale LAN. Don't add FLAC dep for marginal gain. |
| Whisper servers differ in `verbose_json` fields | Parse conservatively — require only `segments[].{text, start, end}`; treat top-level `language`, `duration`, and per-word timestamps as optional. |
| API key leaks in logs | Redact `Authorization` via reqwest tracing config; never `{:?}`-debug the full `RemoteSttProvider` struct. |
| User configures remote but pyannote models absent → diarization silently off | Reuse the existing warning path in `local_provider.rs:109–111`; surface "Diarization skipped — pyannote models not downloaded" in the pipeline progress output. |
| Port confusion (LM Studio 1234, Ollama 11434, Whisper 8080) | Default each field separately; show the intended use in helper text under each. |
| whisper.cpp server sometimes returns `result` instead of `segments` in legacy mode | Require `response_format=verbose_json` explicitly; document in the README appendix that raw mode is unsupported. |
| Settings UI state complexity grows with each mode | Extract a `<RemoteServerConfig />` Svelte component reused by LM Studio, Ollama, and remote STT blocks. Out of scope for the first plan but flagged as a follow-up if the settings tab grows further. |

## Success criteria

1. On Computer B with `stt_mode = Remote`, recording audio and running the pipeline produces a transcript + diarization that matches the local-only output for the same audio file (segment timing and text identical to within Whisper's non-determinism).
2. With the remote server stopped, transcription fails with a clear error that names the configured host — no hang, no silent fallback.
3. With `ollama_host` / `ollama_port` pointing at Computer A, Ollama chat and RAG embedding calls succeed and use the remote host (verify via server logs on Computer A).
4. Upgrading from v0.9.x to the new version preserves behavior: `stt_mode` defaults to `Local`, Ollama host defaults to `localhost`.
5. Tests pass; no regressions in existing STT or RAG flows.

## Out of scope (recap, deferred follow-ups)

- Extract a shared `<RemoteServerConfig />` Svelte component across LM Studio / Ollama / remote STT.
- Streaming transcription.
- Remote pyannote.
- Native whisper.cpp `/inference` endpoint.
- Auto-discovery of servers on the Tailnet.
- Remote STT API-key UI management beyond save / clear (no rotation, no multi-key).
- Concurrent multi-file transcription limits (current behavior: unbounded concurrency per user action, bounded in practice by Whisper server throughput).

## Appendix: Server-side setup for Computer A (for the README / docs)

**whisper.cpp server** (recommended, same engine the app embeds):

```bash
# One-time build
git clone https://github.com/ggerganov/whisper.cpp
cd whisper.cpp
make server

# Run with a model already downloaded for FerriScribe
./server -m /path/to/ggml-large-v3-turbo.bin --host 0.0.0.0 --port 8080
```

Expose the Tailscale IP, not `0.0.0.0` on the LAN, if you want Tailnet-only access. FerriScribe's Settings → STT Remote fields then take the Computer A Tailnet name and port 8080.

**Ollama** on Computer A:

```bash
# Ollama must bind beyond localhost to accept remote connections
OLLAMA_HOST=0.0.0.0:11434 ollama serve
```

Tailscale ACLs provide the access control. No additional auth is required unless you want defense-in-depth with an API key (whisper.cpp server supports `--api-key`).
