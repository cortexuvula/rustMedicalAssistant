# Plan 5: Full Feature Wiring

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire all existing Rust crate implementations to Tauri IPC commands and connect the Svelte frontend to produce working end-to-end flows: audio recording, transcription, AI chat, document generation, agent execution, and export.

**Architecture:** Expand AppState to hold provider registries (AI, STT, TTS). Add ~20 new Tauri commands across 5 new command modules. Create frontend API wrappers and update stores/pages to call real backend. Use Tauri events for streaming (waveform data, chat tokens, processing progress).

**Tech Stack:** Tauri v2 events API, tokio channels, serde, Svelte 5 stores with `@tauri-apps/api/event`

---

### Task 1: Expand AppState with Provider Registries

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` (add tracing dep)

- [ ] **Step 1: Update AppState struct**

Add AI provider registry, STT failover, agent orchestrator, and audio capture handle to AppState:

```rust
// src-tauri/src/state.rs
use medical_ai_providers::ProviderRegistry;
use medical_stt_providers::failover::SttFailover;
use medical_agents::orchestrator::AgentOrchestrator;
use medical_agents::tools::ToolRegistry;
use medical_audio::capture::CaptureHandle;
use medical_db::Database;
use medical_security::key_storage::KeyStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub db: Arc<Database>,
    pub keys: Arc<KeyStorage>,
    pub data_dir: PathBuf,
    pub recording_active: Arc<Mutex<bool>>,
    pub ai_providers: Arc<Mutex<ProviderRegistry>>,
    pub stt_providers: Arc<Mutex<Option<SttFailover>>>,
    pub orchestrator: Arc<AgentOrchestrator>,
    pub capture_handle: Arc<Mutex<Option<CaptureHandle>>>,
    pub waveform_rx: Arc<Mutex<Option<std::sync::mpsc::Receiver<Vec<f32>>>>>,
}
```

- [ ] **Step 2: Add provider initialization helper**

Create `init_providers()` that reads API keys from KeyStorage and builds the provider registry:

```rust
fn init_ai_providers(keys: &KeyStorage) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    
    // Register providers that have API keys configured
    if let Ok(key) = keys.get_key("openai") {
        registry.register(Arc::new(medical_ai_providers::openai::OpenAiProvider::new(&key)));
    }
    if let Ok(key) = keys.get_key("anthropic") {
        registry.register(Arc::new(medical_ai_providers::anthropic::AnthropicProvider::new(&key)));
    }
    // ... groq, cerebras, gemini
    
    // Always register Ollama (no key needed)
    registry.register(Arc::new(medical_ai_providers::ollama::OllamaProvider::new(None)));
    
    registry
}
```

- [ ] **Step 3: Update AppState::initialize()**

Wire the new fields into initialization:

```rust
impl AppState {
    pub fn initialize() -> Result<Self, Box<dyn std::error::Error>> {
        // ... existing db + keys init ...
        
        let ai_providers = init_ai_providers(&keys);
        let stt = init_stt_providers(&keys);
        let tool_registry = ToolRegistry::with_defaults();
        let orchestrator = AgentOrchestrator::new(tool_registry);
        
        Ok(Self {
            db: Arc::new(db),
            keys: Arc::new(keys),
            data_dir,
            recording_active: Arc::new(Mutex::new(false)),
            ai_providers: Arc::new(Mutex::new(ai_providers)),
            stt_providers: Arc::new(Mutex::new(stt)),
            orchestrator: Arc::new(orchestrator),
            capture_handle: Arc::new(Mutex::new(None)),
            waveform_rx: Arc::new(Mutex::new(None)),
        })
    }
}
```

- [ ] **Step 4: Add reinit_providers command**

Tauri command that re-reads API keys and rebuilds providers (called after settings change):

```rust
#[tauri::command]
async fn reinit_providers(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let providers = init_ai_providers(&state.keys);
    let available = providers.list_available();
    *state.ai_providers.lock().await = providers;
    // Also rebuild STT
    let stt = init_stt_providers(&state.keys);
    *state.stt_providers.lock().await = stt;
    Ok(available)
}
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p rust-medical-assistant`
Expected: Compiles with only the existing dead_code warnings.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: expand AppState with AI/STT provider registries and agent orchestrator"
```

