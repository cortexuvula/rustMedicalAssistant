//! Migration 002 — RAG (Retrieval-Augmented Generation) tables.
//!
//! Creates `document_chunks` for storing text chunks with embeddings,
//! an FTS5 virtual table (`chunks_fts`) for BM25 keyword search,
//! and triggers to keep the two in sync.

use rusqlite::Connection;

use crate::DbResult;

pub fn up(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        -- ---------------------------------------------------------------
        -- document_chunks — RAG vector store
        -- ---------------------------------------------------------------
        CREATE TABLE IF NOT EXISTS document_chunks (
            id          TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            content     TEXT NOT NULL,
            embedding   BLOB,
            chunk_index INTEGER DEFAULT 0,
            metadata    TEXT DEFAULT '{}',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_doc
            ON document_chunks(document_id);

        -- ---------------------------------------------------------------
        -- FTS5 for BM25 search on chunks
        -- ---------------------------------------------------------------
        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            content,
            content='document_chunks',
            content_rowid='rowid',
            tokenize='porter unicode61'
        );

        -- ---------------------------------------------------------------
        -- Triggers to keep FTS5 in sync with document_chunks
        -- ---------------------------------------------------------------
        CREATE TRIGGER IF NOT EXISTS chunks_fts_insert
        AFTER INSERT ON document_chunks BEGIN
            INSERT INTO chunks_fts(rowid, content) VALUES (new.rowid, new.content);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_fts_delete
        AFTER DELETE ON document_chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content)
            VALUES ('delete', old.rowid, old.content);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_fts_update
        AFTER UPDATE ON document_chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content)
            VALUES ('delete', old.rowid, old.content);
            INSERT INTO chunks_fts(rowid, content)
            VALUES (new.rowid, new.content);
        END;
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    fn in_memory() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn creates_rag_tables() {
        let conn = in_memory();
        // Apply m001 first since document_chunks is standalone but
        // the full migration engine applies them in order.
        super::up(&conn).expect("migration should succeed");

        // Verify document_chunks exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='document_chunks'",
                [],
                |r| r.get(0),
            )
            .expect("query");
        assert!(count > 0, "document_chunks table should exist");

        // Verify chunks_fts exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='chunks_fts'",
                [],
                |r| r.get(0),
            )
            .expect("query");
        assert!(count > 0, "chunks_fts virtual table should exist");
    }

    #[test]
    fn fts_sync_on_insert() {
        let conn = in_memory();
        super::up(&conn).expect("migration");

        conn.execute(
            "INSERT INTO document_chunks (id, document_id, content) VALUES ('c1', 'doc1', 'hello world')",
            [],
        )
        .expect("insert chunk");

        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .expect("fts query");
        assert_eq!(fts_count, 1, "FTS should find the inserted chunk");
    }

    #[test]
    fn fts_sync_on_delete() {
        let conn = in_memory();
        super::up(&conn).expect("migration");

        conn.execute(
            "INSERT INTO document_chunks (id, document_id, content) VALUES ('c1', 'doc1', 'hello world')",
            [],
        )
        .expect("insert");

        conn.execute("DELETE FROM document_chunks WHERE id = 'c1'", [])
            .expect("delete");

        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .expect("fts query");
        assert_eq!(fts_count, 0, "FTS should be empty after delete");
    }

    #[test]
    fn fts_sync_on_update() {
        let conn = in_memory();
        super::up(&conn).expect("migration");

        conn.execute(
            "INSERT INTO document_chunks (id, document_id, content) VALUES ('c1', 'doc1', 'hello world')",
            [],
        )
        .expect("insert");

        conn.execute(
            "UPDATE document_chunks SET content = 'goodbye world' WHERE id = 'c1'",
            [],
        )
        .expect("update");

        let old_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .expect("fts query");
        assert_eq!(old_count, 0, "old content should not be in FTS");

        let new_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'goodbye'",
                [],
                |r| r.get(0),
            )
            .expect("fts query");
        assert_eq!(new_count, 1, "updated content should be in FTS");
    }
}
