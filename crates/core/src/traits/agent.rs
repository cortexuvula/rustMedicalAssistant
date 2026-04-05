use async_trait::async_trait;

use crate::error::AppResult;
use crate::types::{AgentContext, AgentResponse, ToolDef, ToolOutput};

/// An autonomous agent that can use tools to respond to user requests.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Short identifier for this agent.
    fn name(&self) -> &str;

    /// Human-readable description of what this agent does.
    fn description(&self) -> &str;

    /// The system prompt used to prime this agent.
    fn system_prompt(&self) -> &str;

    /// The set of tools this agent is allowed to invoke.
    fn available_tools(&self) -> Vec<ToolDef>;

    /// Process the given context and return a response, potentially using tools.
    async fn execute(&self, context: AgentContext) -> AppResult<AgentResponse>;
}

/// A discrete capability that an agent can invoke.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the tool's schema definition.
    fn definition(&self) -> ToolDef;

    /// Execute the tool with the given JSON arguments.
    async fn execute(&self, arguments: serde_json::Value) -> AppResult<ToolOutput>;
}
