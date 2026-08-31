//! Persistence layer: SQLite metadata, file tracking, incremental state.
//!
//! Provides a [`Store`] backed by SQLite for managing file records, chunks,
//! edge types, templates, and validation issues. Uses WAL mode for concurrency
//! and foreign keys for referential integrity.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use ctxvault_common::{Error, Result};

// ---------------------------------------------------------------------------
// Record types
// ---------------------------------------------------------------------------

/// A tracked file in the index.
#[derive(Debug, Clone)]
pub struct FileRecord {
    /// Relative path within the corpus.
    pub path: String,
    /// BLAKE3 content hash.
    pub content_hash: String,
    /// File modification time as Unix timestamp (seconds).
    pub modified_at: i64,
    /// Template declared in frontmatter.
    pub template: Option<String>,
    /// Document title.
    pub title: Option<String>,
    /// When this file was last indexed (Unix timestamp seconds).
    pub indexed_at: i64,
}

/// A text chunk stored for a file.
#[derive(Debug, Clone)]
pub struct ChunkRecord {
    /// Zero-based index within the parent document.
    pub chunk_index: usize,
    /// Byte offset of chunk start in original content.
    pub start_byte: usize,
    /// Byte offset of chunk end in original content.
    pub end_byte: usize,
    /// The text content of this chunk.
    pub text: String,
}

/// A registered edge type configuration persisted to the database.
#[derive(Debug, Clone)]
pub struct EdgeTypeRecord {
    /// Name of the edge type.
    pub name: String,
    /// Source kind (e.g. "wikilink", "tag", "frontmatter", "reference").
    pub source: String,
    /// Weight for scoring.
    pub weight: f32,
    /// Whether edges of this type are created in both directions.
    pub bidirectional: bool,
    /// Frontmatter field name (if source is frontmatter).
    pub field: Option<String>,
    /// Additional config serialized as JSON.
    pub config: Option<String>,
}

// ---------------------------------------------------------------------------
// SQL schema
// ---------------------------------------------------------------------------

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    modified_at INTEGER NOT NULL,
    template TEXT,
    title TEXT,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS edge_types (
    name TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    bidirectional INTEGER NOT NULL DEFAULT 0,
    field TEXT,
    config TEXT
);

