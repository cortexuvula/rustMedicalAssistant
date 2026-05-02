//! Database encryption helpers for SQLCipher.
//!
//! The pool's `with_init` callback uses `apply_pragma_key` (when a key is
//! supplied) before any other PRAGMA, since SQLCipher requires the key to be
//! the very first statement on a fresh connection.

use rusqlite::Connection;

use crate::DbResult;

/// Apply `PRAGMA key="x'<hex>'"` so SQLCipher can decrypt the DB.
/// Must run before any other statement on the connection.
pub fn apply_pragma_key(conn: &Connection, key: &[u8; 32]) -> rusqlite::Result<()> {
    let hex_key = hex::encode(key);
    // Use the x'...' blob literal form so the key is interpreted as 32 bytes
    // rather than as the ASCII string of hex digits.
    conn.execute_batch(&format!("PRAGMA key=\"x'{hex_key}'\";"))
}

/// Verify the key by running a trivial read. Returns Err if SQLCipher rejects
/// the key (e.g., wrong key, file not encrypted, file corrupt).
pub fn verify_key(conn: &Connection) -> rusqlite::Result<()> {
    // SELECT count(*) FROM sqlite_master is the canonical key-verification
    // probe — it forces SQLCipher to decrypt page 1.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
}

/// Detect whether the given DB file is plaintext (SQLite header) or
/// encrypted (SQLCipher header). Plaintext returns Ok(true); encrypted
/// returns Ok(false). Errors propagate.
pub fn is_plaintext_db(db_path: &std::path::Path) -> DbResult<bool> {
    use std::io::Read;
    if !db_path.exists() {
        return Ok(false); // treat missing as not-plaintext (caller decides what to do)
    }
    let mut f = std::fs::File::open(db_path)
        .map_err(|e| crate::DbError::Other(format!("open {db_path:?}: {e}")))?;
    let mut header = [0u8; 16];
    let n = f
        .read(&mut header)
        .map_err(|e| crate::DbError::Other(format!("read {db_path:?}: {e}")))?;
    if n < 16 {
        return Ok(false);
    }
    // SQLite plaintext files start with the magic string "SQLite format 3\0"
    // (16 bytes). SQLCipher files start with random ciphertext.
    Ok(&header == b"SQLite format 3\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_pragma_key_does_not_error_on_in_memory_db() {
        // With SQLCipher bundled, applying PRAGMA key on an in-memory DB
        // succeeds and creates an encrypted-in-memory connection. We're
        // checking that the call returns Ok, not the cipher behavior.
        let conn = Connection::open_in_memory().unwrap();
        let key = [0xABu8; 32];
        apply_pragma_key(&conn, &key).expect("pragma key");
    }

    #[test]
    fn is_plaintext_db_returns_false_for_missing_file() {
        let path = std::path::PathBuf::from("/nonexistent/path.db");
        let result = is_plaintext_db(&path).unwrap();
        assert!(!result);
    }

    #[test]
    fn is_plaintext_db_recognizes_sqlite_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t(x);").unwrap();
        drop(conn);
        assert!(is_plaintext_db(&path).unwrap());
    }
}
