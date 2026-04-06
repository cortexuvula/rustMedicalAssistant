use uuid::Uuid;
use medical_core::error::AppResult;

/// Stub ingestion pipeline that parses and indexes documents.
pub struct IngestionPipeline;

impl IngestionPipeline {
    pub fn new() -> Self {
        Self
    }

    /// Ingest a plain-text document. Returns the number of chunks created (always 0 in stub).
    pub async fn ingest_text(
        &self,
        _doc_id: Uuid,
        _title: &str,
        _text: &str,
    ) -> AppResult<u32> {
        Ok(0)
    }

    /// Delete all indexed data for a document.
    pub async fn delete_document(&self, _doc_id: Uuid) -> AppResult<()> {
        Ok(())
    }
}

impl Default for IngestionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Split `text` into overlapping chunks of approximately `chunk_size` words.
///
/// Words that fall within the overlap window are repeated at the start of the
/// next chunk. Returns an empty `Vec` when `chunk_size` is 0.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if chunk_size == 0 {
        return Vec::new();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    if words.len() <= chunk_size {
        return vec![words.join(" ")];
    }

    let step = if chunk_size > overlap {
        chunk_size - overlap
    } else {
        1
    };

    let mut chunks: Vec<String> = Vec::new();
    let mut start = 0;

    while start < words.len() {
        let end = (start + chunk_size).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += step;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_chunking() {
        // 6 words, chunk_size=3, overlap=1 → step=2
        // chunk 0: [0,1,2], chunk 1: [2,3,4], chunk 2: [4,5]
        let text = "one two three four five six";
        let chunks = chunk_text(text, 3, 1);
        assert_eq!(chunks.len(), 3, "expected 3 chunks, got {:?}", chunks);
        assert_eq!(chunks[0], "one two three");
        assert_eq!(chunks[1], "three four five");
        assert_eq!(chunks[2], "five six");
    }

    #[test]
    fn short_text_returns_one_chunk() {
        let text = "hello world";
        let chunks = chunk_text(text, 10, 2);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello world");
    }

    #[test]
    fn empty_returns_empty() {
        let chunks = chunk_text("", 5, 1);
        assert!(chunks.is_empty());
    }

    #[test]
    fn zero_size_returns_empty() {
        let chunks = chunk_text("one two three", 0, 0);
        assert!(chunks.is_empty());
    }
}
