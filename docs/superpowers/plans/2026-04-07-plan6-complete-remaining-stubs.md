# Plan 6: Complete All Remaining Stubs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement every remaining stub to achieve full feature parity — RAG system (embeddings, vector store, BM25, graph, ingestion), DB repos, real processing pipeline, local Whisper STT, cross-platform TTS, AI translation, agent RAG tool, and Modulate STT.

**Architecture:** Fill in existing stub interfaces — no architectural changes. New deps: `cozo` (graph DB), `whisper-rs` (local STT, feature-gated), `tts` (platform TTS), `bytemuck` (vector serialization). Vector store uses SQLite BLOB + Rust cosine. BM25 uses FTS5. Graph uses CozoDB Datalog.

**Tech Stack:** rusqlite + FTS5, cozo, whisper-rs, tts crate, bytemuck, reqwest (OpenAI embeddings)

---

### Task 1: Database Migration for RAG Tables

**Files:**
- Create: `crates/db/src/migrations/m002_rag_tables.rs`
- Modify: `crates/db/src/migrations/mod.rs`

- [ ] **Step 1: Create m002_rag_tables.rs**

```rust
// crates/db/src/migrations/m002_rag_tables.rs
use rusqlite::Connection;
use crate::DbResult;

pub fn up(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS document_chunks (
            id          TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            content     TEXT NOT NULL,
            embedding   BLOB,
            chunk_index INTEGER DEFAULT 0,
            metadata    TEXT DEFAULT '{}',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_chunks_doc ON document_chunks(document_id);

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            content,
            content='document_chunks',
            content_rowid='rowid',
            tokenize='porter unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS chunks_fts_insert AFTER INSERT ON document_chunks BEGIN
            INSERT INTO chunks_fts(rowid, content) VALUES (new.rowid, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS chunks_fts_delete AFTER DELETE ON document_chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
        END;
        CREATE TRIGGER IF NOT EXISTS chunks_fts_update AFTER UPDATE ON document_chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
            INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('insert', new.rowid, new.content);
        END;
    "#)?;
    Ok(())
}
```

- [ ] **Step 2: Register migration in mod.rs**

Add `pub mod m002_rag_tables;` to `crates/db/src/migrations/mod.rs` and add the migration to `all_migrations()`:

```rust
pub fn all_migrations() -> &'static [Migration] {
    &[
        Migration { version: 1, name: "initial_schema", up: m001_initial::up },
        Migration { version: 2, name: "rag_tables", up: m002_rag_tables::up },
    ]
}
```

- [ ] **Step 3: Build and test**

Run: `cargo test -p medical-db`
Expected: All existing migration tests pass + new migration applies cleanly.

- [ ] **Step 4: Commit**

```bash
git add crates/db/src/migrations/
git commit -m "feat(db): add m002 migration for document_chunks and chunks_fts tables"
```

---

### Task 2: VectorsRepo — SQLite CRUD for Document Chunks

**Files:**
- Modify: `crates/db/src/vectors.rs`
- Modify: `crates/db/Cargo.toml` (add bytemuck)

- [ ] **Step 1: Add bytemuck to workspace and db crate**

Add `bytemuck = { version = "1", features = ["derive"] }` to `Cargo.toml` workspace deps and `crates/db/Cargo.toml`.

- [ ] **Step 2: Implement VectorsRepo**

