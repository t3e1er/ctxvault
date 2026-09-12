//! Persistence layer: SQLite metadata, file tracking, incremental state.
//!
//! Provides a [`Store`] backed by SQLite for managing file records, chunks,
//! edge types, templates, and validation issues. Uses WAL mode for concurrency
//! and foreign keys for referential integrity.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use ctxvault_common::types::{
    ChunkRecord, EdgeRecord, EdgeTypeRecord, FileRecord, GraphAffordances, IndexingState,
    IndexingStatus,
};
use ctxvault_common::{Error, Result};

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
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chunks_file_chunk ON chunks(file_path, chunk_index);
CREATE INDEX IF NOT EXISTS idx_chunks_file_covering ON chunks(file_path, chunk_index, start_line, end_line, start_byte, end_byte);

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

CREATE TABLE IF NOT EXISTS indexing_state (
    corpus_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    total_files INTEGER NOT NULL DEFAULT 0,
    indexed_files INTEGER NOT NULL DEFAULT 0,
    last_processed_path TEXT,
    started_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0,
    error_message TEXT
);

CREATE TABLE IF NOT EXISTS code_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    name TEXT NOT NULL,
    scope_path TEXT NOT NULL,
    symbol_type TEXT NOT NULL,
    language TEXT NOT NULL,
    signature TEXT NOT NULL,
    docstring TEXT,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_code_symbols_name ON code_symbols(name);
CREATE INDEX IF NOT EXISTS idx_code_symbols_file ON code_symbols(file_path);
CREATE INDEX IF NOT EXISTS idx_code_symbols_scope ON code_symbols(scope_path);
CREATE INDEX IF NOT EXISTS idx_code_symbols_file_covering ON code_symbols(file_path, scope_path, symbol_type, start_line, end_line);

CREATE TABLE IF NOT EXISTS edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    edge_class TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    confidence REAL NOT NULL DEFAULT 1.0,
    metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_edges_source_type ON edges(source, edge_type);
CREATE INDEX IF NOT EXISTS idx_edges_target_type ON edges(target, edge_type);
CREATE INDEX IF NOT EXISTS idx_edges_composite ON edges(source, edge_type, target);
CREATE INDEX IF NOT EXISTS idx_edges_class_source ON edges(edge_class, source);
CREATE INDEX IF NOT EXISTS idx_edges_class_target ON edges(edge_class, target);
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);

CREATE TABLE IF NOT EXISTS external_refs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    caller_scope_path TEXT NOT NULL,
    raw_target TEXT NOT NULL,
    kind TEXT NOT NULL,
    confidence TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_external_refs_file ON external_refs(file_path);
CREATE INDEX IF NOT EXISTS idx_external_refs_target ON external_refs(raw_target);
"#;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// SQLite-backed persistence store for the ctxvault engine.
pub struct Store {
    conn: std::sync::Mutex<Connection>,
}

