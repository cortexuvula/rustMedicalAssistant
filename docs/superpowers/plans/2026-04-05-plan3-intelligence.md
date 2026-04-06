# Plan 3: Intelligence — Agents, RAG, Processing, Export, Translation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the 5 business-logic crates (`agents`, `rag`, `processing`, `export`, `translation`) that implement medical documentation intelligence — AI agents with tools, hybrid RAG search with MMR reranking, recording processing pipeline, document export (PDF/DOCX/FHIR), and bidirectional translation.

**Architecture:** The `rag` crate implements hybrid 3-way search (vector + BM25 + graph) with query expansion, reciprocal rank fusion, and MMR reranking. The `agents` crate provides 8 specialized medical agents (Medication, Diagnostic, Compliance, DataExtract, Workflow, Referral, Synopsis, Chat) with a tool execution loop. The `processing` crate orchestrates the recording pipeline (STT → SOAP → export → RAG index). Export and translation are standalone.

**Tech Stack:** cozo (embedded graph DB), printpdf, docx-rs, serde, tokio, reqwest (for embeddings)

**Depends on:** Plans 1-2 (core types/traits, db, security, audio, ai-providers, stt-providers, tts-providers)

---

## File Structure

```
crates/
├── agents/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (AgentOrchestrator, re-exports)
│       ├── orchestrator.rs         (tool execution loop, cancellation)
│       ├── tools/
│       │   ├── mod.rs              (ToolRegistry)
│       │   ├── icd_lookup.rs       (ICD code search tool)
│       │   ├── drug_interaction.rs (drug interaction check tool)
│       │   ├── vitals_extractor.rs (vitals extraction tool)
│       │   ├── rag_search.rs       (RAG search tool)
│       │   └── checklist.rs        (checklist generation tool)
│       └── agents/
│           ├── mod.rs              (agent registry)
│           ├── medication.rs       (MedicationAgent)
│           ├── diagnostic.rs       (DiagnosticAgent)
│           ├── compliance.rs       (ComplianceAgent)
│           ├── data_extraction.rs  (DataExtractionAgent)
│           ├── workflow.rs         (WorkflowAgent)
│           ├── referral.rs         (ReferralAgent)
│           ├── synopsis.rs         (SynopsisAgent)
│           └── chat.rs             (ChatAgent)
├── rag/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (HybridRetriever, re-exports)
│       ├── query_expander.rs       (medical abbreviation + synonym expansion)
│       ├── vector_store.rs         (SQLite-backed vector storage + cosine search)
│       ├── bm25.rs                 (BM25 keyword search via FTS5)
│       ├── graph_search.rs         (CozoDB graph traversal)
│       ├── fusion.rs               (reciprocal rank fusion)
│       ├── mmr.rs                  (maximal marginal relevance reranker)
│       ├── embeddings.rs           (embedding generation via AI provider)
│       └── ingestion.rs            (document chunking + indexing pipeline)
├── processing/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (re-exports)
│       ├── pipeline.rs             (RecordingPipeline — step-by-step processing)
│       ├── batch.rs                (BatchProcessor — queue + concurrent workers)
│       ├── soap_generator.rs       (SOAP note generation with templates)
│       └── document_generator.rs   (referral, letter, synopsis generation)
├── export/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (re-exports)
│       ├── pdf.rs                  (PDF export via printpdf)
│       ├── docx.rs                 (DOCX export via docx-rs)
│       └── fhir.rs                 (FHIR R4 Bundle JSON)
└── translation/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                  (TranslationEngine, re-exports)
        ├── session.rs              (TranslationSession — bidirectional state)
        └── canned_responses.rs     (pre-translated medical phrases)
```

---

### Task 1: Add 5 New Crates to Workspace

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: Cargo.toml + src/lib.rs + stubs for agents, rag, processing, export, translation

- [ ] **Step 1: Add workspace members**

Add to root `Cargo.toml` `[workspace.members]`:
```toml
"crates/agents",
"crates/rag",
"crates/processing",
"crates/export",
"crates/translation",
```

- [ ] **Step 2: Create agents crate**

`crates/agents/Cargo.toml`:
```toml
[package]
name = "medical-agents"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tokio-util = { version = "0.7", features = ["rt"] }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

`crates/agents/src/lib.rs`:
```rust
pub mod orchestrator;
pub mod tools;
pub mod agents;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent execution failed: {0}")]
    Execution(String),
    #[error("Tool execution failed: {0}")]
    Tool(String),
    #[error("Max iterations exceeded ({0})")]
    MaxIterations(u32),
    #[error("Cancelled")]
    Cancelled,
    #[error("Provider error: {0}")]
    Provider(String),
}

pub type AgentResult<T> = Result<T, AgentError>;
```

Create stub files for all declared modules and submodules.

- [ ] **Step 3: Create rag crate**

`crates/rag/Cargo.toml`:
```toml
[package]
name = "medical-rag"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
medical-db = { path = "../db" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

`crates/rag/src/lib.rs`:
```rust
pub mod query_expander;
pub mod vector_store;
pub mod bm25;
pub mod graph_search;
pub mod fusion;
pub mod mmr;
pub mod embeddings;
pub mod ingestion;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RagError {
    #[error("Search failed: {0}")]
    Search(String),
    #[error("Embedding error: {0}")]
    Embedding(String),
    #[error("Ingestion error: {0}")]
    Ingestion(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("No results found")]
    NoResults,
}

pub type RagResult<T> = Result<T, RagError>;
```

Create stubs for all modules.

- [ ] **Step 4: Create processing crate**

`crates/processing/Cargo.toml`:
```toml
[package]
name = "medical-processing"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
medical-db = { path = "../db" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

`crates/processing/src/lib.rs`:
```rust
pub mod pipeline;
pub mod batch;
pub mod soap_generator;
pub mod document_generator;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("Pipeline error: {0}")]
    Pipeline(String),
    #[error("Generation error: {0}")]
    Generation(String),
    #[error("STT error: {0}")]
    Stt(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Cancelled")]
    Cancelled,
}

pub type ProcessingResult<T> = Result<T, ProcessingError>;
```

Create stubs.

- [ ] **Step 5: Create export crate**

`crates/export/Cargo.toml`:
```toml
[package]
name = "medical-export"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
printpdf = "0.7"
docx-rs = "0.4"
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

`crates/export/src/lib.rs`:
```rust
pub mod pdf;
pub mod docx;
pub mod fhir;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExportError {
    #[error("PDF export error: {0}")]
    Pdf(String),
    #[error("DOCX export error: {0}")]
    Docx(String),
    #[error("FHIR export error: {0}")]
    Fhir(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ExportResult<T> = Result<T, ExportError>;
```

Create stubs.

- [ ] **Step 6: Create translation crate**

`crates/translation/Cargo.toml`:
```toml
[package]
name = "medical-translation"
version.workspace = true
edition.workspace = true

[dependencies]
medical-core = { path = "../core" }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

`crates/translation/src/lib.rs`:
```rust
pub mod session;
pub mod canned_responses;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TranslationError {
    #[error("Translation failed: {0}")]
    Translation(String),
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
    #[error("Detection failed: {0}")]
    Detection(String),
}

