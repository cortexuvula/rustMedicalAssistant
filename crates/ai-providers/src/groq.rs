//! Groq provider — wraps `OpenAiCompatibleClient`.

use async_trait::async_trait;
use futures_core::Stream;

use medical_core::{
    error::AppResult,
    traits::AiProvider,
    types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse, ToolDef},
};

use crate::http_client::build_client;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct GroqProvider {
    client: OpenAiCompatibleClient,
}

impl GroqProvider {
    pub fn new(api_key: &str) -> Self {
        let http = build_client(api_key, 120).expect("failed to build Groq HTTP client");
        Self {
            client: OpenAiCompatibleClient::new(http, "https://api.groq.com/openai/v1"),
        }
    }
}

#[async_trait]
impl AiProvider for GroqProvider {
    fn name(&self) -> &str {
        "groq"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        // Groq's /models endpoint only returns usable models — no filtering needed.
        // Exclude whisper/distil audio models since they aren't chat models.
        if let Ok(ids) = self.client.list_models().await {
            let mut models: Vec<ModelInfo> = ids
                .into_iter()
                .filter(|id| !id.contains("whisper") && !id.contains("distil"))
                .map(|id| ModelInfo {
                    name: id.clone(),
                    id,
                    provider: "groq".into(),
                    max_tokens: 32_768,
                    supports_tools: true,
                    supports_streaming: true,
                })
                .collect();
            if !models.is_empty() {
                models.sort_by(|a, b| a.id.cmp(&b.id));
                return Ok(models);
            }
        }

        // Fallback
        Ok(vec![
            ModelInfo { id: "llama-3.3-70b-versatile".into(), name: "llama-3.3-70b-versatile".into(), provider: "groq".into(), max_tokens: 32_768, supports_tools: true, supports_streaming: true },
            ModelInfo { id: "mixtral-8x7b-32768".into(), name: "mixtral-8x7b-32768".into(), provider: "groq".into(), max_tokens: 32_768, supports_tools: true, supports_streaming: true },
        ])
    }

    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        self.client.complete(&request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send + Unpin>> {
        let pinned = self.client.complete_stream(&request).await?;
        Ok(Box::new(pinned))
    }

    async fn complete_with_tools(
        &self,
        request: CompletionRequest,
        tools: Vec<ToolDef>,
    ) -> AppResult<ToolCompletionResponse> {
        self.client.complete_with_tools(&request, tools).await
    }
}
