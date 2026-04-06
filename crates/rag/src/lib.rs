pub mod query_expander;
pub mod vector_store;
pub mod bm25;
pub mod graph_search;
pub mod fusion;
pub mod mmr;
pub mod embeddings;
pub mod ingestion;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RagError {
    #[error("search error: {0}")]
    Search(String),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("ingestion error: {0}")]
    Ingestion(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("no results found")]
    NoResults,
}

pub type RagResult<T> = Result<T, RagError>;
