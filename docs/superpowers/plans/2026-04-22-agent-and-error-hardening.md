# Agent Orchestrator & Tauri Error-Boundary Hardening Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three defects in the Tauri ↔ Rust boundary: (1) the agent orchestrator hardcodes `gpt-4o` and breaks every non-OpenAI provider, (2) Tauri commands collapse typed `AppError` into opaque strings, (3) `transcription.rs` copy-pastes the "mark recording failed" block six times with inconsistent event emission.

**Architecture:**
- **Orchestrator**: take the model as an explicit parameter on `execute()`; the caller (`chat_with_agent`) reads it from settings via the existing `load_chat_settings()` helper.
- **Error boundary**: implement `serde::Serialize` on `AppError` manually (produces `{kind, message}` wire shape), then migrate command signatures from `Result<_, String>` to `Result<_, AppError>` so Tauri delivers structured errors to the frontend.
- **Failure helper**: extract `mark_recording_failed(app, state, recording, err_msg)` in `transcription.rs`, emit `transcription-progress: "failed"` consistently, collapse six duplicated blocks.

**Tech Stack:** Rust 2024 edition, Tauri v2, `thiserror`, `serde`/`serde_json`, `tokio`, Svelte 5 + TypeScript.

---

## File Structure

### New / modified Rust files

| Path | Change | Responsibility |
|------|--------|---------------|
| `crates/core/src/error.rs` | modify | Add hand-written `Serialize` + `kind_str()` + `ErrorKind` helper |
| `crates/agents/src/orchestrator.rs` | modify | `execute()` takes `model: &str`; remove hardcoded `"gpt-4o"` |
| `src-tauri/src/commands/chat.rs` | modify | `chat_with_agent` passes settings model into orchestrator; migrate `Result<_, String>` → `Result<_, AppError>` |
| `src-tauri/src/commands/transcription.rs` | modify | Extract `mark_recording_failed`; migrate to `Result<_, AppError>` |
| `src-tauri/src/commands/*.rs` (remaining files) | modify | Migrate return types to `Result<_, AppError>` |

### New / modified frontend files

| Path | Change | Responsibility |
|------|--------|---------------|
| `src/lib/types/errors.ts` | create | Define `TauriError` type + `formatError()` helper |
| `src/lib/stores/*.ts` (error-handling sites) | modify | Replace `String(err)` with `formatError(err)` |

---

## Task 1: Thread the model through `AgentOrchestrator::execute`

**Files:**
- Modify: `crates/agents/src/orchestrator.rs` (lines 38–62, tests 252+)
- Modify: `src-tauri/src/commands/chat.rs` (lines 268–276)

### Task 1.1: Write failing test — orchestrator uses caller-supplied model

- [ ] **Step 1: Add a stub `AiProvider` mock in orchestrator tests**

Append to `crates/agents/src/orchestrator.rs` inside the existing `#[cfg(test)] mod tests` block (after line 271):

