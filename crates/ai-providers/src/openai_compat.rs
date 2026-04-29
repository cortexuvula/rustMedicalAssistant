//! Base client for any OpenAI-compatible chat-completions endpoint.

#![allow(dead_code)]

use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use medical_core::{
    error::{AppError, AppResult},
    types::{
        CompletionRequest, CompletionResponse, Message, MessageContent, Role, StreamChunk,
        ToolCall, ToolCompletionResponse, ToolDef, UsageInfo,
    },
};

use crate::http_client::RetryConfig;
use crate::sse::parse_sse_response;

// ──────────────────────────────────────────────────────────────────────────────
// Internal serde types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ApiToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: ApiFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ApiToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: Option<ApiFunctionDelta>,
}

#[derive(Debug, Deserialize, Clone)]
struct ApiFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiTool {
    #[serde(rename = "type")]
    kind: String,
    function: ApiToolDef,
}

#[derive(Debug, Serialize)]
struct ApiToolDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    model: Option<String>,
    choices: Vec<ChatChoice>,
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: Option<ChatResponseMessage>,
    delta: Option<ChatDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ChatDelta {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ──────────────────────────────────────────────────────────────────────────────
// OpenAiCompatibleClient
// ──────────────────────────────────────────────────────────────────────────────

// ──────────────────────────────────────────────────────────────────────────────
// Models-listing serde types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ModelsListResponse {
    data: Vec<ApiModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ApiModelEntry {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// OpenAiCompatibleClient
// ──────────────────────────────────────────────────────────────────────────────

/// A client for any endpoint implementing the OpenAI chat-completions protocol.
pub struct OpenAiCompatibleClient {
    pub client: Client,
    pub base_url: String,
    pub policy: RetryConfig,
}

impl OpenAiCompatibleClient {
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        policy: RetryConfig,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            policy,
        }
    }

    /// Convert a core `Message` into the OpenAI wire format (`ChatMessage`).
    /// For assistant messages that carry `tool_calls`, we include them so that
    /// subsequent `tool` role messages can reference the tool_call_id.
    fn convert_message(msg: &Message) -> ChatMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        match &msg.content {
            MessageContent::Text(text) => {
                // For assistant messages with tool_calls, the content may be null/empty
                // and the tool_calls must be forwarded.
                let api_tool_calls: Option<Vec<ApiToolCall>> = if msg.tool_calls.is_empty() {
                    None
                } else {
                    Some(
                        msg.tool_calls
                            .iter()
                            .map(|tc| ApiToolCall {
                                id: tc.id.clone(),
                                kind: "function".into(),
                                function: ApiFunction {
                                    name: tc.name.clone(),
                                    arguments: tc.arguments.to_string(),
                                },
                            })
                            .collect(),
                    )
                };
                // Content is null (not present) when tool_calls are the primary payload,
                // but some providers tolerate an empty string; send None when tool_calls
                // are present and text is empty to stay spec-compliant.
                let content = if text.is_empty() && api_tool_calls.is_some() {
                    None
                } else {
                    Some(serde_json::Value::String(text.clone()))
                };
                ChatMessage {
                    role: role.into(),
                    content,
                    tool_call_id: None,
                    tool_calls: api_tool_calls,
                }
            }
            MessageContent::ToolResult {
                tool_call_id,
                content,
            } => ChatMessage {
                role: "tool".into(),
                content: Some(serde_json::Value::String(content.clone())),
                tool_call_id: Some(tool_call_id.clone()),
                tool_calls: None,
            },
        }
    }

    fn build_request(&self, request: &CompletionRequest) -> ChatRequest {
        let mut messages: Vec<ChatMessage> = Vec::new();

        // Inject system prompt as first message if present
        if let Some(sys) = &request.system_prompt {
            messages.push(ChatMessage {
                role: "system".into(),
                content: Some(serde_json::Value::String(sys.clone())),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        for msg in &request.messages {
            messages.push(Self::convert_message(msg));
        }

        ChatRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: None,
            tools: None,
            stream_options: None,
        }
    }

    fn parse_response(
        &self,
        resp: ChatResponse,
        default_model: &str,
    ) -> CompletionResponse {
        let model = resp.model.unwrap_or_else(|| default_model.to_string());
        let usage = resp.usage.map(|u| UsageInfo {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }).unwrap_or_default();

        let num_choices = resp.choices.len();
        let first_choice = resp.choices.into_iter().next();

        if first_choice.is_none() {
            warn!(
                model = %model,
                "AI response contained no choices (choices array was empty)"
            );
        }

        let finish_reason = first_choice
            .as_ref()
            .and_then(|c| c.finish_reason.clone());

        let content = first_choice
            .as_ref()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        if content.is_empty() && num_choices > 0 {
            warn!(
                model = %model,
                finish_reason = ?finish_reason,
                has_message = first_choice.as_ref().and_then(|c| c.message.as_ref()).is_some(),
                "AI response content is empty (choices={num_choices}, finish_reason={finish_reason:?})"
            );
        }

        let tool_calls = first_choice
            .as_ref()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.tool_calls.as_ref())
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Null),
                    })
                    .collect()
            })
            .unwrap_or_default();

        CompletionResponse {
            content,
            model,
            usage,
            tool_calls,
        }
    }

    /// Fetch the list of model IDs from the `/models` endpoint.
    pub async fn list_models(&self) -> AppResult<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let resp: ModelsListResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let mut ids: Vec<String> = resp.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }

    pub async fn complete(&self, request: &CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request(request);

        let response = crate::http_client::send_with_retry(&self.policy, || {
            self.client.post(&url).json(&body)
        })
        .await
        .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        let raw_body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(AppError::AiProvider(format!("HTTP {status}: {raw_body}")));
        }

        let resp: ChatResponse = serde_json::from_str(&raw_body)
            .map_err(|e| {
                warn!(body_preview = &raw_body[..raw_body.len().min(500)], "Failed to parse AI response JSON");
                AppError::AiProvider(format!("JSON parse error: {e}"))
            })?;

        debug!(
            url = %url,
            model = %request.model,
            choices = resp.choices.len(),
            "AI completion response received"
        );

        let finish_reason = resp.choices.first()
            .and_then(|c| c.finish_reason.as_deref())
            .unwrap_or("unknown");
        let has_content = resp.choices.first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.as_ref())
            .map(|c| !c.is_empty())
            .unwrap_or(false);

        if !has_content && finish_reason == "length" {
            return Err(AppError::AiProvider(format!(
                "Model '{}' context window exceeded: the prompt is too long for the model, \
                 leaving no room for output. Try a model with a larger context window, \
                 reduce the prompt size, or increase the model's context length in LM Studio.",
                request.model,
            )));
        }

        Ok(self.parse_response(resp, &request.model))
    }

    pub async fn complete_stream(
        &self,
        request: &CompletionRequest,
    ) -> AppResult<Pin<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = self.build_request(request);
        body.stream = Some(true);
        body.stream_options = Some(StreamOptions { include_usage: true });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let sse = parse_sse_response(response);

        // Convert each SSE data line into zero or more StreamChunks.
        let mapped = sse
            .map(|item| -> Vec<AppResult<StreamChunk>> {
                match item {
                    Err(e) => vec![Err(AppError::AiProvider(e))],
                    Ok(data) => {
                        match serde_json::from_str::<ChatResponse>(&data) {
                            Err(_) => vec![],
                            Ok(resp) => {
                                let mut out = Vec::new();
                                if let Some(choice) = resp.choices.first()
                                    && let Some(delta) = &choice.delta {
                                        // Text delta
                                        if let Some(text) = &delta.content
                                            && !text.is_empty() {
                                                out.push(Ok(StreamChunk::Delta {
                                                    text: text.clone(),
                                                }));
                                            }
                                        // Tool-call deltas
                                        if let Some(tc_deltas) = &delta.tool_calls {
                                            for tc in tc_deltas {
                                                let id = tc.id.clone().unwrap_or_default();
                                                let name = tc
                                                    .function
                                                    .as_ref()
                                                    .and_then(|f| f.name.clone());
                                                let args_delta = tc
                                                    .function
                                                    .as_ref()
                                                    .and_then(|f| f.arguments.clone())
                                                    .unwrap_or_default();
                                                out.push(Ok(StreamChunk::ToolCallDelta {
                                                    id,
                                                    name,
                                                    arguments_delta: args_delta,
                                                }));
                                            }
                                        }
                                    }
                                // Usage chunk (comes in a separate SSE event with usage data)
                                if let Some(u) = resp.usage {
                                    out.push(Ok(StreamChunk::Usage(UsageInfo {
                                        prompt_tokens: u.prompt_tokens,
                                        completion_tokens: u.completion_tokens,
                                        total_tokens: u.total_tokens,
                                    })));
                                    out.push(Ok(StreamChunk::Done));
                                }
                                out
                            }
                        }
                    }
                }
            })
            .flat_map(tokio_stream::iter);

        Ok(Box::pin(mapped))
    }

    pub async fn complete_with_tools(
        &self,
        request: &CompletionRequest,
        tools: Vec<ToolDef>,
    ) -> AppResult<ToolCompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = self.build_request(request);
        body.tools = Some(
            tools
                .into_iter()
                .map(|t| ApiTool {
                    kind: "function".into(),
                    function: ApiToolDef {
                        name: t.name,
                        description: t.description,
                        parameters: t.parameters,
                    },
                })
                .collect(),
        );

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!("HTTP {status}: {text}")));
        }

        let resp: ChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let usage = resp.usage.map(|u| UsageInfo {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }).unwrap_or_default();

        let first_choice = resp.choices.into_iter().next();

        let content = first_choice
            .as_ref()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone());

        let tool_calls = first_choice
            .as_ref()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.tool_calls.as_ref())
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Null),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ToolCompletionResponse {
            content,
            tool_calls,
            usage,
        })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::{Message, MessageContent, Role};

    fn make_client() -> OpenAiCompatibleClient {
        // Build without real auth — only used for struct-level tests.
        OpenAiCompatibleClient::new(Client::new(), "https://api.openai.com/v1", RetryConfig::default())
    }

    fn make_request() -> CompletionRequest {
        CompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("Hello".into()),
                tool_calls: vec![],
            }],
            temperature: None,
            max_tokens: None,
            system_prompt: Some("You are a helpful assistant.".into()),
        }
    }

    #[test]
    fn build_request_includes_system_prompt() {
        let c = make_client();
        let req = make_request();
        let chat_req = c.build_request(&req);
        assert_eq!(chat_req.messages[0].role, "system");
        assert_eq!(
            chat_req.messages[0].content,
            Some(serde_json::Value::String(
                "You are a helpful assistant.".into()
            ))
        );
        assert_eq!(chat_req.messages[1].role, "user");
    }

    #[test]
    fn stream_flag() {
        let c = make_client();
        let req = make_request();
        let mut chat_req = c.build_request(&req);
        assert!(chat_req.stream.is_none());
        chat_req.stream = Some(true);
        assert_eq!(chat_req.stream, Some(true));
    }

    #[test]
    fn parse_response_extracts_content() {
        let c = make_client();
        let resp = ChatResponse {
            model: Some("gpt-4o".into()),
            choices: vec![ChatChoice {
                message: Some(ChatResponseMessage {
                    content: Some("The answer is 42.".into()),
                    tool_calls: None,
                }),
                delta: None,
                finish_reason: Some("stop".into()),
            }],
            usage: Some(ApiUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };
        let completion = c.parse_response(resp, "gpt-4o");
        assert_eq!(completion.content, "The answer is 42.");
        assert_eq!(completion.model, "gpt-4o");
        assert_eq!(completion.usage.total_tokens, 15);
        assert!(completion.tool_calls.is_empty());
    }

    use std::time::Duration;

    fn fast_policy(max_retries: u32) -> RetryConfig {
        RetryConfig {
            max_retries,
            initial_delay: Duration::from_millis(20),
            backoff_factor: 2.0,
            max_delay: Duration::from_millis(200),
        }
    }

    fn build_test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("test client")
    }

    fn make_retry_request() -> CompletionRequest {
        CompletionRequest {
            model: "test-model".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("hello".into()),
                tool_calls: vec![],
            }],
            temperature: Some(0.0),
            max_tokens: None,
            system_prompt: None,
        }
    }

    #[tokio::test]
    async fn complete_recovers_from_503() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "test-model",
                "choices": [{
                    "message": {"content": "hi back"},
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;

        let client = OpenAiCompatibleClient::new(
            build_test_client(),
            format!("{}/v1", server.uri()),
            fast_policy(3),
        );

        let resp = client
            .complete(&make_retry_request())
            .await
            .expect("complete should recover");
        assert_eq!(resp.content, "hi back");
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn complete_does_not_retry_400() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let client = OpenAiCompatibleClient::new(
            build_test_client(),
            format!("{}/v1", server.uri()),
            fast_policy(3),
        );

        let err = client
            .complete(&make_retry_request())
            .await
            .expect_err("400 should be permanent");
        let msg = format!("{err}");
        assert!(msg.contains("400"), "expected 400 in error: {msg}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
