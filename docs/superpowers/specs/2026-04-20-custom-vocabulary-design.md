# Custom Vocabulary Correction System

**Date:** 2026-04-20
**Status:** Approved
**Port source:** `~/Development/Medical-Assistant/src/utils/vocabulary_corrector.py` and `~/Development/Medical-Assistant/src/managers/vocabulary_manager.py`

## Overview

Port the custom vocabulary correction system from the Python Medical-Assistant. The system applies configurable find/replace rules to transcripts immediately after speech-to-text, so the corrected text flows into SOAP generation and all downstream processing.

## Scope

**In scope:**
- Correction engine with word-boundary matching, case sensitivity, priority ordering
- Categories: doctor_names, medication_names, medical_terminology, abbreviations, general
- Database persistence (new migration + repo)
- Tauri commands for CRUD, import/export, and test preview
- JSON import compatible with Python app's `vocabulary.json` format
- JSON export
- Settings UI section with modal management dialog
- `vocabulary_enabled` toggle in AppConfig
- Pipeline hook after STT in transcription command
- Unit tests for correction engine

**Out of scope:**
- Specialty filtering (not used)
- CSV import/export
- Default correction sets (user imports or creates their own)

## Data Model

### Types (`crates/core/src/types/vocabulary.rs`)

```rust
pub enum VocabularyCategory {
    DoctorNames,
    MedicationNames,
    MedicalTerminology,
    Abbreviations,
    General,
}

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

pub struct CorrectionResult {
    pub original_text: String,
    pub corrected_text: String,
    pub corrections_applied: Vec<AppliedCorrection>,
    pub total_replacements: u32,
}

pub struct AppliedCorrection {
    pub find_text: String,
    pub replacement: String,
    pub category: VocabularyCategory,
    pub count: u32,
}
```

### Database (`crates/db/src/migrations/m003_vocabulary.rs`)

```sql
CREATE TABLE IF NOT EXISTS vocabulary_entries (
    id TEXT PRIMARY KEY NOT NULL,
    find_text TEXT NOT NULL,
    replacement TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'general',
    case_sensitive INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_vocabulary_find_text ON vocabulary_entries(find_text);
```

Unique index on `find_text` prevents duplicate rules.

### Repository (`crates/db/src/vocabulary.rs`)

`VocabularyRepo` with methods:
- `list_all(conn) -> Vec<VocabularyEntry>`
- `list_enabled(conn) -> Vec<VocabularyEntry>` — only enabled entries, used by pipeline
- `list_by_category(conn, category) -> Vec<VocabularyEntry>`
- `get_by_id(conn, id) -> VocabularyEntry`
- `insert(conn, entry) -> VocabularyEntry`
- `update(conn, entry) -> VocabularyEntry`
- `delete(conn, id) -> ()`
- `delete_all(conn) -> u32` — returns count deleted
- `count(conn) -> (total, enabled)` — for UI summary

## Correction Engine (`crates/processing/src/vocabulary_corrector.rs`)

### `apply_corrections(text: &str, entries: &[VocabularyEntry]) -> CorrectionResult`

1. Return early if text is empty or entries is empty
2. Sort entries by priority descending, then by `find_text` length descending (longer matches first to prevent partial replacements)
3. For each enabled entry:
   - Build regex: `\b{escaped_find_text}\b` with `(?i)` flag if not case-sensitive
   - Count matches via `find_iter`
   - Apply `replace_all`
   - Record in `corrections_applied` if any matches found
4. Return `CorrectionResult`

### Pattern caching

`HashMap<(String, bool), Regex>` keyed by `(find_text, case_sensitive)`. Compiled once per unique pattern. The cache lives on a `VocabularyCorrector` struct that is created per-call (entries are loaded from DB each time, so caching across calls adds complexity for little gain given the infrequent call pattern — once per transcription).

If performance becomes a concern (very large vocabulary sets), the cache can be elevated to `AppState` and invalidated on CRUD operations.

## Pipeline Integration (`src-tauri/src/commands/transcription.rs`)

After STT completes and before the transcript is persisted:

```
STT produces transcript
  → load vocabulary_enabled from settings
  → if enabled, load enabled entries from DB
  → apply_corrections(transcript, entries)
  → log corrections applied count
  → store corrected transcript on recording
  → persist recording to DB
```

The corrected transcript replaces the raw STT output. Downstream consumers (SOAP generator, referral, letter, synopsis) all read the already-corrected transcript.

## Tauri Commands (`src-tauri/src/commands/vocabulary.rs`)