```rust
// crates/db/src/vectors.rs
use rusqlite::Connection;
use uuid::Uuid;
use crate::DbResult;

pub struct VectorsRepo;

impl VectorsRepo {
    pub fn insert_chunk(
        conn: &Connection,
        id: &Uuid,
        document_id: &Uuid,
        content: &str,
        embedding: &[f32],
        chunk_index: u32,
        metadata: &serde_json::Value,
    ) -> DbResult<()> {
        let embedding_bytes: &[u8] = bytemuck::cast_slice(embedding);
        conn.execute(
            "INSERT OR REPLACE INTO document_chunks (id, document_id, content, embedding, chunk_index, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id.to_string(),
                document_id.to_string(),
                content,
                embedding_bytes,
                chunk_index,
                serde_json::to_string(metadata).unwrap_or_default(),
            ],
        )?;
        Ok(())
    }

    pub fn get_all_embeddings(conn: &Connection) -> DbResult<Vec<(String, String, Vec<f32>)>> {
        let mut stmt = conn.prepare(
            "SELECT id, content, embedding FROM document_chunks WHERE embedding IS NOT NULL"
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let blob: Vec<u8> = row.get(2)?;
            let embedding: Vec<f32> = bytemuck::cast_slice(&blob).to_vec();
            Ok((id, content, embedding))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn delete_by_document(conn: &Connection, document_id: &Uuid) -> DbResult<u64> {
        let deleted = conn.execute(
            "DELETE FROM document_chunks WHERE document_id = ?1",
            [document_id.to_string()],
        )?;
        Ok(deleted as u64)
    }

    pub fn count(conn: &Connection) -> DbResult<u64> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM document_chunks", [], |r| r.get(0)
        )?;
        Ok(count as u64)
    }

    pub fn search_fts(conn: &Connection, query: &str, top_k: usize) -> DbResult<Vec<(String, String, f64)>> {
        let mut stmt = conn.prepare(
            "SELECT dc.id, dc.content, rank
             FROM chunks_fts
             JOIN document_chunks dc ON dc.rowid = chunks_fts.rowid
             WHERE chunks_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![query, top_k as i64], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let mut results = Vec::new();
        for row in rows { results.push(row?); }
        Ok(results)
    }
}
```

- [ ] **Step 3: Add tests**

Test insert, retrieve, FTS search, delete in an in-memory DB (need to run migration first).

- [ ] **Step 4: Build and test, commit**

---

### Task 3: GraphRepo — CozoDB Wrapper

**Files:**
- Modify: `crates/db/src/graph.rs`
- Modify: `crates/db/Cargo.toml` (add cozo)

- [ ] **Step 1: Add cozo dependency**

Add to `crates/db/Cargo.toml`:
```toml
cozo = { version = "0.7", features = ["storage-sqlite"] }
```

- [ ] **Step 2: Implement GraphRepo**

