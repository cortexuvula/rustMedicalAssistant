# Remote LM Studio Host Configuration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow FerriScribe to connect to an LM Studio server running on a remote machine by configuring its host and port in Settings.

**Architecture:** Two new fields (`lmstudio_host`, `lmstudio_port`) in `AppConfig` flow through from the Settings UI → Rust backend → `LmStudioProvider` constructor. A new `test_lmstudio_connection` Tauri command validates connectivity. The Settings UI gets a "LM Studio Server" subsection with host/port inputs and a test button.

**Tech Stack:** Rust (Tauri v2), Svelte 5, reqwest, serde

---

### Task 1: Add `lmstudio_host` and `lmstudio_port` to `AppConfig`

**Files:**
- Modify: `crates/core/src/types/settings.rs`

- [ ] **Step 1: Add default helper functions**

Add these two functions after the existing `default_auto_index_rag` function (after line 175) in `crates/core/src/types/settings.rs`:

```rust
fn default_lmstudio_host() -> String {
    "localhost".into()
}

fn default_lmstudio_port() -> u16 {
    1234
}
```

- [ ] **Step 2: Add fields to `AppConfig` struct**

In the `AppConfig` struct, add these two fields in the "Providers" section, after the `tts_voice` field (after line 210):

```rust
    // LM Studio remote server
    #[serde(default = "default_lmstudio_host")]
    pub lmstudio_host: String,
    #[serde(default = "default_lmstudio_port")]
    pub lmstudio_port: u16,
```

- [ ] **Step 3: Add test assertions for the new defaults**

In the `default_config_values` test (inside the `mod tests` block), add these assertions after the existing `tts_voice` assertion (after line 299):

