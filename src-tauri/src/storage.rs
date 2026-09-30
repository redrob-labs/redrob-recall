use crate::models::{
    AppSettings, ChunkInput, ChunkRecord, DocumentRecord, IndexStats, IndexStatus,
};
use anyhow::{Context, Result};
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{
    params, Connection, Error as SqliteError, ErrorCode, OpenFlags, OptionalExtension, Transaction,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const SCHEMA_VERSION: i64 = 3;

/// The columns `chunks_fts` indexes, in order, with the bm25 weight each carries.
///
/// Declared together and in one place because SQLite cannot catch them drifting apart. Measured:
/// `bm25(chunks_fts, 1.0)` against a two-column table is accepted and silently weights the second
/// column 1.0, and `bm25(chunks_fts, 1.0, 1.0, 1.0)` against the same table is also accepted and
/// silently ignores the third weight. Neither errors, so a weight list written at a call site would
/// fall out of step with the column list invisibly.
///
/// `heading` carries 2.0 rather than 1.0. A heading is a short field, and bm25 already rewards a match
/// in a short field, so the weight is deliberately modest -- it says a heading match is a stronger
/// signal than a body match without letting a three-word heading outrank a paragraph that is genuinely
/// about the term.
const FTS_COLUMNS: &[(&str, f64)] = &[("content", 1.0), ("heading", 2.0)];

/// `content, heading` -- for the CREATE, the triggers and the column list of an INSERT.
fn fts_column_list() -> String {
    FTS_COLUMNS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `bm25(chunks_fts, 1, 2)` -- always as many weights as there are columns.
fn fts_bm25() -> String {
    let weights = FTS_COLUMNS
        .iter()
        .map(|(_, weight)| format!("{weight}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("bm25(chunks_fts, {weights})")
}
const BACKUP_RETENTION: usize = 3;

#[derive(Clone)]
pub struct Storage {
    connection: Arc<Mutex<Connection>>,
    path: Arc<PathBuf>,
}

/// How many tokens an FTS5 snippet carries around the match.
///
/// 40 rather than FTS5's 32 default: a snippet is what a person reads to decide whether a result is the
/// one they wanted, and what the answering model is given as evidence, so a little more surrounding
/// sentence is worth the characters. Capped by FTS5 itself at 64.
const SNIPPET_TOKENS: i64 = 40;

/// One keyword hit: the chunk, its rank, and the text around the match.
#[derive(Debug, Clone)]
pub struct KeywordHit {
    pub chunk_id: u64,
    pub score: f32,
    /// The window FTS5 selected around the matching terms, with `…` where it cut.
    ///
    /// Carried out of the search rather than recomputed later, because only the query that ran the
    /// MATCH knows where the match was. Before this, the excerpt was the whole chunk truncated from its
    /// FIRST character -- measured on a 1,117-character chunk whose match began at character 824, the
    /// 120-character excerpt handed to the answering model did not contain the searched term at all.
    pub snippet: String,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        let had_database =
            path.exists() && std::fs::metadata(path).is_ok_and(|meta| meta.len() > 0);
        let mut connection = Connection::open(path)
            .with_context(|| format!("failed to open metadata database at {}", path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(10))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        let current_version: i64 =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        anyhow::ensure!(
            current_version <= SCHEMA_VERSION,
            "this library was created by a newer version of Redrob Recall"
        );
        if had_database && current_version < SCHEMA_VERSION {
            create_backup_with_connection(&connection, path, "pre-migration")?;
        }
        migrate(&mut connection, current_version)?;
        verify_integrity(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Arc::new(path.to_path_buf()),
        })
    }

    pub fn load_settings(&self) -> Result<AppSettings> {
        let connection = self.connection.lock();
        let value: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'app_settings'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let settings = value
            .map(|raw| serde_json::from_str(&raw).context("invalid saved settings"))
            .unwrap_or_else(|| Ok(AppSettings::default()))?;
        settings.validate().context("saved settings are invalid")?;
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        settings.validate()?;
        let value = serde_json::to_string(settings)?;
        self.connection.lock().execute(
            "INSERT INTO settings(key, value) VALUES ('app_settings', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![value],
        )?;
        Ok(())
    }

    pub fn mark_all_vectors_pending(&self) -> Result<()> {
        self.connection.lock().execute(
            "UPDATE documents SET vector_state = 'pending' WHERE status = 'indexed'",
            [],
        )?;
        Ok(())
    }

    pub fn create_backup(&self) -> Result<PathBuf> {
        let connection = self.connection.lock();
        create_backup_with_connection(&connection, &self.path, "manual")
    }

    pub fn integrity_check(&self) -> Result<()> {
        verify_integrity(&self.connection.lock())
    }

    pub fn is_confirmed_corrupt(path: &Path) -> bool {
        match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(connection) => match connection
                .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
            {
                Ok(result) => result != "ok",
                Err(error) => is_corruption_error(&error),
            },
            Err(error) => is_corruption_error(&error),
        }
    }

    pub fn document_is_current(
        &self,
        path: &str,
        modified_at: &str,
        size_bytes: u64,
    ) -> Result<bool> {
        let connection = self.connection.lock();
        let current = connection
            .query_row(
                "SELECT modified_at = ?2 AND size_bytes = ?3 AND status = 'indexed' AND vector_state = 'ready' FROM documents WHERE path = ?1",
                params![path, modified_at, size_bytes],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        Ok(current)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn replace_document(
        &self,
        path: &str,
        name: &str,
        extension: &str,
        mime_type: &str,
        modified_at: &str,
        size_bytes: u64,
        content_hash: &str,
        chunks: &[ChunkInput],
    ) -> Result<(i64, Vec<u64>)> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let document_id = upsert_document(
            &transaction,
            path,
            name,
            extension,
            mime_type,
            modified_at,
            size_bytes,
            content_hash,
            chunks.len() as u64,
            "indexed",
            "pending",
            None,
        )?;
        transaction.execute(
            "DELETE FROM chunks WHERE document_id = ?1",
            params![document_id],
        )?;
        let mut chunk_ids = Vec::with_capacity(chunks.len());
        for (index, chunk) in chunks.iter().enumerate() {
            transaction.execute(
                "INSERT INTO chunks(document_id, chunk_index, content, page, heading) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![document_id, index as u32, chunk.content, chunk.page, chunk.heading],
            )?;
            chunk_ids.push(transaction.last_insert_rowid() as u64);
        }
        transaction.commit()?;
        Ok((document_id, chunk_ids))
    }

    pub fn mark_vectors_ready(&self, path: &str) -> Result<()> {
        let changed = self.connection.lock().execute(
            "UPDATE documents SET vector_state = 'ready' WHERE path = ?1 AND status = 'indexed'",
            params![path],
        )?;
        anyhow::ensure!(
            changed == 1,
            "indexed document disappeared before vector commit"
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mark_document_failed(
        &self,
        path: &str,
        name: &str,
        extension: &str,
        mime_type: &str,
        modified_at: &str,
        size_bytes: u64,
        error: &str,
    ) -> Result<()> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let document_id = upsert_document(
            &transaction,
            path,
            name,
            extension,
            mime_type,
            modified_at,
            size_bytes,
            "",
            0,
            "failed",
            "failed",
            Some(error),
        )?;
        transaction.execute(
            "DELETE FROM chunks WHERE document_id = ?1",
            params![document_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get_chunk(&self, id: u64) -> Result<Option<ChunkRecord>> {
        let connection = self.connection.lock();
        connection
            .query_row(
                r#"SELECT c.id, c.document_id, c.chunk_index, c.content, c.page, c.heading,
                           d.path, d.name, d.extension, d.modified_at
                    FROM chunks c JOIN documents d ON d.id = c.document_id WHERE c.id = ?1"#,
                params![id],
                map_chunk,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_chunks(&self, ids: &[u64]) -> Result<Vec<ChunkRecord>> {
        ids.iter()
            .filter_map(|id| self.get_chunk(*id).transpose())
            .collect()
    }

    pub fn chunk_ids_for_path(&self, path: &str) -> Result<Vec<u64>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT c.id FROM chunks c JOIN documents d ON d.id = c.document_id WHERE d.path = ?1",
        )?;
        let rows = statement.query_map(params![path], |row| row.get::<_, u64>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Keyword search over the FTS5 index.
    ///
    /// The query goes through `query::to_match_expression`, which parses the boolean operators,
    /// brackets, phrases, field filters and exclusions a person types and compiles them to an FTS5
    /// expression. Before that this function stripped every non-alphanumeric character, quoted each
    /// remaining word and joined them with `OR`: a phrase was torn into its words, an exclusion was
    /// silently dropped, and `heading:policy` searched for the literal word "heading".
    ///
    /// A malformed query is an ERROR rather than an empty result, and deliberately so -- "a quotation
    /// mark is not closed" is something a person can act on, where zero results looks like the corpus
    /// simply has nothing.
    pub fn keyword_search(&self, query: &str, limit: usize) -> Result<Vec<KeywordHit>> {
        let columns: Vec<&str> = FTS_COLUMNS.iter().map(|(name, _)| *name).collect();
        let expression = crate::query::to_match_expression(query, &columns)?;
        let connection = self.connection.lock();
        // The weights come from FTS_COLUMNS rather than being written here, because a literal list at
        // this call site is exactly what would drift when a column is added -- and SQLite accepts a
        // wrong-length weight list without complaining.
        let bm25 = fts_bm25();
        // The snippet is selected HERE, in the query that ran the MATCH, because that is the only place
        // that knows where the match was. Column -1 lets FTS5 pick whichever indexed column matched, so
        // a heading hit is quoted from the heading rather than from unrelated body text.
        let mut statement = connection.prepare(&format!(
            "SELECT rowid, {bm25}, snippet(chunks_fts, -1, '', '', '…', {SNIPPET_TOKENS})
             FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY {bm25} LIMIT ?2"
        ))?;
        let rows = statement.query_map(params![expression, limit as u64], |row| {
            let chunk_id: u64 = row.get(0)?;
            let rank: f64 = row.get(1)?;
            let snippet: String = row.get(2).unwrap_or_default();
            Ok(KeywordHit {
                chunk_id,
                score: (1.0 / (1.0 + rank.abs())) as f32,
                snippet,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_documents(&self, limit: usize, offset: usize) -> Result<Vec<DocumentRecord>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            r#"SELECT id, path, name, extension, mime_type, modified_at, size_bytes,
                      status, chunk_count, last_error
               FROM documents ORDER BY indexed_at DESC LIMIT ?1 OFFSET ?2"#,
        )?;
        let rows = statement.query_map(params![limit as u64, offset as u64], |row| {
            Ok(DocumentRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
                extension: row.get(3)?,
                mime_type: row.get(4)?,
                modified_at: row.get(5)?,
                size_bytes: row.get(6)?,
                status: row.get(7)?,
                chunk_count: row.get(8)?,
                last_error: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn stats(&self, status: IndexStatus, current_file: Option<String>) -> Result<IndexStats> {
        let connection = self.connection.lock();
        let (documents, chunks, failed, total_bytes): (u64, u64, u64, u64) = connection.query_row(
            r#"SELECT COUNT(*), COALESCE(SUM(chunk_count), 0),
                      COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
                      COALESCE(SUM(size_bytes), 0) FROM documents"#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let last_indexed_at = connection
            .query_row("SELECT MAX(indexed_at) FROM documents", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();
        Ok(IndexStats {
            documents,
            indexed_chunks: chunks,
            pending: 0,
            failed,
            total_bytes,
            status,
            current_file,
            last_indexed_at,
        })
    }

    pub fn remove_missing_documents(&self, existing_paths: &[String]) -> Result<Vec<u64>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare("SELECT path FROM documents")?;
        let stored = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let existing = existing_paths
            .iter()
            .collect::<std::collections::HashSet<_>>();
        let mut removed_chunk_ids = Vec::new();
        for path in stored {
            if !existing.contains(&path) {
                let mut chunk_statement = connection.prepare(
                    "SELECT c.id FROM chunks c JOIN documents d ON d.id = c.document_id WHERE d.path = ?1",
                )?;
                removed_chunk_ids.extend(
                    chunk_statement
                        .query_map(params![path], |row| row.get::<_, u64>(0))?
                        .collect::<Result<Vec<_>, _>>()?,
                );
                drop(chunk_statement);
                connection.execute("DELETE FROM documents WHERE path = ?1", params![path])?;
            }
        }
        Ok(removed_chunk_ids)
    }

    pub fn clear(&self) -> Result<()> {
        self.connection.lock().execute_batch(
            "DELETE FROM chunks; DELETE FROM documents; INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild'); VACUUM;",
        )?;
        Ok(())
    }
}

fn is_corruption_error(error: &SqliteError) -> bool {
    matches!(
        error,
        SqliteError::SqliteFailure(details, _)
            if matches!(details.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

fn migrate(connection: &mut Connection, previous_version: i64) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS documents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            extension TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            modified_at TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'indexed',
            vector_state TEXT NOT NULL DEFAULT 'ready',
            chunk_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            indexed_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
            chunk_index INTEGER NOT NULL,
            content TEXT NOT NULL,
            page INTEGER,
            heading TEXT,
            UNIQUE(document_id, chunk_index)
        );

        CREATE INDEX IF NOT EXISTS idx_documents_status ON documents(status);
        CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);
        "#,
    )?;
    if !has_column(&transaction, "documents", "vector_state")? {
        transaction.execute(
            "ALTER TABLE documents ADD COLUMN vector_state TEXT NOT NULL DEFAULT 'ready'",
            [],
        )?;
    }
    ensure_fts_schema(&transaction)?;
    if previous_version < SCHEMA_VERSION {
        transaction.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')", [])?;
    }
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

/// Create `chunks_fts` and its triggers from `FTS_COLUMNS`, replacing an index built on a different
/// column set.
///
/// A virtual table cannot be altered -- measured: SQLite answers `virtual tables may not be altered` --
/// so widening the index means dropping and recreating it. That is lossless here and only here, because
/// this is an EXTERNAL CONTENT table: every indexed word lives in `chunks`, and `rebuild` reads it back
/// from there. Doing the same to an ordinary FTS5 table would destroy the only copy.
///
/// The existing column set is read from the table rather than inferred from the schema version, so a
/// database left half-migrated by an interrupted run is repaired rather than trusted.
fn ensure_fts_schema(connection: &Connection) -> Result<()> {
    let columns = fts_column_list();
    let existing = fts_existing_columns(connection)?;
    let wanted: Vec<String> = FTS_COLUMNS
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    if existing.as_deref() == Some(wanted.as_slice()) {
        return Ok(());
    }

    let values = FTS_COLUMNS
        .iter()
        .map(|(name, _)| format!("new.{name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let old_values = FTS_COLUMNS
        .iter()
        .map(|(name, _)| format!("old.{name}"))
        .collect::<Vec<_>>()
        .join(", ");

    connection.execute_batch(&format!(
        r#"
        DROP TRIGGER IF EXISTS chunks_ai;
        DROP TRIGGER IF EXISTS chunks_ad;
        DROP TRIGGER IF EXISTS chunks_au;
        DROP TABLE IF EXISTS chunks_fts;

        CREATE VIRTUAL TABLE chunks_fts USING fts5(
            {columns},
            content='chunks',
            content_rowid='id',
            tokenize='unicode61 remove_diacritics 2'
        );

        CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
            INSERT INTO chunks_fts(rowid, {columns}) VALUES (new.id, {values});
        END;
        CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, {columns}) VALUES ('delete', old.id, {old_values});
        END;
        CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, {columns}) VALUES ('delete', old.id, {old_values});
            INSERT INTO chunks_fts(rowid, {columns}) VALUES (new.id, {values});
        END;

        INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild');
        "#
    ))?;
    Ok(())
}

/// The columns an existing `chunks_fts` indexes, or `None` when there is no such table.
fn fts_existing_columns(connection: &Connection) -> Result<Option<Vec<String>>> {
    let present: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'chunks_fts'",
        [],
        |row| row.get(0),
    )?;
    if present == 0 {
        return Ok(None);
    }
    // PRAGMA table_info works on an FTS5 table and lists its indexed columns in order.
    let mut statement = connection.prepare("PRAGMA table_info(chunks_fts)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(columns))
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|candidate| candidate == column))
}

fn verify_integrity(connection: &Connection) -> Result<()> {
    let result: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    anyhow::ensure!(
        result == "ok",
        "local metadata database failed integrity check"
    );
    let foreign_key_errors: u64 =
        connection.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    anyhow::ensure!(
        foreign_key_errors == 0,
        "local metadata database has invalid document references"
    );
    Ok(())
}

fn create_backup_with_connection(
    connection: &Connection,
    database_path: &Path,
    reason: &str,
) -> Result<PathBuf> {
    connection.execute_batch("PRAGMA wal_checkpoint(FULL);")?;
    let backup_dir = database_path
        .parent()
        .context("metadata database has no parent directory")?
        .join("backups");
    std::fs::create_dir_all(&backup_dir)?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let destination = backup_dir.join(format!("metadata-{timestamp}-{}-{reason}.db", &unique[..8]));
    connection.execute(
        "VACUUM INTO ?1",
        params![destination.to_string_lossy().to_string()],
    )?;

    let mut backups = std::fs::read_dir(&backup_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("metadata-") && name.ends_with(".db"))
        })
        .collect::<Vec<_>>();
    backups.sort();
    let remove_count = backups.len().saturating_sub(BACKUP_RETENTION);
    for stale in backups.into_iter().take(remove_count) {
        let _ = std::fs::remove_file(stale);
    }
    Ok(destination)
}

#[allow(clippy::too_many_arguments)]
fn upsert_document(
    transaction: &Transaction<'_>,
    path: &str,
    name: &str,
    extension: &str,
    mime_type: &str,
    modified_at: &str,
    size_bytes: u64,
    content_hash: &str,
    chunk_count: u64,
    status: &str,
    vector_state: &str,
    error: Option<&str>,
) -> Result<i64> {
    transaction.execute(
        r#"INSERT INTO documents(path, name, extension, mime_type, modified_at, size_bytes,
                                  content_hash, status, vector_state, chunk_count, last_error, indexed_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
           ON CONFLICT(path) DO UPDATE SET
             name=excluded.name, extension=excluded.extension, mime_type=excluded.mime_type,
             modified_at=excluded.modified_at, size_bytes=excluded.size_bytes,
             content_hash=excluded.content_hash, status=excluded.status,
             vector_state=excluded.vector_state, chunk_count=excluded.chunk_count,
             last_error=excluded.last_error, indexed_at=excluded.indexed_at"#,
        params![
            path,
            name,
            extension,
            mime_type,
            modified_at,
            size_bytes,
            content_hash,
            status,
            vector_state,
            chunk_count,
            error,
            Utc::now().to_rfc3339(),
        ],
    )?;
    let id = transaction.query_row(
        "SELECT id FROM documents WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )?;
    Ok(id)
}

fn map_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChunkRecord> {
    Ok(ChunkRecord {
        id: row.get(0)?,
        document_id: row.get(1)?,
        chunk_index: row.get(2)?,
        content: row.get(3)?,
        page: row.get(4)?,
        heading: row.get(5)?,
        path: row.get(6)?,
        name: row.get(7)?,
        extension: row.get(8)?,
        modified_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{Storage, SCHEMA_VERSION};
    use crate::models::AppSettings;
    use rusqlite::Connection;

    fn temporary_database(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "redrob-recall-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory.join("metadata.db")
    }

    #[test]
    fn new_database_is_migrated_and_can_be_backed_up() {
        let path = temporary_database("new");
        let storage = Storage::open(&path).unwrap();
        storage.save_settings(&AppSettings::default()).unwrap();
        storage.integrity_check().unwrap();
        let backup = storage.create_backup().unwrap();
        assert!(backup.is_file());
        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn legacy_database_gains_vector_commit_state() {
        let path = temporary_database("legacy");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE documents (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    path TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    extension TEXT NOT NULL,
                    mime_type TEXT NOT NULL,
                    modified_at TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    content_hash TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'indexed',
                    chunk_count INTEGER NOT NULL DEFAULT 0,
                    last_error TEXT,
                    indexed_at TEXT NOT NULL
                );",
            )
            .unwrap();
        drop(connection);

        let storage = Storage::open(&path).unwrap();
        let connection = storage.connection.lock();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let has_vector_state: bool = connection
            .prepare("PRAGMA table_info(documents)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .any(|column| column.is_ok_and(|column| column == "vector_state"));
        assert!(has_vector_state);
        drop(connection);
        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// The bug this schema change fixes, as a test that fails on the old index.
    ///
    /// `chunks.heading` existed and was never indexed, so a term appearing only in a heading could not
    /// be found. Measured before the change: a chunk headed "Revenue recognition policy" whose body
    /// never says "policy" did not come back for "policy".
    #[test]
    fn a_term_in_a_heading_is_searchable() {
        let path = temporary_database("heading-search");
        let storage = Storage::open(&path).unwrap();
        seed_one_document(&storage);

        let hits = storage.keyword_search("policy", 10).unwrap();
        assert_eq!(hits.len(), 2, "both the heading match and the body match");

        let heading_only = storage.keyword_search("quarterly", 10).unwrap();
        assert_eq!(
            heading_only.len(),
            1,
            "a body-only term still matches only its chunk"
        );

        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// A database indexed on the old column set is widened, and nothing is lost doing it.
    ///
    /// The index cannot be altered -- SQLite refuses with `virtual tables may not be altered` -- so the
    /// migration drops and recreates it. That is only safe because this is an external-content table:
    /// every indexed word lives in `chunks`, and `rebuild` reads it back. This asserts both halves --
    /// the new column works AND the previously indexed content is still findable.
    #[test]
    fn a_legacy_single_column_index_is_widened_without_losing_content() {
        let path = temporary_database("fts-widen");

        // Build the pre-change schema by hand, including its one-column index.
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE documents (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    path TEXT NOT NULL UNIQUE, name TEXT NOT NULL, extension TEXT NOT NULL,
                    mime_type TEXT NOT NULL, modified_at TEXT NOT NULL, size_bytes INTEGER NOT NULL,
                    content_hash TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'indexed',
                    chunk_count INTEGER NOT NULL DEFAULT 0, last_error TEXT, indexed_at TEXT NOT NULL
                );
                CREATE TABLE chunks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                    chunk_index INTEGER NOT NULL, content TEXT NOT NULL, page INTEGER, heading TEXT,
                    UNIQUE(document_id, chunk_index)
                );
                CREATE VIRTUAL TABLE chunks_fts USING fts5(
                    content, content='chunks', content_rowid='id',
                    tokenize='unicode61 remove_diacritics 2');
                CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
                    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
                END;
                INSERT INTO documents (path, name, extension, mime_type, modified_at, size_bytes,
                                       content_hash, indexed_at)
                VALUES ('/docs/handbook.md', 'handbook.md', 'md', 'text/markdown',
                        '2026-01-01T00:00:00Z', 10, 'hash', '2026-01-01T00:00:00Z');
                INSERT INTO chunks (document_id, chunk_index, content, heading) VALUES
                    (1, 0, 'The figures were reviewed and approved by the committee.',
                        'Revenue recognition policy');",
            )
            .unwrap();
        connection
            .pragma_update(None, "user_version", 2_i64)
            .unwrap();
        drop(connection);

        // Before opening: the heading term is invisible, which is the state being migrated away from.
        let legacy = Connection::open(&path).unwrap();
        let before: i64 = legacy
            .query_row(
                "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'policy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, 0, "the old index cannot find a heading-only term");
        drop(legacy);

        let storage = Storage::open(&path).unwrap();
        {
            let connection = storage.connection.lock();
            let version: i64 = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap();
            assert_eq!(version, SCHEMA_VERSION);

            let mut statement = connection.prepare("PRAGMA table_info(chunks_fts)").unwrap();
            let columns: Vec<String> = statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(columns, vec!["content", "heading"]);
        }

        // The heading is now findable, AND the content that was already indexed still is. The second
        // assertion is the one that would catch a migration that recreated the index without rebuilding.
        assert_eq!(storage.keyword_search("policy", 10).unwrap().len(), 1);
        assert_eq!(storage.keyword_search("committee", 10).unwrap().len(), 1);

        storage.integrity_check().unwrap();
        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// Opening twice must not rebuild the index a second time.
    ///
    /// The schema is reconciled against the table's actual columns rather than the version number, so
    /// that an interrupted migration is repaired; the risk in that choice is doing the work every time,
    /// which on a large corpus is an expensive no-op.
    #[test]
    fn opening_an_already_migrated_database_leaves_the_index_alone() {
        let path = temporary_database("fts-idempotent");
        let storage = Storage::open(&path).unwrap();
        seed_one_document(&storage);
        drop(storage);

        let reopened = Storage::open(&path).unwrap();
        assert_eq!(reopened.keyword_search("policy", 10).unwrap().len(), 2);
        {
            let connection = reopened.connection.lock();
            // A rebuild would have reset the triggers too; assert all three survived.
            let triggers: i64 = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name IN ('chunks_ai', 'chunks_ad', 'chunks_au')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(triggers, 3);
        }
        drop(reopened);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// The bm25 weight list always has one weight per indexed column.
    ///
    /// Measured: SQLite accepts a weight list that is too short AND one that is too long, silently. So
    /// nothing but this check stands between a future column and a wrong ranking.
    #[test]
    fn every_indexed_column_has_a_bm25_weight() {
        let expression = super::fts_bm25();
        let weights = expression.trim_end_matches(')').split(',').skip(1).count();
        assert_eq!(weights, super::FTS_COLUMNS.len());
        assert_eq!(super::fts_column_list(), "content, heading");
    }

    /// The query language reaching the index through the real search method.
    ///
    /// query.rs proves the compiler against a bare FTS5 table. This proves the product's own path uses
    /// it -- the compiler existed and was unreachable until `keyword_search` called it.
    #[test]
    fn the_query_language_reaches_the_index() {
        let path = temporary_database("query-language");
        let storage = Storage::open(&path).unwrap();
        seed_one_document(&storage);

        let found = |query: &str| storage.keyword_search(query, 10).unwrap().len();

        // Two words mean BOTH, not either. The old implementation ORed them, so this returned 2.
        assert_eq!(found("revenue committee"), 1, "only chunk 0 has both");
        assert_eq!(
            found("revenue OR committee"),
            2,
            "either, when asked for either"
        );

        // A phrase stays whole. The old implementation split it into ORed words.
        assert_eq!(found("\"quarterly figures\""), 1);
        assert_eq!(
            found("\"figures quarterly\""),
            0,
            "order matters in a phrase"
        );

        // A field filter scopes, where the old implementation searched for the word "heading".
        assert_eq!(found("heading:appendix"), 1);
        assert_eq!(
            found("content:appendix"),
            0,
            "appendix is a heading, not body text"
        );

        // An exclusion excludes, where the old implementation dropped it.
        assert_eq!(
            found("recognition -appendix"),
            1,
            "the Appendix chunk is removed"
        );

        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// A malformed query is an error carrying a sentence, not an empty result set.
    #[test]
    fn a_malformed_query_is_reported_rather_than_answered_with_nothing() {
        let path = temporary_database("query-malformed");
        let storage = Storage::open(&path).unwrap();
        seed_one_document(&storage);

        // Zero results would look like the corpus having nothing; the message says what to fix.
        let error = storage
            .keyword_search("\"unclosed", 10)
            .unwrap_err()
            .to_string();
        assert!(error.contains("quotation mark"), "got {error}");

        let unknown = storage
            .keyword_search("author:someone", 10)
            .unwrap_err()
            .to_string();
        assert!(
            unknown.contains("content, heading"),
            "names the searchable fields: {unknown}"
        );

        // And an unknown field never reaches SQLite, which would answer with its own error naming the
        // column -- the searchable field list comes from FTS_COLUMNS, so the two cannot disagree.
        assert!(
            unknown.contains("author"),
            "names what was wrong: {unknown}"
        );

        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// The snippet is the text AROUND the match, not the head of the chunk.
    ///
    /// The defect this replaces, measured: the excerpt was the whole chunk and the answer context budget
    /// truncated it from the first character, so on a long chunk whose match came late the model was
    /// handed source text that did not contain the searched term and asked to answer from it.
    #[test]
    fn a_snippet_is_taken_from_around_the_match() {
        let path = temporary_database("snippet");
        let storage = Storage::open(&path).unwrap();
        {
            let connection = storage.connection.lock();
            let filler = "Filler sentence about unrelated matters. ".repeat(20);
            let tail = "More filler afterwards. ".repeat(10);
            let content =
                format!("{filler}The revenue recognition policy applies to subscriptions. {tail}");
            assert!(
                content.find("revenue").unwrap() > 700,
                "the match must be far from the start for this test to mean anything"
            );
            connection
                .execute(
                    "INSERT INTO documents (path, name, extension, mime_type, modified_at,
                                            size_bytes, content_hash, indexed_at)
                     VALUES ('/d.md', 'd.md', 'md', 'text/markdown', '2026-01-01T00:00:00Z', 10,
                             'h', '2026-01-01T00:00:00Z')",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO chunks (document_id, chunk_index, content, heading)
                     VALUES (1, 0, ?1, 'Intro')",
                    [&content],
                )
                .unwrap();
        }

        let hits = storage.keyword_search("revenue", 5).unwrap();
        assert_eq!(hits.len(), 1);
        let snippet = &hits[0].snippet;

        assert!(
            snippet.contains("revenue"),
            "the snippet must contain what was searched for, got {snippet:?}"
        );
        assert!(
            snippet.len() < 600,
            "the snippet must be a window, not the chunk: {} characters",
            snippet.len()
        );
        assert!(
            snippet.contains('…'),
            "and must mark where it cut: {snippet:?}"
        );

        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// A heading match is quoted from the heading, not from unrelated body text.
    #[test]
    fn a_heading_match_is_quoted_from_the_heading() {
        let path = temporary_database("snippet-heading");
        let storage = Storage::open(&path).unwrap();
        seed_one_document(&storage);

        // "Appendix" appears only in the second chunk's heading.
        let hits = storage.keyword_search("appendix", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].snippet.to_lowercase().contains("appendix"),
            "column -1 lets FTS5 quote the column that matched: {:?}",
            hits[0].snippet
        );

        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// A function word must not turn an AND into an exclusion of real matches.
    ///
    /// Measured: with AND semantics, `the revenue` required both terms, so a chunk reading "revenue
    /// recognition applies..." was excluded for lacking "the". bm25 needs no help ranking common terms --
    /// it scored "the" at -1.6e-6 against "revenue"'s -4.29 -- so this is about what MATCHES, not order.
    #[test]
    fn a_function_word_does_not_exclude_real_matches() {
        let path = temporary_database("stopwords");
        let storage = Storage::open(&path).unwrap();
        seed_one_document(&storage);

        let with = storage.keyword_search("the recognition", 10).unwrap().len();
        let without = storage.keyword_search("recognition", 10).unwrap().len();
        assert_eq!(with, without, "the leading function word changed nothing");

        // But a phrase is exact, and rewriting it would answer a different question. The fixture's
        // second chunk reads "per the policy", so the phrase IS there -- and the reversed phrase is not,
        // which is what proves the function word survived rather than being quietly dropped.
        assert_eq!(
            storage.keyword_search("\"the policy\"", 10).unwrap().len(),
            1,
            "the exact phrase is present"
        );
        assert_eq!(
            storage.keyword_search("\"policy the\"", 10).unwrap().len(),
            0,
            "reversed it is not, so the phrase was matched in order with its function word intact"
        );

        drop(storage);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// Insert one document whose heading carries a term its body does not.
    fn seed_one_document(storage: &Storage) {
        let connection = storage.connection.lock();
        connection
            .execute_batch(
                "INSERT INTO documents (path, name, extension, mime_type, modified_at, size_bytes,
                                        content_hash, indexed_at)
                 VALUES ('/docs/handbook.md', 'handbook.md', 'md', 'text/markdown',
                         '2026-01-01T00:00:00Z', 10, 'hash', '2026-01-01T00:00:00Z');
                 INSERT INTO chunks (document_id, chunk_index, content, heading) VALUES
                    (1, 0, 'The quarterly figures were reviewed and approved by the committee.',
                        'Revenue recognition policy'),
                    (1, 1, 'Recognition of revenue happens when control transfers, per the policy.',
                        'Appendix');",
            )
            .unwrap();
    }
}
