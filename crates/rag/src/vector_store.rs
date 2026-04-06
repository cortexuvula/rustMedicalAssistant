use uuid::Uuid;
use medical_core::types::rag::{DocumentChunk, RagResult};
use crate::RagError;

/// In-memory (stub) vector store for similarity search.
pub struct VectorStore;

impl VectorStore {
    pub fn new() -> Self {
        Self
    }

    /// Persist a document chunk (no-op in this stub).
    pub fn store_chunk(&self, _chunk: &DocumentChunk) -> Result<(), RagError> {
        Ok(())
    }

    /// Search for the closest chunks to an embedding vector.
    pub fn search(
        &self,
        _embedding: &[f32],
        _top_k: usize,
        _threshold: f32,
    ) -> Result<Vec<RagResult>, RagError> {
        Ok(Vec::new())
    }

    /// Delete all chunks belonging to a document.
    pub fn delete_document(&self, _document_id: &Uuid) -> Result<(), RagError> {
        Ok(())
    }
}

impl Default for VectorStore {
    fn default() -> Self {
        Self::new()
    }
}
