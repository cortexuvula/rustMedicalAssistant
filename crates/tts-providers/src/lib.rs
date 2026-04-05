pub mod elevenlabs_tts;
pub mod local_tts;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TtsError {
    #[error("Synthesis failed: {0}")]
    Synthesis(String),
    #[error("Voice not found: {0}")]
    VoiceNotFound(String),
    #[error("HTTP error: {0}")]
    Http(String),
}

pub type TtsResult<T> = Result<T, TtsError>;
