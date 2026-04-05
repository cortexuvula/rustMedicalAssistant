# Plan 1: Foundation — Workspace, Core, Database, Security

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundational crates (`core`, `db`, `security`) that every other crate depends on, plus the Cargo workspace scaffold with Tauri and Svelte initialized.

**Architecture:** Multi-crate Cargo workspace. `core` defines shared types, traits, and errors with zero external service dependencies. `db` wraps SQLite (rusqlite) + sqlite-vec + CozoDB with a migration engine. `security` handles AES-256-GCM key storage, PHI redaction, and audit logging. Tauri v2 app shell with Svelte frontend initialized but minimal.

**Tech Stack:** Rust 1.78+, Tauri v2, Svelte 5, TypeScript, rusqlite, cozo, aes-gcm, pbkdf2, sha2, regex, serde, thiserror, chrono, uuid, tracing, r2d2, proptest

---

## File Structure

```
rustMedicalAssistant/
├── Cargo.toml                          (workspace root)
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  (re-exports)
│   │       ├── error.rs                (error hierarchy)
│   │       ├── types/
│   │       │   ├── mod.rs
│   │       │   ├── recording.rs        (Recording, ProcessingStatus)
│   │       │   ├── processing.rs       (BatchOptions, QueueOptions, Priority)
│   │       │   ├── agent.rs            (AgentContext, AgentResponse, ToolDef, ToolOutput)
│   │       │   ├── ai.rs              (CompletionRequest, CompletionResponse, StreamChunk, ModelInfo)
│   │       │   ├── stt.rs             (AudioData, SttConfig, Transcript, TranscriptChunk)
│   │       │   ├── tts.rs             (TtsConfig, VoiceInfo)
│   │       │   ├── rag.rs             (RagResult, ExpandedQuery, SearchConfig)
│   │       │   └── settings.rs        (AppConfig, SoapNoteSettings, AgentSettings, etc.)
│   │       └── traits/
│   │           ├── mod.rs
│   │           ├── ai_provider.rs      (AiProvider trait)
│   │           ├── stt_provider.rs     (SttProvider trait)
│   │           ├── tts_provider.rs     (TtsProvider trait)
│   │           ├── agent.rs            (Agent trait, Tool trait)
│   │           ├── translation.rs      (TranslationProvider trait)
│   │           └── exporter.rs         (Exporter trait)
│   ├── db/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  (Database struct, re-exports)
│   │       ├── pool.rs                 (connection pool via r2d2)
│   │       ├── migrations/
│   │       │   ├── mod.rs              (MigrationEngine)
│   │       │   ├── m001_initial.rs
│   │       │   ├── m002_indexes.rs
│   │       │   ├── m003_fts.rs
│   │       │   ├── m004_metadata.rs
│   │       │   ├── m005_processing_queue.rs
│   │       │   └── m006_recipients.rs
│   │       ├── recordings.rs           (CRUD for recordings table)
│   │       ├── processing_queue.rs     (CRUD for processing_queue + batch tables)
│   │       ├── recipients.rs           (CRUD for saved_recipients)
│   │       ├── settings.rs             (key-value settings table)
│   │       ├── audit.rs                (append-only audit log)
│   │       ├── search.rs               (FTS5 full-text search)
│   │       ├── vectors.rs              (sqlite-vec embeddings)
│   │       └── graph.rs                (CozoDB knowledge graph)
│   └── security/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                  (re-exports)
│           ├── key_storage.rs          (AES-256-GCM encrypted key store)
│           ├── machine_id.rs           (cross-platform machine identifier)
│           ├── phi_redactor.rs         (PHI/PII pattern redaction)
│           ├── audit_logger.rs         (HIPAA-compliant audit logging)
│           ├── input_sanitizer.rs      (prompt injection, XSS prevention)
│           └── rate_limiter.rs         (token bucket rate limiting)
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   └── main.rs                     (minimal Tauri app shell)
│   ├── icons/                          (app icons)
│   └── capabilities/
│       └── default.json
├── src/                                (Svelte frontend)
│   ├── app.html
│   ├── app.css
│   ├── lib/
│   │   └── .gitkeep
│   └── routes/
│       └── +page.svelte
├── package.json
├── svelte.config.js
├── tsconfig.json
├── vite.config.ts
└── tests/                              (integration tests)
    └── .gitkeep
```

---

### Task 1: Initialize Cargo Workspace and Tauri + Svelte App

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`
- Create: `crates/db/Cargo.toml`, `crates/db/src/lib.rs`
- Create: `crates/security/Cargo.toml`, `crates/security/src/lib.rs`
- Create: Tauri + Svelte scaffold via `npm create tauri-app`

- [ ] **Step 1: Install prerequisites**

Run:
```bash
rustup update stable
cargo install create-tauri-app
```
Expected: Tools installed successfully.

- [ ] **Step 2: Initialize Tauri + Svelte project in the repo**

Run:
```bash
cd /home/cortexuvula/Development/rustMedicalAssistant
npm create tauri-app@latest . -- --template svelte-ts --manager npm --yes
```

If the tool doesn't support `.` as target, initialize in a temp dir and move files. The goal is to get the standard Tauri v2 + Svelte + TypeScript scaffold.

Expected: `package.json`, `svelte.config.js`, `vite.config.ts`, `tsconfig.json`, `src-tauri/`, `src/` created.

- [ ] **Step 3: Create workspace root Cargo.toml**

Replace the auto-generated workspace config. The `src-tauri/Cargo.toml` should already exist from step 2.

Write `Cargo.toml` at workspace root:
```toml
[workspace]
resolver = "2"
members = [
    "crates/core",
    "crates/db",
    "crates/security",
    "src-tauri",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
rust-version = "1.78"

[workspace.dependencies]
# Shared dependencies — crates reference these with { workspace = true }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
async-trait = "0.1"
futures-core = "0.3"
```

- [ ] **Step 4: Create core crate skeleton**

Write `crates/core/Cargo.toml`:
```toml
[package]
name = "medical-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
async-trait = { workspace = true }
futures-core = { workspace = true }
tokio = { workspace = true, features = ["sync"] }

[dev-dependencies]
proptest = "1"
```

Write `crates/core/src/lib.rs`:
```rust
pub mod error;
pub mod types;
pub mod traits;
```

- [ ] **Step 5: Create db crate skeleton**

Write `crates/db/Cargo.toml`:
```toml
[package]
name = "medical-db"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
rusqlite = { version = "0.32", features = ["bundled", "vtab", "functions"] }
r2d2 = "0.8"
r2d2_sqlite = "0.25"
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }
tokio = { workspace = true }

[dev-dependencies]
tempfile = "3"
proptest = "1"
```

Write `crates/db/src/lib.rs`:
```rust
pub mod pool;
pub mod migrations;
pub mod recordings;
pub mod processing_queue;
pub mod recipients;
pub mod settings;
pub mod audit;
pub mod search;
pub mod vectors;
pub mod graph;

use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Constraint violation: {0}")]
    Constraint(String),
}

pub type DbResult<T> = Result<T, DbError>;
```

- [ ] **Step 6: Create security crate skeleton**

Write `crates/security/Cargo.toml`:
```toml
[package]
name = "medical-security"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
aes-gcm = "0.10"
pbkdf2 = { version = "0.12", features = ["simple"] }
sha2 = "0.10"
hmac = "0.12"
rand = "0.8"
base64 = "0.22"
regex = "1"
lazy_static = "1"
machine-uid = "0.5"
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
tempfile = "3"
proptest = "1"
```

Write `crates/security/src/lib.rs`:
```rust
pub mod key_storage;
pub mod machine_id;
pub mod phi_redactor;
pub mod audit_logger;
pub mod input_sanitizer;
pub mod rate_limiter;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Decryption error: {0}")]
    Decryption(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid key format")]
    InvalidFormat,
}

pub type SecurityResult<T> = Result<T, SecurityError>;
```

- [ ] **Step 7: Update src-tauri/Cargo.toml to join workspace**

Add workspace membership and local crate dependencies to `src-tauri/Cargo.toml`. The file already exists from the Tauri scaffold — add to the `[dependencies]` section:

```toml
medical-core = { path = "../crates/core" }
medical-db = { path = "../crates/db" }
medical-security = { path = "../crates/security" }
```

- [ ] **Step 8: Verify everything compiles**

Run:
```bash
cd /home/cortexuvula/Development/rustMedicalAssistant
npm install
cargo build 2>&1 | tail -5
```
Expected: Build succeeds (warnings OK, no errors). Some crate modules will be empty — that's fine.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat: initialize Cargo workspace with core, db, security crates and Tauri+Svelte scaffold"
```

---

### Task 2: Core Error Types

**Files:**
- Create: `crates/core/src/error.rs`

- [ ] **Step 1: Write tests for error types**

Write `crates/core/src/error.rs`:
```rust
use thiserror::Error;

/// Top-level application error.
/// Each crate has its own error type; this unifies them at the app boundary.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("AI provider error: {0}")]
    AiProvider(String),

    #[error("STT provider error: {0}")]
    SttProvider(String),

    #[error("TTS provider error: {0}")]
    TtsProvider(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Processing error: {0}")]
    Processing(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error("Translation error: {0}")]
    Translation(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type AppResult<T> = Result<T, AppError>;

/// Severity level for error logging and UI display
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorSeverity {
    Critical,
    Error,
    Warning,
    Info,
}

/// Structured error context for logging and debugging
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorContext {
    pub operation: String,
    pub error: String,
    pub severity: ErrorSeverity,
    pub error_code: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub additional_info: serde_json::Value,
}

impl ErrorContext {
    pub fn new(operation: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            error: error.into(),
            severity: ErrorSeverity::Error,
            error_code: None,
            timestamp: chrono::Utc::now(),
            additional_info: serde_json::Value::Null,
        }
    }

    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_display_formats_correctly() {
        let err = AppError::Database("connection failed".into());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn app_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err, AppError::Io(_)));
        assert!(app_err.to_string().contains("file missing"));
    }

    #[test]
    fn error_context_builder() {
        let ctx = ErrorContext::new("save_recording", "disk full")
            .with_severity(ErrorSeverity::Critical)
            .with_code("DISK_FULL");
        assert_eq!(ctx.operation, "save_recording");
        assert_eq!(ctx.severity, ErrorSeverity::Critical);
        assert_eq!(ctx.error_code.as_deref(), Some("DISK_FULL"));
    }

    #[test]
    fn error_context_serializes_to_json() {
        let ctx = ErrorContext::new("test_op", "test_err");
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["operation"], "test_op");
        assert_eq!(json["error"], "test_err");
        assert!(json["timestamp"].is_string());
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p medical-core`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/error.rs
git commit -m "feat(core): add error hierarchy with AppError, ErrorSeverity, ErrorContext"
```

---

### Task 3: Core Domain Types — Recording and Processing

**Files:**
- Create: `crates/core/src/types/mod.rs`
- Create: `crates/core/src/types/recording.rs`
- Create: `crates/core/src/types/processing.rs`

- [ ] **Step 1: Create types module**

Write `crates/core/src/types/mod.rs`:
```rust
pub mod recording;
pub mod processing;
pub mod agent;
pub mod ai;
pub mod stt;
pub mod tts;
pub mod rag;
pub mod settings;

