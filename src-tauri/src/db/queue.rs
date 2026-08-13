//! The persisted processing queue.
//!
//! CONCEPTION.md §3.3: **one** queue, strictly sequential. It lives in the database rather than
//! in memory because closing the application during a transcription must not lose it — the queue
//! is picked up again at the next launch (CONCEPTION.md §8).
//!
//! Phase 02 owns the storage; the scheduler that drives it is phase 04.

use rusqlite::params;

use super::{Database, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueState {
    Queued,
    Running,
    Done,
    Failed,
}

impl QueueState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub transcript_id: String,
    /// Rank in the queue; the position shown on the waiting line of DESIGN.md §7.
    pub position: i64,
    pub state: QueueState,
    pub enqueued_at: i64,
    pub started_at: Option<i64>,
    pub error: Option<String>,
}

const COLUMNS: &str = "id, transcript_id, position, state, enqueued_at, started_at, error";

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
    let state: String = row.get(3)?;
    Ok(QueueItem {
        id: row.get(0)?,
        transcript_id: row.get(1)?,
        position: row.get(2)?,
        state: QueueState::parse(&state).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(DbError::Unexpected(format!(
                    "état de file inconnu : {state}"
                ))),
            )
        })?,
        enqueued_at: row.get(4)?,
        started_at: row.get(5)?,
        error: row.get(6)?,
    })
}

impl Database {
    /// Appends a transcript to the queue, or leaves it where it is if it is already there.
    ///
    /// Dropping five files at once must queue them in the order they were dropped
    /// (CONCEPTION.md §6), which is what the position does.
    pub fn enqueue(&self, transcript_id: &str, at: i64) -> Result<QueueItem, DbError> {
        let tx = self.conn().unchecked_transaction()?;

        let next_position: i64 = tx.query_row(
            "SELECT coalesce(max(position), -1) + 1 FROM queue_items",
            [],
            |row| row.get(0),
        )?;

        tx.execute(
            "INSERT INTO queue_items (id, transcript_id, position, state, enqueued_at)
             VALUES (?1, ?2, ?3, 'queued', ?4)
             ON CONFLICT (transcript_id) DO NOTHING",
            params![
                crate::model::Transcript::new_id(),
                transcript_id,
                next_position,
                at
            ],
        )?;
        tx.commit()?;

        self.queue_item(transcript_id)?
            .ok_or_else(|| DbError::Unexpected(format!("file : transcription {transcript_id}")))
    }

