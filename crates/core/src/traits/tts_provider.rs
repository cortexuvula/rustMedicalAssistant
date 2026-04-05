use async_trait::async_trait;

use crate::error::AppResult;
use crate::types::{TtsConfig, VoiceInfo};

/// Abstraction over any text-to-speech provider.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// The canonical name of this provider (e.g. "elevenlabs").
    fn name(&self) -> &str;

    /// Returns the voices available from this provider.
    async fn available_voices(&self) -> AppResult<Vec<VoiceInfo>>;

    /// Synthesize the given text and return raw PCM audio bytes.
    async fn synthesize(&self, text: &str, config: TtsConfig) -> AppResult<Vec<u8>>;
}