CREATE TABLE IF NOT EXISTS templates (
    name TEXT PRIMARY KEY,
    definition TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS validation_issues (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    severity TEXT NOT NULL,
    message TEXT NOT NULL,
    field TEXT,
    checked_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS corpus_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// SQLite-backed persistence store for the ctxvault engine.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) a SQLite database at the given path and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| Error::Database(e.to_string()))?;
        Self::initialize(conn)
    }

    /// Open an in-memory database (useful for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| Error::Database(e.to_string()))?;
        Self::initialize(conn)
    }

    /// Common initialization: pragmas + schema.
    fn initialize(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|e| Error::Database(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| Error::Database(e.to_string()))?;
        conn.execute_batch(SCHEMA_SQL).map_err(|e| Error::Database(e.to_string()))?;
        Ok(Self { conn })
    }

    // ------------------------------------------------------------------
    // File tracking
    // ------------------------------------------------------------------

    /// Insert or replace a file record. Sets `indexed_at` to the current time.
    pub fn insert_file(
        &self,
        path: &str,
        content_hash: &str,
        modified_at: i64,
        template: Option<&str>,
        title: Option<&str>,
    ) -> Result<()> {
        let indexed_at = now_unix();
        let _ = self
            .conn
            .execute(
                "INSERT OR REPLACE INTO files (path, content_hash, modified_at, template, title, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![path, content_hash, modified_at, template, title, indexed_at],
            )
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve a single file record by path.
    pub fn get_file(&self, path: &str) -> Result<Option<FileRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path, content_hash, modified_at, template, title, indexed_at
                 FROM files WHERE path = ?1",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![path], |row| {
                Ok(FileRecord {
                    path: row.get(0)?,
                    content_hash: row.get(1)?,
                    modified_at: row.get(2)?,
                    template: row.get(3)?,
                    title: row.get(4)?,
                    indexed_at: row.get(5)?,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(Error::Database(e.to_string())),
            None => Ok(None),
        }
    }

    /// Delete a file record and its associated chunks/validation issues (via CASCADE).
    pub fn delete_file(&self, path: &str) -> Result<()> {
        let _ = self
            .conn
            .execute("DELETE FROM files WHERE path = ?1", params![path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// List all tracked files.
    pub fn list_files(&self) -> Result<Vec<FileRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path, content_hash, modified_at, template, title, indexed_at FROM files ORDER BY path",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(FileRecord {
                    path: row.get(0)?,
                    content_hash: row.get(1)?,
                    modified_at: row.get(2)?,
                    template: row.get(3)?,
                    title: row.get(4)?,
                    indexed_at: row.get(5)?,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    // ------------------------------------------------------------------
    // Chunks
    // ------------------------------------------------------------------

    /// Insert chunks for a file within a transaction.
    pub fn insert_chunks(&self, file_path: &str, chunks: &[ChunkRecord]) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(|e| Error::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO chunks (file_path, chunk_index, start_byte, end_byte, text)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for chunk in chunks {
                let _ = stmt
                    .execute(params![
                        file_path,
                        chunk.chunk_index as i64,
                        chunk.start_byte as i64,
                        chunk.end_byte as i64,
                        chunk.text,
                    ])
                    .map_err(|e| Error::Database(e.to_string()))?;
            }
        }

        tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve all chunks for a given file, ordered by chunk_index.
    pub fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<ChunkRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT chunk_index, start_byte, end_byte, text
                 FROM chunks WHERE file_path = ?1 ORDER BY chunk_index",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![file_path], |row| {
                Ok(ChunkRecord {
                    chunk_index: row.get::<_, i64>(0)? as usize,
                    start_byte: row.get::<_, i64>(1)? as usize,
                    end_byte: row.get::<_, i64>(2)? as usize,
                    text: row.get(3)?,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    /// Delete all chunks for a given file.
    pub fn delete_chunks_for_file(&self, file_path: &str) -> Result<()> {
        let _ = self
            .conn
            .execute("DELETE FROM chunks WHERE file_path = ?1", params![file_path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Edge types
    // ------------------------------------------------------------------

    /// Insert or replace edge type records within a transaction.
    pub fn insert_edge_types(&self, edge_types: &[EdgeTypeRecord]) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(|e| Error::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO edge_types (name, source, weight, bidirectional, field, config)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for et in edge_types {
                let _ = stmt
                    .execute(params![
                        et.name,
                        et.source,
                        et.weight as f64,
                        et.bidirectional as i32,
                        et.field,
                        et.config,
                    ])
                    .map_err(|e| Error::Database(e.to_string()))?;
            }
        }

        tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// List all registered edge types.
    pub fn list_edge_types(&self) -> Result<Vec<EdgeTypeRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, source, weight, bidirectional, field, config FROM edge_types ORDER BY name")
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(EdgeTypeRecord {
                    name: row.get(0)?,
                    source: row.get(1)?,
                    weight: row.get::<_, f64>(2)? as f32,
                    bidirectional: row.get::<_, i32>(3)? != 0,
                    field: row.get(4)?,
                    config: row.get(5)?,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    // ------------------------------------------------------------------
    // Corpus Config (key-value store for settings and audit trail)
    // ------------------------------------------------------------------

    /// Set a configuration value.
    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let updated_at = now_unix();
        let _ = self
            .conn
            .execute(
                "INSERT OR REPLACE INTO corpus_config (key, value, updated_at)
                 VALUES (?1, ?2, ?3)",
                params![key, value, updated_at],
            )
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Get a configuration value.
    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM corpus_config WHERE key = ?1")
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![key], |row| row.get(0))
            .map_err(|e| Error::Database(e.to_string()))?;

        match rows.next() {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(e)) => Err(Error::Database(e.to_string())),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current Unix timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_tables() {
        let store = Store::open_in_memory().expect("should open in-memory db");

        // Verify that all expected tables exist by querying sqlite_master.
        let tables: Vec<String> = {
            let mut stmt = store
                .conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };

        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"chunks".to_string()));
        assert!(tables.contains(&"edge_types".to_string()));
        assert!(tables.contains(&"templates".to_string()));
        assert!(tables.contains(&"validation_issues".to_string()));
    }

    #[test]
    fn file_crud() {
        let store = Store::open_in_memory().unwrap();

        // Insert
        store
            .insert_file("notes/hello.md", "abc123", 1700000000, Some("daily"), Some("Hello"))
            .unwrap();

        // Get
        let record = store.get_file("notes/hello.md").unwrap().expect("should find file");
        assert_eq!(record.path, "notes/hello.md");
        assert_eq!(record.content_hash, "abc123");
        assert_eq!(record.modified_at, 1700000000);
        assert_eq!(record.template.as_deref(), Some("daily"));
        assert_eq!(record.title.as_deref(), Some("Hello"));
        assert!(record.indexed_at > 0);

        // List
        store.insert_file("notes/world.md", "def456", 1700000001, None, None).unwrap();
        let files = store.list_files().unwrap();
        assert_eq!(files.len(), 2);

        // Delete
        store.delete_file("notes/hello.md").unwrap();
        assert!(store.get_file("notes/hello.md").unwrap().is_none());
        assert_eq!(store.list_files().unwrap().len(), 1);
    }

    #[test]
    fn chunk_storage_round_trip() {
        let store = Store::open_in_memory().unwrap();

        // Must have a parent file due to foreign key constraint.
        store.insert_file("doc.md", "hash1", 1700000000, None, None).unwrap();

        let chunks = vec![
            ChunkRecord {
                chunk_index: 0,
                start_byte: 0,
                end_byte: 100,
                text: "First chunk".to_string(),
            },
            ChunkRecord {
                chunk_index: 1,
                start_byte: 100,
                end_byte: 250,
                text: "Second chunk".to_string(),
            },
            ChunkRecord {
                chunk_index: 2,
                start_byte: 250,
                end_byte: 400,
                text: "Third chunk".to_string(),
            },
        ];

        store.insert_chunks("doc.md", &chunks).unwrap();

        let retrieved = store.get_chunks_for_file("doc.md").unwrap();
        assert_eq!(retrieved.len(), 3);
        assert_eq!(retrieved[0].chunk_index, 0);
        assert_eq!(retrieved[0].text, "First chunk");
        assert_eq!(retrieved[1].start_byte, 100);
        assert_eq!(retrieved[2].end_byte, 400);

        // Delete chunks
        store.delete_chunks_for_file("doc.md").unwrap();
        assert!(store.get_chunks_for_file("doc.md").unwrap().is_empty());
    }

    #[test]
    fn edge_type_storage_round_trip() {
        let store = Store::open_in_memory().unwrap();

        let edge_types = vec![
            EdgeTypeRecord {
                name: "Wikilink".to_string(),
                source: "wikilink".to_string(),
                weight: 1.0,
                bidirectional: false,
                field: None,
                config: None,
            },
            EdgeTypeRecord {
                name: "SharedTag".to_string(),
                source: "tag".to_string(),
                weight: 0.5,
                bidirectional: true,
                field: None,
                config: Some(r#"{"max_frequency": 100}"#.to_string()),
            },
            EdgeTypeRecord {
                name: "Implements".to_string(),
                source: "frontmatter".to_string(),
                weight: 0.8,
                bidirectional: false,
                field: Some("implements".to_string()),
                config: None,
            },
        ];

        store.insert_edge_types(&edge_types).unwrap();

        let retrieved = store.list_edge_types().unwrap();
        assert_eq!(retrieved.len(), 3);

        // Sorted by name: Implements, SharedTag, Wikilink
        assert_eq!(retrieved[0].name, "Implements");
        assert_eq!(retrieved[0].source, "frontmatter");
        assert_eq!(retrieved[0].field.as_deref(), Some("implements"));
        assert!(!retrieved[0].bidirectional);

        assert_eq!(retrieved[1].name, "SharedTag");
        assert!(retrieved[1].bidirectional);
        assert!((retrieved[1].weight - 0.5).abs() < f32::EPSILON);
        assert_eq!(retrieved[1].config.as_deref(), Some(r#"{"max_frequency": 100}"#));

        assert_eq!(retrieved[2].name, "Wikilink");
        assert!((retrieved[2].weight - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cascade_delete_removes_chunks() {
        let store = Store::open_in_memory().unwrap();

        store.insert_file("cascade.md", "h1", 1700000000, None, None).unwrap();
        store
            .insert_chunks(
                "cascade.md",
                &[ChunkRecord {
                    chunk_index: 0,
                    start_byte: 0,
                    end_byte: 50,
                    text: "chunk".to_string(),
                }],
            )
            .unwrap();

        // Deleting the file should cascade-delete its chunks.
        store.delete_file("cascade.md").unwrap();
        let chunks = store.get_chunks_for_file("cascade.md").unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn insert_file_upsert_on_conflict() {
        let store = Store::open_in_memory().unwrap();

        store.insert_file("upsert.md", "hash_v1", 1000, None, Some("Title v1")).unwrap();
        store.insert_file("upsert.md", "hash_v2", 2000, Some("note"), Some("Title v2")).unwrap();

        let record = store.get_file("upsert.md").unwrap().unwrap();
        assert_eq!(record.content_hash, "hash_v2");
        assert_eq!(record.modified_at, 2000);
        assert_eq!(record.template.as_deref(), Some("note"));
        assert_eq!(record.title.as_deref(), Some("Title v2"));

        // Should still be only one row.
        assert_eq!(store.list_files().unwrap().len(), 1);
    }
}
