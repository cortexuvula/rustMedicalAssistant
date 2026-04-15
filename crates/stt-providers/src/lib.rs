pub mod audio_prep;
pub mod failover;
pub mod deepgram;
pub mod groq_whisper;
pub mod elevenlabs_stt;
pub mod modulate;
pub mod whisper_local;
pub mod merge;
pub mod models;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("Transcription failed: {0}")]
    Transcription(String),
    #[error("Provider unavailable: {0}")]
    Unavailable(String),
    #[error("All providers exhausted")]
    AllProvidersExhausted,
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Audio format error: {0}")]
    AudioFormat(String),
    #[error("Model download error: {0}")]
    ModelDownload(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

pub type SttResult<T> = Result<T, SttError>;
