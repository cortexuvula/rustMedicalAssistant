# Rust + Tauri Port — Design Specification

**Date:** 2026-04-05
**Status:** Approved
**Source project:** ~/Development/Medical-Assistant (Python 3.10+, Tkinter, ~77K LOC, 424 modules)
**Target project:** ~/Development/rustMedicalAssistant

---

## 1. Overview

Full faithful port of the Medical-Assistant desktop application from Python/Tkinter to Rust/Tauri with a Svelte frontend. Every feature is reproduced: 5 workflow tabs, 6 editor tabs, 8 AI agents, 6 LLM providers, 5 STT providers, TTS, RAG with hybrid search, knowledge graphs, bidirectional translation, document export, HIPAA-compliant security, and batch processing.

The resulting application is fully self-contained — no external database dependencies. All data stored locally via SQLite, sqlite-vec, and CozoDB.

**Target platforms:** Linux, macOS, Windows — all supported from day one.

---

## 2. Architecture: Multi-Crate Cargo Workspace

```
rustMedicalAssistant/
├── Cargo.toml                (workspace root)
├── crates/
│   ├── core/                 (shared types, config, errors, traits)
│   ├── db/                   (SQLite, sqlite-vec, CozoDB, migrations, FTS)
│   ├── security/             (AES-256-GCM, key storage, PHI redaction, audit)
│   ├── audio/                (cpal capture, rodio playback, waveform)
│   ├── ai-providers/         (OpenAI, Anthropic, Gemini, Groq, Cerebras, Ollama)
│   ├── stt-providers/        (Deepgram, ElevenLabs, Groq, Modulate, whisper-rs)
│   ├── tts-providers/        (ElevenLabs, local platform TTS)
│   ├── agents/               (8 agent impls, tool system, orchestration)
│   ├── rag/                  (embeddings, hybrid search, MMR, graph queries)
│   ├── processing/           (recording pipeline, batch queue, document gen)
│   ├── export/               (PDF, DOCX, FHIR R4)
│   └── translation/          (bidirectional translation engine)
├── src-tauri/                (Tauri app, commands, IPC bridge)
└── src/                      (Svelte frontend)
```

### Dependency Graph

```
                    ┌─────────────┐
                    │  src-tauri   │
                    └──────┬──────┘
                           │
          ┌────────────────┼────────────────────┐
          │                │                    │
    ┌─────▼─────┐   ┌─────▼──────┐   ┌────────▼────────┐
    │ processing │   │   agents   │   │   translation   │
    └─────┬─────┘   └─────┬──────┘   └────────┬────────┘
          │               │                    │
    ┌─────▼─────┐   ┌─────▼──────┐            │
    │   export  │   │    rag     │            │
    └─────┬─────┘   └─────┬──────┘            │
          │               │                    │
   ┌──────┼───────┬───────┼──────┬─────────────┘
   │      │       │       │      │
┌──▼──┐ ┌─▼──┐ ┌──▼──┐ ┌─▼──┐ ┌─▼───┐ ┌──────────┐
│audio│ │ db │ │ ai- │ │stt-│ │tts- │ │ security │
│     │ │    │ │provs│ │provs│ │provs│ │          │
└──┬──┘ └─┬──┘ └──┬──┘ └─┬──┘ └──┬──┘ └────┬─────┘
   └──────┴───────┴──────┴───────┴──────────┘
                      │
               ┌──────▼──────┐
               │    core     │
               └─────────────┘
```

**Rule:** `core` has zero external service dependencies. Only foundational crates: `serde`, `thiserror`, `chrono`, `uuid`, `async-trait`, `tokio` (traits only), `futures-core`.

---

## 3. Core Types & Trait System (`core` crate)

### Provider Traits

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    fn available_models(&self) -> Vec<ModelInfo>;
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(&self, request: CompletionRequest)
        -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>>>>>;
    async fn complete_with_tools(&self, request: CompletionRequest, tools: &[ToolDef])
        -> Result<ToolCompletionResponse>;
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supports_streaming(&self) -> bool;
    fn supports_diarization(&self) -> bool;
    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> Result<Transcript>;
    async fn transcribe_stream(&self, stream: AudioStream, config: SttConfig)
        -> Result<Pin<Box<dyn Stream<Item = Result<TranscriptChunk>>>>>;
}

