use serde::{Deserialize, Serialize};

use super::ai::{Message, UsageInfo};
use super::rag::RagResult;
use super::recording::Recording;

/// Definition of a tool that an agent can call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// The output of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    /// Construct a successful output.
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    /// Construct an error output.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// The runtime context passed to an agent when processing a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub user_message: String,
    pub conversation_history: Vec<Message>,
    pub patient_context: Option<PatientContext>,
    pub rag_context: Vec<RagResult>,
    pub recording: Option<Recording>,
}

/// A snapshot of patient-specific context for grounding agent responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientContext {
    pub patient_name: Option<String>,
    pub prior_soap_notes: Vec<String>,
    pub medications: Vec<String>,
    pub conditions: Vec<String>,
    pub allergies: Vec<String>,
}

/// The final response from an agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
    pub tool_calls_made: Vec<AgentToolCallRecord>,
    pub usage: UsageInfo,
    pub iterations: u32,
}

/// A record of a single tool invocation during an agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCallRecord {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: ToolOutput,
    pub duration_ms: u64,
}

/// Runtime settings for a specific agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub system_prompt: Option<String>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: "openai".into(),
            model: "gpt-4o".into(),
            temperature: 0.2,
            max_tokens: 4096,
            system_prompt: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_success() {
        let out = ToolOutput::success("ok");
        assert_eq!(out.content, "ok");
        assert!(!out.is_error);
    }

    #[test]
    fn tool_output_error() {
        let out = ToolOutput::error("something went wrong");
        assert_eq!(out.content, "something went wrong");
        assert!(out.is_error);
    }

    #[test]
    fn agent_settings_defaults() {
        let settings = AgentSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.provider, "openai");
        assert_eq!(settings.model, "gpt-4o");
        assert!((settings.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(settings.max_tokens, 4096);
        assert!(settings.system_prompt.is_none());
    }

    #[test]
    fn tool_output_round_trip() {
        let out = ToolOutput::success("result data");
        let json = serde_json::to_string(&out).unwrap();
        let back: ToolOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "result data");
        assert!(!back.is_error);
    }
}
