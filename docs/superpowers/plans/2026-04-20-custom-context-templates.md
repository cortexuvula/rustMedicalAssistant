# Custom Context Templates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the Python Medical-Assistant's custom context template feature — reusable named snippets of clinical context text the user can apply to the Patient Context field on the Record tab.

**Architecture:** `ContextTemplate { name, body }` lives inline in `AppConfig.custom_context_templates` (no DB table). Pure Rust helpers manage a `Vec<ContextTemplate>`; Tauri commands wrap helpers by loading AppConfig via `SettingsRepo`, mutating, and saving. Frontend adds a picker in the Patient Context accordion on the Record tab and a vocabulary-style management dialog in Settings.

**Tech Stack:** Rust (medical-core + src-tauri), Svelte 5 runes, Tauri v2.

**Reference spec:** `docs/superpowers/specs/2026-04-20-custom-context-templates-design.md`

---

### Task 1: Add `ContextTemplate` type and `AppConfig` field

**Files:**
- Modify: `crates/core/src/types/settings.rs`

- [ ] **Step 1: Write failing test**

Add to the existing `#[cfg(test)] mod tests` block in `crates/core/src/types/settings.rs`:

```rust
#[test]
fn context_templates_default_empty() {
    let config = AppConfig::default();
    assert!(config.custom_context_templates.is_empty());
}

#[test]
fn context_template_round_trip() {
    let mut config = AppConfig::default();
    config.custom_context_templates = vec![
        ContextTemplate { name: "Follow-up".into(), body: "Follow-up visit.".into() },
        ContextTemplate { name: "Telehealth".into(), body: "Video consult.".into() },
    ];
    let json = serde_json::to_string(&config).unwrap();
    let back: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.custom_context_templates.len(), 2);
    assert_eq!(back.custom_context_templates[0].name, "Follow-up");
    assert_eq!(back.custom_context_templates[1].body, "Video consult.");
}

#[test]
fn context_templates_missing_from_json_defaults_empty() {
    let json = r#"{"ai_provider": "openai"}"#;
    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert!(config.custom_context_templates.is_empty());
}
```

- [ ] **Step 2: Run tests — expect failure**

Run: `cargo test -p medical-core settings::tests::context_templates -- --nocapture`
Expected: compile error — `ContextTemplate` undefined, `custom_context_templates` field missing.

- [ ] **Step 3: Add the `ContextTemplate` struct**

Add near the other structs in `crates/core/src/types/settings.rs` (e.g., just above `SoapNoteSettings`):

```rust
/// A named snippet of clinical context text the user can apply to the
/// Patient Context field at recording time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextTemplate {
    pub name: String,
    pub body: String,
}
```

- [ ] **Step 4: Add the `AppConfig` field**

Add this field to the `AppConfig` struct (place it alongside the other processing-related fields like `vocabulary_enabled`):

```rust
    #[serde(default)]
    pub custom_context_templates: Vec<ContextTemplate>,
```

- [ ] **Step 5: Run tests — expect pass**

Run: `cargo test -p medical-core`
Expected: all tests pass, including the three new ones.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/types/settings.rs
git commit -m "feat(core): add ContextTemplate type and custom_context_templates field"
```

---

### Task 2: Pure template helpers (upsert, rename, delete, sort, import parse)

**Files:**
- Create: `src-tauri/src/commands/context_templates.rs`
- Modify: `src-tauri/src/commands/mod.rs`

The Tauri commands in Task 3 will be thin wrappers around these helpers, which we can unit test without AppState.

- [ ] **Step 1: Register the module**

In `src-tauri/src/commands/mod.rs`, add:

```rust
pub mod context_templates;
```

- [ ] **Step 2: Create the helpers file with failing tests**

Create `src-tauri/src/commands/context_templates.rs` containing the helpers and their tests:

```rust
use std::collections::BTreeMap;

use serde::Deserialize;

use medical_core::types::settings::ContextTemplate;

/// Sort templates alphabetically by name (case-insensitive).
pub fn sort_templates(templates: &mut Vec<ContextTemplate>) {
    templates.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
}

/// Insert or replace a template in-place.  Case-sensitive name match.
/// The vec is re-sorted afterwards.
pub fn upsert_into(
    templates: &mut Vec<ContextTemplate>,
    name: String,
    body: String,
) -> ContextTemplate {
    let entry = ContextTemplate { name: name.clone(), body: body.clone() };
    if let Some(existing) = templates.iter_mut().find(|t| t.name == name) {
        existing.body = body;
    } else {
        templates.push(entry.clone());
    }
    sort_templates(templates);
    entry
}

/// Rename a template.  Errors if `old_name` does not exist, or if `new_name`
/// is different and already exists.
pub fn rename_in(
    templates: &mut Vec<ContextTemplate>,
    old_name: &str,
    new_name: String,
) -> Result<ContextTemplate, String> {
    if old_name == new_name {
        return templates
            .iter()
            .find(|t| t.name == old_name)
            .cloned()
            .ok_or_else(|| format!("Template '{old_name}' not found"));
    }
    if templates.iter().any(|t| t.name == new_name) {
        return Err(format!("A template named '{new_name}' already exists"));
    }
    let idx = templates
        .iter()
        .position(|t| t.name == old_name)
        .ok_or_else(|| format!("Template '{old_name}' not found"))?;
    templates[idx].name = new_name;
    let renamed = templates[idx].clone();
    sort_templates(templates);
    Ok(renamed)
}

