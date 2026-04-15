# Streamlined Recording-to-SOAP Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the Record tab into a one-stop workspace: paste context, record, and automatically get a SOAP note — with no tab switching required.

**Architecture:** New `process_recording` backend command chains existing transcription and SOAP generation logic, emitting `pipeline-progress` events. Frontend adds a context panel and pipeline status display to RecordTab, with a toast notification system for background completion. A new `auto_generate_soap` setting controls whether the pipeline auto-starts on recording stop.

**Tech Stack:** Rust/Tauri backend, Svelte 5 frontend, existing STT failover chain and AI provider registry.

---

### Task 1: Add `auto_generate_soap` Setting to Backend

**Files:**
- Modify: `crates/core/src/types/settings.rs`

- [ ] **Step 1: Add the default function and field to AppConfig**

In `crates/core/src/types/settings.rs`, add the default function after `default_auto_retry_failed`:

```rust
fn default_auto_generate_soap() -> bool {
    false
}
```

Then add the field to the `AppConfig` struct, in the "Processing" section (after `auto_generate_letter`):

```rust
    #[serde(default = "default_auto_generate_soap")]
    pub auto_generate_soap: bool,
```

- [ ] **Step 2: Add test assertion**

In the `default_config_values` test, add after the `auto_generate_letter` assertion:

```rust
        assert!(!config.auto_generate_soap);
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p medical-core -- settings`
Expected: All settings tests pass, including the new assertion.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/types/settings.rs
git commit -m "feat: add auto_generate_soap setting (default off)"
```

---

### Task 2: Add `auto_generate_soap` to Frontend Types and Settings UI

**Files:**
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/stores/settings.ts`
- Modify: `src/lib/components/SettingsContent.svelte`

- [ ] **Step 1: Add the field to the TypeScript AppConfig interface**

In `src/lib/types/index.ts`, add to the `AppConfig` interface after `autosave_interval_secs`:

```typescript
  auto_generate_soap: boolean;
```

- [ ] **Step 2: Add the default to the settings store**

In `src/lib/stores/settings.ts`, add to the `defaults` object after `autosave_interval_secs: 60,`:

```typescript
  auto_generate_soap: false,
```

- [ ] **Step 3: Add toggle to Settings UI**

In `src/lib/components/SettingsContent.svelte`, add the toggle in the "Audio / STT" section, after the sample rate form-group closing `</div>` and before the section closing `</section>`:

```svelte
        <div class="form-group">
          <label class="form-label checkbox-label">
            <input
              type="checkbox"
              checked={$settings.auto_generate_soap}
              onchange={(e: Event) => {
                const checked = (e.target as HTMLInputElement).checked;
                settings.updateField('auto_generate_soap', checked);
              }}
            />
            <span>Auto-generate SOAP after recording</span>
          </label>
          <span class="form-hint">When enabled, transcription and SOAP generation start automatically after you stop recording.</span>
        </div>
```

- [ ] **Step 4: Verify in browser**

Open Settings → Audio / STT. Confirm the toggle appears, defaults to off, and persists when toggled.

- [ ] **Step 5: Commit**

```bash
git add src/lib/types/index.ts src/lib/stores/settings.ts src/lib/components/SettingsContent.svelte
git commit -m "feat: add auto-generate SOAP toggle to settings UI"
```

---

### Task 3: Add `process_recording` Backend Command

**Files:**
- Create: `src-tauri/src/commands/pipeline.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create the pipeline command module**

Create `src-tauri/src/commands/pipeline.rs`:

```rust
//! Background pipeline: transcribe → generate SOAP in one command.

use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;
use uuid::Uuid;

use medical_db::recordings::RecordingsRepo;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
struct PipelineProgress {
    recording_id: String,
    stage: String,
    error: Option<String>,
}

