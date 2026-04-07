# Plan 6: Complete All Remaining Stubs — Design Spec

## Goal

Implement every remaining stub in the Rust Medical Assistant to achieve full feature parity with the Python original. This covers: RAG system (embeddings, vector search, BM25, graph search, ingestion), database repos (vectors, graph, processing queue), real processing pipeline, local Whisper STT, cross-platform TTS, AI-powered translation, agent RAG tool, and Modulate STT provider.

## Architecture Overview

All stubs follow the existing trait/interface patterns. No architectural changes — only filling in implementations behind existing APIs. New dependencies: `cozo` (graph DB), `whisper-rs` (local STT), `tts` (platform TTS). Vector storage uses SQLite BLOBs with Rust-side cosine similarity (no native C extensions needed).

---

## 1. RAG Embeddings (`crates/rag/src/embeddings.rs`)

**Current state:** Returns 1536-element zero vector.

**Implementation:** `EmbeddingService` wraps a reqwest HTTP client. Two backends:

- **OpenAI:** POST to `https://api.openai.com/v1/embeddings` with model `text-embedding-3-small` (1536 dims). Auth via Bearer token from KeyStorage `openai` key.
- **Ollama:** POST to `http://localhost:11434/api/embeddings` with model `nomic-embed-text` (768 dims). No auth needed.

The service auto-selects backend: if an OpenAI key exists, use it; otherwise fall back to Ollama.

**Interface (unchanged):**
```rust
impl EmbeddingGenerator {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String>;
    pub fn dimension(&self) -> usize;
}
```

**Dependencies:** `reqwest` (already available via workspace).

---

## 2. Vector Store (`crates/rag/src/vector_store.rs`)

**Current state:** All methods return empty/no-op.

**Implementation:** Store document chunk embeddings in SQLite as f32 BLOB arrays. Search computes cosine similarity in Rust — at medical-assistant corpus sizes (hundreds to low thousands of chunks), this is sub-millisecond.

**New SQLite table** (added via migration m002):
```sql
CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,
    chunk_index INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chunks_doc ON document_chunks(document_id);
```

**Interface (unchanged):**
```rust
impl VectorStore {
    pub async fn store_chunk(&self, chunk: &DocumentChunk) -> Result<(), String>;
    pub async fn search(&self, query_embedding: &[f32], top_k: usize, threshold: f32) -> Result<Vec<RagResult>, String>;
    pub async fn delete_document(&self, document_id: &str) -> Result<(), String>;
}
```

**Vector encoding:** `Vec<f32>` serialized to `Vec<u8>` via `bytemuck::cast_slice` (zero-copy). Stored as SQLite BLOB.

**Dependencies:** `bytemuck` (new, for zero-copy f32<->u8 conversion).

---

## 3. BM25 Search (`crates/rag/src/bm25.rs`)

**Current state:** Returns empty results.

**Implementation:** Use SQLite FTS5 (already compiled into rusqlite). Add an FTS5 virtual table for document chunks:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    content,
    content_rowid='rowid',
    tokenize='porter unicode61'
);
```

Synchronized via triggers on `document_chunks` INSERT/UPDATE/DELETE (same pattern as `recordings_fts`).

**Interface (unchanged):**
```rust
impl Bm25Search {
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<RagResult>, String>;
}
```

FTS5's built-in `bm25()` ranking function handles scoring.

**Dependencies:** None new — rusqlite already has FTS5.

---

## 4. Graph Search via CozoDB (`crates/rag/src/graph_search.rs`)

**Current state:** All methods return empty/no-op.

**Implementation:** Use `cozo` crate (pure Rust embedded graph DB). CozoDB stores to a RocksDB-backed file at `data_dir/graph.db`. Schema uses Datalog relations:

```
:create entity {id: String, name: String, entity_type: String, metadata: String => }
:create relation {from_id: String, to_id: String, relation_type: String, weight: Float => }
```

**Queries:** CozoDB Datalog for graph traversal:
```
?[name, entity_type, score] :=
    *entity{id: eid, name, entity_type},
    *relation{from_id: start_id, to_id: eid, weight: score},
    start_id = $start,
    score > 0.5
