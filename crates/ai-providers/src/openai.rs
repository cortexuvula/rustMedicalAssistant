//! OpenAI provider — wraps `OpenAiCompatibleClient`.

use async_trait::async_trait;
use futures_core::Stream;

use medical_core::{
    error::AppResult,
    traits::AiProvider,
    types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse, ToolDef},
};

use crate::http_client::build_client;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct OpenAiProvider {
    client: OpenAiCompatibleClient,
}

impl OpenAiProvider {
    pub fn new(api_key: &str) -> Self {
        let http = build_client(api_key, 120).expect("failed to build OpenAI HTTP client");
        Self {
            client: OpenAiCompatibleClient::new(http, "https://api.openai.com/v1"),
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "gpt-4o".into(),
                name: "GPT-4o".into(),
                provider: "openai".into(),
                max_tokens: 128_000,
                supports_tools: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "gpt-4o-mini".into(),
                name: "GPT-4o Mini".into(),
                provider: "openai".into(),
                max_tokens: 128_000,
                supports_tools: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "gpt-4-turbo".into(),
                name: "GPT-4 Turbo".into(),
                provider: "openai".into(),
                max_tokens: 128_000,
                supports_tools: true,
                supports_streaming: true,
            },
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
        // Box<Pin<Box<...>>> is Unpin because Box<T> is always Unpin.
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
