use medical_core::error::AppResult;

/// Stub embedding generator that returns zero vectors.
pub struct EmbeddingGenerator;

impl EmbeddingGenerator {
    pub fn new() -> Self {
        Self
    }

    /// Generate a 1536-dimensional embedding for `text`.
    pub async fn embed(&self, _text: &str) -> AppResult<Vec<f32>> {
        Ok(vec![0.0_f32; 1536])
    }

    /// Generate embeddings for a batch of texts.
    pub async fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.0_f32; 1536]).collect())
    }
}

impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}
