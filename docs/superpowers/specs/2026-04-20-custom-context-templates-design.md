# Custom Context Templates

**Date:** 2026-04-20
**Status:** Approved
**Port source:** `~/Development/Medical-Assistant/src/settings/settings_models.py` (lines 346–347) and `~/Development/Medical-Assistant/src/ui/components/context_panel.py`

## Overview

Port the custom context templates feature from the Python Medical-Assistant. A template is a named snippet of clinical context text (visit type, demographics, chief complaint, etc.) that the user saves once and reuses across recordings. Selecting a template appends its body to the Patient Context textarea on the Record tab, so the same boilerplate can precede a new recording's typed notes.

## Scope

**In scope:**
- Simple data model: `name` + `body`, stored inline in `AppConfig` (no DB table)
- Apply behavior: append to `contextText` with `\n\n` separator (no leading blank line if field is empty)
- "Save current as template" action on the Record tab
- Manage dialog in Settings (vocabulary-style): list, add, edit, delete
- Import/export JSON, with importer accepting three shapes for flexibility
- Dropdown picker inside the Patient Context accordion
- Name uniqueness with confirm-overwrite flow
- Alphabetical ordering

**Out of scope:**
- Placeholder/variable substitution (`{patient_age}`, etc.)
- Built-in/preloaded templates — user starts with an empty list
- Favorites or starring
- Agent-workflow template injection (Python has this for chains; not used in our app)
- Diagnostic-dialog templates (a separate system in the Python app)

## Data Model

### Type (`crates/core/src/types/settings.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextTemplate {
    pub name: String,
    pub body: String,
}
```

The `name` is the unique key. No UUID, no timestamps — matches the Python app's flat-map persistence. Duplicate names are rejected by the management logic.

### AppConfig field

```rust
#[serde(default)]
pub custom_context_templates: Vec<ContextTemplate>,
```

Default: empty vec. Stored as a `Vec` (not `HashMap`) to preserve a stable sort and make import/export trivially deterministic.

## Behavior

### Apply (Record tab)

When the user picks a template from the "Apply template…" dropdown:

1. Look up the template by name.
2. If `contextText` is empty: set `contextText = template.body`.
3. Otherwise: set `contextText = contextText.trimEnd() + "\n\n" + template.body`.
4. Reset the dropdown back to the placeholder ("Apply template…") so the same template can be applied again.

### Save current as template (Record tab)

Click "Save as template" button inside the Patient Context accordion:

1. If `contextText` is empty or whitespace-only, disable the button.
2. Open a small prompt modal with a Name input (required) and read-only body preview (the current `contextText`).
3. On Save:
   - Validate name is non-empty and trimmed.
   - If name already exists, show inline "Overwrite existing template?" confirm.
   - On confirm or unique name, call the Tauri command to upsert.
4. Close modal, show a brief success indicator; the dropdown's options refresh immediately.

### Manage (Settings tab → modal)

Follow VocabularyDialog's structure:

- **Header**: "Manage Context Templates" + close button.
- **Toolbar**: "Add Template" button, search input (filters by name or body substring).
- **List**: table with columns — Name, Body preview (truncated), Actions (Edit, Delete).
- **Add/Edit form**: Name input + Body textarea + Save/Cancel.
- **Import JSON / Export JSON** buttons.
- **Delete confirmation**: inline confirm before delete.
- No Delete All button (low value given most users will have < 20 templates).

## UI Integration

### Record tab (`src/lib/pages/RecordTab.svelte`)

Inside the existing Patient Context accordion body, above the textarea:

```
┌─ ▼ Patient Context (optional) ──────────────────────┐
│  [ Apply template…  ▾ ]   [ Save as template ]      │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │ (textarea bound to contextText)              │   │
│  └──────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────┘
```

- The dropdown is disabled (with placeholder "No templates saved") when there are none.
- The Save button is disabled when `contextText` is empty/whitespace.
- Both controls live on a single horizontal row above the textarea, with `gap: 8px`.

### Settings tab (`src/lib/components/SettingsContent.svelte`)

Add a "Context Templates" section in the General or Processing group (same region as Custom Vocabulary). Shows:

- Summary line: "N templates saved"
- "Manage Templates" button opens `ContextTemplateDialog`

## Tauri Commands (`src-tauri/src/commands/context_templates.rs`)

All commands mutate `state.settings` and persist via the existing settings-save path (same pattern as vocabulary and other settings fields).

| Command | Args | Returns |
|---------|------|---------|
| `list_context_templates` | — | `Vec<ContextTemplate>` (alphabetical by name) |
| `upsert_context_template` | `name: String, body: String` | `ContextTemplate` |
| `delete_context_template` | `name: String` | `()` |
| `rename_context_template` | `old_name: String, new_name: String` | `ContextTemplate` |
| `import_context_templates_json` | `file_path: String` | `u32` (count imported) |
| `export_context_templates_json` | `file_path: String` | `u32` (count exported) |

`upsert_context_template` replaces the existing entry if the name matches (case-sensitive), otherwise appends. `rename_context_template` errors if `new_name` already exists and is different from `old_name`. List is always returned alphabetically (case-insensitive compare).

## Import / Export

### Export format (canonical)

```json
{
  "custom_context_templates": {
    "Follow-up": "Follow-up visit for ongoing condition.",
    "Telehealth": "Telehealth consultation via video call."
  }
}
```

This matches the shape the Python app writes into its `settings.json`, so exports can be round-tripped into the Python app if needed.

### Import — accepted shapes

The importer accepts any of the following and normalises to `Vec<ContextTemplate>`:

1. **Wrapped** (canonical): `{ "custom_context_templates": { "Name": "body", ... } }`
2. **Bare dict**: `{ "Name": "body", ... }`
3. **Bare array**: `[ { "name": "Name", "body": "body" }, ... ]`

Existing entries with the same name are overwritten (upsert behaviour). Empty names or empty bodies are skipped with a warning in the log. Implemented via `serde(untagged)` enum, mirroring the vocabulary importer.

## Settings Integration

### Backend (`crates/core/src/types/settings.rs`)

Add to `AppConfig`:

```rust
#[serde(default)]
pub custom_context_templates: Vec<ContextTemplate>,
```

### Frontend types (`src/lib/types/index.ts`)

```typescript
export interface ContextTemplate {
  name: string;
  body: string;
}