```rust
    use async_trait::async_trait;
    use futures_core::Stream;
    use medical_core::error::AppResult;
    use medical_core::traits::AiProvider;
    use medical_core::types::{
        CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse,
        ToolDef, UsageInfo,
    };
    use std::sync::Mutex;

    /// Test double that records every model name it sees.
    struct ModelCapturingProvider {
        captured_models: Mutex<Vec<String>>,
    }

    impl ModelCapturingProvider {
        fn new() -> Self {
            Self { captured_models: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl AiProvider for ModelCapturingProvider {
        fn name(&self) -> &str { "capturing" }
        async fn available_models(&self) -> AppResult<Vec<ModelInfo>> { Ok(vec![]) }
        async fn complete(&self, _req: CompletionRequest) -> AppResult<CompletionResponse> {
            unreachable!("orchestrator uses complete_with_tools")
        }
        async fn complete_stream(
            &self,
            _req: CompletionRequest,
        ) -> AppResult<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send + Unpin>> {
            unreachable!()
        }
        async fn complete_with_tools(
            &self,
            request: CompletionRequest,
            _tools: Vec<ToolDef>,
        ) -> AppResult<ToolCompletionResponse> {
            self.captured_models.lock().unwrap().push(request.model.clone());
            Ok(ToolCompletionResponse {
                content: Some("done".into()),
                tool_calls: vec![],
                usage: UsageInfo::default(),
                finish_reason: None,
            })
        }
    }

    #[tokio::test]
    async fn execute_forwards_caller_supplied_model() {
        use crate::agents::ChatAgent;
        use medical_core::types::AgentContext;
        use tokio_util::sync::CancellationToken;

        let registry = ToolRegistry::default();
        let orchestrator = AgentOrchestrator::new(registry);
        let provider = ModelCapturingProvider::new();
        let agent = ChatAgent::default();
        let context = AgentContext {
            user_message: "hi".into(),
            conversation_history: vec![],
            patient_context: None,
            rag_context: vec![],
            recording: None,
        };

        let _ = orchestrator
            .execute(
                &agent,
                context,
                &provider,
                "claude-sonnet-4-6",
                CancellationToken::new(),
            )
            .await
            .expect("run");

        let captured = provider.captured_models.lock().unwrap();
        assert_eq!(
            captured.as_slice(),
            &["claude-sonnet-4-6".to_string()],
            "orchestrator must pass the caller-supplied model, not a hardcoded default"
        );
    }
```

- [ ] **Step 2: Run test — expect compilation failure**

Run: `cargo test -p medical_agents execute_forwards_caller_supplied_model`
Expected: `error[E0061]: this method takes 4 arguments but 5 arguments were supplied` — the test call-site passes `"claude-sonnet-4-6"` which doesn't match the current 4-arg `execute()`.

### Task 1.2: Add the `model` parameter and remove the hardcode

- [ ] **Step 3: Change `execute` signature to accept `model: &str`**

In `crates/agents/src/orchestrator.rs`, replace the current signature and hardcoded assignment.

Replace lines 38–62 exactly:

```rust
    /// Execute an agent run for the given context using the provided AI provider.
    ///
    /// Builds the message list from context, then iterates:
    /// 1. Call provider with tool definitions
    /// 2. If the provider requests tool calls, execute them and append results
    /// 3. If no tool calls remain, return the final response
    ///
    /// `model` is the model identifier to pass into every `CompletionRequest`.
    /// Callers should source this from user settings for the active provider.
    pub async fn execute(
        &self,
        agent: &dyn Agent,
        context: AgentContext,
        provider: &dyn AiProvider,
        model: &str,
        cancel: CancellationToken,
    ) -> AppResult<AgentResponse> {
        // Get only the tools that are both requested by the agent and present in the registry
        let agent_tool_defs = agent.available_tools();
        let available_tool_defs: Vec<_> = agent_tool_defs
            .iter()
            .filter(|def| self.tool_registry.get(&def.name).is_some())
            .cloned()
            .collect();

        // Build the initial message list
        let mut messages = build_messages(&context);

        let mut tool_calls_made: Vec<AgentToolCallRecord> = Vec::new();
        let mut total_usage = UsageInfo::default();
        let mut iterations: u32 = 0;
```

Then replace lines 83–89 (the `CompletionRequest` build) so it uses the parameter:

```rust
            let request = CompletionRequest {
                model: model.to_string(),
                messages: messages.clone(),
                temperature: Some(0.2),
                max_tokens: Some(4096),
                system_prompt: Some(agent.system_prompt().to_string()),
            };
```

(Delete the former `let model = "gpt-4o".to_string();` block entirely — it no longer exists.)

- [ ] **Step 4: Update the one non-test caller**

In `src-tauri/src/commands/chat.rs`, replace lines 268–276:

```rust
    let cancel = CancellationToken::new();

    let (model, _temperature) = load_chat_settings(&state);

    debug!(
        "chat_with_agent: running agent '{}' with model '{}'",
        agent_name, model
    );

    let response = state
        .orchestrator
        .execute(agent.as_ref(), context, provider.as_ref(), &model, cancel)
        .await
        .map_err(|e| format!("Agent execution failed: {e}"))?;
```

