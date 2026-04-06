use medical_core::types::rag::{GraphEntity, GraphRelation, RagResult};
use crate::RagError;

/// Stub knowledge-graph search.
pub struct GraphSearch;

impl GraphSearch {
    pub fn new() -> Self {
        Self
    }

    /// Search the graph for results related to `query`.
    pub fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<RagResult>, RagError> {
        Ok(Vec::new())
    }

    /// Persist a graph entity node.
    pub fn store_entity(&self, _entity: &GraphEntity) -> Result<(), RagError> {
        Ok(())
    }

    /// Persist a directed relation between two graph entities.
    pub fn store_relation(&self, _relation: &GraphRelation) -> Result<(), RagError> {
        Ok(())
    }
}

impl Default for GraphSearch {
    fn default() -> Self {
        Self::new()
    }
}