/// Run the full transcribe → SOAP pipeline for a recording.
///
/// This command is designed to be called fire-and-forget from the frontend.
/// Progress is reported via `pipeline-progress` events so the frontend can
/// track multiple concurrent pipelines by recording ID.
#[tauri::command]
pub async fn process_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    context: Option<String>,
    template: Option<String>,
) -> Result<String, String> {
    let rid = recording_id.clone();

    // If no explicit template, read the user's preferred template from settings.
    let template = match template {
        Some(t) => Some(t),
        None => {
            let db = std::sync::Arc::clone(&state.db);
            tokio::task::spawn_blocking(move || {
                let conn = db.conn().ok()?;
                let cfg = medical_db::settings::SettingsRepo::load_config(&conn).ok()?;
                let t = match cfg.soap_template {
                    medical_core::types::settings::SoapTemplate::FollowUp => "follow_up",
                    medical_core::types::settings::SoapTemplate::NewPatient => "new_patient",
                    medical_core::types::settings::SoapTemplate::Telehealth => "telehealth",
                    medical_core::types::settings::SoapTemplate::Emergency => "emergency",
                    medical_core::types::settings::SoapTemplate::Pediatric => "pediatric",
                    medical_core::types::settings::SoapTemplate::Geriatric => "geriatric",
                };
                Some(t.to_string())
            })
            .await
            .ok()
            .flatten()
        }
    };

    // --- Stage 1: Transcribe ---
    emit_progress(&app, &rid, "transcribing", None);

    let transcript_result = super::transcription::transcribe_recording(
        app.clone(),
        state.clone(),
        recording_id.clone(),
        None, // language — use default
        Some(true), // diarize
    )
    .await;

    if let Err(ref e) = transcript_result {
        emit_progress(&app, &rid, "failed", Some(e.clone()));
        return Err(e.clone());
    }

    // --- Stage 2: Generate SOAP ---
    emit_progress(&app, &rid, "generating_soap", None);

    let soap_result = super::generation::generate_soap(
        app.clone(),
        state.clone(),
        recording_id.clone(),
        template,
        context,
    )
    .await;

    match soap_result {
        Ok(soap_text) => {
            // Fetch the recording name for the notification
            let display_name = get_recording_display_name(&state, &recording_id).await;
            emit_progress(&app, &rid, "completed", None);

            // Emit a dedicated notification event for the toast
            let _ = app.emit("pipeline-complete", serde_json::json!({
                "recording_id": rid,
                "display_name": display_name,
            }));

            Ok(soap_text)
        }
        Err(e) => {
            emit_progress(&app, &rid, "failed", Some(e.clone()));
            Err(e)
        }
    }
}

fn emit_progress(app: &tauri::AppHandle, recording_id: &str, stage: &str, error: Option<String>) {
    let _ = app.emit(
        "pipeline-progress",
        PipelineProgress {
            recording_id: recording_id.to_string(),
            stage: stage.to_string(),
            error,
        },
    );
}

async fn get_recording_display_name(state: &AppState, recording_id: &str) -> String {
    let uuid = match Uuid::parse_str(recording_id) {
        Ok(u) => u,
        Err(_) => return "Recording".to_string(),
    };
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().ok()?;
        let rec = RecordingsRepo::get_by_id(&conn, &uuid).ok()?;
        Some(rec.patient_name.unwrap_or(rec.filename))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "Recording".to_string())
}
```

- [ ] **Step 2: Register the module in mod.rs**

In `src-tauri/src/commands/mod.rs`, add after the `pub mod transcription;` line:

```rust
pub mod pipeline;
```

- [ ] **Step 3: Register the command in lib.rs**

In `src-tauri/src/lib.rs`, add to the `invoke_handler` list after `commands::transcription::list_stt_providers,`:

```rust
            commands::pipeline::process_recording,
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: Compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/pipeline.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add process_recording pipeline command (transcribe + SOAP)"
```

---

### Task 4: Add Frontend API and Pipeline Store

**Files:**
- Create: `src/lib/api/pipeline.ts`
- Create: `src/lib/stores/pipeline.ts`

- [ ] **Step 1: Create the pipeline API wrapper**

Create `src/lib/api/pipeline.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';

export async function processRecording(
  recordingId: string,
  context?: string,
  template?: string,
): Promise<string> {
  return invoke('process_recording', {
    recordingId,
    context: context ?? null,
    template: template ?? null,
  });
}
```

- [ ] **Step 2: Create the pipeline store**

Create `src/lib/stores/pipeline.ts`:

```typescript
import { writable, get } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { processRecording } from '../api/pipeline';
import { recordings } from './recordings';

