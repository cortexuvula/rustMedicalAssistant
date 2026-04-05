//! Cerebras provider — wraps `OpenAiCompatibleClient`.

use async_trait::async_trait;
use futures_core::Stream;

use medical_core::{
    error::AppResult,
    traits::AiProvider,
    types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse, ToolDef},
};

use crate::http_client::build_client;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct CerebrasProvider {
    client: OpenAiCompatibleClient,
}

impl CerebrasProvider {
    pub fn new(api_key: &str) -> Self {
        let http = build_client(api_key, 120).expect("failed to build Cerebras HTTP client");
        Self {
            client: OpenAiCompatibleClient::new(http, "https://api.cerebras.ai/v1"),
        }
    }
}

#[async_trait]
impl AiProvider for CerebrasProvider {
    fn name(&self) -> &str {
        "cerebras"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "llama-3.3-70b".into(),
                name: "LLaMA 3.3 70B".into(),
                provider: "cerebras".into(),
                max_tokens: 8_192,
                supports_tools: false,
                supports_streaming: true,
            },
            ModelInfo {
                id: "qwen-3-32b".into(),
                name: "Qwen 3 32B".into(),
                provider: "cerebras".into(),
                max_tokens: 8_192,
                supports_tools: false,
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