#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn name(&self) -> &str;
    fn available_voices(&self) -> Vec<VoiceInfo>;
    async fn synthesize(&self, text: &str, config: TtsConfig) -> Result<AudioData>;
}
```

### Agent & Tool Traits

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn system_prompt(&self) -> &str;
    fn available_tools(&self) -> Vec<ToolDef>;
    async fn execute(&self, context: AgentContext) -> Result<AgentResponse>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDef;
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput>;
}
```

### Domain Types

```rust
pub struct Recording {
    pub id: Uuid,
    pub filename: String,
    pub transcript: Option<String>,
    pub soap_note: Option<String>,
    pub referral: Option<String>,
    pub letter: Option<String>,
    pub patient_name: Option<String>,
    pub audio_path: PathBuf,
    pub duration: Duration,
    pub file_size_bytes: u64,
    pub stt_provider: String,
    pub ai_provider: String,
    pub tags: Vec<String>,
    pub status: ProcessingStatus,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// State encoded in enum variants — invalid states unrepresentable
pub enum ProcessingStatus {
    Pending,
    Processing { started_at: DateTime<Utc> },
    Completed { completed_at: DateTime<Utc> },
    Failed { error: String, retry_count: u32 },
}
```

### Streaming Types

```rust
type CompletionStream = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

pub enum StreamChunk {
    Delta(String),
    ToolCall(ToolCallDelta),
    Usage(UsageInfo),
    Done,
}
```

---

## 4. Database Layer (`db` crate)

Three embedded engines in one crate:

### SQLite (via `rusqlite`)

- **recordings** table: maps to `Recording` struct, `ProcessingStatus` serialized as tagged JSON
- **settings** table: key/value with JSON values, backed by `AppConfig`
- **audit_log**: append-only, HIPAA-compliant, UPDATE/DELETE rejected by trigger
- **address_book**: referral contacts with specialty, address, CSV import
- **FTS5** virtual tables for full-text search over transcripts, SOAP notes, referrals
- **Connection pool**: `r2d2`, WAL mode, multiple concurrent readers, serialized writer

### sqlite-vec (vector search, replaces Neon/pgvector)

- Stores embeddings for RAG document chunks and clinical guidelines
- Cosine similarity search for nearest neighbors
- Embeddings generated via configured provider (default: OpenAI text-embedding-3-small)
- Metadata columns: source document, chunk index, timestamps

### CozoDB (knowledge graph, replaces Neo4j)

- Datalog queries for clinical entity relationships
- Entity types: Drug, Condition, Procedure, Symptom, LabTest
- Relation types: treats, contraindicates, causes, diagnoses, indicates
- Persisted via RocksDB backend to local file
- Graph data exported to Svelte frontend for D3.js visualization

### Migration Engine

```rust
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub up: fn(&Connection) -> Result<()>,
}

const MIGRATIONS: &[Migration] = &[
    Migration { version: 1, name: "initial_schema", up: m001_initial },
    Migration { version: 2, name: "add_tags", up: m002_tags },
    Migration { version: 3, name: "add_fts", up: m003_fts },
    // ...
];
```

Forward-only, versioned, runs automatically on startup. Current version tracked in `schema_version` table.

---

## 5. Audio Pipeline (`audio` crate)

### Recording Flow

1. User hits record -> `AudioEngine::start_capture(device, config)`
2. `cpal` audio callback writes PCM samples into a lock-free ring buffer (`ringbuf` crate)
3. Dedicated reader thread drains ring buffer:
   - Encodes to WAV via `hound` crate -> temp file
   - Downsamples to ~128 amplitude values per frame -> bounded channel
4. Tauri event emitter reads channel, sends `waveform-data` events every ~50ms
5. Svelte renders waveform on `<canvas>`
6. Pause/resume toggles a flag — `cpal` callback runs but discards samples

### State Machine

```rust
pub enum RecordingState {
    Idle,
    Recording {
        started_at: Instant,
        device: AudioDevice,
        file_path: PathBuf,
    },
    Paused {
        elapsed: Duration,
        device: AudioDevice,
        file_path: PathBuf,
    },
    Stopped {
        file_path: PathBuf,
        duration: Duration,
    },
}
```

### Details

- **Capture:** `cpal` with lock-free ring buffer
- **Playback:** `rodio` with play/pause/seek/volume
- **Device management:** Enumerate via `cpal`, remember last-used in settings
- **Format:** 16-bit PCM WAV, 44.1kHz mono (configurable)

---

## 6. AI Provider System (`ai-providers` crate)

### Provider Registry

```rust
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn AiProvider>>,
}
```