```rust
// crates/db/src/graph.rs
use cozo::DbInstance;
use uuid::Uuid;
use crate::DbResult;
use medical_core::types::rag::{GraphEntity, GraphRelation, EntityType, RelationType};

pub struct GraphRepo {
    db: DbInstance,
}

impl GraphRepo {
    pub fn open(path: &std::path::Path) -> DbResult<Self> {
        let db = DbInstance::new("sqlite", path.to_str().unwrap_or("graph.db"), "")
            .map_err(|e| crate::DbError::Migration(format!("CozoDB open: {e}")))?;
        // Create schema relations if they don't exist
        let _ = db.run_script(
            ":create entity {id: String => name: String, entity_type: String, properties: String}",
            Default::default(), false,
        );
        let _ = db.run_script(
            ":create relation {from_id: String, to_id: String => relation_type: String, weight: Float, properties: String}",
            Default::default(), false,
        );
        Ok(Self { db })
    }

    pub fn insert_entity(&self, entity: &GraphEntity) -> DbResult<()> {
        let props = serde_json::to_string(&entity.properties).unwrap_or_default();
        let entity_type = serde_json::to_string(&entity.entity_type).unwrap_or_default();
        self.db.run_script(
            "?[id, name, entity_type, properties] <- [[$id, $name, $type, $props]]
             :put entity {id => name, entity_type, properties}",
            btreemap!{
                "id".to_string() => cozo::DataValue::Str(entity.id.to_string().into()),
                "name".to_string() => cozo::DataValue::Str(entity.name.clone().into()),
                "type".to_string() => cozo::DataValue::Str(entity_type.into()),
                "props".to_string() => cozo::DataValue::Str(props.into()),
            }.into(),
            false,
        ).map_err(|e| crate::DbError::Migration(format!("CozoDB insert entity: {e}")))?;
        Ok(())
    }

    pub fn insert_relation(&self, rel: &GraphRelation) -> DbResult<()> {
        let rel_type = serde_json::to_string(&rel.relation_type).unwrap_or_default();
        let props = serde_json::to_string(&rel.properties).unwrap_or_default();
        self.db.run_script(
            "?[from_id, to_id, relation_type, weight, properties] <- [[$from, $to, $rtype, 1.0, $props]]
             :put relation {from_id, to_id => relation_type, weight, properties}",
            btreemap!{
                "from".to_string() => cozo::DataValue::Str(rel.from.to_string().into()),
                "to".to_string() => cozo::DataValue::Str(rel.to.to_string().into()),
                "rtype".to_string() => cozo::DataValue::Str(rel_type.into()),
                "props".to_string() => cozo::DataValue::Str(props.into()),
            }.into(),
            false,
        ).map_err(|e| crate::DbError::Migration(format!("CozoDB insert relation: {e}")))?;
        Ok(())
    }

    pub fn query_related(&self, entity_name: &str, top_k: usize) -> DbResult<Vec<(String, String, String)>> {
        let result = self.db.run_script(
            "?[name, entity_type, relation_type] :=
                *entity{id: eid, name: $query_name},
                *relation{from_id: eid, to_id: tid, relation_type},
                *entity{id: tid, name, entity_type}
             :limit $limit",
            btreemap!{
                "query_name".to_string() => cozo::DataValue::Str(entity_name.into()),
                "limit".to_string() => cozo::DataValue::from(top_k as i64),
            }.into(),
            false,
        ).map_err(|e| crate::DbError::Migration(format!("CozoDB query: {e}")))?;

        let mut results = Vec::new();
        for row in result.rows {
            if row.len() >= 3 {
                let name = row[0].get_str().unwrap_or_default().to_string();
                let etype = row[1].get_str().unwrap_or_default().to_string();
                let rtype = row[2].get_str().unwrap_or_default().to_string();
                results.push((name, etype, rtype));
            }
        }
        Ok(results)
    }

    pub fn search_by_name(&self, query: &str, top_k: usize) -> DbResult<Vec<(String, String, String)>> {
        let result = self.db.run_script(
            "?[id, name, entity_type] :=
                *entity{id, name, entity_type},
                contains(name, $query)
             :limit $limit",
            btreemap!{
                "query".to_string() => cozo::DataValue::Str(query.into()),
                "limit".to_string() => cozo::DataValue::from(top_k as i64),
            }.into(),
            false,
        ).map_err(|e| crate::DbError::Migration(format!("CozoDB search: {e}")))?;

        let mut results = Vec::new();
        for row in result.rows {
            if row.len() >= 3 {
                let id = row[0].get_str().unwrap_or_default().to_string();
                let name = row[1].get_str().unwrap_or_default().to_string();
                let etype = row[2].get_str().unwrap_or_default().to_string();
                results.push((id, name, etype));
            }
        }
        Ok(results)
    }
}
```

- [ ] **Step 3: Add tests**

Test entity/relation insert and query with in-memory CozoDB (`DbInstance::new("mem", "", "")`).

- [ ] **Step 4: Build and test, commit**

---

### Task 4: ProcessingQueueRepo — SQLite Job Queue

**Files:**
- Modify: `crates/db/src/processing_queue.rs`

- [ ] **Step 1: Implement ProcessingQueueRepo**

CRUD operations against the existing `processing_queue` table from m001. Methods: `enqueue()`, `dequeue()`, `update_status()`, `get_by_recording()`, `count_pending()`.

- [ ] **Step 2: Tests and commit**

---

### Task 5: Embeddings — Real OpenAI/Ollama API

**Files:**
- Modify: `crates/rag/src/embeddings.rs`
- Modify: `crates/rag/Cargo.toml` (add reqwest)

- [ ] **Step 1: Implement EmbeddingGenerator with HTTP backends**