export type PipelineStage = 'idle' | 'transcribing' | 'generating_soap' | 'completed' | 'failed';

export interface PipelineEntry {
  recordingId: string;
  stage: PipelineStage;
  error: string | null;
}

interface PipelineState {
  /** The most recent pipeline (shown on Record tab). */
  current: PipelineEntry | null;
  /** All active pipelines keyed by recording ID. */
  active: Record<string, PipelineEntry>;
}

function createPipelineStore() {
  const { subscribe, update, set } = writable<PipelineState>({
    current: null,
    active: {},
  });

  let progressUnlisten: UnlistenFn | null = null;
  let completeUnlisten: UnlistenFn | null = null;

  return {
    subscribe,

    /** Start listening for backend pipeline events. Call once on app mount. */
    async init() {
      progressUnlisten = await listen<{ recording_id: string; stage: string; error?: string }>(
        'pipeline-progress',
        (event) => {
          const { recording_id, stage, error } = event.payload;
          const entry: PipelineEntry = {
            recordingId: recording_id,
            stage: stage as PipelineStage,
            error: error ?? null,
          };
          update((s) => ({
            ...s,
            current: s.current?.recordingId === recording_id ? entry : s.current,
            active: { ...s.active, [recording_id]: entry },
          }));

          // Clean up completed/failed entries from active map after a delay
          if (stage === 'completed' || stage === 'failed') {
            recordings.load(); // Refresh recordings list
            setTimeout(() => {
              update((s) => {
                const { [recording_id]: _, ...rest } = s.active;
                return { ...s, active: rest };
              });
            }, 30000);
          }
        },
      );
    },

    /** Launch the pipeline for a recording. Non-blocking — returns immediately. */
    launch(recordingId: string, context?: string, template?: string) {
      const entry: PipelineEntry = {
        recordingId,
        stage: 'transcribing',
        error: null,
      };
      update((s) => ({
        ...s,
        current: entry,
        active: { ...s.active, [recordingId]: entry },
      }));

      // Fire and forget — progress comes via events
      processRecording(recordingId, context, template).catch((err) => {
        const errorEntry: PipelineEntry = {
          recordingId,
          stage: 'failed',
          error: String(err),
        };
        update((s) => ({
          ...s,
          current: s.current?.recordingId === recordingId ? errorEntry : s.current,
          active: { ...s.active, [recordingId]: errorEntry },
        }));
      });
    },

    /** Clear the current pipeline display (e.g., when starting a new recording). */
    clearCurrent() {
      update((s) => ({ ...s, current: null }));
    },

    /** Retry a failed pipeline. */
    retry(recordingId: string, context?: string, template?: string) {
      this.launch(recordingId, context, template);
    },

    destroy() {
      progressUnlisten?.();
      completeUnlisten?.();
    },
  };
}

export const pipeline = createPipelineStore();
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/api/pipeline.ts src/lib/stores/pipeline.ts
git commit -m "feat: add pipeline API wrapper and store with event tracking"
```

---

### Task 5: Add Toast Notification Component

**Files:**
- Create: `src/lib/components/ToastContainer.svelte`
- Create: `src/lib/stores/toasts.ts`
- Modify: `src/App.svelte`

- [ ] **Step 1: Create the toast store**

Create `src/lib/stores/toasts.ts`:

```typescript
import { writable } from 'svelte/store';

export interface Toast {
  id: string;
  message: string;
  type: 'success' | 'error';
  /** Recording ID for "View" button navigation. */
  recordingId?: string;
  /** Display name shown in the toast. */
  displayName?: string;
  /** Whether to auto-dismiss (errors persist until manually dismissed). */
  autoDismiss: boolean;
}

function createToastStore() {
  const { subscribe, update } = writable<Toast[]>([]);
  let counter = 0;

  return {
    subscribe,

    add(toast: Omit<Toast, 'id'>) {
      const id = `toast-${++counter}`;
      const entry = { ...toast, id };
      update((toasts) => [...toasts, entry]);

      if (toast.autoDismiss) {
        setTimeout(() => {
          this.dismiss(id);
        }, 8000);
      }

      return id;
    },

    dismiss(id: string) {
      update((toasts) => toasts.filter((t) => t.id !== id));
    },
  };
}

