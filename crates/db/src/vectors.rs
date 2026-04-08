//! CRUD operations for the `document_chunks` table (RAG vector store).

use rusqlite::{Connection, params};

use crate::DbResult;

/// A single document chunk with its embedding vector.
#[derive(Debug, Clone)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub chunk_index: i64,
    pub metadata: String,
    pub created_at: String,
}

/// Lightweight tuple returned by `get_all_embeddings`.
#[derive(Debug, Clone)]
pub struct EmbeddingRecord {
    pub id: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

/// Result of an FTS5 full-text search.
#[derive(Debug, Clone)]
pub struct FtsResult {
    pub id: String,
    pub content: String,
    pub rank: f64,
}

pub struct VectorsRepo;

impl VectorsRepo {
    pub fn new() -> Self {
        Self
    }

    // ------------------------------------------------------------------
    // Write operations
    // ------------------------------------------------------------------

    /// Insert (or replace) a document chunk with an optional embedding.
    ///
    /// The `Vec<f32>` embedding is serialised to a `BLOB` using `bytemuck`.
    pub fn insert_chunk(
        conn: &Connection,
        id: &str,
        document_id: &str,
        content: &str,
        embedding: Option<&[f32]>,
        chunk_index: i64,
        metadata: &str,
    ) -> DbResult<()> {
        let blob: Option<Vec<u8>> =
            embedding.map(|e| bytemuck::cast_slice(e).to_vec());

        conn.execute(
            "INSERT OR REPLACE INTO document_chunks
                (id, document_id, content, embedding, chunk_index, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, document_id, content, blob, chunk_index, metadata],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Read operations
    // ------------------------------------------------------------------

    /// Return every chunk that has a non-NULL embedding.
    ///
    /// Each BLOB is deserialised back into `Vec<f32>` via `bytemuck`.
    pub fn get_all_embeddings(conn: &Connection) -> DbResult<Vec<EmbeddingRecord>> {
        let mut stmt = conn.prepare(
            "SELECT id, content, embedding
             FROM document_chunks
             WHERE embedding IS NOT NULL",
        )?;

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let blob: Vec<u8> = row.get(2)?;
                let embedding: Vec<f32> =
                    bytemuck::cast_slice(&blob).to_vec();
                Ok(EmbeddingRecord {
                    id,
                    content,
                    embedding,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Retrieve all chunks belonging to a given document, ordered by
    /// `chunk_index`.
    pub fn get_by_document(
        conn: &Connection,
        document_id: &str,
    ) -> DbResult<Vec<DocumentChunk>> {
        let mut stmt = conn.prepare(
            "SELECT id, document_id, content, embedding, chunk_index, metadata, created_at
             FROM document_chunks
             WHERE document_id = ?1
             ORDER BY chunk_index ASC",
        )?;

        let rows = stmt
            .query_map([document_id], |row| {
                let blob: Option<Vec<u8>> = row.get(3)?;
                let embedding = blob.map(|b| {
                    bytemuck::cast_slice::<u8, f32>(&b).to_vec()
                });

                Ok(DocumentChunk {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    content: row.get(2)?,
                    embedding,
                    chunk_index: row.get(4)?,
                    metadata: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Total number of rows in `document_chunks`.
    pub fn count(conn: &Connection) -> DbResult<u32> {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM document_chunks",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// Full-text search via the FTS5 index.
    ///
    /// Returns up to `top_k` results ranked by BM25 relevance (lower
    /// `rank` = better match in FTS5 convention; we negate so higher =
    /// better for callers).
    pub fn search_fts(
        conn: &Connection,
        query: &str,
        top_k: u32,
    ) -> DbResult<Vec<FtsResult>> {
        let mut stmt = conn.prepare(
            "SELECT dc.id, dc.content, f.rank
             FROM chunks_fts f
             JOIN document_chunks dc ON dc.rowid = f.rowid
             WHERE chunks_fts MATCH ?1
             ORDER BY f.rank
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![query, top_k], |row| {
                Ok(FtsResult {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    rank: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    // ------------------------------------------------------------------
    // Delete operations
    // ------------------------------------------------------------------

    /// Delete all chunks belonging to a document.  Returns the number of
    /// rows removed.
    pub fn delete_by_document(conn: &Connection, document_id: &str) -> DbResult<u32> {
        let deleted = conn.execute(
            "DELETE FROM document_chunks WHERE document_id = ?1",
            [document_id],
        )?;
        Ok(deleted as u32)
    }
}

impl Default for VectorsRepo {
    fn default() -> Self {
        Self::new()
    }
}

// ======================================================================
// Tests
// ======================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::MigrationEngine;
    use rusqlite::Connection;

    fn migrated_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        MigrationEngine::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn insert_and_count() {
        let conn = migrated_conn();
        assert_eq!(VectorsRepo::count(&conn).unwrap(), 0);

        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "Hello world",
            Some(&[1.0, 2.0, 3.0]), 0, "{}",
        )
        .unwrap();

        assert_eq!(VectorsRepo::count(&conn).unwrap(), 1);
    }

    #[test]
    fn insert_and_retrieve_embedding() {
        let conn = migrated_conn();
        let emb = vec![0.1_f32, 0.2, 0.3, 0.4];

        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "test content",
            Some(&emb), 0, "{}",
        )
        .unwrap();

        let all = VectorsRepo::get_all_embeddings(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "c1");
        assert_eq!(all[0].content, "test content");
        assert_eq!(all[0].embedding, emb);
    }

    #[test]
    fn null_embedding_excluded_from_get_all() {
        let conn = migrated_conn();

        // Insert one with embedding, one without.
        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "has embedding",
            Some(&[1.0, 2.0]), 0, "{}",
        )
        .unwrap();
        VectorsRepo::insert_chunk(
            &conn, "c2", "doc1", "no embedding",
            None, 1, "{}",
        )
        .unwrap();

        let all = VectorsRepo::get_all_embeddings(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "c1");
    }

    #[test]
    fn get_by_document_ordered() {
        let conn = migrated_conn();

        // Insert out of order.
        VectorsRepo::insert_chunk(
            &conn, "c2", "doc1", "second chunk",
            None, 1, "{}",
        )
        .unwrap();
        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "first chunk",
            None, 0, "{}",
        )
        .unwrap();

        let chunks = VectorsRepo::get_by_document(&conn, "doc1").unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].id, "c1");
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].id, "c2");
        assert_eq!(chunks[1].chunk_index, 1);
    }

    #[test]
    fn delete_by_document() {
        let conn = migrated_conn();

        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "chunk a", None, 0, "{}",
        )
        .unwrap();
        VectorsRepo::insert_chunk(
            &conn, "c2", "doc1", "chunk b", None, 1, "{}",
        )
        .unwrap();
        VectorsRepo::insert_chunk(
            &conn, "c3", "doc2", "other doc", None, 0, "{}",
        )
        .unwrap();

        let deleted = VectorsRepo::delete_by_document(&conn, "doc1").unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(VectorsRepo::count(&conn).unwrap(), 1);
    }

    #[test]
    fn delete_nonexistent_returns_zero() {
        let conn = migrated_conn();
        let deleted = VectorsRepo::delete_by_document(&conn, "nope").unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn fts_search() {
        let conn = migrated_conn();

        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "the patient has diabetes mellitus",
            None, 0, "{}",
        )
        .unwrap();
        VectorsRepo::insert_chunk(
            &conn, "c2", "doc1", "hypertension treatment protocol",
            None, 1, "{}",
        )
        .unwrap();
        VectorsRepo::insert_chunk(
            &conn, "c3", "doc2", "diabetes management guidelines",
            None, 0, "{}",
        )
        .unwrap();

        let results = VectorsRepo::search_fts(&conn, "diabetes", 10).unwrap();
        assert_eq!(results.len(), 2);
        // Both results should mention diabetes-related content.
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"c1"));
        assert!(ids.contains(&"c3"));
    }

    #[test]
    fn fts_search_top_k_limit() {
        let conn = migrated_conn();

        for i in 0..5 {
            let id = format!("c{i}");
            let content = format!("medical record number {i}");
            VectorsRepo::insert_chunk(
                &conn, &id, "doc1", &content,
                None, i, "{}",
            )
            .unwrap();
        }

        let results = VectorsRepo::search_fts(&conn, "medical", 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn insert_or_replace_overwrites() {
        let conn = migrated_conn();

        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "original",
            Some(&[1.0]), 0, "{}",
        )
        .unwrap();

        VectorsRepo::insert_chunk(
            &conn, "c1", "doc1", "updated",
            Some(&[2.0]), 0, "{}",
        )
        .unwrap();

        assert_eq!(VectorsRepo::count(&conn).unwrap(), 1);

        let all = VectorsRepo::get_all_embeddings(&conn).unwrap();
        assert_eq!(all[0].content, "updated");
        assert_eq!(all[0].embedding, vec![2.0_f32]);
    }
}