```rust
// crates/rag/src/embeddings.rs
use medical_core::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct EmbeddingGenerator {
    client: Client,
    backend: EmbeddingBackend,
    dimension: usize,
}

enum EmbeddingBackend {
    OpenAi { api_key: String },
    Ollama { host: String, model: String },
}

#[derive(Serialize)]
struct OpenAiEmbeddingRequest { model: String, input: Vec<String> }

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse { data: Vec<OpenAiEmbeddingData> }

#[derive(Deserialize)]
struct OpenAiEmbeddingData { embedding: Vec<f32> }

#[derive(Serialize)]
struct OllamaEmbeddingRequest { model: String, prompt: String }

#[derive(Deserialize)]
struct OllamaEmbeddingResponse { embedding: Vec<f32> }

impl EmbeddingGenerator {
    pub fn new_openai(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            backend: EmbeddingBackend::OpenAi { api_key: api_key.to_string() },
            dimension: 1536,
        }
    }

    pub fn new_ollama(host: Option<&str>, model: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            backend: EmbeddingBackend::Ollama {
                host: host.unwrap_or("http://localhost:11434").to_string(),
                model: model.unwrap_or("nomic-embed-text").to_string(),
            },
            dimension: 768,
        }
    }

    pub fn dimension(&self) -> usize { self.dimension }

    pub async fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        match &self.backend {
            EmbeddingBackend::OpenAi { api_key } => {
                let resp = self.client
                    .post("https://api.openai.com/v1/embeddings")
                    .bearer_auth(api_key)
                    .json(&OpenAiEmbeddingRequest {
                        model: "text-embedding-3-small".into(),
                        input: vec![text.to_string()],
                    })
                    .send().await
                    .map_err(|e| AppError::Provider(format!("Embedding HTTP: {e}")))?;
                let body: OpenAiEmbeddingResponse = resp.json().await
                    .map_err(|e| AppError::Provider(format!("Embedding JSON: {e}")))?;
                body.data.into_iter().next()
                    .map(|d| d.embedding)
                    .ok_or_else(|| AppError::Provider("No embedding returned".into()))
            }
            EmbeddingBackend::Ollama { host, model } => {
                let resp = self.client
                    .post(format!("{host}/api/embeddings"))
                    .json(&OllamaEmbeddingRequest { model: model.clone(), prompt: text.to_string() })
                    .send().await
                    .map_err(|e| AppError::Provider(format!("Ollama embedding: {e}")))?;
                let body: OllamaEmbeddingResponse = resp.json().await
                    .map_err(|e| AppError::Provider(format!("Ollama JSON: {e}")))?;
                Ok(body.embedding)
            }
        }
    }

    pub async fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        // OpenAI supports batch natively; for Ollama, iterate
        match &self.backend {
            EmbeddingBackend::OpenAi { api_key } => {
                let resp = self.client
                    .post("https://api.openai.com/v1/embeddings")
                    .bearer_auth(api_key)
                    .json(&OpenAiEmbeddingRequest {
                        model: "text-embedding-3-small".into(),
                        input: texts.iter().map(|t| t.to_string()).collect(),
                    })
                    .send().await
                    .map_err(|e| AppError::Provider(format!("Batch embedding: {e}")))?;
                let body: OpenAiEmbeddingResponse = resp.json().await
                    .map_err(|e| AppError::Provider(format!("Batch JSON: {e}")))?;
                Ok(body.data.into_iter().map(|d| d.embedding).collect())
            }
            EmbeddingBackend::Ollama { .. } => {
                let mut results = Vec::with_capacity(texts.len());
                for text in texts { results.push(self.embed(text).await?); }
                Ok(results)
            }
        }
    }
}

impl Default for EmbeddingGenerator {
    fn default() -> Self { Self::new_ollama(None, None) }
}
```

- [ ] **Step 2: Tests (mock HTTP not needed — test construction and dimension)**

- [ ] **Step 3: Build, commit**

---

### Task 6: Vector Store — Real Cosine Similarity Search

**Files:**
- Modify: `crates/rag/src/vector_store.rs`
- Modify: `crates/rag/Cargo.toml` (add bytemuck, medical-db)

- [ ] **Step 1: Implement VectorStore with SQLite-backed storage**

Constructor takes `Arc<Database>`. `store_chunk()` writes to DB via VectorsRepo. `search()` loads all embeddings, computes cosine similarity in Rust, returns top-k above threshold. `delete_document()` removes from DB.

Use `medical_rag::mmr::cosine_similarity` (already implemented) for the similarity computation.

- [ ] **Step 2: Tests with in-memory DB**