pub type TranslationResult<T> = Result<T, TranslationError>;
```

Create stubs.

- [ ] **Step 7: Update src-tauri/Cargo.toml**

Add dependencies:
```toml
medical-agents = { path = "../crates/agents" }
medical-rag = { path = "../crates/rag" }
medical-processing = { path = "../crates/processing" }
medical-export = { path = "../crates/export" }
medical-translation = { path = "../crates/translation" }
```

- [ ] **Step 8: Verify build and commit**

Run: `cargo build --workspace`
Commit: `git commit -m "feat: add agents, rag, processing, export, translation crate scaffolds"`

---

### Task 2: Translation Crate

**Files:**
- Create: `crates/translation/src/session.rs`
- Create: `crates/translation/src/canned_responses.rs`

- [ ] **Step 1: Write canned responses**

Write `crates/translation/src/canned_responses.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponse {
    pub id: String,
    pub category: String,
    pub text_en: String,
    pub translations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponseSet {
    pub responses: Vec<CannedResponse>,
}

impl CannedResponseSet {
    pub fn default_medical() -> Self {
        Self {
            responses: vec![
                CannedResponse {
                    id: "greeting".into(),
                    category: "general".into(),
                    text_en: "Hello, how are you feeling today?".into(),
                    translations: HashMap::from([
                        ("es".into(), "Hola, ¿cómo se siente hoy?".into()),
                        ("fr".into(), "Bonjour, comment vous sentez-vous aujourd'hui ?".into()),
                        ("de".into(), "Hallo, wie fühlen Sie sich heute?".into()),
                        ("zh".into(), "你好，你今天感觉怎么样？".into()),
                        ("ar".into(), "مرحباً، كيف تشعر اليوم؟".into()),
                    ]),
                },
                CannedResponse {
                    id: "pain_location".into(),
                    category: "assessment".into(),
                    text_en: "Where does it hurt?".into(),
                    translations: HashMap::from([
                        ("es".into(), "¿Dónde le duele?".into()),
                        ("fr".into(), "Où avez-vous mal ?".into()),
                        ("de".into(), "Wo tut es weh?".into()),
                        ("zh".into(), "哪里疼？".into()),
                    ]),
                },
                CannedResponse {
                    id: "pain_scale".into(),
                    category: "assessment".into(),
                    text_en: "On a scale of 1 to 10, how would you rate your pain?".into(),
                    translations: HashMap::from([
                        ("es".into(), "En una escala del 1 al 10, ¿cómo calificaría su dolor?".into()),
                        ("fr".into(), "Sur une échelle de 1 à 10, comment évaluez-vous votre douleur ?".into()),
                        ("de".into(), "Auf einer Skala von 1 bis 10, wie stark sind Ihre Schmerzen?".into()),
                    ]),
                },
                CannedResponse {
                    id: "medication_allergies".into(),
                    category: "history".into(),
                    text_en: "Are you allergic to any medications?".into(),
                    translations: HashMap::from([
                        ("es".into(), "¿Es alérgico a algún medicamento?".into()),
                        ("fr".into(), "Êtes-vous allergique à des médicaments ?".into()),
                        ("de".into(), "Sind Sie gegen Medikamente allergisch?".into()),
                    ]),
                },
                CannedResponse {
                    id: "current_medications".into(),
                    category: "history".into(),
                    text_en: "What medications are you currently taking?".into(),
                    translations: HashMap::from([
                        ("es".into(), "¿Qué medicamentos está tomando actualmente?".into()),
                        ("fr".into(), "Quels médicaments prenez-vous actuellement ?".into()),
                        ("de".into(), "Welche Medikamente nehmen Sie derzeit ein?".into()),
                    ]),
                },
                CannedResponse {
                    id: "symptoms_duration".into(),
                    category: "assessment".into(),
                    text_en: "How long have you had these symptoms?".into(),
                    translations: HashMap::from([
                        ("es".into(), "¿Cuánto tiempo ha tenido estos síntomas?".into()),
                        ("fr".into(), "Depuis combien de temps avez-vous ces symptômes ?".into()),
                        ("de".into(), "Wie lange haben Sie diese Symptome schon?".into()),
                    ]),
                },
                CannedResponse {
                    id: "follow_up".into(),
                    category: "instructions".into(),
                    text_en: "Please come back if your symptoms get worse.".into(),
                    translations: HashMap::from([
                        ("es".into(), "Por favor regrese si sus síntomas empeoran.".into()),
                        ("fr".into(), "Veuillez revenir si vos symptômes s'aggravent.".into()),
                        ("de".into(), "Bitte kommen Sie wieder, wenn sich Ihre Symptome verschlechtern.".into()),
                    ]),
                },
            ],
        }
    }

    pub fn by_category(&self, category: &str) -> Vec<&CannedResponse> {
        self.responses.iter().filter(|r| r.category == category).collect()
    }

    pub fn get(&self, id: &str) -> Option<&CannedResponse> {
        self.responses.iter().find(|r| r.id == id)
    }

    pub fn get_translation(&self, id: &str, lang: &str) -> Option<&str> {
        self.get(id)?.translations.get(lang).map(|s| s.as_str())
    }

    pub fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self.responses.iter().map(|r| r.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_set_has_responses() {
        let set = CannedResponseSet::default_medical();
        assert!(set.responses.len() >= 7);
    }

    #[test]
    fn get_translation_spanish() {
        let set = CannedResponseSet::default_medical();
        let translation = set.get_translation("pain_location", "es");
        assert_eq!(translation, Some("¿Dónde le duele?"));
    }

    #[test]
    fn get_translation_missing_lang() {
        let set = CannedResponseSet::default_medical();
        assert!(set.get_translation("greeting", "xx").is_none());
    }

    #[test]
    fn by_category() {
        let set = CannedResponseSet::default_medical();
        let assessment = set.by_category("assessment");
        assert!(assessment.len() >= 2);
    }

    #[test]
    fn categories_deduped() {
        let set = CannedResponseSet::default_medical();
        let cats = set.categories();
        let mut deduped = cats.clone();
        deduped.dedup();
        assert_eq!(cats, deduped);
    }
}
```

- [ ] **Step 2: Write translation session**

Write `crates/translation/src/session.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSession {
    pub source_lang: String,
    pub target_lang: String,
    pub history: Vec<TranslationEntry>,
    pub mode: TranslationMode,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TranslationMode {
    Bidirectional,
    OneWay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationEntry {
    pub original: String,
    pub translated: String,
    pub source_lang: String,
    pub target_lang: String,
    pub timestamp: DateTime<Utc>,
    pub speaker: Speaker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    Provider,
    Patient,
}

impl TranslationSession {
    pub fn new(source_lang: String, target_lang: String, mode: TranslationMode) -> Self {
        Self {
            source_lang,
            target_lang,
            history: Vec::new(),
            mode,
            created_at: Utc::now(),
        }
    }

    pub fn add_entry(&mut self, original: String, translated: String, speaker: Speaker) {
        let (source_lang, target_lang) = match speaker {
            Speaker::Provider => (self.source_lang.clone(), self.target_lang.clone()),
            Speaker::Patient => (self.target_lang.clone(), self.source_lang.clone()),
        };
        self.history.push(TranslationEntry {
            original,
            translated,
            source_lang,
            target_lang,
            timestamp: Utc::now(),
            speaker,
        });
    }

    pub fn entry_count(&self) -> usize {
        self.history.len()
    }

    pub fn export_text(&self) -> String {
        self.history
            .iter()
            .map(|e| {
                let role = match e.speaker {
                    Speaker::Provider => "Provider",
                    Speaker::Patient => "Patient",
                };
                format!("[{}] {}: {}\n→ {}", e.timestamp.format("%H:%M"), role, e.original, e.translated)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session() {
        let session = TranslationSession::new("en".into(), "es".into(), TranslationMode::Bidirectional);
        assert_eq!(session.source_lang, "en");
        assert_eq!(session.target_lang, "es");
        assert!(session.history.is_empty());
    }

    #[test]
    fn add_entries() {
        let mut session = TranslationSession::new("en".into(), "es".into(), TranslationMode::Bidirectional);
        session.add_entry("Where does it hurt?".into(), "¿Dónde le duele?".into(), Speaker::Provider);
        session.add_entry("Mi cabeza".into(), "My head".into(), Speaker::Patient);
        assert_eq!(session.entry_count(), 2);
        assert_eq!(session.history[0].source_lang, "en");
        assert_eq!(session.history[1].source_lang, "es"); // patient speaks target lang
    }

    #[test]
    fn export_text() {
        let mut session = TranslationSession::new("en".into(), "es".into(), TranslationMode::Bidirectional);
        session.add_entry("Hello".into(), "Hola".into(), Speaker::Provider);
        let export = session.export_text();
        assert!(export.contains("Provider: Hello"));
        assert!(export.contains("→ Hola"));
    }

    #[test]
    fn session_serializes() {
        let session = TranslationSession::new("en".into(), "es".into(), TranslationMode::OneWay);
        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["mode"], "one_way");
    }
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p medical-translation`
Commit: `git commit -m "feat(translation): add translation session and canned medical responses"`

---

### Task 3: Export — FHIR R4 Bundle

**Files:**
- Create: `crates/export/src/fhir.rs`

- [ ] **Step 1: Write FHIR exporter with tests**

Write `crates/export/src/fhir.rs`:
```rust
use chrono::Utc;
use medical_core::types::recording::Recording;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::ExportResult;

/// FHIR R4 Bundle resource types used in clinical documentation.
#[derive(Debug, Serialize, Deserialize)]
pub struct FhirBundle {
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    pub id: String,
    #[serde(rename = "type")]
    pub bundle_type: String,
    pub timestamp: String,
    pub entry: Vec<BundleEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleEntry {
    pub resource: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientInfo {
    pub name: Option<String>,
    pub birth_date: Option<String>,
    pub gender: Option<String>,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PractitionerInfo {
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub specialty: Option<String>,
}

pub struct FhirExporter;

impl FhirExporter {
    pub fn export_bundle(
        recording: &Recording,
        patient: &PatientInfo,
        practitioner: &PractitionerInfo,
    ) -> ExportResult<Vec<u8>> {
        let bundle = Self::build_bundle(recording, patient, practitioner)?;
        let json = serde_json::to_string_pretty(&bundle)
            .map_err(|e| crate::ExportError::Fhir(e.to_string()))?;
        Ok(json.into_bytes())
    }

    pub fn export_document_reference(
        recording: &Recording,
        title: &str,
    ) -> ExportResult<Vec<u8>> {
        let doc_ref = Self::build_document_reference(recording, title);
        let json = serde_json::to_string_pretty(&doc_ref)
            .map_err(|e| crate::ExportError::Fhir(e.to_string()))?;
        Ok(json.into_bytes())
    }

    fn build_bundle(
        recording: &Recording,
        patient: &PatientInfo,
        practitioner: &PractitionerInfo,
    ) -> ExportResult<FhirBundle> {
        let bundle_id = Uuid::new_v4().to_string();
        let patient_id = Uuid::new_v4().to_string();
        let practitioner_id = Uuid::new_v4().to_string();
        let encounter_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        let mut entries = Vec::new();

        // Patient resource
        entries.push(BundleEntry {
            resource: serde_json::json!({
                "resourceType": "Patient",
                "id": patient_id,
                "name": [{"text": patient.name.as_deref().unwrap_or("Unknown")}],
                "gender": patient.gender.as_deref().unwrap_or("unknown"),
                "birthDate": patient.birth_date,
                "identifier": patient.identifier.as_ref().map(|id| vec![
                    serde_json::json!({"value": id})
                ]),
            }),
        });

        // Practitioner resource
        entries.push(BundleEntry {
            resource: serde_json::json!({
                "resourceType": "Practitioner",
                "id": practitioner_id,
                "name": [{"text": practitioner.name.as_deref().unwrap_or("Unknown")}],
                "identifier": practitioner.identifier.as_ref().map(|id| vec![
                    serde_json::json!({"value": id})
                ]),
            }),
        });

        // Encounter resource
        entries.push(BundleEntry {
            resource: serde_json::json!({
                "resourceType": "Encounter",
                "id": encounter_id,
                "status": "finished",
                "class": {"code": "AMB", "display": "ambulatory"},
                "subject": {"reference": format!("Patient/{patient_id}")},
                "participant": [{"individual": {"reference": format!("Practitioner/{practitioner_id}")}}],
                "period": {"start": recording.created_at.to_rfc3339()},
            }),
        });

        // DocumentReference for SOAP note
        if let Some(soap) = &recording.soap_note {
            entries.push(BundleEntry {
                resource: serde_json::json!({
                    "resourceType": "DocumentReference",
                    "id": Uuid::new_v4().to_string(),
                    "status": "current",
                    "type": {"coding": [{"system": "http://loinc.org", "code": "11506-3", "display": "Progress note"}]},
                    "subject": {"reference": format!("Patient/{patient_id}")},
                    "date": now,
                    "content": [{"attachment": {"contentType": "text/plain", "data": base64_encode(soap)}}],
                }),
            });
        }

        // DocumentReference for transcript
        if let Some(transcript) = &recording.transcript {
            entries.push(BundleEntry {
                resource: serde_json::json!({
                    "resourceType": "DocumentReference",
                    "id": Uuid::new_v4().to_string(),
                    "status": "current",
                    "type": {"coding": [{"system": "http://loinc.org", "code": "11488-4", "display": "Consultation note"}]},
                    "subject": {"reference": format!("Patient/{patient_id}")},
                    "date": now,
                    "content": [{"attachment": {"contentType": "text/plain", "data": base64_encode(transcript)}}],
                }),
            });
        }

        Ok(FhirBundle {
            resource_type: "Bundle".into(),
            id: bundle_id,
            bundle_type: "document".into(),
            timestamp: now,
            entry: entries,
        })
    }

    fn build_document_reference(recording: &Recording, title: &str) -> serde_json::Value {
        let content = recording.soap_note.as_deref()
            .or(recording.transcript.as_deref())
            .unwrap_or("");

        serde_json::json!({
            "resourceType": "DocumentReference",
            "id": Uuid::new_v4().to_string(),
            "status": "current",
            "description": title,
            "date": Utc::now().to_rfc3339(),
            "content": [{"attachment": {"contentType": "text/plain", "data": base64_encode(content)}}],
        })
    }
}

fn base64_encode(text: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::recording::{Recording, ProcessingStatus};
    use std::path::PathBuf;

    fn sample_recording() -> Recording {
        let mut rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        rec.patient_name = Some("John Smith".into());
        rec.soap_note = Some("S: Patient presents with headache.\nO: BP 120/80.\nA: Tension headache.\nP: Ibuprofen 400mg PRN.".into());
        rec.transcript = Some("Doctor: How are you feeling? Patient: I have a headache.".into());
        rec
    }

    fn sample_patient() -> PatientInfo {
        PatientInfo {
            name: Some("John Smith".into()),
            birth_date: Some("1985-03-15".into()),
            gender: Some("male".into()),
            identifier: Some("MRN-12345".into()),
        }
    }

    fn sample_practitioner() -> PractitionerInfo {
        PractitionerInfo {
            name: Some("Dr. Jane Doe".into()),
            identifier: Some("NPI-9876543".into()),
            specialty: Some("Family Medicine".into()),
        }
    }

    #[test]
    fn export_bundle_produces_valid_json() {
        let rec = sample_recording();
        let bytes = FhirExporter::export_bundle(&rec, &sample_patient(), &sample_practitioner()).unwrap();
        let json: FhirBundle = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.resource_type, "Bundle");
        assert_eq!(json.bundle_type, "document");
        assert!(json.entry.len() >= 4); // patient + practitioner + encounter + soap doc
    }

    #[test]
    fn bundle_contains_patient_resource() {
        let rec = sample_recording();
        let bytes = FhirExporter::export_bundle(&rec, &sample_patient(), &sample_practitioner()).unwrap();
        let json: FhirBundle = serde_json::from_slice(&bytes).unwrap();
        let patient_entry = json.entry.iter().find(|e| e.resource["resourceType"] == "Patient");
        assert!(patient_entry.is_some());
    }

    #[test]
    fn bundle_contains_soap_document_reference() {
        let rec = sample_recording();
        let bytes = FhirExporter::export_bundle(&rec, &sample_patient(), &sample_practitioner()).unwrap();
        let json: FhirBundle = serde_json::from_slice(&bytes).unwrap();
        let doc_refs: Vec<_> = json.entry.iter()
            .filter(|e| e.resource["resourceType"] == "DocumentReference")
            .collect();
        assert!(doc_refs.len() >= 1);
    }

    #[test]
    fn export_document_reference() {
        let rec = sample_recording();
        let bytes = FhirExporter::export_document_reference(&rec, "SOAP Note").unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["resourceType"], "DocumentReference");
        assert_eq!(json["description"], "SOAP Note");
    }

    #[test]
    fn recording_without_soap_still_exports() {
        let rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        let bytes = FhirExporter::export_bundle(&rec, &sample_patient(), &sample_practitioner()).unwrap();
        let json: FhirBundle = serde_json::from_slice(&bytes).unwrap();
        assert!(json.entry.len() >= 3); // patient + practitioner + encounter (no doc refs)
    }
}
```

Add `base64 = "0.22"` to `crates/export/Cargo.toml` dependencies.

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p medical-export`
Commit: `git commit -m "feat(export): add FHIR R4 Bundle exporter with Patient, Practitioner, Encounter, DocumentReference"`

---

### Task 4: Export — PDF and DOCX (stubs with structure)

**Files:**
- Create: `crates/export/src/pdf.rs`
- Create: `crates/export/src/docx.rs`

- [ ] **Step 1: Write PDF exporter**

Write `crates/export/src/pdf.rs`:
```rust
use medical_core::types::recording::Recording;
use crate::{ExportError, ExportResult};

pub struct PdfExporter;

impl PdfExporter {
    /// Export a recording's SOAP note as a formatted PDF.
    pub fn export_soap(recording: &Recording) -> ExportResult<Vec<u8>> {
        let soap = recording.soap_note.as_deref()
            .ok_or_else(|| ExportError::Pdf("No SOAP note to export".into()))?;

        let title = format!(
            "SOAP Note — {}",
            recording.patient_name.as_deref().unwrap_or("Unknown Patient")
        );

        Self::render_document(&title, soap, &recording.created_at.format("%Y-%m-%d").to_string())
    }

    /// Export a referral letter as PDF.
    pub fn export_referral(recording: &Recording) -> ExportResult<Vec<u8>> {
        let referral = recording.referral.as_deref()
            .ok_or_else(|| ExportError::Pdf("No referral to export".into()))?;
        Self::render_document("Referral Letter", referral, &recording.created_at.format("%Y-%m-%d").to_string())
    }

    /// Export a patient letter as PDF.
    pub fn export_letter(recording: &Recording) -> ExportResult<Vec<u8>> {
        let letter = recording.letter.as_deref()
            .ok_or_else(|| ExportError::Pdf("No letter to export".into()))?;
        Self::render_document("Patient Correspondence", letter, &recording.created_at.format("%Y-%m-%d").to_string())
    }

    fn render_document(title: &str, body: &str, date: &str) -> ExportResult<Vec<u8>> {
        use printpdf::*;

        let (doc, page1, layer1) = PdfDocument::new(title, Mm(210.0), Mm(297.0), "Layer 1");
        let current_layer = doc.get_page(page1).get_layer(layer1);

        let font = doc.add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| ExportError::Pdf(e.to_string()))?;
        let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| ExportError::Pdf(e.to_string()))?;

        // Title
        current_layer.use_text(title, 16.0, Mm(20.0), Mm(277.0), &font_bold);

        // Date
        current_layer.use_text(date, 10.0, Mm(20.0), Mm(268.0), &font);

        // Body — simple line-by-line rendering
        let mut y = 255.0;
        for line in body.lines() {
            if y < 20.0 {
                break; // Simple overflow protection — multi-page not yet implemented
            }

            let is_section = line.starts_with("S:") || line.starts_with("O:")
                || line.starts_with("A:") || line.starts_with("P:")
                || line.starts_with("Subjective") || line.starts_with("Objective")
                || line.starts_with("Assessment") || line.starts_with("Plan");

            if is_section {
                current_layer.use_text(line, 11.0, Mm(20.0), Mm(y), &font_bold);
            } else {
                current_layer.use_text(line, 10.0, Mm(20.0), Mm(y), &font);
            }
            y -= 5.0;
        }

        let bytes = doc.save_to_bytes()
            .map_err(|e| ExportError::Pdf(e.to_string()))?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::recording::Recording;
    use std::path::PathBuf;

    #[test]
    fn export_soap_produces_pdf_bytes() {
        let mut rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        rec.soap_note = Some("S: Headache\nO: BP 120/80\nA: Tension headache\nP: Ibuprofen".into());
        rec.patient_name = Some("John Smith".into());
        let bytes = PdfExporter::export_soap(&rec).unwrap();
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..5], b"%PDF-");
    }

    #[test]
    fn export_soap_without_note_errors() {
        let rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        assert!(PdfExporter::export_soap(&rec).is_err());
    }

    #[test]
    fn export_referral_without_content_errors() {
        let rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        assert!(PdfExporter::export_referral(&rec).is_err());
    }
}
```

- [ ] **Step 2: Write DOCX exporter**

Write `crates/export/src/docx.rs`:
```rust
use medical_core::types::recording::Recording;
use crate::{ExportError, ExportResult};

pub struct DocxExporter;

impl DocxExporter {
    /// Export a recording's SOAP note as a DOCX file.
    pub fn export_soap(recording: &Recording) -> ExportResult<Vec<u8>> {
        let soap = recording.soap_note.as_deref()
            .ok_or_else(|| ExportError::Docx("No SOAP note to export".into()))?;
        let title = format!(
            "SOAP Note — {}",
            recording.patient_name.as_deref().unwrap_or("Unknown Patient")
        );
        Self::render_document(&title, soap, &recording.created_at.format("%Y-%m-%d").to_string())
    }

    /// Export a referral letter as DOCX.
    pub fn export_referral(recording: &Recording) -> ExportResult<Vec<u8>> {
        let referral = recording.referral.as_deref()
            .ok_or_else(|| ExportError::Docx("No referral to export".into()))?;
        Self::render_document("Referral Letter", referral, &recording.created_at.format("%Y-%m-%d").to_string())
    }

    /// Export a patient letter as DOCX.
    pub fn export_letter(recording: &Recording) -> ExportResult<Vec<u8>> {
        let letter = recording.letter.as_deref()
            .ok_or_else(|| ExportError::Docx("No letter to export".into()))?;
        Self::render_document("Patient Correspondence", letter, &recording.created_at.format("%Y-%m-%d").to_string())
    }

    fn render_document(title: &str, body: &str, date: &str) -> ExportResult<Vec<u8>> {
        use docx_rs::*;

        let mut docx = Docx::new();

        // Title paragraph
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text(title).bold().size(32))
                .align(AlignmentType::Center)
        );

        // Date paragraph
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text(date).size(20).color("666666"))
                .align(AlignmentType::Right)
        );

        // Blank line
        docx = docx.add_paragraph(Paragraph::new());

        // Body — detect SOAP sections
        for line in body.lines() {
            let is_section = line.starts_with("S:") || line.starts_with("O:")
                || line.starts_with("A:") || line.starts_with("P:")
                || line.starts_with("Subjective") || line.starts_with("Objective")
                || line.starts_with("Assessment") || line.starts_with("Plan");

            if line.trim().is_empty() {
                docx = docx.add_paragraph(Paragraph::new());
            } else if is_section {
                docx = docx.add_paragraph(
                    Paragraph::new()
                        .add_run(Run::new().add_text(line).bold().size(24))
                );
            } else {
                docx = docx.add_paragraph(
                    Paragraph::new()
                        .add_run(Run::new().add_text(line).size(22))
                );
            }
        }

        let mut buf = Vec::new();
        docx.build().pack(&mut std::io::Cursor::new(&mut buf))
            .map_err(|e| ExportError::Docx(e.to_string()))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::recording::Recording;
    use std::path::PathBuf;

    #[test]
    fn export_soap_produces_docx_bytes() {
        let mut rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        rec.soap_note = Some("S: Headache\nO: BP 120/80\nA: Tension headache\nP: Ibuprofen".into());
        rec.patient_name = Some("John Smith".into());
        let bytes = DocxExporter::export_soap(&rec).unwrap();
        assert!(bytes.len() > 100);
        // DOCX is a ZIP file — starts with PK magic bytes
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn export_soap_without_note_errors() {
        let rec = Recording::new("test.wav".into(), PathBuf::from("/tmp/test.wav"));
        assert!(DocxExporter::export_soap(&rec).is_err());
    }
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p medical-export`
Commit: `git commit -m "feat(export): add PDF and DOCX exporters for SOAP notes, referrals, and letters"`

---

### Task 5: RAG — Query Expander

**Files:**
- Create: `crates/rag/src/query_expander.rs`

- [ ] **Step 1: Write query expander with medical abbreviations**

Write `crates/rag/src/query_expander.rs`:
```rust
use medical_core::types::rag::ExpandedQuery;
use std::collections::HashMap;

pub struct QueryExpander {
    abbreviations: HashMap<String, Vec<String>>,
    synonyms: HashMap<String, Vec<String>>,
}

impl QueryExpander {
    pub fn new() -> Self {
        Self {
            abbreviations: Self::default_abbreviations(),
            synonyms: Self::default_synonyms(),
        }
    }

    pub fn expand(&self, query: &str) -> ExpandedQuery {
        let lower = query.to_lowercase();
        let tokens: Vec<&str> = lower.split_whitespace().collect();
        let mut expanded_terms = Vec::new();

        for token in &tokens {
            if let Some(expansions) = self.abbreviations.get(*token) {
                expanded_terms.extend(expansions.clone());
            }
            if let Some(syns) = self.synonyms.get(*token) {
                expanded_terms.extend(syns.clone());
            }
            // Also try multi-word phrases
            for phrase_len in 2..=4 {
                if tokens.len() >= phrase_len {
                    for window in tokens.windows(phrase_len) {
                        let phrase = window.join(" ");
                        if let Some(syns) = self.synonyms.get(phrase.as_str()) {
                            expanded_terms.extend(syns.clone());
                        }
                    }
                }
            }
        }

        expanded_terms.sort();
        expanded_terms.dedup();
        // Remove terms already in the original query
        expanded_terms.retain(|t| !lower.contains(t.as_str()));

        let full_query = if expanded_terms.is_empty() {
            query.to_string()
        } else {
            format!("{} {}", query, expanded_terms.join(" "))
        };

        ExpandedQuery {
            original: query.to_string(),
            expanded_terms,
            full_query,
        }
    }

    fn default_abbreviations() -> HashMap<String, Vec<String>> {
        let mut m = HashMap::new();
        // Cardiovascular
        m.insert("htn".into(), vec!["hypertension".into(), "high blood pressure".into()]);
        m.insert("chf".into(), vec!["congestive heart failure".into(), "heart failure".into()]);
        m.insert("mi".into(), vec!["myocardial infarction".into(), "heart attack".into()]);
        m.insert("afib".into(), vec!["atrial fibrillation".into()]);
        m.insert("cad".into(), vec!["coronary artery disease".into()]);
        m.insert("dvt".into(), vec!["deep vein thrombosis".into()]);
        m.insert("pe".into(), vec!["pulmonary embolism".into()]);
        // Respiratory
        m.insert("copd".into(), vec!["chronic obstructive pulmonary disease".into()]);
        m.insert("sob".into(), vec!["shortness of breath".into(), "dyspnea".into()]);
        m.insert("uri".into(), vec!["upper respiratory infection".into()]);
        // Endocrine
        m.insert("dm".into(), vec!["diabetes mellitus".into(), "diabetes".into()]);
        m.insert("t2dm".into(), vec!["type 2 diabetes mellitus".into()]);
        m.insert("t1dm".into(), vec!["type 1 diabetes mellitus".into()]);
        m.insert("tsh".into(), vec!["thyroid stimulating hormone".into()]);
        // Neurological
        m.insert("cva".into(), vec!["cerebrovascular accident".into(), "stroke".into()]);
        m.insert("tia".into(), vec!["transient ischemic attack".into()]);
        m.insert("ms".into(), vec!["multiple sclerosis".into()]);
        // GI
        m.insert("gerd".into(), vec!["gastroesophageal reflux disease".into()]);
        m.insert("ibs".into(), vec!["irritable bowel syndrome".into()]);
        // Renal
        m.insert("ckd".into(), vec!["chronic kidney disease".into()]);
        m.insert("uti".into(), vec!["urinary tract infection".into()]);
        m.insert("aki".into(), vec!["acute kidney injury".into()]);
        // General
        m.insert("bmi".into(), vec!["body mass index".into()]);
        m.insert("bp".into(), vec!["blood pressure".into()]);
        m.insert("hr".into(), vec!["heart rate".into()]);
        m.insert("rr".into(), vec!["respiratory rate".into()]);
        m.insert("wbc".into(), vec!["white blood cell".into()]);
        m.insert("rbc".into(), vec!["red blood cell".into()]);
        m.insert("hgb".into(), vec!["hemoglobin".into()]);
        m.insert("plt".into(), vec!["platelet".into()]);
        m.insert("bun".into(), vec!["blood urea nitrogen".into()]);
        m.insert("cr".into(), vec!["creatinine".into()]);
        m.insert("inr".into(), vec!["international normalized ratio".into()]);
        m.insert("esr".into(), vec!["erythrocyte sedimentation rate".into()]);
        m.insert("crp".into(), vec!["c-reactive protein".into()]);
        m.insert("hba1c".into(), vec!["glycated hemoglobin".into(), "hemoglobin a1c".into()]);
        m.insert("ldl".into(), vec!["low density lipoprotein".into()]);
        m.insert("hdl".into(), vec!["high density lipoprotein".into()]);
        m.insert("npo".into(), vec!["nothing by mouth".into()]);
        m.insert("prn".into(), vec!["as needed".into()]);
        m.insert("bid".into(), vec!["twice daily".into()]);
        m.insert("tid".into(), vec!["three times daily".into()]);
        m.insert("qid".into(), vec!["four times daily".into()]);
        m.insert("qd".into(), vec!["once daily".into()]);
        m
    }

    fn default_synonyms() -> HashMap<String, Vec<String>> {
        let mut m = HashMap::new();
        m.insert("heart attack".into(), vec!["myocardial infarction".into(), "mi".into()]);
        m.insert("high blood pressure".into(), vec!["hypertension".into()]);
        m.insert("high blood sugar".into(), vec!["hyperglycemia".into()]);
        m.insert("low blood sugar".into(), vec!["hypoglycemia".into()]);
        m.insert("stroke".into(), vec!["cerebrovascular accident".into(), "cva".into()]);
        m.insert("kidney failure".into(), vec!["renal failure".into()]);
        m.insert("liver disease".into(), vec!["hepatic disease".into()]);
        m.insert("headache".into(), vec!["cephalgia".into()]);
        m.insert("chest pain".into(), vec!["angina".into(), "thoracic pain".into()]);
        m.insert("belly pain".into(), vec!["abdominal pain".into()]);
        m.insert("stomach pain".into(), vec!["abdominal pain".into(), "epigastric pain".into()]);
        m.insert("blood clot".into(), vec!["thrombosis".into(), "thrombus".into()]);
        m.insert("bruise".into(), vec!["contusion".into(), "ecchymosis".into()]);
        m.insert("broken bone".into(), vec!["fracture".into()]);
        m.insert("rash".into(), vec!["dermatitis".into(), "exanthem".into()]);
        m.insert("swelling".into(), vec!["edema".into()]);
        m.insert("dizziness".into(), vec!["vertigo".into(), "lightheadedness".into()]);
        m.insert("tiredness".into(), vec!["fatigue".into(), "malaise".into()]);
        m.insert("itching".into(), vec!["pruritus".into()]);
        m.insert("runny nose".into(), vec!["rhinorrhea".into()]);
        m
    }
}

impl Default for QueryExpander {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_abbreviation() {
        let expander = QueryExpander::new();
        let result = expander.expand("htn treatment");
        assert!(result.expanded_terms.contains(&"hypertension".to_string()));
        assert!(result.full_query.contains("hypertension"));
    }

    #[test]
    fn expands_synonym_phrase() {
        let expander = QueryExpander::new();
        let result = expander.expand("heart attack symptoms");
        assert!(result.expanded_terms.contains(&"myocardial infarction".to_string()));
    }

    #[test]
    fn no_expansion_for_unknown_terms() {
        let expander = QueryExpander::new();
        let result = expander.expand("random unknown query");
        assert!(result.expanded_terms.is_empty());
        assert_eq!(result.full_query, "random unknown query");
    }

    #[test]
    fn does_not_duplicate_terms_already_in_query() {
        let expander = QueryExpander::new();
        let result = expander.expand("hypertension htn");
        // "hypertension" is already in query, shouldn't be in expanded_terms
        assert!(!result.expanded_terms.contains(&"hypertension".to_string()));
    }

    #[test]
    fn expands_multiple_abbreviations() {
        let expander = QueryExpander::new();
        let result = expander.expand("dm htn");
        assert!(result.expanded_terms.iter().any(|t| t.contains("diabetes")));
        assert!(result.expanded_terms.iter().any(|t| t.contains("hypertension")));
    }

    #[test]
    fn case_insensitive() {
        let expander = QueryExpander::new();
        let result = expander.expand("HTN");
        assert!(result.expanded_terms.contains(&"hypertension".to_string()));
    }

    #[test]
    fn expanded_terms_deduped() {
        let expander = QueryExpander::new();
        let result = expander.expand("mi mi mi");
        let count = result.expanded_terms.iter().filter(|t| t.as_str() == "myocardial infarction").count();
        assert_eq!(count, 1);
    }
}
```

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p medical-rag`
Commit: `git commit -m "feat(rag): add medical query expander with 40+ abbreviations and 20+ synonym mappings"`

---

### Task 6: RAG — Fusion and MMR Algorithms

**Files:**
- Create: `crates/rag/src/fusion.rs`
- Create: `crates/rag/src/mmr.rs`

- [ ] **Step 1: Write reciprocal rank fusion**

Write `crates/rag/src/fusion.rs`:
```rust
use medical_core::types::rag::RagResult;
use std::collections::HashMap;
use uuid::Uuid;

/// Reciprocal Rank Fusion (RRF) across multiple result sets.
/// score = sum(1 / (k + rank_in_set)) for each set containing the result.
pub fn reciprocal_rank_fusion(
    result_sets: &[Vec<RagResult>],
    k: f32,
) -> Vec<RagResult> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();
    let mut results_by_id: HashMap<Uuid, RagResult> = HashMap::new();

    for result_set in result_sets {
        for (rank, result) in result_set.iter().enumerate() {
            let rrf_score = 1.0 / (k + rank as f32 + 1.0);
            *scores.entry(result.chunk_id).or_default() += rrf_score;
            results_by_id.entry(result.chunk_id).or_insert_with(|| result.clone());
        }
    }

    let mut fused: Vec<RagResult> = results_by_id
        .into_values()
        .map(|mut r| {
            r.score = *scores.get(&r.chunk_id).unwrap_or(&0.0);
            r
        })
        .collect();

    fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

/// Weighted score fusion. Combines scores from vector, BM25, and graph search.
pub fn weighted_fusion(
    vector_results: &[RagResult],
    bm25_results: &[RagResult],
    graph_results: &[RagResult],
    vector_weight: f32,
    bm25_weight: f32,
    graph_weight: f32,
) -> Vec<RagResult> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();
    let mut results_by_id: HashMap<Uuid, RagResult> = HashMap::new();

    for r in vector_results {
        *scores.entry(r.chunk_id).or_default() += r.score * vector_weight;
        results_by_id.entry(r.chunk_id).or_insert_with(|| r.clone());
    }
    for r in bm25_results {
        *scores.entry(r.chunk_id).or_default() += r.score * bm25_weight;
        results_by_id.entry(r.chunk_id).or_insert_with(|| r.clone());
    }
    for r in graph_results {
        *scores.entry(r.chunk_id).or_default() += r.score * graph_weight;
        results_by_id.entry(r.chunk_id).or_insert_with(|| r.clone());
    }

    let mut fused: Vec<RagResult> = results_by_id
        .into_values()
        .map(|mut r| {
            r.score = *scores.get(&r.chunk_id).unwrap_or(&0.0);
            r
        })
        .collect();

    fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::rag::*;

    fn make_result(id: u128, score: f32, source: SearchSource) -> RagResult {
        RagResult {
            chunk_id: Uuid::from_u128(id),
            document_id: Uuid::from_u128(0),
            content: format!("chunk_{id}"),
            score,
            source,
            metadata: RagChunkMetadata {
                document_title: None,
                chunk_index: 0,
                total_chunks: 1,
                page_number: None,
            },
        }
    }

    #[test]
    fn rrf_combines_two_sets() {
        let set1 = vec![make_result(1, 0.9, SearchSource::Vector), make_result(2, 0.8, SearchSource::Vector)];
        let set2 = vec![make_result(2, 0.7, SearchSource::Bm25), make_result(3, 0.6, SearchSource::Bm25)];
        let fused = reciprocal_rank_fusion(&[set1, set2], 60.0);
        // Result 2 appears in both sets, should have highest RRF score
        assert_eq!(fused[0].chunk_id, Uuid::from_u128(2));
    }

    #[test]
    fn rrf_empty_input() {
        let fused = reciprocal_rank_fusion(&[], 60.0);
        assert!(fused.is_empty());
    }

    #[test]
    fn weighted_fusion_respects_weights() {
        let vec_results = vec![make_result(1, 1.0, SearchSource::Vector)];
        let bm25_results = vec![make_result(2, 1.0, SearchSource::Bm25)];
        let graph_results = vec![];

        let fused = weighted_fusion(&vec_results, &bm25_results, &graph_results, 0.5, 0.3, 0.2);
        // Result 1 has score 0.5, result 2 has score 0.3
        assert_eq!(fused[0].chunk_id, Uuid::from_u128(1));
        assert!((fused[0].score - 0.5).abs() < 0.001);
    }

    #[test]
    fn weighted_fusion_combines_overlapping() {
        let vec_results = vec![make_result(1, 0.8, SearchSource::Vector)];
        let bm25_results = vec![make_result(1, 0.9, SearchSource::Bm25)];
        let fused = weighted_fusion(&vec_results, &bm25_results, &[], 0.5, 0.3, 0.2);
        // 0.8*0.5 + 0.9*0.3 = 0.67
        assert!((fused[0].score - 0.67).abs() < 0.01);
    }
}
```

- [ ] **Step 2: Write MMR reranker**

Write `crates/rag/src/mmr.rs`:
```rust
use medical_core::types::rag::RagResult;

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Jaccard similarity between two strings (word-level).
pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let set_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let set_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    let intersection = set_a.intersection(&set_b).count() as f32;
    let union = set_a.union(&set_b).count() as f32;
    if union == 0.0 { 0.0 } else { intersection / union }
}

/// Maximal Marginal Relevance reranking.
/// Selects top_k results that balance relevance and diversity.
///
/// MMR = λ * relevance - (1-λ) * max_similarity_to_selected
///
/// Uses Jaccard text similarity as fallback when embeddings aren't available.
pub fn mmr_rerank(
    results: &[RagResult],
    lambda: f32,
    top_k: usize,
) -> Vec<RagResult> {
    if results.is_empty() || top_k == 0 {
        return Vec::new();
    }

    let mut selected: Vec<usize> = Vec::new();
    let mut candidates: Vec<usize> = (0..results.len()).collect();

    while selected.len() < top_k && !candidates.is_empty() {
        let mut best_score = f32::NEG_INFINITY;
        let mut best_idx = 0;

        for (pos, &cand_idx) in candidates.iter().enumerate() {
            let relevance = results[cand_idx].score;

            let max_sim = if selected.is_empty() {
                0.0
            } else {
                selected
                    .iter()
                    .map(|&sel_idx| {
                        jaccard_similarity(&results[cand_idx].content, &results[sel_idx].content)
                    })
                    .fold(0.0f32, f32::max)
            };

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;

            if mmr > best_score {
                best_score = mmr;
                best_idx = pos;
            }
        }

        let chosen = candidates.remove(best_idx);
        selected.push(chosen);
    }

    selected
        .into_iter()
        .map(|idx| {
            let mut r = results[idx].clone();
            r.source = medical_core::types::rag::SearchSource::Fused;
            r
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::rag::*;
    use uuid::Uuid;

    #[test]
    fn cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn jaccard_similarity_identical() {
        assert!((jaccard_similarity("hello world", "hello world") - 1.0).abs() < 0.001);
    }

    #[test]
    fn jaccard_similarity_partial() {
        let sim = jaccard_similarity("hello world foo", "hello world bar");
        assert!(sim > 0.3 && sim < 0.8);
    }

    #[test]
    fn jaccard_similarity_disjoint() {
        assert_eq!(jaccard_similarity("hello", "world"), 0.0);
    }

    fn make_result(id: u128, score: f32, content: &str) -> RagResult {
        RagResult {
            chunk_id: Uuid::from_u128(id),
            document_id: Uuid::from_u128(0),
            content: content.into(),
            score,
            source: SearchSource::Fused,
            metadata: RagChunkMetadata { document_title: None, chunk_index: 0, total_chunks: 1, page_number: None },
        }
    }

    #[test]
    fn mmr_selects_top_k() {
        let results = vec![
            make_result(1, 0.9, "patient presents with headache and nausea"),
            make_result(2, 0.85, "patient presents with headache and vomiting"),
            make_result(3, 0.7, "diabetes management guidelines for type 2"),
        ];
        let reranked = mmr_rerank(&results, 0.7, 2);
        assert_eq!(reranked.len(), 2);
    }

    #[test]
    fn mmr_favors_diversity() {
        let results = vec![
            make_result(1, 0.9, "hypertension treatment guidelines medication"),
            make_result(2, 0.88, "hypertension treatment guidelines dosage"), // very similar to 1
            make_result(3, 0.7, "diabetes management insulin therapy"), // diverse
        ];
        let reranked = mmr_rerank(&results, 0.5, 2);
        // With lambda=0.5, diversity matters as much as relevance
        // First should be highest relevance (1), second should prefer diversity (3 over 2)
        assert_eq!(reranked[0].chunk_id, Uuid::from_u128(1));
        assert_eq!(reranked[1].chunk_id, Uuid::from_u128(3));
    }

    #[test]
    fn mmr_empty_input() {
        let reranked = mmr_rerank(&[], 0.7, 5);
        assert!(reranked.is_empty());
    }

    #[test]
    fn mmr_top_k_larger_than_results() {
        let results = vec![make_result(1, 0.9, "only one result")];
        let reranked = mmr_rerank(&results, 0.7, 10);
        assert_eq!(reranked.len(), 1);
    }
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p medical-rag`
Commit: `git commit -m "feat(rag): add reciprocal rank fusion, weighted fusion, and MMR reranker"`

---

### Task 7: RAG — Stub Modules (Vector Store, BM25, Graph, Embeddings, Ingestion)

**Files:**
- Create: `crates/rag/src/vector_store.rs`
- Create: `crates/rag/src/bm25.rs`
- Create: `crates/rag/src/graph_search.rs`
- Create: `crates/rag/src/embeddings.rs`
- Create: `crates/rag/src/ingestion.rs`

These modules require external service integration (sqlite-vec, CozoDB, embedding API). Provide compilable stubs with clear interfaces that will be filled in when the db crate's vector and graph modules are implemented.

- [ ] **Step 1: Write stubs with interfaces**

Write `crates/rag/src/vector_store.rs`:
```rust
//! Vector store using SQLite + sqlite-vec for embedding storage and cosine search.
//! Full implementation requires sqlite-vec extension setup.

use medical_core::types::rag::{DocumentChunk, RagResult};
use crate::RagResult as Result;
use uuid::Uuid;

pub struct VectorStore;

impl VectorStore {
    pub fn new() -> Self { Self }

    /// Store a document chunk with its embedding.
    pub fn store_chunk(&self, _chunk: &DocumentChunk) -> Result<()> {
        Ok(()) // Stub
    }

    /// Search for similar chunks by embedding vector.
    pub fn search(&self, _embedding: &[f32], _top_k: usize, _threshold: f32) -> Result<Vec<RagResult>> {
        Ok(Vec::new()) // Stub — returns empty until sqlite-vec is wired
    }

    /// Delete all chunks for a document.
    pub fn delete_document(&self, _document_id: &Uuid) -> Result<()> {
        Ok(())
    }
}
```

Write `crates/rag/src/bm25.rs`:
```rust
//! BM25 keyword search backed by SQLite FTS5.
//! Leverages the FTS5 tables created in the db crate's migration.

use medical_core::types::rag::{RagResult, RagChunkMetadata, SearchSource};
use crate::RagResult as Result;
use uuid::Uuid;

pub struct Bm25Search;

impl Bm25Search {
    pub fn new() -> Self { Self }

    /// Search document chunks using BM25 keyword matching.
    pub fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<RagResult>> {
        Ok(Vec::new()) // Stub — returns empty until connected to db crate FTS5
    }
}
```

Write `crates/rag/src/graph_search.rs`:
```rust
//! Knowledge graph search via CozoDB.
//! Extracts entities from queries and traverses clinical relationships.

use medical_core::types::rag::{RagResult, GraphEntity, GraphRelation};
use crate::RagResult as Result;

pub struct GraphSearch;

impl GraphSearch {
    pub fn new() -> Self { Self }

    /// Search the knowledge graph for entities related to the query.
    pub fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<RagResult>> {
        Ok(Vec::new()) // Stub — returns empty until CozoDB is wired
    }

    /// Store a clinical entity in the graph.
    pub fn store_entity(&self, _entity: &GraphEntity) -> Result<()> {
        Ok(())
    }

    /// Store a relation between entities.
    pub fn store_relation(&self, _relation: &GraphRelation) -> Result<()> {
        Ok(())
    }
}
```

Write `crates/rag/src/embeddings.rs`:
```rust
//! Embedding generation via AI provider API.
//! Uses the configured embedding model (default: text-embedding-3-small).

use medical_core::error::AppResult;

pub struct EmbeddingGenerator;

impl EmbeddingGenerator {
    pub fn new() -> Self { Self }

    /// Generate embeddings for a text chunk.
    /// Returns a vector of f32 values (dimension depends on model).
    pub async fn embed(&self, _text: &str) -> AppResult<Vec<f32>> {
        // Stub — returns zero vector until connected to embedding API
        Ok(vec![0.0; 1536])
    }

    /// Generate embeddings for multiple texts in a batch.
    pub async fn embed_batch(&self, _texts: &[String]) -> AppResult<Vec<Vec<f32>>> {
        Ok(_texts.iter().map(|_| vec![0.0; 1536]).collect())
    }
}
```

Write `crates/rag/src/ingestion.rs`:
```rust
//! Document ingestion pipeline: PDF/text → chunk → embed → store.

use medical_core::error::AppResult;
use uuid::Uuid;

pub struct IngestionPipeline;

impl IngestionPipeline {
    pub fn new() -> Self { Self }

    /// Ingest a text document: chunk, embed, store in vector store + graph.
    pub async fn ingest_text(&self, _document_id: &Uuid, _title: &str, _text: &str) -> AppResult<u32> {
        Ok(0) // Stub — returns 0 chunks ingested
    }

    /// Delete all indexed data for a document.
    pub async fn delete_document(&self, _document_id: &Uuid) -> AppResult<()> {
        Ok(())
    }
}

/// Split text into overlapping chunks respecting sentence boundaries.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() || chunk_size == 0 {
        return Vec::new();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= chunk_size {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < words.len() {
        let end = (start + chunk_size).min(words.len());
        let chunk = words[start..end].join(" ");
        chunks.push(chunk);

        if end >= words.len() {
            break;
        }
        start += chunk_size - overlap;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_basic() {
        let text = "one two three four five six seven eight nine ten";
        let chunks = chunk_text(text, 5, 2);
        assert_eq!(chunks.len(), 3); // [1-5], [4-8], [7-10]
        assert!(chunks[0].starts_with("one"));
        assert!(chunks[1].starts_with("four"));
    }

    #[test]
    fn chunk_text_short() {
        let text = "short text";
        let chunks = chunk_text(text, 100, 10);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn chunk_text_empty() {
        assert!(chunk_text("", 100, 10).is_empty());
    }

    #[test]
    fn chunk_text_zero_size() {
        assert!(chunk_text("hello", 0, 0).is_empty());
    }
}
```

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p medical-rag`
Commit: `git commit -m "feat(rag): add vector store, BM25, graph search, embeddings, and ingestion stubs with chunking"`

---

### Task 8: Agents — Tool System

**Files:**
- Create: `crates/agents/src/tools/mod.rs`
- Create: `crates/agents/src/tools/icd_lookup.rs`
- Create: `crates/agents/src/tools/drug_interaction.rs`
- Create: `crates/agents/src/tools/vitals_extractor.rs`
- Create: `crates/agents/src/tools/rag_search.rs`
- Create: `crates/agents/src/tools/checklist.rs`

- [ ] **Step 1: Write tool registry and tool implementations**

Write `crates/agents/src/tools/mod.rs`:
```rust
pub mod icd_lookup;
pub mod drug_interaction;
pub mod vitals_extractor;
pub mod rag_search;
pub mod checklist;

use medical_core::types::agent::ToolDef;
use std::collections::HashMap;
use std::sync::Arc;
use medical_core::traits::Tool;

/// Registry of available tools, keyed by name.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.definition().name;
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list_definitions(&self) -> Vec<ToolDef> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(icd_lookup::IcdLookupTool));
        registry.register(Arc::new(drug_interaction::DrugInteractionTool));
        registry.register(Arc::new(vitals_extractor::VitalsExtractorTool));
        registry.register(Arc::new(checklist::ChecklistTool));
        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
```

Write `crates/agents/src/tools/icd_lookup.rs`:
```rust
use async_trait::async_trait;
use medical_core::error::AppResult;
use medical_core::traits::Tool;
use medical_core::types::agent::{ToolDef, ToolOutput};

pub struct IcdLookupTool;

#[async_trait]
impl Tool for IcdLookupTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "search_icd_codes".into(),
            description: "Search for ICD-9 or ICD-10 codes by diagnosis description. Returns matching codes with descriptions.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Diagnosis or condition to search for"},
                    "version": {"type": "string", "enum": ["ICD-9", "ICD-10", "both"], "description": "ICD version to search"}
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> AppResult<ToolOutput> {
        let query = params["query"].as_str().unwrap_or("").to_lowercase();
        let _version = params["version"].as_str().unwrap_or("ICD-10");

        // Simplified ICD lookup — in production, this would query a database
        let results = lookup_icd(&query);
        Ok(ToolOutput::success(serde_json::to_string_pretty(&results).unwrap_or_default()))
    }
}

fn lookup_icd(query: &str) -> Vec<serde_json::Value> {
    // Common ICD-10 codes for demonstration
    let codes = vec![
        ("I10", "Essential (primary) hypertension", vec!["hypertension", "htn", "high blood pressure"]),
        ("E11", "Type 2 diabetes mellitus", vec!["diabetes", "dm", "type 2"]),
        ("J06.9", "Acute upper respiratory infection, unspecified", vec!["uri", "cold", "upper respiratory"]),
        ("M54.5", "Low back pain", vec!["back pain", "lumbago"]),
        ("R51.9", "Headache, unspecified", vec!["headache", "cephalgia"]),
        ("J45", "Asthma", vec!["asthma", "reactive airway"]),
        ("K21.0", "GERD with esophagitis", vec!["gerd", "reflux", "heartburn"]),
        ("F41.1", "Generalized anxiety disorder", vec!["anxiety", "gad"]),
        ("G43.909", "Migraine, unspecified", vec!["migraine"]),
        ("N39.0", "Urinary tract infection", vec!["uti", "urinary infection"]),
    ];

    codes.iter()
        .filter(|(_, _, keywords)| keywords.iter().any(|k| query.contains(k)))
        .map(|(code, desc, _)| serde_json::json!({"code": code, "description": desc}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lookup_hypertension() {
        let tool = IcdLookupTool;
        let result = tool.execute(serde_json::json!({"query": "hypertension"})).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("I10"));
    }

    #[tokio::test]
    async fn lookup_unknown() {
        let tool = IcdLookupTool;
        let result = tool.execute(serde_json::json!({"query": "xyzzy"})).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("[]"));
    }

    #[test]
    fn tool_definition() {
        let tool = IcdLookupTool;
        let def = tool.definition();
        assert_eq!(def.name, "search_icd_codes");
    }
}
```

Write `crates/agents/src/tools/drug_interaction.rs`:
```rust
use async_trait::async_trait;
use medical_core::error::AppResult;
use medical_core::traits::Tool;
use medical_core::types::agent::{ToolDef, ToolOutput};

pub struct DrugInteractionTool;

#[async_trait]
impl Tool for DrugInteractionTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lookup_drug_interactions".into(),
            description: "Check for drug-drug interactions between two or more medications. Returns severity and clinical significance.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "medications": {"type": "array", "items": {"type": "string"}, "description": "List of medication names to check"}
                },
                "required": ["medications"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> AppResult<ToolOutput> {
        let meds: Vec<String> = params["medications"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        if meds.len() < 2 {
            return Ok(ToolOutput::success("Need at least 2 medications to check interactions."));
        }

        let interactions = check_interactions(&meds);
        Ok(ToolOutput::success(serde_json::to_string_pretty(&interactions).unwrap_or_default()))
    }
}

fn check_interactions(meds: &[String]) -> Vec<serde_json::Value> {
    let lower: Vec<String> = meds.iter().map(|m| m.to_lowercase()).collect();
    let mut interactions = Vec::new();

    // Common drug interactions for demonstration
    let known = vec![
        ("warfarin", "aspirin", "Major", "Increased bleeding risk"),
        ("metformin", "contrast dye", "Major", "Risk of lactic acidosis"),
        ("ssri", "maoi", "Contraindicated", "Risk of serotonin syndrome"),
        ("ace inhibitor", "potassium", "Moderate", "Risk of hyperkalemia"),
        ("statin", "grapefruit", "Moderate", "Increased statin levels"),
        ("methotrexate", "nsaid", "Major", "Increased methotrexate toxicity"),
        ("lithium", "nsaid", "Major", "Increased lithium levels"),
        ("warfarin", "nsaid", "Major", "Increased bleeding risk"),
    ];

    for (drug_a, drug_b, severity, description) in known {
        let has_a = lower.iter().any(|m| m.contains(drug_a));
        let has_b = lower.iter().any(|m| m.contains(drug_b));
        if has_a && has_b {
            interactions.push(serde_json::json!({
                "drug_a": drug_a, "drug_b": drug_b,
                "severity": severity, "description": description,
            }));
        }
    }

    interactions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detects_warfarin_aspirin_interaction() {
        let tool = DrugInteractionTool;
        let result = tool.execute(serde_json::json!({
            "medications": ["Warfarin", "Aspirin"]
        })).await.unwrap();
        assert!(result.content.contains("bleeding"));
    }

    #[tokio::test]
    async fn no_interaction_for_safe_combo() {
        let tool = DrugInteractionTool;
        let result = tool.execute(serde_json::json!({
            "medications": ["Ibuprofen", "Acetaminophen"]
        })).await.unwrap();
        assert!(result.content.contains("[]"));
    }
}
```

Write `crates/agents/src/tools/vitals_extractor.rs`:
```rust
use async_trait::async_trait;
use medical_core::error::AppResult;
use medical_core::traits::Tool;
use medical_core::types::agent::{ToolDef, ToolOutput};
use regex::Regex;

pub struct VitalsExtractorTool;

#[async_trait]
impl Tool for VitalsExtractorTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "extract_vitals".into(),
            description: "Extract vital signs from clinical text. Identifies blood pressure, heart rate, temperature, respiratory rate, SpO2, weight, height, and BMI.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Clinical text to extract vitals from"}
                },
                "required": ["text"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> AppResult<ToolOutput> {
        let text = params["text"].as_str().unwrap_or("");
        let vitals = extract_vitals(text);
        Ok(ToolOutput::success(serde_json::to_string_pretty(&vitals).unwrap_or_default()))
    }
}

fn extract_vitals(text: &str) -> serde_json::Value {
    let mut vitals = serde_json::Map::new();

    // Blood pressure: 120/80, BP 120/80
    let bp_re = Regex::new(r"(?i)(?:BP|blood pressure)\s*:?\s*(\d{2,3})\s*/\s*(\d{2,3})").unwrap();
    if let Some(caps) = bp_re.captures(text) {
        vitals.insert("blood_pressure".into(), serde_json::json!({
            "systolic": caps[1].parse::<u32>().unwrap_or(0),
            "diastolic": caps[2].parse::<u32>().unwrap_or(0),
        }));
    }

    // Heart rate: HR 72, pulse 72, heart rate 72
    let hr_re = Regex::new(r"(?i)(?:HR|heart rate|pulse)\s*:?\s*(\d{2,3})").unwrap();
    if let Some(caps) = hr_re.captures(text) {
        vitals.insert("heart_rate".into(), serde_json::json!(caps[1].parse::<u32>().unwrap_or(0)));
    }

    // Temperature: temp 98.6, T 37.0
    let temp_re = Regex::new(r"(?i)(?:temp|temperature|T)\s*:?\s*(\d{2,3}\.?\d*)").unwrap();
    if let Some(caps) = temp_re.captures(text) {
        vitals.insert("temperature".into(), serde_json::json!(caps[1].parse::<f32>().unwrap_or(0.0)));
    }

    // Respiratory rate: RR 16
    let rr_re = Regex::new(r"(?i)(?:RR|respiratory rate|resp rate)\s*:?\s*(\d{1,2})").unwrap();
    if let Some(caps) = rr_re.captures(text) {
        vitals.insert("respiratory_rate".into(), serde_json::json!(caps[1].parse::<u32>().unwrap_or(0)));
    }

    // SpO2: O2 sat 98%, SpO2 98
    let spo2_re = Regex::new(r"(?i)(?:SpO2|O2 sat|oxygen sat|O2)\s*:?\s*(\d{2,3})").unwrap();
    if let Some(caps) = spo2_re.captures(text) {
        vitals.insert("spo2".into(), serde_json::json!(caps[1].parse::<u32>().unwrap_or(0)));
    }

    serde_json::Value::Object(vitals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extracts_bp_and_hr() {
        let tool = VitalsExtractorTool;
        let result = tool.execute(serde_json::json!({
            "text": "Vitals: BP 120/80, HR 72, Temp 98.6F, RR 16, SpO2 98%"
        })).await.unwrap();
        let vitals: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(vitals["blood_pressure"]["systolic"], 120);
        assert_eq!(vitals["heart_rate"], 72);
    }

    #[tokio::test]
    async fn handles_missing_vitals() {
        let tool = VitalsExtractorTool;
        let result = tool.execute(serde_json::json!({
            "text": "Patient reports feeling well."
        })).await.unwrap();
        let vitals: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert!(vitals.as_object().unwrap().is_empty());
    }
}
```

Write `crates/agents/src/tools/rag_search.rs`:
```rust
//! RAG search tool — allows agents to query the knowledge base.
//! Full implementation connects to the rag crate's HybridRetriever.

use async_trait::async_trait;
use medical_core::error::AppResult;
use medical_core::traits::Tool;
use medical_core::types::agent::{ToolDef, ToolOutput};

pub struct RagSearchTool;

#[async_trait]
impl Tool for RagSearchTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "search_knowledge_base".into(),
            description: "Search the medical knowledge base for clinical guidelines, protocols, and reference material.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "top_k": {"type": "integer", "description": "Number of results to return", "default": 5}
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> AppResult<ToolOutput> {
        let _query = params["query"].as_str().unwrap_or("");
        let _top_k = params["top_k"].as_u64().unwrap_or(5);
        // Stub — returns empty results until connected to RAG crate
        Ok(ToolOutput::success("[]"))
    }
}
```

Write `crates/agents/src/tools/checklist.rs`:
```rust
use async_trait::async_trait;
use medical_core::error::AppResult;
use medical_core::traits::Tool;
use medical_core::types::agent::{ToolDef, ToolOutput};

pub struct ChecklistTool;

#[async_trait]
impl Tool for ChecklistTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "generate_checklist".into(),
            description: "Generate a clinical workflow checklist for a given procedure or condition.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "procedure": {"type": "string", "description": "Procedure or condition name"},
                    "context": {"type": "string", "description": "Additional clinical context"}
                },
                "required": ["procedure"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> AppResult<ToolOutput> {
        let procedure = params["procedure"].as_str().unwrap_or("general assessment");
        let checklist = generate_checklist(procedure);
        Ok(ToolOutput::success(checklist))
    }
}

fn generate_checklist(procedure: &str) -> String {
    // Template-based checklists for common procedures
    match procedure.to_lowercase().as_str() {
        p if p.contains("new patient") => {
            "New Patient Assessment Checklist:\n\
             □ Verify patient identity and demographics\n\
             □ Review medical history and medications\n\
             □ Document allergies\n\
             □ Record vital signs (BP, HR, Temp, RR, SpO2)\n\
             □ Perform physical examination\n\
             □ Review family and social history\n\
             □ Document chief complaint\n\
             □ Develop assessment and plan\n\
             □ Discuss follow-up schedule".to_string()
        }
        p if p.contains("follow") => {
            "Follow-Up Visit Checklist:\n\
             □ Review previous visit notes\n\
             □ Assess response to treatment\n\
             □ Record current vital signs\n\
             □ Review medication compliance\n\
             □ Address new concerns\n\
             □ Update assessment and plan\n\
             □ Schedule next follow-up".to_string()
        }
        _ => {
            format!(
                "General Clinical Checklist for {}:\n\
                 □ Identify patient and verify history\n\
                 □ Record vital signs\n\
                 □ Perform focused examination\n\
                 □ Document findings\n\
                 □ Develop plan\n\
                 □ Discuss with patient",
                procedure
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn checklist_for_new_patient() {
        let tool = ChecklistTool;
        let result = tool.execute(serde_json::json!({"procedure": "new patient assessment"})).await.unwrap();
        assert!(result.content.contains("demographics"));
        assert!(result.content.contains("vital signs"));
    }

    #[test]
    fn tool_definition() {
        let tool = ChecklistTool;
        assert_eq!(tool.definition().name, "generate_checklist");
    }
}
```

Add `regex = "1"` to `crates/agents/Cargo.toml` dependencies.

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p medical-agents`
Commit: `git commit -m "feat(agents): add tool registry with ICD lookup, drug interaction, vitals extractor, RAG search, and checklist tools"`

---

### Task 9: Agents — Orchestrator

**Files:**
- Create: `crates/agents/src/orchestrator.rs`

- [ ] **Step 1: Write agent orchestrator with tool execution loop**

Write `crates/agents/src/orchestrator.rs`:
```rust
use medical_core::error::{AppError, AppResult};
use medical_core::traits::{Agent, AiProvider, Tool};
use medical_core::types::agent::*;
use medical_core::types::ai::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use crate::tools::ToolRegistry;

const MAX_ITERATIONS: u32 = 10;

pub struct AgentOrchestrator {
    tool_registry: ToolRegistry,
}

impl AgentOrchestrator {
    pub fn new(tool_registry: ToolRegistry) -> Self {
        Self { tool_registry }
    }

    /// Execute an agent with the tool execution loop.
    /// The agent may request tool calls, which are executed and fed back
    /// until the agent produces a final text response or hits max iterations.
    pub async fn execute(
        &self,
        agent: &dyn Agent,
        context: AgentContext,
        provider: &dyn AiProvider,
        cancel: CancellationToken,
    ) -> AppResult<AgentResponse> {
        let tools = agent.available_tools();
        let tool_defs: Vec<ToolDef> = tools.iter()
            .filter(|t| self.tool_registry.get(&t.name).is_some())
            .cloned()
            .collect();

        let mut messages = context.conversation_history.clone();

        // Add system prompt
        let system_prompt = agent.system_prompt().to_string();

        // Build initial user message with context
        let mut user_content = context.user_message.clone();
        if let Some(patient_ctx) = &context.patient_context {
            if !patient_ctx.prior_soap_notes.is_empty() {
                user_content.push_str(&format!(
                    "\n\nPatient context:\n- Prior notes: {}\n- Medications: {}\n- Conditions: {}",
                    patient_ctx.prior_soap_notes.len(),
                    patient_ctx.medications.join(", "),
                    patient_ctx.conditions.join(", "),
                ));
            }
        }

        if !context.rag_context.is_empty() {
            let rag_text: String = context.rag_context.iter()
                .map(|r| format!("- {}", r.content))
                .collect::<Vec<_>>()
                .join("\n");
            user_content.push_str(&format!("\n\nRelevant knowledge:\n{rag_text}"));
        }

        messages.push(Message {
            role: Role::User,
            content: MessageContent::Text(user_content),
        });

        let mut iterations = 0u32;
        let mut tool_records = Vec::new();

        loop {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }

            if iterations >= MAX_ITERATIONS {
                warn!(agent = agent.name(), "Max iterations reached");
                return Err(AppError::Agent(format!(
                    "Agent '{}' exceeded max iterations ({})",
                    agent.name(),
                    MAX_ITERATIONS
                )));
            }

            iterations += 1;

            let request = CompletionRequest {
                model: String::new(), // Provider uses its default
                messages: messages.clone(),
                temperature: Some(0.3),
                max_tokens: Some(4000),
                system_prompt: Some(system_prompt.clone()),
            };

            if tool_defs.is_empty() {
                // No tools — simple completion
                let response = provider.complete(request).await?;
                return Ok(AgentResponse {
                    content: response.content,
                    tool_calls_made: tool_records,
                    usage: response.usage,
                    iterations,
                });
            }

            // Completion with tools
            let response = provider.complete_with_tools(request, &tool_defs).await?;

            if response.tool_calls.is_empty() {
                // No more tool calls — agent is done
                return Ok(AgentResponse {
                    content: response.content.unwrap_or_default(),
                    tool_calls_made: tool_records,
                    usage: response.usage,
                    iterations,
                });
            }

            // Execute tool calls
            for tool_call in &response.tool_calls {
                if cancel.is_cancelled() {
                    return Err(AppError::Cancelled);
                }

                info!(agent = agent.name(), tool = %tool_call.name, "Executing tool");
                let start = std::time::Instant::now();

                let result = if let Some(tool) = self.tool_registry.get(&tool_call.name) {
                    tool.execute(tool_call.arguments.clone()).await
                        .unwrap_or_else(|e| ToolOutput::error(e.to_string()))
                } else {
                    ToolOutput::error(format!("Unknown tool: {}", tool_call.name))
                };

                let duration = start.elapsed();

                tool_records.push(AgentToolCallRecord {
                    tool_name: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                    result: result.content.clone(),
                    duration_ms: duration.as_millis() as u64,
                });

                // Add assistant message with tool call
                messages.push(Message {
                    role: Role::Assistant,
                    content: MessageContent::Text(format!(
                        "I'll use the {} tool.",
                        tool_call.name
                    )),
                });

                // Add tool result
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: result.content,
                    },
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrator_creates() {
        let registry = ToolRegistry::with_defaults();
        let orchestrator = AgentOrchestrator::new(registry);
        // Just verifying creation doesn't panic
        let _ = orchestrator;
    }
}
```

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p medical-agents`
Commit: `git commit -m "feat(agents): add AgentOrchestrator with tool execution loop and cancellation"`

---

### Task 10: Agents — All 8 Agent Implementations

**Files:**
- Create: `crates/agents/src/agents/mod.rs`
- Create: `crates/agents/src/agents/medication.rs`
- Create: `crates/agents/src/agents/diagnostic.rs`
- Create: `crates/agents/src/agents/compliance.rs`
- Create: `crates/agents/src/agents/data_extraction.rs`
- Create: `crates/agents/src/agents/workflow.rs`
- Create: `crates/agents/src/agents/referral.rs`
- Create: `crates/agents/src/agents/synopsis.rs`
- Create: `crates/agents/src/agents/chat.rs`

- [ ] **Step 1: Write agent registry and all 8 agents**

Each agent is a struct implementing the `Agent` trait with: name, description, system_prompt, available_tools, and execute (delegates to orchestrator).

Write `crates/agents/src/agents/mod.rs`:
```rust
pub mod medication;
pub mod diagnostic;
pub mod compliance;
pub mod data_extraction;
pub mod workflow;
pub mod referral;
pub mod synopsis;
pub mod chat;

pub use medication::MedicationAgent;
pub use diagnostic::DiagnosticAgent;
pub use compliance::ComplianceAgent;
pub use data_extraction::DataExtractionAgent;
pub use workflow::WorkflowAgent;
pub use referral::ReferralAgent;
pub use synopsis::SynopsisAgent;
pub use chat::ChatAgent;
```

Write `crates/agents/src/agents/medication.rs`:
```rust
use async_trait::async_trait;
use medical_core::error::AppResult;
use medical_core::traits::Agent;
use medical_core::types::agent::*;

pub struct MedicationAgent;

#[async_trait]
impl Agent for MedicationAgent {
    fn name(&self) -> &str { "medication" }
    fn description(&self) -> &str { "Drug-drug interactions, dosage validation, prescription generation" }
    fn system_prompt(&self) -> &str {
        "You are a clinical pharmacology specialist with expertise in medication management \
         and clinical decision support. Your role is to:\n\
         1. Identify potential drug-drug interactions\n\
         2. Validate dosages against standard guidelines\n\
         3. Check for contraindications based on patient conditions\n\
         4. Flag Beers Criteria medications for elderly patients\n\
         5. Recommend therapeutic drug monitoring when appropriate\n\n\
         Always cite evidence levels and provide references where possible."
    }
    fn available_tools(&self) -> Vec<ToolDef> {
        vec![
            ToolDef { name: "lookup_drug_interactions".into(), description: "Check drug-drug interactions".into(), parameters: serde_json::json!({"type": "object", "properties": {"medications": {"type": "array", "items": {"type": "string"}}}, "required": ["medications"]}) },
            ToolDef { name: "search_icd_codes".into(), description: "Search ICD codes".into(), parameters: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}) },
        ]
    }
    async fn execute(&self, _context: AgentContext) -> AppResult<AgentResponse> {
        // Execution delegated to AgentOrchestrator
        Err(medical_core::error::AppError::Agent("Use AgentOrchestrator::execute instead".into()))
    }
}
```

Write each remaining agent following the same pattern. Each has unique: name, description, system_prompt, available_tools.

**diagnostic.rs**: name="diagnostic", tools=[search_icd_codes, extract_vitals], prompt about differential diagnosis with ICD codes and confidence percentages.

**compliance.rs**: name="compliance", tools=[generate_checklist], prompt about SOAP note audit against documentation standards.

**data_extraction.rs**: name="data_extraction", tools=[extract_vitals], prompt about extracting structured clinical data (vitals, labs, medications, diagnoses, allergies).

**workflow.rs**: name="workflow", tools=[generate_checklist], prompt about step-by-step clinical guidance with checklists.

**referral.rs**: name="referral", tools=[search_icd_codes], prompt about professional referral letter generation with specialty inference.

**synopsis.rs**: name="synopsis", tools=[] (no tools), prompt about creating concise SOAP note summaries under 200 words.

**chat.rs**: name="chat", tools=[search_icd_codes, lookup_drug_interactions, extract_vitals, search_knowledge_base, generate_checklist] (all tools), prompt about conversational medical AI with full tool access.

- [ ] **Step 2: Write tests**

Add to `crates/agents/src/agents/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::traits::Agent;

    #[test]
    fn all_agents_have_unique_names() {
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(MedicationAgent),
            Box::new(DiagnosticAgent),
            Box::new(ComplianceAgent),
            Box::new(DataExtractionAgent),
            Box::new(WorkflowAgent),
            Box::new(ReferralAgent),
            Box::new(SynopsisAgent),
            Box::new(ChatAgent),
        ];
        let mut names: Vec<&str> = agents.iter().map(|a| a.name()).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "Agent names must be unique");
    }

    #[test]
    fn chat_agent_has_all_tools() {
        let agent = ChatAgent;
        assert!(agent.available_tools().len() >= 5);
    }

    #[test]
    fn synopsis_agent_has_no_tools() {
        let agent = SynopsisAgent;
        assert!(agent.available_tools().is_empty());
    }

    #[test]
    fn all_agents_have_system_prompts() {
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(MedicationAgent), Box::new(DiagnosticAgent),
            Box::new(ComplianceAgent), Box::new(DataExtractionAgent),
            Box::new(WorkflowAgent), Box::new(ReferralAgent),
            Box::new(SynopsisAgent), Box::new(ChatAgent),
        ];
        for agent in &agents {
            assert!(!agent.system_prompt().is_empty(), "Agent '{}' has empty system prompt", agent.name());
            assert!(agent.system_prompt().len() > 50, "Agent '{}' system prompt too short", agent.name());
        }
    }
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p medical-agents`
Commit: `git commit -m "feat(agents): add 8 medical agents (medication, diagnostic, compliance, data extraction, workflow, referral, synopsis, chat)"`

---

### Task 11: Processing — SOAP Generator and Document Generators

**Files:**
- Create: `crates/processing/src/soap_generator.rs`
- Create: `crates/processing/src/document_generator.rs`

- [ ] **Step 1: Write SOAP generator**

Write `crates/processing/src/soap_generator.rs`:
```rust
use medical_core::types::settings::SoapTemplate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapPromptConfig {
    pub template: SoapTemplate,
    pub icd_version: String,
    pub custom_prompt: Option<String>,
    pub include_context: bool,
}

impl Default for SoapPromptConfig {
    fn default() -> Self {
        Self {
            template: SoapTemplate::FollowUp,
            icd_version: "ICD-10".into(),
            custom_prompt: None,
            include_context: true,
        }
    }
}

/// Build the system prompt for SOAP note generation.
pub fn build_soap_prompt(config: &SoapPromptConfig) -> String {
    if let Some(custom) = &config.custom_prompt {
        if !custom.is_empty() {
            return custom.clone();
        }
    }

    let template_instruction = match config.template {
        SoapTemplate::FollowUp => "This is a follow-up visit. Focus on changes since last visit, treatment response, and updated plan.",
        SoapTemplate::NewPatient => "This is a new patient encounter. Include comprehensive history, review of systems, and initial assessment.",
        SoapTemplate::Telehealth => "This is a telehealth visit. Note limitations of remote examination and any recommended in-person follow-up.",
        SoapTemplate::Emergency => "This is an emergency encounter. Prioritize chief complaint, acute findings, and immediate interventions.",
        SoapTemplate::Pediatric => "This is a pediatric encounter. Include developmental milestones, growth parameters, and age-appropriate assessments.",
        SoapTemplate::Geriatric => "This is a geriatric encounter. Address functional status, fall risk, cognitive screening, and polypharmacy review.",
    };

    format!(
        "You are a medical documentation specialist. Generate a comprehensive SOAP note from the provided transcript.\n\n\
         {template_instruction}\n\n\
         Format the note with clear sections:\n\
         S (Subjective): Chief complaint, HPI, review of systems, relevant history\n\
         O (Objective): Vital signs, physical exam findings, lab/imaging results\n\
         A (Assessment): Diagnoses with {icd} codes, clinical reasoning\n\
         P (Plan): Treatment plan, medications, follow-up, patient education\n\n\
         Include {icd} codes for all diagnoses. Be thorough but concise.",
        icd = config.icd_version,
    )
}

/// Build the user prompt with transcript and optional context.
pub fn build_user_prompt(transcript: &str, context: Option<&str>) -> String {
    let mut prompt = format!("Generate a SOAP note from this transcript:\n\n{transcript}");
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            prompt.push_str(&format!("\n\nAdditional context:\n{ctx}"));
        }
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_includes_icd() {
        let config = SoapPromptConfig::default();
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("ICD-10"));
        assert!(prompt.contains("SOAP note"));
    }

    #[test]
    fn custom_prompt_overrides() {
        let config = SoapPromptConfig {
            custom_prompt: Some("My custom prompt".into()),
            ..Default::default()
        };
        assert_eq!(build_soap_prompt(&config), "My custom prompt");
    }

    #[test]
    fn template_specific_instructions() {
        let config = SoapPromptConfig {
            template: SoapTemplate::Emergency,
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("emergency"));
    }

    #[test]
    fn user_prompt_with_context() {
        let prompt = build_user_prompt("transcript text", Some("prior visit notes"));
        assert!(prompt.contains("transcript text"));
        assert!(prompt.contains("prior visit notes"));
    }

    #[test]
    fn user_prompt_without_context() {
        let prompt = build_user_prompt("transcript text", None);
        assert!(prompt.contains("transcript text"));
        assert!(!prompt.contains("context"));
    }
}
```

- [ ] **Step 2: Write document generator**

Write `crates/processing/src/document_generator.rs`:
```rust
/// Prompt builder for referral letters.
pub fn build_referral_prompt(
    soap_note: &str,
    recipient_type: &str,
    urgency: &str,
) -> (String, String) {
    let system = format!(
        "You are a medical referral specialist. Generate a professional, focused referral letter.\n\
         Recipient type: {recipient_type}\n\
         Urgency: {urgency}\n\n\
         Include: reason for referral, relevant clinical findings, current medications, \
         specific questions for the specialist, and requested timeline."
    );

    let user = format!(
        "Generate a referral letter based on this SOAP note:\n\n{soap_note}"
    );

    (system, user)
}

/// Prompt builder for patient correspondence letters.
pub fn build_letter_prompt(soap_note: &str, letter_type: &str) -> (String, String) {
    let system = format!(
        "You are a medical correspondence specialist. Generate a {letter_type} letter for the patient.\n\
         Use clear, non-technical language. Ensure the letter is warm, professional, and informative."
    );

    let user = format!(
        "Generate a {letter_type} letter based on this SOAP note:\n\n{soap_note}"
    );

    (system, user)
}

/// Prompt builder for synopsis generation.
pub fn build_synopsis_prompt(soap_note: &str) -> (String, String) {
    let system = "You are a medical documentation specialist. Create a concise synopsis of the SOAP note in under 200 words. \
         Focus on key findings, diagnosis, and plan.".to_string();

    let user = format!("Summarize this SOAP note:\n\n{soap_note}");

    (system, user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_prompt_includes_recipient_and_urgency() {
        let (system, user) = build_referral_prompt("S: Headache", "specialist", "urgent");
        assert!(system.contains("specialist"));
        assert!(system.contains("urgent"));
        assert!(user.contains("S: Headache"));
    }

    #[test]
    fn letter_prompt_includes_type() {
        let (system, _) = build_letter_prompt("S: Follow up", "follow-up summary");
        assert!(system.contains("follow-up summary"));
    }

    #[test]
    fn synopsis_prompt_word_limit() {
        let (system, _) = build_synopsis_prompt("S: Test");
        assert!(system.contains("200 words"));
    }
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p medical-processing`
Commit: `git commit -m "feat(processing): add SOAP generator with templates and document generator prompt builders"`

---

### Task 12: Processing — Pipeline and Batch Processor

**Files:**
- Create: `crates/processing/src/pipeline.rs`
- Create: `crates/processing/src/batch.rs`

- [ ] **Step 1: Write recording pipeline**

Write `crates/processing/src/pipeline.rs`:
```rust
use medical_core::types::processing::ProcessingEvent;
use medical_core::types::recording::{Recording, ProcessingStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;
use crate::ProcessingResult;

/// Configuration for a processing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub generate_soap: bool,
    pub generate_referral: bool,
    pub generate_letter: bool,
    pub auto_index_rag: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            generate_soap: true,
            generate_referral: false,
            generate_letter: false,
            auto_index_rag: true,
        }
    }
}

/// The pipeline steps in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStep {
    Transcribing,
    GeneratingSoap,
    GeneratingReferral,
    GeneratingLetter,
    ExtractingData,
    IndexingRag,
    Complete,
}

impl PipelineStep {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Transcribing => "Transcribing audio",
            Self::GeneratingSoap => "Generating SOAP note",
            Self::GeneratingReferral => "Generating referral letter",
            Self::GeneratingLetter => "Generating patient letter",
            Self::ExtractingData => "Extracting structured data",
            Self::IndexingRag => "Indexing in knowledge base",
            Self::Complete => "Complete",
        }
    }
}

/// Progress sender for pipeline events.
pub type ProgressSender = mpsc::Sender<ProcessingEvent>;

/// Execute the processing pipeline for a single recording.
/// This is the coordination logic — actual AI calls are injected via closures.
pub async fn run_pipeline(
    recording_id: Uuid,
    config: &PipelineConfig,
    progress: &ProgressSender,
) -> ProcessingResult<Vec<PipelineStep>> {
    let mut completed_steps = Vec::new();

    // Step 1: Transcription (always)
    send_step(progress, recording_id, PipelineStep::Transcribing).await;
    completed_steps.push(PipelineStep::Transcribing);
    send_progress(progress, recording_id, 0.2).await;

    // Step 2: SOAP generation
    if config.generate_soap {
        send_step(progress, recording_id, PipelineStep::GeneratingSoap).await;
        completed_steps.push(PipelineStep::GeneratingSoap);
        send_progress(progress, recording_id, 0.4).await;
    }

    // Step 3: Referral (optional)
    if config.generate_referral {
        send_step(progress, recording_id, PipelineStep::GeneratingReferral).await;
        completed_steps.push(PipelineStep::GeneratingReferral);
        send_progress(progress, recording_id, 0.6).await;
    }

    // Step 4: Letter (optional)
    if config.generate_letter {
        send_step(progress, recording_id, PipelineStep::GeneratingLetter).await;
        completed_steps.push(PipelineStep::GeneratingLetter);
        send_progress(progress, recording_id, 0.7).await;
    }

    // Step 5: Data extraction
    send_step(progress, recording_id, PipelineStep::ExtractingData).await;
    completed_steps.push(PipelineStep::ExtractingData);
    send_progress(progress, recording_id, 0.85).await;

    // Step 6: RAG indexing (optional)
    if config.auto_index_rag {
        send_step(progress, recording_id, PipelineStep::IndexingRag).await;
        completed_steps.push(PipelineStep::IndexingRag);
        send_progress(progress, recording_id, 0.95).await;
    }

    // Done
    send_step(progress, recording_id, PipelineStep::Complete).await;
    completed_steps.push(PipelineStep::Complete);
    let _ = progress.send(ProcessingEvent::Completed { recording_id }).await;

    Ok(completed_steps)
}

async fn send_step(tx: &ProgressSender, recording_id: Uuid, step: PipelineStep) {
    let _ = tx.send(ProcessingEvent::StepChanged {
        recording_id,
        step: step.label().to_string(),
    }).await;
}

async fn send_progress(tx: &ProgressSender, recording_id: Uuid, percent: f32) {
    let _ = tx.send(ProcessingEvent::Progress {
        recording_id,
        percent,
    }).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pipeline_default_steps() {
        let (tx, mut rx) = mpsc::channel(64);
        let config = PipelineConfig::default();
        let id = Uuid::new_v4();

        let steps = run_pipeline(id, &config, &tx).await.unwrap();

        // Default: transcribe + soap + extract + rag + complete
        assert!(steps.contains(&PipelineStep::Transcribing));
        assert!(steps.contains(&PipelineStep::GeneratingSoap));
        assert!(steps.contains(&PipelineStep::ExtractingData));
        assert!(steps.contains(&PipelineStep::IndexingRag));
        assert!(steps.contains(&PipelineStep::Complete));
        assert!(!steps.contains(&PipelineStep::GeneratingReferral));
        assert!(!steps.contains(&PipelineStep::GeneratingLetter));

        // Verify events were sent
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn pipeline_all_steps() {
        let (tx, _rx) = mpsc::channel(64);
        let config = PipelineConfig {
            generate_soap: true,
            generate_referral: true,
            generate_letter: true,
            auto_index_rag: true,
        };

        let steps = run_pipeline(Uuid::new_v4(), &config, &tx).await.unwrap();
        assert_eq!(steps.len(), 7); // all 6 steps + complete
    }

    #[test]
    fn step_labels() {
        assert_eq!(PipelineStep::Transcribing.label(), "Transcribing audio");
        assert_eq!(PipelineStep::Complete.label(), "Complete");
    }
}
```

- [ ] **Step 2: Write batch processor**

Write `crates/processing/src/batch.rs`:
```rust
use medical_core::types::processing::*;
use chrono::Utc;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Batch processing job tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: Uuid,
    pub recording_ids: Vec<Uuid>,
    pub config: super::pipeline::PipelineConfig,
    pub status: BatchState,
    pub total_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub created_at: chrono::DateTime<Utc>,
}

impl BatchJob {
    pub fn new(recording_ids: Vec<Uuid>, config: super::pipeline::PipelineConfig) -> Self {
        let total = recording_ids.len();
        Self {
            id: Uuid::new_v4(),
            recording_ids,
            config,
            status: BatchState::Pending,
            total_count: total,
            completed_count: 0,
            failed_count: 0,
            created_at: Utc::now(),
        }
    }

    pub fn record_success(&mut self) {
        self.completed_count += 1;
        self.update_status();
    }

    pub fn record_failure(&mut self) {
        self.failed_count += 1;
        self.update_status();
    }

    pub fn is_done(&self) -> bool {
        self.completed_count + self.failed_count >= self.total_count
    }

    pub fn progress_percent(&self) -> f32 {
        if self.total_count == 0 {
            return 1.0;
        }
        (self.completed_count + self.failed_count) as f32 / self.total_count as f32
    }

    fn update_status(&mut self) {
        if !self.is_done() {
            self.status = BatchState::Processing;
        } else if self.failed_count == 0 {
            self.status = BatchState::Completed;
        } else if self.completed_count == 0 {
            self.status = BatchState::Failed;
        } else {
            self.status = BatchState::PartiallyCompleted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_batch_job() {
        let ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let job = BatchJob::new(ids, super::super::pipeline::PipelineConfig::default());
        assert_eq!(job.total_count, 3);
        assert_eq!(job.completed_count, 0);
        assert_eq!(job.status, BatchState::Pending);
        assert!(!job.is_done());
    }

    #[test]
    fn batch_tracks_progress() {
        let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let mut job = BatchJob::new(ids, super::super::pipeline::PipelineConfig::default());
        job.record_success();
        assert_eq!(job.completed_count, 1);
        assert!(!job.is_done());
        assert_eq!(job.status, BatchState::Processing);
        assert!((job.progress_percent() - 0.5).abs() < 0.01);

        job.record_success();
        assert!(job.is_done());
        assert_eq!(job.status, BatchState::Completed);
    }

    #[test]
    fn batch_partial_failure() {
        let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let mut job = BatchJob::new(ids, super::super::pipeline::PipelineConfig::default());
        job.record_success();
        job.record_failure();
        assert!(job.is_done());
        assert_eq!(job.status, BatchState::PartiallyCompleted);
    }

    #[test]
    fn batch_total_failure() {
        let ids = vec![Uuid::new_v4()];
        let mut job = BatchJob::new(ids, super::super::pipeline::PipelineConfig::default());
        job.record_failure();
        assert!(job.is_done());
        assert_eq!(job.status, BatchState::Failed);
    }

    #[test]
    fn empty_batch() {
        let job = BatchJob::new(vec![], super::super::pipeline::PipelineConfig::default());
        assert!(job.is_done());
        assert_eq!(job.progress_percent(), 1.0);
    }
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p medical-processing`
Commit: `git commit -m "feat(processing): add recording pipeline with progress events and batch processor"`

---

### Task 13: Final Verification

- [ ] **Step 1: Build entire workspace**

Run: `cargo build --workspace`
Expected: Clean build.

- [ ] **Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass. Report total count.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace`
Fix any warnings.

- [ ] **Step 4: Commit fixes and push**

```bash
git add -A
git commit -m "fix: address clippy warnings in Plan 3 crates"
git push origin master
```
