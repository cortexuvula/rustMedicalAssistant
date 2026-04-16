# Remote LM Studio Host Configuration

## Goal

Allow FerriScribe instances running on other computers on the local network to connect to an LM Studio server running on a specific host machine, so that only one machine needs to run LM Studio and its models.

## Architecture

Settings-based host configuration. Two new fields in `AppConfig` let the user specify the LM Studio server address. The backend reads these fields at provider initialization and passes the constructed URL to `LmStudioProvider::new()`. A "Test Connection" command validates connectivity before the user commits to a config.

No new dependencies. No auto-discovery. The user types the host IP and port once per remote machine.

## Data Model

Two new fields in `AppConfig` (`crates/core/src/types/settings.rs`):

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `lmstudio_host` | `String` | `"localhost"` | Hostname or IP address of the LM Studio server |
| `lmstudio_port` | `u16` | `1234` | Port number of the LM Studio server |

These are persisted via the existing settings DB mechanism (`SettingsRepo`). An empty `lmstudio_host` is treated as `"localhost"`.

## Backend Changes

### `init_ai_providers` signature change

Currently:
```rust
pub fn init_ai_providers(keys: &KeyStorage) -> ProviderRegistry
```

Changes to:
```rust
pub fn init_ai_providers(keys: &KeyStorage, config: &AppConfig) -> ProviderRegistry
```

The LM Studio registration changes from:
```rust
registry.register(Arc::new(LmStudioProvider::new(None)));
```
to:
```rust
let host = if config.lmstudio_host.is_empty() { "localhost" } else { &config.lmstudio_host };
let lmstudio_url = format!("http://{}:{}", host, config.lmstudio_port);
registry.register(Arc::new(LmStudioProvider::new(Some(&lmstudio_url))));
```

Both call sites (`AppState::initialize` and `reinit_providers`) already load `AppConfig` from the DB, so they pass it through.

### New Tauri command: `test_lmstudio_connection`

```rust
#[tauri::command]
pub async fn test_lmstudio_connection(host: String, port: u16) -> Result<String, String>
```

- Constructs `http://{host}:{port}/v1/models`
- Makes a GET request with a 5-second timeout using a one-off `reqwest::Client` (no auth headers needed for LM Studio)
- On success: returns the number of models found (e.g., "Connected - 3 models available")
- On failure: returns a descriptive error (connection refused, timeout, etc.)

This command is registered in the Tauri app alongside the existing commands.

### `reinit_providers` update

`reinit_providers` in `providers.rs` already loads `AppConfig` for the whisper model setting. It passes the config to the updated `init_ai_providers` signature. No structural change needed beyond passing the config through.

## Settings UI

In `SettingsContent.svelte`, a new "LM Studio Server" subsection appears within the "AI Models" section. It is always visible (not gated by provider selection) since the user may want to configure the host before switching to LM Studio.

### UI elements

- **Host input** — text field, placeholder `"localhost"`, bound to `$settings.lmstudio_host`. Saves via `settings.updateField('lmstudio_host', value)`.
- **Port input** — number field, placeholder `1234`, min=1, max=65535, bound to `$settings.lmstudio_port`. Saves via `settings.updateField('lmstudio_port', value)`.
- **Test Connection button** — calls `test_lmstudio_connection(host, port)`. Shows:
  - Spinner while testing
  - Green checkmark + success message on success
  - Red X + error message on failure
  - Result resets when host or port values change

After host or port changes are saved, `reinit_providers` is called to rebuild the LM Studio provider with the new URL so changes take effect immediately without restarting.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Empty host field | Treated as `"localhost"` (default) |
| Invalid port (out of range) | Number input constrains to 1–65535; backend validates before constructing URL |
| LM Studio not running | Provider still registers (appears in dropdown). API calls fail with connection error. "Test Connection" catches this early. |
| Host unreachable on network | "Test Connection" returns descriptive error (timeout, connection refused). Provider still registers but calls fail at use time. |
| URL change while idle | `reinit_providers` rebuilds the provider immediately |

## Files Modified

| File | Change |
|------|--------|
| `crates/core/src/types/settings.rs` | Add `lmstudio_host` and `lmstudio_port` to `AppConfig` with defaults |
| `src-tauri/src/state.rs` | Update `init_ai_providers` to accept `&AppConfig` and use host/port for LM Studio |
| `src-tauri/src/commands/providers.rs` | Pass config to `init_ai_providers`; add `test_lmstudio_connection` command |
| `src-tauri/src/lib.rs` | Register `test_lmstudio_connection` in the Tauri command list |
| `src/lib/components/SettingsContent.svelte` | Add LM Studio Server subsection with host, port, test connection |
| `src/lib/api/settings.ts` or equivalent | Add `testLmStudioConnection` invoke wrapper |
