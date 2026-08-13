//! Indexing and reading back transcripts.
//!
//! The record itself lives in `data/<id>.json`; what is written here is what the library screen
//! and the search need to answer without opening a single JSON file.

use rusqlite::params;

use super::{from_sql_ms, to_sql_ms, Database, DbError};
use crate::model::{Ms, Segment, Transcript, TranscriptStatus, Word};

/// One row of the library list — everything DESIGN.md §7 puts on a card, and nothing more.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptRow {
    pub id: String,
    pub source_path: String,
    pub media_path: String,
    pub sha256: String,
    pub created_at: i64,
    pub duration_ms: Ms,
    pub title: String,
    pub title_is_custom: bool,
    pub language: Option<String>,
    pub model: String,
    pub status: TranscriptStatus,
    pub error: Option<String>,
    pub transcript_hash: String,
    /// The one summary line of the card. Empty until the LLM layer of phase 08 has produced one.
    pub summary: String,
}

/// The columns [`row_from`] expects, in order.
const COLUMNS: [&str; 14] = [
    "id",
    "source_path",
    "media_path",
    "sha256",
    "created_at",
    "duration_ms",
    "title",
    "title_is_custom",
    "language",
    "model",
    "status",
    "error",
    "transcript_hash",
    "summary",
];

/// The select list, qualified by a table name.
///
/// The search of [`super::search`] joins `transcripts` with the FTS table, which carries columns
/// of the same names; without the qualification SQLite rejects the query as ambiguous.
pub(super) fn select_list(table: &str) -> String {
    COLUMNS
        .iter()
        .map(|column| format!("{table}.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptRow> {
    let status: String = row.get(10)?;
    Ok(TranscriptRow {
        id: row.get(0)?,
        source_path: row.get(1)?,
        media_path: row.get(2)?,
        sha256: row.get(3)?,
        created_at: row.get(4)?,
        duration_ms: from_sql_ms(row.get(5)?),
        title: row.get(6)?,
        title_is_custom: row.get(7)?,
        language: row.get(8)?,
        model: row.get(9)?,
        // An unknown status can only come from a database written by a newer build. Falling back
        // to `Failed` would lie about the transcript; `Queued` would relaunch it. Neither is
        // right, so the read fails loudly and the caller reports it.
        status: TranscriptStatus::parse(&status).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(super::DbError::Unexpected(format!(
                    "statut de transcription inconnu : {status}"
                ))),
            )
        })?,
        error: row.get(11)?,
        transcript_hash: row.get(12)?,
        summary: row.get(13)?,
    })
}

impl Database {
    /// Writes a transcript into the index: the row, its segments, its words, its tags.
    ///
    /// Idempotent — re-indexing the same transcript replaces its structure without touching the
    /// generated outputs attached to it. That is why the row is upserted rather than replaced:
    /// `INSERT OR REPLACE` would delete the row first, and the cascade would take the LLM
    /// outputs, the queue entry and the tags down with it.
    pub fn save_transcript(&self, transcript: &Transcript) -> Result<(), DbError> {
        let displayed = transcript.displayed();
        let body = crate::model::edits::flatten(&displayed);
        let hash = crate::model::hash::transcript_hash(&body);

        let tx = self.conn().unchecked_transaction()?;

        tx.execute(
            "INSERT INTO transcripts (id, source_path, media_path, sha256, created_at,
                 duration_ms, title, title_is_custom, language, model, status, error,
                 transcript_hash, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT (id) DO UPDATE SET
                 source_path = excluded.source_path,
                 media_path = excluded.media_path,
                 sha256 = excluded.sha256,
                 created_at = excluded.created_at,
                 duration_ms = excluded.duration_ms,
                 title = excluded.title,
                 title_is_custom = excluded.title_is_custom,
                 language = excluded.language,
                 model = excluded.model,
                 status = excluded.status,
                 error = excluded.error,
                 transcript_hash = excluded.transcript_hash,
                 body = excluded.body",
            params![
                transcript.id,
                transcript.source_path,
                transcript.media_path,
                transcript.sha256,
                transcript.created_at,
                to_sql_ms(transcript.duration_ms),
                transcript.title,
                transcript.title_is_custom,
                transcript.language,
                transcript.model,
                transcript.status.as_str(),
                transcript.error,
                hash,
                body,
            ],
        )?;

        tx.execute(
            "DELETE FROM words WHERE transcript_id = ?1",
            [&transcript.id],
        )?;
        tx.execute(
            "DELETE FROM segments WHERE transcript_id = ?1",
            [&transcript.id],
        )?;

        {
            let mut segment_stmt = tx.prepare(
                "INSERT INTO segments (transcript_id, idx, start_ms, end_ms, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut word_stmt = tx.prepare(
                "INSERT INTO words (transcript_id, word_id, segment_idx, idx, start_ms, end_ms,
                     text, prob)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;

            for (segment_idx, segment) in transcript.segments.iter().enumerate() {
                let segment_idx = segment_idx as i64;
                segment_stmt.execute(params![
                    transcript.id,
                    segment_idx,
                    to_sql_ms(segment.start_ms),
                    to_sql_ms(segment.end_ms),
                    segment.text,
                ])?;

                for (idx, word) in segment.words.iter().enumerate() {
                    word_stmt.execute(params![
                        transcript.id,
                        word.id,
                        segment_idx,
                        idx as i64,
                        to_sql_ms(word.start_ms),
                        to_sql_ms(word.end_ms),
                        word.text,
                        f64::from(word.prob),
                    ])?;
                }
            }
        }

        Self::write_tags(&tx, &transcript.id, &transcript.tags)?;
        tx.commit()?;
        Ok(())
    }