/// Remove a template by name.
pub fn delete_in(templates: &mut Vec<ContextTemplate>, name: &str) -> Result<(), String> {
    let idx = templates
        .iter()
        .position(|t| t.name == name)
        .ok_or_else(|| format!("Template '{name}' not found"))?;
    templates.remove(idx);
    Ok(())
}

/// Accepted JSON shapes for import.  Tried in order; the first match wins.
#[derive(Deserialize)]
#[serde(untagged)]
enum ImportShape {
    /// { "custom_context_templates": { "Name": "body", ... } }
    Wrapped {
        custom_context_templates: BTreeMap<String, String>,
    },
    /// Bare array of objects
    BareArray(Vec<ContextTemplate>),
    /// Bare { "Name": "body", ... } dict
    BareDict(BTreeMap<String, String>),
}

/// Parse any accepted JSON shape into a list of templates.
pub fn parse_import_json(content: &str) -> Result<Vec<ContextTemplate>, String> {
    let shape: ImportShape =
        serde_json::from_str(content).map_err(|e| format!("Invalid JSON: {e}"))?;
    Ok(match shape {
        ImportShape::Wrapped { custom_context_templates } => custom_context_templates
            .into_iter()
            .map(|(name, body)| ContextTemplate { name, body })
            .collect(),
        ImportShape::BareArray(list) => list,
        ImportShape::BareDict(map) => map
            .into_iter()
            .map(|(name, body)| ContextTemplate { name, body })
            .collect(),
    })
}

/// Apply imported entries into an existing template list.  Skips empty-name
/// or empty-body entries.  Existing names are overwritten (upsert).  Returns
/// the count actually applied.
pub fn apply_import(
    templates: &mut Vec<ContextTemplate>,
    imported: Vec<ContextTemplate>,
) -> u32 {
    let mut count = 0u32;
    for entry in imported {
        let name = entry.name.trim().to_string();
        let body = entry.body.trim().to_string();
        if name.is_empty() || body.is_empty() {
            continue;
        }
        upsert_into(templates, name, body);
        count += 1;
    }
    count
}