pub use recording::*;
pub use processing::*;
pub use agent::*;
pub use ai::*;
pub use stt::*;
pub use tts::*;
pub use rag::*;
pub use settings::*;
```

- [ ] **Step 2: Write Recording types with tests**

Write `crates/core/src/types/recording.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: Uuid,
    pub filename: String,
    pub transcript: Option<String>,
    pub soap_note: Option<String>,
    pub referral: Option<String>,
    pub letter: Option<String>,
    pub chat: Option<String>,
    pub patient_name: Option<String>,
    pub audio_path: PathBuf,
    pub duration_seconds: Option<f64>,
    pub file_size_bytes: Option<u64>,
    pub stt_provider: Option<String>,
    pub ai_provider: Option<String>,
    pub tags: Vec<String>,
    pub status: ProcessingStatus,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl Recording {
    pub fn new(filename: String, audio_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            filename,
            transcript: None,
            soap_note: None,
            referral: None,
            letter: None,
            chat: None,
            patient_name: None,
            audio_path,
            duration_seconds: None,
            file_size_bytes: None,
            stt_provider: None,
            ai_provider: None,
            tags: Vec::new(),
            status: ProcessingStatus::Pending,
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn is_processed(&self) -> bool {
        matches!(self.status, ProcessingStatus::Completed { .. })
    }

    pub fn has_transcript(&self) -> bool {
        self.transcript.as_ref().is_some_and(|t| !t.is_empty())
    }

    pub fn has_soap_note(&self) -> bool {
        self.soap_note.as_ref().is_some_and(|s| !s.is_empty())
    }
}

/// Processing status with state data encoded in variants.
/// Invalid states (e.g., "completed but still processing") are unrepresentable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProcessingStatus {
    Pending,
    Processing {
        started_at: DateTime<Utc>,
    },
    Completed {
        completed_at: DateTime<Utc>,
    },
    Failed {
        error: String,
        retry_count: u32,
    },
}

impl ProcessingStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. })
    }

    pub fn can_retry(&self) -> bool {
        match self {
            Self::Failed { retry_count, .. } => *retry_count < 3,
            _ => false,
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing { .. } => "processing",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
        }
    }
}

/// Summary view of a recording for list displays
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSummary {
    pub id: Uuid,
    pub filename: String,
    pub patient_name: Option<String>,
    pub status: ProcessingStatus,
    pub duration_seconds: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub has_transcript: bool,
    pub has_soap_note: bool,
    pub has_referral: bool,
    pub has_letter: bool,
}

