//! Ollama provider — wraps `OpenAiCompatibleClient` against a local server.

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::Client;

use medical_core::{
    error::AppResult,
    traits::AiProvider,
    types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse, ToolDef},
};

use crate::openai_compat::OpenAiCompatibleClient;

pub struct OllamaProvider {
    client: OpenAiCompatibleClient,
}

impl OllamaProvider {
    /// Create a new Ollama provider.
    ///
    /// `host` defaults to `http://localhost:11434` when `None`.
    pub fn new(host: Option<&str>) -> Self {
        let base = host.unwrap_or("http://localhost:11434");
        let base_url = format!("{base}/v1");
        // No auth header for Ollama.
        let http = Client::builder()
            .pool_max_idle_per_host(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build Ollama HTTP client");
        Self {
            client: OpenAiCompatibleClient::new(http, base_url),
        }
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn available_models(&self) -> AppResult<Vec<ModelInfo>> {
        // Ollama supports the OpenAI-compatible /v1/models endpoint
        if let Ok(ids) = self.client.list_models().await {
            let mut models: Vec<ModelInfo> = ids
                .into_iter()
                .map(|id| ModelInfo {
                    name: id.clone(),
                    id,
                    provider: "ollama".into(),
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
            id: "llama3".into(),
            name: "llama3".into(),
            provider: "ollama".into(),
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
        let p = OllamaProvider::new(None);
        assert_eq!(p.client.base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn creates_with_custom_host() {
        let p = OllamaProvider::new(Some("http://192.168.1.10:11434"));
        assert_eq!(p.client.base_url, "http://192.168.1.10:11434/v1");
    }
}
