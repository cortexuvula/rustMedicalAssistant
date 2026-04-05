use async_trait::async_trait;
use futures_core::Stream;

use crate::error::AppResult;
use crate::types::{
    CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, ToolCompletionResponse, ToolDef,
};

/// Abstraction over any AI completion provider.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// The canonical name of this provider (e.g. "openai").
    fn name(&self) -> &str;

    /// Returns the list of models this provider supports.
    async fn available_models(&self) -> AppResult<Vec<ModelInfo>>;

    /// Send a completion request and wait for the full response.
    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse>;

    /// Send a completion request and receive a stream of chunks.
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<StreamChunk>> + Send + Unpin>>;

    /// Send a completion request that may invoke tools, returning the result
    /// with any requested tool calls.
    async fn complete_with_tools(
        &self,
        request: CompletionRequest,
        tools: Vec<ToolDef>,
    ) -> AppResult<ToolCompletionResponse>;
}
