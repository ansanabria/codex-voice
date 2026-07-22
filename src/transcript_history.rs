use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptEntry {
    pub id: i64,
    pub created_at: i64,
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDocument {
    schema_version: u8,
    entries: Vec<TranscriptEntry>,
    has_more: bool,
}

pub(crate) struct InsertedTranscript {
    database: PathBuf,
    id: i64,
}

impl InsertedTranscript {
    pub(crate) fn rollback(&self) -> io::Result<()> {
        open_at(&self.database)?
            .execute("DELETE FROM transcripts WHERE id = ?1", [self.id])
            .map_err(sqlite_error)?;
        Ok(())
    }
}

fn database_path() -> io::Result<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(base.join("codex-voice/transcripts.sqlite3"))
}

fn sqlite_error(error: rusqlite::Error) -> io::Error {
    io::Error::other(format!("transcript history database: {error}"))
}

fn open_at(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    let connection = Connection::open(path).map_err(sqlite_error)?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS transcripts (
               id INTEGER PRIMARY KEY,
               created_at INTEGER NOT NULL,
               text TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS transcripts_newest
               ON transcripts(created_at DESC, id DESC);",
        )
        .map_err(sqlite_error)?;
    Ok(connection)
}

fn open() -> io::Result<Connection> {
    let path = database_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    open_at(&path)
}

pub(crate) fn add(text: &str) -> io::Result<InsertedTranscript> {
    let database = database_path()?;
    insert(open()?, database, text)
}

#[cfg(test)]
fn insert_at(database: PathBuf, text: &str) -> io::Result<InsertedTranscript> {
    let connection = open_at(&database)?;
    insert(connection, database, text)
}

fn insert(connection: Connection, database: PathBuf, text: &str) -> io::Result<InsertedTranscript> {
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis() as i64;
    connection
        .execute(
            "INSERT INTO transcripts (created_at, text) VALUES (?1, ?2)",
            params![created_at, text],
        )
        .map_err(sqlite_error)?;
    Ok(InsertedTranscript {
        database,
        id: connection.last_insert_rowid(),
    })
}

#[cfg(test)]
pub(crate) fn insert_at_for_test(path: &Path, text: &str) -> io::Result<InsertedTranscript> {
    insert_at(path.to_owned(), text)
}

#[cfg(test)]
pub(crate) fn texts_at_for_test(path: &Path) -> io::Result<Vec<String>> {
    let connection = open_at(path)?;
    let mut statement = connection
        .prepare("SELECT text FROM transcripts ORDER BY id")
        .map_err(sqlite_error)?;
    let texts = statement
        .query_map([], |row| row.get(0))
        .map_err(sqlite_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_error)?;
    Ok(texts)
}

pub(crate) fn last() -> io::Result<Option<TranscriptEntry>> {
    open()?
        .query_row(
            "SELECT id, created_at, text FROM transcripts ORDER BY created_at DESC, id DESC LIMIT 1",
            [],
            |row| Ok(TranscriptEntry { id: row.get(0)?, created_at: row.get(1)?, text: row.get(2)? }),
        )
        .optional()
        .map_err(sqlite_error)
}

pub(crate) fn list_json(offset: usize, limit: usize, query: &str) -> io::Result<String> {
    let limit = limit.clamp(1, 100);
    let connection = open()?;
    let pattern = format!(
        "%{}%",
        query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let mut statement = connection
        .prepare(
            "SELECT id, created_at, text FROM transcripts
             WHERE text LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(sqlite_error)?;
    let rows = statement
        .query_map(params![pattern, (limit + 1) as i64, offset as i64], |row| {
            Ok(TranscriptEntry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                text: row.get(2)?,
            })
        })
        .map_err(sqlite_error)?;
    let mut entries = rows.collect::<Result<Vec<_>, _>>().map_err(sqlite_error)?;
    let has_more = entries.len() > limit;
    entries.truncate(limit);
    serde_json::to_string(&HistoryDocument {
        schema_version: 1,
        entries,
        has_more,
    })
    .map_err(io::Error::other)
}

pub(crate) fn delete(id: i64) -> io::Result<()> {
    open()?
        .execute("DELETE FROM transcripts WHERE id = ?1", [id])
        .map_err(sqlite_error)?;
    Ok(())
}

pub(crate) fn clear() -> io::Result<()> {
    open()?
        .execute("DELETE FROM transcripts", [])
        .map_err(sqlite_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_searches_and_deletes_transcripts() {
        let path = env::temp_dir().join(format!(
            "codex-voice-history-test-{}.sqlite3",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let connection = open_at(&path).unwrap();
        connection
            .execute(
                "INSERT INTO transcripts(created_at, text) VALUES (1, 'alpha'), (2, 'beta')",
                [],
            )
            .unwrap();
        let found: String = connection
            .query_row(
                "SELECT text FROM transcripts WHERE text LIKE '%ph%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, "alpha");
        connection
            .execute("DELETE FROM transcripts WHERE id = 1", [])
            .unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM transcripts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        drop(connection);
        let _ = fs::remove_file(path);
    }
}
