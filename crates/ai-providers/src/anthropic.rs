//! Anthropic Claude provider — uses the Messages API (not OpenAI-compatible).

#![allow(dead_code)]

use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use medical_core::{
    error::{AppError, AppResult},
    traits::AiProvider,
    types::{
        CompletionRequest, CompletionResponse, MessageContent, ModelInfo, Role, StreamChunk,
        ToolCall, ToolCompletionResponse, ToolDef, UsageInfo,
    },
};

use crate::sse::parse_sse_response;

const BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

// ──────────────────────────────────────────────────────────────────────────────
// Internal serde types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    model: Option<String>,
    content: Vec<ContentBlock>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// Streaming event types
#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    kind: String,
    delta: Option<StreamDelta>,
    content_block: Option<ContentBlock>,
    message: Option<MessageEvent>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct MessageEvent {
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    partial_json: Option<String>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Provider
// ──────────────────────────────────────────────────────────────────────────────

pub struct AnthropicProvider {
    client: Client,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: &str) -> Self {
        // Build with both x-api-key and anthropic-version default headers.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "x-api-key",
            reqwest::header::HeaderValue::from_str(api_key).unwrap_or_else(|_| {
                reqwest::header::HeaderValue::from_static("invalid")
            }),
        );
        headers.insert(
            "anthropic-version",
            reqwest::header::HeaderValue::from_static(ANTHROPIC_VERSION),
        );

        let client = Client::builder()
            .default_headers(headers)
            .pool_max_idle_per_host(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build Anthropic HTTP client");

        Self {
            client,
            base_url: BASE_URL.to_string(),
        }
    }

    fn build_request(&self, request: &CompletionRequest, stream: bool) -> AnthropicRequest {
        let system = request.system_prompt.clone();

        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|msg| {
                let role = match msg.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    Role::System => "user", // filtered above, but guard
                };
                let content = match &msg.content {
                    MessageContent::Text(text) => serde_json::Value::String(text.clone()),
                    MessageContent::ToolResult {
                        tool_call_id,
                        content,
                    } => serde_json::json!([{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content,
                    }]),
                };
                AnthropicMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect();

        AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            temperature: request.temperature,
            stream: if stream { Some(true) } else { None },
            tools: None,
        }
    }

    fn extract_response(
        &self,
        resp: AnthropicResponse,
        default_model: &str,
    ) -> CompletionResponse {
        let model = resp.model.unwrap_or_else(|| default_model.to_string());
        let usage = resp
            .usage
            .map(|u| UsageInfo {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            })
            .unwrap_or_default();

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in &resp.content {
            match block.kind.as_str() {
                "text" => {
                    if let Some(t) = &block.text {
                        text_parts.push(t.clone());
                    }
                }
                "tool_use" => {
                    tool_calls.push(ToolCall {
                        id: block.id.clone().unwrap_or_default(),
                        name: block.name.clone().unwrap_or_default(),
                        arguments: block.input.clone().unwrap_or(serde_json::Value::Null),
                    });
                }
                _ => {}
            }
        }

        CompletionResponse {
            content: text_parts.join(""),
            model,
            usage,
            tool_calls,
        }
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "claude-opus-4-20250514".into(),
                name: "Claude Opus 4".into(),
                provider: "anthropic".into(),
                max_tokens: 200_000,
                supports_tools: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-sonnet-4-20250514".into(),
                name: "Claude Sonnet 4".into(),
                provider: "anthropic".into(),
                max_tokens: 200_000,
                supports_tools: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-haiku-4-20250514".into(),
                name: "Claude Haiku 4".into(),
                provider: "anthropic".into(),
                max_tokens: 200_000,
                supports_tools: true,
                supports_streaming: true,
            },
        ])
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/messages", self.base_url);
        let body = self.build_request(&request, false);

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

        let resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        Ok(self.extract_response(resp, &request.model))
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send + Unpin>> {
        let url = format!("{}/messages", self.base_url);
        let body = self.build_request(&request, true);

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

        let mapped = sse
            .map(|item| -> Vec<AppResult<StreamChunk>> {
                match item {
                    Err(e) => vec![Err(AppError::AiProvider(e))],
                    Ok(data) => {
                        match serde_json::from_str::<StreamEvent>(&data) {
                            Err(_) => vec![],
                            Ok(event) => {
                                let mut out = Vec::new();
                                match event.kind.as_str() {
                                    "content_block_start" => {
                                        if let Some(cb) = &event.content_block {
                                            if cb.kind == "tool_use" {
                                                out.push(Ok(StreamChunk::ToolCallDelta {
                                                    id: cb.id.clone().unwrap_or_default(),
                                                    name: cb.name.clone(),
                                                    arguments_delta: String::new(),
                                                }));
                                            }
                                        }
                                    }
                                    "content_block_delta" => {
                                        if let Some(delta) = &event.delta {
                                            match delta.kind.as_deref() {
                                                Some("text_delta") => {
                                                    if let Some(text) = &delta.text
                                                        && !text.is_empty() {
                                                            out.push(Ok(StreamChunk::Delta {
                                                                text: text.clone(),
                                                            }));
                                                        }
                                                }
                                                Some("input_json_delta") => {
                                                    if let Some(args) = &delta.partial_json {
                                                        out.push(Ok(StreamChunk::ToolCallDelta {
                                                            id: String::new(),
                                                            name: None,
                                                            arguments_delta: args.clone(),
                                                        }));
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    "message_delta" => {
                                        if let Some(delta) = &event.delta
                                            && let Some(usage) = &delta.usage {
                                                out.push(Ok(StreamChunk::Usage(UsageInfo {
                                                    prompt_tokens: usage.input_tokens,
                                                    completion_tokens: usage.output_tokens,
                                                    total_tokens: usage.input_tokens
                                                        + usage.output_tokens,
                                                })));
                                            }
                                        // Also check top-level usage
                                        if let Some(usage) = &event.usage {
                                            out.push(Ok(StreamChunk::Usage(UsageInfo {
                                                prompt_tokens: usage.input_tokens,
                                                completion_tokens: usage.output_tokens,
                                                total_tokens: usage.input_tokens
                                                    + usage.output_tokens,
                                            })));
                                        }
                                    }
                                    "message_stop" => {
                                        out.push(Ok(StreamChunk::Done));
                                    }
                                    _ => {}
                                }
                                out
                            }
                        }
                    }
                }
            })
            .flat_map(tokio_stream::iter);

        Ok(Box::new(Box::pin(mapped)))
    }

    async fn complete_with_tools(
        &self,
        request: CompletionRequest,
        tools: Vec<ToolDef>,
    ) -> AppResult<ToolCompletionResponse> {
        let url = format!("{}/messages", self.base_url);
        let mut body = self.build_request(&request, false);
        body.tools = Some(
            tools
                .into_iter()
                .map(|t| AnthropicTool {
                    name: t.name,
                    description: t.description,
                    input_schema: t.parameters,
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

        let resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let usage = resp
            .usage
            .as_ref()
            .map(|u| UsageInfo {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            })
            .unwrap_or_default();

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in &resp.content {
            match block.kind.as_str() {
                "text" => {
                    if let Some(t) = &block.text {
                        text_parts.push(t.clone());
                    }
                }
                "tool_use" => {
                    tool_calls.push(ToolCall {
                        id: block.id.clone().unwrap_or_default(),
                        name: block.name.clone().unwrap_or_default(),
                        arguments: block.input.clone().unwrap_or(serde_json::Value::Null),
                    });
                }
                _ => {}
            }
        }

        let content = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join(""))
        };

        Ok(ToolCompletionResponse {
            content,
            tool_calls,
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn model_list_contains_claude() {
        let provider = AnthropicProvider::new("dummy-key");
        let models = provider.available_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("claude")));
        assert!(models.iter().all(|m| m.supports_tools));
        assert!(models.iter().all(|m| m.supports_streaming));
    }
}
