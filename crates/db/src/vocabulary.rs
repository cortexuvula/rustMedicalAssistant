use chrono::{DateTime, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use medical_core::types::vocabulary::{VocabularyCategory, VocabularyEntry};

use crate::{DbError, DbResult};

pub struct VocabularyRepo;

impl VocabularyRepo {
    pub fn list_all(conn: &Connection) -> DbResult<Vec<VocabularyEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries
             ORDER BY priority DESC, length(find_text) DESC"
        )?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn list_enabled(conn: &Connection) -> DbResult<Vec<VocabularyEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries
             WHERE enabled = 1
             ORDER BY priority DESC, length(find_text) DESC"
        )?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn list_by_category(conn: &Connection, category: &VocabularyCategory) -> DbResult<Vec<VocabularyEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries
             WHERE category = ?1
             ORDER BY priority DESC, length(find_text) DESC"
        )?;
        let rows = stmt.query_map([category.as_str()], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn get_by_id(conn: &Connection, id: &Uuid) -> DbResult<VocabularyEntry> {
        conn.query_row(
            "SELECT id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at
             FROM vocabulary_entries WHERE id = ?1",
            [id.to_string()],
            Self::row_to_entry,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                DbError::NotFound(format!("vocabulary entry {id}"))
            }
            other => DbError::Sqlite(other),
        })
    }

    pub fn insert(conn: &Connection, entry: &VocabularyEntry) -> DbResult<()> {
        conn.execute(
            "INSERT INTO vocabulary_entries (id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry.id.to_string(),
                entry.find_text,
                entry.replacement,
                entry.category.as_str(),
                entry.case_sensitive as i32,
                entry.priority,
                entry.enabled as i32,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert(conn: &Connection, entry: &VocabularyEntry) -> DbResult<()> {
        conn.execute(
            "INSERT INTO vocabulary_entries (id, find_text, replacement, category, case_sensitive, priority, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(find_text) DO UPDATE SET
                 replacement = excluded.replacement,
                 category = excluded.category,
                 case_sensitive = excluded.case_sensitive,
                 priority = excluded.priority,
                 enabled = excluded.enabled,
                 updated_at = excluded.updated_at",
            rusqlite::params![
                entry.id.to_string(),
                entry.find_text,
                entry.replacement,
                entry.category.as_str(),
                entry.case_sensitive as i32,
                entry.priority,
                entry.enabled as i32,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn update(conn: &Connection, entry: &VocabularyEntry) -> DbResult<()> {
        let rows = conn.execute(
            "UPDATE vocabulary_entries SET find_text = ?1, replacement = ?2, category = ?3, case_sensitive = ?4, priority = ?5, enabled = ?6, updated_at = ?7
             WHERE id = ?8",
            rusqlite::params![
                entry.find_text,
                entry.replacement,
                entry.category.as_str(),
                entry.case_sensitive as i32,
                entry.priority,
                entry.enabled as i32,
                entry.updated_at.to_rfc3339(),
                entry.id.to_string(),
            ],
        )?;
        if rows == 0 {
            return Err(DbError::NotFound(format!("vocabulary entry {}", entry.id)));
        }
        Ok(())
    }

    pub fn delete(conn: &Connection, id: &Uuid) -> DbResult<()> {
        let rows = conn.execute(
            "DELETE FROM vocabulary_entries WHERE id = ?1",
            [id.to_string()],
        )?;
        if rows == 0 {
            return Err(DbError::NotFound(format!("vocabulary entry {id}")));
        }
        Ok(())
    }

    pub fn delete_all(conn: &Connection) -> DbResult<u32> {
        let rows = conn.execute("DELETE FROM vocabulary_entries", [])?;
        Ok(rows as u32)
    }

    pub fn count(conn: &Connection) -> DbResult<(u32, u32)> {
        let total: u32 = conn.query_row(
            "SELECT COUNT(*) FROM vocabulary_entries",
            [],
            |r| r.get(0),
        )?;
        let enabled: u32 = conn.query_row(
            "SELECT COUNT(*) FROM vocabulary_entries WHERE enabled = 1",
            [],
            |r| r.get(0),
        )?;
        Ok((total, enabled))
    }

    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<VocabularyEntry> {
        let id_str: String = row.get(0)?;
        let category_str: String = row.get(3)?;
        let case_sensitive_int: i32 = row.get(4)?;
        let enabled_int: i32 = row.get(6)?;
        let created_str: String = row.get(7)?;
        let updated_str: String = row.get(8)?;

        Ok(VocabularyEntry {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::nil()),
            find_text: row.get(1)?,
            replacement: row.get(2)?,
            category: VocabularyCategory::from_str(&category_str),
            case_sensitive: case_sensitive_int != 0,
            priority: row.get(5)?,
            enabled: enabled_int != 0,
            created_at: DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}
