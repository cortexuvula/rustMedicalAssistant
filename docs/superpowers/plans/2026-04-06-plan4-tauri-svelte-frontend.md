# Plan 4: Tauri Bridge & Svelte Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Tauri IPC command layer and the full Svelte 5 frontend — connecting all 12 Rust crates to a professional medical desktop UI with 5 workflow tabs, recording controls, chat interface, dark/light theming, and keyboard shortcuts.

**Architecture:** Tauri commands are thin bridges grouped by domain (audio, recordings, processing, ai, agents, rag, export, settings, translation, tts). Svelte stores provide reactive state. The UI follows a sidebar + tabbed-content layout matching the Python app: sidebar navigation, recording header, 7-tab notebook (Transcript, SOAP, Referral, Letter, Chat, RAG Chat, Context), and status bar.

**Tech Stack:** Tauri v2, Svelte 5, TypeScript, CSS custom properties (theming), HTML5 Canvas (waveform)

**Depends on:** Plans 1-3 (all 12 Rust crates built and tested)

---

## File Structure

```
src-tauri/src/
├── main.rs                         (entry point — unchanged)
├── lib.rs                          (Tauri builder with all commands registered)
├── state.rs                        (AppState — holds Arc refs to all services)
├── commands/
│   ├── mod.rs                      (re-exports all command modules)
│   ├── audio.rs                    (start_recording, stop, pause, resume, list_devices)
│   ├── recordings.rs               (list, search, get, delete, update_tags)
│   ├── processing.rs               (enqueue, cancel, get_queue_status)
│   ├── ai.rs                       (complete, complete_stream, list_providers, list_models)
│   ├── agents.rs                   (execute_agent, cancel_agent, list_agents)
│   ├── rag.rs                      (search, ingest_document, delete_document)
│   ├── export.rs                   (export_pdf, export_docx, export_fhir)
│   ├── settings.rs                 (get_settings, update_settings, get_key, set_key)
│   ├── translation.rs              (translate, get_canned, start_session)
│   └── tts.rs                      (synthesize, list_voices)

src/
├── main.ts                         (mount App)
├── app.css                         (global styles + theme variables)
├── App.svelte                      (root: layout + router)
├── lib/
│   ├── api/                        (Tauri invoke wrappers)
│   │   ├── audio.ts
│   │   ├── recordings.ts
│   │   ├── processing.ts
│   │   ├── ai.ts
│   │   ├── agents.ts
│   │   ├── rag.ts
│   │   ├── export.ts
│   │   ├── settings.ts
│   │   ├── translation.ts
│   │   └── tts.ts
│   ├── stores/                     (Svelte stores — reactive state)
│   │   ├── recordings.ts
│   │   ├── audio.ts
│   │   ├── processing.ts
│   │   ├── ai.ts
│   │   ├── chat.ts
│   │   ├── settings.ts
│   │   └── theme.ts
│   ├── types/                      (TypeScript type definitions)
│   │   └── index.ts
│   ├── components/                 (reusable UI components)
│   │   ├── Sidebar.svelte
│   │   ├── StatusBar.svelte
│   │   ├── RecordingHeader.svelte
│   │   ├── Waveform.svelte
│   │   ├── RecordingCard.svelte
│   │   ├── ChatMessage.svelte
│   │   ├── TextEditor.svelte
│   │   ├── SearchBar.svelte
│   │   ├── TabBar.svelte
│   │   ├── Modal.svelte
│   │   └── KeyboardShortcuts.svelte
│   ├── pages/                      (main tab content)
│   │   ├── RecordTab.svelte
│   │   ├── RecordingsTab.svelte
│   │   ├── GenerateTab.svelte
│   │   ├── ChatTab.svelte
│   │   └── EditorTab.svelte
│   └── dialogs/
│       └── SettingsDialog.svelte
```

---

### Task 1: Tauri AppState and Startup

**Files:**
- Create: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write AppState**

Write `src-tauri/src/state.rs`:
```rust
use medical_db::Database;
use medical_security::key_storage::KeyStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Application state shared across all Tauri commands.
/// Initialized once at startup and passed via tauri::State.
pub struct AppState {
    pub db: Arc<Database>,
    pub keys: Arc<KeyStorage>,
    pub data_dir: PathBuf,
    pub recording_active: Arc<Mutex<bool>>,
}

impl AppState {
    pub fn initialize() -> Result<Self, Box<dyn std::error::Error>> {
        // Resolve data directory (platform-specific)
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rust-medical-assistant");
        std::fs::create_dir_all(&data_dir)?;

        // Open database
        let db_path = data_dir.join("medical.db");
        let db = Database::open(&db_path)?;

        // Open key storage
        let config_dir = data_dir.join("config");
        let keys = KeyStorage::open(&config_dir)?;

        Ok(Self {
            db: Arc::new(db),
            keys: Arc::new(keys),
            data_dir,
            recording_active: Arc::new(Mutex::new(false)),
        })
    }
}
```

Add `dirs = "6"` to `src-tauri/Cargo.toml` dependencies.

- [ ] **Step 2: Update lib.rs to register state and commands**

Replace `src-tauri/src/lib.rs`:
```rust
mod state;
mod commands;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::initialize()
        .expect("Failed to initialize application state");

    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // Recordings
            commands::recordings::list_recordings,
            commands::recordings::get_recording,
            commands::recordings::search_recordings,
            commands::recordings::delete_recording,
            // Settings
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_api_key,
            commands::settings::set_api_key,
            commands::settings::list_api_keys,
            // Export
            commands::export::export_pdf,
            commands::export::export_docx,
            commands::export::export_fhir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Create commands module structure**

Write `src-tauri/src/commands/mod.rs`:
```rust
pub mod recordings;
pub mod settings;
pub mod export;
```

Write `src-tauri/src/commands/recordings.rs`:
```rust
use crate::state::AppState;
use medical_core::types::recording::RecordingSummary;
use medical_db::recordings::RecordingsRepo;
use medical_db::search::SearchRepo;