---

### Task 2: Audio Recording Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create audio command module**

Implement these commands:
- `list_audio_devices` — returns list of input device names
- `start_recording` — starts cpal capture, stores handle in AppState, spawns waveform emitter
- `stop_recording` — stops capture, creates Recording in DB, returns recording ID
- `pause_recording` — pauses capture
- `resume_recording` — resumes capture

Key implementation detail for `start_recording`: Use `tauri::Emitter` to emit waveform events:

```rust
#[tauri::command]
async fn start_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let device = medical_audio::device::get_input_device(None)
        .map_err(|e| e.to_string())?;
    let config = medical_audio::capture::CaptureConfig::default();
    let wav_path = state.data_dir.join("recordings")
        .join(format!("{}.wav", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(wav_path.parent().unwrap()).map_err(|e| e.to_string())?;
    
    let (handle, waveform_rx) = medical_audio::capture::start_capture(&device, config, &wav_path)
        .map_err(|e| e.to_string())?;
    
    *state.capture_handle.lock().await = Some(handle);
    *state.recording_active.lock().await = true;
    
    // Spawn a task to forward waveform data as Tauri events
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        while let Ok(data) = waveform_rx.recv() {
            let _ = app_clone.emit("waveform-data", &data);
        }
    });
    
    Ok(())
}
```

For `stop_recording`: Drop the CaptureHandle (which flushes WAV), create a Recording entry in the DB:

```rust
#[tauri::command]
async fn stop_recording(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let handle = state.capture_handle.lock().await.take();
    if let Some(h) = handle {
        h.stop();
    }
    *state.recording_active.lock().await = false;
    
    // Create recording entry in DB
    let recording = Recording::new();
    // ... set fields, insert into DB ...
    
    Ok(recording.id.to_string())
}
```

- [ ] **Step 2: Register commands in lib.rs**

Add to invoke_handler: `start_recording`, `stop_recording`, `pause_recording`, `resume_recording`, `list_audio_devices`

- [ ] **Step 3: Build and test**

Run: `cargo build -p rust-medical-assistant`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/audio.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add audio recording Tauri commands with waveform streaming"
```

---

### Task 3: AI Chat Tauri Commands (Streaming)

**Files:**
- Create: `src-tauri/src/commands/chat.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create chat command module**

Implement:
- `chat_send` — non-streaming completion, returns full response
- `chat_stream` — streaming completion via Tauri events

For streaming, use `futures_util::StreamExt` to consume the provider's stream and emit token events:

```rust
#[tauri::command]
async fn chat_stream(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    messages: Vec<ChatMessageInput>,
    model: Option<String>,
) -> Result<(), String> {
    let providers = state.ai_providers.lock().await;
    let provider = providers.active().ok_or("No AI provider configured")?;
    
    let request = CompletionRequest {
        model: model.unwrap_or_else(|| "gpt-4o".into()),
        messages: convert_messages(&messages),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        system_prompt: Some("You are a medical AI assistant.".into()),
    };
    
    let mut stream = provider.complete_stream(request).await.map_err(|e| e.to_string())?;
    
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(StreamChunk::Delta { content, .. }) => {
                let _ = app.emit("chat-token", &content);
            }
            Ok(StreamChunk::Done { usage, .. }) => {
                let _ = app.emit("chat-done", &usage);
            }
            Err(e) => {
                let _ = app.emit("chat-error", &e.to_string());
                break;
            }
        }
    }
    
    Ok(())
}
```

- [ ] **Step 2: Add agent chat command**

`chat_with_agent` — runs the agent orchestrator for a user message:

```rust
#[tauri::command]
async fn chat_with_agent(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    message: String,
    agent_name: String,
) -> Result<serde_json::Value, String> {
    let providers = state.ai_providers.lock().await;
    let provider = providers.active().ok_or("No AI provider configured")?;
    
    let agent = get_agent_by_name(&agent_name).ok_or("Unknown agent")?;
    let context = AgentContext {
        user_message: message,
        conversation_history: vec![],
        patient_context: None,
        rag_context: vec![],
        recording: None,
    };
    
    let cancel = tokio_util::sync::CancellationToken::new();
    let response = state.orchestrator.execute(agent.as_ref(), context, provider, cancel)
        .await.map_err(|e| e.to_string())?;
    
    serde_json::to_value(&response).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Register commands, build, commit**

---

### Task 4: STT Transcription Command

**Files:**
- Create: `src-tauri/src/commands/transcription.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create transcription command**

