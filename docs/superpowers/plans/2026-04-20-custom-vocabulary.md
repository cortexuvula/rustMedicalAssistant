# Custom Vocabulary Correction System — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the custom vocabulary correction system from the Python Medical-Assistant so that configurable find/replace rules are applied to transcripts immediately after STT.

**Architecture:** New types in `crates/core`, new DB migration + repo in `crates/db`, correction engine in `crates/processing`, Tauri commands in `src-tauri`, Svelte UI in `src/lib`. Corrections run once after STT; the corrected transcript is stored and used by all downstream consumers.

**Tech Stack:** Rust (regex, serde, rusqlite, uuid, chrono), TypeScript/Svelte 5, Tauri v2

**Spec:** `docs/superpowers/specs/2026-04-20-custom-vocabulary-design.md`

---

## File Map

### New files
| File | Responsibility |
|------|---------------|
| `crates/core/src/types/vocabulary.rs` | `VocabularyEntry`, `VocabularyCategory`, `CorrectionResult`, `AppliedCorrection` types |
| `crates/db/src/migrations/m003_vocabulary.rs` | `vocabulary_entries` table migration |
| `crates/db/src/vocabulary.rs` | `VocabularyRepo` — all CRUD + count queries |
| `crates/processing/src/vocabulary_corrector.rs` | Correction engine + unit tests |
| `src-tauri/src/commands/vocabulary.rs` | Tauri commands for CRUD, import/export, test |
| `src/lib/api/vocabulary.ts` | Frontend API bindings |
| `src/lib/components/VocabularyDialog.svelte` | Modal dialog for vocabulary management |

### Modified files
| File | Change |
|------|--------|
| `crates/core/src/types/mod.rs` | Add `pub mod vocabulary` + re-export |
| `crates/core/src/types/settings.rs` | Add `vocabulary_enabled` field to `AppConfig` |
| `crates/db/src/lib.rs` | Add `pub mod vocabulary` |
| `crates/db/src/migrations/mod.rs` | Register m003 migration |
| `crates/processing/src/lib.rs` | Add `pub mod vocabulary_corrector` |
| `src-tauri/src/commands/mod.rs` | Add `pub mod vocabulary` |
| `src-tauri/src/lib.rs` | Register vocabulary commands in `generate_handler!` |
| `src-tauri/src/commands/transcription.rs` | Hook corrections after STT |
| `src/lib/types/index.ts` | Add vocabulary TypeScript types |
| `src/lib/stores/settings.ts` | Add `vocabulary_enabled` default |
| `src/lib/components/SettingsContent.svelte` | Add vocabulary section to General tab |

---

## Task 1: Core Types

**Files:**
- Create: `crates/core/src/types/vocabulary.rs`
- Modify: `crates/core/src/types/mod.rs`

- [ ] **Step 1: Create vocabulary types**

```rust
// crates/core/src/types/vocabulary.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyCategory {
    DoctorNames,
    MedicationNames,
    MedicalTerminology,
    Abbreviations,
    General,
}

impl Default for VocabularyCategory {
    fn default() -> Self {
        Self::General
    }
}

impl VocabularyCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DoctorNames => "doctor_names",
            Self::MedicationNames => "medication_names",
            Self::MedicalTerminology => "medical_terminology",
            Self::Abbreviations => "abbreviations",
            Self::General => "general",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "doctor_names" => Self::DoctorNames,
            "medication_names" => Self::MedicationNames,
            "medical_terminology" => Self::MedicalTerminology,
            "abbreviations" => Self::Abbreviations,
            _ => Self::General,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyEntry {
    pub id: Uuid,
    pub find_text: String,
    pub replacement: String,
    pub category: VocabularyCategory,
    pub case_sensitive: bool,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedCorrection {
    pub find_text: String,
    pub replacement: String,
    pub category: VocabularyCategory,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionResult {
    pub original_text: String,
    pub corrected_text: String,
    pub corrections_applied: Vec<AppliedCorrection>,
    pub total_replacements: u32,
}
```

- [ ] **Step 2: Register the module**

Add to `crates/core/src/types/mod.rs`:

```rust
pub mod vocabulary;
```

And add the re-export:

```rust
pub use vocabulary::*;
```

The file should look like:

```rust
pub mod recording;
pub mod processing;
pub mod agent;
pub mod ai;
pub mod stt;
pub mod tts;
pub mod rag;
pub mod settings;
pub mod vocabulary;

pub use recording::*;
pub use processing::*;
pub use agent::*;
pub use ai::*;
pub use stt::*;
pub use tts::*;
pub use rag::*;
pub use settings::*;
pub use vocabulary::*;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p medical-core`
Expected: success with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/types/vocabulary.rs crates/core/src/types/mod.rs
git commit -m "feat(core): add vocabulary entry and correction result types"
```

---

## Task 2: Database Migration

**Files:**
- Create: `crates/db/src/migrations/m003_vocabulary.rs`
- Modify: `crates/db/src/migrations/mod.rs`

- [ ] **Step 1: Create the migration**

```rust
// crates/db/src/migrations/m003_vocabulary.rs

use rusqlite::Connection;

use crate::DbResult;

