pub mod key_storage;
pub mod machine_id;
pub mod phi_redactor;
pub mod audit_logger;
pub mod input_sanitizer;
pub mod rate_limiter;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Decryption error: {0}")]
    Decryption(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid key format")]
    InvalidFormat,
}

pub type SecurityResult<T> = Result<T, SecurityError>;
