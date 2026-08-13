//! The library index: `library.db`.
//!
//! SQLite is compiled into the binary (`rusqlite/bundled`), so the application never depends on
//! a system SQLite and the FTS5 module is guaranteed to be there.
//!
//! What lives here is only what has to be **queried**. The authoritative record of a transcript
//! is its JSON file (see [`crate::model`]); this database can be rebuilt from those files.

use std::path::Path;

use rusqlite::Connection;

pub mod migrations;
pub mod outputs;
pub mod queue;
pub mod search;
pub mod tags;
pub mod transcripts;

pub use outputs::LlmOutput;
pub use queue::{QueueItem, QueueState};
pub use search::{fts_query, SearchHit};
pub use tags::TagCount;
pub use transcripts::TranscriptRow;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("erreur de la base de la bibliothèque : {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("valeur inattendue en base : {0}")]
    Unexpected(String),
}

/// An open connection to the library index.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens `library.db`, creating it and bringing it up to the current schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// An index that lives only for the duration of a test.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, DbError> {
        // `foreign_keys` is per-connection and off by default; without it, deleting a transcript
        // would leave its segments, words and outputs behind for good.
        // WAL keeps reads from blocking the writer, which matters as soon as the streaming
        // transcription of phase 04 writes while the library screen reads.
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;

        let mut db = Self { conn };
        migrations::apply(&mut db.conn)?;
        Ok(db)
    }

    #[must_use]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    #[must_use]
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Closes the connection, flushing the write-ahead log.
    ///
    /// Moving the library folder (ROADMAP phase 02, task 6) goes through here: on Windows the
    /// files stay locked as long as a connection is open, and the `-wal` file left behind by a
    /// brutal close would carry committed data that the moved copy would be missing.
    pub fn close(self) -> Result<(), DbError> {
        self.conn.close().map_err(|(_, err)| DbError::Sqlite(err))
    }

    /// Reads a library-local setting.
    pub fn setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let mut statement = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = statement.query([key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Writes a library-local setting.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }
}

/// Milliseconds are stored as SQLite INTEGER, which is signed.
///
/// The saturation is unreachable in practice — `i64::MAX` milliseconds is nearly 300 million
/// years — and is there so that no arithmetic on a duration can ever panic in a release build.
pub(crate) fn to_sql_ms(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Reads back a duration stored by [`to_sql_ms`]. A negative value cannot happen; clamping it to
/// zero is still cheaper than a fallible read at every call site.
pub(crate) fn from_sql_ms(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_database_has_every_table_and_the_search_index() {
        let db = Database::open_in_memory().expect("open");

        let mut statement = db
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");

        for expected in [
            "llm_outputs",
            "queue_items",
            "segments",
            "settings",
            "tags",
            "transcript_tags",
            "transcripts",
            "transcripts_fts",
            "words",
        ] {
            assert!(
                tables.iter().any(|name| name == expected),
                "table {expected} is missing from {tables:?}"
            );
        }
    }

    #[test]
    fn fts5_is_available_in_this_build() {
        // The whole search of phase 05 rests on it, and a SQLite built without FTS5 fails only
        // at the moment the virtual table is created — that is, here.
        let db = Database::open_in_memory().expect("open");
        db.conn()
            .execute_batch("CREATE VIRTUAL TABLE probe USING fts5(x); DROP TABLE probe;")
            .expect("FTS5 must be compiled in");
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = Database::open_in_memory().expect("open");
        let enabled: i64 = db
            .conn()
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("pragma");
        assert_eq!(enabled, 1);

        let orphan = db.conn().execute(
            "INSERT INTO segments (transcript_id, idx, start_ms, end_ms, text)
             VALUES ('nowhere', 0, 0, 1, 'x')",
            [],
        );
        assert!(orphan.is_err(), "an orphan segment must be refused");
    }

    #[test]
    fn library_local_settings_round_trip() {
        let db = Database::open_in_memory().expect("open");
        assert_eq!(db.setting("dernier_ouvert").expect("read"), None);

        db.set_setting("dernier_ouvert", "abc").expect("write");
        assert_eq!(
            db.setting("dernier_ouvert").expect("read"),
            Some("abc".to_string())
        );

        db.set_setting("dernier_ouvert", "déjà").expect("overwrite");
        assert_eq!(
            db.setting("dernier_ouvert").expect("read"),
            Some("déjà".to_string())
        );
    }

    #[test]
    fn millisecond_conversions_round_trip() {
        for value in [0_u64, 1, 726_000, 7_200_000] {
            assert_eq!(from_sql_ms(to_sql_ms(value)), value);
        }
        assert_eq!(from_sql_ms(-1), 0);
    }
}