pub fn up(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS vocabulary_entries (
            id              TEXT PRIMARY KEY NOT NULL,
            find_text       TEXT NOT NULL,
            replacement     TEXT NOT NULL,
            category        TEXT NOT NULL DEFAULT 'general',
            case_sensitive  INTEGER NOT NULL DEFAULT 0,
            priority        INTEGER NOT NULL DEFAULT 0,
            enabled         INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_vocabulary_find_text
            ON vocabulary_entries(find_text);
    "#)?;
    Ok(())
}
```

- [ ] **Step 2: Register migration in mod.rs**

In `crates/db/src/migrations/mod.rs`, add the module declaration after line 8:

```rust
pub mod m003_vocabulary;
```

And add the migration entry to `all_migrations()` after the m002 entry:

```rust
Migration {
    version: 3,
    name: "vocabulary",
    up: m003_vocabulary::up,
},
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p medical-db`
Expected: success

- [ ] **Step 4: Run existing migration tests**

Run: `cargo test -p medical-db -- migrations`
Expected: all pass (fresh DB now applies 3 migrations)

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/migrations/m003_vocabulary.rs crates/db/src/migrations/mod.rs
git commit -m "feat(db): add vocabulary_entries table migration"
```

---

## Task 3: Database Repository

**Files:**
- Create: `crates/db/src/vocabulary.rs`
- Modify: `crates/db/src/lib.rs`

- [ ] **Step 1: Create VocabularyRepo**

```rust
// crates/db/src/vocabulary.rs

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use medical_core::types::vocabulary::{VocabularyCategory, VocabularyEntry};

use crate::{DbError, DbResult};

pub struct VocabularyRepo;

impl VocabularyRepo {
    pub fn list_all(conn: &Connection) -> DbResult<Vec<VocabularyEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries
             ORDER BY priority DESC, length(find_text) DESC"
        )?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn list_enabled(conn: &Connection) -> DbResult<Vec<VocabularyEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries
             WHERE enabled = 1
             ORDER BY priority DESC, length(find_text) DESC"
        )?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn list_by_category(conn: &Connection, category: &VocabularyCategory) -> DbResult<Vec<VocabularyEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries
             WHERE category = ?1
             ORDER BY priority DESC, length(find_text) DESC"
        )?;
        let rows = stmt.query_map([category.as_str()], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn get_by_id(conn: &Connection, id: &Uuid) -> DbResult<VocabularyEntry> {
        conn.query_row(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries WHERE id = ?1",
            [id.to_string()],
            Self::row_to_entry,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                DbError::NotFound(format!("vocabulary entry {id}"))
            }
            other => DbError::Sqlite(other),
        })
    }

    pub fn insert(conn: &Connection, entry: &VocabularyEntry) -> DbResult<()> {
        conn.execute(
            "INSERT INTO vocabulary_entries (id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry.id.to_string(),
                entry.find_text,
                entry.replacement,
                entry.category.as_str(),
                entry.case_sensitive as i32,
                entry.priority,
                entry.enabled as i32,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert(conn: &Connection, entry: &VocabularyEntry) -> DbResult<()> {
        conn.execute(
            "INSERT INTO vocabulary_entries (id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(find_text) DO UPDATE SET
                 replacement = excluded.replacement,
                 category = excluded.category,
                 case_sensitive = excluded.case_sensitive,
                 priority = excluded.priority,
                 enabled = excluded.enabled,
                 updated_at = excluded.updated_at",
            rusqlite::params![
                entry.id.to_string(),
                entry.find_text,
                entry.replacement,
                entry.category.as_str(),
                entry.case_sensitive as i32,
                entry.priority,
                entry.enabled as i32,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn update(conn: &Connection, entry: &VocabularyEntry) -> DbResult<()> {
        let rows = conn.execute(
            "UPDATE vocabulary_entries SET find_text = ?1, replacement = ?2, category = ?3, case_sensitive = ?4, priority = ?5, enabled = ?6, updated_at = ?7
             WHERE id = ?8",
            rusqlite::params![
                entry.find_text,
                entry.replacement,
                entry.category.as_str(),
                entry.case_sensitive as i32,
                entry.priority,
                entry.enabled as i32,
                entry.updated_at.to_rfc3339(),
                entry.id.to_string(),
            ],
        )?;
        if rows == 0 {
            return Err(DbError::NotFound(format!("vocabulary entry {}", entry.id)));
        }
        Ok(())
    }

    pub fn delete(conn: &Connection, id: &Uuid) -> DbResult<()> {
        let rows = conn.execute(
            "DELETE FROM vocabulary_entries WHERE id = ?1",
            [id.to_string()],
        )?;
        if rows == 0 {
            return Err(DbError::NotFound(format!("vocabulary entry {id}")));
        }
        Ok(())
    }

    pub fn delete_all(conn: &Connection) -> DbResult<u32> {
        let rows = conn.execute("DELETE FROM vocabulary_entries", [])?;
        Ok(rows as u32)
    }

    pub fn count(conn: &Connection) -> DbResult<(u32, u32)> {
        let total: u32 = conn.query_row(
            "SELECT COUNT(*) FROM vocabulary_entries",
            [],
            |r| r.get(0),
        )?;
        let enabled: u32 = conn.query_row(
            "SELECT COUNT(*) FROM vocabulary_entries WHERE enabled = 1",
            [],
            |r| r.get(0),
        )?;
        Ok((total, enabled))
    }

    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<VocabularyEntry> {
        let id_str: String = row.get(0)?;
        let category_str: String = row.get(3)?;
        let case_sensitive_int: i32 = row.get(4)?;
        let enabled_int: i32 = row.get(6)?;
        let created_str: String = row.get(7)?;
        let updated_str: String = row.get(8)?;

        Ok(VocabularyEntry {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::nil()),
            find_text: row.get(1)?,
            replacement: row.get(2)?,
            category: VocabularyCategory::from_str(&category_str),
            case_sensitive: case_sensitive_int != 0,
            priority: row.get(5)?,
            enabled: enabled_int != 0,
            created_at: DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}
```

- [ ] **Step 2: Register in lib.rs**

Add to `crates/db/src/lib.rs` after line 8 (`pub mod search;`):

```rust
pub mod vocabulary;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p medical-db`
Expected: success

- [ ] **Step 4: Commit**

```bash
git add crates/db/src/vocabulary.rs crates/db/src/lib.rs
git commit -m "feat(db): add VocabularyRepo with CRUD and upsert operations"
```

---

## Task 4: Correction Engine with Tests

**Files:**
- Create: `crates/processing/src/vocabulary_corrector.rs`
- Modify: `crates/processing/src/lib.rs`

- [ ] **Step 1: Write the correction engine with tests**

```rust
// crates/processing/src/vocabulary_corrector.rs

use std::collections::HashMap;

use regex::Regex;
use tracing::{debug, info};

use medical_core::types::vocabulary::{
    AppliedCorrection, CorrectionResult, VocabularyEntry,
};

pub fn apply_corrections(text: &str, entries: &[VocabularyEntry]) -> CorrectionResult {
    if text.is_empty() || entries.is_empty() {
        return CorrectionResult {
            original_text: text.to_string(),
            corrected_text: text.to_string(),
            corrections_applied: vec![],
            total_replacements: 0,
        };
    }

    let mut sorted: Vec<&VocabularyEntry> = entries.iter().filter(|e| e.enabled).collect();
    sorted.sort_by(|a, b| {
        b.priority.cmp(&a.priority)
            .then_with(|| b.find_text.len().cmp(&a.find_text.len()))
    });

    let mut corrected = text.to_string();
    let mut applied = Vec::new();
    let mut total = 0u32;
    let mut cache: HashMap<(String, bool), Option<Regex>> = HashMap::new();

    for entry in sorted {
        let key = (entry.find_text.clone(), entry.case_sensitive);
        let pattern = cache.entry(key).or_insert_with(|| {
            let escaped = regex::escape(&entry.find_text);
            let pat = format!(r"\b{escaped}\b");
            let flags = if entry.case_sensitive { "" } else { "(?i)" };
            Regex::new(&format!("{flags}{pat}")).ok()
        });

        if let Some(re) = pattern {
            let count = re.find_iter(&corrected).count();
            if count > 0 {
                corrected = re.replace_all(&corrected, entry.replacement.as_str()).into_owned();
                total += count as u32;
                applied.push(AppliedCorrection {
                    find_text: entry.find_text.clone(),
                    replacement: entry.replacement.clone(),
                    category: entry.category.clone(),
                    count: count as u32,
                });
                debug!(
                    find = %entry.find_text,
                    replace = %entry.replacement,
                    count,
                    "Applied vocabulary correction"
                );
            }
        }
    }

    if total > 0 {
        info!(total_replacements = total, "Vocabulary corrections applied");
    }

    CorrectionResult {
        original_text: text.to_string(),
        corrected_text: corrected,
        corrections_applied: applied,
        total_replacements: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medical_core::types::vocabulary::VocabularyCategory;
    use uuid::Uuid;

    fn entry(find: &str, replace: &str) -> VocabularyEntry {
        VocabularyEntry {
            id: Uuid::new_v4(),
            find_text: find.to_string(),
            replacement: replace.to_string(),
            category: VocabularyCategory::General,
            case_sensitive: false,
            priority: 0,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_text_returns_unchanged() {
        let result = apply_corrections("", &[entry("htn", "hypertension")]);
        assert_eq!(result.corrected_text, "");
        assert_eq!(result.total_replacements, 0);
    }

    #[test]
    fn empty_entries_returns_unchanged() {
        let result = apply_corrections("patient has htn", &[]);
        assert_eq!(result.corrected_text, "patient has htn");
        assert_eq!(result.total_replacements, 0);
    }

    #[test]
    fn basic_replacement() {
        let entries = vec![entry("htn", "hypertension")];
        let result = apply_corrections("patient has htn", &entries);
        assert_eq!(result.corrected_text, "patient has hypertension");
        assert_eq!(result.total_replacements, 1);
        assert_eq!(result.corrections_applied.len(), 1);
        assert_eq!(result.corrections_applied[0].find_text, "htn");
        assert_eq!(result.corrections_applied[0].count, 1);
    }

    #[test]
    fn word_boundary_prevents_partial_match() {
        let entries = vec![entry("htn", "hypertension")];
        let result = apply_corrections("washington is a city", &entries);
        assert_eq!(result.corrected_text, "washington is a city");
        assert_eq!(result.total_replacements, 0);
    }

    #[test]
    fn case_insensitive_by_default() {
        let entries = vec![entry("htn", "hypertension")];
        let result = apply_corrections("patient has HTN", &entries);
        assert_eq!(result.corrected_text, "patient has hypertension");
        assert_eq!(result.total_replacements, 1);
    }

    #[test]
    fn case_sensitive_when_set() {
        let mut e = entry("HTN", "hypertension");
        e.case_sensitive = true;
        let entries = vec![e];

        let result = apply_corrections("patient has htn and HTN", &entries);
        assert_eq!(result.corrected_text, "patient has htn and hypertension");
        assert_eq!(result.total_replacements, 1);
    }

    #[test]
    fn higher_priority_applied_first() {
        let mut e1 = entry("cp", "chest pain");
        e1.priority = 0;
        let mut e2 = entry("sob", "shortness of breath");
        e2.priority = 10;

        let entries = vec![e1, e2];
        let result = apply_corrections("patient reports sob and cp", &entries);
        assert_eq!(result.corrected_text, "patient reports shortness of breath and chest pain");
        assert_eq!(result.total_replacements, 2);
        // sob should be first in corrections_applied (higher priority)
        assert_eq!(result.corrections_applied[0].find_text, "sob");
        assert_eq!(result.corrections_applied[1].find_text, "cp");
    }

    #[test]
    fn longer_match_before_shorter_at_same_priority() {
        let e1 = entry("dm", "diabetes mellitus");
        let e2 = entry("dm type 2", "diabetes mellitus type 2");
        let entries = vec![e1, e2];
        let result = apply_corrections("patient has dm type 2", &entries);
        assert_eq!(result.corrected_text, "patient has diabetes mellitus type 2");
    }

    #[test]
    fn disabled_entries_skipped() {
        let mut e = entry("htn", "hypertension");
        e.enabled = false;
        let entries = vec![e];
        let result = apply_corrections("patient has htn", &entries);
        assert_eq!(result.corrected_text, "patient has htn");
        assert_eq!(result.total_replacements, 0);
    }

    #[test]
    fn multiple_occurrences_counted() {
        let entries = vec![entry("htn", "hypertension")];
        let result = apply_corrections("htn noted, also htn related issues", &entries);
        assert_eq!(result.corrected_text, "hypertension noted, also hypertension related issues");
        assert_eq!(result.total_replacements, 2);
        assert_eq!(result.corrections_applied[0].count, 2);
    }

    #[test]
    fn multiple_different_corrections() {
        let entries = vec![
            entry("htn", "hypertension"),
            entry("dm", "diabetes mellitus"),
        ];
        let result = apply_corrections("patient has htn and dm", &entries);
        assert_eq!(result.corrected_text, "patient has hypertension and diabetes mellitus");
        assert_eq!(result.total_replacements, 2);
        assert_eq!(result.corrections_applied.len(), 2);
    }

    #[test]
    fn preserves_original_text() {
        let entries = vec![entry("htn", "hypertension")];
        let result = apply_corrections("patient has htn", &entries);
        assert_eq!(result.original_text, "patient has htn");
        assert_eq!(result.corrected_text, "patient has hypertension");
    }
}
```

- [ ] **Step 2: Register the module**

Add to `crates/processing/src/lib.rs` after line 4 (`pub mod document_generator;`):

```rust
pub mod vocabulary_corrector;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p medical-processing -- vocabulary_corrector`
Expected: all 11 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/processing/src/vocabulary_corrector.rs crates/processing/src/lib.rs
git commit -m "feat(processing): add vocabulary correction engine with tests"
```

---

## Task 5: Settings Integration

**Files:**
- Modify: `crates/core/src/types/settings.rs`
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/stores/settings.ts`

- [ ] **Step 1: Add vocabulary_enabled to AppConfig**

In `crates/core/src/types/settings.rs`, add the default function after `default_auto_index_rag` (around line 175):

```rust
fn default_vocabulary_enabled() -> bool {
    true
}
```

Add the field to `AppConfig` struct, after the `auto_index_rag` field (around line 236):

```rust
    #[serde(default = "default_vocabulary_enabled")]
    pub vocabulary_enabled: bool,
```

Add assertion to the `default_config_values` test:

```rust
        assert!(config.vocabulary_enabled);
```

- [ ] **Step 2: Add TypeScript type**

In `src/lib/types/index.ts`, add `vocabulary_enabled: boolean;` to the `AppConfig` interface (before the `[key: string]: any;` line).

- [ ] **Step 3: Add frontend default**

In `src/lib/stores/settings.ts`, add to the `defaults` object:

```typescript
    vocabulary_enabled: true,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo test -p medical-core -- settings`
Expected: all settings tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/types/settings.rs src/lib/types/index.ts src/lib/stores/settings.ts
git commit -m "feat(settings): add vocabulary_enabled config field"
```

---

## Task 6: Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/vocabulary.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create vocabulary commands**

```rust
// src-tauri/src/commands/vocabulary.rs

use std::sync::Arc;

use chrono::Utc;
use serde::Deserialize;
use tracing::{info, instrument};
use uuid::Uuid;

use medical_core::types::vocabulary::{CorrectionResult, VocabularyCategory, VocabularyEntry};
use medical_db::vocabulary::VocabularyRepo;
use medical_processing::vocabulary_corrector;

use crate::state::AppState;

#[tauri::command]
pub async fn list_vocabulary_entries(
    state: tauri::State<'_, AppState>,
    category: Option<String>,
) -> Result<Vec<VocabularyEntry>, String> {
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        match category {
            Some(cat) => {
                let cat = VocabularyCategory::from_str(&cat);
                VocabularyRepo::list_by_category(&conn, &cat).map_err(|e| e.to_string())
            }
            None => VocabularyRepo::list_all(&conn).map_err(|e| e.to_string()),
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn add_vocabulary_entry(
    state: tauri::State<'_, AppState>,
    find_text: String,
    replacement: String,
    category: Option<String>,
    case_sensitive: Option<bool>,
    priority: Option<i32>,
    enabled: Option<bool>,
) -> Result<VocabularyEntry, String> {
    let now = Utc::now();
    let entry = VocabularyEntry {
        id: Uuid::new_v4(),
        find_text,
        replacement,
        category: VocabularyCategory::from_str(&category.unwrap_or_default()),
        case_sensitive: case_sensitive.unwrap_or(false),
        priority: priority.unwrap_or(0),
        enabled: enabled.unwrap_or(true),
        created_at: now,
        updated_at: now,
    };
    let db = Arc::clone(&state.db);
    let entry_clone = entry.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::insert(&conn, &entry_clone).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;
    info!(find = %entry.find_text, "Vocabulary entry added");
    Ok(entry)
}

#[tauri::command]
pub async fn update_vocabulary_entry(
    state: tauri::State<'_, AppState>,
    id: String,
    find_text: String,
    replacement: String,
    category: Option<String>,
    case_sensitive: Option<bool>,
    priority: Option<i32>,
    enabled: Option<bool>,
) -> Result<VocabularyEntry, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| format!("Invalid ID: {e}"))?;
    let db = Arc::clone(&state.db);
    let db2 = Arc::clone(&state.db);

    let existing = tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    let entry = VocabularyEntry {
        id: existing.id,
        find_text,
        replacement,
        category: VocabularyCategory::from_str(&category.unwrap_or_else(|| existing.category.as_str().to_string())),
        case_sensitive: case_sensitive.unwrap_or(existing.case_sensitive),
        priority: priority.unwrap_or(existing.priority),
        enabled: enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: Utc::now(),
    };

    let entry_clone = entry.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db2.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::update(&conn, &entry_clone).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;
    Ok(entry)
}

#[tauri::command]
pub async fn delete_vocabulary_entry(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| format!("Invalid ID: {e}"))?;
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::delete(&conn, &uuid).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn delete_all_vocabulary_entries(
    state: tauri::State<'_, AppState>,
) -> Result<u32, String> {
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::delete_all(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_vocabulary_count(
    state: tauri::State<'_, AppState>,
) -> Result<(u32, u32), String> {
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::count(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn import_vocabulary_json(
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<u32, String> {
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;

    #[derive(Deserialize)]
    struct ImportFile {
        corrections: Vec<ImportEntry>,
    }
    #[derive(Deserialize)]
    struct ImportEntry {
        find_text: String,
        replacement: String,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        case_sensitive: Option<bool>,
        #[serde(default)]
        priority: Option<i32>,
        #[serde(default)]
        enabled: Option<bool>,
    }

    let data: ImportFile = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON format: {e}"))?;

    let now = Utc::now();
    let entries: Vec<VocabularyEntry> = data
        .corrections
        .into_iter()
        .filter(|e| !e.find_text.trim().is_empty() && !e.replacement.trim().is_empty())
        .map(|e| VocabularyEntry {
            id: Uuid::new_v4(),
            find_text: e.find_text.trim().to_string(),
            replacement: e.replacement.trim().to_string(),
            category: VocabularyCategory::from_str(&e.category.unwrap_or_default()),
            case_sensitive: e.case_sensitive.unwrap_or(false),
            priority: e.priority.unwrap_or(0),
            enabled: e.enabled.unwrap_or(true),
            created_at: now,
            updated_at: now,
        })
        .collect();

    let count = entries.len() as u32;
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        for entry in &entries {
            VocabularyRepo::upsert(&conn, entry).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    info!(count, path = %file_path, "Imported vocabulary entries");
    Ok(count)
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn export_vocabulary_json(
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<u32, String> {
    let db = Arc::clone(&state.db);
    let entries = tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::list_all(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    let count = entries.len() as u32;
    let export = serde_json::json!({
        "version": "1.0",
        "corrections": entries.iter().map(|e| serde_json::json!({
            "find_text": e.find_text,
            "replacement": e.replacement,
            "category": e.category.as_str(),
            "case_sensitive": e.case_sensitive,
            "priority": e.priority,
            "enabled": e.enabled,
        })).collect::<Vec<_>>()
    });

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| format!("JSON serialization error: {e}"))?;
    tokio::fs::write(&file_path, json)
        .await
        .map_err(|e| format!("Failed to write file: {e}"))?;

    info!(count, path = %file_path, "Exported vocabulary entries");
    Ok(count)
}

#[tauri::command]
pub async fn test_vocabulary_correction(
    state: tauri::State<'_, AppState>,
    text: String,
) -> Result<CorrectionResult, String> {
    let db = Arc::clone(&state.db);
    let entries = tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        VocabularyRepo::list_enabled(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    Ok(vocabulary_corrector::apply_corrections(&text, &entries))
}
```

- [ ] **Step 2: Register module in commands/mod.rs**

Add to `src-tauri/src/commands/mod.rs` after `pub mod transcription;`:

```rust
pub mod vocabulary;
```

- [ ] **Step 3: Register commands in lib.rs**

Add to the `generate_handler!` macro in `src-tauri/src/lib.rs` (after the `commands::logging::frontend_log,` line, before the closing `]`):

```rust
            commands::vocabulary::list_vocabulary_entries,
            commands::vocabulary::add_vocabulary_entry,
            commands::vocabulary::update_vocabulary_entry,
            commands::vocabulary::delete_vocabulary_entry,
            commands::vocabulary::delete_all_vocabulary_entries,
            commands::vocabulary::get_vocabulary_count,
            commands::vocabulary::import_vocabulary_json,
            commands::vocabulary::export_vocabulary_json,
            commands::vocabulary::test_vocabulary_correction,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: success

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/vocabulary.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(commands): add vocabulary CRUD, import/export, and test commands"
```

---

## Task 7: Pipeline Integration

**Files:**
- Modify: `src-tauri/src/commands/transcription.rs`

- [ ] **Step 1: Hook vocabulary corrections after STT**

In `src-tauri/src/commands/transcription.rs`, add imports at the top:

```rust
use medical_db::vocabulary::VocabularyRepo;
use medical_db::settings::SettingsRepo;
use medical_processing::vocabulary_corrector;
```

Find the section where the transcript is persisted (around line 263-276). The current code is:

```rust
    // Persist the transcript and mark as Completed — on a blocking thread.
    let db = Arc::clone(&state.db);
    let mut recording = recording;
    recording.transcript = Some(display_text.clone());
```

Replace with:

```rust
    // Apply vocabulary corrections if enabled
    let db_vocab = Arc::clone(&state.db);
    let display_text = tokio::task::spawn_blocking(move || {
        let conn = db_vocab.conn().map_err(|e| e.to_string())?;
        let config = SettingsRepo::load_config(&conn).ok();
        let vocab_enabled = config.map(|c| c.vocabulary_enabled).unwrap_or(true);
        if vocab_enabled {
            let entries = VocabularyRepo::list_enabled(&conn).map_err(|e| e.to_string())?;
            if !entries.is_empty() {
                let result = vocabulary_corrector::apply_corrections(&display_text, &entries);
                if result.total_replacements > 0 {
                    tracing::info!(
                        replacements = result.total_replacements,
                        "Applied vocabulary corrections to transcript"
                    );
                }
                return Ok::<String, String>(result.corrected_text);
            }
        }
        Ok(display_text)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    // Persist the transcript and mark as Completed — on a blocking thread.
    let db = Arc::clone(&state.db);
    let mut recording = recording;
    recording.transcript = Some(display_text.clone());
```

Note: `display_text` is moved into the blocking task and the corrected version is returned, shadowing the original binding.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/transcription.rs
git commit -m "feat(pipeline): apply vocabulary corrections after STT transcription"
```

---

## Task 8: Frontend API Bindings

**Files:**
- Create: `src/lib/api/vocabulary.ts`

- [ ] **Step 1: Create API bindings**

```typescript
// src/lib/api/vocabulary.ts

import { invoke } from '@tauri-apps/api/core';

export interface VocabularyEntry {
  id: string;
  find_text: string;
  replacement: string;
  category: string;
  case_sensitive: boolean;
  priority: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface AppliedCorrection {
  find_text: string;
  replacement: string;
  category: string;
  count: number;
}

export interface CorrectionResult {
  original_text: string;
  corrected_text: string;
  corrections_applied: AppliedCorrection[];
  total_replacements: number;
}

export async function listVocabularyEntries(category?: string): Promise<VocabularyEntry[]> {
  return invoke('list_vocabulary_entries', { category: category ?? null });
}

export async function addVocabularyEntry(
  findText: string,
  replacement: string,
  category?: string,
  caseSensitive?: boolean,
  priority?: number,
  enabled?: boolean,
): Promise<VocabularyEntry> {
  return invoke('add_vocabulary_entry', {
    findText,
    replacement,
    category: category ?? null,
    caseSensitive: caseSensitive ?? null,
    priority: priority ?? null,
    enabled: enabled ?? null,
  });
}

export async function updateVocabularyEntry(
  id: string,
  findText: string,
  replacement: string,
  category?: string,
  caseSensitive?: boolean,
  priority?: number,
  enabled?: boolean,
): Promise<VocabularyEntry> {
  return invoke('update_vocabulary_entry', {
    id,
    findText,
    replacement,
    category: category ?? null,
    caseSensitive: caseSensitive ?? null,
    priority: priority ?? null,
    enabled: enabled ?? null,
  });
}

export async function deleteVocabularyEntry(id: string): Promise<void> {
  return invoke('delete_vocabulary_entry', { id });
}

export async function deleteAllVocabularyEntries(): Promise<number> {
  return invoke('delete_all_vocabulary_entries');
}

export async function getVocabularyCount(): Promise<[number, number]> {
  return invoke('get_vocabulary_count');
}

export async function importVocabularyJson(filePath: string): Promise<number> {
  return invoke('import_vocabulary_json', { filePath });
}

export async function exportVocabularyJson(filePath: string): Promise<number> {
  return invoke('export_vocabulary_json', { filePath });
}

export async function testVocabularyCorrection(text: string): Promise<CorrectionResult> {
  return invoke('test_vocabulary_correction', { text });
}
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/api/vocabulary.ts
git commit -m "feat(frontend): add vocabulary API bindings"
```

---

## Task 9: Vocabulary Dialog Component

**Files:**
- Create: `src/lib/components/VocabularyDialog.svelte`

- [ ] **Step 1: Create the dialog component**

```svelte
<!-- src/lib/components/VocabularyDialog.svelte -->
<script lang="ts">
  import {
    listVocabularyEntries,
    addVocabularyEntry,
    updateVocabularyEntry,
    deleteVocabularyEntry,
    deleteAllVocabularyEntries,
    testVocabularyCorrection,
    type VocabularyEntry,
    type CorrectionResult,
  } from '../api/vocabulary';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let entries = $state<VocabularyEntry[]>([]);
  let loading = $state(false);
  let filterCategory = $state('all');
  let searchText = $state('');

  // Add/Edit form
  let editing = $state<VocabularyEntry | null>(null);
  let showForm = $state(false);
  let formFind = $state('');
  let formReplace = $state('');
  let formCategory = $state('general');
  let formCaseSensitive = $state(false);
  let formPriority = $state(0);
  let formEnabled = $state(true);
  let formError = $state('');

  // Test area
  let testInput = $state('');
  let testResult = $state<CorrectionResult | null>(null);

  const CATEGORIES = [
    { value: 'general', label: 'General' },
    { value: 'doctor_names', label: 'Doctor Names' },
    { value: 'medication_names', label: 'Medications' },
    { value: 'medical_terminology', label: 'Terminology' },
    { value: 'abbreviations', label: 'Abbreviations' },
  ];

  function categoryLabel(value: string): string {
    return CATEGORIES.find((c) => c.value === value)?.label ?? value;
  }

  async function loadEntries() {
    loading = true;
    try {
      const cat = filterCategory === 'all' ? undefined : filterCategory;
      entries = await listVocabularyEntries(cat);
    } catch (err) {
      console.error('Failed to load vocabulary entries:', err);
    } finally {
      loading = false;
    }
  }

  function filteredEntries(): VocabularyEntry[] {
    if (!searchText.trim()) return entries;
    const q = searchText.toLowerCase();
    return entries.filter(
      (e) =>
        e.find_text.toLowerCase().includes(q) ||
        e.replacement.toLowerCase().includes(q),
    );
  }

  function openAddForm() {
    editing = null;
    formFind = '';
    formReplace = '';
    formCategory = 'general';
    formCaseSensitive = false;
    formPriority = 0;
    formEnabled = true;
    formError = '';
    showForm = true;
  }

  function openEditForm(entry: VocabularyEntry) {
    editing = entry;
    formFind = entry.find_text;
    formReplace = entry.replacement;
    formCategory = entry.category;
    formCaseSensitive = entry.case_sensitive;
    formPriority = entry.priority;
    formEnabled = entry.enabled;
    formError = '';
    showForm = true;
  }

  function closeForm() {
    showForm = false;
    editing = null;
    formError = '';
  }

  async function handleSave() {
    if (!formFind.trim() || !formReplace.trim()) {
      formError = 'Find and replacement text are required.';
      return;
    }
    try {
      if (editing) {
        await updateVocabularyEntry(
          editing.id,
          formFind.trim(),
          formReplace.trim(),
          formCategory,
          formCaseSensitive,
          formPriority,
          formEnabled,
        );
      } else {
        await addVocabularyEntry(
          formFind.trim(),
          formReplace.trim(),
          formCategory,
          formCaseSensitive,
          formPriority,
          formEnabled,
        );
      }
      closeForm();
      await loadEntries();
    } catch (err: any) {
      formError = err?.toString() || 'Failed to save entry.';
    }
  }

  async function handleDelete(entry: VocabularyEntry) {
    if (!confirm(`Delete correction "${entry.find_text}" → "${entry.replacement}"?`)) return;
    try {
      await deleteVocabularyEntry(entry.id);
      await loadEntries();
    } catch (err) {
      console.error('Failed to delete entry:', err);
    }
  }

  async function handleDeleteAll() {
    if (!confirm(`Delete ALL ${entries.length} vocabulary entries? This cannot be undone.`)) return;
    try {
      await deleteAllVocabularyEntries();
      await loadEntries();
    } catch (err) {
      console.error('Failed to delete all entries:', err);
    }
  }

  async function handleToggleEnabled(entry: VocabularyEntry) {
    try {
      await updateVocabularyEntry(
        entry.id,
        entry.find_text,
        entry.replacement,
        entry.category,
        entry.case_sensitive,
        entry.priority,
        !entry.enabled,
      );
      await loadEntries();
    } catch (err) {
      console.error('Failed to toggle entry:', err);
    }
  }

  async function handleTest() {
    if (!testInput.trim()) return;
    try {
      testResult = await testVocabularyCorrection(testInput);
    } catch (err) {
      console.error('Test failed:', err);
    }
  }

  $effect(() => {
    if (open) {
      loadEntries();
      testResult = null;
    }
  });

  $effect(() => {
    filterCategory;
    if (open) loadEntries();
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="vocab-overlay" onclick={onclose}>
    <div class="vocab-dialog" onclick={(e) => e.stopPropagation()}>
      <div class="vocab-header">
        <h2>Manage Vocabulary</h2>
        <button class="btn-close" onclick={onclose}>&times;</button>
      </div>

      <div class="vocab-toolbar">
        <select bind:value={filterCategory}>
          <option value="all">All Categories</option>
          {#each CATEGORIES as cat}
            <option value={cat.value}>{cat.label}</option>
          {/each}
        </select>
        <input
          type="text"
          placeholder="Search..."
          bind:value={searchText}
        />
        <button class="btn-add" onclick={openAddForm}>+ Add Entry</button>
      </div>

      {#if showForm}
        <div class="vocab-form">
          <h3>{editing ? 'Edit' : 'Add'} Entry</h3>
          {#if formError}
            <div class="form-error">{formError}</div>
          {/if}
          <div class="form-row">
            <label>
              Find Text
              <input type="text" bind:value={formFind} placeholder="e.g. htn" />
            </label>
            <label>
              Replacement
              <input type="text" bind:value={formReplace} placeholder="e.g. hypertension" />
            </label>
          </div>
          <div class="form-row">
            <label>
              Category
              <select bind:value={formCategory}>
                {#each CATEGORIES as cat}
                  <option value={cat.value}>{cat.label}</option>
                {/each}
              </select>
            </label>
            <label>
              Priority
              <input type="number" bind:value={formPriority} min="0" max="100" />
            </label>
          </div>
          <div class="form-row">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={formCaseSensitive} />
              Case Sensitive
            </label>
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={formEnabled} />
              Enabled
            </label>
          </div>
          <div class="form-actions">
            <button class="btn-save" onclick={handleSave}>Save</button>
            <button class="btn-cancel" onclick={closeForm}>Cancel</button>
          </div>
        </div>
      {/if}

      <div class="vocab-table-wrap">
        {#if loading}
          <p class="loading-text">Loading...</p>
        {:else if filteredEntries().length === 0}
          <p class="empty-text">No vocabulary entries found.</p>
        {:else}
          <table class="vocab-table">
            <thead>
              <tr>
                <th>Find</th>
                <th>Replace With</th>
                <th>Category</th>
                <th>Enabled</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredEntries() as entry (entry.id)}
                <tr class:disabled={!entry.enabled}>
                  <td class="mono">{entry.find_text}</td>
                  <td>{entry.replacement}</td>
                  <td>{categoryLabel(entry.category)}</td>
                  <td>
                    <input
                      type="checkbox"
                      checked={entry.enabled}
                      onchange={() => handleToggleEnabled(entry)}
                    />
                  </td>
                  <td class="actions">
                    <button class="btn-edit" onclick={() => openEditForm(entry)}>Edit</button>
                    <button class="btn-delete" onclick={() => handleDelete(entry)}>Del</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

      <div class="vocab-test">
        <h3>Test Corrections</h3>
        <textarea
          bind:value={testInput}
          placeholder="Paste sample text to test corrections..."
          rows="3"
        ></textarea>
        <button class="btn-test" onclick={handleTest} disabled={!testInput.trim()}>
          Test
        </button>
        {#if testResult}
          <div class="test-result">
            <strong>{testResult.total_replacements} replacement{testResult.total_replacements !== 1 ? 's' : ''}</strong>
            <pre>{testResult.corrected_text}</pre>
          </div>
        {/if}
      </div>

      <div class="vocab-footer">
        <button class="btn-delete-all" onclick={handleDeleteAll} disabled={entries.length === 0}>
          Delete All ({entries.length})
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .vocab-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .vocab-dialog {
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    border-radius: 8px;
    width: 90vw;
    max-width: 800px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .vocab-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color, #333);
  }
  .vocab-header h2 { margin: 0; font-size: 1.2rem; }
  .btn-close {
    background: none;
    border: none;
    color: var(--text-secondary, #aaa);
    font-size: 1.5rem;
    cursor: pointer;
  }
  .vocab-toolbar {
    display: flex;
    gap: 8px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color, #333);
  }
  .vocab-toolbar select,
  .vocab-toolbar input {
    padding: 6px 10px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
  }
  .vocab-toolbar input { flex: 1; }
  .btn-add {
    padding: 6px 14px;
    border-radius: 4px;
    border: none;
    background: var(--accent-color, #4a9eff);
    color: white;
    cursor: pointer;
    white-space: nowrap;
  }
  .vocab-form {
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color, #333);
    background: var(--bg-primary, #111);
  }
  .vocab-form h3 { margin: 0 0 8px; font-size: 0.95rem; }
  .form-error { color: #ff6b6b; margin-bottom: 8px; font-size: 0.85rem; }
  .form-row { display: flex; gap: 12px; margin-bottom: 8px; }
  .form-row label { flex: 1; display: flex; flex-direction: column; gap: 4px; font-size: 0.85rem; }
  .form-row input[type="text"],
  .form-row input[type="number"],
  .form-row select {
    padding: 6px 8px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
  }
  .checkbox-label { flex-direction: row !important; align-items: center; gap: 6px !important; }
  .form-actions { display: flex; gap: 8px; margin-top: 8px; }
  .btn-save { padding: 6px 16px; border-radius: 4px; border: none; background: var(--accent-color, #4a9eff); color: white; cursor: pointer; }
  .btn-cancel { padding: 6px 16px; border-radius: 4px; border: 1px solid var(--border-color, #444); background: transparent; color: var(--text-primary, #e0e0e0); cursor: pointer; }
  .vocab-table-wrap {
    flex: 1;
    overflow-y: auto;
    padding: 0 20px;
  }
  .loading-text, .empty-text { text-align: center; color: var(--text-secondary, #888); padding: 24px; }
  .vocab-table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
  .vocab-table th { text-align: left; padding: 8px 6px; border-bottom: 1px solid var(--border-color, #333); color: var(--text-secondary, #888); font-weight: 500; }
  .vocab-table td { padding: 6px; border-bottom: 1px solid var(--border-color, #222); }
  .vocab-table tr.disabled { opacity: 0.5; }
  .mono { font-family: monospace; }
  .actions { display: flex; gap: 4px; }
  .btn-edit, .btn-delete { padding: 3px 8px; border-radius: 3px; border: 1px solid var(--border-color, #444); background: transparent; color: var(--text-secondary, #aaa); cursor: pointer; font-size: 0.8rem; }
  .btn-delete { color: #ff6b6b; border-color: #ff6b6b44; }
  .vocab-test {
    padding: 12px 20px;
    border-top: 1px solid var(--border-color, #333);
  }
  .vocab-test h3 { margin: 0 0 8px; font-size: 0.95rem; }
  .vocab-test textarea {
    width: 100%;
    padding: 8px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
    resize: vertical;
    font-family: inherit;
  }
  .btn-test { margin-top: 6px; padding: 6px 14px; border-radius: 4px; border: none; background: var(--accent-color, #4a9eff); color: white; cursor: pointer; }
  .test-result { margin-top: 8px; }
  .test-result pre { background: var(--bg-primary, #111); padding: 8px; border-radius: 4px; white-space: pre-wrap; font-size: 0.85rem; margin-top: 4px; }
  .vocab-footer {
    padding: 12px 20px;
    border-top: 1px solid var(--border-color, #333);
    display: flex;
    justify-content: flex-end;
  }
  .btn-delete-all { padding: 6px 14px; border-radius: 4px; border: 1px solid #ff6b6b44; background: transparent; color: #ff6b6b; cursor: pointer; }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/components/VocabularyDialog.svelte
git commit -m "feat(frontend): add vocabulary management dialog component"
```

---

## Task 10: Settings UI Integration

**Files:**
- Modify: `src/lib/components/SettingsContent.svelte`

- [ ] **Step 1: Add imports and state**

At the top of `SettingsContent.svelte`, add the imports:

```typescript
import VocabularyDialog from './VocabularyDialog.svelte';
import { getVocabularyCount, importVocabularyJson, exportVocabularyJson } from '../api/vocabulary';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
```

Add state variables (alongside existing state declarations, around line 48):

```typescript
let vocabDialogOpen = $state(false);
let vocabCount = $state<[number, number]>([0, 0]);
```

Add a function to load vocabulary count (alongside existing handler functions):

```typescript
async function loadVocabCount() {
    try {
        vocabCount = await getVocabularyCount();
    } catch (err) {
        console.error('Failed to load vocabulary count:', err);
    }
}

async function handleImportVocabulary() {
    const selected = await openDialog({
        multiple: false,
        title: 'Import Vocabulary JSON',
        filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
        try {
            const count = await importVocabularyJson(selected as string);
            alert(`Imported ${count} vocabulary entries.`);
            await loadVocabCount();
        } catch (err: any) {
            alert(`Import failed: ${err}`);
        }
    }
}

async function handleExportVocabulary() {
    const selected = await saveDialog({
        title: 'Export Vocabulary JSON',
        defaultPath: 'vocabulary.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
        try {
            const count = await exportVocabularyJson(selected);
            alert(`Exported ${count} vocabulary entries.`);
        } catch (err: any) {
            alert(`Export failed: ${err}`);
        }
    }
}

function handleVocabDialogClose() {
    vocabDialogOpen = false;
    loadVocabCount();
}
```

Add `loadVocabCount()` to the `onMount` block:

```typescript
loadVocabCount(),
```

- [ ] **Step 2: Add vocabulary section to General settings tab**

In the `general` section of the template (after the "Recording Storage Folder" `</div>` and before the closing `</section>` tag around line 370), add:

```svelte
        <h3 class="section-title" style="margin-top: 24px">Custom Vocabulary</h3>
        <p class="section-desc">Automatically correct words in transcripts after speech-to-text.</p>

        <div class="form-group">
          <label class="toggle-label">
            <input
              type="checkbox"
              checked={$settings.vocabulary_enabled}
              onchange={() => settings.updateField('vocabulary_enabled', !$settings.vocabulary_enabled)}
            />
            <span>Enable vocabulary corrections</span>
          </label>
        </div>

        <div class="form-group">
          <span class="form-label">
            {vocabCount[0]} entries ({vocabCount[1]} enabled)
          </span>
          <div class="vocab-buttons">
            <button class="btn-browse" onclick={() => { vocabDialogOpen = true; }}>
              Manage Vocabulary
            </button>
            <button class="btn-browse" onclick={handleImportVocabulary}>
              Import JSON
            </button>
            <button class="btn-browse" onclick={handleExportVocabulary}>
              Export JSON
            </button>
          </div>
        </div>
```

At the very end of the component template (before `</div>` closing tag), add the dialog:

```svelte
<VocabularyDialog open={vocabDialogOpen} onclose={handleVocabDialogClose} />
```

Add the CSS for the button row:

```css
.vocab-buttons {
    display: flex;
    gap: 8px;
    margin-top: 4px;
}
```

- [ ] **Step 3: Verify the app builds and opens**

Run: `npm run tauri dev`
Expected: App opens. Navigate to Settings → General. The "Custom Vocabulary" section should appear at the bottom with toggle, count, and buttons. Clicking "Manage Vocabulary" opens the dialog.

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/SettingsContent.svelte
git commit -m "feat(frontend): add vocabulary settings section with dialog integration"
```

---

## Task 11: End-to-End Verification

- [ ] **Step 1: Run all Rust tests**

Run: `cargo test`
Expected: all tests pass, including new vocabulary_corrector tests and migration tests

- [ ] **Step 2: Start the app and manually test the full flow**

Run: `npm run tauri dev`

Test sequence:
1. Settings → General → verify "Custom Vocabulary" section appears
2. Click "Manage Vocabulary" → verify dialog opens
3. Click "+ Add Entry" → add `htn` → `hypertension` (category: Abbreviations) → Save
4. Verify entry appears in table
5. Click "Edit" → change replacement to "Hypertension" → Save
6. Toggle the enabled checkbox → verify it toggles
7. In the Test area: type "patient has htn and sob" → click Test → verify "htn" is replaced
8. Close dialog → verify count shows "1 entries (1 enabled)"
9. Click "Import JSON" → import a vocabulary.json from the Python app (if available)
10. Record/transcribe audio → verify corrections are applied to the transcript

- [ ] **Step 3: Final commit if any cleanup needed**

```bash
git add -A
git commit -m "feat: complete custom vocabulary correction system"
```
