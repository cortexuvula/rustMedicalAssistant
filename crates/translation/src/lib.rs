pub mod session;
pub mod canned_responses;
pub mod ai_translator;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranslationError {
    #[error("translation error: {0}")]
    Translation(String),
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("language detection error: {0}")]
    Detection(String),
}

pub type TranslationResult<T> = Result<T, TranslationError>;
