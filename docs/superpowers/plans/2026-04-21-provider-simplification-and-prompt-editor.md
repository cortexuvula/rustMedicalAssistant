# Provider Simplification and Prompt Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove all cloud AI providers (OpenAI, Anthropic, Gemini, Groq, Cerebras) leaving only LM Studio and Ollama. Add a Prompts tab to Settings for viewing and editing default system prompts for SOAP, Referral, Letter, and Synopsis, using placeholder-token substitution.

**Architecture:** Delete 5 provider modules. Simplify `ProviderRegistry` initialization. Replace dual-path SOAP prompt (`build_anthropic_prompt`/`build_generic_prompt`) with a single placeholder-driven template resolved by a new `prompt_resolver` helper. Wire `custom_referral_prompt`/`custom_letter_prompt`/`custom_synopsis_prompt` through generation commands. Add a Prompts tab UI with default-fetch and save/reset for each doc type.

**Tech Stack:** Rust (workspace crates: `medical-core`, `medical-ai-providers`, `medical-processing`, `medical-rag`), Tauri v2, Svelte 5 (runes: `$state`, `$derived`, `$effect`), TypeScript.

**Design spec:** `docs/superpowers/specs/2026-04-21-provider-simplification-and-prompt-editor-design.md`

**Pre-implementation note:** The working tree currently has an uncommitted partial edit to `crates/processing/src/soap_generator.rs` (from an earlier session — a simplified `build_anthropic_prompt()` with no placeholders). **Task 1 begins by reverting that file to the committed state** so the plan executes from a clean base.

---

## File Structure

**Files deleted:**
- `crates/ai-providers/src/anthropic.rs`
- `crates/ai-providers/src/openai.rs`
- `crates/ai-providers/src/gemini.rs`
- `crates/ai-providers/src/groq.rs`
- `crates/ai-providers/src/cerebras.rs`

**Files created:**
- `crates/processing/src/prompt_resolver.rs` — Placeholder substitution helper
- `src/lib/api/prompts.ts` — Frontend API wrapper for `get_default_prompt`

**Files heavily modified:**
- `crates/core/src/types/settings.rs` — Drop provider-specific model fields, add `custom_synopsis_prompt`, add stale-provider migration
- `crates/ai-providers/src/lib.rs` — Drop deleted module declarations
- `crates/processing/src/soap_generator.rs` — Replace dual-path prompt with placeholder-driven template
- `crates/processing/src/document_generator.rs` — Accept custom-template override per doc type
- `crates/processing/src/lib.rs` — Export `prompt_resolver`
- `crates/rag/src/embeddings.rs` — Remove OpenAI embedding backend
- `src-tauri/src/state.rs` — Simplify `init_ai_providers`, RAG always-Ollama
- `src-tauri/src/commands/providers.rs` — Update `reinit_providers` for new `init_ai_providers` signature
- `src-tauri/src/commands/generation.rs` — Remove `max_tokens_for_provider`, load new custom prompts, pass through to generators
- `src-tauri/src/commands/settings.rs` — Add new `get_default_prompt` command
- `src-tauri/src/lib.rs` — Register new command in `invoke_handler!`
- `src/lib/types/index.ts` — Add custom prompt fields
- `src/lib/components/SettingsContent.svelte` — Remove API Keys tab, add Prompts tab

---

## Task 1: Revert stale WIP + add stale-provider migration

**Files:**
- Revert: `crates/processing/src/soap_generator.rs` (uncommitted changes from previous session)
- Modify: `crates/core/src/types/settings.rs`
- Test: `crates/core/src/types/settings.rs` (tests module)

**Context:** Before touching provider-removal, we need (a) a clean soap_generator.rs as the starting point for Task 5's rewrite, and (b) a settings-load-time migration so that users who previously had `ai_provider: "anthropic"` don't crash after the providers are deleted. The migration lives on `AppConfig` as a `migrate()` method called after deserialization.

- [ ] **Step 1: Revert uncommitted soap_generator.rs changes**

```bash
git checkout crates/processing/src/soap_generator.rs
```

Verify:
```bash
git diff crates/processing/src/soap_generator.rs
```
Expected: no output (no diff).

- [ ] **Step 2: Write a failing test for stale-provider migration**

Open `crates/core/src/types/settings.rs`. In the `#[cfg(test)] mod tests` block (after the `context_templates_missing_from_json_defaults_empty` test near line 434), add:

```rust
#[test]
fn stale_ai_provider_migrates_to_lmstudio() {
    let json = r#"{"ai_provider": "anthropic"}"#;
    let mut config: AppConfig = serde_json::from_str(json).unwrap();
    config.migrate();
    assert_eq!(config.ai_provider, "lmstudio");
}

#[test]
fn valid_ai_provider_not_changed_by_migrate() {
    let json = r#"{"ai_provider": "ollama"}"#;
    let mut config: AppConfig = serde_json::from_str(json).unwrap();
    config.migrate();
    assert_eq!(config.ai_provider, "ollama");
}

#[test]
fn default_ai_provider_migrates_to_lmstudio() {
    // The default value is "openai" from default_ai_provider() — this must also migrate.
    let mut config = AppConfig::default();
    config.migrate();
    assert_eq!(config.ai_provider, "lmstudio");
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p medical-core --lib settings::tests::stale_ai_provider_migrates_to_lmstudio 2>&1 | tail -20
```
Expected: FAIL with `method migrate not found` or similar compile error.

- [ ] **Step 4: Implement `AppConfig::migrate()`**

In `crates/core/src/types/settings.rs`, after the `impl Default for AppConfig` block (around line 314), add:

```rust
impl AppConfig {
    /// Migrate deserialized config values to match the current supported set.
    ///
    /// Run after deserialization; silently corrects values that are no longer
    /// valid (e.g. cloud provider names left over from older versions).
    pub fn migrate(&mut self) {
        if !matches!(self.ai_provider.as_str(), "lmstudio" | "ollama") {
            tracing::warn!(
                stale = %self.ai_provider,
                "ai_provider migrated to 'lmstudio' (cloud providers are no longer supported)"
            );
            self.ai_provider = "lmstudio".into();
        }
    }
}
```

Also change the default AI provider so new installs start on a valid value:

```rust
fn default_ai_provider() -> String {
    "lmstudio".into()
}
```

This requires updating the existing `default_config_values` test (line 327) where `assert_eq!(config.ai_provider, "openai")` currently stands:

```rust
assert_eq!(config.ai_provider, "lmstudio");
```

