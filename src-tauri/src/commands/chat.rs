use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use tauri::Emitter;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

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
) -> Result<String, String> {
    let provider = {
        let registry = state.ai_providers.lock().await;
        registry.get_active_arc()
    }
    .ok_or_else(|| "No active AI provider configured".to_string())?;

    let core_messages = convert_messages(messages);

    let request = CompletionRequest {
        model: model.unwrap_or_else(|| "gpt-4o".to_string()),
        messages: core_messages,
        temperature: Some(0.7),
        max_tokens: Some(4096),
        system_prompt,
    };

    debug!("chat_send: calling provider '{}'", provider.name());

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

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
) -> Result<(), String> {
    let provider = {
        let registry = state.ai_providers.lock().await;
        registry.get_active_arc()
    }
    .ok_or_else(|| "No active AI provider configured".to_string())?;

    let core_messages = convert_messages(messages);

    let request = CompletionRequest {
        model: model.unwrap_or_else(|| "gpt-4o".to_string()),
        messages: core_messages,
        temperature: Some(0.7),
        max_tokens: Some(4096),
        system_prompt,
    };

    debug!("chat_stream: calling provider '{}'", provider.name());

    let mut stream = provider
        .complete_stream(request)
        .await
        .map_err(|e| format!("Failed to start streaming: {e}"))?;

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
) -> Result<serde_json::Value, String> {
    let agent = get_agent_by_name(&agent_name)
        .ok_or_else(|| format!("Unknown agent: '{agent_name}'"))?;

    let provider = {
        let registry = state.ai_providers.lock().await;
        registry.get_active_arc()
    }
    .ok_or_else(|| "No active AI provider configured".to_string())?;

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

    debug!("chat_with_agent: running agent '{}'", agent_name);

    let response = state
        .orchestrator
        .execute(agent.as_ref(), context, provider.as_ref(), cancel)
        .await
        .map_err(|e| format!("Agent execution failed: {e}"))?;

    serde_json::to_value(&response).map_err(|e| format!("Serialization failed: {e}"))
}

/// List all registered AI provider names.
#[tauri::command]
pub async fn list_ai_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let registry = state.ai_providers.lock().await;
    Ok(registry.list_available())
}

/// Set the active AI provider by name. Returns `true` if the provider exists
/// and was activated, `false` otherwise.
#[tauri::command]
pub async fn set_active_provider(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<bool, String> {
    let mut registry = state.ai_providers.lock().await;
    Ok(registry.set_active(&name))
}

/// Fetch available models for a given provider (or active provider if name is None).
#[tauri::command]
pub async fn list_models(
    state: tauri::State<'_, AppState>,
    provider_name: Option<String>,
) -> Result<Vec<medical_core::types::ModelInfo>, String> {
    let provider = {
        let registry = state.ai_providers.lock().await;
        match provider_name {
            Some(name) => registry.get_arc(&name),
            None => registry.get_active_arc(),
        }
    };
    let provider = provider.ok_or("Provider not found or not configured")?;
    provider.available_models().await.map_err(|e| e.to_string())
}