| Command | Args | Returns |
|---------|------|---------|
| `list_vocabulary_entries` | `category: Option<String>` | `Vec<VocabularyEntry>` |
| `add_vocabulary_entry` | `find_text, replacement, category, case_sensitive, priority, enabled` | `VocabularyEntry` |
| `update_vocabulary_entry` | `id, find_text, replacement, category, case_sensitive, priority, enabled` | `VocabularyEntry` |
| `delete_vocabulary_entry` | `id` | `()` |
| `delete_all_vocabulary_entries` | — | `u32` (count deleted) |
| `import_vocabulary_json` | `file_path: String` | `u32` (count imported) |
| `export_vocabulary_json` | `file_path: String` | `u32` (count exported) |
| `test_vocabulary_correction` | `text: String` | `CorrectionResult` |

### Import format compatibility

The Python app's `vocabulary.json`:
```json
{
  "version": "1.0",
  "corrections": [
    {
      "find_text": "htn",
      "replacement": "hypertension",
      "category": "abbreviations",
      "specialty": "cardiology",
      "case_sensitive": false,
      "priority": 0,
      "enabled": true
    }
  ]
}
```

The importer reads this format directly, ignoring the `specialty` field. On import, existing entries with the same `find_text` are updated (upsert behavior).

### Export format

Same JSON structure as above (without `specialty`), compatible for re-import.

## Settings Integration

### Backend (`crates/core/src/types/settings.rs`)

Add to `AppConfig`:
```rust
#[serde(default = "default_vocabulary_enabled")]
pub vocabulary_enabled: bool,
```

Default: `true`.

### Frontend (`src/lib/stores/settings.ts`)

Add `vocabulary_enabled: boolean` to the settings store with default `true`.

## Frontend UI

### Settings section (in `SettingsContent.svelte`)

New "Custom Vocabulary" section containing:
- Toggle: "Enable vocabulary corrections" bound to `vocabulary_enabled`
- Summary text: "X entries (Y enabled)"
- Button: "Manage Vocabulary" → opens VocabularyDialog
- Button: "Import JSON" → Tauri file dialog → calls `import_vocabulary_json`
- Button: "Export JSON" → Tauri save dialog → calls `export_vocabulary_json`

### Modal dialog (`VocabularyDialog.svelte`)

- **Header**: Title + close button
- **Toolbar**: Category filter dropdown (All / Doctor Names / Medications / Terminology / Abbreviations / General), search text input, "Add Entry" button
- **Table**: Columns — Find Text, Replacement, Category, Enabled (toggle), Actions (Edit, Delete)
- **Add/Edit form** (inline or sub-modal): find_text input, replacement input, category dropdown, case_sensitive checkbox, priority number input, enabled checkbox, Save/Cancel buttons
- **Delete confirmation**: Simple confirm dialog before delete
- **Test area** (bottom): textarea for sample text, "Test" button, shows corrected output with highlighted changes
- **Delete All** button with confirmation

Follows existing app component patterns and styling.

## Testing

Unit tests in `crates/processing/src/vocabulary_corrector.rs`:
- Empty text returns unchanged
- Empty entries returns unchanged
- Basic replacement works
- Word boundary prevents partial matches (e.g., "htn" doesn't match "washington")
- Case-insensitive matching
- Case-sensitive matching
- Priority ordering (higher priority applied first)
- Longer matches applied before shorter ones at same priority
- Disabled entries skipped
- Multiple replacements in same text
- CorrectionResult tracks all applied corrections with counts

## Files to Create

| File | Description |
|------|-------------|
| `crates/core/src/types/vocabulary.rs` | VocabularyEntry, CorrectionResult types |
| `crates/db/src/migrations/m003_vocabulary.rs` | CREATE TABLE migration |
| `crates/db/src/vocabulary.rs` | VocabularyRepo CRUD |
| `crates/processing/src/vocabulary_corrector.rs` | Correction engine + tests |
| `src-tauri/src/commands/vocabulary.rs` | Tauri commands |
| `src/lib/api/vocabulary.ts` | Frontend API bindings |
| `src/lib/components/VocabularyDialog.svelte` | Modal dialog component |

## Files to Modify

| File | Change |
|------|--------|
| `crates/core/src/types/mod.rs` | Add `pub mod vocabulary` |
| `crates/processing/src/lib.rs` | Add `pub mod vocabulary_corrector` |
| `crates/db/src/lib.rs` | Register m003 migration, export vocabulary repo |
| `src-tauri/src/commands/mod.rs` | Add `pub mod vocabulary` |
| `src-tauri/src/main.rs` or `lib.rs` | Register vocabulary commands |
| `src-tauri/src/commands/transcription.rs` | Hook corrections after STT |
| `crates/core/src/types/settings.rs` | Add `vocabulary_enabled` field |
| `src/lib/types/index.ts` | Add TypeScript types |
| `src/lib/stores/settings.ts` | Add `vocabulary_enabled` |
| `src/lib/components/SettingsContent.svelte` | Add vocabulary section |
