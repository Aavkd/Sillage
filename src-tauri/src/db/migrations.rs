//! Versioned migrations.
//!
//! The version lives in SQLite's own `user_version` pragma rather than in a table of our own:
//! it is transactional, it costs nothing to read, and it cannot itself need a migration.
//!
//! Rules for adding one: append, never edit. A migration that has shipped has already run on the
//! user's library, and rewriting it only changes what *new* libraries look like — which is how
//! two installations of the same version end up with two different schemas.

use rusqlite::Connection;

use super::DbError;

struct Migration {
    version: u32,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("schema/001_initial.sql"),
}];

/// The schema version this build expects.
#[must_use]
pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |migration| migration.version)
}

/// Reads the version currently stored in the database. `0` on a fresh file.
pub fn current_version(conn: &Connection) -> Result<u32, DbError> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as u32)
}

/// Applies every migration the database has not seen yet, and returns the version reached.
///
/// Running it on an up-to-date database is a no-op, which is what makes it safe to call on every
/// single launch — and what ROADMAP phase 02 asks to prove by running it twice in a row.
pub fn apply(conn: &mut Connection) -> Result<u32, DbError> {
    let current = current_version(conn)?;

    for migration in MIGRATIONS.iter().filter(|m| m.version > current) {
        // One transaction per migration: a failure half-way leaves the database exactly as it
        // was, on the previous version, instead of on a version that never existed.
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.pragma_update(None, "user_version", migration.version)?;
        tx.commit()?;
    }

    current_version(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn versions_are_unique_and_increasing() {
        // An out-of-order or duplicated version silently skips a migration.
        let mut previous = 0;
        for migration in MIGRATIONS {
            assert!(
                migration.version > previous,
                "migration {} is out of order",
                migration.version
            );
            previous = migration.version;
        }
        assert_eq!(latest_version(), previous);
    }

    #[test]
    fn a_fresh_database_reaches_the_latest_version() {
        let db = Database::open_in_memory().expect("open");
        assert_eq!(
            current_version(db.conn()).expect("version"),
            latest_version()
        );
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let mut db = Database::open_in_memory().expect("open");
        assert_eq!(apply(db.conn_mut()).expect("second run"), latest_version());
        assert_eq!(apply(db.conn_mut()).expect("third run"), latest_version());
    }
}