export const toasts = createToastStore();
```

- [ ] **Step 2: Create the ToastContainer component**

Create `src/lib/components/ToastContainer.svelte`:

```svelte
<script lang="ts">
  import { toasts, type Toast } from '../stores/toasts';

  interface Props {
    onNavigate?: (tab: string, recordingId: string) => void;
  }
  let { onNavigate }: Props = $props();

  function handleView(toast: Toast) {
    if (toast.recordingId && onNavigate) {
      onNavigate('soap', toast.recordingId);
    }
    toasts.dismiss(toast.id);
  }
</script>

{#if $toasts.length > 0}
  <div class="toast-container">
    {#each $toasts as toast (toast.id)}
      <div class="toast" class:toast-success={toast.type === 'success'} class:toast-error={toast.type === 'error'}>
        <span class="toast-message">{toast.message}</span>
        <div class="toast-actions">
          {#if toast.type === 'success' && toast.recordingId}
            <button class="toast-btn toast-btn-view" onclick={() => handleView(toast)}>View</button>
          {/if}
          <button class="toast-btn toast-btn-dismiss" onclick={() => toasts.dismiss(toast.id)}>Dismiss</button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    top: 16px;
    right: 16px;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 400px;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-radius: var(--radius-md, 8px);
    font-size: 13px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    animation: slideIn 0.2s ease-out;
  }

  @keyframes slideIn {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }

  .toast-success {
    background-color: var(--bg-secondary, #1f2937);
    border: 1px solid var(--success, #22c55e);
    color: var(--text-primary, #f9fafb);
  }

  .toast-error {
    background-color: var(--bg-secondary, #1f2937);
    border: 1px solid var(--danger, #ef4444);
    color: var(--text-primary, #f9fafb);
  }

  .toast-message {
    flex: 1;
  }

  .toast-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .toast-btn {
    padding: 4px 10px;
    border-radius: var(--radius-sm, 4px);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.15s ease;
  }

  .toast-btn-view {
    background-color: var(--accent, #3b82f6);
    color: white;
  }

  .toast-btn-view:hover {
    background-color: var(--accent-hover, #2563eb);
  }

  .toast-btn-dismiss {
    background-color: transparent;
    color: var(--text-muted, #9ca3af);
    border: 1px solid var(--border, #374151);
  }

  .toast-btn-dismiss:hover {
    background-color: var(--bg-hover, #374151);
    color: var(--text-primary, #f9fafb);
  }
</style>
```

- [ ] **Step 3: Wire toast notifications to pipeline events in App.svelte**

In `src/App.svelte`, add imports at the top of the `<script>` block:

```typescript
  import { pipeline } from './lib/stores/pipeline';
  import { toasts } from './lib/stores/toasts';
  import ToastContainer from './lib/components/ToastContainer.svelte';
```

In the `onMount`, after the existing `progressUnlisten` setup, add pipeline init and the `pipeline-complete` listener:

```typescript
    await pipeline.init();

    const pipelineCompleteUnlisten = await listen<{ recording_id: string; display_name: string }>(
      'pipeline-complete',
      (event) => {
        const { recording_id, display_name } = event.payload;
        toasts.add({
          message: `SOAP note ready for ${display_name}`,
          type: 'success',
          recordingId: recording_id,
          displayName: display_name,
          autoDismiss: true,
        });
      },
    );

    const pipelineFailedUnlisten = await listen<{ recording_id: string; stage: string; error?: string }>(
      'pipeline-progress',
      (event) => {
        if (event.payload.stage === 'failed') {
          toasts.add({
            message: `Processing failed: ${event.payload.error ?? 'Unknown error'}`,
            type: 'error',
            recordingId: event.payload.recording_id,
            autoDismiss: false,
          });
        }
      },
    );
```

In the `onDestroy`, add:

```typescript
    pipeline.destroy();
    pipelineCompleteUnlisten?.();
    pipelineFailedUnlisten?.();
```

Add a `navigateToSoap` function and the `ToastContainer` in the template:

```typescript
  import { selectedRecording, selectRecording } from './lib/stores/recordings';

  async function navigateToSoap(tab: string, recordingId: string) {
    await selectRecording(recordingId);
    activeTab = tab;
  }
```

Add `<ToastContainer onNavigate={navigateToSoap} />` just before the closing `</div>` of the `.app-shell`.

- [ ] **Step 4: Check that `selectRecording` is exported from recordings store**

In `src/lib/stores/recordings.ts`, verify there is a standalone `selectRecording` function export. If it only exists as a method on the `recordings` store object, add a standalone export:

```typescript
export async function selectRecording(id: string) {
  // ... fetch and set $selectedRecording
}
```

- [ ] **Step 5: Verify in browser**

The app should load without errors. No toasts visible yet (nothing triggers them).

- [ ] **Step 6: Commit**

```bash
git add src/lib/stores/toasts.ts src/lib/components/ToastContainer.svelte src/App.svelte src/lib/stores/recordings.ts
git commit -m "feat: add toast notification system with pipeline-complete listener"
```

---

### Task 6: Redesign RecordTab with Context Panel and Pipeline Status

**Files:**
- Modify: `src/lib/pages/RecordTab.svelte`

- [ ] **Step 1: Rewrite RecordTab.svelte**

Replace the contents of `src/lib/pages/RecordTab.svelte` with:

```svelte
<script lang="ts">
  import { audio } from '../stores/audio';
  import { settings } from '../stores/settings';
  import { pipeline, type PipelineStage } from '../stores/pipeline';
  import { recordings } from '../stores/recordings';
  import { importAudioFile } from '../api/recordings';
  import RecordingHeader from '../components/RecordingHeader.svelte';
  import { open } from '@tauri-apps/plugin-dialog';

  // Context panel state
  let contextText = $state('');
  let contextCollapsed = $state(true);

  // Import flow state
  let importedRecordingId = $state<string | null>(null);
  let importedFilename = $state<string | null>(null);
  let importing = $state(false);
  let importError = $state<string | null>(null);

  // Track the recording ID the current pipeline status refers to
  let pipelineRecordingId = $state<string | null>(null);

  function stageLabel(stage: PipelineStage): string {
    switch (stage) {
      case 'transcribing': return 'Transcribing audio...';
      case 'generating_soap': return 'Generating SOAP note...';
      case 'completed': return 'SOAP note ready';
      case 'failed': return 'Pipeline failed';
      default: return '';
    }
  }

  function handleStartRecording() {
    // Clear context for a fresh recording
    contextText = '';
    importedRecordingId = null;
    importedFilename = null;
    importError = null;
    pipeline.clearCurrent();
    audio.startRecording();
  }

  function handleStopRecording() {
    audio.stop().then(() => {
      const recordingId = $audio.lastRecordingId;
      if (!recordingId) return;

      pipelineRecordingId = recordingId;

      if ($settings.auto_generate_soap) {
        pipeline.launch(recordingId, contextText || undefined);
      }
    });
  }

  function handleProcessRecording() {
    const recordingId = $audio.lastRecordingId ?? importedRecordingId;
    if (!recordingId) return;
    pipelineRecordingId = recordingId;
    pipeline.launch(recordingId, contextText || undefined);
  }

  function handleRetry() {
    if (!pipelineRecordingId) return;
    pipeline.retry(pipelineRecordingId, contextText || undefined);
  }

  async function handleUploadAudio() {
    importError = null;
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'Audio Files', extensions: ['wav', 'mp3', 'ogg', 'flac', 'm4a', 'aac', 'wma', 'webm'] },
        ],
      });
      if (!selected) return;

      importing = true;
      const filePath = typeof selected === 'string' ? selected : selected;
      const recordingId = await importAudioFile(filePath);
      importedRecordingId = recordingId;
      importedFilename = filePath.split('/').pop()?.split('\\').pop() ?? 'audio file';
      await recordings.load();

      if ($settings.auto_generate_soap) {
        pipelineRecordingId = recordingId;
        pipeline.launch(recordingId, contextText || undefined);
      }
    } catch (e: any) {
      importError = e?.toString() || 'Import failed';
    } finally {
      importing = false;
    }
  }

  async function handleCopySoap() {
    // The SOAP text was persisted to the recording by the pipeline.
    // Read it from the pipeline result isn't stored locally, so we
    // load the recording and copy its soap_note.
    const rid = pipelineRecordingId;
    if (!rid) return;
    try {
      const { getRecording } = await import('../api/recordings');
      const rec = await getRecording(rid);
      if (rec?.soap_note) {
        await navigator.clipboard.writeText(rec.soap_note);
      }
    } catch (_) {
      // Fallback: clipboard API may fail in some contexts
    }
  }
</script>

<div class="record-tab">
  <!-- Context Panel (collapsible, top) -->
  <div class="context-panel" class:collapsed={contextCollapsed}>
    <button class="context-toggle" onclick={() => (contextCollapsed = !contextCollapsed)}>
      <span class="toggle-arrow">{contextCollapsed ? '▶' : '▼'}</span>
      Patient Context
      <span class="context-hint">(optional)</span>
    </button>
    {#if !contextCollapsed}
      <textarea
        class="context-textarea"
        placeholder="Paste chart notes, medications, history..."
        bind:value={contextText}
        rows="5"
      ></textarea>
    {/if}
  </div>

  <!-- Recording Controls (middle, unchanged) -->
  <RecordingHeader
    onStart={handleStartRecording}
    onStop={handleStopRecording}
  />

  <!-- Main content area -->
  <div class="record-content">
    {#if $pipeline.current && pipelineRecordingId}
      <!-- Pipeline Status -->
      <div class="pipeline-status">
        <div class="pipeline-stages">
          <div class="stage" class:active={$pipeline.current.stage === 'transcribing'} class:done={['generating_soap', 'completed'].includes($pipeline.current.stage)}>
            {#if $pipeline.current.stage === 'transcribing'}
              <span class="spinner"></span>
            {:else if ['generating_soap', 'completed'].includes($pipeline.current.stage)}
              <span class="stage-check">✓</span>
            {:else}
              <span class="stage-dot">○</span>
            {/if}
            Transcribe
          </div>
          <span class="stage-arrow">→</span>
          <div class="stage" class:active={$pipeline.current.stage === 'generating_soap'} class:done={$pipeline.current.stage === 'completed'}>
            {#if $pipeline.current.stage === 'generating_soap'}
              <span class="spinner"></span>
            {:else if $pipeline.current.stage === 'completed'}
              <span class="stage-check">✓</span>
            {:else}
              <span class="stage-dot">○</span>
            {/if}
            SOAP Note
          </div>
          <span class="stage-arrow">→</span>
          <div class="stage" class:done={$pipeline.current.stage === 'completed'}>
            {#if $pipeline.current.stage === 'completed'}
              <span class="stage-check">✓</span>
            {:else}
              <span class="stage-dot">○</span>
            {/if}
            Done
          </div>
        </div>

        <p class="pipeline-label">{stageLabel($pipeline.current.stage)}</p>

        {#if $pipeline.current.stage === 'completed'}
          <div class="post-actions">
            <button class="btn-primary" onclick={handleCopySoap}>Copy SOAP Note</button>
          </div>
        {/if}

        {#if $pipeline.current.stage === 'failed'}
          <div class="error-text">{$pipeline.current.error}</div>
          <div class="post-actions">
            <button class="btn-primary" onclick={handleRetry}>Retry</button>
          </div>
        {/if}
      </div>

    {:else if importedRecordingId && $audio.state === 'idle'}
      <!-- Imported file, pipeline not yet started -->
      <div class="state-message">
        <div class="state-icon">✓</div>
        <h2>Audio File Imported</h2>
        <p><strong>{importedFilename}</strong> has been added to your recordings.</p>

        {#if !$settings.auto_generate_soap}
          <div class="post-actions">
            <button class="btn-primary" onclick={handleProcessRecording}>
              Process Recording
            </button>
          </div>
        {/if}

        {#if importError}
          <div class="error-text">{importError}</div>
        {/if}
      </div>

    {:else if $audio.state === 'idle'}
      <div class="state-message">
        <div class="state-icon">🎙</div>
        <h2>Ready to Record</h2>
        <p>Press <strong>Record</strong> to start capturing audio, or upload an existing file.</p>

        <div class="post-actions">
          <button
            class="btn-upload"
            onclick={handleUploadAudio}
            disabled={importing}
          >
            {#if importing}
              <span class="spinner"></span> Importing...
            {:else}
              Upload Audio File
            {/if}
          </button>
        </div>

        {#if importError}
          <div class="error-text">{importError}</div>
        {/if}
      </div>

    {:else if $audio.state === 'recording'}
      <div class="state-message">
        <div class="state-icon recording-pulse">●</div>
        <h2>Recording in Progress</h2>
        <p>Audio is being captured. Press <strong>Pause</strong> or <strong>Stop</strong> when done.</p>
      </div>

    {:else if $audio.state === 'paused'}
      <div class="state-message">
        <div class="state-icon">⏸</div>
        <h2>Recording Paused</h2>
        <p>Press <strong>Resume</strong> to continue or <strong>Stop</strong> to finish.</p>
      </div>

    {:else if $audio.state === 'stopped'}
      <div class="state-message">
        <div class="state-icon">✓</div>
        <h2>Recording Complete</h2>
        <p>Your recording has been saved.</p>

        {#if !$settings.auto_generate_soap && $audio.lastRecordingId}
          <div class="post-actions">
            <button class="btn-primary" onclick={handleProcessRecording}>
              Process Recording
            </button>
          </div>
        {/if}

        <p class="hint">Or start a <strong>New Recording</strong>.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .record-tab {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Context Panel */
  .context-panel {
    background-color: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .context-toggle {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
    text-align: left;
    cursor: pointer;
    background: none;
    border: none;
  }

  .context-toggle:hover {
    color: var(--text-primary);
  }

  .toggle-arrow {
    font-size: 10px;
    width: 12px;
    text-align: center;
  }

  .context-hint {
    font-weight: 400;
    color: var(--text-muted);
    font-size: 12px;
  }

  .context-textarea {
    display: block;
    width: 100%;
    padding: 8px 16px 12px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-primary);
    background-color: var(--bg-primary);
    border: none;
    border-top: 1px solid var(--border-light, var(--border));
    resize: vertical;
    min-height: 80px;
    max-height: 200px;
  }

  .context-textarea:focus {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .context-textarea::placeholder {
    color: var(--text-muted);
  }

  /* Main Content */
  .record-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 32px;
  }

  .state-message {
    text-align: center;
    max-width: 400px;
  }

  .state-icon {
    font-size: 48px;
    margin-bottom: 16px;
    line-height: 1;
  }

  .recording-pulse {
    color: var(--danger);
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  h2 {
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 8px;
  }

  p {
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.6;
  }

  strong {
    color: var(--text-secondary);
  }

  .post-actions {
    margin-top: 16px;
    margin-bottom: 8px;
    display: flex;
    gap: 10px;
    justify-content: center;
    flex-wrap: wrap;
  }

  .btn-primary {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    background-color: var(--accent);
    color: white;
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-primary:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* Pipeline Status */
  .pipeline-status {
    text-align: center;
    max-width: 500px;
  }

  .pipeline-stages {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  .stage {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-muted);
    transition: color 0.2s ease;
  }

  .stage.active {
    color: var(--accent);
  }

  .stage.done {
    color: var(--success);
  }

  .stage-arrow {
    color: var(--text-muted);
    font-size: 14px;
  }

  .stage-check {
    color: var(--success);
    font-size: 14px;
  }

  .stage-dot {
    color: var(--text-muted);
    font-size: 12px;
  }

  .pipeline-label {
    font-size: 15px;
    font-weight: 500;
    color: var(--text-primary);
    margin-bottom: 8px;
  }

  .spinner {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-text {
    margin-top: 8px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background-color: rgba(239, 68, 68, 0.1);
    color: var(--danger, #ef4444);
    font-size: 13px;
  }

  .hint {
    margin-top: 12px;
    font-size: 13px;
  }

  .btn-upload {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-upload:hover:not(:disabled) {
    background-color: var(--bg-hover);
  }

  .btn-upload:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
```

- [ ] **Step 2: Update RecordingHeader to accept optional callbacks**

The RecordTab now needs to intercept Start and Stop to inject pipeline logic. Modify `src/lib/components/RecordingHeader.svelte` to accept optional `onStart` and `onStop` props. Replace the `<script>` block:

```svelte
<script lang="ts">
  import { audio } from '../stores/audio';
  import { formatDuration } from '../utils/format';
  import Waveform from './Waveform.svelte';

  interface Props {
    onStart?: () => void;
    onStop?: () => void;
  }
  let { onStart, onStop }: Props = $props();

  function handleStart() {
    if (onStart) {
      onStart();
    } else {
      audio.startRecording();
    }
  }

  function handleStop() {
    if (onStop) {
      onStop();
    } else {
      audio.stop();
    }
  }
</script>
```

Then in the template, replace `onclick={() => audio.startRecording()}` with `onclick={handleStart}` and replace `onclick={() => audio.stop()}` (both instances — in `recording` and `paused` states) with `onclick={handleStop}`.

- [ ] **Step 3: Verify in browser**

1. Open the Record tab. Context panel should appear (collapsed by default). Click to expand, paste text.
2. With auto-generate OFF: record, stop, see "Process Recording" button.
3. With auto-generate ON: record, stop, pipeline status appears (Transcribing → SOAP → Done).
4. When pipeline completes, "Copy SOAP Note" button appears.
5. Toast notification appears.
6. Can immediately click "New Recording" and start recording again while pipeline runs.

- [ ] **Step 4: Commit**

```bash
git add src/lib/pages/RecordTab.svelte src/lib/components/RecordingHeader.svelte
git commit -m "feat: redesign Record tab with context panel and pipeline status"
```

---

### Task 7: Integration Test and Polish

**Files:**
- Possibly adjust: `src/App.svelte` (cleanup duplicate listeners)
- Possibly adjust: `src/lib/stores/pipeline.ts` (edge cases)

- [ ] **Step 1: Clean up duplicate generation-progress listener**

In `src/App.svelte`, the existing `generation-progress` listener from the GenerateTab workflow should remain (it serves the Generate tab). The new `pipeline-progress` listener is separate and handles the pipeline events. Verify both co-exist without conflict.

- [ ] **Step 2: End-to-end test with auto-generate ON**

1. Enable "Auto-generate SOAP after recording" in Settings.
2. Expand context panel, paste some text.
3. Click Record, speak, click Stop.
4. Verify: pipeline status appears immediately (Transcribing → Generating SOAP → Done).
5. Verify: toast "SOAP note ready for ..." appears.
6. Click "View" on toast — navigates to SOAP editor tab.
7. Verify: SOAP note content is displayed.

- [ ] **Step 3: End-to-end test with auto-generate OFF**

1. Disable the setting.
2. Record and stop.
3. Verify: "Process Recording" button appears (no auto-start).
4. Click "Process Recording".
5. Same pipeline stages appear.
6. Toast appears on completion.

- [ ] **Step 4: Test concurrent recordings**

1. Enable auto-generate.
2. Record Patient A, stop. Pipeline starts.
3. Immediately click "New Recording", record Patient B, stop.
4. Verify: Patient A's pipeline continues in background.
5. Patient B's pipeline starts.
6. Two separate toasts appear when each completes.

- [ ] **Step 5: Test imported file with auto-generate**

1. Enable auto-generate.
2. Click "Upload Audio File", select a WAV file.
3. Verify: pipeline auto-starts for imported file.
4. Toast appears when done.

- [ ] **Step 6: Test error and retry**

1. Disconnect from internet (or use a recording with no audio content).
2. Run pipeline, wait for failure.
3. Verify: error message appears with "Retry" button.
4. Reconnect, click Retry, verify recovery.

- [ ] **Step 7: Commit final state**

```bash
git add -A
git commit -m "feat: streamlined recording-to-SOAP pipeline complete"
```