Also update the `partial_json_deserialize` test (line 370-371): the incoming `"ai_provider": "anthropic"` is still a valid *deserialization* input (migrate() hasn't been called), so that assertion still works. Leave that test alone.

Add this import near the top of the file if not already present:

```rust
// Already present: use serde::{Deserialize, Serialize};
// Add nothing — tracing is accessed via fully qualified path.
```

The `medical-core` crate already has `tracing` as a dependency (verify with `cargo tree -p medical-core | grep tracing`); if not, add it via:

```bash
cargo add tracing -p medical-core
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p medical-core --lib settings 2>&1 | tail -30
```
Expected: all tests pass, including the 3 new migration tests.

- [ ] **Step 6: Call `migrate()` at load sites**

Callers of `load_config()` need to invoke `migrate()` on the returned config. Grep for the call sites:

```bash
grep -rn "load_config" src-tauri/src crates/db/src --include="*.rs" | head -20
```

For each call site that binds the result as a mutable variable, call `.migrate()` afterward. The primary call sites are:

**`src-tauri/src/state.rs:165-167`** — update to:
```rust
let config = {
    let conn = db.conn().ok();
    conn.and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
        .map(|mut c| { c.migrate(); c })
};
```

**`src-tauri/src/commands/providers.rs:14-19`** — update to:
```rust
let config = {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let mut cfg = medical_db::settings::SettingsRepo::load_config(&conn)
        .map_err(|e| e.to_string())?;
    cfg.migrate();
    cfg
};
```

Run `cargo check --workspace` to confirm clean compilation.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/types/settings.rs src-tauri/src/state.rs src-tauri/src/commands/providers.rs
git commit -m "$(cat <<'EOF'
feat(settings): add migrate() to fix stale cloud provider values

Users whose config saved ai_provider as "openai"/"anthropic"/etc. are
now silently migrated to "lmstudio" at load time. Default AI provider
for fresh installs is now "lmstudio".

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add the `prompt_resolver` module

**Files:**
- Create: `crates/processing/src/prompt_resolver.rs`
- Modify: `crates/processing/src/lib.rs`

**Context:** Templates contain `{placeholder_name}` tokens. At generation time we substitute each known placeholder with its resolved value. Unknown placeholders pass through unchanged (so users see their typo in the final prompt). This is a pure, standalone helper — no dependencies on other crates in the workspace.

- [ ] **Step 1: Write failing tests**

Create `crates/processing/src/prompt_resolver.rs`:

```rust
//! Placeholder substitution for user-editable prompt templates.
//!
//! Replaces `{key}` tokens in a template with values from a placeholder map.
//! Unknown tokens pass through unchanged so typos remain visible to the user.

use std::collections::HashMap;

/// Substitute `{key}` tokens in `template` with values from `placeholders`.
pub fn resolve_prompt(template: &str, placeholders: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in placeholders {
        let token = format!("{{{}}}", key);
        out = out.replace(&token, value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_placeholders_returns_template_unchanged() {
        let tmpl = "Hello {name}, you are {age}.";
        let result = resolve_prompt(tmpl, &HashMap::new());
        assert_eq!(result, "Hello {name}, you are {age}.");
    }

    #[test]
    fn substitutes_all_known_placeholders() {
        let tmpl = "Referral to {recipient_type} with {urgency} urgency.";
        let mut map = HashMap::new();
        map.insert("recipient_type", "Cardiologist".into());
        map.insert("urgency", "routine".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Referral to Cardiologist with routine urgency.");
    }

    #[test]
    fn unknown_placeholder_passes_through() {
        let tmpl = "Hello {name}, {missing_token} should stay.";
        let mut map = HashMap::new();
        map.insert("name", "Alice".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Hello Alice, {missing_token} should stay.");
    }

    #[test]
    fn literal_braces_without_known_keys_stay() {
        let tmpl = "Use { like this } is fine.";
        let mut map = HashMap::new();
        map.insert("name", "Alice".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Use { like this } is fine.");
    }

    #[test]
    fn same_placeholder_replaced_multiple_times() {
        let tmpl = "{name} went to see {name}.";
        let mut map = HashMap::new();
        map.insert("name", "Bob".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Bob went to see Bob.");
    }

    #[test]
    fn empty_value_substituted_correctly() {
        let tmpl = "Start\n{optional_line}\nEnd";
        let mut map = HashMap::new();
        map.insert("optional_line", "".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Start\n\nEnd");
    }
}
```

- [ ] **Step 2: Export the module from the crate**

In `crates/processing/src/lib.rs`, add:

```rust
pub mod prompt_resolver;
```

(Insert alphabetically with other `pub mod` declarations.)

- [ ] **Step 3: Run tests to verify all 6 pass**

```bash
cargo test -p medical-processing --lib prompt_resolver 2>&1 | tail -20
```
Expected: `test result: ok. 6 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/processing/src/prompt_resolver.rs crates/processing/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(processing): add prompt_resolver for placeholder substitution

Provides resolve_prompt() that replaces {key} tokens in templates with
values from a map. Unknown tokens pass through unchanged.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Strip OpenAI embedding backend from RAG

**Files:**
- Modify: `crates/rag/src/embeddings.rs`
- Modify: `src-tauri/src/state.rs:184-192`

**Context:** The RAG embedding generator currently supports OpenAI (via `new_openai`) or Ollama (via `new_ollama`). State initialization prefers OpenAI when a key is present. Removing cloud providers means RAG is Ollama-only. The `EmbeddingBackend::OpenAi` variant and `new_openai` constructor come out; the struct's `backend` field becomes a single-variant wrapper — simplify to store host/model directly.

- [ ] **Step 1: Write a failing test verifying Ollama is the only backend**

In `crates/rag/src/embeddings.rs`, within the existing `#[cfg(test)] mod tests` block (near lines 245-275), add at the end:

```rust
#[test]
fn ollama_is_the_only_constructor() {
    // Compile-time check: this builds only if new_openai has been removed.
    let _ = EmbeddingGenerator::new_ollama(None, None);
    // If this test compiles, the simplification is complete.
}
```

- [ ] **Step 2: Run existing tests to confirm current state**

```bash
cargo test -p medical-rag --lib embeddings 2>&1 | tail -10
```
Expected: passing (we haven't removed anything yet).

- [ ] **Step 3: Remove the OpenAI branch from `embeddings.rs`**

In `crates/rag/src/embeddings.rs`:

Replace the `enum EmbeddingBackend` (lines 5-9) and surrounding OpenAI request/response structs (lines 22-36) by deleting them entirely. Simplify `EmbeddingGenerator`:

```rust
use medical_core::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// HTTP-backed embedding generator using Ollama.
pub struct EmbeddingGenerator {
    client: Client,
    host: String,
    model: String,
    dim: usize,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct OllamaResponse {
    embedding: Vec<f32>,
}

impl EmbeddingGenerator {
    /// Create a generator backed by a local Ollama instance.
    ///
    /// Defaults to `http://localhost:11434` and the `nomic-embed-text` model (768 dims).
    pub fn new_ollama(host: Option<&str>, model: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            host: host.unwrap_or("http://localhost:11434").to_owned(),
            model: model.unwrap_or("nomic-embed-text").to_owned(),
            dim: 768,
        }
    }

    /// The dimensionality of the vectors produced by this generator.
    pub fn dimension(&self) -> usize {
        self.dim
    }

    /// Generate an embedding for a single text.
    pub async fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        let body = OllamaRequest {
            model: &self.model,
            prompt: text,
        };
        let url = format!("{}/api/embeddings", self.host);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(format!("Ollama request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!(
                "Ollama API error {status}: {body_text}"
            )));
        }

        let parsed: OllamaResponse = resp
            .json()
            .await
            .map_err(|e| AppError::AiProvider(format!("Ollama response parse error: {e}")))?;

        Ok(parsed.embedding)
    }
}
```

Also remove the existing test at line 250 that constructs `new_openai("sk-test-key")` — delete that test function entirely. Keep the Ollama-constructor tests (lines 256, 269).

- [ ] **Step 4: Update `src-tauri/src/state.rs` RAG init**

Replace lines 184-192:

```rust
        // --- RAG subsystem ---
        // Create the embedding generator: prefer OpenAI if key exists, else Ollama
        let embedding_generator = if let Ok(Some(key)) = keys.get_key("openai") {
            info!("RAG: using OpenAI embeddings");
            Arc::new(EmbeddingGenerator::new_openai(&key))
        } else {
            info!("RAG: using Ollama embeddings (local)");
            Arc::new(EmbeddingGenerator::new_ollama(None, None))
        };
```

With:

```rust
        // --- RAG subsystem ---
        info!("RAG: using Ollama embeddings (local)");
        let embedding_generator = Arc::new(EmbeddingGenerator::new_ollama(None, None));
```

- [ ] **Step 5: Build and run tests**

```bash
cargo test -p medical-rag --lib 2>&1 | tail -20
```
Expected: passing including the new compile-time check test.

```bash
cargo check -p rust-medical-assistant-lib 2>&1 | tail -20
```
Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add crates/rag/src/embeddings.rs src-tauri/src/state.rs
git commit -m "$(cat <<'EOF'
refactor(rag): drop OpenAI embedding backend, Ollama-only

RAG now uses Ollama for embeddings exclusively. Users must have an
embedding model pulled in Ollama (default: nomic-embed-text).

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Delete cloud provider modules

**Files:**
- Delete: `crates/ai-providers/src/{anthropic,openai,gemini,groq,cerebras}.rs`
- Modify: `crates/ai-providers/src/lib.rs`
- Modify: `src-tauri/src/state.rs` (imports + `init_ai_providers` body)
- Modify: `src-tauri/src/commands/providers.rs` (update `init_ai_providers` call)

**Context:** The 5 cloud provider modules come out. `ProviderRegistry::register` signature stays the same. `init_ai_providers` loses the `keys` parameter since it's no longer needed (both remaining providers are keyless). All call sites update.

- [ ] **Step 1: Delete the 5 provider source files**

```bash
rm crates/ai-providers/src/anthropic.rs \
   crates/ai-providers/src/openai.rs \
   crates/ai-providers/src/gemini.rs \
   crates/ai-providers/src/groq.rs \
   crates/ai-providers/src/cerebras.rs