### Providers

| Provider | Implementation | Notes |
|----------|---------------|-------|
| OpenAI | `OpenAiProvider` | Native SSE streaming via `eventsource-stream` |
| Anthropic | `AnthropicProvider` | Native SSE streaming |
| Gemini | `GeminiProvider` | Native SSE streaming |
| Groq | `GroqProvider` | OpenAI-compatible, shares `OpenAiCompatibleClient` |
| Cerebras | `CerebrasProvider` | OpenAI-compatible, shares `OpenAiCompatibleClient` |
| Ollama | `OllamaProvider` | OpenAI-compatible, local HTTP |

Groq, Cerebras, and Ollama share an `OpenAiCompatibleClient` base with endpoint/auth overrides — 3 providers for the cost of 1 implementation.

All streaming responses bridged to Svelte frontend via Tauri `ai-stream-chunk` events.

---

## 7. STT Provider System (`stt-providers` crate)

Same registry pattern as AI providers, plus a failover chain:

```rust
pub struct SttFailover {
    chain: Vec<Box<dyn SttProvider>>,
}
```

Tries each provider in order, logs failures, continues to next. Returns `Error::AllProvidersExhausted` if all fail.

| Provider | Implementation | Special Features |
|----------|---------------|-----------------|
| Deepgram | HTTP API | Nova-2 Medical model, medical terminology |
| ElevenLabs | HTTP API | Scribe v2, speaker diarization, entity detection |
| Groq | HTTP API | Whisper-based, ultra-fast |
| Modulate | HTTP API | Voice emotion detection, deepfake detection, PII redaction. Emotion data stored in `Recording.metadata` and surfaced in the RecordingCard component as sentiment tags. |
| Local Whisper | `whisper-rs` | Offline, models downloaded on first use, runs on `rayon` thread pool |

---

## 8. TTS Provider System (`tts-providers` crate)

| Provider | Implementation | Notes |
|----------|---------------|-------|
| ElevenLabs | HTTP API via `reqwest` | Flash v2.5, Turbo v2.5, Multilingual v2 |
| Local platform TTS | Platform-specific | `speech-dispatcher` (Linux), `NSSpeechSynthesizer` (macOS), `SAPI` (Windows) |

Audio bytes from API played through `rodio`.

---

## 9. Agent System (`agents` crate)

### AgentOrchestrator

Routes requests to agents, manages the tool execution loop, enforces max iterations (10), handles cancellation via `CancellationToken` (from `tokio-util`).

### Tool Execution Loop

1. Send prompt + tools to AiProvider
2. If response contains tool_calls: execute each via `Tool::execute()`, append results, go to 1
3. If response is text: return to caller

### Agents and Their Tools

| Agent | Tools | Purpose |
|-------|-------|---------|
| Medication | DrugInteraction, IcdLookup | Drug safety, dosage, prescriptions |
| Diagnostic | IcdLookup, LabExtractor, VitalsExtractor | Differential diagnosis, workups |
| Compliance | ChecklistTool | SOAP audit against documentation standards |
| DataExtract | VitalsExtractor, LabExtractor | Structured data extraction from transcripts |
| Workflow | ChecklistTool | Step-by-step clinical guidance |
| Referral | PatientHistory, DocumentGenerator | Referral letter generation |
| Synopsis | PatientHistory | SOAP note summarization |
| Chat | RagSearch, PatientHistory, all above | General conversational AI with full tool access |

### Agent Context

```rust
pub struct AgentContext {
    pub user_message: String,
    pub conversation_history: Vec<Message>,
    pub patient_context: Option<PatientContext>,
    pub rag_context: Vec<RagResult>,
    pub recording: Option<Recording>,
    pub cancellation: CancellationToken,
}
```

---

## 10. RAG System (`rag` crate)

### HybridRetriever

Three-way parallel search via `tokio::join!`:

1. **Vector search** (sqlite-vec): embed query, cosine similarity, adaptive threshold (0.75 -> 0.6 if too few results)
2. **BM25 keyword search** (SQLite FTS5): catches exact terminology embeddings miss (drug names, ICD codes, lab values)
3. **Graph search** (CozoDB): extract entities from query, traverse knowledge graph for related entities

### Query Expansion

```rust
pub struct QueryExpander {
    abbreviations: HashMap<String, Vec<String>>,  // HTN -> Hypertension
    synonyms: HashMap<String, Vec<String>>,        // heart attack <-> MI
}
```

