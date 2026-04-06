pub mod pdf;
pub mod docx;
pub mod fhir;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("PDF export error: {0}")]
    Pdf(String),
    #[error("DOCX export error: {0}")]
    Docx(String),
    #[error("FHIR export error: {0}")]
    Fhir(String),
    #[error("IO error: {0}")]
    Io(String),
}

pub type ExportResult<T> = Result<T, ExportError>;