    /// Re-indexes the text after a correction has stabilised (phase 07), and returns the new
    /// `transcript_hash`.
    ///
    /// Only the two indexed columns move, so the search index is rebuilt for this transcript and
    /// nothing else is touched.
    pub fn refresh_text(&self, transcript: &Transcript) -> Result<String, DbError> {
        let body = transcript.displayed_text();
        let hash = crate::model::hash::transcript_hash(&body);

        self.conn().execute(
            "UPDATE transcripts SET body = ?2, transcript_hash = ?3 WHERE id = ?1",
            params![transcript.id, body, hash],
        )?;
        Ok(hash)
    }

    pub fn transcript_row(&self, id: &str) -> Result<Option<TranscriptRow>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {} FROM transcripts WHERE id = ?1",
            select_list("transcripts")
        ))?;
        let mut rows = statement.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_from(row)?)),
            None => Ok(None),
        }
    }

    /// The library list: newest first (CONCEPTION.md §5.1).
    pub fn list_transcripts(&self) -> Result<Vec<TranscriptRow>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {} FROM transcripts ORDER BY created_at DESC, id DESC",
            select_list("transcripts")
        ))?;
        let rows = statement
            .query_map([], row_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Number of transcripts — the counter next to « Bibliothèque » in the sidebar.
    pub fn count_transcripts(&self) -> Result<u32, DbError> {
        let count: i64 = self
            .conn()
            .query_row("SELECT count(*) FROM transcripts", [], |row| row.get(0))?;
        Ok(count.max(0) as u32)
    }

    /// Entries sharing a media hash — the duplicate check of CONCEPTION.md §8.
    pub fn transcripts_with_sha256(&self, sha256: &str) -> Result<Vec<TranscriptRow>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {} FROM transcripts WHERE sha256 = ?1 ORDER BY created_at DESC",
            select_list("transcripts")
        ))?;
        let rows = statement
            .query_map([sha256], row_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Reads the verbatim structure back out of the index.
    ///
    /// The JSON file stays the reference; this exists so the index is verifiably a faithful
    /// mirror of it, and so phase 06 can ask time-based questions in SQL.
    pub fn segments_of(&self, transcript_id: &str) -> Result<Vec<Segment>, DbError> {
        let mut statement = self.conn().prepare(
            "SELECT idx, start_ms, end_ms, text FROM segments
             WHERE transcript_id = ?1 ORDER BY idx",
        )?;
        let mut segments: Vec<Segment> = statement
            .query_map([transcript_id], |row| {
                Ok(Segment {
                    start_ms: from_sql_ms(row.get(1)?),
                    end_ms: from_sql_ms(row.get(2)?),
                    text: row.get(3)?,
                    words: Vec::new(),
                })
            })?
            .collect::<Result<_, _>>()?;

        let mut statement = self.conn().prepare(
            "SELECT segment_idx, word_id, start_ms, end_ms, text, prob FROM words
             WHERE transcript_id = ?1 ORDER BY segment_idx, idx",
        )?;
        let words = statement.query_map([transcript_id], |row| {
            let segment_idx: i64 = row.get(0)?;
            Ok((
                segment_idx.max(0) as usize,
                Word {
                    id: row.get(1)?,
                    start_ms: from_sql_ms(row.get(2)?),
                    end_ms: from_sql_ms(row.get(3)?),
                    text: row.get(4)?,
                    prob: row.get::<_, f64>(5)? as f32,
                },
            ))
        })?;

        for word in words {
            let (segment_idx, word) = word?;
            if let Some(segment) = segments.get_mut(segment_idx) {
                segment.words.push(word);
            }
        }

        Ok(segments)
    }

    pub fn set_title(&self, id: &str, title: &str, is_custom: bool) -> Result<(), DbError> {
        self.conn().execute(
            "UPDATE transcripts SET title = ?2, title_is_custom = ?3 WHERE id = ?1",
            params![id, title, is_custom],
        )?;
        Ok(())
    }

    pub fn set_status(
        &self,
        id: &str,
        status: TranscriptStatus,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn().execute(
            "UPDATE transcripts SET status = ?2, error = ?3 WHERE id = ?1",
            params![id, status.as_str(), error],
        )?;
        Ok(())
    }

    pub fn set_language(&self, id: &str, language: Option<&str>) -> Result<(), DbError> {
        self.conn().execute(
            "UPDATE transcripts SET language = ?2 WHERE id = ?1",
            params![id, language],
        )?;
        Ok(())
    }

    /// Removes a transcript from the index. The cascade takes its segments, words, tag links,
    /// outputs and queue entry with it; the media and JSON files are the caller's business.
    pub fn delete_transcript(&self, id: &str) -> Result<(), DbError> {
        self.conn()
            .execute("DELETE FROM transcripts WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{Edit, TranscriptStatus};

    pub(crate) fn transcript(id: &str, title: &str) -> Transcript {
        Transcript {
            id: id.to_string(),
            source_path: format!(r"D:\Audio\{title}.m4a"),
            media_path: format!("media/{id}.m4a"),
            sha256: "0".repeat(64),
            created_at: 1_755_000_000_000,
            duration_ms: 4_000,
            title: title.to_string(),
            title_is_custom: false,
            language: Some("fr".to_string()),
            model: "large-v3-turbo".to_string(),
            status: TranscriptStatus::Done,
            error: None,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 2_000,
                text: "le chat dort".to_string(),
                words: vec![
                    Word {
                        id: 0,
                        start_ms: 0,
                        end_ms: 700,
                        text: "le".to_string(),
                        prob: 0.91,
                    },
                    Word {
                        id: 1,
                        start_ms: 700,
                        end_ms: 1_400,
                        text: "chat".to_string(),
                        prob: 0.42,
                    },
                    Word {
                        id: 2,
                        start_ms: 1_400,
                        end_ms: 2_000,
                        text: "dort".to_string(),
                        prob: 0.99,
                    },
                ],
            }],
            edits: Vec::new(),
            tags: vec!["client".to_string()],
            next_word_id: 3,
        }
    }

    #[test]
    fn a_saved_transcript_reads_back_identically() {
        let db = Database::open_in_memory().expect("open");
        let transcript = transcript("a", "Entretien");
        db.save_transcript(&transcript).expect("save");

        let row = db.transcript_row("a").expect("read").expect("present");
        assert_eq!(row.title, "Entretien");
        assert_eq!(row.duration_ms, 4_000);
        assert_eq!(row.language.as_deref(), Some("fr"));
        assert_eq!(row.status, TranscriptStatus::Done);
        assert_eq!(row.transcript_hash, transcript.transcript_hash());
        assert_eq!(db.tags_of("a").expect("tags"), vec!["client".to_string()]);
    }

    #[test]
    fn the_index_mirrors_the_verbatim_structure() {
        let db = Database::open_in_memory().expect("open");
        let transcript = transcript("a", "Entretien");
        db.save_transcript(&transcript).expect("save");

        assert_eq!(db.segments_of("a").expect("segments"), transcript.segments);
    }

    #[test]
    fn an_unknown_transcript_reads_as_absent_rather_than_failing() {
        let db = Database::open_in_memory().expect("open");
        assert_eq!(db.transcript_row("nope").expect("read"), None);
        assert!(db.segments_of("nope").expect("segments").is_empty());
    }

    #[test]
    fn saving_twice_replaces_the_structure_without_duplicating_it() {
        let db = Database::open_in_memory().expect("open");
        let mut transcript = transcript("a", "Entretien");
        db.save_transcript(&transcript).expect("first save");

        transcript.segments[0].words.pop();
        transcript.title = "Entretien Marchand".to_string();
        db.save_transcript(&transcript).expect("second save");

        assert_eq!(db.count_transcripts().expect("count"), 1);
        assert_eq!(db.segments_of("a").expect("segments")[0].words.len(), 2);
        assert_eq!(
            db.transcript_row("a")
                .expect("read")
                .expect("present")
                .title,
            "Entretien Marchand"
        );
    }

    #[test]
    fn the_indexed_hash_follows_the_corrections() {
        let db = Database::open_in_memory().expect("open");
        let mut transcript = transcript("a", "Entretien");
        db.save_transcript(&transcript).expect("save");
        let before = db.transcript_row("a").expect("read").expect("present");

        transcript.edits.push(Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        });
        let hash = db.refresh_text(&transcript).expect("refresh");

        let after = db.transcript_row("a").expect("read").expect("present");
        assert_ne!(after.transcript_hash, before.transcript_hash);
        assert_eq!(after.transcript_hash, hash);
        assert_eq!(hash, transcript.transcript_hash());
    }

    #[test]
    fn the_list_comes_back_newest_first() {
        let db = Database::open_in_memory().expect("open");
        for (id, created_at) in [("a", 1_000_i64), ("b", 3_000), ("c", 2_000)] {
            let mut transcript = transcript(id, id);
            transcript.created_at = created_at;
            db.save_transcript(&transcript).expect("save");
        }

        let ids: Vec<_> = db
            .list_transcripts()
            .expect("list")
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn duplicates_are_found_by_media_hash() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&transcript("a", "Une")).expect("save");
        db.save_transcript(&transcript("b", "Deux")).expect("save");

        // Both fixtures share a sha256: transcribing the same file twice is allowed
        // (CONCEPTION.md §8), so the index must report both rather than refuse the second.
        assert_eq!(
            db.transcripts_with_sha256(&"0".repeat(64))
                .expect("lookup")
                .len(),
            2
        );
        assert!(db
            .transcripts_with_sha256("autre")
            .expect("lookup")
            .is_empty());
    }

    #[test]
    fn metadata_updates_touch_only_what_they_name() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&transcript("a", "Entretien"))
            .expect("save");

        db.set_title("a", "Titre à moi", true).expect("title");
        db.set_status("a", TranscriptStatus::Failed, Some("ffmpeg a échoué"))
            .expect("status");
        db.set_language("a", None).expect("language");

        let row = db.transcript_row("a").expect("read").expect("present");
        assert_eq!(row.title, "Titre à moi");
        assert!(row.title_is_custom);
        assert_eq!(row.status, TranscriptStatus::Failed);
        assert_eq!(row.error.as_deref(), Some("ffmpeg a échoué"));
        assert_eq!(row.language, None);
    }

    #[test]
    fn deleting_a_transcript_takes_its_structure_with_it() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&transcript("a", "Entretien"))
            .expect("save");
        db.delete_transcript("a").expect("delete");

        assert_eq!(db.count_transcripts().expect("count"), 0);
        assert!(db.segments_of("a").expect("segments").is_empty());

        let orphans: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM words", [], |row| row.get(0))
            .expect("count");
        assert_eq!(orphans, 0);
    }

    #[test]
    fn word_probabilities_survive_the_index() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&transcript("a", "Entretien"))
            .expect("save");

        let probs: Vec<f32> = db.segments_of("a").expect("segments")[0]
            .words
            .iter()
            .map(|word| word.prob)
            .collect();
        assert_eq!(probs, vec![0.91_f32, 0.42, 0.99]);
    }
}
