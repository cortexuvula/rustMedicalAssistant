# Provider Simplification and Prompt Editor — Design

**Date:** 2026-04-21
**Status:** Draft, awaiting user review

---

## Goal

1. Reduce the supported AI provider surface to **only local providers**: LM Studio and Ollama. All cloud providers (OpenAI, Anthropic, Gemini, Groq, Cerebras) are removed from the codebase.
2. Expose a **Prompts tab** in Settings where the user can view and edit the system prompts used for each generated document type (SOAP, Referral, Letter, Clinical Synopsis). Templates support placeholder tokens that are substituted at generation time.

## Non-Goals

- No changes to STT, TTS, audio capture, recording storage, or the chat agent system prompt (chat prompt stays hardcoded for this iteration).
- No live preview of resolved prompts in the editor — the editor shows the template with placeholders; substitution happens at generation time only.
- No user-facing migration dialog or one-time warning. Stale provider values are silently corrected on load.
- No deletion of stored API keys from the keyring on upgrade. They simply stop being read; user can manually remove them via OS keychain tools if desired.

---

## Architecture Summary

**Before:** 7 provider modules behind a `ProviderRegistry` dispatch. Provider-specific branching scattered across `state.rs`, `generation.rs`, `soap_generator.rs`. SOAP prompt has two variants (generic vs. Anthropic-optimised). Custom prompt fields exist for SOAP/Referral/Letter in settings but only SOAP reads them.

**After:** Two provider modules (LM Studio, Ollama), both using `openai_compat.rs` under the hood. A single default prompt per document type with placeholder tokens (`{icd_label}`, `{recipient_type}`, etc.). A new Settings tab lets users override any of the four default prompts. Custom overrides flow through a unified resolution helper.

---

## Phase A — Provider Removal

### A.1 Delete provider modules

Remove:
- `crates/ai-providers/src/anthropic.rs`
- `crates/ai-providers/src/openai.rs`
- `crates/ai-providers/src/gemini.rs`
- `crates/ai-providers/src/groq.rs`
- `crates/ai-providers/src/cerebras.rs`

Update `crates/ai-providers/src/lib.rs`:
- Drop the `pub mod anthropic; pub mod openai; pub mod gemini; pub mod groq; pub mod cerebras;` declarations
- Drop corresponding re-exports

Verify via `cargo check -p medical-ai-providers` after deletion.

### A.2 Simplify provider initialization

`src-tauri/src/state.rs:81-126` currently reads:
```rust
if let Ok(Some(key)) = keys.get_key("openai") { registry.register("openai", OpenAiProvider::new(&key)); }
// ... 4 more similar blocks
registry.register("ollama", OllamaProvider::new());
registry.register("lmstudio", LmStudioProvider::new(&cfg.lmstudio_host, cfg.lmstudio_port));
```

Becomes:
```rust
registry.register("ollama", OllamaProvider::new());
registry.register("lmstudio", LmStudioProvider::new(&cfg.lmstudio_host, cfg.lmstudio_port));
```

The `keys` parameter to `init_ai_providers` becomes unused. Remove it from the signature.

### A.3 Remove API Keys tab

`src/lib/components/SettingsContent.svelte`:
- Delete `API_PROVIDERS` constant (lines 26-41)
- Delete the API Keys tab section (template markup) and its `handleSaveApiKey` handler (lines 177-197)
- Delete the tab navigation entry for "API Keys"

### A.4 Drop per-provider model config

`crates/core/src/types/settings.rs:66-83` — `SoapNoteSettings` struct currently has `openai_model`, `anthropic_model`, `groq_model`, plus `icd_code_version`. Remove the three provider-specific model fields. Keep `icd_code_version` (still referenced by the placeholder system in Phase B).

### A.5 Remove `max_tokens_for_provider`

`src-tauri/src/commands/generation.rs:132-137`:
```rust
fn max_tokens_for_provider(provider: &str) -> Option<u32> {
    match provider {
        "lmstudio" | "ollama" => None,
        _ => Some(4096),
    }
}
```

Delete the function. At the four call sites (SOAP line 243, referral 406, letter 506, synopsis 558), replace `max_tokens_for_provider(&settings.ai_provider)` with the literal `None`.

### A.6 RAG: Ollama embeddings only