impl Store {
    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("store connection mutex poisoned")
    }

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
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 10000;
            PRAGMA mmap_size = 268435456;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -65536;
            PRAGMA synchronous = NORMAL;
            "#,
        )
        .map_err(|e| Error::Database(e.to_string()))?;
        conn.execute_batch(SCHEMA_SQL).map_err(|e| Error::Database(e.to_string()))?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    /// Checkpoint the SQLite WAL journal.
    pub fn checkpoint(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
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
            .conn()
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
        let conn = self.conn();
        let mut stmt = conn
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
            .conn()
            .execute("DELETE FROM files WHERE path = ?1", params![path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// List all tracked files.
    pub fn list_files(&self) -> Result<Vec<FileRecord>> {
        let conn = self.conn();
        let mut stmt = conn
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
        let conn = self.conn();
        let tx = conn.unchecked_transaction().map_err(|e| Error::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO chunks (file_path, chunk_index, start_byte, end_byte, start_line, end_line)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for chunk in chunks {
                let _ = stmt
                    .execute(params![
                        file_path,
                        chunk.chunk_index as i64,
                        chunk.start_byte as i64,
                        chunk.end_byte as i64,
                        chunk.start_line as i64,
                        chunk.end_line as i64,
                    ])
                    .map_err(|e| Error::Database(e.to_string()))?;
            }
        }

        tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve all chunks for a given file, ordered by chunk_index.
    pub fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<ChunkRecord>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT chunk_index, start_byte, end_byte, start_line, end_line
                 FROM chunks WHERE file_path = ?1 ORDER BY chunk_index",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![file_path], |row| {
                Ok(ChunkRecord {
                    chunk_index: row.get::<_, i64>(0)? as usize,
                    start_byte: row.get::<_, i64>(1)? as usize,
                    end_byte: row.get::<_, i64>(2)? as usize,
                    start_line: row.get::<_, i64>(3)? as usize,
                    end_line: row.get::<_, i64>(4)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    /// Retrieve a single chunk for a given file and chunk index.
    pub fn get_chunk(&self, file_path: &str, chunk_index: usize) -> Result<Option<ChunkRecord>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT chunk_index, start_byte, end_byte, start_line, end_line
                 FROM chunks WHERE file_path = ?1 AND chunk_index = ?2",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![file_path, chunk_index as i64], |row| {
                Ok(ChunkRecord {
                    chunk_index: row.get::<_, i64>(0)? as usize,
                    start_byte: row.get::<_, i64>(1)? as usize,
                    end_byte: row.get::<_, i64>(2)? as usize,
                    start_line: row.get::<_, i64>(3)? as usize,
                    end_line: row.get::<_, i64>(4)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        match rows.next() {
            Some(res) => res.map(Some).map_err(|e| Error::Database(e.to_string())),
            None => Ok(None),
        }
    }

    /// Delete all chunks for a given file.
    pub fn delete_chunks_for_file(&self, file_path: &str) -> Result<()> {
        let _ = self
            .conn()
            .execute("DELETE FROM chunks WHERE file_path = ?1", params![file_path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Edge types
    // ------------------------------------------------------------------

    /// Insert or replace edge type records within a transaction.
    pub fn insert_edge_types(&self, edge_types: &[EdgeTypeRecord]) -> Result<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction().map_err(|e| Error::Database(e.to_string()))?;

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
        let conn = self.conn();
        let mut stmt = conn
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
    // Relational Edges
    // ------------------------------------------------------------------

    /// Insert a batch of relational edges within a single transaction.
    pub fn insert_edges(&self, edges: &[EdgeRecord]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| Error::Database(e.to_string()))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO edges (source, target, edge_type, edge_class, weight, confidence, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for edge in edges {
                stmt.execute(params![
                    edge.source,
                    edge.target,
                    edge.edge_type,
                    edge.edge_class,
                    edge.weight,
                    edge.confidence,
                    edge.metadata,
                ])
                .map_err(|e| Error::Database(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Delete all edges where the given path is source or target.
    pub fn delete_edges_for_node(&self, path: &str) -> Result<()> {
        let _ = self
            .conn()
            .execute("DELETE FROM edges WHERE source = ?1 OR target = ?1", params![path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve all incident edges (where node is source or target).
    pub fn get_edges_for_node(&self, path: &str) -> Result<Vec<EdgeRecord>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, source, target, edge_type, edge_class, weight, confidence, metadata
                 FROM edges WHERE source = ?1 OR target = ?1",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![path], |row| {
                Ok(EdgeRecord {
                    id: Some(row.get(0)?),
                    source: row.get(1)?,
                    target: row.get(2)?,
                    edge_type: row.get(3)?,
                    edge_class: row.get(4)?,
                    weight: row.get(5)?,
                    confidence: row.get(6)?,
                    metadata: row.get(7)?,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    /// Retrieve degree affordance counts for a node using indexed SQLite aggregation.
    pub fn get_degree_counts(&self, node: &str) -> Result<GraphAffordances> {
        let conn = self.conn();

        let mut out_stmt = conn
            .prepare("SELECT edge_type, COUNT(*) FROM edges WHERE source = ?1 GROUP BY edge_type")
            .map_err(|e| Error::Database(e.to_string()))?;
        let out_rows = out_stmt
            .query_map(params![node], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut in_stmt = conn
            .prepare("SELECT edge_type, COUNT(*) FROM edges WHERE target = ?1 GROUP BY edge_type")
            .map_err(|e| Error::Database(e.to_string()))?;
        let in_rows = in_stmt
            .query_map(params![node], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut affordances = GraphAffordances::default();

        for item in out_rows {
            let (edge_type, count) = item.map_err(|e| Error::Database(e.to_string()))?;
            match edge_type.as_str() {
                "calls" => affordances.calls_out = Some(count),
                "implements" => affordances.implements = Some(count),
                "imports" => affordances.imports = Some(count),
                "wikilink" => affordances.wikilinks_out = Some(count),
                "documents" => affordances.documents_code = Some(count),
                _ => {}
            }
        }

        for item in in_rows {
            let (edge_type, count) = item.map_err(|e| Error::Database(e.to_string()))?;
            match edge_type.as_str() {
                "calls" => affordances.calls_in = Some(count),
                "wikilink" => affordances.wikilinks_in = Some(count),
                "documents" => {
                    let existing = affordances.documents_code.unwrap_or(0);
                    affordances.documents_code = Some(existing + count);
                }
                _ => {}
            }
        }

        Ok(affordances)
    }

    /// Remove all edges from the database.
    pub fn clear_all_edges(&self) -> Result<()> {
        let _ = self
            .conn()
            .execute("DELETE FROM edges", [])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Corpus Config (key-value store for settings and audit trail)
    // ------------------------------------------------------------------

    /// Set a configuration value.
    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let updated_at = now_unix();
        let _ = self
            .conn()
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
        let conn = self.conn();
        let mut stmt = conn
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

    // ------------------------------------------------------------------
    // Indexing State Tracking (Resumable Paginated Indexing)
    // ------------------------------------------------------------------

    /// Retrieve the current indexing state for a corpus.
    pub fn get_indexing_state(&self, corpus_id: &str) -> Result<Option<IndexingState>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT corpus_id, status, total_files, indexed_files, last_processed_path, started_at, updated_at, error_message
                 FROM indexing_state WHERE corpus_id = ?1",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![corpus_id], |row| {
                let status_str: String = row.get(1)?;
                let status = status_str.parse::<IndexingStatus>().unwrap_or(IndexingStatus::Idle);
                Ok(IndexingState {
                    corpus_id: row.get(0)?,
                    status,
                    total_files: row.get::<_, i64>(2)? as usize,
                    indexed_files: row.get::<_, i64>(3)? as usize,
                    last_processed_path: row.get(4)?,
                    started_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    error_message: row.get(7)?,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        match rows.next() {
            Some(Ok(state)) => Ok(Some(state)),
            Some(Err(e)) => Err(Error::Database(e.to_string())),
            None => Ok(None),
        }
    }

    /// Insert or update the indexing state for a corpus.
    pub fn update_indexing_state(&self, state: &IndexingState) -> Result<()> {
        let status_str = state.status.to_string();
        let _ = self
            .conn()
            .execute(
                "INSERT OR REPLACE INTO indexing_state (corpus_id, status, total_files, indexed_files, last_processed_path, started_at, updated_at, error_message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    state.corpus_id,
                    status_str,
                    state.total_files as i64,
                    state.indexed_files as i64,
                    state.last_processed_path,
                    state.started_at,
                    state.updated_at,
                    state.error_message,
                ],
            )
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Reset or delete the indexing state for a corpus.
    pub fn reset_indexing_state(&self, corpus_id: &str) -> Result<()> {
        let _ = self
            .conn()
            .execute("DELETE FROM indexing_state WHERE corpus_id = ?1", params![corpus_id])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Code Symbols
    // ------------------------------------------------------------------

    /// Save code symbols extracted from a file. Replaces any existing symbols for the file.
    pub fn save_code_symbols(
        &self,
        file_path: &str,
        symbols: &[ctxvault_common::types::CodeSymbol],
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| Error::Database(e.to_string()))?;

        // Delete existing symbols for this file
        tx.execute("DELETE FROM code_symbols WHERE file_path = ?1", params![file_path])
            .map_err(|e| Error::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO code_symbols (file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for sym in symbols {
                let type_str = serde_json::to_string(&sym.symbol_type)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string();
                stmt.execute(params![
                    sym.file_path,
                    sym.name,
                    sym.scope_path,
                    type_str,
                    sym.language,
                    sym.signature,
                    sym.docstring,
                    sym.start_line as i64,
                    sym.end_line as i64,
                ])
                .map_err(|e| Error::Database(e.to_string()))?;
            }
        }

        tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve all code symbols defined in a given file.
    pub fn get_code_symbols_for_file(
        &self,
        file_path: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line
                 FROM code_symbols WHERE file_path = ?1 ORDER BY start_line",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![file_path], |row| {
                let type_str: String = row.get(3)?;
                let symbol_type: ctxvault_common::types::CodeSymbolType =
                    serde_json::from_str(&format!("\"{type_str}\""))
                        .unwrap_or(ctxvault_common::types::CodeSymbolType::Function);
                Ok(ctxvault_common::types::CodeSymbol {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    scope_path: row.get(2)?,
                    symbol_type,
                    language: row.get(4)?,
                    signature: row.get(5)?,
                    docstring: row.get(6)?,
                    start_line: row.get::<_, i64>(7)? as usize,
                    end_line: row.get::<_, i64>(8)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    /// Retrieve all code symbols defined across a batch of files in a single batch query.
    pub fn get_code_symbols_for_files(
        &self,
        file_paths: &[&str],
    ) -> Result<std::collections::HashMap<String, Vec<ctxvault_common::types::CodeSymbol>>> {
        let mut map: std::collections::HashMap<String, Vec<ctxvault_common::types::CodeSymbol>> =
            std::collections::HashMap::with_capacity(file_paths.len());
        if file_paths.is_empty() {
            return Ok(map);
        }

        let conn = self.conn();
        for chunk in file_paths.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line
                 FROM code_symbols WHERE file_path IN ({placeholders}) ORDER BY file_path, start_line"
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| Error::Database(e.to_string()))?;
            let params_vec: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            let rows = stmt
                .query_map(params_vec.as_slice(), |row| {
                    let type_str: String = row.get(3)?;
                    let symbol_type: ctxvault_common::types::CodeSymbolType =
                        serde_json::from_str(&format!("\"{type_str}\""))
                            .unwrap_or(ctxvault_common::types::CodeSymbolType::Function);
                    Ok(ctxvault_common::types::CodeSymbol {
                        file_path: row.get(0)?,
                        name: row.get(1)?,
                        scope_path: row.get(2)?,
                        symbol_type,
                        language: row.get(4)?,
                        signature: row.get(5)?,
                        docstring: row.get(6)?,
                        start_line: row.get::<_, i64>(7)? as usize,
                        end_line: row.get::<_, i64>(8)? as usize,
                    })
                })
                .map_err(|e| Error::Database(e.to_string()))?;

            for sym in rows {
                let s = sym.map_err(|e| Error::Database(e.to_string()))?;
                map.entry(s.file_path.clone()).or_default().push(s);
            }
        }

        Ok(map)
    }

    /// Find code symbols matching a name pattern.
    pub fn find_symbols_by_name(
        &self,
        name_pattern: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line
                 FROM code_symbols WHERE name LIKE ?1 OR scope_path LIKE ?1 ORDER BY name",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let search_pattern = format!("%{name_pattern}%");
        let rows = stmt
            .query_map(params![search_pattern], |row| {
                let type_str: String = row.get(3)?;
                let symbol_type: ctxvault_common::types::CodeSymbolType =
                    serde_json::from_str(&format!("\"{type_str}\""))
                        .unwrap_or(ctxvault_common::types::CodeSymbolType::Function);
                Ok(ctxvault_common::types::CodeSymbol {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    scope_path: row.get(2)?,
                    symbol_type,
                    language: row.get(4)?,
                    signature: row.get(5)?,
                    docstring: row.get(6)?,
                    start_line: row.get::<_, i64>(7)? as usize,
                    end_line: row.get::<_, i64>(8)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    /// Find code symbols whose fully qualified `scope_path` matches exactly.
    ///
    /// Unlike [`Store::find_symbols_by_name`] (which does a fuzzy `LIKE`), this
    /// Look up code symbols with an exact matching scope_path.
    ///
    /// If no exact matches are found, falls back to normalized scope path resolution
    /// (ignoring generic type/lifetime parameters).
    pub fn find_symbols_by_qualified_name(
        &self,
        scope_path: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line
                 FROM code_symbols WHERE scope_path = ?1 ORDER BY file_path, start_line",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![scope_path], |row| {
                let type_str: String = row.get(3)?;
                let symbol_type: ctxvault_common::types::CodeSymbolType =
                    serde_json::from_str(&format!("\"{type_str}\""))
                        .unwrap_or(ctxvault_common::types::CodeSymbolType::Function);
                Ok(ctxvault_common::types::CodeSymbol {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    scope_path: row.get(2)?,
                    symbol_type,
                    language: row.get(4)?,
                    signature: row.get(5)?,
                    docstring: row.get(6)?,
                    start_line: row.get::<_, i64>(7)? as usize,
                    end_line: row.get::<_, i64>(8)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        let exact_matches: Vec<ctxvault_common::types::CodeSymbol> = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(e.to_string()))?;

        drop(stmt);
        drop(conn);

        if !exact_matches.is_empty() {
            return Ok(exact_matches);
        }

        self.find_symbols_by_normalized_scope(scope_path)
    }

    /// Look up code symbols matching the given scope path ignoring generic type/lifetime parameters.
    pub fn find_symbols_by_normalized_scope(
        &self,
        scope_path: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        let norm_query = crate::parser::code::normalize_scope_path(scope_path);
        let leaf = norm_query.split(" > ").last().unwrap_or(&norm_query).trim();

        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line
                 FROM code_symbols WHERE name = ?1 ORDER BY file_path, start_line",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![leaf], |row| {
                let type_str: String = row.get(3)?;
                let symbol_type: ctxvault_common::types::CodeSymbolType =
                    serde_json::from_str(&format!("\"{type_str}\""))
                        .unwrap_or(ctxvault_common::types::CodeSymbolType::Function);
                Ok(ctxvault_common::types::CodeSymbol {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    scope_path: row.get(2)?,
                    symbol_type,
                    language: row.get(4)?,
                    signature: row.get(5)?,
                    docstring: row.get(6)?,
                    start_line: row.get::<_, i64>(7)? as usize,
                    end_line: row.get::<_, i64>(8)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        let candidates: Vec<ctxvault_common::types::CodeSymbol> = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(e.to_string()))?;

        let matched: Vec<ctxvault_common::types::CodeSymbol> = candidates
            .into_iter()
            .filter(|sym| {
                let norm_candidate = crate::parser::code::normalize_scope_path(&sym.scope_path);
                crate::parser::code::scope_matches(&norm_candidate, &norm_query)
            })
            .collect();

        Ok(matched)
    }

    /// Retrieve all code symbols in the entire store.
    pub fn get_all_code_symbols(&self) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT file_path, name, scope_path, symbol_type, language, signature, docstring, start_line, end_line
                 FROM code_symbols ORDER BY file_path, start_line",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let type_str: String = row.get(3)?;
                let symbol_type: ctxvault_common::types::CodeSymbolType =
                    serde_json::from_str(&format!("\"{type_str}\""))
                        .unwrap_or(ctxvault_common::types::CodeSymbolType::Function);
                Ok(ctxvault_common::types::CodeSymbol {
                    file_path: row.get(0)?,
                    name: row.get(1)?,
                    scope_path: row.get(2)?,
                    symbol_type,
                    language: row.get(4)?,
                    signature: row.get(5)?,
                    docstring: row.get(6)?,
                    start_line: row.get::<_, i64>(7)? as usize,
                    end_line: row.get::<_, i64>(8)? as usize,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    // ------------------------------------------------------------------
    // External references (unresolved call/import targets)
    // ------------------------------------------------------------------

    /// Delete all external references captured for a file.
    ///
    /// Called before re-inserting the fresh set on re-index so that
    /// re-indexing a file is idempotent (mirrors [`Store::delete_edges_for_node`]).
    pub fn clear_external_refs_for_file(&self, file_path: &str) -> Result<()> {
        let _ = self
            .conn()
            .execute("DELETE FROM external_refs WHERE file_path = ?1", params![file_path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Insert a batch of external references for a file within a single transaction.
    pub fn insert_external_refs(
        &self,
        file_path: &str,
        refs: &[ctxvault_common::types::ExternalRef],
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| Error::Database(e.to_string()))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO external_refs (file_path, caller_scope_path, raw_target, kind, confidence)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for r in refs {
                stmt.execute(params![
                    file_path,
                    r.caller_scope_path,
                    r.raw_target,
                    external_ref_kind_to_str(r.kind),
                    resolution_confidence_to_str(r.confidence),
                ])
                .map_err(|e| Error::Database(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve every external reference in the store.
    pub fn get_external_refs(&self) -> Result<Vec<ctxvault_common::types::ExternalRef>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT caller_scope_path, raw_target, kind, confidence FROM external_refs ORDER BY id",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], external_ref_from_row)
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }

    /// Retrieve the external references captured for a single file.
    pub fn get_external_refs_for_file(
        &self,
        file_path: &str,
    ) -> Result<Vec<ctxvault_common::types::ExternalRef>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT caller_scope_path, raw_target, kind, confidence
                 FROM external_refs WHERE file_path = ?1 ORDER BY id",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![file_path], external_ref_from_row)
            .map_err(|e| Error::Database(e.to_string()))?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| Error::Database(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Port adapter: MetadataCatalog
// ---------------------------------------------------------------------------

impl ctxvault_common::ports::MetadataCatalog for Store {
    fn insert_file(
        &self,
        path: &str,
        content_hash: &str,
        modified_at: i64,
        template: Option<&str>,
        title: Option<&str>,
    ) -> Result<()> {
        Store::insert_file(self, path, content_hash, modified_at, template, title)
    }

    fn get_file(&self, path: &str) -> Result<Option<FileRecord>> {
        Store::get_file(self, path)
    }

    fn delete_file(&self, path: &str) -> Result<()> {
        Store::delete_file(self, path)
    }

    fn list_files(&self) -> Result<Vec<FileRecord>> {
        Store::list_files(self)
    }

    fn insert_chunks(&self, file_path: &str, chunks: &[ChunkRecord]) -> Result<()> {
        Store::insert_chunks(self, file_path, chunks)
    }

    fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<ChunkRecord>> {
        Store::get_chunks_for_file(self, file_path)
    }

    fn get_chunk(&self, file_path: &str, chunk_index: usize) -> Result<Option<ChunkRecord>> {
        Store::get_chunk(self, file_path, chunk_index)
    }

    fn delete_chunks_for_file(&self, file_path: &str) -> Result<()> {
        Store::delete_chunks_for_file(self, file_path)
    }

    fn insert_edge_types(&self, edge_types: &[EdgeTypeRecord]) -> Result<()> {
        Store::insert_edge_types(self, edge_types)
    }

    fn list_edge_types(&self) -> Result<Vec<EdgeTypeRecord>> {
        Store::list_edge_types(self)
    }

    fn insert_edges(&self, edges: &[EdgeRecord]) -> Result<()> {
        Store::insert_edges(self, edges)
    }

    fn delete_edges_for_node(&self, path: &str) -> Result<()> {
        Store::delete_edges_for_node(self, path)
    }

    fn get_edges_for_node(&self, path: &str) -> Result<Vec<EdgeRecord>> {
        Store::get_edges_for_node(self, path)
    }

    fn get_degree_counts(&self, node: &str) -> Result<GraphAffordances> {
        Store::get_degree_counts(self, node)
    }

    fn clear_all_edges(&self) -> Result<()> {
        Store::clear_all_edges(self)
    }

    fn set_config(&self, key: &str, value: &str) -> Result<()> {
        Store::set_config(self, key, value)
    }

    fn get_config(&self, key: &str) -> Result<Option<String>> {
        Store::get_config(self, key)
    }

    fn get_indexing_state(&self, corpus_id: &str) -> Result<Option<IndexingState>> {
        Store::get_indexing_state(self, corpus_id)
    }

    fn update_indexing_state(&self, state: &IndexingState) -> Result<()> {
        Store::update_indexing_state(self, state)
    }

    fn reset_indexing_state(&self, corpus_id: &str) -> Result<()> {
        Store::reset_indexing_state(self, corpus_id)
    }

    fn save_code_symbols(
        &self,
        file_path: &str,
        symbols: &[ctxvault_common::types::CodeSymbol],
    ) -> Result<()> {
        Store::save_code_symbols(self, file_path, symbols)
    }

    fn get_code_symbols_for_file(
        &self,
        file_path: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        Store::get_code_symbols_for_file(self, file_path)
    }

    fn get_code_symbols_for_files(
        &self,
        file_paths: &[&str],
    ) -> Result<std::collections::HashMap<String, Vec<ctxvault_common::types::CodeSymbol>>> {
        Store::get_code_symbols_for_files(self, file_paths)
    }

    fn find_symbols_by_name(
        &self,
        name_pattern: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        Store::find_symbols_by_name(self, name_pattern)
    }

    fn find_symbols_by_qualified_name(
        &self,
        scope_path: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        Store::find_symbols_by_qualified_name(self, scope_path)
    }

    fn find_symbols_by_normalized_scope(
        &self,
        scope_path: &str,
    ) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        Store::find_symbols_by_normalized_scope(self, scope_path)
    }

    fn get_all_code_symbols(&self) -> Result<Vec<ctxvault_common::types::CodeSymbol>> {
        Store::get_all_code_symbols(self)
    }

    fn clear_external_refs_for_file(&self, file_path: &str) -> Result<()> {
        Store::clear_external_refs_for_file(self, file_path)
    }

    fn insert_external_refs(
        &self,
        file_path: &str,
        refs: &[ctxvault_common::types::ExternalRef],
    ) -> Result<()> {
        Store::insert_external_refs(self, file_path, refs)
    }

    fn get_external_refs(&self) -> Result<Vec<ctxvault_common::types::ExternalRef>> {
        Store::get_external_refs(self)
    }

    fn get_external_refs_for_file(
        &self,
        file_path: &str,
    ) -> Result<Vec<ctxvault_common::types::ExternalRef>> {
        Store::get_external_refs_for_file(self, file_path)
    }

    fn checkpoint(&self) -> Result<()> {
        Store::checkpoint(self)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current Unix timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// Persisted string tag for an [`ExternalRefKind`](ctxvault_common::types::ExternalRefKind).
fn external_ref_kind_to_str(kind: ctxvault_common::types::ExternalRefKind) -> &'static str {
    use ctxvault_common::types::ExternalRefKind;
    match kind {
        ExternalRefKind::Call => "call",
        ExternalRefKind::Import => "import",
    }
}

/// Parse a persisted external-reference kind tag, defaulting to `Call`.
fn external_ref_kind_from_str(s: &str) -> ctxvault_common::types::ExternalRefKind {
    use ctxvault_common::types::ExternalRefKind;
    match s {
        "import" => ExternalRefKind::Import,
        _ => ExternalRefKind::Call,
    }
}

/// Persisted string tag for a [`ResolutionConfidence`](ctxvault_common::types::ResolutionConfidence).
fn resolution_confidence_to_str(
    confidence: ctxvault_common::types::ResolutionConfidence,
) -> &'static str {
    match confidence {
        ctxvault_common::types::ResolutionConfidence::High => "high",
        ctxvault_common::types::ResolutionConfidence::Medium => "medium",
        ctxvault_common::types::ResolutionConfidence::Speculative => "speculative",
    }
}

/// Parse a persisted resolution-confidence tag, defaulting to `Speculative`.
fn resolution_confidence_from_str(s: &str) -> ctxvault_common::types::ResolutionConfidence {
    match s {
        "high" => ctxvault_common::types::ResolutionConfidence::High,
        "medium" => ctxvault_common::types::ResolutionConfidence::Medium,
        _ => ctxvault_common::types::ResolutionConfidence::Speculative,
    }
}

/// Map an `external_refs` row (`caller_scope_path, raw_target, kind, confidence`)
/// to a domain [`ExternalRef`](ctxvault_common::types::ExternalRef).
fn external_ref_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ctxvault_common::types::ExternalRef> {
    let kind_str: String = row.get(2)?;
    let conf_str: String = row.get(3)?;
    Ok(ctxvault_common::types::ExternalRef {
        caller_scope_path: row.get(0)?,
        raw_target: row.get(1)?,
        kind: external_ref_kind_from_str(&kind_str),
        confidence: resolution_confidence_from_str(&conf_str),
    })
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
            let conn = store.conn();
            let mut stmt = conn
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
                start_line: 1,
                end_line: 5,
            },
            ChunkRecord {
                chunk_index: 1,
                start_byte: 100,
                end_byte: 250,
                start_line: 6,
                end_line: 12,
            },
            ChunkRecord {
                chunk_index: 2,
                start_byte: 250,
                end_byte: 400,
                start_line: 13,
                end_line: 20,
            },
        ];

        store.insert_chunks("doc.md", &chunks).unwrap();

        let retrieved = store.get_chunks_for_file("doc.md").unwrap();
        assert_eq!(retrieved.len(), 3);
        assert_eq!(retrieved[0].chunk_index, 0);
        assert_eq!(retrieved[0].start_line, 1);
        assert_eq!(retrieved[0].end_line, 5);
        assert_eq!(retrieved[1].start_byte, 100);
        assert_eq!(retrieved[2].end_byte, 400);

        let single = store.get_chunk("doc.md", 1).unwrap().unwrap();
        assert_eq!(single.chunk_index, 1);
        assert_eq!(single.start_byte, 100);
        assert_eq!(single.end_byte, 250);

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
                    start_line: 1,
                    end_line: 3,
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

    #[test]
    fn test_indexing_state_round_trip() {
        let store = Store::open_in_memory().unwrap();

        // Initially None
        assert!(store.get_indexing_state("corpus_a").unwrap().is_none());

        let state = IndexingState {
            corpus_id: "corpus_a".to_string(),
            status: IndexingStatus::Indexing,
            total_files: 100,
            indexed_files: 45,
            last_processed_path: Some("docs/intro.md".to_string()),
            started_at: 1700000000,
            updated_at: 1700000050,
            error_message: None,
        };

        store.update_indexing_state(&state).unwrap();

        let retrieved = store.get_indexing_state("corpus_a").unwrap().expect("should find state");
        assert_eq!(retrieved.corpus_id, "corpus_a");
        assert_eq!(retrieved.status, IndexingStatus::Indexing);
        assert_eq!(retrieved.total_files, 100);
        assert_eq!(retrieved.indexed_files, 45);
        assert_eq!(retrieved.last_processed_path.as_deref(), Some("docs/intro.md"));
        assert_eq!(retrieved.started_at, 1700000000);
        assert_eq!(retrieved.updated_at, 1700000050);
        assert!(retrieved.error_message.is_none());

        // Update to Completed
        let mut completed = state.clone();
        completed.status = IndexingStatus::Completed;
        completed.indexed_files = 100;
        completed.updated_at = 1700000100;
        store.update_indexing_state(&completed).unwrap();

        let updated = store.get_indexing_state("corpus_a").unwrap().unwrap();
        assert_eq!(updated.status, IndexingStatus::Completed);
        assert_eq!(updated.indexed_files, 100);

        // Reset
        store.reset_indexing_state("corpus_a").unwrap();
        assert!(store.get_indexing_state("corpus_a").unwrap().is_none());
    }

    #[test]
    fn test_find_symbols_by_normalized_scope() {
        let store = Store::open_in_memory().unwrap();

        let sym1 = ctxvault_common::types::CodeSymbol {
            file_path: "binder.rs".to_string(),
            name: "instantiate".to_string(),
            scope_path: "EarlyBinder<'tcx, T> > instantiate".to_string(),
            symbol_type: ctxvault_common::types::CodeSymbolType::Function,
            language: "rust".to_string(),
            signature: "pub fn instantiate(&self) -> T".to_string(),
            docstring: None,
            start_line: 10,
            end_line: 20,
        };
        let sym2 = ctxvault_common::types::CodeSymbol {
            file_path: "binder.rs".to_string(),
            name: "peek".to_string(),
            scope_path: "EarlyBinder<'tcx, T> > peek".to_string(),
            symbol_type: ctxvault_common::types::CodeSymbolType::Function,
            language: "rust".to_string(),
            signature: "pub fn peek(&self)".to_string(),
            docstring: None,
            start_line: 22,
            end_line: 30,
        };
        let sym3 = ctxvault_common::types::CodeSymbol {
            file_path: "other.rs".to_string(),
            name: "instantiate".to_string(),
            scope_path: "OtherBinder<'a> > instantiate".to_string(),
            symbol_type: ctxvault_common::types::CodeSymbolType::Function,
            language: "rust".to_string(),
            signature: "pub fn instantiate(&self)".to_string(),
            docstring: None,
            start_line: 5,
            end_line: 15,
        };

        store.insert_file("binder.rs", "hash1", 1000, None, None).unwrap();
        store.insert_file("other.rs", "hash2", 1000, None, None).unwrap();
        store.insert_file("binder2.rs", "hash3", 1000, None, None).unwrap();

        store.save_code_symbols("binder.rs", &[sym1, sym2]).unwrap();
        store.save_code_symbols("other.rs", &[sym3]).unwrap();

        // 1. Exact match works
        let exact =
            store.find_symbols_by_qualified_name("EarlyBinder<'tcx, T> > instantiate").unwrap();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].scope_path, "EarlyBinder<'tcx, T> > instantiate");

        // 2. Normalized scope query resolves generic scope path
        let normalized = store.find_symbols_by_qualified_name("EarlyBinder > instantiate").unwrap();
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].scope_path, "EarlyBinder<'tcx, T> > instantiate");

        // Direct call to find_symbols_by_normalized_scope
        let direct_norm =
            store.find_symbols_by_normalized_scope("EarlyBinder > instantiate").unwrap();
        assert_eq!(direct_norm.len(), 1);
        assert_eq!(direct_norm[0].scope_path, "EarlyBinder<'tcx, T> > instantiate");

        // 3. Nonexistent returns empty vec
        let missing = store.find_symbols_by_qualified_name("Nonexistent > instantiate").unwrap();
        assert!(missing.is_empty());

        // 4. Ambiguous methods across different types with same normalized name
        let sym4 = ctxvault_common::types::CodeSymbol {
            file_path: "binder2.rs".to_string(),
            name: "instantiate".to_string(),
            scope_path: "EarlyBinder<'a, A> > instantiate".to_string(),
            symbol_type: ctxvault_common::types::CodeSymbolType::Function,
            language: "rust".to_string(),
            signature: "pub fn instantiate(&self) -> A".to_string(),
            docstring: None,
            start_line: 1,
            end_line: 10,
        };
        store.save_code_symbols("binder2.rs", &[sym4]).unwrap();

        let ambiguous = store.find_symbols_by_qualified_name("EarlyBinder > instantiate").unwrap();
        assert_eq!(ambiguous.len(), 2, "should return both candidates for disambiguation");
        assert!(ambiguous.iter().any(|s| s.scope_path == "EarlyBinder<'tcx, T> > instantiate"));
        assert!(ambiguous.iter().any(|s| s.scope_path == "EarlyBinder<'a, A> > instantiate"));
    }

    #[test]
    fn test_edges_table_crud_and_degrees() {
        let store = Store::open_in_memory().unwrap();

        let edges = vec![
            EdgeRecord {
                id: None,
                source: "file_a.rs > func_a".to_string(),
                target: "file_b.rs > func_b".to_string(),
                edge_type: "calls".to_string(),
                edge_class: "structural".to_string(),
                weight: 1.0,
                confidence: 1.0,
                metadata: None,
            },
            EdgeRecord {
                id: None,
                source: "file_c.rs > func_c".to_string(),
                target: "file_b.rs > func_b".to_string(),
                edge_type: "calls".to_string(),
                edge_class: "structural".to_string(),
                weight: 1.0,
                confidence: 1.0,
                metadata: None,
            },
            EdgeRecord {
                id: None,
                source: "file_b.rs > func_b".to_string(),
                target: "interface.rs > TraitB".to_string(),
                edge_type: "implements".to_string(),
                edge_class: "structural".to_string(),
                weight: 1.0,
                confidence: 1.0,
                metadata: None,
            },
            EdgeRecord {
                id: None,
                source: "file_b.rs > func_b".to_string(),
                target: "dep.rs".to_string(),
                edge_type: "imports".to_string(),
                edge_class: "structural".to_string(),
                weight: 1.0,
                confidence: 1.0,
                metadata: None,
            },
            EdgeRecord {
                id: None,
                source: "docs/spec.md".to_string(),
                target: "file_b.rs > func_b".to_string(),
                edge_type: "documents".to_string(),
                edge_class: "hybrid".to_string(),
                weight: 1.0,
                confidence: 1.0,
                metadata: None,
            },
        ];

        store.insert_edges(&edges).unwrap();

        let b_edges = store.get_edges_for_node("file_b.rs > func_b").unwrap();
        assert_eq!(b_edges.len(), 5);

        let deg = store.get_degree_counts("file_b.rs > func_b").unwrap();
        assert_eq!(deg.calls_in, Some(2));
        assert_eq!(deg.implements, Some(1));
        assert_eq!(deg.imports, Some(1));
        assert_eq!(deg.documents_code, Some(1));
        assert_eq!(deg.calls_out, None);

        // Delete edges for node
        store.delete_edges_for_node("file_b.rs > func_b").unwrap();
        let remaining = store.get_edges_for_node("file_b.rs > func_b").unwrap();
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_external_refs_round_trip_and_idempotent() {
        use ctxvault_common::types::{ExternalRef, ExternalRefKind, ResolutionConfidence};

        let store = Store::open_in_memory().unwrap();

        let refs = vec![
            ExternalRef {
                caller_scope_path: "src/a.rs > run".to_string(),
                raw_target: "external_crate::do_thing".to_string(),
                kind: ExternalRefKind::Call,
                confidence: ResolutionConfidence::Speculative,
            },
            ExternalRef {
                caller_scope_path: "src/a.rs".to_string(),
                raw_target: "serde::Serialize".to_string(),
                kind: ExternalRefKind::Import,
                confidence: ResolutionConfidence::Speculative,
            },
        ];

        store.insert_external_refs("src/a.rs", &refs).unwrap();

        // Round-trip: all rows read back, kind/confidence preserved.
        let all = store.get_external_refs().unwrap();
        assert_eq!(all.len(), 2);
        let call = all.iter().find(|r| r.kind == ExternalRefKind::Call).unwrap();
        assert_eq!(call.raw_target, "external_crate::do_thing");
        assert_eq!(call.confidence, ResolutionConfidence::Speculative);
        let import = all.iter().find(|r| r.kind == ExternalRefKind::Import).unwrap();
        assert_eq!(import.raw_target, "serde::Serialize");

        // Per-file read.
        assert_eq!(store.get_external_refs_for_file("src/a.rs").unwrap().len(), 2);

        // Idempotent re-index cycle (clear-then-insert) does not duplicate rows.
        store.clear_external_refs_for_file("src/a.rs").unwrap();
        store.insert_external_refs("src/a.rs", &refs).unwrap();
        assert_eq!(store.get_external_refs().unwrap().len(), 2);

        // Clearing one file leaves it empty.
        store.clear_external_refs_for_file("src/a.rs").unwrap();
        assert!(store.get_external_refs().unwrap().is_empty());
    }
}