/// Serialise templates to the canonical wrapped JSON form used for export.
pub fn export_json(templates: &[ContextTemplate]) -> String {
    let map: BTreeMap<&str, &str> = templates
        .iter()
        .map(|t| (t.name.as_str(), t.body.as_str()))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "custom_context_templates": map,
    }))
    .expect("serialising context templates should never fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<ContextTemplate> {
        vec![
            ContextTemplate { name: "Follow-up".into(), body: "Follow-up visit.".into() },
            ContextTemplate { name: "Telehealth".into(), body: "Video consult.".into() },
        ]
    }

    #[test]
    fn sort_is_case_insensitive_alphabetical() {
        let mut v = vec![
            ContextTemplate { name: "zebra".into(), body: "z".into() },
            ContextTemplate { name: "Apple".into(), body: "a".into() },
            ContextTemplate { name: "banana".into(), body: "b".into() },
        ];
        sort_templates(&mut v);
        assert_eq!(v[0].name, "Apple");
        assert_eq!(v[1].name, "banana");
        assert_eq!(v[2].name, "zebra");
    }

    #[test]
    fn upsert_inserts_new() {
        let mut v = Vec::new();
        let result = upsert_into(&mut v, "New".into(), "body".into());
        assert_eq!(result.name, "New");
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn upsert_replaces_existing_body() {
        let mut v = sample();
        upsert_into(&mut v, "Follow-up".into(), "updated body".into());
        assert_eq!(v.len(), 2);
        let fu = v.iter().find(|t| t.name == "Follow-up").unwrap();
        assert_eq!(fu.body, "updated body");
    }

    #[test]
    fn rename_changes_name_and_re_sorts() {
        let mut v = sample();
        let renamed = rename_in(&mut v, "Follow-up", "Zebra-visit".into()).unwrap();
        assert_eq!(renamed.name, "Zebra-visit");
        assert_eq!(v.last().unwrap().name, "Zebra-visit");
    }

    #[test]
    fn rename_to_existing_name_errors() {
        let mut v = sample();
        let err = rename_in(&mut v, "Follow-up", "Telehealth".into()).unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[test]
    fn rename_identity_returns_template() {
        let mut v = sample();
        let r = rename_in(&mut v, "Follow-up", "Follow-up".into()).unwrap();
        assert_eq!(r.name, "Follow-up");
    }

    #[test]
    fn rename_missing_errors() {
        let mut v = sample();
        let err = rename_in(&mut v, "Missing", "Other".into()).unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn delete_removes_entry() {
        let mut v = sample();
        delete_in(&mut v, "Follow-up").unwrap();
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn delete_missing_errors() {
        let mut v = Vec::new();
        assert!(delete_in(&mut v, "Missing").is_err());
    }

    #[test]
    fn parse_wrapped_json() {
        let json = r#"{"custom_context_templates": {"A": "body A", "B": "body B"}}"#;
        let parsed = parse_import_json(json).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn parse_bare_dict() {
        let json = r#"{"A": "body A", "B": "body B"}"#;
        let parsed = parse_import_json(json).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn parse_bare_array() {
        let json = r#"[{"name": "A", "body": "body A"}, {"name": "B", "body": "body B"}]"#;
        let parsed = parse_import_json(json).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn parse_invalid_errors() {
        assert!(parse_import_json("not json").is_err());
    }

    #[test]
    fn apply_import_skips_empty_and_trims() {
        let mut v: Vec<ContextTemplate> = Vec::new();
        let imported = vec![
            ContextTemplate { name: "  Keep  ".into(), body: "  yes  ".into() },
            ContextTemplate { name: "".into(), body: "skip".into() },
            ContextTemplate { name: "skip".into(), body: "".into() },
        ];
        let count = apply_import(&mut v, imported);
        assert_eq!(count, 1);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "Keep");
        assert_eq!(v[0].body, "yes");
    }

    #[test]
    fn apply_import_upserts_existing() {
        let mut v = sample();
        let imported = vec![ContextTemplate {
            name: "Follow-up".into(),
            body: "new body".into(),
        }];
        let count = apply_import(&mut v, imported);
        assert_eq!(count, 1);
        assert_eq!(v.len(), 2);
        let fu = v.iter().find(|t| t.name == "Follow-up").unwrap();
        assert_eq!(fu.body, "new body");
    }

    #[test]
    fn export_produces_wrapped_form_and_round_trips() {
        let v = sample();
        let json = export_json(&v);
        assert!(json.contains("custom_context_templates"));
        let reparsed = parse_import_json(&json).unwrap();
        assert_eq!(reparsed.len(), 2);
    }
}
```

- [ ] **Step 3: Run tests — expect pass**

Run: `cargo test -p rust-medical-assistant context_templates -- --nocapture`
Expected: all 14 helper tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/mod.rs src-tauri/src/commands/context_templates.rs
git commit -m "feat(commands): add context template helpers with tests"
```

---

### Task 3: Tauri commands (list/upsert/rename/delete/import/export)

**Files:**
- Modify: `src-tauri/src/commands/context_templates.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Append command wrappers to the helpers file**

At the bottom of `src-tauri/src/commands/context_templates.rs` (before the `#[cfg(test)]` block), add:

```rust
use medical_db::settings::SettingsRepo;
use tracing::{info, instrument};

use crate::state::AppState;

fn load_config(state: &tauri::State<'_, AppState>) -> Result<medical_core::types::settings::AppConfig, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SettingsRepo::load_config(&conn).map_err(|e| e.to_string())
}

fn save_config(
    state: &tauri::State<'_, AppState>,
    config: &medical_core::types::settings::AppConfig,
) -> Result<(), String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SettingsRepo::save_config(&conn, config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_context_templates(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContextTemplate>, String> {
    let config = load_config(&state)?;
    let mut templates = config.custom_context_templates;
    sort_templates(&mut templates);
    Ok(templates)
}

#[tauri::command]
pub fn upsert_context_template(
    state: tauri::State<'_, AppState>,
    name: String,
    body: String,
) -> Result<ContextTemplate, String> {
    let name = name.trim().to_string();
    let body = body.trim().to_string();
    if name.is_empty() {
        return Err("Template name cannot be empty".into());
    }
    if body.is_empty() {
        return Err("Template body cannot be empty".into());
    }
    let mut config = load_config(&state)?;
    let result = upsert_into(&mut config.custom_context_templates, name, body);
    save_config(&state, &config)?;
    info!(name = %result.name, "Upserted context template");
    Ok(result)
}

#[tauri::command]
pub fn rename_context_template(
    state: tauri::State<'_, AppState>,
    old_name: String,
    new_name: String,
) -> Result<ContextTemplate, String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("Template name cannot be empty".into());
    }
    let mut config = load_config(&state)?;
    let result = rename_in(&mut config.custom_context_templates, &old_name, new_name)?;
    save_config(&state, &config)?;
    Ok(result)
}

#[tauri::command]
pub fn delete_context_template(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let mut config = load_config(&state)?;
    delete_in(&mut config.custom_context_templates, &name)?;
    save_config(&state, &config)?;
    info!(name, "Deleted context template");
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn import_context_templates_json(
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<u32, String> {
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;
    let imported = parse_import_json(&content)?;
    let mut config = load_config(&state)?;
    let count = apply_import(&mut config.custom_context_templates, imported);
    save_config(&state, &config)?;
    info!(count, path = %file_path, "Imported context templates");
    Ok(count)
}

#[tauri::command]
#[instrument(skip(state))]
pub async fn export_context_templates_json(
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<u32, String> {
    let config = load_config(&state)?;
    let count = config.custom_context_templates.len() as u32;
    let json = export_json(&config.custom_context_templates);
    tokio::fs::write(&file_path, json)
        .await
        .map_err(|e| format!("Failed to write file: {e}"))?;
    info!(count, path = %file_path, "Exported context templates");
    Ok(count)
}
```

- [ ] **Step 2: Register the six commands in `generate_handler!`**

In `src-tauri/src/lib.rs`, add these lines inside the `tauri::generate_handler![ ... ]` array (alongside the vocabulary commands):

```rust
            commands::context_templates::list_context_templates,
            commands::context_templates::upsert_context_template,
            commands::context_templates::rename_context_template,
            commands::context_templates::delete_context_template,
            commands::context_templates::import_context_templates_json,
            commands::context_templates::export_context_templates_json,
```

- [ ] **Step 3: Build — expect pass**

Run: `cargo build -p rust-medical-assistant`
Expected: clean build, only pre-existing warnings.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/context_templates.rs src-tauri/src/lib.rs
git commit -m "feat(commands): add context template CRUD and import/export Tauri commands"
```

---

### Task 4: Frontend TypeScript bindings, types, and store default

**Files:**
- Create: `src/lib/api/contextTemplates.ts`
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/stores/settings.ts`

- [ ] **Step 1: Create the API bindings file**

Create `src/lib/api/contextTemplates.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';

export interface ContextTemplate {
  name: string;
  body: string;
}

export function listContextTemplates(): Promise<ContextTemplate[]> {
  return invoke('list_context_templates');
}

export function upsertContextTemplate(
  name: string,
  body: string,
): Promise<ContextTemplate> {
  return invoke('upsert_context_template', { name, body });
}

export function renameContextTemplate(
  oldName: string,
  newName: string,
): Promise<ContextTemplate> {
  return invoke('rename_context_template', { oldName, newName });
}

export function deleteContextTemplate(name: string): Promise<void> {
  return invoke('delete_context_template', { name });
}

export function importContextTemplatesJson(filePath: string): Promise<number> {
  return invoke('import_context_templates_json', { filePath });
}

export function exportContextTemplatesJson(filePath: string): Promise<number> {
  return invoke('export_context_templates_json', { filePath });
}
```

- [ ] **Step 2: Add `ContextTemplate` to the frontend types**

In `src/lib/types/index.ts`, locate the `AppConfig` interface. Add this interface near it (above `AppConfig`):

```typescript
export interface ContextTemplate {
  name: string;
  body: string;
}
```

Then add this field inside `AppConfig` (in the same group as `vocabulary_enabled`):

```typescript
  custom_context_templates: ContextTemplate[];
```

- [ ] **Step 3: Add the default in the settings store**

In `src/lib/stores/settings.ts`, add this entry to the `defaults` object (after `vocabulary_enabled: true,`):

```typescript
  custom_context_templates: [],
```

- [ ] **Step 4: Verify svelte-check — expect no new errors**

Run: `npx svelte-check --fail-on-warnings=false 2>&1 | tail -8`
Expected: no new errors attributable to these files (the three existing pre-existing errors in unrelated files are acceptable).

- [ ] **Step 5: Commit**

```bash
git add src/lib/api/contextTemplates.ts src/lib/types/index.ts src/lib/stores/settings.ts
git commit -m "feat(frontend): add ContextTemplate API, types, and store default"
```

---

### Task 5: `ContextTemplateDialog.svelte` management component

**Files:**
- Create: `src/lib/components/ContextTemplateDialog.svelte`

This mirrors `VocabularyDialog.svelte`'s structure but for the simpler 2-field (name/body) data shape.

- [ ] **Step 1: Create the component**

Create `src/lib/components/ContextTemplateDialog.svelte`:

```svelte
<script lang="ts">
  import {
    listContextTemplates,
    upsertContextTemplate,
    renameContextTemplate,
    deleteContextTemplate,
    type ContextTemplate,
  } from '../api/contextTemplates';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let templates = $state<ContextTemplate[]>([]);
  let loading = $state(false);
  let searchText = $state('');

  // Add/Edit form
  let editing = $state<ContextTemplate | null>(null);
  let showForm = $state(false);
  let formName = $state('');
  let formBody = $state('');
  let formError = $state('');

  async function loadTemplates() {
    loading = true;
    try {
      templates = await listContextTemplates();
    } catch (err) {
      console.error('Failed to load templates:', err);
    } finally {
      loading = false;
    }
  }

  function filtered(): ContextTemplate[] {
    if (!searchText.trim()) return templates;
    const q = searchText.toLowerCase();
    return templates.filter(
      (t) => t.name.toLowerCase().includes(q) || t.body.toLowerCase().includes(q),
    );
  }

  function openAdd() {
    editing = null;
    formName = '';
    formBody = '';
    formError = '';
    showForm = true;
  }

  function openEdit(t: ContextTemplate) {
    editing = t;
    formName = t.name;
    formBody = t.body;
    formError = '';
    showForm = true;
  }

  function closeForm() {
    showForm = false;
    editing = null;
    formError = '';
  }

  async function handleSave() {
    const name = formName.trim();
    const body = formBody.trim();
    if (!name) { formError = 'Name is required.'; return; }
    if (!body) { formError = 'Body is required.'; return; }
    try {
      if (editing) {
        if (editing.name !== name) {
          await renameContextTemplate(editing.name, name);
        }
        await upsertContextTemplate(name, body);
      } else {
        if (templates.some((t) => t.name === name)) {
          formError = `A template named "${name}" already exists.`;
          return;
        }
        await upsertContextTemplate(name, body);
      }
      closeForm();
      await loadTemplates();
    } catch (err: any) {
      formError = err?.toString() || 'Failed to save template.';
    }
  }

  async function handleDelete(t: ContextTemplate) {
    if (!confirm(`Delete template "${t.name}"?`)) return;
    try {
      await deleteContextTemplate(t.name);
      await loadTemplates();
    } catch (err) {
      console.error('Failed to delete template:', err);
    }
  }

  $effect(() => {
    if (open) {
      loadTemplates();
    }
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="ct-overlay" onclick={onclose}>
    <div class="ct-dialog" onclick={(e) => e.stopPropagation()}>
      <div class="ct-header">
        <h2>Manage Context Templates</h2>
        <button class="btn-close" onclick={onclose}>&times;</button>
      </div>

      <div class="ct-toolbar">
        <input
          class="search-input"
          type="text"
          placeholder="Search name or body..."
          bind:value={searchText}
        />
        <button class="btn-add" onclick={openAdd}>+ Add Template</button>
      </div>

      <div class="ct-body">
        {#if showForm}
          <div class="ct-form">
            <div class="form-header">
              <h3>{editing ? 'Edit' : 'Add'} Template</h3>
              <button class="btn-close-form" aria-label="Close form" onclick={closeForm}>&times;</button>
            </div>
            {#if formError}
              <div class="form-error">{formError}</div>
            {/if}
            <label class="field">
              <span>Name</span>
              <input type="text" bind:value={formName} placeholder="e.g. Follow-up visit" />
            </label>
            <label class="field">
              <span>Body</span>
              <textarea bind:value={formBody} rows="5" placeholder="Template body text..."></textarea>
            </label>
            <div class="form-actions">
              <button class="btn-save" onclick={handleSave}>Save</button>
              <button class="btn-cancel" onclick={closeForm}>Cancel</button>
            </div>
          </div>
        {/if}

        <div class="ct-list-wrap">
          {#if loading}
            <p class="loading-text">Loading...</p>
          {:else if filtered().length === 0}
            <p class="empty-text">
              {templates.length === 0 ? 'No templates yet. Click "+ Add Template" to create one.' : 'No templates match the search.'}
            </p>
          {:else}
            <table class="ct-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Body Preview</th>
                  <th class="col-actions">Actions</th>
                </tr>
              </thead>
              <tbody>
                {#each filtered() as t (t.name)}
                  <tr>
                    <td class="name-cell">{t.name}</td>
                    <td class="body-cell truncate">{t.body.replace(/\n/g, ' ')}</td>
                    <td class="col-actions actions">
                      <button class="btn-edit" onclick={() => openEdit(t)}>Edit</button>
                      <button class="btn-delete" onclick={() => handleDelete(t)}>Del</button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
      </div>

      <div class="ct-footer">
        <span class="footer-count">
          {filtered().length} shown{searchText ? ` of ${templates.length}` : ''}
        </span>
      </div>
    </div>
  </div>
{/if}

<style>
  .ct-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .ct-dialog {
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    border-radius: 8px;
    width: 90vw;
    max-width: 820px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  .ct-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border-color, #333);
    flex: 0 0 auto;
  }
  .ct-header h2 { margin: 0; font-size: 1.1rem; font-weight: 600; }
  .btn-close {
    background: none;
    border: none;
    color: var(--text-secondary, #aaa);
    font-size: 1.4rem;
    line-height: 1;
    padding: 4px 8px;
    cursor: pointer;
    border-radius: 4px;
  }
  .btn-close:hover { background: rgba(255, 255, 255, 0.08); }

  .ct-toolbar {
    display: flex;
    gap: 8px;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border-color, #333);
    flex: 0 0 auto;
    align-items: center;
  }
  .search-input {
    flex: 1 1 auto;
    min-width: 0;
    padding: 6px 10px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
    font-size: 0.9rem;
  }
  .btn-add {
    flex: 0 0 auto;
    padding: 6px 14px;
    border-radius: 4px;
    border: none;
    background: var(--accent-color, #4a9eff);
    color: white;
    cursor: pointer;
    white-space: nowrap;
    font-size: 0.9rem;
  }
  .btn-add:hover { filter: brightness(1.1); }

  .ct-body { flex: 1 1 auto; overflow-y: auto; min-height: 0; }

  .ct-form {
    padding: 14px 20px;
    border-bottom: 1px solid var(--border-color, #333);
    background: var(--bg-primary, #111);
  }
  .form-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
  .form-header h3 { margin: 0; font-size: 0.95rem; font-weight: 600; }
  .btn-close-form {
    background: none; border: none; color: var(--text-secondary, #888);
    font-size: 1.2rem; line-height: 1; padding: 2px 6px; cursor: pointer; border-radius: 3px;
  }
  .btn-close-form:hover { background: rgba(255, 255, 255, 0.08); }
  .form-error {
    color: #ff6b6b; margin-bottom: 10px; font-size: 0.85rem;
    padding: 6px 10px; background: rgba(255, 107, 107, 0.1); border-radius: 4px;
  }
  .field { display: flex; flex-direction: column; gap: 4px; font-size: 0.8rem; color: var(--text-secondary, #aaa); margin-bottom: 10px; }
  .field span { font-weight: 500; }
  .field input, .field textarea {
    padding: 7px 10px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    font-size: 0.9rem;
    font-family: inherit;
  }
  .field textarea { resize: vertical; min-height: 80px; }
  .form-actions { display: flex; gap: 8px; }
  .btn-save {
    padding: 7px 18px; border-radius: 4px; border: none;
    background: var(--accent-color, #4a9eff); color: white; cursor: pointer; font-size: 0.9rem;
  }
  .btn-save:hover { filter: brightness(1.1); }
  .btn-cancel {
    padding: 7px 18px; border-radius: 4px;
    border: 1px solid var(--border-color, #444); background: transparent;
    color: var(--text-primary, #e0e0e0); cursor: pointer; font-size: 0.9rem;
  }
  .btn-cancel:hover { background: rgba(255, 255, 255, 0.05); }

  .ct-list-wrap { padding: 8px 20px 16px; }
  .loading-text, .empty-text { text-align: center; color: var(--text-secondary, #888); padding: 32px; font-size: 0.9rem; }
  .ct-table { width: 100%; border-collapse: collapse; font-size: 0.88rem; table-layout: fixed; }
  .ct-table th {
    text-align: left; padding: 8px; border-bottom: 1px solid var(--border-color, #333);
    color: var(--text-secondary, #888); font-weight: 500; font-size: 0.8rem;
    text-transform: uppercase; letter-spacing: 0.03em;
    position: sticky; top: 0; background: var(--bg-secondary, #1e1e1e); z-index: 1;
  }
  .ct-table td {
    padding: 8px; border-bottom: 1px solid var(--border-color, #222);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ct-table tr:hover td { background: rgba(255, 255, 255, 0.03); }
  .name-cell { width: 200px; font-weight: 500; }
  .body-cell { color: var(--text-secondary, #bbb); }
  .truncate { max-width: 0; }
  .col-actions { width: 110px; }
  .actions { display: flex; gap: 4px; }
  .btn-edit, .btn-delete {
    padding: 3px 10px; border-radius: 3px; border: 1px solid var(--border-color, #444);
    background: transparent; color: var(--text-secondary, #bbb); cursor: pointer; font-size: 0.78rem;
  }
  .btn-edit:hover { background: rgba(255, 255, 255, 0.05); }
  .btn-delete { color: #ff6b6b; border-color: #ff6b6b44; }
  .btn-delete:hover { background: rgba(255, 107, 107, 0.08); }

  .ct-footer {
    padding: 10px 20px; border-top: 1px solid var(--border-color, #333);
    display: flex; justify-content: flex-end; align-items: center; flex: 0 0 auto;
  }
  .footer-count { font-size: 0.82rem; color: var(--text-secondary, #888); }

  /* Override global input { width: 100% } for any checkboxes */
  .ct-dialog input[type="checkbox"] {
    width: 14px !important; height: 14px; min-width: 14px; padding: 0; margin: 0;
  }
</style>
```

- [ ] **Step 2: Verify svelte-check passes**

Run: `npx svelte-check --fail-on-warnings=false 2>&1 | tail -5`
Expected: no new errors from `ContextTemplateDialog.svelte`.

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/ContextTemplateDialog.svelte
git commit -m "feat(frontend): add ContextTemplateDialog component"
```

---

### Task 6: Wire dialog into Settings tab + import/export buttons

**Files:**
- Modify: `src/lib/components/SettingsContent.svelte`

- [ ] **Step 1: Import the dialog and API**

Near the other imports at the top of `<script lang="ts">` in `SettingsContent.svelte`, add:

```typescript
  import ContextTemplateDialog from './ContextTemplateDialog.svelte';
  import {
    listContextTemplates,
    importContextTemplatesJson,
    exportContextTemplatesJson,
  } from '../api/contextTemplates';
```

- [ ] **Step 2: Add state and handlers**

After the existing `vocabDialogOpen` state line, add:

```typescript
  let ctxTemplateDialogOpen = $state(false);
  let ctxTemplateCount = $state(0);

  async function loadCtxTemplateCount() {
    try {
      const list = await listContextTemplates();
      ctxTemplateCount = list.length;
    } catch (err) {
      console.error('Failed to load context template count:', err);
    }
  }

  async function handleImportCtxTemplates() {
    const selected = await openDialog({
      multiple: false,
      title: 'Import Context Templates JSON',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      try {
        const count = await importContextTemplatesJson(selected as string);
        alert(`Imported ${count} context templates.`);
        await loadCtxTemplateCount();
      } catch (err: any) {
        alert(`Import failed: ${err}`);
      }
    }
  }

  async function handleExportCtxTemplates() {
    const selected = await saveDialog({
      title: 'Export Context Templates JSON',
      defaultPath: 'context_templates.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      try {
        const count = await exportContextTemplatesJson(selected);
        alert(`Exported ${count} context templates.`);
      } catch (err: any) {
        alert(`Export failed: ${err}`);
      }
    }
  }

  function handleCtxTemplateDialogClose() {
    ctxTemplateDialogOpen = false;
    loadCtxTemplateCount();
  }
```

- [ ] **Step 3: Load count on mount**

Find the existing effect or onMount that calls `loadVocabCount()`. Add `loadCtxTemplateCount()` alongside it (search for `loadVocabCount();`). If you see something like:

```typescript
  $effect(() => {
    loadVocabCount();
  });
```

Change to:

```typescript
  $effect(() => {
    loadVocabCount();
    loadCtxTemplateCount();
  });
```

Or if `loadVocabCount()` is only called from `onMount`, mirror it.

- [ ] **Step 4: Add the UI section**

Immediately after the closing `</div>` of the vocab `form-group` (after line containing "Export JSON" for vocabulary — approx line 453), insert:

```svelte
        <h3 class="section-title" style="margin-top: 24px">Context Templates</h3>
        <p class="section-desc">Reusable snippets of clinical context that can be applied to the Patient Context field on the Record tab.</p>

        <div class="form-group">
          <span class="form-label">
            {ctxTemplateCount} template{ctxTemplateCount === 1 ? '' : 's'} saved
          </span>
          <div class="vocab-buttons">
            <button class="btn-browse" onclick={() => { ctxTemplateDialogOpen = true; }}>
              Manage Templates
            </button>
            <button class="btn-browse" onclick={handleImportCtxTemplates}>
              Import JSON
            </button>
            <button class="btn-browse" onclick={handleExportCtxTemplates}>
              Export JSON
            </button>
          </div>
        </div>
```

(The `.vocab-buttons` class is already defined in this file, so we reuse it.)

- [ ] **Step 5: Mount the dialog at the bottom**

Right after the `<VocabularyDialog ... />` line near the end of the file, add:

```svelte
<ContextTemplateDialog open={ctxTemplateDialogOpen} onclose={handleCtxTemplateDialogClose} />
```

- [ ] **Step 6: Verify**

Run: `npx svelte-check --fail-on-warnings=false 2>&1 | tail -5`
Expected: no new errors.

- [ ] **Step 7: Commit**

```bash
git add src/lib/components/SettingsContent.svelte
git commit -m "feat(frontend): wire Context Templates section in Settings"
```

---

### Task 7: Record tab picker + Save-as-template modal

**Files:**
- Modify: `src/lib/pages/RecordTab.svelte`

- [ ] **Step 1: Add imports and state**

In the `<script lang="ts">` block of `RecordTab.svelte`, add near the existing imports:

```typescript
  import {
    listContextTemplates,
    upsertContextTemplate,
    type ContextTemplate,
  } from '../lib/api/contextTemplates';
  // Note: adjust the relative path to `../lib/api/contextTemplates` based on
  // RecordTab's location.  If RecordTab is at `src/lib/pages/RecordTab.svelte`
  // the correct path is `../api/contextTemplates`.
```

Use `../api/contextTemplates` since `RecordTab.svelte` lives in `src/lib/pages/`.

After the existing `contextText` / `contextCollapsed` state, add:

```typescript
  // Template picker state
  let templates = $state<ContextTemplate[]>([]);
  let selectedTemplate = $state('');

  // Save-as-template modal state
  let saveModalOpen = $state(false);
  let saveModalName = $state('');
  let saveModalError = $state('');
  let saveModalOverwriteConfirm = $state(false);

  async function loadTemplates() {
    try {
      templates = await listContextTemplates();
    } catch (err) {
      console.error('Failed to load templates:', err);
    }
  }

  function applyTemplate(name: string) {
    if (!name) return;
    const t = templates.find((x) => x.name === name);
    if (!t) return;
    if (contextText.trim() === '') {
      contextText = t.body;
    } else {
      contextText = contextText.replace(/\s+$/, '') + '\n\n' + t.body;
    }
    // Reset dropdown so the same template can be applied again
    selectedTemplate = '';
    // Ensure the accordion is open so the user sees the inserted text
    contextCollapsed = false;
  }

  function openSaveModal() {
    if (contextText.trim() === '') return;
    saveModalName = '';
    saveModalError = '';
    saveModalOverwriteConfirm = false;
    saveModalOpen = true;
  }

  function closeSaveModal() {
    saveModalOpen = false;
    saveModalError = '';
    saveModalOverwriteConfirm = false;
  }

  async function confirmSaveTemplate() {
    const name = saveModalName.trim();
    if (!name) {
      saveModalError = 'Name is required.';
      return;
    }
    const exists = templates.some((t) => t.name === name);
    if (exists && !saveModalOverwriteConfirm) {
      saveModalOverwriteConfirm = true;
      saveModalError = `A template named "${name}" exists. Click Save again to overwrite.`;
      return;
    }
    try {
      await upsertContextTemplate(name, contextText);
      await loadTemplates();
      closeSaveModal();
    } catch (err: any) {
      saveModalError = err?.toString() || 'Failed to save template.';
    }
  }
```

- [ ] **Step 2: Add onMount to load templates**

If there's already an `onMount` in this file, add `loadTemplates();` inside it. Otherwise add:

```typescript
  import { onMount } from 'svelte';

  onMount(() => {
    loadTemplates();
  });
```

- [ ] **Step 3: Add picker UI inside the Patient Context accordion**

Find the block that renders the context textarea (around line 125–130):

```svelte
      <textarea
        class="context-textarea"
        placeholder="Paste chart notes, medications, history..."
        bind:value={contextText}
        rows="5"
      ></textarea>
```

Immediately BEFORE that `<textarea>`, insert a toolbar row:

```svelte
      <div class="template-toolbar">
        <select
          class="template-picker"
          bind:value={selectedTemplate}
          onchange={() => applyTemplate(selectedTemplate)}
          disabled={templates.length === 0}
        >
          <option value="">
            {templates.length === 0 ? 'No templates saved' : 'Apply template…'}
          </option>
          {#each templates as t (t.name)}
            <option value={t.name}>{t.name}</option>
          {/each}
        </select>
        <button
          class="btn-save-template"
          onclick={openSaveModal}
          disabled={contextText.trim() === ''}
          title={contextText.trim() === '' ? 'Type something first' : 'Save current text as a new template'}
        >
          Save as template
        </button>
      </div>
```

- [ ] **Step 4: Add the save-as-template modal**

Find the end of the main template block (after the closing tag of the Patient Context accordion section). Add the modal as a sibling at the root template level (outside any existing flow containers):

```svelte
{#if saveModalOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="save-modal-overlay" onclick={closeSaveModal}>
    <div class="save-modal" onclick={(e) => e.stopPropagation()}>
      <div class="save-modal-header">
        <h3>Save as Template</h3>
        <button class="btn-close" aria-label="Close" onclick={closeSaveModal}>&times;</button>
      </div>
      {#if saveModalError}
        <div class="save-modal-error">{saveModalError}</div>
      {/if}
      <label class="save-modal-field">
        <span>Name</span>
        <input type="text" bind:value={saveModalName} placeholder="e.g. Follow-up visit" autofocus />
      </label>
      <div class="save-modal-field">
        <span>Preview</span>
        <pre class="save-modal-preview">{contextText}</pre>
      </div>
      <div class="save-modal-actions">
        <button class="btn-save" onclick={confirmSaveTemplate}>
          {saveModalOverwriteConfirm ? 'Overwrite' : 'Save'}
        </button>
        <button class="btn-cancel" onclick={closeSaveModal}>Cancel</button>
      </div>
    </div>
  </div>
{/if}
```

- [ ] **Step 5: Add styles**

Append to the existing `<style>` block (or create one if missing). If there's no `<style>` block yet, add a new one at the bottom of the file:

```svelte
<style>
  .template-toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
    align-items: center;
  }
  .template-picker {
    flex: 1 1 auto;
    min-width: 0;
    padding: 6px 10px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
    font-size: 0.88rem;
  }
  .template-picker:disabled { opacity: 0.6; cursor: not-allowed; }
  .btn-save-template {
    flex: 0 0 auto;
    padding: 6px 14px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: transparent;
    color: var(--text-primary, #e0e0e0);
    cursor: pointer;
    font-size: 0.88rem;
    white-space: nowrap;
  }
  .btn-save-template:hover:not(:disabled) { background: rgba(255, 255, 255, 0.05); }
  .btn-save-template:disabled { opacity: 0.4; cursor: not-allowed; }

  .save-modal-overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 2000;
  }
  .save-modal {
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    border-radius: 8px;
    width: 90vw; max-width: 520px; max-height: 85vh;
    display: flex; flex-direction: column;
    padding: 20px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  .save-modal-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .save-modal-header h3 { margin: 0; font-size: 1.05rem; }
  .save-modal .btn-close {
    background: none; border: none; color: var(--text-secondary, #aaa);
    font-size: 1.4rem; line-height: 1; padding: 4px 8px; cursor: pointer; border-radius: 4px;
  }
  .save-modal .btn-close:hover { background: rgba(255, 255, 255, 0.08); }
  .save-modal-error {
    color: #ff6b6b; margin-bottom: 10px; font-size: 0.85rem;
    padding: 6px 10px; background: rgba(255, 107, 107, 0.1); border-radius: 4px;
  }
  .save-modal-field { display: flex; flex-direction: column; gap: 4px; font-size: 0.85rem; color: var(--text-secondary, #aaa); margin-bottom: 10px; }
  .save-modal-field span { font-weight: 500; }
  .save-modal-field input {
    padding: 7px 10px; border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0); font-size: 0.9rem;
  }
  .save-modal-preview {
    background: var(--bg-primary, #111); padding: 10px; border-radius: 4px;
    border: 1px solid var(--border-color, #333); max-height: 180px; overflow-y: auto;
    white-space: pre-wrap; font-size: 0.85rem; margin: 0; font-family: inherit;
  }
  .save-modal-actions { display: flex; gap: 8px; margin-top: 8px; }
  .save-modal .btn-save {
    padding: 7px 18px; border-radius: 4px; border: none;
    background: var(--accent-color, #4a9eff); color: white; cursor: pointer; font-size: 0.9rem;
  }
  .save-modal .btn-save:hover { filter: brightness(1.1); }
  .save-modal .btn-cancel {
    padding: 7px 18px; border-radius: 4px;
    border: 1px solid var(--border-color, #444); background: transparent;
    color: var(--text-primary, #e0e0e0); cursor: pointer; font-size: 0.9rem;
  }
</style>
```

(If the file already has a `<style>` block, append the new rules inside it.)

- [ ] **Step 6: Verify**

Run: `npx svelte-check --fail-on-warnings=false 2>&1 | tail -5`
Expected: no new errors from `RecordTab.svelte`.

- [ ] **Step 7: Commit**

```bash
git add src/lib/pages/RecordTab.svelte
git commit -m "feat(frontend): add context template picker and save-as-template modal to Record tab"
```

---

### Task 8: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: all tests pass, including the new settings tests and context_templates tests.

- [ ] **Step 2: Full build**

Run: `cargo build -p rust-medical-assistant 2>&1 | tail -10`
Expected: clean build.

- [ ] **Step 3: Full svelte-check**

Run: `npx svelte-check --fail-on-warnings=false 2>&1 | tail -10`
Expected: no new errors introduced by this work. Pre-existing errors in unrelated files are acceptable.

- [ ] **Step 4: Smoke-test manually (if dev server available)**

Start the app, then:
1. Go to Settings → General → "Context Templates" section. Confirm the section renders with count 0 and Manage/Import/Export buttons.
2. Click Manage → dialog opens. Click "+ Add Template" → form appears. Add a template named "Test" with body "Hello world". Save. The table should show the new entry.
3. Close dialog. Go to the Record tab. Open the Patient Context accordion. The dropdown should show "Test" as an option.
4. Select "Test" — the textarea populates with "Hello world".
5. Type " plus typed notes" at the end, then click "Save as template". Name it "Test 2". Save. Confirm dropdown now shows both.
6. Select "Test" again with the text field still populated — confirm it appends (prior text preserved, blank line, then template body).
7. Try import: create a file `/tmp/test_templates.json` containing `{"Foo":"bar"}` and import. Confirm count increases and dialog shows new entry.
8. Export to `/tmp/out.json` — confirm the file has the `custom_context_templates` wrapped shape.

- [ ] **Step 5: No commit needed unless fixes were required.**

---

## Remember
- Keep changes minimal per task — the subagent's job is to implement exactly what the step says.
- Frequent commits: one per task.
- Follow TDD on Task 1 and Task 2 (tests first, then code).
- Import paths: RecordTab lives in `src/lib/pages/`, so `../api/contextTemplates`. Settings content in `src/lib/components/`, so `../api/contextTemplates`.