`transcribe_recording` — loads WAV file, sends to STT failover, updates recording:

```rust
#[tauri::command]
async fn transcribe_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
) -> Result<String, String> {
    let _ = app.emit("transcription-progress", "loading_audio");
    
    // Load recording from DB
    let recording = state.db.conn()
        .and_then(|c| medical_db::recordings::RecordingsRepo::get_by_id(&c, &recording_id))
        .map_err(|e| e.to_string())?
        .ok_or("Recording not found")?;
    
    // Load WAV file into AudioData
    let wav_path = state.data_dir.join("recordings").join(format!("{}.wav", recording_id));
    let audio = load_wav_to_audio_data(&wav_path).map_err(|e| e.to_string())?;
    
    let _ = app.emit("transcription-progress", "transcribing");
    
    // Transcribe via failover chain
    let stt_guard = state.stt_providers.lock().await;
    let stt = stt_guard.as_ref().ok_or("No STT provider configured")?;
    let config = SttConfig::default();
    let transcript = stt.transcribe(audio, config).await.map_err(|e| e.to_string())?;
    
    // Update recording in DB with transcript
    // ...
    
    let _ = app.emit("transcription-progress", "complete");
    Ok(transcript.text)
}
```

- [ ] **Step 2: Add WAV loading helper**

```rust
fn load_wav_to_audio_data(path: &Path) -> Result<AudioData, String> {
    let reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let samples: Vec<f32> = reader.into_samples::<f32>()
        .filter_map(|s| s.ok())
        .collect();
    Ok(AudioData {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}
```

- [ ] **Step 3: Register command, build, commit**

---

### Task 5: Document Generation Commands

**Files:**
- Create: `src-tauri/src/commands/generation.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create generation commands**

Three commands using the processing crate's prompt builders + AI provider:

- `generate_soap` — builds SOAP prompt from transcript, calls AI, stores result
- `generate_referral` — builds referral prompt from SOAP note, calls AI, stores result
- `generate_letter` — builds patient letter prompt, calls AI, stores result

```rust
#[tauri::command]
async fn generate_soap(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    template: Option<String>,
) -> Result<String, String> {
    let _ = app.emit("generation-progress", serde_json::json!({"type": "soap", "status": "started"}));
    
    // Load recording with transcript
    let recording = load_recording(&state, &recording_id)?;
    let transcript = recording.transcript.as_ref().ok_or("No transcript available")?;
    
    // Build prompts
    let soap_config = SoapPromptConfig {
        template: parse_template(&template.unwrap_or_default()),
        ..Default::default()
    };
    let system_prompt = build_soap_prompt(&soap_config);
    let user_prompt = build_user_prompt(transcript, None);
    
    // Call AI provider
    let providers = state.ai_providers.lock().await;
    let provider = providers.active().ok_or("No AI provider configured")?;
    let request = CompletionRequest {
        model: "gpt-4o".into(),
        messages: vec![
            Message { role: Role::User, content: MessageContent::Text(user_prompt), tool_calls: vec![] },
        ],
        temperature: Some(0.3),
        max_tokens: Some(4096),
        system_prompt: Some(system_prompt),
    };
    
    let response = provider.complete(request).await.map_err(|e| e.to_string())?;
    let soap_text = response.content.unwrap_or_default();
    
    // Update recording in DB
    // ...
    
    let _ = app.emit("generation-progress", serde_json::json!({"type": "soap", "status": "complete"}));
    Ok(soap_text)
}
```

Similarly for `generate_referral` and `generate_letter`, using `build_referral_prompt` and `build_letter_prompt`.

- [ ] **Step 2: Register commands, build, commit**

---

### Task 6: Frontend API Wrappers

**Files:**
- Create: `src/lib/api/audio.ts`
- Create: `src/lib/api/chat.ts`
- Create: `src/lib/api/generation.ts`
- Create: `src/lib/api/transcription.ts`

- [ ] **Step 1: Create audio API**

```typescript
// src/lib/api/audio.ts
import { invoke } from '@tauri-apps/api/core';