```

- [ ] **Step 2: Update `crates/ai-providers/src/lib.rs`**

Remove these lines (currently lines 4-8):
```rust
pub mod openai;
pub mod anthropic;
pub mod gemini;
pub mod groq;
pub mod cerebras;
```

The final `pub mod` list becomes:
```rust
pub mod http_client;
pub mod sse;
pub mod openai_compat;
pub mod ollama;
pub mod lmstudio;
```

- [ ] **Step 3: Verify ai-providers crate builds**

```bash
cargo check -p medical-ai-providers 2>&1 | tail -10
```
Expected: clean compile (crate has no other references to deleted modules).

- [ ] **Step 4: Update `src-tauri/src/state.rs`**

Remove the 5 unused imports (lines 7-11):
```rust
use medical_ai_providers::openai::OpenAiProvider;
use medical_ai_providers::anthropic::AnthropicProvider;
use medical_ai_providers::gemini::GeminiProvider;
use medical_ai_providers::groq::GroqProvider;
use medical_ai_providers::cerebras::CerebrasProvider;
```

Replace the entire `pub fn init_ai_providers(...)` function (lines 80-126) with:

```rust
/// Register all supported AI providers (LM Studio + Ollama).
///
/// Both providers are local and keyless; `config` supplies LM Studio's
/// host/port.
pub fn init_ai_providers(config: &AppConfig) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    // Ollama — always available (local, no key needed)
    info!("Registering Ollama provider (local)");
    registry.register(Arc::new(OllamaProvider::new(None)));

    // LM Studio — always available (local or remote, no key needed)
    let lmstudio_host = if config.lmstudio_host.is_empty() { "localhost" } else { &config.lmstudio_host };
    let lmstudio_url = format!("http://{}:{}", lmstudio_host, config.lmstudio_port);
    info!(url = %lmstudio_url, "Registering LM Studio provider");
    registry.register(Arc::new(LmStudioProvider::new(Some(&lmstudio_url))));

    info!("AI providers available: {:?}", registry.list_available());
    registry
}
```

Also update the single call site inside `AppState::initialize()` (line 170):

```rust
// Before:
let mut ai_providers = init_ai_providers(&keys, &config_ref);