```rust
        assert_eq!(config.lmstudio_host, "localhost");
        assert_eq!(config.lmstudio_port, 1234);
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p medical-core`
Expected: All tests pass, including the updated `default_config_values`.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/types/settings.rs
git commit -m "feat: add lmstudio_host and lmstudio_port to AppConfig"
```

---

### Task 2: Update `init_ai_providers` to use LM Studio host config

**Files:**
- Modify: `src-tauri/src/state.rs`

- [ ] **Step 1: Add `AppConfig` import**

At the top of `src-tauri/src/state.rs`, add this import after the existing `use medical_ai_providers::ProviderRegistry;` line (line 14):

```rust
use medical_core::types::settings::AppConfig;
```

- [ ] **Step 2: Change `init_ai_providers` signature and LM Studio registration**

Change the `init_ai_providers` function signature from:

```rust
pub fn init_ai_providers(keys: &KeyStorage) -> ProviderRegistry {
```

to:

```rust
pub fn init_ai_providers(keys: &KeyStorage, config: &AppConfig) -> ProviderRegistry {
```

Then replace the LM Studio registration block (currently lines 116–118):

```rust
    // LM Studio — always available (local, no key needed)
    info!("Registering LM Studio provider (local)");
    registry.register(Arc::new(LmStudioProvider::new(None)));
```

with:

```rust
    // LM Studio — always available (local or remote, no key needed)
    let lmstudio_host = if config.lmstudio_host.is_empty() { "localhost" } else { &config.lmstudio_host };
    let lmstudio_url = format!("http://{}:{}", lmstudio_host, config.lmstudio_port);
    info!(url = %lmstudio_url, "Registering LM Studio provider");
    registry.register(Arc::new(LmStudioProvider::new(Some(&lmstudio_url))));
```

- [ ] **Step 3: Update `AppState::initialize` call site**

In `AppState::initialize()`, the call to `init_ai_providers` (currently line 159) is:

```rust
        let mut ai_providers = init_ai_providers(&keys);
```

The `config` variable is loaded just below on lines 162–165. Move the `init_ai_providers` call after the config is loaded. Replace lines 158–165 with:

```rust
        // Load saved settings to configure preferred providers
        let config = {
            let conn = db.conn().ok();
            conn.and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
        };
        let config_ref = config.as_ref().cloned().unwrap_or_default();

        // Initialize provider registries from saved API keys + config
        let mut ai_providers = init_ai_providers(&keys, &config_ref);
```

Then update the whisper model extraction (currently line 167–169) to use `config_ref`:

```rust
        let whisper_model = config.as_ref()
            .map(|c| c.whisper_model.as_str())
            .unwrap_or("large-v3-turbo");
```

(This line stays the same — it still reads from the original `config` Option.)

- [ ] **Step 4: Run `cargo check` to verify compilation**

Run: `cargo check`
Expected: Compiles with no errors (there will be a compilation error in `providers.rs` because we changed the signature — that's expected and fixed in Task 3).

Actually, `providers.rs` calls `init_ai_providers` too, so the build will fail until Task 3. Verify with:

Run: `cargo check 2>&1 | grep "error\["`
Expected: One error in `providers.rs` about argument count mismatch.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat: init_ai_providers reads LM Studio host/port from config"
```

---

### Task 3: Update `reinit_providers` and add `test_lmstudio_connection`

**Files:**
- Modify: `src-tauri/src/commands/providers.rs`

- [ ] **Step 1: Update `reinit_providers` to load config and pass it**

Replace the entire `src-tauri/src/commands/providers.rs` file with:

```rust
use std::time::Duration;

use tracing::info;

use crate::state::{self, AppState};

/// Re-read API keys from storage and rebuild AI + STT provider registries.
///
/// Returns the list of available AI provider names after reinitialization.
#[tauri::command]
pub async fn reinit_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // Load saved settings for provider config (host, port, active provider, whisper model)
    let config = {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        medical_db::settings::SettingsRepo::load_config(&conn)
            .map_err(|e| e.to_string())?
    };

    // Rebuild AI providers with current config (includes LM Studio host/port)
    let mut ai_registry = state::init_ai_providers(&state.keys, &config);

    // Restore the user's active provider preference from saved settings
    // so reinit doesn't silently switch to a random provider.
    ai_registry.set_active(&config.ai_provider);

    let available = ai_registry.list_available();
    {
        let mut guard = state.ai_providers.lock().await;
        *guard = ai_registry;
    }

    // Rebuild local STT provider with current whisper model setting
    let stt = state::init_stt_providers(&state.data_dir, &config.whisper_model);
    {
        let mut guard = state.stt_providers.lock().await;
        *guard = stt;
    }

    info!(providers = ?available, "Providers reinitialized");

    Ok(available)
}

/// Test connectivity to an LM Studio server.
///
/// Makes a GET request to `http://{host}:{port}/v1/models` with a 5-second
/// timeout. Returns a success message with the model count, or an error.
#[tauri::command]
pub async fn test_lmstudio_connection(host: String, port: u16) -> Result<String, String> {
    let effective_host = if host.is_empty() { "localhost".to_string() } else { host };
    let url = format!("http://{}:{}/v1/models", effective_host, port);

    info!(url = %url, "Testing LM Studio connection");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            if e.is_connect() {
                format!("Connection refused — is LM Studio running at {}:{}?", effective_host, port)
            } else if e.is_timeout() {
                format!("Connection timed out — check that {}:{} is reachable", effective_host, port)
            } else {
                format!("Connection failed: {e}")
            }
        })?;

    if !response.status().is_success() {
        return Err(format!("Server returned HTTP {}", response.status()));
    }

    // Parse the OpenAI-compatible models response to count models
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from server: {e}"))?;

    let model_count = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(format!("Connected — {} model{} available", model_count, if model_count == 1 { "" } else { "s" }))
}
```

- [ ] **Step 2: Run `cargo check` to verify compilation**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/providers.rs
git commit -m "feat: update reinit_providers, add test_lmstudio_connection command"
```

---