`src-tauri/src/state.rs:184-192`:
```rust
let embedding_generator = if let Ok(Some(key)) = keys.get_key("openai") {
    info!("RAG: using OpenAI embeddings");
    Arc::new(EmbeddingGenerator::new_openai(&key))
} else {
    info!("RAG: using Ollama embeddings (local)");
    Arc::new(EmbeddingGenerator::new_ollama(None, None))
};
```

Becomes:
```rust
info!("RAG: using Ollama embeddings (local)");
let embedding_generator = Arc::new(EmbeddingGenerator::new_ollama(None, None));
```

Also update `crates/rag/src/embeddings.rs`:
- Remove `EmbeddingGenerator::new_openai()` constructor and the underlying OpenAI embedding implementation
- Remove the OpenAI embedding test (line 250)

### A.7 Remove provider-specific prompt branching

`crates/processing/src/soap_generator.rs:103-106` currently:
```rust
match config.provider.as_deref() {
    Some("anthropic") | Some("lmstudio") => build_anthropic_prompt(),
    _ => build_generic_prompt(icd_instruction, icd_label, template_instruction),
}
```

Becomes:
```rust
resolve_prompt(
    config.custom_prompt.as_deref().unwrap_or(default_soap_prompt()),
    &soap_placeholders(&config.icd_version, &config.template),
)
```

`build_generic_prompt` is deleted.

### A.8 Simplify `reinit_providers` command

`src-tauri/src/commands/providers.rs:11-43` rebuilds the registry after a key or config change. After removal, this command still has a legitimate purpose: rebuilding the LM Studio provider when `lmstudio_host`/`lmstudio_port` changes. Update it to drop the `keys` argument in its inner call to `init_ai_providers` (since that parameter is removed per A.2) and remove any log lines referencing cloud-key reloads.

### A.9 Migration

`crates/core/src/types/settings.rs` — in the settings loader (wherever `AppConfig::load` or equivalent lives), after deserialization:

```rust
if !matches!(cfg.ai_provider.as_str(), "lmstudio" | "ollama") {
    tracing::warn!(
        "Stale ai_provider '{}' detected after provider removal; resetting to 'lmstudio'",
        cfg.ai_provider
    );
    cfg.ai_provider = "lmstudio".into();
}
```

No user-facing message. A log line is sufficient.

---

## Phase B — Placeholder-Driven Prompt Templates

### B.1 Template resolution helper

New module: `crates/processing/src/prompt_resolver.rs`

```rust
use std::collections::HashMap;

/// Substitutes `{key}` tokens in `template` with values from `placeholders`.
/// Unknown placeholders pass through unchanged (so the user sees them in
/// output and can fix their template).
pub fn resolve_prompt(template: &str, placeholders: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in placeholders {
        let token = format!("{{{}}}", key);
        out = out.replace(&token, value);
    }
    out
}
```

**Tests required:**
- Empty placeholders map — template returned unchanged
- All placeholders substituted correctly
- Unknown placeholder in template — passes through (not an error)
- Template with literal `{` braces that aren't placeholders — left alone
- Same placeholder appears twice — both replaced

### B.2 Default SOAP prompt

The user's approved prompt text, with these substitutions relative to the verbatim version:
- `ICD-9 Code: [code]` → `{icd_label}`
- In the Assessment section's inline comment: `Include ICD-9 code inline` → `Include {icd_instruction} inline`
- New first line after the opening sentence: `{template_guidance}` on its own line, followed by a blank line

`{icd_label}` resolves to:
- `"ICD-10 Code: [code]"` (when setting = ICD-10, default)
- `"ICD-9 Code: [code]"` (when setting = ICD-9)
- `"ICD-9 Code: [code]\nICD-10 Code: [code]"` (when setting = both)

`{icd_instruction}` resolves to `"ICD-10 code"`, `"ICD-9 code"`, or `"both ICD-9 and ICD-10 codes"`.

`{template_guidance}` resolves to the current template's instruction string (FollowUp/NewPatient/Telehealth/Emergency/Pediatric/Geriatric) — those strings are already defined in `soap_generator.rs:76-101`.

When `template_guidance` would be empty, `resolve_prompt` still substitutes the empty string, leaving a blank line. This is acceptable.

### B.3 Referral / Letter / Synopsis default prompts

Current state in `crates/processing/src/document_generator.rs`:
- `build_referral_prompt(soap_note, recipient_type, urgency)` — interpolates `recipient_type` and `urgency` into a hardcoded system prompt string.
- `build_letter_prompt(soap_note, letter_type)` — interpolates `letter_type`.
- `build_synopsis_prompt(soap_note)` — no interpolation in the system prompt.

