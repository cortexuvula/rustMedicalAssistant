pub mod orchestrator;
pub mod tools;
pub mod agents;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent execution error: {0}")]
    Execution(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("max iterations reached: {0}")]
    MaxIterations(u32),
    #[error("agent cancelled")]
    Cancelled,
    #[error("provider error: {0}")]
    Provider(String),
}

pub type AgentResult<T> = Result<T, AgentError>;