// After:
let mut ai_providers = init_ai_providers(&config_ref);
```

- [ ] **Step 5: Update `src-tauri/src/commands/providers.rs`**

At line 22, change:
```rust
let mut ai_registry = state::init_ai_providers(&state.keys, &config);
```
To:
```rust
let mut ai_registry = state::init_ai_providers(&config);
```

Also update the doc comment at lines 7-9: change "Re-read API keys from storage and rebuild AI + STT provider registries" to "Rebuild AI + STT provider registries (e.g. after LM Studio host/port changes)".

- [ ] **Step 6: Build the full workspace**

```bash
cargo check --workspace 2>&1 | tail -20
```
Expected: clean compile.

```bash
cargo test --workspace 2>&1 | tail -5
```
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(providers): remove cloud AI provider modules

Deletes OpenAI, Anthropic, Gemini, Groq, and Cerebras provider
implementations. Only LM Studio and Ollama remain — both keyless and
local. init_ai_providers no longer takes a KeyStorage reference.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Refactor SOAP prompt with placeholders

**Files:**
- Modify: `crates/processing/src/soap_generator.rs`
- Test: `crates/processing/src/soap_generator.rs` (tests module)

**Context:** Replace the dual-path prompt (`build_anthropic_prompt`/`build_generic_prompt`) with a single `default_soap_prompt()` containing the user's approved prompt text, with three placeholder tokens inserted. Add a helper `soap_placeholders(icd_version, template)` that builds the substitution map. The dispatch in `build_soap_prompt` collapses to: pick template (custom or default), resolve placeholders, return.

The `SoapPromptConfig.provider` field is removed (no longer needed — only one path).

- [ ] **Step 1: Write failing tests for the new shape**

Replace the existing `anthropic_prompt_has_example` test in `crates/processing/src/soap_generator.rs` (near line 642) with these new tests. Also update `default_prompt_includes_extraction_requirements` since its assertions (EXTRACTION REQUIREMENTS, QUALITY VERIFICATION) won't exist in the new prompt.

```rust
    #[test]
    fn default_soap_prompt_has_structure_markers() {
        let config = SoapPromptConfig::default();
        let prompt = build_soap_prompt(&config);
        // Core section markers
        assert!(prompt.contains("Subjective"));
        assert!(prompt.contains("Objective"));
        assert!(prompt.contains("Assessment"));
        assert!(prompt.contains("Differential Diagnosis"));
        assert!(prompt.contains("Plan"));
        assert!(prompt.contains("Follow up"));
        assert!(prompt.contains("Clinical Synopsis"));
        // Rules section
        assert!(prompt.contains("RULES:"));
        assert!(prompt.contains("FORMATTING RULES"));
    }

    #[test]
    fn default_soap_prompt_resolves_icd9() {
        let config = SoapPromptConfig {
            icd_version: "ICD-9".into(),
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("ICD-9 Code: [code]"));
        assert!(!prompt.contains("{icd_label}"));
        assert!(!prompt.contains("{icd_instruction}"));
    }

    #[test]
    fn default_soap_prompt_resolves_icd10() {
        let config = SoapPromptConfig {
            icd_version: "ICD-10".into(),
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("ICD-10 Code: [code]"));
    }

    #[test]
    fn default_soap_prompt_resolves_both_icd() {
        let config = SoapPromptConfig {
            icd_version: "both".into(),
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("ICD-9 Code: [code]"));
        assert!(prompt.contains("ICD-10 Code: [code]"));
    }

    #[test]
    fn default_soap_prompt_includes_template_guidance() {
        let config = SoapPromptConfig {
            template: SoapTemplate::NewPatient,
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("comprehensive history"));
    }

    #[test]
    fn custom_soap_prompt_overrides_default() {
        let config = SoapPromptConfig {
            custom_prompt: Some("My custom template with {icd_label}".into()),
            icd_version: "ICD-9".into(),
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        // Custom template is used, and placeholders are still resolved
        assert!(prompt.starts_with("My custom template with ICD-9 Code: [code]"));
    }

    #[test]
    fn empty_custom_prompt_falls_back_to_default() {
        let config = SoapPromptConfig {
            custom_prompt: Some("".into()),
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        // Empty string should not be treated as a real custom prompt
        assert!(prompt.contains("You are a physician creating a SOAP note"));
    }
```

Also delete the existing `anthropic_prompt_has_example` test (no longer applicable), the `default_prompt_includes_extraction_requirements` test (its assertions don't match the new prompt), and the `custom_prompt_overrides` test at line 654-661 (replaced by `custom_soap_prompt_overrides_default` above — but **keep** the assertion that an empty `custom_prompt` falls back, covered by the new `empty_custom_prompt_falls_back_to_default` test).

Keep `template_specific_instructions` (lines 664-702) but it needs to assert against the resolved placeholder output, which now lives in the prompt text. It should still pass because the `template_instruction` strings are substituted into the template — verify by re-reading after implementation.

- [ ] **Step 2: Run tests to verify they fail to compile**

```bash
cargo test -p medical-processing --lib soap_generator::tests 2>&1 | tail -20
```
Expected: compile errors referencing missing default text / missing methods.

- [ ] **Step 3: Rewrite `soap_generator.rs`**

Replace the entire file content from line 1 through the end of `build_anthropic_prompt` / `build_generic_prompt` (roughly lines 1-385) with a placeholder-driven version. The rest of the file below those functions (post-processing, sanitization, tests) stays.

New top-of-file content (replace lines 1-107):

```rust
//! System and user prompt builders for SOAP note generation.
//!
//! The system prompt uses a default template with placeholder tokens
//! (`{icd_label}`, `{icd_instruction}`, `{template_guidance}`). A user-supplied
//! `custom_prompt` overrides the default template; placeholders in either are
//! resolved at generation time via `prompt_resolver::resolve_prompt`.

use std::collections::HashMap;

use chrono::Local;
use medical_core::types::settings::SoapTemplate;

use crate::prompt_resolver::resolve_prompt;

// ---------------------------------------------------------------------------
// Public config
// ---------------------------------------------------------------------------

/// Inputs to `build_soap_prompt`.
#[derive(Debug, Clone)]
pub struct SoapPromptConfig {
    pub template: SoapTemplate,
    /// One of "ICD-9", "ICD-10", "both" (case-sensitive).
    pub icd_version: String,
    /// User-supplied override; empty string is treated as absent.
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

// ---------------------------------------------------------------------------
// Placeholder resolution
// ---------------------------------------------------------------------------

/// Build the placeholder map for the SOAP template.
fn soap_placeholders(icd_version: &str, template: &SoapTemplate) -> HashMap<&'static str, String> {
    let (icd_instruction, icd_label) = icd_code_parts(icd_version);
    let template_guidance = template_guidance_text(template);

    let mut map = HashMap::new();
    map.insert("icd_instruction", icd_instruction.to_string());
    map.insert("icd_label", icd_label.to_string());
    map.insert("template_guidance", template_guidance.to_string());
    map
}

fn icd_code_parts(version: &str) -> (&'static str, &'static str) {
    match version {
        "ICD-9" => ("ICD-9 code", "ICD-9 Code: [code]"),
        "both" => (
            "both ICD-9 and ICD-10 codes",
            "ICD-9 Code: [code]\nICD-10 Code: [code]",
        ),
        _ => ("ICD-10 code", "ICD-10 Code: [code]"),
    }
}

fn template_guidance_text(template: &SoapTemplate) -> &'static str {
    match template {
        SoapTemplate::FollowUp => {
            "Focus on changes since last visit, interval history, and response to current treatment plan."
        }
        SoapTemplate::NewPatient => {
            "Provide comprehensive history including past medical history, family history, social history, and review of systems."
        }
        SoapTemplate::Telehealth => {
            "Note the limitations of remote examination. Document what was assessed virtually and any elements requiring in-person follow-up."
        }
        SoapTemplate::Emergency => {
            "Prioritise acute findings. Document chief complaint, vital signs, acute interventions, and disposition."
        }
        SoapTemplate::Pediatric => {
            "Include developmental milestones, immunisation status, growth parameters, and age-appropriate screening."
        }
        SoapTemplate::Geriatric => {
            "Address functional status, fall risk assessment, polypharmacy review, cognitive screening, and social support."
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// The built-in default SOAP system prompt.
///
/// Contains three placeholder tokens: `{template_guidance}`, `{icd_label}`,
/// and `{icd_instruction}`, resolved by `build_soap_prompt`.
pub fn default_soap_prompt() -> &'static str {
    r#"You are a physician creating a SOAP note from a patient consultation transcript.

{template_guidance}

RULES:

1. NEVER fabricate, infer, or assume clinical details not in the transcript. If something was not discussed, write "Not discussed."
2. The transcript is the sole source of truth. Every clinical finding, symptom, medication, and diagnosis must be directly traceable to something said during the visit.
3. Do NOT use medical knowledge to add details the physician did not mention.
4. If supplementary background is provided, it is secondary. Use it only for past history context. Never let it override the transcript. If context conflicts with transcript, prefer the transcript. Conditions or medications from background only (not transcript) go under past history only, never in Assessment or Plan.
5. Say "the patient" — never use names.
6. Replace "VML" with "Valley Medical Laboratories."

OUTPUT FORMAT — plain text only, no markdown:

{icd_label}

Subjective:
- Chief complaint: [from transcript]
- History of present illness: [from transcript]
- Past medical history: [from transcript or background]
- Surgical history: [from transcript or "Not discussed"]
- Current medications:
  - [medication 1]
  - [medication 2]
- Allergies: [from transcript or "Not discussed"]
- Family history: [from transcript or "Not discussed"]
- Social history: [from transcript or "Not discussed"]
- Review of systems: [from transcript or "Not performed"]

Objective:
- [Visit type, e.g., telehealth or in-person]
- Vital signs: [from transcript or "Not recorded"]
- General appearance: [from transcript]
- Physical examination: [from transcript or "limited due to telehealth format"]
- Laboratory results: [from transcript or "No new labs discussed"]
- Imaging: [from transcript or "No imaging discussed"]

Assessment:
- [ONE cohesive paragraph summarizing diagnoses, clinical status, and reasoning. Include {icd_instruction} inline. Not broken into sub-items.]

Differential Diagnosis:
- [Only diagnoses explicitly discussed during the visit. If none discussed: "- No differential diagnoses were discussed during the visit"]

Plan:
- [Each intervention as a separate dash line]

Follow up:
- [Follow-up timeline and instructions]
- [Seek urgent care for: specific red flags from transcript]
- [Return sooner if: conditions from transcript]

Clinical Synopsis:
- [One-paragraph summary of visit. Output this exactly once, at the very end.]

FORMATTING RULES:
- Every content line starts with dash (-)
- Include ALL categories even if "Not discussed"
- One blank line between sections
- Assessment is ONE paragraph, not sub-items
- No decorative characters (no ===, ---, ***, ##)
- Plain text section headers followed by colon"#
}

/// Build the SOAP system prompt: select template (custom or default), then resolve placeholders.
pub fn build_soap_prompt(config: &SoapPromptConfig) -> String {
    let template = config
        .custom_prompt
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_soap_prompt());

    let placeholders = soap_placeholders(&config.icd_version, &config.template);
    resolve_prompt(template, &placeholders)
}
```

The rest of the file (from `// Pre-processing` section onward — the `MAX_PROMPT_LENGTH`, `build_user_prompt`, sanitization helpers, `clean_text`, `format_soap_paragraphs`, `postprocess_soap`) stays unchanged. The old `build_generic_prompt` and `build_anthropic_prompt` functions are removed entirely.

Tests at the bottom of the file: remove the now-obsolete ones (`default_prompt_includes_extraction_requirements`, `anthropic_prompt_has_example`, `custom_prompt_overrides`, `icd_9_variant`, `both_icd_variant`) and add the new tests from Step 1. **Keep** `template_specific_instructions`, `user_prompt_includes_datetime`, `user_prompt_with_context`, the sanitize_* tests, and the `format_*`/`postprocess_*` tests.

Note: the existing `icd_9_variant` and `both_icd_variant` tests (if present) are replaced by the new `default_soap_prompt_resolves_icd9`, `_icd10`, `_both_icd` tests.

- [ ] **Step 4: Run tests**

```bash
cargo test -p medical-processing --lib soap_generator 2>&1 | tail -30
```
Expected: all tests pass. If `template_specific_instructions` fails because its assertion strings have changed, update them to match the `template_guidance_text` strings above.

- [ ] **Step 5: Update callers of `SoapPromptConfig`**

The `provider` field is gone. Grep for call sites:

```bash
grep -rn "SoapPromptConfig" src-tauri/src crates --include="*.rs"
```

In `src-tauri/src/commands/generation.rs` around line 247 (the call constructing `SoapPromptConfig` in `generate_soap`), remove the `provider: Some(settings.ai_provider.clone())` line from the struct literal if present.

- [ ] **Step 6: Run workspace build**

```bash
cargo check --workspace 2>&1 | tail -10
```
Expected: clean.

```bash
cargo test --workspace 2>&1 | grep -E "FAILED|test result" | head -5
```
Expected: all passing.

- [ ] **Step 7: Commit**

```bash
git add crates/processing/src/soap_generator.rs src-tauri/src/commands/generation.rs
git commit -m "$(cat <<'EOF'
refactor(prompts): SOAP prompt is now placeholder-driven

Replaces the dual build_anthropic_prompt/build_generic_prompt split
with a single default_soap_prompt() exposing {icd_label},
{icd_instruction}, and {template_guidance} tokens. Custom overrides
also benefit from placeholder substitution.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Refactor document_generator for custom-template overrides

**Files:**
- Modify: `crates/processing/src/document_generator.rs`
- Modify: `crates/core/src/types/settings.rs` (add `custom_synopsis_prompt`)

**Context:** The three builders in `document_generator.rs` currently interpolate `recipient_type`/`urgency`/`letter_type` directly via `format!`. We convert them to placeholder-driven templates that accept an optional custom override. Each builder gets a new final parameter: `custom_template: Option<&str>`. Default templates preserve the current text verbatim, with interpolation points replaced by `{placeholder}` tokens.

Also add the missing `custom_synopsis_prompt` field to `AppConfig`.

- [ ] **Step 1: Add `custom_synopsis_prompt` field to AppConfig**

In `crates/core/src/types/settings.rs`, in the AppConfig struct near line 269 (after `custom_letter_prompt`):

```rust
    #[serde(default)]
    pub custom_synopsis_prompt: Option<String>,
```

- [ ] **Step 2: Write failing tests for new builder signatures**

In `crates/processing/src/document_generator.rs`, replace the existing `#[cfg(test)] mod tests` block with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_default_contains_recipient_and_urgency() {
        let soap = "S: Chest pain\nO: BP 140/90\nA: Hypertension\nP: Refer to Cardiology";
        let (system, user) = build_referral_prompt(soap, "Cardiologist", "urgent", None);

        assert!(system.contains("Cardiologist"));
        assert!(system.contains("urgent"));
        assert!(!system.contains("{recipient_type}"));
        assert!(!system.contains("{urgency}"));
        assert!(user.contains("Chest pain"));
    }

    #[test]
    fn referral_custom_template_overrides() {
        let soap = "S: foo";
        let custom = "CUSTOM: Refer to {recipient_type} ({urgency})";
        let (system, _user) = build_referral_prompt(soap, "Neurology", "routine", Some(custom));

        assert!(system.starts_with("CUSTOM: Refer to Neurology (routine)"));
    }

    #[test]
    fn referral_empty_custom_falls_back_to_default() {
        let soap = "S: foo";
        let (system, _user) = build_referral_prompt(soap, "Derm", "routine", Some(""));
        assert!(system.contains("professional referral letters"));
    }

    #[test]
    fn letter_default_contains_type() {
        let soap = "S: Anxiety\nO: HR 90\nA: GAD\nP: CBT referral";
        let (system, user) = build_letter_prompt(soap, "results", None);

        assert!(system.contains("results"));
        assert!(!system.contains("{letter_type}"));
        assert!(user.contains("Anxiety"));
    }

    #[test]
    fn letter_custom_template_overrides() {
        let soap = "S: foo";
        let custom = "CUSTOM: {letter_type} letter";
        let (system, _user) = build_letter_prompt(soap, "follow-up", Some(custom));
        assert!(system.starts_with("CUSTOM: follow-up letter"));
    }

    #[test]
    fn synopsis_default_mentions_word_limit() {
        let soap = "S: Patient reports fatigue\nO: Haemoglobin 9.0\nA: Iron deficiency anaemia";
        let (system, user) = build_synopsis_prompt(soap, None);
        assert!(system.contains("200 words") || system.contains("200-word"));
        assert!(user.contains("Iron deficiency anaemia"));
    }

    #[test]
    fn synopsis_custom_template_overrides() {
        let soap = "S: foo";
        let (system, _user) = build_synopsis_prompt(soap, Some("CUSTOM SYNOPSIS"));
        assert!(system.starts_with("CUSTOM SYNOPSIS"));
    }
}
```

- [ ] **Step 3: Run tests to verify failure**

```bash
cargo test -p medical-processing --lib document_generator 2>&1 | tail -20
```
Expected: compile errors about argument count mismatch.

- [ ] **Step 4: Rewrite `document_generator.rs`**

Replace the file entirely with:

```rust
//! Prompt builders for referral letters, patient correspondence, and synopses.
//!
//! Each builder accepts an optional custom template override; placeholders
//! (`{recipient_type}`, `{urgency}`, `{letter_type}`) are resolved by
//! `prompt_resolver::resolve_prompt`.

use std::collections::HashMap;

use crate::prompt_resolver::resolve_prompt;

// ---------------------------------------------------------------------------
// Default templates
// ---------------------------------------------------------------------------

pub fn default_referral_prompt() -> &'static str {
    "You are a medical scribe assistant specialising in professional referral letters. \
     Write a formal referral letter addressed to a {recipient_type}. \
     The urgency of this referral is: {urgency}. \
     Use appropriate clinical language, include relevant history and findings from the SOAP \
     note, clearly state the reason for referral, and request the desired action. \
     Format the letter professionally with greeting, body, and closing."
}

