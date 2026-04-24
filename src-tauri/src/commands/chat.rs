use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use tauri::Emitter;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use medical_core::error::{AppError, AppResult};
use medical_core::traits::Agent;
use medical_core::types::{
    AgentContext, CompletionRequest, Message, MessageContent, Role, StreamChunk, UsageInfo,
};

use medical_agents::agents::{
    ChatAgent, ComplianceAgent, DataExtractionAgent, DiagnosticAgent, MedicationAgent,
    ReferralAgent, SynopsisAgent, WorkflowAgent,
};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Input / output types
// ---------------------------------------------------------------------------

/// Lightweight message type received from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessageInput {
    pub role: String,
    pub content: String,
}

/// Payload emitted for streaming token events.
#[derive(Debug, Clone, Serialize)]
struct TokenPayload {
    content: String,
}

/// Payload emitted when streaming completes.
#[derive(Debug, Clone, Serialize)]
struct DonePayload {
    usage: Option<UsageInfo>,
    finish_reason: Option<String>,
}

/// Payload emitted on streaming errors.
#[derive(Debug, Clone, Serialize)]
struct ErrorPayload {
    message: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load the AI model and temperature from saved settings.
/// Falls back to sensible defaults if settings can't be read.
fn load_chat_settings(state: &tauri::State<'_, AppState>) -> (String, f32) {
    let conn = state.db.conn().ok();
    let config = conn
        .and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
        .map(|mut c| { c.migrate(); c });
    match config {
        Some(cfg) => (cfg.ai_model, cfg.temperature),
        None => ("gpt-4o".to_string(), 0.7),
    }
}

/// Convert a frontend role string to the core `Role` enum.
fn parse_role(s: &str) -> Role {
    match s.to_lowercase().as_str() {
        "system" => Role::System,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}

/// Convert a `Vec<ChatMessageInput>` to `Vec<Message>`.
fn convert_messages(inputs: Vec<ChatMessageInput>) -> Vec<Message> {
    inputs
        .into_iter()
        .map(|m| Message {
            role: parse_role(&m.role),
            content: MessageContent::Text(m.content),
            tool_calls: vec![],
        })
        .collect()
}

/// Look up an agent by name and return a boxed trait object.
fn get_agent_by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "chat" => Some(Box::new(ChatAgent)),
        "medication" => Some(Box::new(MedicationAgent)),
        "diagnostic" => Some(Box::new(DiagnosticAgent)),
        "compliance" => Some(Box::new(ComplianceAgent)),
        "data_extraction" => Some(Box::new(DataExtractionAgent)),
        "workflow" => Some(Box::new(WorkflowAgent)),
        "referral" => Some(Box::new(ReferralAgent)),
        "synopsis" => Some(Box::new(SynopsisAgent)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Non-streaming chat completion.
///
/// Sends the provided messages to the active AI provider and returns the full
/// response content as a string.
#[tauri::command]
pub async fn chat_send(
    state: tauri::State<'_, AppState>,
    messages: Vec<ChatMessageInput>,
    model: Option<String>,
    system_prompt: Option<String>,
) -> AppResult<String> {
    // Load model/temperature from settings when not explicitly provided
    let (settings_model, settings_temp) = load_chat_settings(&state);

    let provider = {
        let registry = state.ai_providers.lock().await;
        registry.get_active_arc()
    }
    .ok_or_else(|| AppError::AiProvider("No active AI provider configured".to_string()))?;

    let core_messages = convert_messages(messages);

    let request = CompletionRequest {
        model: model.unwrap_or(settings_model),
        messages: core_messages,
        temperature: Some(settings_temp),
        max_tokens: Some(4096),
        system_prompt,
    };

    debug!("chat_send: calling provider '{}'", provider.name());

    let response = provider
        .complete(request)
        .await
        .map_err(|e| AppError::AiProvider(format!("AI completion failed: {e}")))?;

    Ok(response.content)
}

/// Streaming chat completion via Tauri events.
///
/// Emits the following events on the given `AppHandle`:
/// - `chat-token`  — for each text delta (`TokenPayload`)
/// - `chat-done`   — when the stream finishes (`DonePayload`)
/// - `chat-error`  — on error (`ErrorPayload`)
#[tauri::command]
pub async fn chat_stream(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    messages: Vec<ChatMessageInput>,
    model: Option<String>,
    system_prompt: Option<String>,
) -> AppResult<()> {
    // Load model/temperature from settings when not explicitly provided
    let (settings_model, settings_temp) = load_chat_settings(&state);

    let provider = {
        let registry = state.ai_providers.lock().await;
        registry.get_active_arc()
    }
    .ok_or_else(|| AppError::AiProvider("No active AI provider configured".to_string()))?;

    let core_messages = convert_messages(messages);

    let request = CompletionRequest {
        model: model.unwrap_or(settings_model),
        messages: core_messages,
        temperature: Some(settings_temp),
        max_tokens: Some(4096),
        system_prompt,
    };

    debug!("chat_stream: calling provider '{}'", provider.name());

    let mut stream = provider
        .complete_stream(request)
        .await
        .map_err(|e| AppError::AiProvider(format!("Failed to start streaming: {e}")))?;

    // Consume the stream in a background task so the command returns immediately.
    tokio::spawn(async move {
        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => match chunk {
                    StreamChunk::Delta { text } => {
                        let _ = app.emit("chat-token", TokenPayload { content: text });
                    }
                    StreamChunk::ToolCallDelta { .. } => {
                        // Tool-call deltas are not surfaced in the basic chat stream.
                    }
                    StreamChunk::Usage(usage) => {
                        let _ = app.emit(
                            "chat-done",
                            DonePayload {
                                usage: Some(usage),
                                finish_reason: Some("stop".to_string()),
                            },
                        );
                    }
                    StreamChunk::Done => {
                        let _ = app.emit(
                            "chat-done",
                            DonePayload {
                                usage: None,
                                finish_reason: Some("stop".to_string()),
                            },
                        );
                    }
                },
                Err(e) => {
                    error!("chat_stream error: {e}");
                    let _ = app.emit(
                        "chat-error",
                        ErrorPayload {
                            message: e.to_string(),
                        },
                    );
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Execute a named agent against the active AI provider.
///
/// Available agent names: `chat`, `medication`, `diagnostic`, `compliance`,
/// `data_extraction`, `workflow`, `referral`, `synopsis`.
///
/// Returns the full `AgentResponse` as a JSON value.
#[tauri::command]
pub async fn chat_with_agent(
    state: tauri::State<'_, AppState>,
    message: String,
    agent_name: String,
    conversation_history: Option<Vec<ChatMessageInput>>,
) -> AppResult<serde_json::Value> {
    let agent = get_agent_by_name(&agent_name)
        .ok_or_else(|| AppError::Agent(format!("Unknown agent: '{agent_name}'")))?;

    let provider = {
        let registry = state.ai_providers.lock().await;
        registry.get_active_arc()
    }
    .ok_or_else(|| AppError::AiProvider("No active AI provider configured".to_string()))?;

    let history = conversation_history
        .map(convert_messages)
        .unwrap_or_default();

    let context = AgentContext {
        user_message: message,
        conversation_history: history,
        patient_context: None,
        rag_context: vec![],
        recording: None,
    };

    let cancel = CancellationToken::new();

    let (model, _temperature) = load_chat_settings(&state);

    debug!(
        "chat_with_agent: running agent '{}' with model '{}'",
        agent_name, model
    );

    let response = state
        .orchestrator
        .execute(agent.as_ref(), context, provider.as_ref(), &model, cancel)
        .await
        .map_err(|e| AppError::Agent(format!("Agent execution failed: {e}")))?;

    Ok(serde_json::to_value(&response)?)
}

/// List all registered AI provider names.
#[tauri::command]
pub async fn list_ai_providers(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<String>> {
    let registry = state.ai_providers.lock().await;
    Ok(registry.list_available())
}

/// Set the active AI provider by name. Returns `true` if the provider exists
/// and was activated, `false` otherwise.
#[tauri::command]
pub async fn set_active_provider(
    state: tauri::State<'_, AppState>,
    name: String,
) -> AppResult<bool> {
    let mut registry = state.ai_providers.lock().await;
    Ok(registry.set_active(&name))
}

/// Fetch available models for a given provider (or active provider if name is None).
#[tauri::command]
pub async fn list_models(
    state: tauri::State<'_, AppState>,
    provider_name: Option<String>,
) -> AppResult<Vec<medical_core::types::ModelInfo>> {
    let provider = {
        let registry = state.ai_providers.lock().await;
        match provider_name {
            Some(name) => registry.get_arc(&name),
            None => registry.get_active_arc(),
        }
    };
    let provider = provider
        .ok_or_else(|| AppError::AiProvider("Provider not found or not configured".to_string()))?;
    provider
        .available_models()
        .await
        .map_err(|e| AppError::AiProvider(e.to_string()))
}