export async function listAudioDevices(): Promise<string[]> {
  return invoke('list_audio_devices');
}
export async function startRecording(): Promise<void> {
  return invoke('start_recording');
}
export async function stopRecording(): Promise<string> {
  return invoke('stop_recording');
}
export async function pauseRecording(): Promise<void> {
  return invoke('pause_recording');
}
export async function resumeRecording(): Promise<void> {
  return invoke('resume_recording');
}
```

- [ ] **Step 2: Create chat API**

```typescript
// src/lib/api/chat.ts
import { invoke } from '@tauri-apps/api/core';
import type { ChatMessage } from '../types';

export async function chatSend(messages: ChatMessage[], model?: string): Promise<string> {
  return invoke('chat_send', { messages, model });
}
export async function chatStream(messages: ChatMessage[], model?: string): Promise<void> {
  return invoke('chat_stream', { messages, model });
}
export async function chatWithAgent(message: string, agentName: string): Promise<any> {
  return invoke('chat_with_agent', { message, agentName });
}
export async function reinitProviders(): Promise<string[]> {
  return invoke('reinit_providers');
}
```

- [ ] **Step 3: Create generation API**

```typescript
// src/lib/api/generation.ts
import { invoke } from '@tauri-apps/api/core';

export async function generateSoap(recordingId: string, template?: string): Promise<string> {
  return invoke('generate_soap', { recordingId, template });
}
export async function generateReferral(recordingId: string): Promise<string> {
  return invoke('generate_referral', { recordingId });
}
export async function generateLetter(recordingId: string): Promise<string> {
  return invoke('generate_letter', { recordingId });
}
```

- [ ] **Step 4: Create transcription API**

```typescript
// src/lib/api/transcription.ts
import { invoke } from '@tauri-apps/api/core';

export async function transcribeRecording(recordingId: string): Promise<string> {
  return invoke('transcribe_recording', { recordingId });
}
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/api/
git commit -m "feat: add frontend API wrappers for audio, chat, generation, transcription"
```

---

### Task 7: Wire Frontend Stores to Real Backend

**Files:**
- Modify: `src/lib/stores/audio.ts`
- Modify: `src/lib/stores/chat.ts`
- Modify: `src/lib/stores/recordings.ts`
- Modify: `src/lib/stores/settings.ts`

- [ ] **Step 1: Wire audio store to Tauri commands**

Update `audio.startRecording()` to call `startRecording()` API, listen for `waveform-data` events:

```typescript
import { listen } from '@tauri-apps/api/event';
import * as audioApi from '../api/audio';

// In createAudioStore:
let waveformUnlisten: (() => void) | null = null;

async startRecording(device: string | null = null) {
    try {
        await audioApi.startRecording();
        // Listen for waveform events from Rust
        waveformUnlisten = await listen<number[]>('waveform-data', (event) => {
            update((s) => ({
                ...s,
                waveformData: [...s.waveformData, ...event.payload].slice(-256),
            }));
        });
        update((s) => ({ ...s, state: 'recording', elapsed: 0, waveformData: [], deviceName: device }));
        startTimer();
    } catch (e) {
        console.error('Failed to start recording:', e);
    }
},

async stop() {
    clearTimer();
    try {
        const recordingId = await audioApi.stopRecording();
        if (waveformUnlisten) { waveformUnlisten(); waveformUnlisten = null; }
        update((s) => ({ ...s, state: 'stopped', lastRecordingId: recordingId }));
    } catch (e) {
        console.error('Failed to stop recording:', e);
        update((s) => ({ ...s, state: 'stopped' }));
    }
},
```

- [ ] **Step 2: Wire chat store to streaming backend**

Replace mock response with real AI streaming:

```typescript
import { listen } from '@tauri-apps/api/event';
import * as chatApi from '../api/chat';