- [ ] **Step 3: Build, commit**

---

### Task 7: BM25 Search — FTS5 Integration

**Files:**
- Modify: `crates/rag/src/bm25.rs`

- [ ] **Step 1: Implement Bm25Search with FTS5**

Constructor takes `Arc<Database>`. `search()` calls `VectorsRepo::search_fts()` and converts results to `RagResult` format.

- [ ] **Step 2: Tests, build, commit**

---

### Task 8: Graph Search — CozoDB Integration

**Files:**
- Modify: `crates/rag/src/graph_search.rs`

- [ ] **Step 1: Implement GraphSearch with GraphRepo**

Constructor takes `Arc<GraphRepo>`. `search()` queries CozoDB for entities matching the query. `store_entity()` and `store_relation()` delegate to GraphRepo.

- [ ] **Step 2: Tests, build, commit**

---

### Task 9: Ingestion Pipeline — Full Wiring

**Files:**
- Modify: `crates/rag/src/ingestion.rs`

- [ ] **Step 1: Implement IngestionPipeline**

Constructor takes references to EmbeddingGenerator, VectorStore, Bm25Search (implicit via DB), and GraphSearch. `ingest_text()`:
1. Call `chunk_text()` (already implemented)
2. Build DocumentChunk for each with UUID
3. Call `embed_batch()` for all chunk texts
4. Store each chunk via VectorStore (which writes to DB, auto-triggers FTS5)
5. Extract medical entities with simple regex patterns (drug names, conditions)
6. Store entities via GraphSearch
7. Return chunk count

- [ ] **Step 2: Tests with mocked embedding (return fixed vectors)**

- [ ] **Step 3: Build, commit**

---

### Task 10: Agent RAG Search Tool — Connect to Real RAG

**Files:**
- Modify: `crates/agents/src/tools/rag_search.rs`

- [ ] **Step 1: Implement real RAG search**

The tool needs access to the RAG system. Since tools are created via `ToolRegistry::with_defaults()`, we need to pass the RAG components in. Modify `RagSearchTool` to optionally hold references, falling back to "not configured" when None.

The execute method: embed query → search vector store → search BM25 → fuse with RRF → rerank with MMR → return results as JSON.

- [ ] **Step 2: Update tests, build, commit**

---

### Task 11: Local Whisper STT (Feature-gated)

**Files:**
- Modify: `crates/stt-providers/src/whisper_local.rs`
- Modify: `crates/stt-providers/Cargo.toml`

- [ ] **Step 1: Add whisper-rs dependency (feature-gated)**

In `crates/stt-providers/Cargo.toml`:
```toml
[features]
default = []
local-stt = ["whisper-rs"]

[dependencies]
whisper-rs = { version = "0.13", optional = true }
```

- [ ] **Step 2: Implement WhisperLocalProvider**