### Task 4: Register the new command in Tauri

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the command to the invoke handler**

In `src-tauri/src/lib.rs`, in the `tauri::generate_handler!` macro invocation, add this line after `commands::providers::reinit_providers,` (after line 128):

```rust
            commands::providers::test_lmstudio_connection,
```

- [ ] **Step 2: Run `cargo check` to verify**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: register test_lmstudio_connection Tauri command"
```

---

### Task 5: Add frontend API wrapper and TypeScript type updates

**Files:**
- Modify: `src/lib/api/settings.ts`
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/stores/settings.ts`

- [ ] **Step 1: Add `testLmStudioConnection` to the API layer**

In `src/lib/api/settings.ts`, add this function at the end of the file:

```typescript
export async function testLmStudioConnection(host: string, port: number): Promise<string> {
  return invoke('test_lmstudio_connection', { host, port });
}
```

- [ ] **Step 2: Add the new fields to the `AppConfig` TypeScript interface**

In `src/lib/types/index.ts`, add these two fields to the `AppConfig` interface, after the `tts_voice` field (after line 57):

```typescript
  lmstudio_host: string;
  lmstudio_port: number;
```

- [ ] **Step 3: Add defaults to the settings store**

In `src/lib/stores/settings.ts`, add these two fields to the `defaults` object, after `storage_path: null,` (after line 21):

```typescript
  lmstudio_host: 'localhost',
  lmstudio_port: 1234,
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/api/settings.ts src/lib/types/index.ts src/lib/stores/settings.ts
git commit -m "feat: add LM Studio host/port to frontend types and API"
```

---

### Task 6: Add the LM Studio Server UI section to Settings

**Files:**
- Modify: `src/lib/components/SettingsContent.svelte`

- [ ] **Step 1: Add state variables and imports for the test connection feature**

In the `<script>` block of `SettingsContent.svelte`, add these state variables after the existing `downloadProgress` variable (after line 45):

```typescript
  let lmstudioTestStatus = $state<'idle' | 'testing' | 'success' | 'error'>('idle');
  let lmstudioTestMessage = $state('');
```

Add the `testLmStudioConnection` import. Modify the existing import from `'../api/settings'` (line 5) to:

```typescript
  import { listApiKeys, setApiKey, testLmStudioConnection } from '../api/settings';
```

- [ ] **Step 2: Add handler functions**

Add these functions after the existing `handleTemperatureChange` function (after line 241):

```typescript
  async function handleLmStudioHostChange(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    await settings.updateField('lmstudio_host', value);
    lmstudioTestStatus = 'idle';
    lmstudioTestMessage = '';
    await reinitProviders();
  }

  async function handleLmStudioPortChange(e: Event) {
    const value = parseInt((e.target as HTMLInputElement).value, 10);
    if (value >= 1 && value <= 65535) {
      await settings.updateField('lmstudio_port', value);
      lmstudioTestStatus = 'idle';
      lmstudioTestMessage = '';
      await reinitProviders();
    }
  }

  async function handleTestLmStudioConnection() {
    lmstudioTestStatus = 'testing';
    lmstudioTestMessage = '';
    try {
      const host = $settings.lmstudio_host || 'localhost';
      const port = $settings.lmstudio_port || 1234;
      const msg = await testLmStudioConnection(host, port);
      lmstudioTestStatus = 'success';
      lmstudioTestMessage = msg;
    } catch (err: any) {
      lmstudioTestStatus = 'error';
      lmstudioTestMessage = err?.toString() || 'Connection failed';
    }
  }
```

- [ ] **Step 3: Add the LM Studio Server UI section**

In the template, inside the `{:else if activeSection === 'models'}` block, add this HTML block **after** the temperature `</div>` closing tag and **before** the `</section>` closing tag (after line 446, before line 447):