async sendMessage(content: string) {
    addUserMessage(content);
    startStreaming();
    
    const unlisten = await listen<string>('chat-token', (event) => {
        appendToLast(event.payload);
    });
    const unlistenDone = await listen('chat-done', () => {
        stopStreaming();
        unlisten();
        unlistenDone();
    });
    const unlistenError = await listen<string>('chat-error', (event) => {
        appendToLast(`\n\nError: ${event.payload}`);
        stopStreaming();
        unlisten();
        unlistenDone();
        unlistenError();
    });
    
    try {
        const messages = get(chatMessages); // get current messages for context
        await chatApi.chatStream(messages);
    } catch (e) {
        stopStreaming();
        unlisten();
        unlistenDone();
    }
}
```

- [ ] **Step 3: Commit**

---

### Task 8: Wire Frontend Pages to Real Backend

**Files:**
- Modify: `src/lib/components/RecordingHeader.svelte`
- Modify: `src/lib/pages/RecordTab.svelte`
- Modify: `src/lib/pages/ChatTab.svelte`
- Modify: `src/lib/pages/GenerateTab.svelte`
- Modify: `src/lib/pages/RecordingsTab.svelte`

- [ ] **Step 1: Wire RecordingHeader to real audio**

Update button handlers to call async store methods:

```svelte
<button class="btn btn-record" on:click={() => audio.startRecording()}>
```

This already points to the store — once the store calls the API (Task 7), buttons work automatically.

- [ ] **Step 2: Wire RecordTab to show transcription option**

After recording stops, show a "Transcribe" button that calls the transcription API:

```svelte
{:else if $audio.state === 'stopped'}
  <div class="state-message">
    <div class="state-icon">✓</div>
    <h2>Recording Complete</h2>
    <button class="btn-transcribe" on:click={handleTranscribe}>
      Transcribe Recording
    </button>
  </div>
{/if}
```

- [ ] **Step 3: Wire ChatTab to use real streaming**

Replace the mock response with the store's `sendMessage()` which now calls the real backend. Remove the mock code entirely.

- [ ] **Step 4: Wire GenerateTab to call generation API**

```svelte
async function handleGenerate(type: 'soap' | 'referral' | 'letter') {
    if (!$selectedRecording) return;
    generating = type;
    try {
        if (type === 'soap') {
            const result = await generateSoap($selectedRecording.id);
            // Refresh recording data
        } else if (type === 'referral') {
            const result = await generateReferral($selectedRecording.id);
        } else {
            const result = await generateLetter($selectedRecording.id);
        }
        await recordings.load(); // refresh list
    } catch (e) {
        error = e.toString();
    } finally {
        generating = null;
    }
}
```

- [ ] **Step 5: Add loading/progress states to UI**

Add spinners, progress indicators, and error messages to all pages.

- [ ] **Step 6: Build frontend and test**

Run: `npm run build`
Expected: Clean build, no TypeScript errors.

- [ ] **Step 7: Commit**

```bash
git add src/
git commit -m "feat: wire all frontend pages to real Tauri backend"
```

---

### Task 9: Update Recording Model for Audio Path

**Files:**
- Modify: `crates/core/src/types/recording.rs`
- Modify: `crates/db/src/recordings.rs`
- Modify: `crates/db/src/migrations/m001_initial.rs` (add migration for audio_path if needed)

- [ ] **Step 1: Add audio_path field to Recording**

Ensure the Recording struct has an `audio_path: Option<String>` field to track where the WAV file is stored, and that the DB schema supports it.

- [ ] **Step 2: Update RecordingsRepo to handle audio_path**

Ensure insert/update/get properly handle the audio_path column.

- [ ] **Step 3: Commit**

---

### Task 10: Integration Testing & Polish

**Files:**
- Modify: Various files for bug fixes discovered during integration

- [ ] **Step 1: Run full cargo test**

Run: `cargo test --workspace`
Expected: All existing tests pass plus any new ones.

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy --workspace`
Fix any new warnings.

- [ ] **Step 3: Run Vite build**

Run: `npm run build`
Expected: Clean build.

- [ ] **Step 4: Launch app and smoke test**

Run: `cargo tauri dev`
Test each flow:
1. Record audio (verify waveform shows)
2. Stop recording (verify recording appears in list)
3. Open ChatTab, send message (verify streaming works if API key set)
4. Select recording, generate SOAP (verify generation works if API key set)
5. Check settings (verify API key management)

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: Plan 5 complete — all features wired end-to-end"
```