// In AppConfig:
custom_context_templates: ContextTemplate[];
```

### Frontend store (`src/lib/stores/settings.ts`)

Default: `custom_context_templates: []`.

## Testing

Unit tests in Rust:
- Serde round-trip of `ContextTemplate` and `AppConfig.custom_context_templates`
- Upsert replaces existing by name, appends new
- Rename blocks on collision
- Import accepts all three shapes
- Import upserts (overwrites existing by name)
- Import skips empty name / empty body
- Export produces canonical wrapped form
- Export → import round-trip preserves all entries

Manual QA:
- Record tab: dropdown lists templates alphabetically
- Apply into empty field: body appears as-is (no leading newline)
- Apply into non-empty field: body appears after `\n\n`
- Save as template: prompt works, overwrite confirm triggers when name collides
- Settings manage dialog: add/edit/delete all flow cleanly
- Import from Python app's `settings.json` (bare dict form) works

## Files to Create

| File | Description |
|------|-------------|
| `src-tauri/src/commands/context_templates.rs` | 6 Tauri commands (list/upsert/delete/rename/import/export) |
| `src/lib/api/contextTemplates.ts` | Frontend API bindings |
| `src/lib/components/ContextTemplateDialog.svelte` | Management modal |

## Files to Modify

| File | Change |
|------|--------|
| `crates/core/src/types/settings.rs` | Add `ContextTemplate` struct + `custom_context_templates` field on `AppConfig` |
| `src-tauri/src/commands/mod.rs` | Add `pub mod context_templates` |
| `src-tauri/src/lib.rs` | Register 6 context-template commands in `generate_handler!` |
| `src/lib/types/index.ts` | Add `ContextTemplate` type + `custom_context_templates` on `AppConfig` |
| `src/lib/stores/settings.ts` | Add `custom_context_templates: []` default |
| `src/lib/pages/RecordTab.svelte` | Add picker dropdown + Save-as-template button + apply/save logic + save-modal |
| `src/lib/components/SettingsContent.svelte` | Add Context Templates section with Manage button + dialog integration |