    pub fn queue_item(&self, transcript_id: &str) -> Result<Option<QueueItem>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM queue_items WHERE transcript_id = ?1"
        ))?;
        let mut rows = statement.query([transcript_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_from(row)?)),
            None => Ok(None),
        }
    }

    /// The whole queue, in order.
    pub fn queue(&self) -> Result<Vec<QueueItem>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM queue_items ORDER BY position"
        ))?;
        let items = statement
            .query_map([], row_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    /// The next item to process, or `None` when the queue is drained.
    pub fn next_queued(&self) -> Result<Option<QueueItem>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM queue_items WHERE state = 'queued'
             ORDER BY position LIMIT 1"
        ))?;
        let mut rows = statement.query([])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_from(row)?)),
            None => Ok(None),
        }
    }

    pub fn set_queue_state(
        &self,
        transcript_id: &str,
        state: QueueState,
        started_at: Option<i64>,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn().execute(
            "UPDATE queue_items
             SET state = ?2, started_at = coalesce(?3, started_at), error = ?4
             WHERE transcript_id = ?1",
            params![transcript_id, state.as_str(), started_at, error],
        )?;
        Ok(())
    }

    /// Puts back into the queue anything that was mid-flight when the application stopped.
    ///
    /// Nothing can be `running` at startup: no whisper process survived the previous session.
    /// Returns how many were rescued, so phase 04 can say so rather than restarting silently.
    pub fn requeue_interrupted(&self) -> Result<usize, DbError> {
        let rescued = self.conn().execute(
            "UPDATE queue_items SET state = 'queued', started_at = NULL WHERE state = 'running'",
            [],
        )?;
        Ok(rescued)
    }

    pub fn remove_from_queue(&self, transcript_id: &str) -> Result<(), DbError> {
        self.conn().execute(
            "DELETE FROM queue_items WHERE transcript_id = ?1",
            [transcript_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::transcripts::tests::transcript;

    fn db(ids: &[&str]) -> Database {
        let db = Database::open_in_memory().expect("open");
        for id in ids {
            db.save_transcript(&transcript(id, id)).expect("save");
        }
        db
    }

    #[test]
    fn items_keep_the_order_they_were_dropped_in() {
        let db = db(&["a", "b", "c"]);
        for (index, id) in ["a", "b", "c"].iter().enumerate() {
            db.enqueue(id, 1_000 + index as i64).expect("enqueue");
        }

        let order: Vec<_> = db
            .queue()
            .expect("queue")
            .into_iter()
            .map(|item| item.transcript_id)
            .collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn enqueuing_the_same_transcript_twice_does_not_duplicate_it() {
        let db = db(&["a"]);
        let first = db.enqueue("a", 1_000).expect("enqueue");
        let again = db.enqueue("a", 2_000).expect("enqueue again");

        assert_eq!(first, again, "the original entry is left alone");
        assert_eq!(db.queue().expect("queue").len(), 1);
    }

    #[test]
    fn only_the_head_of_the_queue_comes_out_next() {
        let db = db(&["a", "b"]);
        db.enqueue("a", 1_000).expect("enqueue");
        db.enqueue("b", 1_001).expect("enqueue");

        assert_eq!(
            db.next_queued().expect("next").map(|i| i.transcript_id),
            Some("a".to_string())
        );

        db.set_queue_state("a", QueueState::Running, Some(1_100), None)
            .expect("running");
        assert_eq!(
            db.next_queued().expect("next").map(|i| i.transcript_id),
            Some("b".to_string()),
            "a running item is no longer a candidate — one at a time (CONCEPTION §3.3)"
        );
    }

    #[test]
    fn a_state_change_keeps_the_start_time_it_was_given() {
        let db = db(&["a"]);
        db.enqueue("a", 1_000).expect("enqueue");
        db.set_queue_state("a", QueueState::Running, Some(1_100), None)
            .expect("running");
        db.set_queue_state("a", QueueState::Done, None, None)
            .expect("done");

        let item = db.queue_item("a").expect("read").expect("present");
        assert_eq!(item.state, QueueState::Done);
        assert_eq!(item.started_at, Some(1_100), "the start time is not lost");
    }

    #[test]
    fn a_failure_keeps_its_message() {
        let db = db(&["a"]);
        db.enqueue("a", 1_000).expect("enqueue");
        db.set_queue_state("a", QueueState::Failed, None, Some("VRAM insuffisante"))
            .expect("failed");

        let item = db.queue_item("a").expect("read").expect("present");
        assert_eq!(item.error.as_deref(), Some("VRAM insuffisante"));
    }

    #[test]
    fn the_queue_survives_a_close_and_picks_up_where_it_stopped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("library.db");

        {
            let db = Database::open(&path).expect("open");
            for id in ["a", "b"] {
                db.save_transcript(&transcript(id, id)).expect("save");
                db.enqueue(id, 1_000).expect("enqueue");
            }
            db.set_queue_state("a", QueueState::Running, Some(1_100), None)
                .expect("running");
            db.close().expect("close");
        }

        let db = Database::open(&path).expect("reopen");
        assert_eq!(db.requeue_interrupted().expect("requeue"), 1);
        assert_eq!(
            db.next_queued().expect("next").map(|i| i.transcript_id),
            Some("a".to_string()),
            "the interrupted item comes back at its own place, not at the end"
        );
        assert_eq!(db.requeue_interrupted().expect("again"), 0);
    }

    #[test]
    fn removing_a_transcript_removes_its_queue_entry() {
        let db = db(&["a"]);
        db.enqueue("a", 1_000).expect("enqueue");

        db.delete_transcript("a").expect("delete");
        assert!(db.queue().expect("queue").is_empty());
    }

    #[test]
    fn an_item_can_be_taken_out_of_the_queue_on_its_own() {
        let db = db(&["a", "b"]);
        db.enqueue("a", 1_000).expect("enqueue");
        db.enqueue("b", 1_001).expect("enqueue");

        db.remove_from_queue("a").expect("remove");
        let remaining: Vec<_> = db
            .queue()
            .expect("queue")
            .into_iter()
            .map(|item| item.transcript_id)
            .collect();
        assert_eq!(remaining, vec!["b"]);
        assert!(
            db.transcript_row("a").expect("read").is_some(),
            "leaving the queue must not delete the transcript"
        );
    }

    #[test]
    fn queue_states_survive_the_trip_through_sql() {
        for state in [
            QueueState::Queued,
            QueueState::Running,
            QueueState::Done,
            QueueState::Failed,
        ] {
            assert_eq!(QueueState::parse(state.as_str()), Some(state));
        }
        assert_eq!(QueueState::parse("en attente"), None);
    }
}