- [ ] **Step 5: Run the new test — expect PASS**

Run: `cargo test -p medical_agents execute_forwards_caller_supplied_model`
Expected: `test execute_forwards_caller_supplied_model ... ok`

- [ ] **Step 6: Run the full workspace build and test suite**

Run: `cargo build --workspace && cargo test --workspace`
Expected: build succeeds, all tests pass. If any other existing call-site of `orchestrator.execute(...)` surfaces, it's inside the same commands module and should have been caught by step 4.

- [ ] **Step 7: Commit**

```bash
git add crates/agents/src/orchestrator.rs src-tauri/src/commands/chat.rs
git commit -m "fix(agents): thread caller-supplied model through orchestrator

The orchestrator hardcoded \"gpt-4o\" in every CompletionRequest, so
every non-OpenAI provider (Ollama, LM Studio, Anthropic, Gemini) broke
when the user invoked any agent. Callers now pass the model explicitly;
chat_with_agent sources it from AppConfig.ai_model via load_chat_settings."
```

---

## Task 2: Extract `mark_recording_failed` helper and restore missing progress emits

**Files:**
- Modify: `src-tauri/src/commands/transcription.rs` (add helper; replace 6 duplicated blocks)

### Task 2.1: Write failing test — helper marks recording Failed and emits event

The existing transcription.rs test module at line 442 tests only the pure `is_repeated_phrase_hallucination` function. We'll add a DB-integration test for the new helper.

- [ ] **Step 1: Add a test that exercises the helper's DB side-effect**

Append to `src-tauri/src/commands/transcription.rs` inside the existing `#[cfg(test)] mod tests` (after line 485):

```rust
    use chrono::Utc;
    use medical_core::types::recording::{ProcessingStatus, Recording};
    use medical_db::recordings::RecordingsRepo;
    use medical_db::Database;
    use std::sync::Arc;
    use uuid::Uuid;

    fn mk_recording() -> Recording {
        Recording {
            id: Uuid::new_v4(),
            title: "t".into(),
            audio_path: std::path::PathBuf::from("/tmp/nope.wav"),
            transcript: None,
            stt_provider: None,
            soap_note: None,
            context: None,
            status: ProcessingStatus::Processing { started_at: Utc::now() },
            created_at: Utc::now(),
            updated_at: Utc::now(),
            duration_seconds: None,
        }
    }

    #[tokio::test]
    async fn mark_recording_failed_updates_status_to_failed() {
        let db = Arc::new(Database::open_in_memory().expect("open in-memory db"));
        let rec = mk_recording();
        let id = rec.id;
        {
            let conn = db.conn().expect("conn");
            RecordingsRepo::insert(&conn, &rec).expect("insert");
        }

        super::mark_recording_failed_db_only(&db, rec, "boom".to_string()).await;

        let conn = db.conn().expect("conn");
        let loaded = RecordingsRepo::get_by_id(&conn, &id).expect("get");
        match loaded.status {
            ProcessingStatus::Failed { error, retry_count } => {
                assert_eq!(error, "boom");
                assert_eq!(retry_count, 0);
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }
```

Note: this test only exercises the DB-update half of the helper. The event-emit half calls into Tauri's `AppHandle::emit`, which requires a live Tauri runtime; we split the helper so the DB logic is testable as a pure async function and the emit is done by a thin wrapper. See steps 3–4.

- [ ] **Step 2: Run the test — expect compilation failure**

