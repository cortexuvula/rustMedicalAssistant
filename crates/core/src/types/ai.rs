use serde::{Deserialize, Serialize};

/// Metadata about an available AI model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub max_tokens: u32,
    pub supports_tools: bool,
    pub supports_streaming: bool,
}

/// A request to generate a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: Option<String>,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

/// The role of the message author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// The body of a message — either plain text or a tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// A complete response from the AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: UsageInfo,
    pub tool_calls: Vec<ToolCall>,
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Token usage statistics for a completion.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A chunk of a streaming completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamChunk {
    Delta {
        text: String,
    },
    ToolCallDelta {
        id: String,
        name: Option<String>,
        arguments_delta: String,
    },
    Usage(UsageInfo),
    Done,
}

/// Response from a completion that may include tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCompletionResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: UsageInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_content_text_serializes() {
        let content = MessageContent::Text("hello".into());
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json, serde_json::json!("hello"));
    }

    #[test]
    fn message_content_tool_result_serializes() {
        let content = MessageContent::ToolResult {
            tool_call_id: "call_1".into(),
            content: "result text".into(),
        };
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["tool_call_id"], "call_1");
        assert_eq!(json["content"], "result text");
    }

    #[test]
    fn stream_chunk_tagged_serialization() {
        let delta = StreamChunk::Delta { text: "Hi".into() };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["type"], "delta");
        assert_eq!(json["text"], "Hi");

        let done = StreamChunk::Done;
        let json = serde_json::to_value(&done).unwrap();
        assert_eq!(json["type"], "done");
    }

    #[test]
    fn role_serializes_snake_case() {
        let role = Role::Assistant;
        let json = serde_json::to_value(&role).unwrap();
        assert_eq!(json, "assistant");

        let system: Role = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(system, Role::System);
    }

    #[test]
    fn completion_response_round_trip() {
        let resp = CompletionResponse {
            content: "Hello".into(),
            model: "gpt-4o".into(),
            usage: UsageInfo {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            tool_calls: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CompletionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "Hello");
        assert_eq!(back.usage.total_tokens, 15);
    }
}
