use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// A supported language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
}

/// Abstraction over any translation backend.
#[async_trait]
pub trait TranslationProvider: Send + Sync {
    /// The canonical name of this provider (e.g. "deepl").
    fn name(&self) -> &str;

    /// Returns all languages this provider can translate to/from.
    async fn supported_languages(&self) -> AppResult<Vec<Language>>;

    /// Translate `text` from `source_language` (BCP-47 code, e.g. "en") into
    /// `target_language`.  If `source_language` is `None` the provider will
    /// attempt to detect it automatically.
    async fn translate(
        &self,
        text: &str,
        source_language: Option<&str>,
        target_language: &str,
    ) -> AppResult<String>;

    /// Detect the language of the supplied text.  Returns the BCP-47 language
    /// code of the most likely language.
    async fn detect_language(&self, text: &str) -> AppResult<String>;
}