Run: `cargo test -p medical_tauri_app mark_recording_failed_updates_status_to_failed` (replace crate name if `src-tauri`'s `[package] name` differs — check with `grep "^name" src-tauri/Cargo.toml`; use that name).
Expected: `error[E0425]: cannot find function \`mark_recording_failed_db_only\` in module \`super\``.

### Task 2.2: Implement the helper pair

- [ ] **Step 3: Add the DB-only helper and the event-emitting wrapper**

Insert at the top of `src-tauri/src/commands/transcription.rs`, immediately after the `load_wav_to_audio_data` function (after line 75):

```rust
/// Persist `Failed` status for a recording. Ignores DB errors — the caller is
/// already returning the original error, so a DB write failure here would only
/// obscure it. This is the testable half of `mark_recording_failed`.
pub(super) async fn mark_recording_failed_db_only(
    db: &Arc<medical_db::Database>,
    mut recording: medical_core::types::recording::Recording,
    err_msg: String,
) {
    recording.status = ProcessingStatus::Failed {
        error: err_msg,
        retry_count: 0,
    };
    let db = Arc::clone(db);
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(conn) = db.conn() {
            let _ = RecordingsRepo::update(&conn, &recording);
        }
    })
    .await;
}

/// Mark a recording as `Failed`, persist the status, and emit
/// `transcription-progress: "failed"` so the frontend spinner clears.
///
/// Returns the error message unchanged so callers can `return Err(mark_recording_failed(...).await);`.
async fn mark_recording_failed(
    app: &tauri::AppHandle,
    db: &Arc<medical_db::Database>,
    recording: medical_core::types::recording::Recording,
    err_msg: String,
) -> String {
    mark_recording_failed_db_only(db, recording, err_msg.clone()).await;
    let _ = app.emit("transcription-progress", "failed");
    err_msg
}
```

- [ ] **Step 4: Run the new test — expect PASS**

Run: `cargo test -p <tauri-crate-name> mark_recording_failed_updates_status_to_failed`
Expected: `test mark_recording_failed_updates_status_to_failed ... ok`

### Task 2.3: Replace the six duplicated blocks

Each replacement below is exact. The pattern: delete the twelve-ish lines that build `ProcessingStatus::Failed`, spawn_block, and (sometimes) emit; replace with one line calling the helper.

- [ ] **Step 5: Replace block at lines 125–139 (WAV file not found)**

Find in `transcription.rs`:

```rust
    if !wav_path.exists() {
        let err_msg = format!("WAV file not found: {}", wav_path.display());
        // Mark failed on a blocking thread
        let db = Arc::clone(&state.db);
        let mut rec = recording;
        rec.status = ProcessingStatus::Failed {
            error: err_msg.clone(),
            retry_count: 0,
        };
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = db.conn() {
                let _ = RecordingsRepo::update(&conn, &rec);
            }
        })
        .await;
        return Err(err_msg);
    }
```

Replace with:

```rust
    if !wav_path.exists() {
        let err_msg = format!("WAV file not found: {}", wav_path.display());
        return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
    }
```

- [ ] **Step 6: Replace block at lines 179–196 (empty audio samples)**

Find:

```rust
    if audio.samples.is_empty() {
        let err_msg = format!("WAV file contains no audio samples: {}", wav_path.display());
        tracing::error!("{err_msg}");
        // Mark as Failed in DB before returning.
        let db = Arc::clone(&state.db);
        let mut rec = recording.clone();
        rec.status = ProcessingStatus::Failed {
            error: err_msg.clone(),
            retry_count: 0,
        };
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = db.conn() {
                let _ = RecordingsRepo::update(&conn, &rec);
            }
        })
        .await;
        return Err(err_msg);
    }
```

Replace with:

```rust
    if audio.samples.is_empty() {
        let err_msg = format!("WAV file contains no audio samples: {}", wav_path.display());
        tracing::error!("{err_msg}");
        return Err(mark_recording_failed(&app, &state.db, recording.clone(), err_msg).await);
    }
```

- [ ] **Step 7: Replace block at lines 213–229 (no STT provider)**

Find (the `None => { ... }` arm inside the match):

```rust
            None => {
                let err_msg = "No STT provider configured. Download a Whisper model in Settings → Audio / STT.".to_string();
                tracing::error!("{err_msg}");
                // Mark recording as Failed so it doesn't stay stuck at Processing.
                let db = Arc::clone(&state.db);
                let mut rec = recording;
                rec.status = ProcessingStatus::Failed {
                    error: err_msg.clone(),
                    retry_count: 0,
                };
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(conn) = db.conn() {
                        let _ = RecordingsRepo::update(&conn, &rec);
                    }
                })
                .await;
                return Err(err_msg);
            }
```

Replace with:

```rust
            None => {
                let err_msg = "No STT provider configured. Download a Whisper model in Settings → Audio / STT.".to_string();
                tracing::error!("{err_msg}");
                return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
            }
```

- [ ] **Step 8: Replace block at lines 235–252 (STT transcribe error)**

Find (the `Err(e) => { ... }` arm):

```rust
        Err(e) => {
            let err_msg = format!("Transcription failed: {e}");
            tracing::error!(error = %e, "STT transcription failed");
            // Mark recording as Failed so it doesn't stay stuck at Processing.
            let db = Arc::clone(&state.db);
            let mut rec = recording;
            rec.status = ProcessingStatus::Failed {
                error: err_msg.clone(),
                retry_count: 0,
            };
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(conn) = db.conn() {
                    let _ = RecordingsRepo::update(&conn, &rec);
                }
            })
            .await;
            return Err(err_msg);
        }
```

Replace with:

```rust
        Err(e) => {
            let err_msg = format!("Transcription failed: {e}");
            tracing::error!(error = %e, "STT transcription failed");
            return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
        }
```

- [ ] **Step 9: Replace hallucination-guard block (lines 268–292)**

Find the entire `if rms < 0.001 && is_repeated_phrase_hallucination(...)` body and replace with:

```rust
    if rms < 0.001 && is_repeated_phrase_hallucination(&transcript.text) {
        let err_msg = format!(
            "Transcription rejected: the audio was effectively silent (rms={rms:.6}) and the model returned a repeated-phrase hallucination. Check your microphone or audio routing."
        );
        tracing::warn!(
            provider = %transcript.provider,
            rms = %format!("{:.6}", rms),
            text_preview = %transcript.text.chars().take(80).collect::<String>(),
            "Rejecting likely Whisper hallucination from silent source"
        );
        return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
    }
```

- [ ] **Step 10: Replace empty-text block (lines 296–317)**

Find the `if display_text.trim().is_empty() { ... }` body and replace with:

```rust
    if display_text.trim().is_empty() {
        let err_msg = "Transcription produced no text — the recording may be silent or too short.".to_string();
        tracing::warn!(
            provider = %transcript.provider,
            segments = transcript.segments.len(),
            "{err_msg}"
        );
        return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
    }
```

- [ ] **Step 11: Build, test, and verify a grep**

Run: `cargo build -p <tauri-crate-name>`
Expected: success.

Run: `cargo test -p <tauri-crate-name>`
Expected: all tests pass including the new `mark_recording_failed_updates_status_to_failed`.

Run: `grep -c "ProcessingStatus::Failed" src-tauri/src/commands/transcription.rs`
Expected: `1` (only the one inside `mark_recording_failed_db_only` — all six duplicates are gone).

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/commands/transcription.rs
git commit -m "refactor(transcription): extract mark_recording_failed helper

Collapses six copy-pasted \"set status=Failed + spawn_blocking update\"
blocks into one helper, and fixes four of those paths that silently
skipped emitting transcription-progress: \"failed\" — the frontend
spinner got stuck on those failure modes."
```

---

## Task 3: Make `AppError` serializable and migrate command return types

**Files:**
- Modify: `crates/core/src/error.rs`
- Modify: all `src-tauri/src/commands/*.rs`
- Create: `src/lib/types/errors.ts`
- Modify: error-handling call-sites in `src/lib/stores/*.ts`

### Task 3.1: Hand-write `Serialize` for `AppError`

`AppError` wraps `std::io::Error` and `serde_json::Error` via `#[from]`. Neither inner type implements `Serialize`, so `#[derive(Serialize)]` won't compile. We implement `Serialize` manually to produce `{kind: "Io", message: "..."}`.

- [ ] **Step 1: Write the failing serialization test**

Append to `crates/core/src/error.rs` inside the existing `#[cfg(test)] mod tests` block (after line 135):

```rust
    #[test]
    fn app_error_serializes_with_kind_and_message() {
        let err = AppError::AiProvider("bad API key".into());
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(json["kind"], "AiProvider");
        assert_eq!(json["message"], "AI provider error: bad API key");
    }

    #[test]
    fn app_error_io_serializes_with_io_kind() {
        let err: AppError = std::io::Error::new(std::io::ErrorKind::NotFound, "x").into();
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(json["kind"], "Io");
        assert!(
            json["message"].as_str().unwrap().contains("x"),
            "message must contain the underlying error text"
        );
    }

    #[test]
    fn app_error_cancelled_serializes() {
        let err = AppError::Cancelled;
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(json["kind"], "Cancelled");
        assert_eq!(json["message"], "Cancelled");
    }
```

- [ ] **Step 2: Run test — expect compile failure**

Run: `cargo test -p medical_core app_error_serializes_with_kind_and_message`
Expected: `error[E0277]: the trait bound \`AppError: Serialize\` is not satisfied`.

- [ ] **Step 3: Implement `kind_str` and manual `Serialize`**

Append to `crates/core/src/error.rs` (after line 53, immediately after the `AppError` enum definition, before `pub type AppResult<T>`):

```rust
impl AppError {
    /// Stable machine-readable discriminant for this error. Matches the variant name.
    pub fn kind_str(&self) -> &'static str {
        match self {
            AppError::Database(_) => "Database",
            AppError::Security(_) => "Security",
            AppError::Audio(_) => "Audio",
            AppError::AiProvider(_) => "AiProvider",
            AppError::SttProvider(_) => "SttProvider",
            AppError::TtsProvider(_) => "TtsProvider",
            AppError::Agent(_) => "Agent",
            AppError::Rag(_) => "Rag",
            AppError::Processing(_) => "Processing",
            AppError::Export(_) => "Export",
            AppError::Translation(_) => "Translation",
            AppError::Config(_) => "Config",
            AppError::Io(_) => "Io",
            AppError::Serialization(_) => "Serialization",
            AppError::Cancelled => "Cancelled",
            AppError::Other(_) => "Other",
        }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind_str())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Other(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Other(s.to_string())
    }
}
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test -p medical_core`
Expected: all three new serialize tests pass; existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/error.rs
git commit -m "feat(core): implement Serialize for AppError with {kind, message} shape

Tauri v2 requires error types to implement Serialize to deliver them as
structured payloads to the frontend. Manual impl produces a stable
machine-readable \"kind\" discriminant + human-readable \"message\",
avoiding the need to add Serialize bounds on the inner io::Error /
serde_json::Error wrapped by the #[from] variants."
```

### Task 3.2: Migrate `src-tauri/src/commands/transcription.rs` to `AppError`

This file is the most tangled; migrating it first validates the pattern. All later command files follow the same mechanical rewrite.

- [ ] **Step 6: Change return types and error conversions in `transcription.rs`**

Open `src-tauri/src/commands/transcription.rs`. Apply these edits in order:

1. At the top, alongside the existing imports, add:
   ```rust
   use medical_core::error::{AppError, AppResult};
   ```

2. Change the `transcribe_recording` signature (around line 87–93) return type from `Result<String, String>` to `AppResult<String>`.

3. Change `list_stt_providers` (around line 432–434) return type from `Result<Vec<(String, bool)>, String>` to `AppResult<Vec<(String, bool)>>`.

4. Search-and-replace inside this file only: replace every `.map_err(|e| e.to_string())` with `.map_err(AppError::from)`, EXCEPT for two cases that must remain as strings because they pre-existed inside nested `Result<_, String>` closures:
   - Inside the inner `spawn_blocking` closure at lines 108–118 (`Ok::<_, String>(recording)`): change the closure's error type to `AppError` and propagate via `.map_err(AppError::from)` instead.
   - Same for the vocabulary-correction closure at lines 321–343 (`Ok::<String, String>(...)`): change to `Ok::<String, AppError>(...)` and update the internal `.map_err` calls.

5. Change `load_wav_to_audio_data` signature (line 46) from `Result<AudioData, String>` to `Result<AudioData, AppError>`. Internal `.map_err(|e| format!(...))` calls become `.map_err(|e| AppError::Processing(format!(...)))`.

6. Convert the five `return Err(err_msg)` sites (inside the blocks we kept after Task 2's refactor — they now call `mark_recording_failed` and return its `String`). Wrap the return value in `AppError::Processing(...)`:

   Pattern — change `return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);` to `return Err(AppError::Processing(mark_recording_failed(&app, &state.db, recording, err_msg).await));`

7. Near line 104 (`Uuid::parse_str(...)`), change `.map_err(|e| e.to_string())?` to `.map_err(|e| AppError::Other(format!("invalid recording id: {e}")))?`.

8. Near line 120, 146, 343, 358 (`format!("Task join error: {e}")` patterns): wrap as `AppError::Other(format!("Task join error: {e}"))`.

- [ ] **Step 7: Build the crate**

Run: `cargo build -p <tauri-crate-name>`
Expected: success. If any `?` fails due to missing `From` impl, add `.map_err(AppError::from)` at that site.

- [ ] **Step 8: Run transcription-related tests**

Run: `cargo test -p <tauri-crate-name> transcription`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/commands/transcription.rs
git commit -m "refactor(commands): migrate transcription.rs to Result<_, AppError>

Commands now return structured errors to the frontend: {kind, message}
instead of an opaque string. Establishes the migration template applied
to the remaining command modules in follow-ups."
```

### Task 3.3: Migrate remaining command modules

- [ ] **Step 10: Enumerate the remaining command files**

Run: `ls src-tauri/src/commands/`
Expected list (may include additional files): `audio.rs`, `chat.rs`, `context_templates.rs`, `export.rs`, `generation.rs`, `ingestion.rs`, `logging.rs`, `mod.rs`, `pipeline.rs`, `rag.rs`, `recordings.rs`, `settings_cmd.rs` (or similar), `stt_models.rs`, `vocabulary.rs`.

- [ ] **Step 11: Apply the same mechanical migration file-by-file**

For **each** file in the list above (skip `mod.rs` and any already migrated in Task 2/3.2), apply:

1. Add `use medical_core::error::{AppError, AppResult};` if not already present.
2. Change every `Result<T, String>` in a `#[tauri::command]` return position to `AppResult<T>`.
3. Change every `.map_err(|e| e.to_string())` to `.map_err(AppError::from)`.
4. Change every `.map_err(|e| format!("...: {e}"))` to `.map_err(|e| AppError::<variant>(format!("...: {e}")))` where `<variant>` matches the domain (e.g., `Agent` for agent errors, `Database` for DB ops, `Config` for settings ops).
5. Change every `.ok_or_else(|| "...".to_string())` to `.ok_or_else(|| AppError::Config("...".into()))` or the nearest matching variant.
6. Change `return Err("...".to_string())` / `return Err(format!(...))` to `return Err(AppError::<variant>(...))`.

After each file, run:

```bash
cargo build -p <tauri-crate-name>
```

Expected: success. Fix the specific compile error before moving to the next file.

- [ ] **Step 12: Verify no `Result<_, String>` remains in command signatures**

Run:

```bash
grep -rn "Result<.*, String>" src-tauri/src/commands/
```

Expected: zero matches. (If a match remains, it's either a return from a non-`#[tauri::command]` helper — leave it — or a missed command. Check and migrate.)

- [ ] **Step 13: Run the full workspace tests**

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 14: Commit**

```bash
git add src-tauri/src/commands/
git commit -m "refactor(commands): migrate remaining commands to Result<_, AppError>

Completes the structured-error migration. All Tauri commands now return
{kind, message} errors to the frontend, enabling category-specific UX
(e.g., retry prompts for Network errors, settings deep-links for
Config errors)."
```

### Task 3.4: Update frontend error-handling to consume structured errors

- [ ] **Step 15: Create the TypeScript error helper**

Create `src/lib/types/errors.ts`:

```typescript
/** Structured error payload emitted by Tauri commands. */
export type TauriError = {
  kind:
    | "Database"
    | "Security"
    | "Audio"
    | "AiProvider"
    | "SttProvider"
    | "TtsProvider"
    | "Agent"
    | "Rag"
    | "Processing"
    | "Export"
    | "Translation"
    | "Config"
    | "Io"
    | "Serialization"
    | "Cancelled"
    | "Other";
  message: string;
};

/**
 * Best-effort extraction of a human-readable message from anything `invoke`
 * might throw. Handles the new structured shape, plain strings (legacy), and
 * unknowns.
 */
export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const e = err as Partial<TauriError> & { message?: unknown };
    if (typeof e.message === "string") return e.message;
  }
  return String(err);
}

