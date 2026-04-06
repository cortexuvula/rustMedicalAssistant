use async_trait::async_trait;
use medical_core::{
    error::AppResult,
    traits::Tool,
    types::{ToolDef, ToolOutput},
};
use serde_json::json;

/// Tool for querying the medical knowledge base (RAG system).
/// Currently returns empty results until connected to the RAG crate.
pub struct RagSearchTool;

#[async_trait]
impl Tool for RagSearchTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "search_knowledge_base".into(),
            description: "Search the medical knowledge base using semantic similarity. Returns relevant clinical documents and evidence for the given query.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The clinical question or search query"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5,
                        "minimum": 1,
                        "maximum": 20
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> AppResult<ToolOutput> {
        let query = match arguments.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return Ok(ToolOutput::error("query parameter is required")),
        };

        let top_k = arguments
            .get("top_k")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        // Stub: returns empty results until connected to actual RAG crate
        let content = serde_json::to_string_pretty(&json!({
            "query": query,
            "top_k": top_k,
            "results": [],
            "note": "Knowledge base search is not yet connected. Results will be available once the RAG system is initialized."
        }))
        .unwrap_or_else(|_| "serialization error".into());

        Ok(ToolOutput::success(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_empty_results_stub() {
        let tool = RagSearchTool;
        let result = tool
            .execute(json!({"query": "hypertension treatment guidelines"}))
            .await
            .unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed["results"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn respects_top_k_parameter() {
        let tool = RagSearchTool;
        let result = tool
            .execute(json!({"query": "diabetes management", "top_k": 10}))
            .await
            .unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["top_k"].as_u64().unwrap(), 10);
    }

    #[test]
    fn tool_definition_has_correct_name() {
        let tool = RagSearchTool;
        assert_eq!(tool.definition().name, "search_knowledge_base");
    }
}
