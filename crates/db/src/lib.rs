pub mod pool;
pub mod migrations;
pub mod recordings;
pub mod processing_queue;
pub mod recipients;
pub mod settings;
pub mod audit;
pub mod search;
pub mod vectors;
pub mod graph;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Constraint violation: {0}")]
    Constraint(String),
}

pub type DbResult<T> = Result<T, DbError>;