After refactor, each builder returns `(system_prompt, user_prompt)` where:
- **System prompt** = resolved template (custom override OR default, then substituted).
- **User prompt** = unchanged — carries the SOAP note and any additional dynamic content.

Default templates preserve the existing system-prompt text verbatim, replacing the interpolation points with placeholders:

| Doc | Placeholders |
|---|---|
| Referral | `{recipient_type}`, `{urgency}` |
| Letter | `{letter_type}` |
| Synopsis | *(none — empty placeholder map, template returned as-is)* |

### B.4 Settings schema changes

`crates/core/src/types/settings.rs`:

Add field (currently missing):
```rust
pub custom_synopsis_prompt: Option<String>,
```

Existing fields `custom_soap_prompt`, `custom_referral_prompt`, `custom_letter_prompt` stay.

`src/lib/types/index.ts` — add the three missing fields to `AppConfig`:
```typescript
custom_soap_prompt: string | null;
custom_referral_prompt: string | null;
custom_letter_prompt: string | null;
custom_synopsis_prompt: string | null;
```

(Only `custom_soap_prompt` exists in current AppConfig implicitly via `[key: string]: any` — make all four explicit.)

### B.5 Wire custom_referral_prompt / custom_letter_prompt through generators

Currently the `GenerationSettings` struct in `src-tauri/src/commands/generation.rs:37-43` only loads `custom_soap_prompt`. Extend to include `custom_referral_prompt`, `custom_letter_prompt`, and `custom_synopsis_prompt`. The referral/letter/synopsis generation commands currently don't accept a custom-prompt override; modify their prompt-builder calls to pass it through:

```rust
let (system, user) = build_referral_prompt(
    soap_note,
    recipient_type,
    urgency,
    settings.custom_referral_prompt.as_deref(), // new arg
);
```

`build_referral_prompt` signature gains a final `custom_template: Option<&str>` parameter; if `Some` and non-empty, it is used as the template, else the default. Same pattern for `build_letter_prompt` and `build_synopsis_prompt`.

### B.6 New Tauri command: `get_default_prompt`

For the "Reset to default" button in the UI to work, the frontend needs access to each doc type's default prompt text. Expose:

```rust
#[tauri::command]
pub fn get_default_prompt(doc_type: String) -> Result<String, String> {
    match doc_type.as_str() {
        "soap" => Ok(default_soap_prompt().to_string()),
        "referral" => Ok(default_referral_prompt().to_string()),
        "letter" => Ok(default_letter_prompt().to_string()),
        "synopsis" => Ok(default_synopsis_prompt().to_string()),
        _ => Err(format!("Unknown doc_type: {}", doc_type)),
    }
}
```

Also register in `lib.rs`'s `invoke_handler!` macro call.

Frontend API wrapper: `src/lib/api/prompts.ts` with `getDefaultPrompt(docType: 'soap' | 'referral' | 'letter' | 'synopsis'): Promise<string>`.

---

## Phase C — Prompts Tab UI

### C.1 Tab placement

`src/lib/components/SettingsContent.svelte` currently has tabs: General, API Keys (being removed), AI Models, Audio/STT, etc. Insert a new **Prompts** tab after AI Models.

### C.2 Layout

Two-column layout inside the tab:

```
┌──────────────┬──────────────────────────────────────────────┐
│ Doc types    │  SOAP Note                                   │
│  ▸ SOAP      │  ┌──────────────────────────────────────────┐│
│  ▸ Referral  │  │ [textarea with current template text]    ││
│    Letter    │  │                                          ││
│    Synopsis  │  │                                          ││
│              │  └──────────────────────────────────────────┘│
│              │                                              │
│              │  ▼ Available placeholders                    │
│              │    {icd_label}      — ICD code header line   │
│              │    {icd_instruction}— inline ICD reference   │
│              │    {template_guidance} — template hint       │
│              │                                              │
│              │  Status: Using default                       │
│              │  [Save as custom]  [Reset to default]        │
└──────────────┴──────────────────────────────────────────────┘
```

### C.3 State flow

1. On tab mount: load current AppConfig, for each doc type determine "default" vs "custom" based on whether the corresponding `custom_*_prompt` field is `null`/empty.
2. When user selects a doc type in the sidebar: load that doc's template into the textarea.
   - If `custom_*_prompt` is set: load the custom text, status = "Using custom"
   - If not set: call `get_default_prompt(doc_type)` to fetch the default text, status = "Using default"