pub fn default_letter_prompt() -> &'static str {
    "You are a medical scribe assistant helping to write patient-friendly correspondence. \
     Generate a {letter_type} letter for the patient. \
     Use clear, plain language the patient can understand. \
     Avoid unexplained medical jargon. \
     Be empathetic and professional."
}

pub fn default_synopsis_prompt() -> &'static str {
    "You are a medical scribe assistant. Summarise the provided SOAP note in a \
     concise synopsis of no more than 200 words. \
     Capture the key subjective complaints, objective findings, primary diagnosis, \
     and treatment plan. \
     Write in clear, professional language suitable for a quick clinical overview."
}

// ---------------------------------------------------------------------------
// Referral letter
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating a referral letter.
pub fn build_referral_prompt(
    soap_note: &str,
    recipient_type: &str,
    urgency: &str,
    custom_template: Option<&str>,
) -> (String, String) {
    let template = custom_template
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_referral_prompt());

    let mut placeholders = HashMap::new();
    placeholders.insert("recipient_type", recipient_type.to_string());
    placeholders.insert("urgency", urgency.to_string());

    let system = resolve_prompt(template, &placeholders);

    let user = format!(
        "Please write a referral letter to a {recipient_type} with {urgency} urgency based on \
         the following SOAP note:\n\n{soap_note}",
        recipient_type = recipient_type,
        urgency = urgency,
        soap_note = soap_note,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Patient letter
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating patient correspondence.
pub fn build_letter_prompt(
    soap_note: &str,
    letter_type: &str,
    custom_template: Option<&str>,
) -> (String, String) {
    let template = custom_template
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_letter_prompt());

    let mut placeholders = HashMap::new();
    placeholders.insert("letter_type", letter_type.to_string());

    let system = resolve_prompt(template, &placeholders);

    let user = format!(
        "Please write a {letter_type} letter for the patient based on the following SOAP \
         note:\n\n{soap_note}",
        letter_type = letter_type,
        soap_note = soap_note,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Synopsis
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating a brief SOAP synopsis.
pub fn build_synopsis_prompt(
    soap_note: &str,
    custom_template: Option<&str>,
) -> (String, String) {
    let template = custom_template
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_synopsis_prompt());

    // Synopsis template has no placeholders; pass empty map.
    let system = resolve_prompt(template, &HashMap::new());

    let user = format!(
        "Please summarise the following SOAP note in under 200 words:\n\n{soap_note}",
        soap_note = soap_note,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_default_contains_recipient_and_urgency() {
        let soap = "S: Chest pain\nO: BP 140/90\nA: Hypertension\nP: Refer to Cardiology";
        let (system, user) = build_referral_prompt(soap, "Cardiologist", "urgent", None);

        assert!(system.contains("Cardiologist"));
        assert!(system.contains("urgent"));
        assert!(!system.contains("{recipient_type}"));
        assert!(!system.contains("{urgency}"));
        assert!(user.contains("Chest pain"));
    }

    #[test]
    fn referral_custom_template_overrides() {
        let soap = "S: foo";
        let custom = "CUSTOM: Refer to {recipient_type} ({urgency})";
        let (system, _user) = build_referral_prompt(soap, "Neurology", "routine", Some(custom));

        assert!(system.starts_with("CUSTOM: Refer to Neurology (routine)"));
    }

    #[test]
    fn referral_empty_custom_falls_back_to_default() {
        let soap = "S: foo";
        let (system, _user) = build_referral_prompt(soap, "Derm", "routine", Some(""));
        assert!(system.contains("professional referral letters"));
    }

    #[test]
    fn letter_default_contains_type() {
        let soap = "S: Anxiety\nO: HR 90\nA: GAD\nP: CBT referral";
        let (system, user) = build_letter_prompt(soap, "results", None);

        assert!(system.contains("results"));
        assert!(!system.contains("{letter_type}"));
        assert!(user.contains("Anxiety"));
    }

    #[test]
    fn letter_custom_template_overrides() {
        let soap = "S: foo";
        let custom = "CUSTOM: {letter_type} letter";
        let (system, _user) = build_letter_prompt(soap, "follow-up", Some(custom));
        assert!(system.starts_with("CUSTOM: follow-up letter"));
    }

    #[test]
    fn synopsis_default_mentions_word_limit() {
        let soap = "S: Patient reports fatigue\nO: Haemoglobin 9.0\nA: Iron deficiency anaemia";
        let (system, user) = build_synopsis_prompt(soap, None);
        assert!(system.contains("200 words") || system.contains("200-word"));
        assert!(user.contains("Iron deficiency anaemia"));
    }

    #[test]
    fn synopsis_custom_template_overrides() {
        let soap = "S: foo";
        let (system, _user) = build_synopsis_prompt(soap, Some("CUSTOM SYNOPSIS"));
        assert!(system.starts_with("CUSTOM SYNOPSIS"));
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p medical-processing --lib document_generator 2>&1 | tail -20
```
Expected: 7 passing.

- [ ] **Step 6: Fix callers in `src-tauri/src/commands/generation.rs`**

The three generation commands call the old 3-arg/2-arg/1-arg signatures. Update the call sites (approximate line numbers):

- `generate_referral` around line 391:
  ```rust
  let (system, user) = build_referral_prompt(soap_note, recipient_type, urgency, None);
  ```
  *(We add the `None` for now; Task 7 wires the real custom_prompt through.)*

- `generate_letter` around line 492:
  ```rust
  let (system, user) = build_letter_prompt(soap_note, letter_type, None);
  ```

- `generate_synopsis` around line 545:
  ```rust
  let (system, user) = build_synopsis_prompt(soap_note, None);
  ```

- [ ] **Step 7: Workspace build**

```bash
cargo check --workspace && cargo test --workspace 2>&1 | grep -E "FAILED|error" | head
```
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add crates/processing/src/document_generator.rs crates/core/src/types/settings.rs src-tauri/src/commands/generation.rs
git commit -m "$(cat <<'EOF'
refactor(prompts): accept custom-template override for referral/letter/synopsis

Each document builder now takes Option<&str> for a user-supplied
template, with placeholder-token resolution. Adds custom_synopsis_prompt
field to AppConfig.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Wire custom prompts through generation commands

**Files:**
- Modify: `src-tauri/src/commands/generation.rs`

**Context:** Currently `GenerationSettings` only loads `custom_soap_prompt`. Extend it to load all four custom prompt fields, then pass them through to the respective builders. Also remove the now-dead `max_tokens_for_provider` helper.

- [ ] **Step 1: Read current `GenerationSettings` struct**

Around line 37:
```rust
struct GenerationSettings {
    model: String,
    temperature: f32,
    icd_version: String,
    ai_provider: String,
    custom_soap_prompt: Option<String>,
}
```

- [ ] **Step 2: Extend GenerationSettings**

Add three fields:

```rust
struct GenerationSettings {
    model: String,
    temperature: f32,
    icd_version: String,
    ai_provider: String,
    custom_soap_prompt: Option<String>,
    custom_referral_prompt: Option<String>,
    custom_letter_prompt: Option<String>,
    custom_synopsis_prompt: Option<String>,
}
```

Find where `GenerationSettings` is constructed (likely in a loader function like `load_recording_and_settings`) and populate the new fields from `config`:

```rust
custom_referral_prompt: config.custom_referral_prompt.clone(),
custom_letter_prompt: config.custom_letter_prompt.clone(),
custom_synopsis_prompt: config.custom_synopsis_prompt.clone(),
```

- [ ] **Step 3: Pass the custom prompts at the call sites**

- `generate_referral` (around line 391):
  ```rust
  let (system, user) = build_referral_prompt(
      soap_note,
      recipient_type,
      urgency,
      settings.custom_referral_prompt.as_deref(),
  );
  ```

- `generate_letter` (around line 492):
  ```rust
  let (system, user) = build_letter_prompt(
      soap_note,
      letter_type,
      settings.custom_letter_prompt.as_deref(),
  );
  ```

- `generate_synopsis` (around line 545):
  ```rust
  let (system, user) = build_synopsis_prompt(
      soap_note,
      settings.custom_synopsis_prompt.as_deref(),
  );
  ```

- [ ] **Step 4: Remove `max_tokens_for_provider`**

Delete lines 132-137 (the function):
```rust
fn max_tokens_for_provider(provider: &str) -> Option<u32> {
    match provider {
        "lmstudio" | "ollama" => None,
        _ => Some(4096),
    }
}
```

At the four call sites (`CompletionRequest` constructions — `generate_soap` ~line 243, `generate_referral` ~line 406, `generate_letter` ~line 506, `generate_synopsis` ~line 558), replace `max_tokens: max_tokens_for_provider(&settings.ai_provider),` with `max_tokens: None,`.

- [ ] **Step 5: Build and test**

```bash
cargo check -p rust-medical-assistant-lib 2>&1 | tail -10
cargo test -p rust-medical-assistant-lib --lib 2>&1 | tail -10
```
Expected: clean compile, tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/generation.rs
git commit -m "$(cat <<'EOF'
feat(generation): load and apply per-doc custom prompts

GenerationSettings now carries custom_referral_prompt,
custom_letter_prompt, and custom_synopsis_prompt from AppConfig into
the prompt builders. Removes the now-trivial max_tokens_for_provider
helper (both remaining providers use None).

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Remove per-provider model config from SoapNoteSettings

**Files:**
- Modify: `crates/core/src/types/settings.rs`

**Context:** `SoapNoteSettings` has `openai_model`, `anthropic_model`, `groq_model` — all unused at runtime and only meaningful for removed providers. Keep `icd_code_version` (may be referenced elsewhere, and conceptually still valid).

- [ ] **Step 1: Remove the three model fields and their Default impl lines**

In `crates/core/src/types/settings.rs`, change `SoapNoteSettings` (lines 66-83):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapNoteSettings {
    pub icd_code_version: IcdVersion,
}

impl Default for SoapNoteSettings {
    fn default() -> Self {
        Self {
            icd_code_version: IcdVersion::default(),
        }
    }
}
```

- [ ] **Step 2: Check for callers of removed fields**

```bash
grep -rn "openai_model\|anthropic_model\|groq_model" src-tauri/src crates --include="*.rs"
```

Expected: no matches. If any remain, delete those lines.

- [ ] **Step 3: Build and test**

```bash
cargo test --workspace 2>&1 | grep -E "FAILED|error\[" | head
```
Expected: no failures.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/types/settings.rs
git commit -m "$(cat <<'EOF'
refactor(settings): drop dead per-provider model fields

SoapNoteSettings no longer tracks openai_model/anthropic_model/groq_model
since those providers are gone. icd_code_version remains.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Add `get_default_prompt` Tauri command

**Files:**
- Modify: `src-tauri/src/commands/settings.rs` (or a new module if preferred)
- Modify: `src-tauri/src/lib.rs` (register in `invoke_handler!`)
- Create: `src/lib/api/prompts.ts`

**Context:** The Prompts tab's "Reset to default" button needs the default template text. Expose it via a Tauri command.

- [ ] **Step 1: Add command to `src-tauri/src/commands/settings.rs`**

Append (after existing commands):

```rust
/// Return the built-in default system prompt for the given document type.
///
/// `doc_type` must be one of: "soap", "referral", "letter", "synopsis".
#[tauri::command]
pub fn get_default_prompt(doc_type: String) -> Result<String, String> {
    use medical_processing::document_generator::{
        default_letter_prompt, default_referral_prompt, default_synopsis_prompt,
    };
    use medical_processing::soap_generator::default_soap_prompt;

    match doc_type.as_str() {
        "soap" => Ok(default_soap_prompt().to_string()),
        "referral" => Ok(default_referral_prompt().to_string()),
        "letter" => Ok(default_letter_prompt().to_string()),
        "synopsis" => Ok(default_synopsis_prompt().to_string()),
        _ => Err(format!("Unknown doc_type: {}", doc_type)),
    }
}
```

- [ ] **Step 2: Register in `invoke_handler!`**

In `src-tauri/src/lib.rs`, find the `invoke_handler!` macro invocation and add `get_default_prompt` to the list alongside other settings commands.

Grep first to find the right location:
```bash
grep -n "invoke_handler\|get_api_key\|set_api_key" src-tauri/src/lib.rs | head
```

Add `commands::settings::get_default_prompt,` to the handler list.

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p rust-medical-assistant-lib 2>&1 | tail -5
```

- [ ] **Step 4: Create frontend wrapper**

Create `src/lib/api/prompts.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';

export type DocType = 'soap' | 'referral' | 'letter' | 'synopsis';

export async function getDefaultPrompt(docType: DocType): Promise<string> {
  return await invoke<string>('get_default_prompt', { docType });
}
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/settings.rs src-tauri/src/lib.rs src/lib/api/prompts.ts
git commit -m "$(cat <<'EOF'
feat(commands): add get_default_prompt for 4 doc types

Exposes the built-in default system prompt for SOAP/Referral/Letter/
Synopsis to the frontend. Used by the Prompts tab's "Reset to default"
button and the initial template load when no custom override is set.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Update TypeScript types + remove API Keys tab from Settings UI

**Files:**
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/components/SettingsContent.svelte`

**Context:** Add the four `custom_*_prompt` fields to the `AppConfig` TypeScript interface. Then strip the API Keys tab entirely from SettingsContent.svelte (the section markup, the `apikeys` entry in `navItems`, the `'apikeys'` in the `Section` union, the `API_PROVIDERS` constant, the `handleSaveApiKey` handler, the `apiKeyInputs`/`saveStatus`/`storedKeys` state and related code).

- [ ] **Step 1: Extend AppConfig in types/index.ts**

Open `src/lib/types/index.ts`. Find the `AppConfig` interface (lines 54-75) and add the four custom prompt fields:

```typescript
  custom_soap_prompt: string | null;
  custom_referral_prompt: string | null;
  custom_letter_prompt: string | null;
  custom_synopsis_prompt: string | null;
```

Insert these after the `custom_context_templates` line.

- [ ] **Step 2: Run svelte-check**

```bash
npm run check 2>&1 | tail -20
```
Expected: clean (the new fields are on `AppConfig` but not yet referenced in any component).

- [ ] **Step 3: Update the Section union and navItems**

In `src/lib/components/SettingsContent.svelte`:

Line 23, change:
```typescript
type Section = 'general' | 'apikeys' | 'models' | 'audio';
```
To:
```typescript
type Section = 'general' | 'prompts' | 'models' | 'audio';
```

Lines 386-391, change navItems:
```typescript
const navItems: { id: Section; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'prompts', label: 'Prompts' },
    { id: 'models', label: 'AI Models' },
    { id: 'audio', label: 'Audio / STT' },
];
```

- [ ] **Step 4: Remove API-keys-related state and handlers**

Remove lines 26-41 (the `API_PROVIDERS` array, the `storedKeys`/`apiKeyInputs`/`saveStatus` declarations, and the initialization `for` loop).

Remove the `setApiKey`/`listApiKeys` imports from line 5 (keep `testLmStudioConnection`).

Find and delete the `handleSaveApiKey` function (grep for it around lines 177-197 to confirm exact location).

Also find and remove the `onMount` logic that calls `listApiKeys()` — search for `listApiKeys()` to locate.

- [ ] **Step 5: Remove the API Keys tab section markup**

Find and delete the `{:else if activeSection === 'apikeys'}` block (starts around line 521). The block ends where the next `{:else if activeSection === 'models'}` begins (around line 560). Delete from the `{:else if activeSection === 'apikeys'}` line up to (but not including) the next `{:else if` line. The Prompts tab section is added in Task 11 — for now, don't add anything new; the activeSection `'prompts'` will simply render nothing until Task 11.

- [ ] **Step 6: Run svelte-check**

```bash
npm run check 2>&1 | tail -20
```
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/lib/types/index.ts src/lib/components/SettingsContent.svelte
git commit -m "$(cat <<'EOF'
feat(ui): remove API Keys tab, prep Prompts tab slot

Adds custom_*_prompt fields to AppConfig TS interface. Removes the
entire API Keys section from Settings along with its handlers and
state. Adds a 'prompts' section slot (rendered in Task 11).

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Build the Prompts tab UI

**Files:**
- Modify: `src/lib/components/SettingsContent.svelte`

**Context:** Build the Prompts tab: left sidebar with 4 doc types, right pane with a textarea, placeholder info panel, status line, and Save/Reset buttons. State flow matches spec section C.3. No live preview.

- [ ] **Step 1: Add imports and state**

Near the top of the `<script lang="ts">` block in `SettingsContent.svelte`, add:

```typescript
import { getDefaultPrompt, type DocType } from '../api/prompts';
```

After the other `$state` declarations (around line 61), add the Prompts tab state:

```typescript
  type PromptInfo = {
    key: DocType;
    label: string;
    configField: 'custom_soap_prompt' | 'custom_referral_prompt' | 'custom_letter_prompt' | 'custom_synopsis_prompt';
    placeholders: { token: string; description: string }[];
  };

  const PROMPT_TYPES: PromptInfo[] = [
    {
      key: 'soap',
      label: 'SOAP Note',
      configField: 'custom_soap_prompt',
      placeholders: [
        { token: '{icd_label}', description: 'ICD code header line (from ICD version setting)' },
        { token: '{icd_instruction}', description: 'Inline ICD reference phrase' },
        { token: '{template_guidance}', description: 'SOAP template hint (FollowUp, NewPatient, etc.)' },
      ],
    },
    {
      key: 'referral',
      label: 'Referral Letter',
      configField: 'custom_referral_prompt',
      placeholders: [
        { token: '{recipient_type}', description: 'e.g. Cardiologist, Orthopaedics' },
        { token: '{urgency}', description: 'routine, urgent, emergency' },
      ],
    },
    {
      key: 'letter',
      label: 'Patient Letter',
      configField: 'custom_letter_prompt',
      placeholders: [
        { token: '{letter_type}', description: 'e.g. results, instructions, follow-up' },
      ],
    },
    {
      key: 'synopsis',
      label: 'Clinical Synopsis',
      configField: 'custom_synopsis_prompt',
      placeholders: [],
    },
  ];

  let activePromptKey = $state<DocType>('soap');
  let promptEditorText = $state<string>('');
  let promptIsCustom = $state<boolean>(false);
  let promptDirty = $state<boolean>(false);
  let promptLoading = $state<boolean>(false);
  let promptSaveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
```

- [ ] **Step 2: Add loader logic**

Add a helper function in the script block that loads the textarea content for a given doc type:

```typescript
  async function loadPromptEditor(docType: DocType) {
    promptLoading = true;
    promptDirty = false;
    promptSaveStatus = 'idle';
    try {
      const info = PROMPT_TYPES.find((p) => p.key === docType)!;
      const customValue = $settings?.[info.configField] as string | null | undefined;
      if (customValue && customValue.length > 0) {
        promptEditorText = customValue;
        promptIsCustom = true;
      } else {
        promptEditorText = await getDefaultPrompt(docType);
        promptIsCustom = false;
      }
    } catch (e) {
      console.error('Failed to load prompt editor:', e);
      promptEditorText = '';
      promptIsCustom = false;
    } finally {
      promptLoading = false;
    }
  }

  async function handlePromptSelect(docType: DocType) {
    if (promptDirty) {
      const confirmed = confirm('You have unsaved changes. Discard them?');
      if (!confirmed) return;
    }
    activePromptKey = docType;
    await loadPromptEditor(docType);
  }

  async function handlePromptSave() {
    const info = PROMPT_TYPES.find((p) => p.key === activePromptKey)!;
    promptSaveStatus = 'saving';
    try {
      await settings.updateField(info.configField, promptEditorText);
      promptIsCustom = true;
      promptDirty = false;
      promptSaveStatus = 'saved';
      setTimeout(() => { promptSaveStatus = 'idle'; }, 1500);
    } catch (e) {
      console.error('Failed to save custom prompt:', e);
      promptSaveStatus = 'error';
    }
  }

  async function handlePromptReset() {
    const info = PROMPT_TYPES.find((p) => p.key === activePromptKey)!;
    if (promptIsCustom && !confirm('Clear the custom prompt and restore the default?')) return;
    try {
      await settings.updateField(info.configField, null);
      promptEditorText = await getDefaultPrompt(activePromptKey);
      promptIsCustom = false;
      promptDirty = false;
      promptSaveStatus = 'idle';
    } catch (e) {
      console.error('Failed to reset prompt:', e);
      promptSaveStatus = 'error';
    }
  }
```

- [ ] **Step 3: Trigger initial load when entering the Prompts tab**

Add an `$effect` to load when `activeSection` becomes `'prompts'`:

```typescript
  $effect(() => {
    if (activeSection === 'prompts') {
      loadPromptEditor(activePromptKey);
    }
  });
```

- [ ] **Step 4: Add the Prompts tab markup**

In the template area of SettingsContent.svelte, locate the slot where the `'prompts'` section belongs (after General, before Models — this matches the navItems order). Insert:

```svelte
    {:else if activeSection === 'prompts'}
      <section class="settings-section prompts-section">
        <h2>Prompts</h2>
        <p class="section-description">
          View and customize the system prompts sent to the AI for each document type.
          Placeholder tokens are substituted at generation time.
        </p>

        <div class="prompts-layout">
          <aside class="prompts-sidebar">
            {#each PROMPT_TYPES as pt}
              <button
                class="prompts-nav-item"
                class:active={activePromptKey === pt.key}
                onclick={() => handlePromptSelect(pt.key)}
              >
                {pt.label}
              </button>
            {/each}
          </aside>

          <div class="prompts-editor">
            {#if promptLoading}
              <div class="prompts-loading">Loading…</div>
            {:else}
              {@const info = PROMPT_TYPES.find((p) => p.key === activePromptKey)}
              <h3>{info?.label}</h3>

              <textarea
                class="prompt-textarea"
                bind:value={promptEditorText}
                oninput={() => (promptDirty = true)}
                rows="20"
                spellcheck="false"
              ></textarea>

              {#if info && info.placeholders.length > 0}
                <details class="prompts-placeholders">
                  <summary>Available placeholders</summary>
                  <ul>
                    {#each info.placeholders as ph}
                      <li>
                        <code>{ph.token}</code> — {ph.description}
                      </li>
                    {/each}
                  </ul>
                </details>
              {/if}

              <div class="prompts-status">
                Using: <strong>{promptIsCustom ? 'custom' : 'default'}</strong>
                {#if promptDirty}<span class="dirty-indicator"> (unsaved changes)</span>{/if}
              </div>

              <div class="prompts-actions">
                <button
                  class="btn btn-primary"
                  onclick={handlePromptSave}
                  disabled={!promptDirty || promptSaveStatus === 'saving'}
                >
                  {promptSaveStatus === 'saving' ? 'Saving…' : promptSaveStatus === 'saved' ? 'Saved' : 'Save as custom'}
                </button>
                <button
                  class="btn"
                  onclick={handlePromptReset}
                  disabled={!promptIsCustom && !promptDirty}
                >
                  Reset to default
                </button>
              </div>
              {#if promptSaveStatus === 'error'}
                <p class="error-message">Failed to save. See console for details.</p>
              {/if}
            {/if}
          </div>
        </div>
      </section>
```

- [ ] **Step 5: Add styles**

Add at the end of the `<style>` block:

```css
.prompts-layout {
  display: grid;
  grid-template-columns: 180px 1fr;
  gap: 1.5rem;
  align-items: start;
  margin-top: 1rem;
}

.prompts-sidebar {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  border-right: 1px solid var(--border-color);
  padding-right: 0.75rem;
}

.prompts-nav-item {
  text-align: left;
  padding: 0.5rem 0.75rem;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--text-primary);
  cursor: pointer;
  font-size: 0.9rem;
}

.prompts-nav-item:hover {
  background: var(--hover-bg);
}

.prompts-nav-item.active {
  background: var(--accent-bg);
  border-color: var(--accent-color);
}

.prompts-editor {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.prompt-textarea {
  width: 100%;
  font-family: var(--font-mono, monospace);
  font-size: 0.85rem;
  line-height: 1.4;
  padding: 0.75rem;
  background: var(--input-bg);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  resize: vertical;
  min-height: 400px;
}

.prompts-placeholders {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 0.5rem 0.75rem;
}

.prompts-placeholders summary {
  cursor: pointer;
  font-weight: 500;
}

.prompts-placeholders ul {
  margin: 0.5rem 0 0;
  padding-left: 1.25rem;
}

.prompts-placeholders code {
  background: var(--code-bg, rgba(128, 128, 128, 0.15));
  padding: 0.1rem 0.3rem;
  border-radius: 3px;
  font-size: 0.85rem;
}

.prompts-status {
  font-size: 0.9rem;
  color: var(--text-secondary);
}

.prompts-status .dirty-indicator {
  color: var(--warning-color, orange);
}

.prompts-actions {
  display: flex;
  gap: 0.5rem;
}

.prompts-loading {
  padding: 2rem;
  text-align: center;
  color: var(--text-secondary);
}
```

If any CSS variables (`--accent-bg`, `--code-bg`, `--warning-color`) aren't already defined in the app's theme, the fallbacks in the rules above keep things legible.

- [ ] **Step 6: svelte-check**

```bash
npm run check 2>&1 | tail -20
```
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/lib/components/SettingsContent.svelte
git commit -m "$(cat <<'EOF'
feat(ui): add Prompts tab for editing system prompts

New Settings tab lets the user view/edit default system prompts for
SOAP, Referral, Letter, and Synopsis. Save persists the edit as a
custom override; Reset clears the override and restores the default.
Placeholder tokens are documented in a collapsible info panel per
doc type.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Verification and cleanup

**Files:** (read-only verification; no code changes unless cleanup uncovers issues)

**Context:** Full test suite pass, manual UI QA on the Prompts tab, check for leftover references to removed providers.

- [ ] **Step 1: Full workspace test**

```bash
cargo test --workspace 2>&1 | tail -20
```
Expected: all tests pass.

- [ ] **Step 2: Svelte-check and build**

```bash
npm run check && npm run build 2>&1 | tail -10
```
Expected: clean.

- [ ] **Step 3: Scan for dead provider references**

```bash
grep -rn 'anthropic\|openai\|gemini\|groq\|cerebras' \
  --include='*.rs' --include='*.ts' --include='*.svelte' \
  src-tauri/src crates/ai-providers crates/core crates/processing crates/rag src | \
  grep -v '^docs/\|test-key\|http_client\.rs\|openai_compat\.rs'
```

Review each hit:
- `openai_compat.rs` — expected (used by LM Studio and Ollama)
- `http_client.rs` — expected (shared HTTP utilities)
- Any hit in `settings.rs` tests — expected as test-data strings
- Any other hit — investigate. A leftover import or call site → fix it.

- [ ] **Step 4: Manual UI verification**

```bash
npm run tauri dev
```

Open Settings → Prompts tab. For each of the 4 doc types:
1. Select in sidebar → textarea loads with default text, status shows "Using: default"
2. Edit the textarea → "(unsaved changes)" appears
3. Click "Save as custom" → status transitions to "Saved" briefly, then "Using: custom"
4. Close and reopen Settings → textarea loads with custom text, still "Using: custom"
5. Click "Reset to default" → confirm → textarea reverts, status returns to "Using: default"

Also verify:
- No "API Keys" tab in Settings nav
- General tab still works (theme, language, autosave toggles)
- AI Models tab lists models from LM Studio/Ollama (if running locally)
- LM Studio host/port fields still function with Test Connection button

- [ ] **Step 5: Test SOAP generation end-to-end**

Record a short audio clip or use an existing recording, generate a SOAP note. Verify:
- The SOAP note renders correctly
- The ICD label matches the current setting
- Template guidance is visible (e.g. for FollowUp, "changes since last visit" appears in the prompt)

- [ ] **Step 6: Final commit (if cleanup was required)**

If Step 3 revealed leftover references and fixes were applied:

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore: clean up remaining references to removed providers

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 7: Summary**

The working tree should be clean:

```bash
git status
```
Expected: `nothing to commit, working tree clean`.

Recent log should show the 11+ commits from Tasks 1–11 plus any cleanup:

```bash
git log --oneline -15
```

---

## Done criteria

- `cargo test --workspace` all green
- `npm run check` clean
- `npm run build` succeeds
- Grep for cloud provider references returns only the expected allowlist
- Manual UI test: Prompts tab loads, saves, and resets for all 4 doc types
- End-to-end generation produces a valid SOAP note with correctly resolved placeholders