impl From<&Recording> for RecordingSummary {
    fn from(r: &Recording) -> Self {
        Self {
            id: r.id,
            filename: r.filename.clone(),
            patient_name: r.patient_name.clone(),
            status: r.status.clone(),
            duration_seconds: r.duration_seconds,
            created_at: r.created_at,
            tags: r.tags.clone(),
            has_transcript: r.has_transcript(),
            has_soap_note: r.has_soap_note(),
            has_referral: r.referral.as_ref().is_some_and(|s| !s.is_empty()),
            has_letter: r.letter.as_ref().is_some_and(|s| !s.is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_recording_starts_pending() {
        let rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        assert!(matches!(rec.status, ProcessingStatus::Pending));
        assert!(!rec.is_processed());
        assert!(!rec.has_transcript());
        assert!(!rec.has_soap_note());
    }

    #[test]
    fn processing_status_terminal_states() {
        assert!(!ProcessingStatus::Pending.is_terminal());
        assert!(!ProcessingStatus::Processing { started_at: Utc::now() }.is_terminal());
        assert!(ProcessingStatus::Completed { completed_at: Utc::now() }.is_terminal());
        assert!(ProcessingStatus::Failed { error: "err".into(), retry_count: 0 }.is_terminal());
    }

    #[test]
    fn processing_status_retry_logic() {
        let can = ProcessingStatus::Failed { error: "err".into(), retry_count: 2 };
        assert!(can.can_retry());
        let cannot = ProcessingStatus::Failed { error: "err".into(), retry_count: 3 };
        assert!(!cannot.can_retry());
        assert!(!ProcessingStatus::Pending.can_retry());
    }

    #[test]
    fn processing_status_serializes_with_tag() {
        let status = ProcessingStatus::Failed {
            error: "timeout".into(),
            retry_count: 1,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error"], "timeout");
        assert_eq!(json["retry_count"], 1);
    }

    #[test]
    fn recording_summary_from_recording() {
        let mut rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        rec.transcript = Some("Patient presents with...".into());
        rec.soap_note = Some("S: ...".into());
        let summary = RecordingSummary::from(&rec);
        assert!(summary.has_transcript);
        assert!(summary.has_soap_note);
        assert!(!summary.has_referral);
    }

    #[test]
    fn processing_status_labels() {
        assert_eq!(ProcessingStatus::Pending.status_label(), "pending");
        assert_eq!(
            ProcessingStatus::Processing { started_at: Utc::now() }.status_label(),
            "processing"
        );
    }
}
```

- [ ] **Step 3: Write Processing types with tests**

Write `crates/core/src/types/processing.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

impl Priority {
    pub fn as_i32(&self) -> i32 {
        match self {
            Self::Low => -1,
            Self::Normal => 0,
            Self::High => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Transcribe,
    GenerateSoap,
    GenerateReferral,
    GenerateLetter,
    ExtractData,
    IndexRag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueTask {
    pub id: Uuid,
    pub recording_id: Uuid,
    pub task_type: TaskType,
    pub priority: Priority,
    pub status: QueueTaskStatus,
    pub created_at: DateTime<Utc>,
    pub batch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QueueTaskStatus {
    Pending,
    Processing { started_at: DateTime<Utc> },
    Completed { completed_at: DateTime<Utc>, result: Option<String> },
    Failed { error: String, error_count: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProcessingOptions {
    pub generate_soap: bool,
    pub generate_referral: bool,
    pub generate_letter: bool,
    pub skip_existing: bool,
    pub continue_on_error: bool,
    pub priority: Priority,
    pub max_concurrent: usize,
}

impl Default for BatchProcessingOptions {
    fn default() -> Self {
        Self {
            generate_soap: true,
            generate_referral: false,
            generate_letter: false,
            skip_existing: true,
            continue_on_error: true,
            priority: Priority::Normal,
            max_concurrent: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatus {
    pub batch_id: Uuid,
    pub total_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub status: BatchState,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Pending,
    Processing,
    Completed,
    PartiallyCompleted,
    Failed,
}

/// Event emitted to the frontend during processing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessingEvent {
    StepChanged { recording_id: Uuid, step: String },
    Progress { recording_id: Uuid, percent: f32 },
    Completed { recording_id: Uuid },
    Failed { recording_id: Uuid, error: String },
    QueueStatus { pending: usize, processing: usize, completed: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        assert!(Priority::Low.as_i32() < Priority::Normal.as_i32());
        assert!(Priority::Normal.as_i32() < Priority::High.as_i32());
    }

    #[test]
    fn batch_options_defaults() {
        let opts = BatchProcessingOptions::default();
        assert!(opts.generate_soap);
        assert!(!opts.generate_referral);
        assert!(opts.skip_existing);
        assert_eq!(opts.max_concurrent, 3);
    }

    #[test]
    fn queue_task_status_serializes_with_tag() {
        let status = QueueTaskStatus::Failed {
            error: "timeout".into(),
            error_count: 2,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error_count"], 2);
    }

    #[test]
    fn processing_event_serializes_with_tag() {
        let event = ProcessingEvent::Progress {
            recording_id: Uuid::new_v4(),
            percent: 0.75,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "progress");
        assert_eq!(json["percent"], 0.75);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p medical-core`
Expected: All tests pass (previous error tests + new recording/processing tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/types/
git commit -m "feat(core): add Recording, ProcessingStatus, QueueTask, BatchProcessingOptions types"
```

---

### Task 4: Core Domain Types — AI, STT, TTS, RAG, Agent

**Files:**
- Create: `crates/core/src/types/ai.rs`
- Create: `crates/core/src/types/stt.rs`
- Create: `crates/core/src/types/tts.rs`
- Create: `crates/core/src/types/rag.rs`
- Create: `crates/core/src/types/agent.rs`

- [ ] **Step 1: Write AI types**

Write `crates/core/src/types/ai.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub max_tokens: u32,
    pub supports_tools: bool,
    pub supports_streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    ToolResult { tool_call_id: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: UsageInfo,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Chunks emitted during streaming completions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamChunk {
    Delta { text: String },
    ToolCallDelta { id: String, name: Option<String>, arguments_delta: String },
    Usage(UsageInfo),
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCompletionResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: UsageInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_content_text_serializes() {
        let msg = Message {
            role: Role::User,
            content: MessageContent::Text("Hello".into()),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn stream_chunk_serializes_with_tag() {
        let chunk = StreamChunk::Delta { text: "Hello".into() };
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["type"], "delta");
        assert_eq!(json["text"], "Hello");
    }
}
```

- [ ] **Step 2: Write STT types**

Write `crates/core/src/types/stt.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioData {
    pub fn from_file_path(path: PathBuf) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate: 44100,
            channels: 1,
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (self.sample_rate as f64 * self.channels as f64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub language: String,
    pub diarize: bool,
    pub num_speakers: Option<u32>,
    pub model: Option<String>,
    pub smart_formatting: bool,
    pub profanity_filter: bool,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            language: "en-US".into(),
            diarize: false,
            num_speakers: None,
            model: None,
            smart_formatting: true,
            profanity_filter: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
    pub language: Option<String>,
    pub duration_seconds: Option<f64>,
    pub provider: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptChunk {
    pub text: String,
    pub is_final: bool,
    pub speaker: Option<String>,
}

/// Placeholder for audio stream (will be a channel receiver in audio crate)
pub type AudioStream = tokio::sync::mpsc::Receiver<Vec<f32>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_data_duration() {
        let data = AudioData {
            samples: vec![0.0; 44100],
            sample_rate: 44100,
            channels: 1,
        };
        assert!((data.duration_seconds() - 1.0).abs() < 0.001);
    }

    #[test]
    fn audio_data_duration_zero_rate() {
        let data = AudioData { samples: vec![0.0; 100], sample_rate: 0, channels: 1 };
        assert_eq!(data.duration_seconds(), 0.0);
    }

    #[test]
    fn stt_config_defaults() {
        let config = SttConfig::default();
        assert_eq!(config.language, "en-US");
        assert!(!config.diarize);
        assert!(config.smart_formatting);
    }
}
```

- [ ] **Step 3: Write TTS types**

Write `crates/core/src/types/tts.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub voice: String,
    pub language: Option<String>,
    pub speed: f32,
    pub volume: f32,
    pub model: Option<String>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            voice: "default".into(),
            language: None,
            speed: 1.0,
            volume: 1.0,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
    pub gender: Option<String>,
    pub preview_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_config_defaults() {
        let config = TtsConfig::default();
        assert_eq!(config.speed, 1.0);
        assert_eq!(config.volume, 1.0);
    }
}
```

- [ ] **Step 4: Write RAG types**

Write `crates/core/src/types/rag.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagResult {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub content: String,
    pub score: f32,
    pub source: SearchSource,
    pub metadata: RagChunkMetadata,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    Vector,
    Bm25,
    Graph,
    Fused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagChunkMetadata {
    pub document_title: Option<String>,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub page_number: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub top_k: usize,
    pub similarity_threshold: f32,
    pub mmr_lambda: f32,
    pub enable_vector: bool,
    pub enable_bm25: bool,
    pub enable_graph: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            similarity_threshold: 0.75,
            mmr_lambda: 0.7,
            enable_vector: true,
            enable_bm25: true,
            enable_graph: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpandedQuery {
    pub original: String,
    pub expanded_terms: Vec<String>,
    pub full_query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub content: String,
    pub embedding: Vec<f32>,
    pub chunk_index: u32,
    pub metadata: RagChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEntity {
    pub id: String,
    pub entity_type: EntityType,
    pub name: String,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Drug,
    Condition,
    Procedure,
    Symptom,
    LabTest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRelation {
    pub from: String,
    pub to: String,
    pub relation_type: RelationType,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Treats,
    Contraindicates,
    Causes,
    Diagnoses,
    Indicates,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_config_defaults() {
        let config = SearchConfig::default();
        assert_eq!(config.top_k, 5);
        assert_eq!(config.similarity_threshold, 0.75);
        assert!(config.enable_vector);
        assert!(config.enable_bm25);
        assert!(config.enable_graph);
    }

    #[test]
    fn entity_type_serializes() {
        let json = serde_json::to_value(EntityType::Drug).unwrap();
        assert_eq!(json, "drug");
    }

    #[test]
    fn relation_type_serializes() {
        let json = serde_json::to_value(RelationType::Contraindicates).unwrap();
        assert_eq!(json, "contraindicates");
    }
}
```

- [ ] **Step 5: Write Agent types**

Write `crates/core/src/types/agent.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::ai::{Message, UsageInfo};
use super::rag::RagResult;
use super::recording::Recording;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn success(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: false }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: true }
    }
}

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub user_message: String,
    pub conversation_history: Vec<Message>,
    pub patient_context: Option<PatientContext>,
    pub rag_context: Vec<RagResult>,
    pub recording: Option<Recording>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientContext {
    pub patient_name: Option<String>,
    pub prior_soap_notes: Vec<String>,
    pub medications: Vec<String>,
    pub conditions: Vec<String>,
    pub allergies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
    pub tool_calls_made: Vec<AgentToolCallRecord>,
    pub usage: UsageInfo,
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCallRecord {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: String,
    pub duration_ms: u64,
}

/// Agent configuration for settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub system_prompt: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openai".into(),
            model: "gpt-4o".into(),
            temperature: 0.3,
            max_tokens: 4000,
            system_prompt: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_success() {
        let output = ToolOutput::success("result");
        assert!(!output.is_error);
        assert_eq!(output.content, "result");
    }

    #[test]
    fn tool_output_error() {
        let output = ToolOutput::error("something failed");
        assert!(output.is_error);
    }

    #[test]
    fn agent_settings_defaults() {
        let settings = AgentSettings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.temperature, 0.3);
        assert_eq!(settings.max_tokens, 4000);
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p medical-core`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/types/
git commit -m "feat(core): add AI, STT, TTS, RAG, and Agent domain types"
```

---

### Task 5: Core Domain Types — Settings (AppConfig)

**Files:**
- Create: `crates/core/src/types/settings.rs`

- [ ] **Step 1: Write AppConfig with tests**

Write `crates/core/src/types/settings.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::agent::AgentSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Light
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IcdVersion {
    Icd9,
    Icd10,
    Both,
}

impl Default for IcdVersion {
    fn default() -> Self {
        Self::Icd9
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoapTemplate {
    FollowUp,
    NewPatient,
    Telehealth,
    Emergency,
    Pediatric,
    Geriatric,
}

impl Default for SoapTemplate {
    fn default() -> Self {
        Self::FollowUp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // General
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_language")]
    pub language: String,
    pub storage_path: Option<String>,

    // Audio
    pub input_device: Option<String>,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_channels")]
    pub channels: u16,

    // Providers
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    #[serde(default = "default_stt_provider")]
    pub stt_provider: String,
    #[serde(default)]
    pub stt_failover_chain: Vec<String>,
    #[serde(default = "default_tts_provider")]
    pub tts_provider: String,
    #[serde(default = "default_tts_voice")]
    pub tts_voice: String,

    // Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    // Processing
    #[serde(default)]
    pub auto_generate_referral: bool,
    #[serde(default)]
    pub auto_generate_letter: bool,
    #[serde(default = "default_true")]
    pub auto_index_rag: bool,
    #[serde(default)]
    pub icd_version: IcdVersion,

    // Templates
    #[serde(default)]
    pub soap_template: SoapTemplate,
    pub custom_soap_prompt: Option<String>,
    pub custom_referral_prompt: Option<String>,
    pub custom_letter_prompt: Option<String>,

    // RAG
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_top_k")]
    pub search_top_k: usize,
    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f32,

    // Autosave
    #[serde(default = "default_true")]
    pub autosave_enabled: bool,
    #[serde(default = "default_autosave_interval")]
    pub autosave_interval_secs: u64,

    // Features
    #[serde(default = "default_true")]
    pub quick_continue_mode: bool,
    #[serde(default = "default_max_workers")]
    pub max_background_workers: usize,
    #[serde(default = "default_true")]
    pub show_processing_notifications: bool,
    #[serde(default = "default_true")]
    pub auto_retry_failed: bool,
    #[serde(default = "default_max_retries")]
    pub max_retry_attempts: u32,

    // Window
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,

    // Per-provider model settings
    #[serde(default)]
    pub soap_note_settings: SoapNoteSettings,

    // Agent settings
    #[serde(default)]
    pub agent_settings: std::collections::HashMap<String, AgentSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapNoteSettings {
    #[serde(default = "default_ai_model")]
    pub openai_model: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    #[serde(default = "default_anthropic_model")]
    pub anthropic_model: String,
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,
    #[serde(default = "default_groq_model")]
    pub groq_model: String,
    #[serde(default = "default_cerebras_model")]
    pub cerebras_model: String,
    #[serde(default)]
    pub icd_code_version: IcdVersion,
    pub system_message: Option<String>,
}

impl Default for SoapNoteSettings {
    fn default() -> Self {
        Self {
            openai_model: default_ai_model(),
            ollama_model: default_ollama_model(),
            anthropic_model: default_anthropic_model(),
            gemini_model: default_gemini_model(),
            groq_model: default_groq_model(),
            cerebras_model: default_cerebras_model(),
            icd_code_version: IcdVersion::default(),
            system_message: None,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        serde_json::from_str("{}").expect("default AppConfig must deserialize from empty object")
    }
}

fn default_language() -> String { "en-US".into() }
fn default_sample_rate() -> u32 { 44100 }
fn default_channels() -> u16 { 1 }
fn default_ai_provider() -> String { "openai".into() }
fn default_ai_model() -> String { "gpt-4o".into() }
fn default_stt_provider() -> String { "groq".into() }
fn default_tts_provider() -> String { "elevenlabs".into() }
fn default_tts_voice() -> String { "default".into() }
fn default_temperature() -> f32 { 0.4 }
fn default_true() -> bool { true }
fn default_embedding_model() -> String { "text-embedding-3-small".into() }
fn default_top_k() -> usize { 5 }
fn default_mmr_lambda() -> f32 { 0.7 }
fn default_autosave_interval() -> u64 { 60 }
fn default_max_workers() -> usize { 2 }
fn default_max_retries() -> u32 { 3 }
fn default_window_width() -> u32 { 1200 }
fn default_window_height() -> u32 { 800 }
fn default_ollama_model() -> String { "llama3".into() }
fn default_anthropic_model() -> String { "claude-sonnet-4-20250514".into() }
fn default_gemini_model() -> String { "gemini-2.0-flash".into() }
fn default_groq_model() -> String { "llama-3.3-70b-versatile".into() }
fn default_cerebras_model() -> String { "llama-3.3-70b".into() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = AppConfig::default();
        assert_eq!(config.ai_provider, "openai");
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 1);
        assert_eq!(config.search_top_k, 5);
        assert!(config.autosave_enabled);
        assert!(config.quick_continue_mode);
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ai_provider, config.ai_provider);
        assert_eq!(restored.sample_rate, config.sample_rate);
    }

    #[test]
    fn config_deserializes_from_partial_json() {
        let json = r#"{"ai_provider": "anthropic", "temperature": 0.8}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.ai_provider, "anthropic");
        assert_eq!(config.temperature, 0.8);
        // Defaults fill in the rest
        assert_eq!(config.sample_rate, 44100);
        assert!(config.autosave_enabled);
    }

    #[test]
    fn theme_serializes_snake_case() {
        assert_eq!(serde_json::to_value(Theme::Dark).unwrap(), "dark");
        assert_eq!(serde_json::to_value(Theme::Light).unwrap(), "light");
    }

    #[test]
    fn icd_version_serializes() {
        assert_eq!(serde_json::to_value(IcdVersion::Both).unwrap(), "both");
    }

    #[test]
    fn soap_template_serializes() {
        assert_eq!(serde_json::to_value(SoapTemplate::Telehealth).unwrap(), "telehealth");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-core`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/types/settings.rs
git commit -m "feat(core): add AppConfig with full settings model and serde defaults"
```

---

### Task 6: Core Traits

**Files:**
- Create: `crates/core/src/traits/mod.rs`
- Create: `crates/core/src/traits/ai_provider.rs`
- Create: `crates/core/src/traits/stt_provider.rs`
- Create: `crates/core/src/traits/tts_provider.rs`
- Create: `crates/core/src/traits/agent.rs`
- Create: `crates/core/src/traits/translation.rs`
- Create: `crates/core/src/traits/exporter.rs`

- [ ] **Step 1: Write all trait definitions**

Write `crates/core/src/traits/mod.rs`:
```rust
pub mod ai_provider;
pub mod stt_provider;
pub mod tts_provider;
pub mod agent;
pub mod translation;
pub mod exporter;

pub use ai_provider::AiProvider;
pub use stt_provider::SttProvider;
pub use tts_provider::TtsProvider;
pub use agent::{Agent, Tool};
pub use translation::TranslationProvider;
pub use exporter::Exporter;
```

Write `crates/core/src/traits/ai_provider.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use std::pin::Pin;
use crate::error::AppResult;
use crate::types::ai::*;

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    fn available_models(&self) -> Vec<ModelInfo>;
    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse>;
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>>;
    async fn complete_with_tools(
        &self,
        request: CompletionRequest,
        tools: &[ToolDef],
    ) -> AppResult<ToolCompletionResponse>;
}
```

Write `crates/core/src/traits/stt_provider.rs`:
```rust
use async_trait::async_trait;
use futures_core::Stream;
use std::pin::Pin;
use crate::error::AppResult;
use crate::types::stt::*;

#[async_trait]
pub trait SttProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supports_streaming(&self) -> bool;
    fn supports_diarization(&self) -> bool;
    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript>;
    async fn transcribe_stream(
        &self,
        stream: AudioStream,
        config: SttConfig,
    ) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send>>>;
}
```

Write `crates/core/src/traits/tts_provider.rs`:
```rust
use async_trait::async_trait;
use crate::error::AppResult;
use crate::types::tts::*;
use crate::types::stt::AudioData;

#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn name(&self) -> &str;
    fn available_voices(&self) -> Vec<VoiceInfo>;
    async fn synthesize(&self, text: &str, config: TtsConfig) -> AppResult<AudioData>;
}
```

Write `crates/core/src/traits/agent.rs`:
```rust
use async_trait::async_trait;
use crate::error::AppResult;
use crate::types::agent::*;

#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn system_prompt(&self) -> &str;
    fn available_tools(&self) -> Vec<ToolDef>;
    async fn execute(&self, context: AgentContext) -> AppResult<AgentResponse>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDef;
    async fn execute(&self, params: serde_json::Value) -> AppResult<ToolOutput>;
}
```

Write `crates/core/src/traits/translation.rs`:
```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Language {
    pub code: String,
    pub name: String,
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_languages(&self) -> Vec<Language>;
    async fn translate(&self, text: &str, from: &Language, to: &Language) -> AppResult<String>;
    async fn detect_language(&self, text: &str) -> AppResult<Language>;
}
```

Write `crates/core/src/traits/exporter.rs`:
```rust
use crate::error::AppResult;
use crate::types::recording::Recording;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Pdf,
    Docx,
    FhirBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub include_transcript: bool,
    pub include_soap_note: bool,
    pub include_referral: bool,
    pub include_letter: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            include_transcript: true,
            include_soap_note: true,
            include_referral: true,
            include_letter: true,
        }
    }
}

pub trait Exporter: Send + Sync {
    fn format(&self) -> ExportFormat;
    fn export(&self, recording: &Recording, config: &ExportConfig) -> AppResult<Vec<u8>>;
}
```

- [ ] **Step 2: Update lib.rs re-exports**

Update `crates/core/src/lib.rs`:
```rust
pub mod error;
pub mod types;
pub mod traits;

// Convenience re-exports
pub use error::{AppError, AppResult, ErrorSeverity, ErrorContext};
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p medical-core`
Expected: Compiles without errors.

- [ ] **Step 4: Run all core tests**

Run: `cargo test -p medical-core`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/
git commit -m "feat(core): add provider traits (AiProvider, SttProvider, TtsProvider, Agent, Tool, TranslationProvider, Exporter)"
```

---

### Task 7: Database — Connection Pool and Migration Engine

**Files:**
- Create: `crates/db/src/pool.rs`
- Create: `crates/db/src/migrations/mod.rs`
- Create: `crates/db/src/migrations/m001_initial.rs`

- [ ] **Step 1: Write connection pool**

Write `crates/db/src/pool.rs`:
```rust
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;
use crate::DbResult;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn create_pool(db_path: &Path) -> DbResult<DbPool> {
    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn| {
            // Enable WAL mode for concurrent reads
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=5000;"
            )?;
            Ok(())
        });

    let pool = Pool::builder()
        .max_size(8)
        .build(manager)?;

    Ok(pool)
}

pub fn create_memory_pool() -> DbResult<DbPool> {
    let manager = SqliteConnectionManager::memory()
        .with_init(|conn| {
            conn.execute_batch("PRAGMA foreign_keys=ON;")?;
            Ok(())
        });

    let pool = Pool::builder()
        .max_size(1)  // in-memory DBs can't share across connections
        .build(manager)?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_pool_creates_and_connects() {
        let pool = create_memory_pool().unwrap();
        let conn = pool.get().unwrap();
        let result: i64 = conn.query_row("SELECT 1", [], |row| row.get(0)).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn memory_pool_has_foreign_keys_enabled() {
        let pool = create_memory_pool().unwrap();
        let conn = pool.get().unwrap();
        let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn file_pool_creates_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path).unwrap();
        let conn = pool.get().unwrap();
        let wal: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(wal, "wal");
    }
}
```

- [ ] **Step 2: Write migration engine**

Write `crates/db/src/migrations/mod.rs`:
```rust
pub mod m001_initial;

use rusqlite::Connection;
use tracing::{info, warn};
use crate::{DbError, DbResult};

pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub up: fn(&Connection) -> DbResult<()>,
}

/// All migrations in order. Add new migrations to the end.
pub fn all_migrations() -> &'static [Migration] {
    &[
        Migration { version: 1, name: "initial_schema", up: m001_initial::up },
    ]
}

pub struct MigrationEngine;

impl MigrationEngine {
    /// Run all pending migrations. Returns count of migrations applied.
    pub fn migrate(conn: &Connection) -> DbResult<u32> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"
        ).map_err(|e| DbError::Migration(e.to_string()))?;

        let current = Self::current_version(conn)?;
        let mut applied = 0u32;

        for migration in all_migrations() {
            if migration.version > current {
                info!(version = migration.version, name = migration.name, "Applying migration");
                (migration.up)(conn)?;
                conn.execute(
                    "INSERT INTO schema_version (version, name) VALUES (?1, ?2)",
                    rusqlite::params![migration.version, migration.name],
                ).map_err(|e| DbError::Migration(e.to_string()))?;
                applied += 1;
            }
        }

        if applied > 0 {
            info!(applied, "Migrations complete");
        }

        Ok(applied)
    }

    pub fn current_version(conn: &Connection) -> DbResult<u32> {
        // Table might not exist yet
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| DbError::Migration(e.to_string()))?;

        if !exists {
            return Ok(0);
        }

        let version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .map_err(|e| DbError::Migration(e.to_string()))?;

        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn
    }

    #[test]
    fn fresh_db_has_version_zero() {
        let conn = memory_conn();
        assert_eq!(MigrationEngine::current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn migrate_applies_all_pending() {
        let conn = memory_conn();
        let applied = MigrationEngine::migrate(&conn).unwrap();
        assert!(applied > 0);
        assert_eq!(
            MigrationEngine::current_version(&conn).unwrap(),
            all_migrations().last().unwrap().version
        );
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = memory_conn();
        MigrationEngine::migrate(&conn).unwrap();
        let applied = MigrationEngine::migrate(&conn).unwrap();
        assert_eq!(applied, 0);
    }

    #[test]
    fn schema_version_table_tracks_migrations() {
        let conn = memory_conn();
        MigrationEngine::migrate(&conn).unwrap();
        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count as usize, all_migrations().len());
    }
}
```

- [ ] **Step 3: Write initial migration (m001)**

Write `crates/db/src/migrations/m001_initial.rs`:
```rust
use rusqlite::Connection;
use crate::{DbError, DbResult};

pub fn up(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS recordings (
            id TEXT PRIMARY KEY NOT NULL,
            filename TEXT NOT NULL,
            transcript TEXT,
            soap_note TEXT,
            referral TEXT,
            letter TEXT,
            chat TEXT,
            patient_name TEXT,
            audio_path TEXT,
            duration_seconds REAL,
            file_size_bytes INTEGER,
            stt_provider TEXT,
            ai_provider TEXT,
            tags TEXT,
            processing_status TEXT NOT NULL DEFAULT '\"pending\"',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            metadata TEXT DEFAULT 'null'
        );

        CREATE INDEX IF NOT EXISTS idx_recordings_created_at ON recordings(created_at);
        CREATE INDEX IF NOT EXISTS idx_recordings_filename ON recordings(filename);
        CREATE INDEX IF NOT EXISTS idx_recordings_status ON recordings(processing_status);

        CREATE VIRTUAL TABLE IF NOT EXISTS recordings_fts USING fts5(
            filename, transcript, soap_note, referral, letter, patient_name,
            content='recordings',
            content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS recordings_ai AFTER INSERT ON recordings BEGIN
            INSERT INTO recordings_fts(rowid, filename, transcript, soap_note, referral, letter, patient_name)
            VALUES (new.rowid, new.filename, new.transcript, new.soap_note, new.referral, new.letter, new.patient_name);
        END;

        CREATE TRIGGER IF NOT EXISTS recordings_ad AFTER DELETE ON recordings BEGIN
            INSERT INTO recordings_fts(recordings_fts, rowid, filename, transcript, soap_note, referral, letter, patient_name)
            VALUES ('delete', old.rowid, old.filename, old.transcript, old.soap_note, old.referral, old.letter, old.patient_name);
        END;

        CREATE TRIGGER IF NOT EXISTS recordings_au AFTER UPDATE ON recordings BEGIN
            INSERT INTO recordings_fts(recordings_fts, rowid, filename, transcript, soap_note, referral, letter, patient_name)
            VALUES ('delete', old.rowid, old.filename, old.transcript, old.soap_note, old.referral, old.letter, old.patient_name);
            INSERT INTO recordings_fts(rowid, filename, transcript, soap_note, referral, letter, patient_name)
            VALUES (new.rowid, new.filename, new.transcript, new.soap_note, new.referral, new.letter, new.patient_name);
        END;

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            action TEXT NOT NULL,
            actor TEXT NOT NULL DEFAULT 'system',
            resource TEXT,
            details TEXT
        );

        -- Prevent modifications to audit log
        CREATE TRIGGER IF NOT EXISTS audit_no_update BEFORE UPDATE ON audit_log BEGIN
            SELECT RAISE(ABORT, 'audit_log is append-only: updates are not allowed');
        END;

        CREATE TRIGGER IF NOT EXISTS audit_no_delete BEFORE DELETE ON audit_log BEGIN
            SELECT RAISE(ABORT, 'audit_log is append-only: deletes are not allowed');
        END;

        CREATE TABLE IF NOT EXISTS saved_recipients (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            title TEXT,
            specialty TEXT,
            organization TEXT,
            address TEXT,
            city TEXT,
            state TEXT,
            postal_code TEXT,
            phone TEXT,
            fax TEXT,
            email TEXT,
            notes TEXT,
            category TEXT DEFAULT 'general',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS processing_queue (
            id TEXT PRIMARY KEY NOT NULL,
            recording_id TEXT NOT NULL,
            task_type TEXT NOT NULL,
            priority INTEGER DEFAULT 0,
            status TEXT NOT NULL DEFAULT '\"pending\"',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            started_at TEXT,
            completed_at TEXT,
            error_count INTEGER DEFAULT 0,
            last_error TEXT,
            result TEXT,
            batch_id TEXT,
            FOREIGN KEY (recording_id) REFERENCES recordings(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS batch_processing (
            id TEXT PRIMARY KEY NOT NULL,
            total_count INTEGER NOT NULL,
            completed_count INTEGER DEFAULT 0,
            failed_count INTEGER DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            started_at TEXT,
            completed_at TEXT,
            status TEXT NOT NULL DEFAULT 'pending'
        );"
    ).map_err(|e| DbError::Migration(format!("m001_initial: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        up(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(tables.contains(&"recordings".to_string()));
        assert!(tables.contains(&"settings".to_string()));
        assert!(tables.contains(&"audit_log".to_string()));
        assert!(tables.contains(&"saved_recipients".to_string()));
        assert!(tables.contains(&"processing_queue".to_string()));
        assert!(tables.contains(&"batch_processing".to_string()));
    }

    #[test]
    fn audit_log_rejects_updates() {
        let conn = Connection::open_in_memory().unwrap();
        up(&conn).unwrap();
        conn.execute(
            "INSERT INTO audit_log (action, resource) VALUES ('test', 'res')",
            [],
        ).unwrap();
        let result = conn.execute(
            "UPDATE audit_log SET action='modified' WHERE id=1",
            [],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("append-only"));
    }

    #[test]
    fn audit_log_rejects_deletes() {
        let conn = Connection::open_in_memory().unwrap();
        up(&conn).unwrap();
        conn.execute(
            "INSERT INTO audit_log (action, resource) VALUES ('test', 'res')",
            [],
        ).unwrap();
        let result = conn.execute("DELETE FROM audit_log WHERE id=1", []);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("append-only"));
    }

    #[test]
    fn fts_table_created() {
        let conn = Connection::open_in_memory().unwrap();
        up(&conn).unwrap();
        // FTS tables show up as tables in sqlite_master
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='recordings_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count > 0);
    }
}
```

- [ ] **Step 4: Run all db tests**

Run: `cargo test -p medical-db`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/
git commit -m "feat(db): add connection pool, migration engine, and initial schema (recordings, FTS5, audit_log, queue)"
```

---

### Task 8: Database — Recordings CRUD

**Files:**
- Create: `crates/db/src/recordings.rs`

- [ ] **Step 1: Write recordings CRUD with tests**

Write `crates/db/src/recordings.rs`:
```rust
use medical_core::types::recording::{Recording, ProcessingStatus, RecordingSummary};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;
use crate::{DbError, DbResult};

pub struct RecordingsRepo;

impl RecordingsRepo {
    pub fn insert(conn: &Connection, recording: &Recording) -> DbResult<()> {
        let status_json = serde_json::to_string(&recording.status)
            .map_err(|e| DbError::Constraint(e.to_string()))?;
        let tags_json = serde_json::to_string(&recording.tags)
            .map_err(|e| DbError::Constraint(e.to_string()))?;
        let metadata_json = serde_json::to_string(&recording.metadata)
            .map_err(|e| DbError::Constraint(e.to_string()))?;

        conn.execute(
            "INSERT INTO recordings (
                id, filename, transcript, soap_note, referral, letter, chat,
                patient_name, audio_path, duration_seconds, file_size_bytes,
                stt_provider, ai_provider, tags, processing_status, created_at, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                recording.id.to_string(),
                recording.filename,
                recording.transcript,
                recording.soap_note,
                recording.referral,
                recording.letter,
                recording.chat,
                recording.patient_name,
                recording.audio_path.to_string_lossy().to_string(),
                recording.duration_seconds,
                recording.file_size_bytes.map(|v| v as i64),
                recording.stt_provider,
                recording.ai_provider,
                tags_json,
                status_json,
                recording.created_at.to_rfc3339(),
                metadata_json,
            ],
        )?;

        Ok(())
    }

    pub fn get_by_id(conn: &Connection, id: &Uuid) -> DbResult<Recording> {
        conn.query_row(
            "SELECT * FROM recordings WHERE id = ?1",
            params![id.to_string()],
            Self::row_to_recording,
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(format!("Recording {id}")),
            other => DbError::Sqlite(other),
        })
    }

    pub fn list_all(conn: &Connection, limit: u32, offset: u32) -> DbResult<Vec<RecordingSummary>> {
        let mut stmt = conn.prepare(
            "SELECT * FROM recordings ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        )?;

        let recordings = stmt
            .query_map(params![limit, offset], Self::row_to_recording)?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .map(RecordingSummary::from)
            .collect();

        Ok(recordings)
    }

    pub fn update(conn: &Connection, recording: &Recording) -> DbResult<()> {
        let status_json = serde_json::to_string(&recording.status)
            .map_err(|e| DbError::Constraint(e.to_string()))?;
        let tags_json = serde_json::to_string(&recording.tags)
            .map_err(|e| DbError::Constraint(e.to_string()))?;
        let metadata_json = serde_json::to_string(&recording.metadata)
            .map_err(|e| DbError::Constraint(e.to_string()))?;

        let rows = conn.execute(
            "UPDATE recordings SET
                filename = ?2, transcript = ?3, soap_note = ?4,
                referral = ?5, letter = ?6, chat = ?7,
                patient_name = ?8, audio_path = ?9,
                duration_seconds = ?10, file_size_bytes = ?11,
                stt_provider = ?12, ai_provider = ?13,
                tags = ?14, processing_status = ?15, metadata = ?16
            WHERE id = ?1",
            params![
                recording.id.to_string(),
                recording.filename,
                recording.transcript,
                recording.soap_note,
                recording.referral,
                recording.letter,
                recording.chat,
                recording.patient_name,
                recording.audio_path.to_string_lossy().to_string(),
                recording.duration_seconds,
                recording.file_size_bytes.map(|v| v as i64),
                recording.stt_provider,
                recording.ai_provider,
                tags_json,
                status_json,
                metadata_json,
            ],
        )?;

        if rows == 0 {
            return Err(DbError::NotFound(format!("Recording {}", recording.id)));
        }

        Ok(())
    }

    pub fn delete(conn: &Connection, id: &Uuid) -> DbResult<()> {
        let rows = conn.execute(
            "DELETE FROM recordings WHERE id = ?1",
            params![id.to_string()],
        )?;

        if rows == 0 {
            return Err(DbError::NotFound(format!("Recording {id}")));
        }

        Ok(())
    }

    pub fn count(conn: &Connection) -> DbResult<u32> {
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM recordings",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn row_to_recording(row: &Row) -> rusqlite::Result<Recording> {
        let id_str: String = row.get("id")?;
        let audio_path_str: String = row.get::<_, Option<String>>("audio_path")?.unwrap_or_default();
        let tags_str: String = row.get::<_, Option<String>>("tags")?.unwrap_or_else(|| "[]".to_string());
        let status_str: String = row.get("processing_status")?;
        let created_str: String = row.get("created_at")?;
        let metadata_str: String = row.get::<_, Option<String>>("metadata")?.unwrap_or_else(|| "null".to_string());

        Ok(Recording {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
            filename: row.get("filename")?,
            transcript: row.get("transcript")?,
            soap_note: row.get("soap_note")?,
            referral: row.get("referral")?,
            letter: row.get("letter")?,
            chat: row.get("chat")?,
            patient_name: row.get("patient_name")?,
            audio_path: audio_path_str.into(),
            duration_seconds: row.get("duration_seconds")?,
            file_size_bytes: row.get::<_, Option<i64>>("file_size_bytes")?.map(|v| v as u64),
            stt_provider: row.get("stt_provider")?,
            ai_provider: row.get("ai_provider")?,
            tags: serde_json::from_str(&tags_str).unwrap_or_default(),
            status: serde_json::from_str(&status_str).unwrap_or(ProcessingStatus::Pending),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            metadata: serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::MigrationEngine;
    use std::path::PathBuf;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        MigrationEngine::migrate(&conn).unwrap();
        conn
    }

    fn sample_recording() -> Recording {
        Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"))
    }

    #[test]
    fn insert_and_retrieve() {
        let conn = setup();
        let rec = sample_recording();
        RecordingsRepo::insert(&conn, &rec).unwrap();
        let fetched = RecordingsRepo::get_by_id(&conn, &rec.id).unwrap();
        assert_eq!(fetched.filename, "test.wav");
        assert!(matches!(fetched.status, ProcessingStatus::Pending));
    }

    #[test]
    fn get_nonexistent_returns_not_found() {
        let conn = setup();
        let result = RecordingsRepo::get_by_id(&conn, &Uuid::new_v4());
        assert!(matches!(result, Err(DbError::NotFound(_))));
    }

    #[test]
    fn update_recording() {
        let conn = setup();
        let mut rec = sample_recording();
        RecordingsRepo::insert(&conn, &rec).unwrap();
        rec.transcript = Some("Patient presents with headache".into());
        rec.status = ProcessingStatus::Completed { completed_at: chrono::Utc::now() };
        RecordingsRepo::update(&conn, &rec).unwrap();
        let fetched = RecordingsRepo::get_by_id(&conn, &rec.id).unwrap();
        assert!(fetched.transcript.unwrap().contains("headache"));
        assert!(matches!(fetched.status, ProcessingStatus::Completed { .. }));
    }

    #[test]
    fn delete_recording() {
        let conn = setup();
        let rec = sample_recording();
        RecordingsRepo::insert(&conn, &rec).unwrap();
        RecordingsRepo::delete(&conn, &rec.id).unwrap();
        assert!(matches!(
            RecordingsRepo::get_by_id(&conn, &rec.id),
            Err(DbError::NotFound(_))
        ));
    }

    #[test]
    fn list_recordings_with_pagination() {
        let conn = setup();
        for i in 0..5 {
            let rec = Recording::new(format!("recording_{i}.wav"), PathBuf::from(format!("/tmp/{i}.wav")));
            RecordingsRepo::insert(&conn, &rec).unwrap();
        }
        let page1 = RecordingsRepo::list_all(&conn, 3, 0).unwrap();
        assert_eq!(page1.len(), 3);
        let page2 = RecordingsRepo::list_all(&conn, 3, 3).unwrap();
        assert_eq!(page2.len(), 2);
    }

    #[test]
    fn count_recordings() {
        let conn = setup();
        assert_eq!(RecordingsRepo::count(&conn).unwrap(), 0);
        RecordingsRepo::insert(&conn, &sample_recording()).unwrap();
        assert_eq!(RecordingsRepo::count(&conn).unwrap(), 1);
    }

    #[test]
    fn tags_round_trip() {
        let conn = setup();
        let mut rec = sample_recording();
        rec.tags = vec!["urgent".into(), "follow-up".into()];
        RecordingsRepo::insert(&conn, &rec).unwrap();
        let fetched = RecordingsRepo::get_by_id(&conn, &rec.id).unwrap();
        assert_eq!(fetched.tags, vec!["urgent", "follow-up"]);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-db`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/db/src/recordings.rs
git commit -m "feat(db): add recordings CRUD with pagination, FTS5 sync, and tag support"
```

---

### Task 9: Database — Settings and Audit CRUD

**Files:**
- Create: `crates/db/src/settings.rs`
- Create: `crates/db/src/audit.rs`

- [ ] **Step 1: Write settings store with tests**

Write `crates/db/src/settings.rs`:
```rust
use medical_core::types::settings::AppConfig;
use rusqlite::{params, Connection};
use crate::{DbError, DbResult};

pub struct SettingsRepo;

impl SettingsRepo {
    pub fn get(conn: &Connection, key: &str) -> DbResult<Option<String>> {
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    pub fn set(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = datetime('now')",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, key: &str) -> DbResult<()> {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }

    pub fn load_config(conn: &Connection) -> DbResult<AppConfig> {
        match Self::get(conn, "app_config")? {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| DbError::Constraint(format!("Invalid config JSON: {e}"))),
            None => Ok(AppConfig::default()),
        }
    }

    pub fn save_config(conn: &Connection, config: &AppConfig) -> DbResult<()> {
        let json = serde_json::to_string(config)
            .map_err(|e| DbError::Constraint(e.to_string()))?;
        Self::set(conn, "app_config", &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::MigrationEngine;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        MigrationEngine::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn get_missing_returns_none() {
        let conn = setup();
        assert!(SettingsRepo::get(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn set_and_get() {
        let conn = setup();
        SettingsRepo::set(&conn, "theme", "dark").unwrap();
        assert_eq!(SettingsRepo::get(&conn, "theme").unwrap().unwrap(), "dark");
    }

    #[test]
    fn set_overwrites_existing() {
        let conn = setup();
        SettingsRepo::set(&conn, "theme", "dark").unwrap();
        SettingsRepo::set(&conn, "theme", "light").unwrap();
        assert_eq!(SettingsRepo::get(&conn, "theme").unwrap().unwrap(), "light");
    }

    #[test]
    fn delete_setting() {
        let conn = setup();
        SettingsRepo::set(&conn, "key", "value").unwrap();
        SettingsRepo::delete(&conn, "key").unwrap();
        assert!(SettingsRepo::get(&conn, "key").unwrap().is_none());
    }

    #[test]
    fn load_default_config_when_none_saved() {
        let conn = setup();
        let config = SettingsRepo::load_config(&conn).unwrap();
        assert_eq!(config.ai_provider, "openai");
    }

    #[test]
    fn save_and_load_config() {
        let conn = setup();
        let mut config = AppConfig::default();
        config.ai_provider = "anthropic".into();
        config.temperature = 0.8;
        SettingsRepo::save_config(&conn, &config).unwrap();
        let loaded = SettingsRepo::load_config(&conn).unwrap();
        assert_eq!(loaded.ai_provider, "anthropic");
        assert_eq!(loaded.temperature, 0.8);
    }
}
```

- [ ] **Step 2: Write audit log store with tests**

Write `crates/db/src/audit.rs`:
```rust
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use crate::{DbError, DbResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub timestamp: String,
    pub action: String,
    pub actor: String,
    pub resource: Option<String>,
    pub details: Option<String>,
}

pub struct AuditRepo;

impl AuditRepo {
    pub fn append(
        conn: &Connection,
        action: &str,
        actor: &str,
        resource: Option<&str>,
        details: Option<&str>,
    ) -> DbResult<i64> {
        conn.execute(
            "INSERT INTO audit_log (action, actor, resource, details) VALUES (?1, ?2, ?3, ?4)",
            params![action, actor, resource, details],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_recent(conn: &Connection, limit: u32) -> DbResult<Vec<AuditEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, action, actor, resource, details
             FROM audit_log ORDER BY id DESC LIMIT ?1"
        )?;

        let entries = stmt
            .query_map(params![limit], |row| {
                Ok(AuditEntry {
                    id: row.get("id")?,
                    timestamp: row.get("timestamp")?,
                    action: row.get("action")?,
                    actor: row.get("actor")?,
                    resource: row.get("resource")?,
                    details: row.get("details")?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn count(conn: &Connection) -> DbResult<u32> {
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM audit_log",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::MigrationEngine;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        MigrationEngine::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn append_and_list() {
        let conn = setup();
        AuditRepo::append(&conn, "recording_created", "user", Some("rec_123"), None).unwrap();
        AuditRepo::append(&conn, "soap_generated", "system", Some("rec_123"), Some("gpt-4o")).unwrap();
        let entries = AuditRepo::list_recent(&conn, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "soap_generated"); // most recent first
    }

    #[test]
    fn audit_log_is_append_only() {
        let conn = setup();
        AuditRepo::append(&conn, "test", "user", None, None).unwrap();
        let update_result = conn.execute("UPDATE audit_log SET action='hacked'", []);
        assert!(update_result.is_err());
        let delete_result = conn.execute("DELETE FROM audit_log", []);
        assert!(delete_result.is_err());
    }

    #[test]
    fn count_entries() {
        let conn = setup();
        assert_eq!(AuditRepo::count(&conn).unwrap(), 0);
        AuditRepo::append(&conn, "a", "u", None, None).unwrap();
        AuditRepo::append(&conn, "b", "u", None, None).unwrap();
        assert_eq!(AuditRepo::count(&conn).unwrap(), 2);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p medical-db`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/db/src/settings.rs crates/db/src/audit.rs
git commit -m "feat(db): add settings key-value store and append-only audit log"
```

---

### Task 10: Database — FTS5 Search

**Files:**
- Create: `crates/db/src/search.rs`

- [ ] **Step 1: Write full-text search with tests**

Write `crates/db/src/search.rs`:
```rust
use medical_core::types::recording::{Recording, ProcessingStatus};
use rusqlite::{params, Connection};
use uuid::Uuid;
use crate::{DbError, DbResult};
use crate::recordings::RecordingsRepo;

pub struct SearchRepo;

impl SearchRepo {
    /// Full-text search across recordings using FTS5.
    /// Returns matching recording IDs ranked by relevance.
    pub fn search(conn: &Connection, query: &str, limit: u32) -> DbResult<Vec<Uuid>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT r.id FROM recordings r
             JOIN recordings_fts fts ON r.rowid = fts.rowid
             WHERE recordings_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
        )?;

        let ids = stmt
            .query_map(params![query, limit], |row| {
                let id_str: String = row.get(0)?;
                Ok(Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    /// Search with full recording data returned
    pub fn search_recordings(conn: &Connection, query: &str, limit: u32) -> DbResult<Vec<Recording>> {
        let ids = Self::search(conn, query, limit)?;
        let mut results = Vec::with_capacity(ids.len());
        for id in &ids {
            match RecordingsRepo::get_by_id(conn, id) {
                Ok(rec) => results.push(rec),
                Err(DbError::NotFound(_)) => continue, // stale FTS entry
                Err(e) => return Err(e),
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::MigrationEngine;
    use crate::recordings::RecordingsRepo;
    use medical_core::types::recording::Recording;
    use std::path::PathBuf;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        MigrationEngine::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let conn = setup();
        assert!(SearchRepo::search(&conn, "", 10).unwrap().is_empty());
        assert!(SearchRepo::search(&conn, "   ", 10).unwrap().is_empty());
    }

    #[test]
    fn search_finds_by_transcript() {
        let conn = setup();
        let mut rec = Recording::new("visit.wav".into(), PathBuf::from("/tmp/visit.wav"));
        rec.transcript = Some("Patient presents with severe headache and nausea".into());
        RecordingsRepo::insert(&conn, &rec).unwrap();

        let results = SearchRepo::search(&conn, "headache", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], rec.id);
    }

    #[test]
    fn search_finds_by_soap_note() {
        let conn = setup();
        let mut rec = Recording::new("visit.wav".into(), PathBuf::from("/tmp/visit.wav"));
        rec.soap_note = Some("S: Patient reports persistent lower back pain".into());
        RecordingsRepo::insert(&conn, &rec).unwrap();

        let results = SearchRepo::search(&conn, "back pain", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_finds_by_patient_name() {
        let conn = setup();
        let mut rec = Recording::new("visit.wav".into(), PathBuf::from("/tmp/visit.wav"));
        rec.patient_name = Some("John Smith".into());
        RecordingsRepo::insert(&conn, &rec).unwrap();

        let results = SearchRepo::search(&conn, "Smith", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_respects_limit() {
        let conn = setup();
        for i in 0..5 {
            let mut rec = Recording::new(format!("v{i}.wav"), PathBuf::from(format!("/tmp/{i}.wav")));
            rec.transcript = Some("common keyword appears in every recording".into());
            RecordingsRepo::insert(&conn, &rec).unwrap();
        }
        let results = SearchRepo::search(&conn, "common keyword", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn fts_updates_on_recording_update() {
        let conn = setup();
        let mut rec = Recording::new("visit.wav".into(), PathBuf::from("/tmp/visit.wav"));
        rec.transcript = Some("original text".into());
        RecordingsRepo::insert(&conn, &rec).unwrap();

        // Should find by original text
        assert_eq!(SearchRepo::search(&conn, "original", 10).unwrap().len(), 1);

        // Update transcript
        rec.transcript = Some("updated text with migraine".into());
        RecordingsRepo::update(&conn, &rec).unwrap();

        // Should no longer find original, but find new text
        assert_eq!(SearchRepo::search(&conn, "original", 10).unwrap().len(), 0);
        assert_eq!(SearchRepo::search(&conn, "migraine", 10).unwrap().len(), 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-db`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/db/src/search.rs
git commit -m "feat(db): add FTS5 full-text search over recordings"
```

---

### Task 11: Database — Stub Modules for Vectors, Graph, Queue, Recipients

**Files:**
- Create: `crates/db/src/vectors.rs`
- Create: `crates/db/src/graph.rs`
- Create: `crates/db/src/processing_queue.rs`
- Create: `crates/db/src/recipients.rs`

These are stubs that compile and have placeholder types. Full implementations come in Plan 2 (vectors) and Plan 3 (graph, queue). This keeps the workspace compiling.

- [ ] **Step 1: Write stubs**

Write `crates/db/src/vectors.rs`:
```rust
//! sqlite-vec vector storage for RAG embeddings.
//! Full implementation in Plan 2.

pub struct VectorStore;

impl VectorStore {
    pub fn new() -> Self {
        Self
    }
}
```

Write `crates/db/src/graph.rs`:
```rust
//! CozoDB knowledge graph storage.
//! Full implementation in Plan 2.

pub struct GraphStore;

impl GraphStore {
    pub fn new() -> Self {
        Self
    }
}
```

Write `crates/db/src/processing_queue.rs`:
```rust
//! Processing queue and batch processing CRUD.
//! Full implementation in Plan 3.

pub struct ProcessingQueueRepo;

impl ProcessingQueueRepo {
    pub fn new() -> Self {
        Self
    }
}
```

Write `crates/db/src/recipients.rs`:
```rust
//! Saved recipients (address book) CRUD.
//! Full implementation in Plan 3.

pub struct RecipientsRepo;

impl RecipientsRepo {
    pub fn new() -> Self {
        Self
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p medical-db`
Expected: Compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add crates/db/src/vectors.rs crates/db/src/graph.rs crates/db/src/processing_queue.rs crates/db/src/recipients.rs
git commit -m "feat(db): add stub modules for vectors, graph, processing queue, and recipients"
```

---

### Task 12: Security — Machine ID

**Files:**
- Create: `crates/security/src/machine_id.rs`

- [ ] **Step 1: Write cross-platform machine ID with tests**

Write `crates/security/src/machine_id.rs`:
```rust
use sha2::{Sha256, Digest};
use crate::SecurityError;

/// Get a stable machine identifier.
/// Tries platform-specific sources, falls back to username + home directory.
/// Returns a SHA-256 hex string.
pub fn get_machine_id() -> Result<String, SecurityError> {
    let raw = raw_machine_id()?;
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(target_os = "linux")]
fn raw_machine_id() -> Result<String, SecurityError> {
    // Try /etc/machine-id first
    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }
    // Try /var/lib/dbus/machine-id
    if let Ok(id) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }
    fallback_id()
}

#[cfg(target_os = "macos")]
fn raw_machine_id() -> Result<String, SecurityError> {
    // macOS: use IOPlatformSerialNumber via system_profiler or ioreg
    let output = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| SecurityError::Encryption(format!("Failed to run ioreg: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(uuid) = line.split('"').nth(3) {
                return Ok(uuid.to_string());
            }
        }
    }
    fallback_id()
}

#[cfg(target_os = "windows")]
fn raw_machine_id() -> Result<String, SecurityError> {
    // Windows: read MachineGuid from registry
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .map_err(|e| SecurityError::Encryption(format!("Failed to read registry: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("MachineGuid") {
            if let Some(guid) = line.split_whitespace().last() {
                return Ok(guid.to_string());
            }
        }
    }
    fallback_id()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn raw_machine_id() -> Result<String, SecurityError> {
    fallback_id()
}

fn fallback_id() -> Result<String, SecurityError> {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".into());
    Ok(format!("{username}:{home}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_returns_hex_string() {
        let id = get_machine_id().unwrap();
        assert_eq!(id.len(), 64); // SHA-256 hex = 64 chars
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn machine_id_is_stable() {
        let id1 = get_machine_id().unwrap();
        let id2 = get_machine_id().unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn fallback_id_works() {
        let id = fallback_id().unwrap();
        assert!(!id.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-security`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/security/src/machine_id.rs
git commit -m "feat(security): add cross-platform machine ID derivation"
```

---

### Task 13: Security — AES-256-GCM Key Storage

**Files:**
- Create: `crates/security/src/key_storage.rs`

- [ ] **Step 1: Write key storage with tests**

Write `crates/security/src/key_storage.rs`:
```rust
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::{SecurityError, SecurityResult};
use crate::machine_id::get_machine_id;

const SALT_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const PBKDF2_ITERATIONS: u32 = 600_000;

pub struct KeyStorage {
    cipher: Aes256Gcm,
    storage_path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct KeyFile {
    keys: HashMap<String, StoredKey>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct StoredKey {
    /// Base64-encoded nonce + ciphertext
    encrypted: String,
    stored_at: String,
    /// First 8 chars of SHA-256 hash for verification
    key_hash: String,
}

impl KeyStorage {
    pub fn open(config_dir: &Path) -> SecurityResult<Self> {
        std::fs::create_dir_all(config_dir)?;
        let salt = Self::load_or_create_salt(config_dir)?;
        let master_key = Self::derive_master_key(&salt)?;
        let key = Key::<Aes256Gcm>::from_slice(&master_key);
        let cipher = Aes256Gcm::new(key);
        let storage_path = config_dir.join("keys.enc");

        Ok(Self { cipher, storage_path })
    }

    pub fn store_key(&self, provider: &str, api_key: &str) -> SecurityResult<()> {
        let mut file = self.load_file()?;

        let mut nonce_bytes = [0u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self.cipher.encrypt(nonce, api_key.as_bytes())
            .map_err(|e| SecurityError::Encryption(e.to_string()))?;

        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ciphertext);

        let key_hash = {
            use sha2::Digest;
            let hash = sha2::Sha256::digest(api_key.as_bytes());
            format!("{:x}", hash)[..8].to_string()
        };

        file.keys.insert(provider.to_string(), StoredKey {
            encrypted: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &combined),
            stored_at: chrono::Utc::now().to_rfc3339(),
            key_hash,
        });

        self.save_file(&file)
    }

    pub fn get_key(&self, provider: &str) -> SecurityResult<Option<String>> {
        let file = self.load_file()?;
        let stored = match file.keys.get(provider) {
            Some(s) => s,
            None => return Ok(None),
        };

        let combined = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &stored.encrypted)
            .map_err(|e| SecurityError::Decryption(e.to_string()))?;

        if combined.len() < NONCE_LENGTH {
            return Err(SecurityError::InvalidFormat);
        }

        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LENGTH);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| SecurityError::Decryption(e.to_string()))?;

        String::from_utf8(plaintext)
            .map(Some)
            .map_err(|e| SecurityError::Decryption(e.to_string()))
    }

    pub fn remove_key(&self, provider: &str) -> SecurityResult<bool> {
        let mut file = self.load_file()?;
        let removed = file.keys.remove(provider).is_some();
        if removed {
            self.save_file(&file)?;
        }
        Ok(removed)
    }

    pub fn list_providers(&self) -> SecurityResult<Vec<String>> {
        let file = self.load_file()?;
        Ok(file.keys.keys().cloned().collect())
    }

    fn derive_master_key(salt: &[u8; SALT_LENGTH]) -> SecurityResult<[u8; 32]> {
        let input = std::env::var("MEDICAL_ASSISTANT_MASTER_KEY")
            .unwrap_or_else(|_| get_machine_id().unwrap_or_else(|_| "fallback-key".into()));

        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(input.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
        Ok(key)
    }

    fn load_or_create_salt(config_dir: &Path) -> SecurityResult<[u8; SALT_LENGTH]> {
        let salt_path = config_dir.join("salt.bin");
        if salt_path.exists() {
            let bytes = std::fs::read(&salt_path)?;
            if bytes.len() == SALT_LENGTH {
                let mut salt = [0u8; SALT_LENGTH];
                salt.copy_from_slice(&bytes);
                return Ok(salt);
            }
        }

        let mut salt = [0u8; SALT_LENGTH];
        OsRng.fill_bytes(&mut salt);
        std::fs::write(&salt_path, &salt)?;
        Ok(salt)
    }

    fn load_file(&self) -> SecurityResult<KeyFile> {
        if !self.storage_path.exists() {
            return Ok(KeyFile::default());
        }
        let data = std::fs::read_to_string(&self.storage_path)?;
        serde_json::from_str(&data)
            .map_err(|e| SecurityError::Decryption(format!("Corrupt key file: {e}")))
    }

    fn save_file(&self, file: &KeyFile) -> SecurityResult<()> {
        let data = serde_json::to_string_pretty(file)
            .map_err(|e| SecurityError::Encryption(e.to_string()))?;
        std::fs::write(&self.storage_path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, KeyStorage) {
        let dir = tempdir().unwrap();
        let storage = KeyStorage::open(dir.path()).unwrap();
        (dir, storage)
    }

    #[test]
    fn store_and_retrieve_key() {
        let (_dir, storage) = setup();
        storage.store_key("openai", "sk-test-12345").unwrap();
        let key = storage.get_key("openai").unwrap();
        assert_eq!(key.as_deref(), Some("sk-test-12345"));
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let (_dir, storage) = setup();
        assert!(storage.get_key("nonexistent").unwrap().is_none());
    }

    #[test]
    fn overwrite_existing_key() {
        let (_dir, storage) = setup();
        storage.store_key("openai", "old-key").unwrap();
        storage.store_key("openai", "new-key").unwrap();
        assert_eq!(storage.get_key("openai").unwrap().as_deref(), Some("new-key"));
    }

    #[test]
    fn remove_key() {
        let (_dir, storage) = setup();
        storage.store_key("openai", "sk-test").unwrap();
        assert!(storage.remove_key("openai").unwrap());
        assert!(storage.get_key("openai").unwrap().is_none());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let (_dir, storage) = setup();
        assert!(!storage.remove_key("nonexistent").unwrap());
    }

    #[test]
    fn list_providers() {
        let (_dir, storage) = setup();
        storage.store_key("openai", "key1").unwrap();
        storage.store_key("anthropic", "key2").unwrap();
        let mut providers = storage.list_providers().unwrap();
        providers.sort();
        assert_eq!(providers, vec!["anthropic", "openai"]);
    }

    #[test]
    fn salt_persists_across_instances() {
        let dir = tempdir().unwrap();
        let storage1 = KeyStorage::open(dir.path()).unwrap();
        storage1.store_key("test", "secret").unwrap();
        // Open a second instance — should use same salt
        let storage2 = KeyStorage::open(dir.path()).unwrap();
        assert_eq!(storage2.get_key("test").unwrap().as_deref(), Some("secret"));
    }

    #[test]
    fn different_nonces_per_encryption() {
        let (_dir, storage) = setup();
        storage.store_key("provider1", "same-key").unwrap();
        storage.store_key("provider2", "same-key").unwrap();
        let file = storage.load_file().unwrap();
        // Even with same plaintext, ciphertexts differ due to unique nonces
        assert_ne!(
            file.keys["provider1"].encrypted,
            file.keys["provider2"].encrypted
        );
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-security`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/security/src/key_storage.rs
git commit -m "feat(security): add AES-256-GCM key storage with PBKDF2 key derivation"
```

---

### Task 14: Security — PHI Redactor

**Files:**
- Create: `crates/security/src/phi_redactor.rs`

- [ ] **Step 1: Write PHI redactor with tests**

Write `crates/security/src/phi_redactor.rs`:
```rust
use lazy_static::lazy_static;
use regex::Regex;

/// Redacts PHI/PII from text by replacing matches with typed placeholders.
pub struct PhiRedactor;

struct RedactionPattern {
    regex: Regex,
    placeholder: &'static str,
}

lazy_static! {
    static ref PATTERNS: Vec<RedactionPattern> = vec![
        // SSN: 123-45-6789 or 123456789
        RedactionPattern {
            regex: Regex::new(r"\b\d{3}-?\d{2}-?\d{4}\b").unwrap(),
            placeholder: "[SSN]",
        },
        // Phone: various formats
        RedactionPattern {
            regex: Regex::new(r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b").unwrap(),
            placeholder: "[PHONE]",
        },
        // Email
        RedactionPattern {
            regex: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap(),
            placeholder: "[EMAIL]",
        },
        // Date of birth patterns: MM/DD/YYYY, MM-DD-YYYY, YYYY-MM-DD
        RedactionPattern {
            regex: Regex::new(r"\b(?:DOB|Date of Birth|Born|D\.O\.B\.?)\s*:?\s*\d{1,2}[-/]\d{1,2}[-/]\d{2,4}\b").unwrap(),
            placeholder: "[DOB]",
        },
        // MRN / Medical Record Number
        RedactionPattern {
            regex: Regex::new(r"\b(?:MRN|Medical Record|Record\s*#?|Chart\s*#?)\s*:?\s*[A-Z0-9-]{4,20}\b").unwrap(),
            placeholder: "[MRN]",
        },
        // Street addresses: number + street name
        RedactionPattern {
            regex: Regex::new(r"\b\d{1,5}\s+[A-Za-z]+(?:\s+[A-Za-z]+)*\s+(?:St|Street|Ave|Avenue|Blvd|Boulevard|Dr|Drive|Ln|Lane|Rd|Road|Ct|Court|Way|Pl|Place)\.?\b").unwrap(),
            placeholder: "[ADDRESS]",
        },
        // Zip codes
        RedactionPattern {
            regex: Regex::new(r"\b\d{5}(?:-\d{4})?\b").unwrap(),
            placeholder: "[ZIP]",
        },
    ];
}

impl PhiRedactor {
    /// Redact all recognized PHI patterns from text.
    pub fn redact(text: &str) -> String {
        let mut result = text.to_string();
        for pattern in PATTERNS.iter() {
            result = pattern.regex.replace_all(&result, pattern.placeholder).to_string();
        }
        result
    }

    /// Check if text contains any PHI patterns.
    pub fn contains_phi(text: &str) -> bool {
        PATTERNS.iter().any(|p| p.regex.is_match(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_ssn() {
        assert_eq!(PhiRedactor::redact("SSN is 123-45-6789"), "SSN is [SSN]");
        assert_eq!(PhiRedactor::redact("SSN is 123456789"), "SSN is [SSN]");
    }

    #[test]
    fn redacts_phone() {
        assert_eq!(PhiRedactor::redact("Call 555-123-4567"), "Call [PHONE]");
        assert_eq!(PhiRedactor::redact("Call (555) 123-4567"), "Call [PHONE]");
    }

    #[test]
    fn redacts_email() {
        assert_eq!(
            PhiRedactor::redact("Email: john@example.com"),
            "Email: [EMAIL]"
        );
    }

    #[test]
    fn redacts_dob() {
        assert_eq!(
            PhiRedactor::redact("DOB: 01/15/1990"),
            "[DOB]"
        );
    }

    #[test]
    fn redacts_mrn() {
        assert_eq!(
            PhiRedactor::redact("MRN: ABC-12345"),
            "[MRN]"
        );
    }

    #[test]
    fn redacts_address() {
        assert_eq!(
            PhiRedactor::redact("Lives at 123 Main Street"),
            "Lives at [ADDRESS]"
        );
    }

    #[test]
    fn contains_phi_detects_patterns() {
        assert!(PhiRedactor::contains_phi("SSN: 123-45-6789"));
        assert!(PhiRedactor::contains_phi("Email: test@test.com"));
        assert!(!PhiRedactor::contains_phi("No PHI here, just medical terms"));
    }

    #[test]
    fn preserves_non_phi_text() {
        let text = "Patient presents with chronic headache. BP 120/80. Heart rate 72 bpm.";
        assert_eq!(PhiRedactor::redact(text), text);
    }

    #[test]
    fn handles_multiple_patterns_in_one_string() {
        let text = "Patient john@hospital.com SSN 123-45-6789 phone 555-123-4567";
        let redacted = PhiRedactor::redact(text);
        assert!(redacted.contains("[EMAIL]"));
        assert!(redacted.contains("[SSN]"));
        assert!(redacted.contains("[PHONE]"));
        assert!(!redacted.contains("john@hospital.com"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p medical-security`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/security/src/phi_redactor.rs
git commit -m "feat(security): add PHI/PII redactor with pattern matching"
```

---

### Task 15: Security — Stub Modules (Audit Logger, Input Sanitizer, Rate Limiter)

**Files:**
- Create: `crates/security/src/audit_logger.rs`
- Create: `crates/security/src/input_sanitizer.rs`
- Create: `crates/security/src/rate_limiter.rs`

- [ ] **Step 1: Write stubs**

Write `crates/security/src/audit_logger.rs`:
```rust
//! HIPAA-compliant audit logger that wraps db::AuditRepo with PHI redaction.
//! Full implementation connects to the db crate's audit_log table.

use crate::phi_redactor::PhiRedactor;

pub struct AuditLogger;

impl AuditLogger {
    pub fn new() -> Self {
        Self
    }

    /// Redact PHI from details before logging.
    pub fn redact_for_log(details: &str) -> String {
        PhiRedactor::redact(details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_phi_in_log_details() {
        let details = "Patient john@test.com updated record";
        let redacted = AuditLogger::redact_for_log(details);
        assert!(redacted.contains("[EMAIL]"));
        assert!(!redacted.contains("john@test.com"));
    }
}
```

Write `crates/security/src/input_sanitizer.rs`:
```rust
//! Input sanitization for prompt injection, XSS prevention, and length limits.
//! Full implementation in Plan 3.

pub struct InputSanitizer;

impl InputSanitizer {
    /// Strip HTML tags from input.
    pub fn strip_html(input: &str) -> String {
        lazy_static::lazy_static! {
            static ref HTML_TAG: regex::Regex = regex::Regex::new(r"<[^>]+>").unwrap();
        }
        HTML_TAG.replace_all(input, "").to_string()
    }

    /// Enforce maximum input length.
    pub fn truncate(input: &str, max_len: usize) -> &str {
        if input.len() <= max_len {
            input
        } else {
            &input[..input.floor_char_boundary(max_len)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html() {
        assert_eq!(InputSanitizer::strip_html("<b>bold</b>"), "bold");
        assert_eq!(InputSanitizer::strip_html("<script>alert('xss')</script>"), "alert('xss')");
    }

    #[test]
    fn truncates_to_max_length() {
        assert_eq!(InputSanitizer::truncate("hello", 10), "hello");
        assert_eq!(InputSanitizer::truncate("hello world", 5), "hello");
    }
}
```

Write `crates/security/src/rate_limiter.rs`:
```rust
//! Token bucket rate limiter.
//! Full implementation in Plan 2 when provider crates need it.

use std::time::Instant;

pub struct RateLimiter {
    capacity: u32,
    tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            capacity: requests_per_minute,
            tokens: requests_per_minute as f64,
            refill_rate: requests_per_minute as f64 / 60.0,
            last_refill: Instant::now(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_requests_within_capacity() {
        let mut limiter = RateLimiter::new(10);
        for _ in 0..10 {
            assert!(limiter.try_acquire());
        }
    }

    #[test]
    fn blocks_when_exhausted() {
        let mut limiter = RateLimiter::new(2);
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());
    }
}
```

- [ ] **Step 2: Run all security tests**

Run: `cargo test -p medical-security`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/security/src/audit_logger.rs crates/security/src/input_sanitizer.rs crates/security/src/rate_limiter.rs
git commit -m "feat(security): add audit logger, input sanitizer, and rate limiter"
```

---

### Task 16: Database — Top-Level Database Struct

**Files:**
- Modify: `crates/db/src/lib.rs`

- [ ] **Step 1: Write Database facade with tests**

Update `crates/db/src/lib.rs` to add a `Database` struct that owns the pool and provides convenience methods:

```rust
pub mod pool;
pub mod migrations;
pub mod recordings;
pub mod processing_queue;
pub mod recipients;
pub mod settings;
pub mod audit;
pub mod search;
pub mod vectors;
pub mod graph;

use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Constraint violation: {0}")]
    Constraint(String),
}

pub type DbResult<T> = Result<T, DbError>;

use pool::DbPool;
use migrations::MigrationEngine;

/// Main database handle. Owns the connection pool and runs migrations on creation.
pub struct Database {
    pool: DbPool,
}

impl Database {
    /// Open (or create) the database at the given path and run migrations.
    pub fn open(db_path: &Path) -> DbResult<Self> {
        let pool = pool::create_pool(db_path)?;
        let conn = pool.get()?;
        MigrationEngine::migrate(&conn)?;
        Ok(Self { pool })
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> DbResult<Self> {
        let pool = pool::create_memory_pool()?;
        let conn = pool.get()?;
        MigrationEngine::migrate(&conn)?;
        Ok(Self { pool })
    }

    /// Get a connection from the pool.
    pub fn conn(&self) -> DbResult<r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>> {
        self.pool.get().map_err(DbError::Pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::recording::Recording;
    use std::path::PathBuf;

    #[test]
    fn database_opens_in_memory() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn().unwrap();
        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM recordings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn database_opens_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let conn = db.conn().unwrap();
        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM recordings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn full_workflow_through_database() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn().unwrap();

        // Insert a recording
        let rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        recordings::RecordingsRepo::insert(&conn, &rec).unwrap();

        // Verify it exists
        assert_eq!(recordings::RecordingsRepo::count(&conn).unwrap(), 1);

        // Save settings
        settings::SettingsRepo::set(&conn, "theme", "dark").unwrap();
        assert_eq!(
            settings::SettingsRepo::get(&conn, "theme").unwrap().as_deref(),
            Some("dark")
        );

        // Write audit log
        audit::AuditRepo::append(&conn, "test_action", "test_user", Some("test"), None).unwrap();
        assert_eq!(audit::AuditRepo::count(&conn).unwrap(), 1);
    }
}
```

- [ ] **Step 2: Run all tests across workspace**

Run: `cargo test --workspace`
Expected: All tests pass across core, db, and security crates.

- [ ] **Step 3: Commit**

```bash
git add crates/db/src/lib.rs
git commit -m "feat(db): add Database facade with pool management, migrations, and convenience API"
```

---

### Task 17: Final Verification and Workspace Build

- [ ] **Step 1: Run full workspace build**

Run: `cargo build --workspace`
Expected: Clean build, no errors.

- [ ] **Step 2: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass. Count total test cases.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings 2>&1 | head -30`
Expected: No errors. Fix any clippy warnings if present.

- [ ] **Step 4: Verify Tauri dev server starts**

Run:
```bash
cd /home/cortexuvula/Development/rustMedicalAssistant
npm run tauri dev 2>&1 | head -20
```
Expected: Compilation starts, Tauri window opens (or at least gets to the dev server stage). Kill with Ctrl+C.

- [ ] **Step 5: Commit any clippy fixes**

If clippy required changes:
```bash
git add -A
git commit -m "fix: address clippy warnings across workspace"
```

- [ ] **Step 6: Final commit and push**

```bash
git push origin master
```
