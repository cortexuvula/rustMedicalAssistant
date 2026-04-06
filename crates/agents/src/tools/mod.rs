pub mod icd_lookup;
pub mod drug_interaction;
pub mod vitals_extractor;
pub mod rag_search;
pub mod checklist;

pub use icd_lookup::IcdLookupTool;
pub use drug_interaction::DrugInteractionTool;
pub use vitals_extractor::VitalsExtractorTool;
pub use rag_search::RagSearchTool;
pub use checklist::ChecklistTool;

use std::collections::HashMap;
use std::sync::Arc;

use medical_core::{
    traits::Tool,
    types::ToolDef,
};

/// Registry that holds all available tools by name.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool in the registry.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.definition().name.clone();
        self.tools.insert(name, tool);
    }

    /// Retrieve a tool by its name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Return the definitions of all registered tools.
    pub fn list_definitions(&self) -> Vec<ToolDef> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Create a registry pre-loaded with all 5 default medical tools.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(IcdLookupTool));
        registry.register(Arc::new(DrugInteractionTool));
        registry.register(Arc::new(VitalsExtractorTool));
        registry.register(Arc::new(RagSearchTool));
        registry.register(Arc::new(ChecklistTool));
        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_new_is_empty() {
        let registry = ToolRegistry::new();
        assert!(registry.list_definitions().is_empty());
    }

    #[test]
    fn registry_with_defaults_has_five_tools() {
        let registry = ToolRegistry::with_defaults();
        assert_eq!(registry.list_definitions().len(), 5);
    }

    #[test]
    fn registry_get_known_tool() {
        let registry = ToolRegistry::with_defaults();
        assert!(registry.get("search_icd_codes").is_some());
        assert!(registry.get("lookup_drug_interactions").is_some());
        assert!(registry.get("extract_vitals").is_some());
        assert!(registry.get("search_knowledge_base").is_some());
        assert!(registry.get("generate_checklist").is_some());
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let registry = ToolRegistry::with_defaults();
        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn registry_default_same_as_with_defaults() {
        let registry = ToolRegistry::default();
        assert_eq!(registry.list_definitions().len(), 5);
    }

    #[test]
    fn registry_register_custom_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(IcdLookupTool));
        assert_eq!(registry.list_definitions().len(), 1);
        assert!(registry.get("search_icd_codes").is_some());
    }
}
