use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// The output format for a document export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Pdf,
    Docx,
    FhirBundle,
}

/// Options controlling how a document is exported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub format: ExportFormat,
    pub include_metadata: bool,
    pub watermark: Option<String>,
    pub page_size: String,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Pdf,
            include_metadata: true,
            watermark: None,
            page_size: "A4".into(),
        }
    }
}

/// Abstraction over document exporters.
#[async_trait]
pub trait Exporter: Send + Sync {
    /// The format this exporter produces.
    fn format(&self) -> ExportFormat;

    /// Export the given content using the provided configuration, returning
    /// the raw bytes of the exported document.
    async fn export(&self, content: &str, config: ExportConfig) -> AppResult<Vec<u8>>;
}