:order -score
:limit $top_k
```

**Interface (unchanged):**
```rust
impl GraphSearch {
    pub async fn store_entity(&self, entity: &GraphEntity) -> Result<(), String>;
    pub async fn store_relation(&self, relation: &GraphRelation) -> Result<(), String>;
    pub async fn search(&self, query: &str, entity_types: &[EntityType], top_k: usize) -> Result<Vec<RagResult>, String>;
}
```

**Dependencies:** `cozo` crate (pure Rust, embeds RocksDB).

---

## 5. RAG Ingestion Pipeline (`crates/rag/src/ingestion.rs`)

**Current state:** `ingest_text()` returns 0 chunks. `chunk_text()` is already implemented.

**Implementation:** Wire the full pipeline:
1. Call `chunk_text()` to split document into overlapping chunks
2. Call `EmbeddingGenerator::embed_batch()` on all chunks
3. Store each chunk + embedding in VectorStore
4. Insert chunk content into FTS5 for BM25
5. Extract medical entities (drug names, conditions, procedures) from text using regex patterns
6. Store entities and relations in GraphSearch
7. Return count of chunks ingested

**Entity extraction:** Use regex patterns to identify medical terms (drug names from a built-in list, ICD code references, anatomy terms). This is best-effort — the knowledge graph enriches search but isn't required for basic RAG.

---

## 6. Database Repos (`crates/db/src/`)

### VectorsRepo (`vectors.rs`)
CRUD for `document_chunks` table:
- `insert_chunk(conn, id, document_id, content, embedding, chunk_index, metadata)`
- `get_all_embeddings(conn)` — returns all (id, content, embedding) for similarity search
- `search_by_document(conn, document_id)` — get chunks for a document
- `delete_by_document(conn, document_id)` — remove all chunks for a document
- `count(conn)` — total chunk count

### GraphRepo (`graph.rs`)
Wrapper around CozoDB instance:
- `open(path)` — open/create CozoDB database, run schema setup
- `insert_entity(entity)` — insert entity node
- `insert_relation(relation)` — insert edge
- `query_related(entity_id, top_k)` — find related entities
- `search_by_name(query, entity_types, top_k)` — text search on entity names

### ProcessingQueueRepo (`processing_queue.rs`)
CRUD for the existing `processing_queue` SQLite table:
- `enqueue(conn, recording_id, task_type, priority)`
- `dequeue(conn)` — get next pending task (ordered by priority, created_at)
- `update_status(conn, task_id, status)`
- `get_by_recording(conn, recording_id)` — all tasks for a recording
- `count_pending(conn)` — pending task count

---

## 7. Processing Pipeline — Real Implementation (`crates/processing/src/pipeline.rs`)

**Current state:** Emits mock events with 0ms duration, no actual work.

**Implementation:** Replace mock events with actual calls:

1. **Transcription step:** Load WAV → call STT failover → store transcript
2. **SOAP generation step:** Build SOAP prompt → call AI provider → store result
3. **Referral step (optional):** Build referral prompt → call AI → store
4. **Letter step (optional):** Build letter prompt → call AI → store
5. **Ingestion step:** Ingest transcript + SOAP into RAG system

Each step emits real ProcessingEvents with actual duration_ms. The pipeline accepts provider references (AI + STT) as parameters so it's testable.

---

## 8. Local Whisper STT (`crates/stt-providers/src/whisper_local.rs`)

**Current state:** Empty struct with constructor only.

**Implementation:** Use `whisper-rs` crate (Rust bindings to whisper.cpp).

- Model: `ggml-base.en.bin` (~150MB), stored in `data_dir/models/`
- On first use: check if model exists, if not return error with download instructions
- Implements `SttProvider` trait: `transcribe()` loads audio, runs inference, returns Transcript
- Feature-gated: `#[cfg(feature = "local-stt")]` in Cargo.toml so builds without whisper.cpp work

**Interface:** Implements existing `SttProvider` trait — no changes needed.

**Dependencies:** `whisper-rs = "0.13"` (feature-gated).

---

## 9. Cross-Platform TTS (`crates/tts-providers/src/local_tts.rs`)

**Current state:** Empty struct with constructor only.

**Implementation:** Use `tts` crate which wraps:
- Linux: speech-dispatcher
- macOS: AVSpeechSynthesizer
- Windows: SAPI

Implements `TtsProvider` trait:
- `available_voices()` — query system voices
- `synthesize(text, config)` — speak text, return audio bytes

The `tts` crate handles platform detection automatically.

**Dependencies:** `tts = "0.26"`.

---

## 10. AI-Powered Translation (`crates/translation/src/`)

**Current state:** Only canned responses (7 phrases in 5 languages).

**Implementation:** Add `AiTranslationProvider` that wraps an `AiProvider`:
- Build a translation prompt: "Translate the following medical text from {source} to {target}. Preserve medical terminology accuracy."
- Call `provider.complete()` with the prompt
- Return translated text

Integrates with existing `TranslationSession` for conversation tracking.

**Dependencies:** None new — uses existing AI provider infrastructure.

---

## 11. Agent RAG Tool (`crates/agents/src/tools/rag_search.rs`)

**Current state:** Returns empty results with a "not connected" note.

**Implementation:** Wire to real RAG system:
1. Parse query from tool arguments
2. Call `EmbeddingGenerator::embed()` on the query
3. Run parallel searches: vector store + BM25 + graph search
4. Fuse results with `reciprocal_rank_fusion()` (already implemented)
5. Rerank with `mmr_rerank()` (already implemented)
6. Return top-k results as JSON

---

## 12. Modulate STT Provider (`crates/stt-providers/src/modulate.rs`)

**Current state:** Constructor takes API key but does nothing.

**Implementation:** HTTP client for Modulate API:
- POST audio to Modulate endpoint
- Parse transcription response
- Implement `SttProvider` trait

Lower priority — Modulate is less commonly used than Deepgram/Groq/ElevenLabs.

---

## New Dependencies Summary

| Crate | Version | Purpose | Feature-gated? |
|-------|---------|---------|----------------|
| `cozo` | latest | Embedded graph DB | No |
| `whisper-rs` | 0.13 | Local Whisper STT | Yes (`local-stt`) |
| `tts` | 0.26 | Cross-platform TTS | No |
| `bytemuck` | 1.x | Zero-copy f32/u8 conversion | No |

## Migration

Add migration `m002_rag_tables.rs`:
- `document_chunks` table
- `chunks_fts` FTS5 virtual table
- INSERT/DELETE triggers for FTS5 sync
- `processing_queue` table updates (if needed)

CozoDB schema is managed by the `GraphRepo::open()` method (separate from SQLite migrations).

## Testing

Each stub implementation gets tests alongside the code:
- Embedding: mock HTTP responses, verify vector dimensions
- Vector store: in-memory SQLite, store and retrieve chunks, verify cosine similarity ranking
- BM25: in-memory SQLite with FTS5, verify ranking
- Graph: in-memory CozoDB, store entities/relations, query
- Ingestion: end-to-end with mock embeddings
- Processing pipeline: mock providers, verify event sequence and actual calls
- Whisper: unit tests for audio preprocessing (actual inference needs model file)
- TTS: verify voice listing (platform-dependent)
- Translation: mock AI provider, verify prompt construction
