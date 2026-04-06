use medical_core::types::rag::RagResult;
use crate::RagError;

/// Stub BM25 full-text search index.
pub struct Bm25Search;

impl Bm25Search {
    pub fn new() -> Self {
        Self
    }

    /// Search the index for documents matching `query`, returning up to `top_k` results.
    pub fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<RagResult>, RagError> {
        Ok(Vec::new())
    }
}

impl Default for Bm25Search {
    fn default() -> Self {
        Self::new()
    }
}