```rust
#[cfg(feature = "local-stt")]
pub struct WhisperLocalProvider {
    model_path: PathBuf,
}

#[cfg(feature = "local-stt")]
impl WhisperLocalProvider {
    pub fn new(model_path: PathBuf) -> Result<Self, AppError> {
        if !model_path.exists() {
            return Err(AppError::SttProvider(format!(
                "Whisper model not found at {}. Download ggml-base.en.bin to this path.",
                model_path.display()
            )));
        }
        Ok(Self { model_path })
    }
}

#[cfg(feature = "local-stt")]
#[async_trait]
impl SttProvider for WhisperLocalProvider {
    fn name(&self) -> &str { "whisper-local" }
    fn supports_streaming(&self) -> bool { false }
    fn supports_diarization(&self) -> bool { false }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let model_path = self.model_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            let ctx = whisper_rs::WhisperContext::new_with_params(
                model_path.to_str().unwrap(),
                whisper_rs::WhisperContextParameters::default(),
            ).map_err(|e| AppError::SttProvider(format!("Whisper init: {e}")))?;

            let mut state = ctx.create_state()
                .map_err(|e| AppError::SttProvider(format!("Whisper state: {e}")))?;

            let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
            if let Some(ref lang) = config.language {
                params.set_language(Some(lang));
            }
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);

            state.full(params, &audio.samples)
                .map_err(|e| AppError::SttProvider(format!("Whisper transcribe: {e}")))?;

            let num_segments = state.full_n_segments()
                .map_err(|e| AppError::SttProvider(format!("Whisper segments: {e}")))?;
            let mut text = String::new();
            let mut segments = Vec::new();
            for i in 0..num_segments {
                let seg_text = state.full_get_segment_text(i)
                    .map_err(|e| AppError::SttProvider(format!("Whisper segment text: {e}")))?;
                let start = state.full_get_segment_t0(i)
                    .map_err(|e| AppError::SttProvider(format!("Whisper t0: {e}")))? as f64 / 100.0;
                let end = state.full_get_segment_t1(i)
                    .map_err(|e| AppError::SttProvider(format!("Whisper t1: {e}")))? as f64 / 100.0;
                text.push_str(&seg_text);
                text.push(' ');
                segments.push(medical_core::types::stt::TranscriptSegment {
                    text: seg_text,
                    start, end,
                    speaker: None,
                    confidence: None,
                });
            }

            Ok::<Transcript, AppError>(Transcript {
                text: text.trim().to_string(),
                segments,
                language: config.language,
                duration_seconds: Some(audio.duration_seconds()),
                provider: "whisper-local".to_string(),
                metadata: serde_json::Value::Null,
            })
        }).await.map_err(|e| AppError::SttProvider(format!("Whisper task: {e}")))?;

        result
    }

    async fn transcribe_stream(&self, _stream: AudioStream, _config: SttConfig)
        -> AppResult<Box<dyn futures_core::Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>> {
        Err(AppError::SttProvider("whisper-local: streaming not supported".into()))
    }
}
```

- [ ] **Step 3: Build (without feature flag), commit**

---

### Task 12: Cross-Platform TTS

**Files:**
- Modify: `crates/tts-providers/src/local_tts.rs`
- Modify: `crates/tts-providers/Cargo.toml`

- [ ] **Step 1: Add tts dependency**

In `crates/tts-providers/Cargo.toml`:
```toml
tts = "0.26"
```

- [ ] **Step 2: Implement LocalTtsProvider**

```rust
use async_trait::async_trait;
use medical_core::{error::{AppError, AppResult}, traits::TtsProvider, types::{TtsConfig, VoiceInfo}};

pub struct LocalTtsProvider {
    tts: std::sync::Mutex<tts::Tts>,
}

impl LocalTtsProvider {
    pub fn new() -> AppResult<Self> {
        let tts_instance = tts::Tts::default()
            .map_err(|e| AppError::Provider(format!("TTS init: {e}")))?;
        Ok(Self { tts: std::sync::Mutex::new(tts_instance) })
    }
}

#[async_trait]
impl TtsProvider for LocalTtsProvider {
    fn name(&self) -> &str { "local" }

    async fn available_voices(&self) -> AppResult<Vec<VoiceInfo>> {
        let tts = self.tts.lock().unwrap();
        let voices = tts.voices()
            .map_err(|e| AppError::Provider(format!("TTS voices: {e}")))?;
        Ok(voices.into_iter().map(|v| VoiceInfo {
            id: v.id().to_string(),
            name: v.name().to_string(),
            language: v.language().language.to_string(),
            provider: "local".into(),
        }).collect())
    }

    async fn synthesize(&self, text: &str, _config: TtsConfig) -> AppResult<Vec<u8>> {
        let mut tts = self.tts.lock().unwrap();
        tts.speak(text, false)
            .map_err(|e| AppError::Provider(format!("TTS speak: {e}")))?;
        // tts crate speaks directly to speakers; return empty bytes
        // (platform TTS doesn't produce a byte buffer easily)
        Ok(Vec::new())
    }
}

impl Default for LocalTtsProvider {
    fn default() -> Self { Self::new().unwrap_or_else(|_| Self { tts: std::sync::Mutex::new(tts::Tts::default().unwrap()) }) }
}
```

- [ ] **Step 3: Build, commit**

---

### Task 13: AI-Powered Translation

**Files:**
- Create: `crates/translation/src/ai_translator.rs`
- Modify: `crates/translation/src/lib.rs`