### Result Fusion

Reciprocal Rank Fusion (RRF) across all three result sets:
```
score = sum(1 / (k + rank_in_set))  // k = 60.0
```

### MMR Reranking

Maximal Marginal Relevance after fusion for diversity:
- lambda 0.7 (favor relevance over diversity)
- top_k configurable (default 5)

### Document Ingestion Pipeline

1. PDF/text upload -> extract text (direct or OCR via `tesseract-rs` + `image` crate)
2. Chunk: 512-token windows, 64-token overlap, sentence-boundary-aware
3. Embed chunks via configured embedding provider (batch API call)
4. Store chunks + embeddings in sqlite-vec
5. Extract clinical entities via LLM -> store in CozoDB as nodes/edges
6. Async with progress reporting to frontend

---

## 11. Processing Pipeline (`processing` crate)

### ProcessingQueue

Bounded `tokio::mpsc` channel (capacity 32) feeding a worker pool (3 workers).

### Pipeline Steps

1. Update status -> Processing
2. Load audio file
3. Transcribe (SttFailover)
4. Generate SOAP note (AiProvider)
5. Optionally generate referral/letter
6. Extract structured data (DataExtractAgent)
7. Index in RAG (embed + store)
8. Update status -> Completed

**On failure:** Retry up to 3 times with exponential backoff, then mark Failed.

### Progress Events

```rust
pub enum ProcessingEvent {
    StepChanged { recording_id: Uuid, step: Step },
    Progress { recording_id: Uuid, percent: f32 },
    Completed { recording_id: Uuid },
    Failed { recording_id: Uuid, error: String },
    QueueStatus { pending: usize, processing: usize, completed: usize },
}
```

"Quick Continue Mode": user can review completed recordings while others continue processing.

---

## 12. Export System (`export` crate)

```rust
pub trait Exporter: Send + Sync {
    fn format(&self) -> ExportFormat;
    fn export(&self, recording: &Recording, config: &ExportConfig) -> Result<Vec<u8>>;
}
```

| Exporter | Crate | Output |
|----------|-------|--------|
| PdfExporter | `printpdf` | SOAP notes, referral letters, patient correspondence |
| DocxExporter | `docx-rs` | Same documents as Word files |
| FhirExporter | `serde_json` | FHIR R4 Bundle (Patient, Encounter, Condition, Observation, MedicationStatement, DocumentReference) |

---

## 13. Security (`security` crate)

### Key Storage (AES-256-GCM)

- Master key: PBKDF2-HMAC-SHA256, 600,000 iterations (OWASP 2024 recommendation)
- Input: machine-id + 32-byte random salt, or `MEDICAL_ASSISTANT_MASTER_KEY` env var
- Storage: `keys.enc` (encrypted keys), `salt.bin` (per-installation salt)
- One-time migration from Python Fernet format on first launch

### PHI Redactor

- 60+ field patterns (SSN, MRN, DOB, phone, email, address, patient/provider names, etc.)
- `regex::RegexSet` for efficient single-pass multi-pattern matching
- Compiled once at startup via `lazy_static`
- Replaces matches with typed placeholders: `[SSN]`, `[DOB]`, `[PATIENT_NAME]`

### Audit Logger

- Append-only SQLite table, UPDATE/DELETE rejected by trigger
- All entries PHI-redacted before writing
- Fields: timestamp, action, actor, resource, details

### Input Sanitizer

- Prompt injection detection (pattern matching)
- HTML/script tag stripping
- Max input length enforcement
- SQL injection prevented at db crate level (parameterized queries)

### Rate Limiter

- Token bucket per provider endpoint
- Configurable requests/min and tokens/min
- Shared via `Arc<RateLimiter>`

---

## 14. Translation System (`translation` crate)

### Bidirectional Flow

1. Provider speaks -> STT -> translate to patient's language -> TTS speaks
2. Patient speaks -> STT -> translate to provider's language -> display

### Translation Provider Trait

```rust
#[async_trait]
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_languages(&self) -> Vec<Language>;
    async fn translate(&self, text: &str, from: Language, to: Language) -> Result<String>;
    async fn detect_language(&self, text: &str) -> Result<Language>;
}
```

### Canned Responses

Pre-built categorized medical phrases stored as bundled TOML. Pre-translated into common languages for instant use without API calls.

---

## 15. Tauri IPC Bridge (`src-tauri/`)

### Command Groups

