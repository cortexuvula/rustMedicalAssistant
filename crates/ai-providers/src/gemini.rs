//! Google Gemini provider — uses the generateContent API.

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
        ToolCompletionResponse, ToolDef, UsageInfo,
    },
};

use crate::sse::parse_sse_response;

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

// ──────────────────────────────────────────────────────────────────────────────
// Internal serde types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "generationConfig")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Provider
// ──────────────────────────────────────────────────────────────────────────────

pub struct GeminiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl GeminiProvider {
    pub fn new(api_key: &str) -> Self {
        // Gemini uses the key as a URL param, not a header.
        let client = Client::builder()
            .pool_max_idle_per_host(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build Gemini HTTP client");

        Self {
            client,
            api_key: api_key.to_string(),
            base_url: BASE_URL.to_string(),
        }
    }

    /// Build the endpoint URL for a given model and action.
    pub fn build_url(&self, model: &str, stream: bool) -> String {
        if stream {
            format!(
                "{}/models/{}:streamGenerateContent?alt=sse&key={}",
                self.base_url, model, self.api_key
            )
        } else {
            format!(
                "{}/models/{}:generateContent?key={}",
                self.base_url, model, self.api_key
            )
        }
    }

    async fn fetch_models_from_api(&self) -> AppResult<Vec<ModelInfo>> {
        let url = format!("{}/models?key={}", self.base_url, self.api_key);
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

        #[derive(Deserialize)]
        struct GeminiModelEntry {
            name: String,
            #[serde(rename = "displayName")]
            display_name: Option<String>,
            #[serde(rename = "supportedGenerationMethods", default)]
            supported_generation_methods: Vec<String>,
            #[serde(rename = "inputTokenLimit", default)]
            input_token_limit: u32,
        }

        #[derive(Deserialize)]
        struct GeminiModelsResponse {
            models: Vec<GeminiModelEntry>,
        }

        let resp: GeminiModelsResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let mut models: Vec<ModelInfo> = resp
            .models
            .into_iter()
            .filter(|m| m.supported_generation_methods.iter().any(|method| method == "generateContent"))
            .filter(|m| {
                // Only include gemini models, not older PaLM/embedding models
                let id = m.name.strip_prefix("models/").unwrap_or(&m.name);
                id.starts_with("gemini")
            })
            .map(|m| {
                let id = m.name.strip_prefix("models/").unwrap_or(&m.name).to_string();
                let name = m.display_name.unwrap_or_else(|| id.clone());
                ModelInfo {
                    id,
                    name,
                    provider: "gemini".into(),
                    max_tokens: m.input_token_limit,
                    supports_tools: true,
                    supports_streaming: true,
                }
            })
            .collect();

        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    fn build_request(&self, request: &CompletionRequest) -> GeminiRequest {
        let system_instruction = request.system_prompt.as_ref().map(|s| {
            GeminiSystemInstruction {
                parts: vec![GeminiPart { text: s.clone() }],
            }
        });

        let contents: Vec<GeminiContent> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|msg| {
                let role = match msg.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "model",
                    Role::System => "user",
                };
                let text = match &msg.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::ToolResult { content, .. } => content.clone(),
                };
                GeminiContent {
                    role: role.to_string(),
                    parts: vec![GeminiPart { text }],
                }
            })
            .collect();

        let generation_config =
            if request.temperature.is_some() || request.max_tokens.is_some() {
                Some(GenerationConfig {
                    temperature: request.temperature,
                    max_output_tokens: request.max_tokens,
                })
            } else {
                None
            };

        GeminiRequest {
            contents,
            system_instruction,
            generation_config,
        }
    }

    fn extract_text(resp: &GeminiResponse) -> String {
        resp.candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.content.as_ref())
            .map(|content| {
                content
                    .parts
                    .iter()
                    .map(|p| p.text.clone())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    }

    fn extract_usage(resp: &GeminiResponse) -> UsageInfo {
        resp.usage_metadata
            .as_ref()
            .map(|u| UsageInfo {
                prompt_tokens: u.prompt_token_count.unwrap_or(0),
                completion_tokens: u.candidates_token_count.unwrap_or(0),
                total_tokens: u.total_token_count.unwrap_or(0),
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        // Try to fetch models from Gemini API
        if let Ok(models) = self.fetch_models_from_api().await {
            if !models.is_empty() {
                return Ok(models);
            }
        }

        // Fallback
        Ok(vec![
            ModelInfo { id: "gemini-2.0-flash".into(), name: "Gemini 2.0 Flash".into(), provider: "gemini".into(), max_tokens: 1_048_576, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gemini-1.5-pro".into(), name: "Gemini 1.5 Pro".into(), provider: "gemini".into(), max_tokens: 2_097_152, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "gemini-1.5-flash".into(), name: "Gemini 1.5 Flash".into(), provider: "gemini".into(), max_tokens: 1_048_576, supports_tools: true, supports_streaming: true },
        ])
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = self.build_url(&request.model, false);
        let body = self.build_request(&request);

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

        let resp: GeminiResponse = response
            .json()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let content = Self::extract_text(&resp);
        let usage = Self::extract_usage(&resp);

        Ok(CompletionResponse {
            content,
            model: request.model.clone(),
            usage,
            tool_calls: vec![],
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send + Unpin>> {
        let url = self.build_url(&request.model, true);
        let body = self.build_request(&request);

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
                        match serde_json::from_str::<GeminiResponse>(&data) {
                            Err(_) => vec![],
                            Ok(resp) => {
                                let mut out = Vec::new();
                                let text = Self::extract_text(&resp);
                                if !text.is_empty() {
                                    out.push(Ok(StreamChunk::Delta { text }));
                                }
                                if let Some(usage_meta) = &resp.usage_metadata {
                                    out.push(Ok(StreamChunk::Usage(UsageInfo {
                                        prompt_tokens: usage_meta.prompt_token_count.unwrap_or(0),
                                        completion_tokens: usage_meta
                                            .candidates_token_count
                                            .unwrap_or(0),
                                        total_tokens: usage_meta.total_token_count.unwrap_or(0),
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

        Ok(Box::new(Box::pin(mapped)))
    }

    async fn complete_with_tools(
        &self,
        request: CompletionRequest,
        _tools: Vec<ToolDef>,
    ) -> AppResult<ToolCompletionResponse> {
        // Basic implementation: delegate to complete and return empty tool_calls.
        let resp = self.complete(request).await?;
        Ok(ToolCompletionResponse {
            content: if resp.content.is_empty() {
                None
            } else {
                Some(resp.content)
            },
            tool_calls: vec![],
            usage: resp.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> GeminiProvider {
        GeminiProvider::new("test-key")
    }

    #[tokio::test]
    async fn model_list() {
        let p = make_provider();
        let models = p.available_models().await.unwrap();
        assert_eq!(models.len(), 3);
        assert!(models.iter().any(|m| m.id == "gemini-2.0-flash"));
        assert!(models.iter().any(|m| m.id == "gemini-1.5-pro"));
        assert!(models.iter().any(|m| m.id == "gemini-1.5-flash"));
    }

    #[test]
    fn url_generation_non_stream() {
        let p = make_provider();
        let url = p.build_url("gemini-2.0-flash", false);
        assert!(url.contains("generateContent"));
        assert!(!url.contains("streamGenerateContent"));
        assert!(url.contains("key=test-key"));
    }

    #[test]
    fn url_generation_stream() {
        let p = make_provider();
        let url = p.build_url("gemini-2.0-flash", true);
        assert!(url.contains("streamGenerateContent"));
        assert!(url.contains("alt=sse"));
        assert!(url.contains("key=test-key"));
    }
}