- [ ] **Step 1: Implement AiTranslationProvider**

```rust
use async_trait::async_trait;
use std::sync::Arc;
use medical_core::{
    error::AppResult,
    traits::{AiProvider, TranslationProvider},
    types::{CompletionRequest, Message, MessageContent, Role},
};

pub struct AiTranslationProvider {
    provider: Arc<dyn AiProvider>,
}

impl AiTranslationProvider {
    pub fn new(provider: Arc<dyn AiProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl TranslationProvider for AiTranslationProvider {
    fn name(&self) -> &str { "ai" }

    async fn supported_languages(&self) -> AppResult<Vec<medical_core::traits::Language>> {
        Ok(vec![
            Language { code: "en".into(), name: "English".into() },
            Language { code: "es".into(), name: "Spanish".into() },
            Language { code: "fr".into(), name: "French".into() },
            Language { code: "de".into(), name: "German".into() },
            Language { code: "zh".into(), name: "Chinese".into() },
            Language { code: "ja".into(), name: "Japanese".into() },
            Language { code: "ko".into(), name: "Korean".into() },
            Language { code: "pt".into(), name: "Portuguese".into() },
            Language { code: "ar".into(), name: "Arabic".into() },
            Language { code: "hi".into(), name: "Hindi".into() },
        ])
    }

    async fn translate(&self, text: &str, source: Option<&str>, target: &str) -> AppResult<String> {
        let source_str = source.unwrap_or("auto-detected language");
        let request = CompletionRequest {
            model: String::new(), // use provider default
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text(format!(
                    "Translate the following medical text from {source_str} to {target}. \
                     Preserve medical terminology accuracy. Return ONLY the translation, no explanation.\n\n{text}"
                )),
                tool_calls: vec![],
            }],
            temperature: Some(0.1),
            max_tokens: Some(4096),
            system_prompt: Some("You are a medical translator. Translate accurately, preserving clinical terminology.".into()),
        };
        let response = self.provider.complete(request).await?;
        Ok(response.content.unwrap_or_default())
    }

    async fn detect_language(&self, text: &str) -> AppResult<String> {
        let request = CompletionRequest {
            model: String::new(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text(format!(
                    "Detect the language of this text. Return ONLY the BCP-47 language code (e.g. 'en', 'es', 'fr'):\n\n{text}"
                )),
                tool_calls: vec![],
            }],
            temperature: Some(0.0),
            max_tokens: Some(10),
            system_prompt: None,
        };
        let response = self.provider.complete(request).await?;
        Ok(response.content.unwrap_or_else(|| "en".into()).trim().to_string())
    }
}
```

- [ ] **Step 2: Register module in lib.rs, tests, build, commit**

---

### Task 14: Modulate STT Provider

**Files:**
- Modify: `crates/stt-providers/src/modulate.rs`

- [ ] **Step 1: Implement ModulateProvider with HTTP API**

Implement `SttProvider` trait. POST audio to Modulate API, parse response. Similar pattern to Deepgram/Groq Whisper providers.

- [ ] **Step 2: Build, commit**

---

### Task 15: Integration — Wire RAG into AppState and Tauri

**Files:**
- Modify: `src-tauri/src/state.rs` (add RAG system to AppState)
- Create: `src-tauri/src/commands/rag.rs` (new Tauri commands)
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add RAG system to AppState**

Add `EmbeddingGenerator`, `VectorStore`, `GraphRepo` (wrapped in Arc/Mutex) to AppState. Initialize from API keys.

- [ ] **Step 2: Add Tauri commands**

- `ingest_document(recording_id)` — ingest a recording's transcript/SOAP into RAG
- `search_rag(query, top_k)` — search the RAG system
- `rag_stats()` — return chunk count and entity count

- [ ] **Step 3: Register commands, build, commit**

---

### Task 16: Integration Testing & Final Polish

**Files:**
- Various fixes from integration testing

- [ ] **Step 1: cargo test --workspace**
- [ ] **Step 2: cargo clippy --workspace**
- [ ] **Step 3: npm run build**
- [ ] **Step 4: cargo tauri dev — smoke test**
- [ ] **Step 5: Final commit**