```rust
mod commands {
    pub mod audio;       // start_recording, stop_recording, pause, resume, list_devices
    pub mod recordings;  // list, search, get, delete, update_tags
    pub mod processing;  // enqueue, cancel, get_queue_status
    pub mod ai;          // complete, complete_stream, list_providers, list_models
    pub mod agents;      // execute_agent, cancel_agent, list_agents
    pub mod rag;         // search, ingest_document, delete_document, get_graph
    pub mod export;      // export_pdf, export_docx, export_fhir
    pub mod settings;    // get_settings, update_settings, get_key, set_key
    pub mod translation; // translate, detect_language, list_languages
    pub mod tts;         // synthesize, play, stop, list_voices
}
```

Commands are thin bridges — no business logic. Type conversion and error mapping only.

### App State

```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub audio: Arc<AudioEngine>,
    pub ai: Arc<ProviderRegistry>,
    pub stt: Arc<SttFailover>,
    pub tts: Arc<dyn TtsProvider>,
    pub agents: Arc<AgentOrchestrator>,
    pub rag: Arc<HybridRetriever>,
    pub keys: Arc<KeyStorage>,
    pub processing: Arc<ProcessingQueue>,
}
```

### Event Streams (Rust -> Svelte)

```rust
enum TauriEvent {
    WaveformData(Vec<f32>),
    AiStreamChunk(StreamChunk),
    ProcessingProgress(ProcessingEvent),
    AgentToolCall(ToolCallInfo),
    RecordingStateChanged(RecordingState),
    RagSearchProgress(SearchProgress),
}
```

---

## 16. Svelte Frontend (`src/`)

### Structure

```
src/
├── lib/
│   ├── stores/              (reactive state)
│   │   ├── recordings.ts    audio.ts    processing.ts
│   │   ├── ai.ts            chat.ts     rag.ts
│   │   ├── settings.ts      translation.ts
│   │
│   ├── components/          (25+ reusable components)
│   │   ├── Waveform.svelte          RecordingCard.svelte
│   │   ├── SoapEditor.svelte        ChatMessage.svelte
│   │   ├── AgentActivity.svelte     KnowledgeGraph.svelte
│   │   ├── SearchBar.svelte         ProgressBar.svelte
│   │   ├── StatusIndicator.svelte   KeyboardShortcut.svelte
│   │   └── ...
│   │
│   ├── layouts/
│   │   └── AppLayout.svelte  (sidebar + tabs + status bar)
│   │
│   ├── pages/               (tab content)
│   │   ├── RecordTab.svelte       ProcessTab.svelte
│   │   ├── GenerateTab.svelte     RecordingsTab.svelte
│   │   ├── ChatTab.svelte         EditorTab.svelte (x6, generic user-titled text editors)
│   │
│   ├── dialogs/
│   │   ├── Settings.svelte          TemplateSelector.svelte
│   │   ├── BatchProgress.svelte     RsvpReader.svelte
│   │   ├── AddressBook.svelte       ContactImport.svelte
│   │
│   ├── actions/             (Svelte actions)
│   │   ├── shortcut.ts      tooltip.ts      clickOutside.ts
│   │
│   └── api/                 (Tauri invoke wrappers)
│       ├── audio.ts    recordings.ts    processing.ts
│       ├── ai.ts       agents.ts        rag.ts
│       ├── export.ts   settings.ts      translation.ts
│
├── app.html    app.css    routes/+page.svelte
```

### Key UI Features

- **Theming:** CSS custom properties, dark/light toggle
- **Keyboard shortcuts:** 50+ shortcuts via global `shortcut.ts` action, help dialog for discovery
- **Knowledge graph:** D3.js or Cytoscape.js interactive canvas with pan/zoom/drag
- **Waveform:** Canvas-based real-time display during recording
- **Streaming AI:** Incremental text rendering from Tauri events
- **RSVP Reader:** Speed-reading dialog with ORP (Optimal Recognition Point) highlighting

### Reactivity Flow

```
User action -> Svelte component
  -> invoke() Tauri command -> Rust processes
  -> returns result OR emits event stream
  -> Svelte store updates -> components re-render
```

---

## 17. Application Lifecycle

### Startup Sequence