#[tauri::command]
pub fn list_recordings(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<RecordingSummary>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    RecordingsRepo::list_all(&conn, limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recording(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<medical_core::types::recording::Recording, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_recordings(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<medical_core::types::recording::Recording>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SearchRepo::search_recordings(&conn, &query, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_recording(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    RecordingsRepo::delete(&conn, &uuid).map_err(|e| e.to_string())
}
```

Write `src-tauri/src/commands/settings.rs`:
```rust
use crate::state::AppState;
use medical_core::types::settings::AppConfig;
use medical_db::settings::SettingsRepo;

#[tauri::command]
pub fn get_settings(
    state: tauri::State<'_, AppState>,
) -> Result<AppConfig, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SettingsRepo::load_config(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    SettingsRepo::save_config(&conn, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_api_key(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> Result<Option<String>, String> {
    state.keys.get_key(&provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_api_key(
    state: tauri::State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<(), String> {
    state.keys.store_key(&provider, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_api_keys(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state.keys.list_providers().map_err(|e| e.to_string())
}
```

Write `src-tauri/src/commands/export.rs`:
```rust
use crate::state::AppState;
use medical_db::recordings::RecordingsRepo;
use medical_export::pdf::PdfExporter;
use medical_export::docx::DocxExporter;
use medical_export::fhir::{FhirExporter, PatientInfo, PractitionerInfo};

#[tauri::command]
pub fn export_pdf(
    state: tauri::State<'_, AppState>,
    recording_id: String,
    export_type: String,
) -> Result<Vec<u8>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let uuid = uuid::Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let recording = RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?;

    match export_type.as_str() {
        "soap" => PdfExporter::export_soap(&recording).map_err(|e| e.to_string()),
        "referral" => PdfExporter::export_referral(&recording).map_err(|e| e.to_string()),
        "letter" => PdfExporter::export_letter(&recording).map_err(|e| e.to_string()),
        _ => Err("Invalid export type".into()),
    }
}

#[tauri::command]
pub fn export_docx(
    state: tauri::State<'_, AppState>,
    recording_id: String,
    export_type: String,
) -> Result<Vec<u8>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let uuid = uuid::Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let recording = RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?;

    match export_type.as_str() {
        "soap" => DocxExporter::export_soap(&recording).map_err(|e| e.to_string()),
        "referral" => DocxExporter::export_referral(&recording).map_err(|e| e.to_string()),
        "letter" => DocxExporter::export_letter(&recording).map_err(|e| e.to_string()),
        _ => Err("Invalid export type".into()),
    }
}

#[tauri::command]
pub fn export_fhir(
    state: tauri::State<'_, AppState>,
    recording_id: String,
) -> Result<Vec<u8>, String> {
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let uuid = uuid::Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let recording = RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?;

    let patient = PatientInfo {
        name: recording.patient_name.clone(),
        birth_date: None,
        gender: None,
        identifier: None,
    };
    let practitioner = PractitionerInfo {
        name: None,
        identifier: None,
        specialty: None,
    };

    FhirExporter::export_bundle(&recording, &patient, &practitioner)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Verify Tauri builds**

Run: `cargo build -p rust-medical-assistant`
Expected: Builds successfully.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(tauri): add AppState, startup sequence, and recording/settings/export commands"
```

---

### Task 2: TypeScript Types and API Layer

**Files:**
- Create: `src/lib/types/index.ts`
- Create: `src/lib/api/recordings.ts`
- Create: `src/lib/api/settings.ts`
- Create: `src/lib/api/export.ts`

- [ ] **Step 1: Write TypeScript type definitions**

Write `src/lib/types/index.ts`:
```typescript
// Recording types (mirrors medical_core::types::recording)
export interface Recording {
  id: string;
  filename: string;
  transcript: string | null;
  soap_note: string | null;
  referral: string | null;
  letter: string | null;
  chat: string | null;
  patient_name: string | null;
  audio_path: string;
  duration_seconds: number | null;
  file_size_bytes: number | null;
  stt_provider: string | null;
  ai_provider: string | null;
  tags: string[];
  status: ProcessingStatus;
  created_at: string;
  metadata: any;
}

export type ProcessingStatus =
  | { status: 'pending' }
  | { status: 'processing'; started_at: string }
  | { status: 'completed'; completed_at: string }
  | { status: 'failed'; error: string; retry_count: number };

export interface RecordingSummary {
  id: string;
  filename: string;
  patient_name: string | null;
  status: ProcessingStatus;
  duration_seconds: number | null;
  created_at: string;
  tags: string[];
  has_transcript: boolean;
  has_soap_note: boolean;
  has_referral: boolean;
  has_letter: boolean;
}

// Settings
export interface AppConfig {
  theme: 'dark' | 'light';
  language: string;
  ai_provider: string;
  ai_model: string;
  stt_provider: string;
  tts_provider: string;
  tts_voice: string;
  temperature: number;
  sample_rate: number;
  autosave_enabled: boolean;
  autosave_interval_secs: number;
  search_top_k: number;
  mmr_lambda: number;
  [key: string]: any;
}

// Chat
export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
  agent?: string;
  tool_calls?: ToolCallRecord[];
}

export interface ToolCallRecord {
  tool_name: string;
  arguments: any;
  result: string;
  duration_ms: number;
}

// Processing
export interface ProcessingEvent {
  type: 'step_changed' | 'progress' | 'completed' | 'failed' | 'queue_status';
  recording_id?: string;
  step?: string;
  percent?: number;
  error?: string;
  pending?: number;
  processing?: number;
  completed?: number;
}

// Audio
export interface AudioDevice {
  name: string;
  is_input: boolean;
  is_default: boolean;
  sample_rates: number[];
  channels: number[];
}
```

- [ ] **Step 2: Write API wrappers**

Write `src/lib/api/recordings.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';
import type { Recording, RecordingSummary } from '../types';

export async function listRecordings(limit = 50, offset = 0): Promise<RecordingSummary[]> {
  return invoke('list_recordings', { limit, offset });
}

export async function getRecording(id: string): Promise<Recording> {
  return invoke('get_recording', { id });
}

export async function searchRecordings(query: string, limit = 20): Promise<Recording[]> {
  return invoke('search_recordings', { query, limit });
}

export async function deleteRecording(id: string): Promise<void> {
  return invoke('delete_recording', { id });
}
```

Write `src/lib/api/settings.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';
import type { AppConfig } from '../types';

export async function getSettings(): Promise<AppConfig> {
  return invoke('get_settings');
}

export async function saveSettings(config: AppConfig): Promise<void> {
  return invoke('save_settings', { config });
}

export async function getApiKey(provider: string): Promise<string | null> {
  return invoke('get_api_key', { provider });
}

export async function setApiKey(provider: string, key: string): Promise<void> {
  return invoke('set_api_key', { provider, key });
}

export async function listApiKeys(): Promise<string[]> {
  return invoke('list_api_keys');
}
```

Write `src/lib/api/export.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';

export async function exportPdf(recordingId: string, exportType: 'soap' | 'referral' | 'letter'): Promise<number[]> {
  return invoke('export_pdf', { recordingId, exportType });
}

export async function exportDocx(recordingId: string, exportType: 'soap' | 'referral' | 'letter'): Promise<number[]> {
  return invoke('export_docx', { recordingId, exportType });
}

export async function exportFhir(recordingId: string): Promise<number[]> {
  return invoke('export_fhir', { recordingId });
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/
git commit -m "feat(frontend): add TypeScript types and Tauri API wrappers"
```

---

### Task 3: Svelte Stores

**Files:**
- Create: `src/lib/stores/theme.ts`
- Create: `src/lib/stores/settings.ts`
- Create: `src/lib/stores/recordings.ts`
- Create: `src/lib/stores/audio.ts`
- Create: `src/lib/stores/chat.ts`

- [ ] **Step 1: Write theme store**

Write `src/lib/stores/theme.ts`:
```typescript
import { writable } from 'svelte/store';

export type Theme = 'light' | 'dark';

function createThemeStore() {
  const { subscribe, set, update } = writable<Theme>('light');

  return {
    subscribe,
    set(theme: Theme) {
      document.documentElement.setAttribute('data-theme', theme);
      set(theme);
    },
    toggle() {
      update(current => {
        const next = current === 'light' ? 'dark' : 'light';
        document.documentElement.setAttribute('data-theme', next);
        return next;
      });
    },
  };
}

export const theme = createThemeStore();
```

- [ ] **Step 2: Write settings store**

Write `src/lib/stores/settings.ts`:
```typescript
import { writable } from 'svelte/store';
import type { AppConfig } from '../types';
import { getSettings, saveSettings } from '../api/settings';

const defaultConfig: AppConfig = {
  theme: 'light',
  language: 'en-US',
  ai_provider: 'openai',
  ai_model: 'gpt-4o',
  stt_provider: 'groq',
  tts_provider: 'elevenlabs',
  tts_voice: 'default',
  temperature: 0.4,
  sample_rate: 16000,
  autosave_enabled: true,
  autosave_interval_secs: 60,
  search_top_k: 5,
  mmr_lambda: 0.7,
};

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppConfig>(defaultConfig);

  return {
    subscribe,
    async load() {
      try {
        const config = await getSettings();
        set(config);
      } catch (e) {
        console.error('Failed to load settings:', e);
      }
    },
    async save(config: AppConfig) {
      set(config);
      try {
        await saveSettings(config);
      } catch (e) {
        console.error('Failed to save settings:', e);
      }
    },
    async updateField(key: string, value: any) {
      update(current => {
        const updated = { ...current, [key]: value };
        saveSettings(updated).catch(e => console.error('Save failed:', e));
        return updated;
      });
    },
  };
}

export const settings = createSettingsStore();
```

- [ ] **Step 3: Write recordings store**

Write `src/lib/stores/recordings.ts`:
```typescript
import { writable, derived } from 'svelte/store';
import type { RecordingSummary, Recording } from '../types';
import { listRecordings, getRecording, searchRecordings, deleteRecording } from '../api/recordings';

function createRecordingsStore() {
  const { subscribe, set, update } = writable<RecordingSummary[]>([]);
  const loading = writable(false);
  const searchQuery = writable('');

  return {
    subscribe,
    loading,
    searchQuery,
    async load(limit = 50, offset = 0) {
      loading.set(true);
      try {
        const recordings = await listRecordings(limit, offset);
        set(recordings);
      } catch (e) {
        console.error('Failed to load recordings:', e);
      } finally {
        loading.set(false);
      }
    },
    async search(query: string) {
      searchQuery.set(query);
      if (!query.trim()) {
        return this.load();
      }
      loading.set(true);
      try {
        const results = await searchRecordings(query);
        const summaries: RecordingSummary[] = results.map(r => ({
          id: r.id,
          filename: r.filename,
          patient_name: r.patient_name,
          status: r.status,
          duration_seconds: r.duration_seconds,
          created_at: r.created_at,
          tags: r.tags,
          has_transcript: !!r.transcript,
          has_soap_note: !!r.soap_note,
          has_referral: !!r.referral,
          has_letter: !!r.letter,
        }));
        set(summaries);
      } catch (e) {
        console.error('Search failed:', e);
      } finally {
        loading.set(false);
      }
    },
    async remove(id: string) {
      try {
        await deleteRecording(id);
        update(recs => recs.filter(r => r.id !== id));
      } catch (e) {
        console.error('Delete failed:', e);
      }
    },
  };
}

export const recordings = createRecordingsStore();

// Selected recording for detail view
export const selectedRecording = writable<Recording | null>(null);

export async function selectRecording(id: string) {
  try {
    const recording = await getRecording(id);
    selectedRecording.set(recording);
  } catch (e) {
    console.error('Failed to load recording:', e);
  }
}
```

- [ ] **Step 4: Write audio store**

Write `src/lib/stores/audio.ts`:
```typescript
import { writable } from 'svelte/store';

export type RecordingState = 'idle' | 'recording' | 'paused' | 'stopped';

function createAudioStore() {
  const state = writable<RecordingState>('idle');
  const elapsed = writable(0);
  const waveformData = writable<number[]>([]);
  const deviceName = writable<string>('');

  let timer: ReturnType<typeof setInterval> | null = null;

  return {
    state,
    elapsed,
    waveformData,
    deviceName,

    startRecording(device: string) {
      state.set('recording');
      deviceName.set(device);
      elapsed.set(0);
      waveformData.set([]);
      timer = setInterval(() => {
        elapsed.update(n => n + 1);
      }, 1000);
    },

    pause() {
      state.set('paused');
      if (timer) { clearInterval(timer); timer = null; }
    },

    resume() {
      state.set('recording');
      timer = setInterval(() => {
        elapsed.update(n => n + 1);
      }, 1000);
    },

    stop() {
      state.set('stopped');
      if (timer) { clearInterval(timer); timer = null; }
    },

    reset() {
      state.set('idle');
      elapsed.set(0);
      waveformData.set([]);
      if (timer) { clearInterval(timer); timer = null; }
    },

    pushWaveform(data: number[]) {
      waveformData.set(data);
    },
  };
}

export const audio = createAudioStore();
```

- [ ] **Step 5: Write chat store**

Write `src/lib/stores/chat.ts`:
```typescript
import { writable } from 'svelte/store';
import type { ChatMessage } from '../types';

function createChatStore() {
  const { subscribe, update, set } = writable<ChatMessage[]>([]);
  const isStreaming = writable(false);

  return {
    subscribe,
    isStreaming,

    addUserMessage(content: string) {
      const msg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'user',
        content,
        timestamp: new Date().toISOString(),
      };
      update(msgs => [...msgs, msg]);
      return msg;
    },

    addAssistantMessage(content: string, agent?: string, tool_calls?: any[]) {
      const msg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content,
        timestamp: new Date().toISOString(),
        agent,
        tool_calls,
      };
      update(msgs => [...msgs, msg]);
    },

    appendToLast(delta: string) {
      update(msgs => {
        if (msgs.length === 0) return msgs;
        const last = { ...msgs[msgs.length - 1] };
        last.content += delta;
        return [...msgs.slice(0, -1), last];
      });
    },

    startStreaming() {
      isStreaming.set(true);
      // Add empty assistant message to stream into
      const msg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: '',
        timestamp: new Date().toISOString(),
      };
      update(msgs => [...msgs, msg]);
    },

    stopStreaming() {
      isStreaming.set(false);
    },

    clear() {
      set([]);
    },
  };
}

export const chat = createChatStore();
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/stores/
git commit -m "feat(frontend): add Svelte stores for theme, settings, recordings, audio, and chat"
```

---

### Task 4: Global CSS Theme System

**Files:**
- Create: `src/app.css`

- [ ] **Step 1: Write global CSS with theme variables**

Write `src/app.css`:
```css
/* Theme System — CSS Custom Properties */
:root,
[data-theme='light'] {
  --bg-primary: #ffffff;
  --bg-secondary: #f8f9fa;
  --bg-tertiary: #e9ecef;
  --bg-sidebar: #f1f3f5;
  --bg-card: #ffffff;
  --bg-hover: #e9ecef;
  --bg-active: #dee2e6;
  --bg-input: #ffffff;
  --bg-code: #f4f4f5;

  --text-primary: #212529;
  --text-secondary: #495057;
  --text-muted: #868e96;
  --text-inverse: #ffffff;

  --border: #dee2e6;
  --border-light: #e9ecef;
  --border-focus: #4c6ef5;

  --accent: #4c6ef5;
  --accent-hover: #3b5bdb;
  --accent-light: #dbe4ff;

  --success: #40c057;
  --warning: #fab005;
  --danger: #fa5252;
  --info: #339af0;

  --shadow-sm: 0 1px 2px rgba(0,0,0,0.05);
  --shadow-md: 0 4px 6px rgba(0,0,0,0.07);
  --shadow-lg: 0 10px 15px rgba(0,0,0,0.1);

  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;

  --font-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  --font-mono: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;

  --header-height: 56px;
  --sidebar-width: 240px;
  --statusbar-height: 28px;
}

[data-theme='dark'] {
  --bg-primary: #1a1b1e;
  --bg-secondary: #25262b;
  --bg-tertiary: #2c2e33;
  --bg-sidebar: #1e1f23;
  --bg-card: #25262b;
  --bg-hover: #2c2e33;
  --bg-active: #373a40;
  --bg-input: #2c2e33;
  --bg-code: #25262b;

  --text-primary: #c1c2c5;
  --text-secondary: #909296;
  --text-muted: #5c5f66;
  --text-inverse: #1a1b1e;

  --border: #373a40;
  --border-light: #2c2e33;
  --border-focus: #5c7cfa;

  --accent: #5c7cfa;
  --accent-hover: #748ffc;
  --accent-light: #253366;

  --success: #51cf66;
  --warning: #fcc419;
  --danger: #ff6b6b;
  --info: #4dabf7;

  --shadow-sm: 0 1px 2px rgba(0,0,0,0.2);
  --shadow-md: 0 4px 6px rgba(0,0,0,0.3);
  --shadow-lg: 0 10px 15px rgba(0,0,0,0.4);
}

/* Reset & Globals */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

html, body {
  height: 100%;
  font-family: var(--font-sans);
  font-size: 14px;
  color: var(--text-primary);
  background: var(--bg-primary);
  overflow: hidden;
  -webkit-font-smoothing: antialiased;
}

/* Scrollbar styling */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: var(--border);
  border-radius: 4px;
}
::-webkit-scrollbar-thumb:hover {
  background: var(--text-muted);
}

/* Utility classes */
.text-muted { color: var(--text-muted); }
.text-secondary { color: var(--text-secondary); }
.text-success { color: var(--success); }
.text-warning { color: var(--warning); }
.text-danger { color: var(--danger); }
.text-info { color: var(--info); }

.truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Button base */
button {
  cursor: pointer;
  font-family: inherit;
  font-size: inherit;
  border: none;
  outline: none;
  background: none;
  color: inherit;
}

button:focus-visible {
  outline: 2px solid var(--border-focus);
  outline-offset: 2px;
}

/* Input base */
input, textarea, select {
  font-family: inherit;
  font-size: inherit;
  color: var(--text-primary);
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 6px 10px;
}

input:focus, textarea:focus, select:focus {
  border-color: var(--border-focus);
  outline: none;
  box-shadow: 0 0 0 2px var(--accent-light);
}
```

- [ ] **Step 2: Commit**

```bash
git add src/app.css
git commit -m "feat(frontend): add global CSS theme system with light/dark variables"
```

---

### Task 5: App Layout (Sidebar + Content + Status Bar)

**Files:**
- Modify: `src/App.svelte`
- Create: `src/lib/components/Sidebar.svelte`
- Create: `src/lib/components/StatusBar.svelte`
- Create: `src/lib/components/TabBar.svelte`

- [ ] **Step 1: Write Sidebar component**

Write `src/lib/components/Sidebar.svelte`:
```svelte
<script lang="ts">
  import { theme } from '../stores/theme';

  export let activeTab: string = 'record';

  const navItems = [
    { id: 'record', label: 'Record', icon: '⏺' },
    { id: 'recordings', label: 'Recordings', icon: '📋' },
    { id: 'generate', label: 'Generate', icon: '✨' },
    { id: 'chat', label: 'Chat', icon: '💬' },
  ];

  const editorItems = [
    { id: 'transcript', label: 'Transcript' },
    { id: 'soap', label: 'SOAP Note' },
    { id: 'referral', label: 'Referral' },
    { id: 'letter', label: 'Letter' },
  ];

  function handleNav(id: string) {
    activeTab = id;
  }
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <h2 class="app-title">Medical Assistant</h2>
  </div>

  <nav class="sidebar-nav">
    <div class="nav-section">
      <span class="nav-section-label">Workflow</span>
      {#each navItems as item}
        <button
          class="nav-item"
          class:active={activeTab === item.id}
          on:click={() => handleNav(item.id)}
        >
          <span class="nav-icon">{item.icon}</span>
          <span class="nav-label">{item.label}</span>
        </button>
      {/each}
    </div>

    <div class="nav-section">
      <span class="nav-section-label">Documents</span>
      {#each editorItems as item}
        <button
          class="nav-item"
          class:active={activeTab === item.id}
          on:click={() => handleNav(item.id)}
        >
          <span class="nav-label">{item.label}</span>
        </button>
      {/each}
    </div>
  </nav>

  <div class="sidebar-footer">
    <button class="theme-toggle" on:click={() => theme.toggle()}>
      {$theme === 'light' ? '🌙' : '☀️'} {$theme === 'light' ? 'Dark' : 'Light'}
    </button>
    <button class="nav-item" on:click={() => handleNav('settings')}>
      Settings
    </button>
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-width);
    height: 100%;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .sidebar-header {
    padding: 16px;
    border-bottom: 1px solid var(--border);
  }
  .app-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .sidebar-nav {
    flex: 1;
    padding: 8px;
  }
  .nav-section {
    margin-bottom: 16px;
  }
  .nav-section-label {
    display: block;
    padding: 4px 12px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 13px;
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }
  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .nav-item.active {
    background: var(--accent-light);
    color: var(--accent);
    font-weight: 500;
  }
  .nav-icon {
    font-size: 16px;
    width: 20px;
    text-align: center;
  }
  .sidebar-footer {
    padding: 8px;
    border-top: 1px solid var(--border);
  }
  .theme-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 13px;
    text-align: left;
  }
  .theme-toggle:hover {
    background: var(--bg-hover);
  }
</style>
```

- [ ] **Step 2: Write StatusBar component**

Write `src/lib/components/StatusBar.svelte`:
```svelte
<script lang="ts">
  import { audio } from '../stores/audio';
  import { settings } from '../stores/settings';

  function formatTime(seconds: number): string {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
  }
</script>

<footer class="status-bar">
  <div class="status-left">
    {#if $audio.state !== 'idle'}
      <span class="status-indicator recording">
        {$audio.state === 'recording' ? '● REC' : '⏸ PAUSED'} {formatTime($audio.elapsed)}
      </span>
    {:else}
      <span class="status-indicator">Ready</span>
    {/if}
  </div>
  <div class="status-right">
    <span class="status-provider">AI: {$settings.ai_provider}/{$settings.ai_model}</span>
    <span class="status-provider">STT: {$settings.stt_provider}</span>
  </div>
</footer>

<style>
  .status-bar {
    height: var(--statusbar-height);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
  }
  .status-left, .status-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .status-indicator.recording {
    color: var(--danger);
    font-weight: 600;
  }
  .status-provider {
    color: var(--text-muted);
  }
</style>
```

- [ ] **Step 3: Write TabBar component**

Write `src/lib/components/TabBar.svelte`:
```svelte
<script lang="ts">
  export let tabs: { id: string; label: string }[] = [];
  export let activeTab: string = '';

  function select(id: string) {
    activeTab = id;
  }
</script>

<div class="tab-bar">
  {#each tabs as tab}
    <button
      class="tab"
      class:active={activeTab === tab.id}
      on:click={() => select(tab.id)}
    >
      {tab.label}
    </button>
  {/each}
</div>

<style>
  .tab-bar {
    display: flex;
    border-bottom: 1px solid var(--border);
    background: var(--bg-secondary);
    padding: 0 8px;
    gap: 0;
    overflow-x: auto;
  }
  .tab {
    padding: 8px 16px;
    font-size: 13px;
    color: var(--text-secondary);
    border-bottom: 2px solid transparent;
    white-space: nowrap;
    transition: color 0.15s, border-color 0.15s;
  }
  .tab:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
  .tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
    font-weight: 500;
  }
</style>
```

- [ ] **Step 4: Write root App.svelte**

Replace `src/App.svelte`:
```svelte
<script lang="ts">
  import './app.css';
  import Sidebar from './lib/components/Sidebar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import RecordTab from './lib/pages/RecordTab.svelte';
  import RecordingsTab from './lib/pages/RecordingsTab.svelte';
  import ChatTab from './lib/pages/ChatTab.svelte';
  import EditorTab from './lib/pages/EditorTab.svelte';
  import { settings } from './lib/stores/settings';
  import { theme } from './lib/stores/theme';
  import { onMount } from 'svelte';

  let activeTab = 'record';

  onMount(async () => {
    await settings.load();
    theme.set($settings.theme || 'light');
  });
</script>

<div class="app-layout">
  <Sidebar bind:activeTab />

  <main class="content">
    {#if activeTab === 'record'}
      <RecordTab />
    {:else if activeTab === 'recordings'}
      <RecordingsTab />
    {:else if activeTab === 'chat'}
      <ChatTab />
    {:else if activeTab === 'transcript' || activeTab === 'soap' || activeTab === 'referral' || activeTab === 'letter'}
      <EditorTab tabId={activeTab} />
    {:else if activeTab === 'generate'}
      <div class="placeholder">
        <h2>Generate</h2>
        <p class="text-muted">Select a recording and choose what to generate.</p>
      </div>
    {:else if activeTab === 'settings'}
      <div class="placeholder">
        <h2>Settings</h2>
        <p class="text-muted">Settings dialog — coming in a future task.</p>
      </div>
    {:else}
      <div class="placeholder">
        <h2>{activeTab}</h2>
      </div>
    {/if}
  </main>

  <StatusBar />
</div>

<style>
  .app-layout {
    display: grid;
    grid-template-columns: var(--sidebar-width) 1fr;
    grid-template-rows: 1fr var(--statusbar-height);
    height: 100vh;
  }
  .content {
    grid-column: 2;
    grid-row: 1;
    overflow-y: auto;
    background: var(--bg-primary);
  }
  :global(.app-layout > footer) {
    grid-column: 1 / -1;
    grid-row: 2;
  }
  :global(.app-layout > aside) {
    grid-column: 1;
    grid-row: 1;
  }
  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
  }
</style>
```

- [ ] **Step 5: Verify frontend builds**

Run: `cd /home/cortexuvula/Development/rustMedicalAssistant && npm run build`
Expected: Builds successfully.

- [ ] **Step 6: Commit**

```bash
git add src/
git commit -m "feat(frontend): add app layout with sidebar, status bar, tab navigation, and theme toggle"
```

---

### Task 6: Record Tab Page

**Files:**
- Create: `src/lib/pages/RecordTab.svelte`
- Create: `src/lib/components/Waveform.svelte`
- Create: `src/lib/components/RecordingHeader.svelte`

- [ ] **Step 1: Write Waveform canvas component**

Write `src/lib/components/Waveform.svelte`:
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { audio } from '../stores/audio';

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;

  onMount(() => {
    ctx = canvas.getContext('2d')!;
    drawWaveform([]);
  });

  $: if (ctx && $audio.waveformData) {
    drawWaveform($audio.waveformData);
  }

  function drawWaveform(data: number[]) {
    if (!ctx) return;
    const w = canvas.width;
    const h = canvas.height;
    const midY = h / 2;

    ctx.clearRect(0, 0, w, h);

    // Background
    ctx.fillStyle = 'var(--bg-tertiary)';
    ctx.fillRect(0, 0, w, h);

    // Center line
    ctx.strokeStyle = 'var(--border)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, midY);
    ctx.lineTo(w, midY);
    ctx.stroke();

    if (data.length === 0) return;

    // Waveform bars
    const barWidth = w / data.length;
    const style = getComputedStyle(canvas);
    ctx.fillStyle = style.getPropertyValue('--accent') || '#4c6ef5';

    for (let i = 0; i < data.length; i++) {
      const amplitude = Math.min(data[i], 1.0);
      const barHeight = amplitude * (h * 0.8);
      const x = i * barWidth;
      ctx.fillRect(x, midY - barHeight / 2, Math.max(barWidth - 1, 1), barHeight);
    }
  }
</script>

<canvas bind:this={canvas} class="waveform" width="600" height="80"></canvas>

<style>
  .waveform {
    width: 100%;
    height: 80px;
    border-radius: var(--radius-md);
    background: var(--bg-tertiary);
  }
</style>
```

- [ ] **Step 2: Write RecordingHeader component**

Write `src/lib/components/RecordingHeader.svelte`:
```svelte
<script lang="ts">
  import { audio } from '../stores/audio';
  import Waveform from './Waveform.svelte';

  function formatTime(seconds: number): string {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }

  function handleRecord() {
    audio.startRecording('Default Microphone');
  }
  function handlePause() { audio.pause(); }
  function handleResume() { audio.resume(); }
  function handleStop() { audio.stop(); }
  function handleReset() { audio.reset(); }
</script>

<div class="recording-header">
  <div class="controls">
    {#if $audio.state === 'idle'}
      <button class="btn btn-record" on:click={handleRecord}>
        ⏺ Record
      </button>
    {:else if $audio.state === 'recording'}
      <button class="btn btn-pause" on:click={handlePause}>⏸ Pause</button>
      <button class="btn btn-stop" on:click={handleStop}>⏹ Stop</button>
    {:else if $audio.state === 'paused'}
      <button class="btn btn-resume" on:click={handleResume}>▶ Resume</button>
      <button class="btn btn-stop" on:click={handleStop}>⏹ Stop</button>
    {:else}
      <button class="btn btn-primary" on:click={handleReset}>New Recording</button>
    {/if}

    <span class="timer">{formatTime($audio.elapsed)}</span>
  </div>

  <Waveform />
</div>

<style>
  .recording-header {
    padding: 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-secondary);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .timer {
    font-family: var(--font-mono);
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
    margin-left: 12px;
  }
  .btn {
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-weight: 500;
    font-size: 13px;
    transition: background 0.15s;
  }
  .btn-record {
    background: var(--danger);
    color: white;
  }
  .btn-record:hover { opacity: 0.9; }
  .btn-pause {
    background: var(--warning);
    color: var(--text-inverse);
  }
  .btn-stop {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border);
  }
  .btn-resume {
    background: var(--success);
    color: white;
  }
  .btn-primary {
    background: var(--accent);
    color: white;
  }
  .btn-primary:hover { background: var(--accent-hover); }
</style>
```

- [ ] **Step 3: Write RecordTab page**

Write `src/lib/pages/RecordTab.svelte`:
```svelte
<script lang="ts">
  import RecordingHeader from '../components/RecordingHeader.svelte';
  import { audio } from '../stores/audio';
</script>

<div class="record-tab">
  <RecordingHeader />

  <div class="record-content">
    {#if $audio.state === 'idle'}
      <div class="empty-state">
        <h2>Ready to Record</h2>
        <p class="text-muted">Press the Record button or use <kbd>Ctrl+R</kbd> to start recording a medical consultation.</p>
      </div>
    {:else if $audio.state === 'stopped'}
      <div class="empty-state">
        <h2>Recording Complete</h2>
        <p class="text-muted">Your recording is ready. Switch to the Process tab to transcribe and generate documentation.</p>
      </div>
    {:else}
      <div class="recording-info">
        <p>Recording in progress. Speak clearly into the microphone.</p>
        <p class="text-muted">The waveform above shows your audio input level.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .record-tab {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .record-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .empty-state {
    text-align: center;
  }
  .empty-state h2 {
    margin-bottom: 8px;
    font-size: 20px;
  }
  .recording-info {
    text-align: center;
  }
  kbd {
    padding: 2px 6px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 3px;
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/Waveform.svelte src/lib/components/RecordingHeader.svelte src/lib/pages/RecordTab.svelte
git commit -m "feat(frontend): add record tab with waveform display and recording controls"
```

---

### Task 7: Recordings Tab Page

**Files:**
- Create: `src/lib/pages/RecordingsTab.svelte`
- Create: `src/lib/components/RecordingCard.svelte`
- Create: `src/lib/components/SearchBar.svelte`

- [ ] **Step 1: Write SearchBar component**

Write `src/lib/components/SearchBar.svelte`:
```svelte
<script lang="ts">
  export let value: string = '';
  export let placeholder: string = 'Search...';
  export let onSearch: (query: string) => void = () => {};

  let timer: ReturnType<typeof setTimeout>;

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    value = target.value;
    clearTimeout(timer);
    timer = setTimeout(() => onSearch(value), 300);
  }
</script>

<div class="search-bar">
  <input
    type="text"
    {placeholder}
    {value}
    on:input={handleInput}
  />
</div>

<style>
  .search-bar {
    padding: 12px 16px;
  }
  input {
    width: 100%;
    padding: 8px 12px;
    border-radius: var(--radius-md);
  }
</style>
```

- [ ] **Step 2: Write RecordingCard component**

Write `src/lib/components/RecordingCard.svelte`:
```svelte
<script lang="ts">
  import type { RecordingSummary } from '../types';

  export let recording: RecordingSummary;
  export let selected: boolean = false;
  export let onClick: () => void = () => {};

  function statusIcon(status: RecordingSummary['status']): string {
    switch (status.status) {
      case 'completed': return '✓';
      case 'processing': return '⟳';
      case 'failed': return '✗';
      default: return '—';
    }
  }

  function statusColor(status: RecordingSummary['status']): string {
    switch (status.status) {
      case 'completed': return 'var(--success)';
      case 'processing': return 'var(--info)';
      case 'failed': return 'var(--danger)';
      default: return 'var(--text-muted)';
    }
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  function formatDuration(seconds: number | null): string {
    if (!seconds) return '--:--';
    const m = Math.floor(seconds / 60);
    const s = Math.round(seconds % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }
</script>

<button class="recording-card" class:selected on:click={onClick}>
  <span class="status-icon" style="color: {statusColor(recording.status)}">
    {statusIcon(recording.status)}
  </span>
  <div class="card-body">
    <div class="card-title truncate">
      {recording.patient_name || recording.filename}
    </div>
    <div class="card-meta text-muted">
      {formatDate(recording.created_at)} · {formatDuration(recording.duration_seconds)}
    </div>
  </div>
  <div class="card-badges">
    {#if recording.has_transcript}<span class="badge">T</span>{/if}
    {#if recording.has_soap_note}<span class="badge">S</span>{/if}
    {#if recording.has_referral}<span class="badge">R</span>{/if}
  </div>
</button>

<style>
  .recording-card {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 10px 16px;
    text-align: left;
    border-bottom: 1px solid var(--border-light);
    transition: background 0.1s;
  }
  .recording-card:hover { background: var(--bg-hover); }
  .recording-card.selected { background: var(--accent-light); }
  .status-icon { font-size: 16px; font-weight: bold; width: 20px; text-align: center; }
  .card-body { flex: 1; min-width: 0; }
  .card-title { font-size: 13px; font-weight: 500; }
  .card-meta { font-size: 11px; margin-top: 2px; }
  .card-badges { display: flex; gap: 4px; }
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    font-size: 10px;
    font-weight: 600;
    border-radius: 50%;
    background: var(--bg-tertiary);
    color: var(--text-secondary);
  }
</style>
```

- [ ] **Step 3: Write RecordingsTab page**

Write `src/lib/pages/RecordingsTab.svelte`:
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { recordings, selectedRecording, selectRecording } from '../stores/recordings';
  import SearchBar from '../components/SearchBar.svelte';
  import RecordingCard from '../components/RecordingCard.svelte';

  onMount(() => { recordings.load(); });

  function handleSearch(query: string) {
    recordings.search(query);
  }
</script>

<div class="recordings-tab">
  <SearchBar placeholder="Search recordings..." onSearch={handleSearch} />

  <div class="recordings-list">
    {#if $recordings.loading}
      <div class="loading">Loading...</div>
    {:else if $recordings.length === 0}
      <div class="empty">
        <p class="text-muted">No recordings found.</p>
      </div>
    {:else}
      {#each $recordings as recording}
        <RecordingCard
          {recording}
          selected={$selectedRecording?.id === recording.id}
          onClick={() => selectRecording(recording.id)}
        />
      {/each}
    {/if}
  </div>
</div>

<style>
  .recordings-tab {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .recordings-list {
    flex: 1;
    overflow-y: auto;
  }
  .loading, .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px;
  }
</style>
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/SearchBar.svelte src/lib/components/RecordingCard.svelte src/lib/pages/RecordingsTab.svelte
git commit -m "feat(frontend): add recordings tab with search, list, and status indicators"
```

---

### Task 8: Editor Tab (SOAP, Referral, Letter, Transcript)

**Files:**
- Create: `src/lib/pages/EditorTab.svelte`
- Create: `src/lib/components/TextEditor.svelte`

- [ ] **Step 1: Write TextEditor component**

Write `src/lib/components/TextEditor.svelte`:
```svelte
<script lang="ts">
  export let value: string = '';
  export let placeholder: string = '';
  export let readonly: boolean = false;
  export let onChange: (value: string) => void = () => {};

  function handleInput(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    value = target.value;
    onChange(value);
  }
</script>

<div class="text-editor">
  <textarea
    {value}
    {placeholder}
    {readonly}
    spellcheck="true"
    on:input={handleInput}
  ></textarea>
</div>

<style>
  .text-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  textarea {
    flex: 1;
    width: 100%;
    resize: none;
    padding: 16px;
    font-size: 14px;
    line-height: 1.6;
    border: none;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-family: var(--font-sans);
  }
  textarea:focus {
    outline: none;
    box-shadow: none;
  }
  textarea::placeholder {
    color: var(--text-muted);
  }
</style>
```

- [ ] **Step 2: Write EditorTab page**

Write `src/lib/pages/EditorTab.svelte`:
```svelte
<script lang="ts">
  import { selectedRecording } from '../stores/recordings';
  import TextEditor from '../components/TextEditor.svelte';

  export let tabId: string;

  const tabConfig: Record<string, { field: string; label: string; placeholder: string }> = {
    transcript: { field: 'transcript', label: 'Transcript', placeholder: 'Transcript will appear here after processing...' },
    soap: { field: 'soap_note', label: 'SOAP Note', placeholder: 'SOAP note will appear here after generation...' },
    referral: { field: 'referral', label: 'Referral Letter', placeholder: 'Referral letter will appear here...' },
    letter: { field: 'letter', label: 'Patient Letter', placeholder: 'Patient letter will appear here...' },
  };

  $: config = tabConfig[tabId] || tabConfig.transcript;
  $: content = $selectedRecording ? ($selectedRecording as any)[config.field] || '' : '';
</script>

<div class="editor-tab">
  <div class="editor-header">
    <h2>{config.label}</h2>
    {#if $selectedRecording}
      <span class="text-muted">
        {$selectedRecording.patient_name || $selectedRecording.filename}
      </span>
    {/if}
  </div>

  {#if $selectedRecording}
    <TextEditor value={content} placeholder={config.placeholder} />
  {:else}
    <div class="empty-state">
      <p class="text-muted">Select a recording from the Recordings tab to view its {config.label.toLowerCase()}.</p>
    </div>
  {/if}
</div>

<style>
  .editor-tab {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .editor-header {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .editor-header h2 {
    font-size: 16px;
    font-weight: 600;
  }
  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/TextEditor.svelte src/lib/pages/EditorTab.svelte
git commit -m "feat(frontend): add editor tab with text editor for transcript, SOAP, referral, and letter"
```

---

### Task 9: Chat Tab

**Files:**
- Create: `src/lib/pages/ChatTab.svelte`
- Create: `src/lib/components/ChatMessage.svelte`

- [ ] **Step 1: Write ChatMessage component**

Write `src/lib/components/ChatMessage.svelte`:
```svelte
<script lang="ts">
  import type { ChatMessage as ChatMsg } from '../types';

  export let message: ChatMsg;

  function formatTime(iso: string): string {
    return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
</script>

<div class="chat-message" class:user={message.role === 'user'} class:assistant={message.role === 'assistant'}>
  <div class="message-header">
    <span class="role">{message.role === 'user' ? 'You' : message.agent || 'Assistant'}</span>
    <span class="time text-muted">{formatTime(message.timestamp)}</span>
  </div>
  <div class="message-content">
    {message.content}
  </div>
  {#if message.tool_calls && message.tool_calls.length > 0}
    <div class="tool-calls">
      {#each message.tool_calls as tc}
        <div class="tool-call">
          <span class="tool-name">{tc.tool_name}</span>
          <span class="tool-duration text-muted">{tc.duration_ms}ms</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .chat-message {
    padding: 12px 16px;
    margin: 4px 0;
  }
  .chat-message.user {
    background: var(--accent-light);
    border-radius: var(--radius-md);
    margin-left: 40px;
  }
  .chat-message.assistant {
    background: var(--bg-card);
    border-radius: var(--radius-md);
    margin-right: 40px;
    border: 1px solid var(--border-light);
  }
  .message-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  .role { font-weight: 600; font-size: 12px; }
  .time { font-size: 11px; }
  .message-content {
    font-size: 14px;
    line-height: 1.5;
    white-space: pre-wrap;
  }
  .tool-calls {
    margin-top: 8px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tool-call {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    font-size: 11px;
    background: var(--bg-tertiary);
    border-radius: 12px;
  }
  .tool-name { font-weight: 500; }
</style>
```

- [ ] **Step 2: Write ChatTab page**

Write `src/lib/pages/ChatTab.svelte`:
```svelte
<script lang="ts">
  import { chat } from '../stores/chat';
  import ChatMessage from '../components/ChatMessage.svelte';
  import { tick } from 'svelte';

  let inputValue = '';
  let chatContainer: HTMLDivElement;

  async function sendMessage() {
    if (!inputValue.trim()) return;
    const msg = inputValue.trim();
    inputValue = '';

    chat.addUserMessage(msg);
    await scrollToBottom();

    // Simulate assistant response (real implementation calls Tauri command)
    chat.startStreaming();
    await scrollToBottom();

    // Mock streaming delay
    const response = `I understand you're asking about "${msg}". As a medical AI assistant, I can help with clinical questions, medication information, diagnostic guidance, and documentation. How can I assist you further?`;
    for (const char of response) {
      chat.appendToLast(char);
      await new Promise(r => setTimeout(r, 10));
    }
    chat.stopStreaming();
    await scrollToBottom();
  }

  async function scrollToBottom() {
    await tick();
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }
</script>

<div class="chat-tab">
  <div class="chat-messages" bind:this={chatContainer}>
    {#if $chat.length === 0}
      <div class="empty-state">
        <h3>Medical AI Chat</h3>
        <p class="text-muted">Ask questions about clinical guidelines, medications, diagnoses, or get help with documentation.</p>
      </div>
    {:else}
      {#each $chat as message}
        <ChatMessage {message} />
      {/each}
    {/if}
  </div>

  <div class="chat-input">
    <textarea
      bind:value={inputValue}
      placeholder="Ask a medical question..."
      rows="2"
      on:keydown={handleKeydown}
    ></textarea>
    <button class="send-btn" on:click={sendMessage} disabled={!inputValue.trim()}>
      Send
    </button>
  </div>
</div>

<style>
  .chat-tab {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    text-align: center;
    gap: 8px;
  }
  .chat-input {
    display: flex;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
    background: var(--bg-secondary);
  }
  .chat-input textarea {
    flex: 1;
    resize: none;
    border-radius: var(--radius-md);
    padding: 10px 12px;
    font-size: 14px;
  }
  .send-btn {
    padding: 8px 20px;
    background: var(--accent);
    color: white;
    border-radius: var(--radius-md);
    font-weight: 500;
    align-self: flex-end;
  }
  .send-btn:hover:not(:disabled) { background: var(--accent-hover); }
  .send-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
```

- [ ] **Step 3: Write GenerateTab placeholder**

Write `src/lib/pages/GenerateTab.svelte`:
```svelte
<script lang="ts">
  import { selectedRecording } from '../stores/recordings';
</script>

<div class="generate-tab">
  {#if $selectedRecording}
    <div class="generate-content">
      <h2>Generate Documentation</h2>
      <p class="text-muted">Recording: {$selectedRecording.patient_name || $selectedRecording.filename}</p>

      <div class="generate-options">
        <button class="generate-btn" disabled={!!$selectedRecording.soap_note}>
          Generate SOAP Note
          {#if $selectedRecording.soap_note}<span class="badge-done">Done</span>{/if}
        </button>
        <button class="generate-btn" disabled={!!$selectedRecording.referral}>
          Generate Referral
          {#if $selectedRecording.referral}<span class="badge-done">Done</span>{/if}
        </button>
        <button class="generate-btn" disabled={!!$selectedRecording.letter}>
          Generate Letter
          {#if $selectedRecording.letter}<span class="badge-done">Done</span>{/if}
        </button>
      </div>
    </div>
  {:else}
    <div class="empty-state">
      <h2>Generate</h2>
      <p class="text-muted">Select a recording from the Recordings tab first.</p>
    </div>
  {/if}
</div>

<style>
  .generate-tab { height: 100%; display: flex; align-items: center; justify-content: center; }
  .generate-content { text-align: center; }
  .generate-options { display: flex; flex-direction: column; gap: 8px; margin-top: 24px; }
  .generate-btn {
    padding: 12px 24px;
    background: var(--accent);
    color: white;
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .generate-btn:hover:not(:disabled) { background: var(--accent-hover); }
  .generate-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .badge-done { font-size: 11px; background: var(--success); padding: 2px 8px; border-radius: 12px; }
  .empty-state { text-align: center; }
</style>
```

- [ ] **Step 4: Update App.svelte to import GenerateTab**

Add import and route for GenerateTab in App.svelte.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/ChatMessage.svelte src/lib/pages/ChatTab.svelte src/lib/pages/GenerateTab.svelte src/App.svelte
git commit -m "feat(frontend): add chat tab with streaming messages and generate tab"
```

---

### Task 10: Settings Dialog

**Files:**
- Create: `src/lib/dialogs/SettingsDialog.svelte`
- Create: `src/lib/components/Modal.svelte`

- [ ] **Step 1: Write Modal component**

Write `src/lib/components/Modal.svelte`:
```svelte
<script lang="ts">
  export let open: boolean = false;
  export let title: string = '';
  export let onClose: () => void = () => {};

  function handleBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if open}
  <div class="modal-backdrop" on:click={handleBackdrop} role="dialog" aria-modal="true">
    <div class="modal">
      <div class="modal-header">
        <h2>{title}</h2>
        <button class="close-btn" on:click={onClose}>×</button>
      </div>
      <div class="modal-body">
        <slot />
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .modal {
    background: var(--bg-primary);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    width: 90%;
    max-width: 600px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }
  .modal-header h2 { font-size: 16px; }
  .close-btn {
    font-size: 24px;
    line-height: 1;
    color: var(--text-muted);
    padding: 4px 8px;
  }
  .close-btn:hover { color: var(--text-primary); }
  .modal-body {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
  }
</style>
```

- [ ] **Step 2: Write SettingsDialog**

Write `src/lib/dialogs/SettingsDialog.svelte`:
```svelte
<script lang="ts">
  import Modal from '../components/Modal.svelte';
  import { settings } from '../stores/settings';
  import { theme } from '../stores/theme';
  import { listApiKeys, setApiKey, getApiKey } from '../api/settings';

  export let open: boolean = false;

  let activeSection = 'general';
  let apiKeyInputs: Record<string, string> = {};
  let storedKeys: string[] = [];

  const sections = [
    { id: 'general', label: 'General' },
    { id: 'api_keys', label: 'API Keys' },
    { id: 'ai', label: 'AI Models' },
    { id: 'audio', label: 'Audio/STT' },
  ];

  const providers = ['openai', 'anthropic', 'gemini', 'groq', 'cerebras', 'deepgram', 'elevenlabs'];

  async function loadKeys() {
    storedKeys = await listApiKeys();
  }

  async function saveKey(provider: string) {
    const key = apiKeyInputs[provider];
    if (key) {
      await setApiKey(provider, key);
      apiKeyInputs[provider] = '';
      await loadKeys();
    }
  }

  function close() { open = false; }

  $: if (open) loadKeys();
</script>

<Modal {open} title="Settings" onClose={close}>
  <div class="settings-layout">
    <nav class="settings-nav">
      {#each sections as section}
        <button
          class="settings-nav-item"
          class:active={activeSection === section.id}
          on:click={() => activeSection = section.id}
        >
          {section.label}
        </button>
      {/each}
    </nav>

    <div class="settings-content">
      {#if activeSection === 'general'}
        <div class="setting-group">
          <label>Theme</label>
          <select value={$settings.theme} on:change={(e) => {
            const val = (e.target as HTMLSelectElement).value as 'light' | 'dark';
            theme.set(val);
            settings.updateField('theme', val);
          }}>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </div>
        <div class="setting-group">
          <label>Autosave</label>
          <input type="checkbox" checked={$settings.autosave_enabled}
            on:change={(e) => settings.updateField('autosave_enabled', (e.target as HTMLInputElement).checked)} />
        </div>
        <div class="setting-group">
          <label>Autosave Interval (seconds)</label>
          <input type="number" value={$settings.autosave_interval_secs} min="10" max="600"
            on:change={(e) => settings.updateField('autosave_interval_secs', parseInt((e.target as HTMLInputElement).value))} />
        </div>

      {:else if activeSection === 'api_keys'}
        {#each providers as provider}
          <div class="setting-group">
            <label>
              {provider}
              {#if storedKeys.includes(provider)}
                <span class="key-badge">Stored</span>
              {/if}
            </label>
            <div class="key-input-row">
              <input type="password" placeholder="Enter API key..."
                bind:value={apiKeyInputs[provider]} />
              <button class="save-key-btn" on:click={() => saveKey(provider)}>Save</button>
            </div>
          </div>
        {/each}

      {:else if activeSection === 'ai'}
        <div class="setting-group">
          <label>AI Provider</label>
          <select value={$settings.ai_provider}
            on:change={(e) => settings.updateField('ai_provider', (e.target as HTMLSelectElement).value)}>
            <option value="openai">OpenAI</option>
            <option value="anthropic">Anthropic</option>
            <option value="gemini">Google Gemini</option>
            <option value="groq">Groq</option>
            <option value="cerebras">Cerebras</option>
            <option value="ollama">Ollama</option>
          </select>
        </div>
        <div class="setting-group">
          <label>Temperature</label>
          <input type="range" min="0" max="2" step="0.1" value={$settings.temperature}
            on:input={(e) => settings.updateField('temperature', parseFloat((e.target as HTMLInputElement).value))} />
          <span>{$settings.temperature}</span>
        </div>

      {:else if activeSection === 'audio'}
        <div class="setting-group">
          <label>STT Provider</label>
          <select value={$settings.stt_provider}
            on:change={(e) => settings.updateField('stt_provider', (e.target as HTMLSelectElement).value)}>
            <option value="deepgram">Deepgram</option>
            <option value="groq_whisper">Groq Whisper</option>
            <option value="elevenlabs">ElevenLabs</option>
          </select>
        </div>
        <div class="setting-group">
          <label>Sample Rate</label>
          <select value={$settings.sample_rate}
            on:change={(e) => settings.updateField('sample_rate', parseInt((e.target as HTMLSelectElement).value))}>
            <option value={16000}>16 kHz</option>
            <option value={44100}>44.1 kHz</option>
            <option value={48000}>48 kHz</option>
          </select>
        </div>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .settings-layout { display: flex; gap: 16px; min-height: 400px; }
  .settings-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 120px;
    border-right: 1px solid var(--border);
    padding-right: 16px;
  }
  .settings-nav-item {
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    text-align: left;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .settings-nav-item:hover { background: var(--bg-hover); }
  .settings-nav-item.active { background: var(--accent-light); color: var(--accent); font-weight: 500; }
  .settings-content { flex: 1; }
  .setting-group { margin-bottom: 16px; }
  .setting-group label { display: block; font-size: 13px; font-weight: 500; margin-bottom: 4px; }
  .setting-group select, .setting-group input[type="number"] { width: 100%; }
  .key-badge {
    font-size: 10px;
    background: var(--success);
    color: white;
    padding: 1px 6px;
    border-radius: 8px;
    margin-left: 6px;
  }
  .key-input-row { display: flex; gap: 8px; }
  .key-input-row input { flex: 1; }
  .save-key-btn {
    padding: 6px 12px;
    background: var(--accent);
    color: white;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }
</style>
```

- [ ] **Step 3: Wire settings dialog into App.svelte**

Update App.svelte to add settings dialog toggle when activeTab === 'settings'.

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/Modal.svelte src/lib/dialogs/SettingsDialog.svelte src/App.svelte
git commit -m "feat(frontend): add settings dialog with API keys, AI models, audio, and general tabs"
```

---

### Task 11: Final Verification

- [ ] **Step 1: Install frontend deps and build**

Run:
```bash
cd /home/cortexuvula/Development/rustMedicalAssistant
npm install
npm run build
```
Expected: Vite builds successfully.

- [ ] **Step 2: Build Tauri**

Run: `cargo build --workspace`
Expected: Clean build.

- [ ] **Step 3: Run all Rust tests**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --workspace`
Fix any warnings.

- [ ] **Step 5: Commit and push**

```bash
git add -A
git commit -m "fix: address clippy and build warnings for Plan 4"
git push origin master
```
