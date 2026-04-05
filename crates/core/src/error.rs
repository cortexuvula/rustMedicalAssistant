use thiserror::Error;

/// Top-level application error.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("AI provider error: {0}")]
    AiProvider(String),

    #[error("STT provider error: {0}")]
    SttProvider(String),

    #[error("TTS provider error: {0}")]
    TtsProvider(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Processing error: {0}")]
    Processing(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error("Translation error: {0}")]
    Translation(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type AppResult<T> = Result<T, AppError>;

/// Severity level for error logging and UI display
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorSeverity {
    Critical,
    Error,
    Warning,
    Info,
}

/// Structured error context for logging and debugging
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorContext {
    pub operation: String,
    pub error: String,
    pub severity: ErrorSeverity,
    pub error_code: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub additional_info: serde_json::Value,
}

impl ErrorContext {
    pub fn new(operation: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            error: error.into(),
            severity: ErrorSeverity::Error,
            error_code: None,
            timestamp: chrono::Utc::now(),
            additional_info: serde_json::Value::Null,
        }
    }

    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_display_formats_correctly() {
        let err = AppError::Database("connection failed".into());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn app_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err, AppError::Io(_)));
        assert!(app_err.to_string().contains("file missing"));
    }

    #[test]
    fn error_context_builder() {
        let ctx = ErrorContext::new("save_recording", "disk full")
            .with_severity(ErrorSeverity::Critical)
            .with_code("DISK_FULL");
        assert_eq!(ctx.operation, "save_recording");
        assert_eq!(ctx.severity, ErrorSeverity::Critical);
        assert_eq!(ctx.error_code.as_deref(), Some("DISK_FULL"));
    }

    #[test]
    fn error_context_serializes_to_json() {
        let ctx = ErrorContext::new("test_op", "test_err");
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["operation"], "test_op");
        assert_eq!(json["error"], "test_err");
        assert!(json["timestamp"].is_string());
    }
}
