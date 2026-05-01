use async_trait::async_trait;
use futures_core::Stream;
use tokio_util::sync::CancellationToken;

use crate::error::AppResult;
use crate::types::{AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk};

/// Abstraction over any speech-to-text provider.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// The canonical name of this provider (e.g. "groq").
    fn name(&self) -> &str;

    /// Returns `true` if this provider supports streaming transcription.
    fn supports_streaming(&self) -> bool;

    /// Returns `true` if this provider supports speaker diarization.
    fn supports_diarization(&self) -> bool;

    /// Transcribe a complete audio buffer and return the full transcript.
    ///
    /// `cancel`: when the token fires, the provider should return `AppError::Cancelled`
    /// as promptly as possible. Implementations differ in how interruptible they are:
    /// remote providers cancel in-flight HTTP via `tokio::select!`; local providers
    /// check the token before/after the blocking model invocation but cannot
    /// interrupt it mid-pass.
    async fn transcribe(
        &self,
        audio: AudioData,
        config: SttConfig,
        cancel: CancellationToken,
    ) -> AppResult<Transcript>;

    /// Transcribe a live audio stream, yielding chunks as they are recognized.
    async fn transcribe_stream(
        &self,
        stream: AudioStream,
        config: SttConfig,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>>;
}