/** Type guard: was this a structured AppError, not a raw string? */
export function isTauriError(err: unknown): err is TauriError {
  return (
    !!err &&
    typeof err === "object" &&
    typeof (err as TauriError).kind === "string" &&
    typeof (err as TauriError).message === "string"
  );
}
```

- [ ] **Step 16: Update stores that currently stringify errors**

For each `src/lib/stores/*.ts` file that has a `catch (err)` block (from the earlier grep: `contextTemplates.ts:17`, `settings.ts:56,69`, `pipeline.ts:146`, `recordings.ts:22,54,69,81`, plus any others), replace patterns like:

```typescript
} catch (err) {
  console.error("...", err);
  error = String(err);
}
```

with:

```typescript
import { formatError } from "$lib/types/errors";
// ...
} catch (err) {
  console.error("...", err);
  error = formatError(err);
}
```

`formatError` handles both the new object shape and the old string shape, so the frontend is resilient regardless of migration order.

- [ ] **Step 17: Run frontend type-check and build**

Run: `npm run check` (or whichever script runs `svelte-check` — confirm with `grep '\"check\"' package.json`).
Expected: no new type errors.

Run: `npm run build`
Expected: success.

- [ ] **Step 18: Manually smoke-test an error path in the app**

Run: `npm run tauri dev`, trigger a known failing operation (e.g., open the app with Ollama stopped, then invoke an agent). Verify in the browser devtools console:

Expected: the caught error is an object with `kind` and `message` properties, not just a string. The UI surface (toast/inline error) displays the `message` field correctly.

*If you can't reach a GUI, say so in the PR description — don't claim UI verification you didn't do.*

- [ ] **Step 19: Commit**

```bash
git add src/lib/types/errors.ts src/lib/stores/
git commit -m "feat(frontend): consume structured AppError from Tauri commands

Adds TauriError type and formatError helper. Updates existing stores to
extract the message field from the new error shape while remaining
tolerant of any commands not yet migrated."
```

---

## Verification checklist (run before opening the PR)

- [ ] `cargo build --workspace` — clean build
- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — no new lints
- [ ] `grep -c "gpt-4o" crates/agents/src/orchestrator.rs` → 0 (the hardcode is gone; any remaining matches are in test fixtures only)
- [ ] `grep -c "ProcessingStatus::Failed" src-tauri/src/commands/transcription.rs` → 1 (only in `mark_recording_failed_db_only`)
- [ ] `grep -rn "Result<.*, String>" src-tauri/src/commands/` — no `#[tauri::command]` return signatures match
- [ ] `npm run check` — no new Svelte/TS errors
- [ ] `npm run build` — frontend builds

## Out of scope (intentionally deferred)

- **Incrementing `retry_count`** in `mark_recording_failed`. Current code hardcodes `0` in all six sites; we preserve that behavior. A follow-up can look at the incoming `recording.status` and increment if already `Failed`.
- **Per-call model override** from the frontend. Task 1 settles on "orchestrator uses the settings model." If the UI ever needs to override per-run, add a parameter to `chat_with_agent`; no core type changes required.
- **Richer error taxonomy**. `AppError`'s 16 variants are sufficient for structured dispatch. Finer categories (e.g., splitting `AiProvider` into `AuthFailed` / `RateLimit` / `NetworkDown`) is a larger design conversation.