1. Parse CLI args (`--dev`, `--storage-path`, `--log-level`)
2. Initialize tracing (structured logging with PHI redaction)
3. Resolve data directory (platform-specific: `~/.local/share/`, `~/Library/Application Support/`, `%APPDATA%/`)
4. Open/migrate SQLite database
5. Open/migrate CozoDB
6. Initialize KeyStorage (detect Fernet migration if needed)
7. Load settings from DB
8. Initialize provider registries (AI, STT, TTS) — validate keys, fetch models
9. Initialize AudioEngine (enumerate devices)
10. Initialize RAG HybridRetriever
11. Initialize ProcessingQueue (resume interrupted jobs)
12. Initialize AgentOrchestrator
13. Build AppState, launch Tauri window

### Graceful Shutdown

1. Cancel in-flight agent executions
2. Drain processing queue (mark in-progress as pending for resume)
3. Stop audio recording if active (save partial file)
4. Flush audit log
5. Close database connections
6. Close CozoDB
7. Exit

### Settings Model

```rust
pub struct AppConfig {
    // General
    pub theme: Theme,                     // Dark | Light
    pub language: String,
    pub storage_path: Option<PathBuf>,

    // Audio
    pub input_device: Option<String>,
    pub sample_rate: u32,                 // default 44100
    pub channels: u16,                    // default 1

    // Providers
    pub ai_provider: String,
    pub ai_model: String,
    pub stt_provider: String,
    pub stt_failover_chain: Vec<String>,
    pub tts_provider: String,
    pub tts_voice: String,

    // Processing
    pub auto_generate_referral: bool,
    pub auto_generate_letter: bool,
    pub auto_index_rag: bool,
    pub icd_version: IcdVersion,          // Icd9 | Icd10 | Both

    // Templates
    pub soap_template: SoapTemplate,      // FollowUp | NewPatient | Telehealth | ...
    pub custom_soap_prompt: Option<String>,
    pub custom_referral_prompt: Option<String>,
    pub custom_letter_prompt: Option<String>,

    // RAG
    pub embedding_model: String,
    pub search_top_k: usize,              // default 5
    pub mmr_lambda: f32,                  // default 0.7

    // Autosave
    pub autosave_interval_secs: u64,      // default 60
}
```

---

## 18. Testing Strategy

1. **Unit tests alongside features:** Each ported module gets `#[cfg(test)]` tests mirroring the Python test intent, not implementation. Rust's type system makes many Python tests compile-time guarantees.

2. **Property-based tests:** `proptest` crate for core logic invariants:
   - Transcription output is always valid UTF-8
   - Graph traversals never panic on malformed input
   - Encryption round-trips are lossless
   - Query expansion is idempotent
   - Recording state machine transitions are valid

3. **Integration tests:** `tests/` directory exercising full workflows (record -> process -> generate). Safety net for cross-crate interactions.

4. **UI tests deferred:** Tauri's test suite covers the bridge. WebDriver tests added after UI stabilizes.

5. **Coverage target:** 80%, not 100%. Skip trivial coverage.

---

## 19. Key Dependencies

| Purpose | Crate |
|---------|-------|
| Async runtime | `tokio` |
| HTTP client | `reqwest` |
| SSE streaming | `eventsource-stream` |
| Serialization | `serde`, `serde_json` |
| Error handling | `thiserror`, `anyhow` |
| SQLite | `rusqlite` |
| Vector search | `sqlite-vec` |
| Knowledge graph | `cozo` |
| Connection pool | `r2d2` |
| Audio capture | `cpal` |
| Audio playback | `rodio` |
| WAV encoding | `hound` |
| Ring buffer | `ringbuf` |
| Whisper STT | `whisper-rs` |
| Encryption | `aes-gcm`, `pbkdf2`, `sha2` |
| Regex | `regex` |
| PDF export | `printpdf` |
| DOCX export | `docx-rs` |
| OCR | `tesseract-rs` |
| Graph (in-memory) | `petgraph` |
| Property testing | `proptest` |
| Logging | `tracing`, `tracing-subscriber` |
| CLI args | `clap` |
| Dates | `chrono` |
| UUIDs | `uuid` |
| Token counting | `tiktoken-rs` |
| Cancellation | `tokio-util` (CancellationToken) |
| Machine ID | `machine-uid` |
| Tauri | `tauri` v2 |

---

## 20. Shared Patterns Across Provider Crates

- **Circuit breaker:** Track consecutive failures per provider, back off after N errors
- **Request timeout:** Configurable per provider
- **Rate limiting:** Token bucket algorithm per endpoint
- **Token counting:** `tiktoken-rs` for OpenAI-compatible models
- **Retry with backoff:** Exponential backoff for transient failures