```svelte
        <!-- LM Studio Server -->
        <div class="form-group-divider"></div>
        <h4 class="subsection-title">LM Studio Server</h4>
        <p class="subsection-hint">
          Configure the LM Studio server address. Use <code>localhost</code> if LM Studio runs on this machine, or enter a remote IP for a network server.
        </p>

        <div class="form-group">
          <label for="lmstudio-host" class="form-label">Host</label>
          <input
            id="lmstudio-host"
            type="text"
            value={$settings.lmstudio_host}
            placeholder="localhost"
            onchange={handleLmStudioHostChange}
            class="text-input"
          />
        </div>

        <div class="form-group">
          <label for="lmstudio-port" class="form-label">Port</label>
          <input
            id="lmstudio-port"
            type="number"
            value={$settings.lmstudio_port}
            placeholder="1234"
            min="1"
            max="65535"
            onchange={handleLmStudioPortChange}
            class="text-input port-input"
          />
        </div>

        <div class="form-group">
          <button
            class="btn-test-connection"
            onclick={handleTestLmStudioConnection}
            disabled={lmstudioTestStatus === 'testing'}
          >
            {#if lmstudioTestStatus === 'testing'}
              Testing…
            {:else}
              Test Connection
            {/if}
          </button>
          {#if lmstudioTestStatus === 'success'}
            <span class="test-result test-success">✓ {lmstudioTestMessage}</span>
          {:else if lmstudioTestStatus === 'error'}
            <span class="test-result test-error">✗ {lmstudioTestMessage}</span>
          {/if}
        </div>
```

- [ ] **Step 4: Add CSS styles**

In the `<style>` block at the bottom of `SettingsContent.svelte`, add these styles:

```css
  .form-group-divider {
    border-top: 1px solid var(--border);
    margin: 20px 0 16px;
  }

  .subsection-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px;
  }

  .subsection-hint {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0 0 12px;
    line-height: 1.5;
  }

  .subsection-hint code {
    font-size: 11px;
    background-color: var(--bg-tertiary, #374151);
    padding: 1px 5px;
    border-radius: 3px;
  }

  .port-input {
    max-width: 120px;
  }

  .btn-test-connection {
    padding: 6px 14px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background-color 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }

  .btn-test-connection:hover:not(:disabled) {
    background-color: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--accent);
  }

  .btn-test-connection:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .test-result {
    font-size: 13px;
    margin-left: 10px;
  }

  .test-success {
    color: #22c55e;
  }

  .test-error {
    color: var(--danger, #ef4444);
  }
```

- [ ] **Step 5: Verify the UI renders correctly**

Run: `npm run dev` (or `cargo tauri dev`)
Navigate to Settings → AI Models.
Expected: The "LM Studio Server" subsection appears below the temperature slider, with Host, Port, and Test Connection controls.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/SettingsContent.svelte
git commit -m "feat: add LM Studio Server configuration UI in Settings"
```

---

### Task 7: End-to-end verification

**Files:** None (testing only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test`
Expected: All tests pass (including the new `AppConfig` default assertions).

- [ ] **Step 2: Run the Svelte type check**

Run: `npx svelte-check --threshold warning`
Expected: No new errors introduced (pre-existing errors are acceptable).

- [ ] **Step 3: Manual end-to-end test**

1. Start FerriScribe with `cargo tauri dev`
2. Go to Settings → AI Models
3. Verify the LM Studio Server section shows Host = `localhost`, Port = `1234`
4. If LM Studio is running locally, click "Test Connection" — expect green checkmark with model count
5. Change Host to a non-existent IP (e.g., `192.168.99.99`), click "Test Connection" — expect red X with timeout/connection error
6. Change Host back to the correct value, verify "Test Connection" succeeds again
7. Select "LM Studio" as the AI provider, pick a model, and verify chat/generation works

- [ ] **Step 4: Final commit (if any cleanup needed)**

```bash
git add -A
git commit -m "chore: final cleanup for remote LM Studio host feature"
```