3. User edits the textarea — no auto-save; tracks dirty state.
4. **Save as custom**: persist whatever text is in the textarea via the existing `save_config` command into the corresponding `custom_*_prompt` field. Status → "Using custom". (If the user saves text identical to the default, we still store it as a custom override; they can use "Reset to default" to clear.)
5. **Reset to default**: set `custom_*_prompt` to `null` in AppConfig, save, re-fetch default, populate textarea. Status → "Using default".
6. Confirm before switching doc types if current pane has unsaved edits.

### C.4 Placeholder info panel

Static per doc type, rendered as a collapsible `<details>` element. Contents:

- **SOAP**: `{icd_label}`, `{icd_instruction}`, `{template_guidance}`
- **Referral**: `{recipient_type}`, `{urgency}`
- **Letter**: `{letter_type}`
- **Synopsis**: *(no placeholders)*

Each entry shows the token and a one-line description of what it resolves to.

---

## Data Model Diff

### Rust (`crates/core/src/types/settings.rs`)

```diff
 pub struct SoapNoteSettings {
-    pub openai_model: String,
-    pub anthropic_model: String,
-    pub groq_model: String,
     pub icd_code_version: IcdVersion,
 }

 pub struct AppConfig {
     // ...
     pub custom_soap_prompt: Option<String>,
     pub custom_referral_prompt: Option<String>,
     pub custom_letter_prompt: Option<String>,
+    pub custom_synopsis_prompt: Option<String>,
     // ...
 }
```

### TypeScript (`src/lib/types/index.ts`)

```diff
 export interface AppConfig {
     // ...
+    custom_soap_prompt: string | null;
+    custom_referral_prompt: string | null;
+    custom_letter_prompt: string | null;
+    custom_synopsis_prompt: string | null;
     // ...
 }
```

### `SoapPromptConfig`

```diff
 pub struct SoapPromptConfig {
     pub template: SoapTemplate,
     pub icd_version: String,
     pub custom_prompt: Option<String>,
     pub include_context: bool,
-    pub provider: Option<String>,
 }
```

---

## Testing

### Backend (Rust)

- `prompt_resolver.rs` unit tests (5 cases as listed in B.1).
- `soap_generator::default_soap_prompt_contains_markers` — assert default prompt contains `{icd_label}`, `{template_guidance}`, `Clinical Synopsis`, `FORMATTING RULES`.
- `soap_generator::resolved_soap_prompt_substitutes_icd9` — build `SoapPromptConfig` with `icd_version: "ICD-9"`, assert resolved prompt contains `"ICD-9 Code: [code]"` and does not contain `{icd_label}`.
- `document_generator` tests for referral/letter/synopsis custom-prompt overrides (each: override wins over default, empty override falls back to default).
- `state::init_ai_providers_registers_only_local_providers` — verify registry has exactly `lmstudio` and `ollama`.
- `settings::stale_ai_provider_migrates_to_lmstudio` — load config with `ai_provider: "anthropic"`, assert it becomes `"lmstudio"`.

### Frontend (svelte-check + manual)

- `svelte-check` passes cleanly after SettingsContent.svelte edits.
- Manual: open Settings → Prompts tab → verify each of the 4 doc types loads its default text, edit/save, reopen, value persists.
- Manual: Reset to default clears the override and shows default text again.
- Manual: switching doc types with unsaved edits prompts for confirmation.

---

## Risks & Open Questions

- **Ollama embedding model availability**: RAG now requires Ollama to have an embedding model pulled (e.g., `nomic-embed-text`). If the user has never pulled one, RAG search will fail at ingest time. We log a helpful error but don't pre-check at startup. Acceptable for v1.
- **`{` in user-authored templates**: If a user writes a literal `{curly brace}` that isn't a known placeholder, it passes through unchanged (per B.1). Documented behavior.
- **Chat system prompt**: explicitly out of scope for this iteration. Can be added later using the same infrastructure.
- **SoapNoteSettings after field removal**: struct becomes a single-field wrapper around `icd_code_version`. Left as-is rather than inlining, since it mirrors a conceptual grouping and changing it has ripple effects in serialization.

---

## Approval

Awaiting user sign-off before proceeding to implementation plan.
