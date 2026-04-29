//! LM Studio provider — wraps `OpenAiCompatibleClient` against a local LM Studio server.

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::Client;

use medical_core::{
    error::{AppError, AppResult},
    traits::AiProvider,
    types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse, ToolDef},
};

use crate::http_client::RetryConfig;
use crate::openai_compat::OpenAiCompatibleClient;

pub struct LmStudioProvider {
    client: OpenAiCompatibleClient,
}

impl LmStudioProvider {
    /// Create a new LM Studio provider.
    ///
    /// `host` defaults to `http://localhost:1234` when `None`.
    /// `policy` controls retry behavior for inner HTTP calls.
    pub fn new(host: Option<&str>, policy: RetryConfig) -> AppResult<Self> {
        let base = host.unwrap_or("http://localhost:1234");
        let base_url = format!("{base}/v1");
        let http = Client::builder()
            .pool_max_idle_per_host(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::AiProvider(format!("Failed to build LM Studio HTTP client: {e}")))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, base_url, policy),
        })
    }
}

#[async_trait]
impl AiProvider for LmStudioProvider {
    fn name(&self) -> &str {
        "lmstudio"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        // LM Studio supports the OpenAI-compatible /v1/models endpoint
        if let Ok(ids) = self.client.list_models().await {
            let mut models: Vec<ModelInfo> = ids
                .into_iter()
                .map(|id| ModelInfo {
                    name: id.clone(),
                    id,
                    provider: "lmstudio".into(),
                    max_tokens: 8_192,
                    supports_tools: false,
                    supports_streaming: true,
                })
                .collect();
            if !models.is_empty() {
                models.sort_by(|a, b| a.id.cmp(&b.id));
                return Ok(models);
            }
        }

        // Fallback
        Ok(vec![ModelInfo {
            id: "default".into(),
            name: "default".into(),
            provider: "lmstudio".into(),
            max_tokens: 8_192,
            supports_tools: false,
            supports_streaming: true,
        }])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_with_default_host() {
        let p = LmStudioProvider::new(None, RetryConfig::default()).expect("build default provider");
        assert_eq!(p.client.base_url, "http://localhost:1234/v1");
    }

    #[test]
    fn creates_with_custom_host() {
        let p = LmStudioProvider::new(
            Some("http://192.168.1.10:1234"),
            RetryConfig::default(),
        )
        .expect("build custom provider");
        assert_eq!(p.client.base_url, "http://192.168.1.10:1234/v1");
    }
}
