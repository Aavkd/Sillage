//! Generated outputs, and the staleness comparison that phase 08 hangs on.
//!
//! CONCEPTION.md §3.4: `source_transcript_hash ≠ transcript_hash` ⇒ the output is stale. The
//! content stays; only a badge appears (decision #18). Nothing here ever deletes a generated
//! output because the text moved.

use rusqlite::params;

use super::{Database, DbError};

/// One generated output. `kind` is `cleaned`, `summary`, `notes`, or `custom:<slug>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmOutput {
    pub id: String,
    pub transcript_id: String,
    pub kind: String,
    pub provider: String,
    pub model: String,
    pub prompt_version: u32,
    /// The `transcript_hash` this content was generated from.
    pub source_transcript_hash: String,
    /// Epoch milliseconds.
    pub generated_at: i64,
    pub content: String,
}

impl LlmOutput {
    /// Whether the transcript has moved since this output was generated.
    #[must_use]
    pub fn is_stale(&self, current_transcript_hash: &str) -> bool {
        self.source_transcript_hash != current_transcript_hash
    }
}

/// Always qualified: `stale_outputs` joins `transcripts`, which has an `id` and a `model` of its
/// own, and an unqualified list would be ambiguous.
const COLUMNS: &str = "o.id, o.transcript_id, o.kind, o.provider, o.model, o.prompt_version, \
                       o.source_transcript_hash, o.generated_at, o.content";

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<LlmOutput> {
    Ok(LlmOutput {
        id: row.get(0)?,
        transcript_id: row.get(1)?,
        kind: row.get(2)?,
        provider: row.get(3)?,
        model: row.get(4)?,
        prompt_version: row.get(5)?,
        source_transcript_hash: row.get(6)?,
        generated_at: row.get(7)?,
        content: row.get(8)?,
    })
}

impl Database {
    /// Stores a generated output, replacing the previous one of the same kind.
    ///
    /// Regenerating is the ordinary path (the `Régénérer` button of DESIGN.md §8), so the
    /// conflict is resolved on `(transcript_id, kind)` rather than on the identifier: the caller
    /// does not have to know whether an output already existed.
    pub fn save_output(&self, output: &LlmOutput) -> Result<(), DbError> {
        self.conn().execute(
            "INSERT INTO llm_outputs (id, transcript_id, kind, provider, model, prompt_version,
                 source_transcript_hash, generated_at, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (transcript_id, kind) DO UPDATE SET
                 id = excluded.id,
                 provider = excluded.provider,
                 model = excluded.model,
                 prompt_version = excluded.prompt_version,
                 source_transcript_hash = excluded.source_transcript_hash,
                 generated_at = excluded.generated_at,
                 content = excluded.content",
            params![
                output.id,
                output.transcript_id,
                output.kind,
                output.provider,
                output.model,
                output.prompt_version,
                output.source_transcript_hash,
                output.generated_at,
                output.content,
            ],
        )?;
        Ok(())
    }

    /// Every output of a transcript, oldest kind first by name so panels keep a stable order.
    pub fn outputs_of(&self, transcript_id: &str) -> Result<Vec<LlmOutput>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM llm_outputs o WHERE o.transcript_id = ?1 ORDER BY o.kind"
        ))?;
        let outputs = statement
            .query_map([transcript_id], row_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(outputs)
    }

    pub fn output(&self, transcript_id: &str, kind: &str) -> Result<Option<LlmOutput>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM llm_outputs o WHERE o.transcript_id = ?1 AND o.kind = ?2"
        ))?;
        let mut rows = statement.query((transcript_id, kind))?;
        match rows.next()? {
            Some(row) => Ok(Some(row_from(row)?)),
            None => Ok(None),
        }
    }

    /// Outputs whose source text has moved — those wearing the `OBSOLÈTE` badge.
    pub fn stale_outputs(&self, transcript_id: &str) -> Result<Vec<LlmOutput>, DbError> {
        let mut statement = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM llm_outputs o
             JOIN transcripts t ON t.id = o.transcript_id
             WHERE o.transcript_id = ?1 AND o.source_transcript_hash <> t.transcript_hash
             ORDER BY o.kind"
        ))?;
        let outputs = statement
            .query_map([transcript_id], row_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(outputs)
    }

    pub fn delete_output(&self, transcript_id: &str, kind: &str) -> Result<(), DbError> {
        self.conn().execute(
            "DELETE FROM llm_outputs WHERE transcript_id = ?1 AND kind = ?2",
            (transcript_id, kind),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::transcripts::tests::transcript;
    use crate::model::Edit;

    fn output(transcript_id: &str, kind: &str, hash: &str, content: &str) -> LlmOutput {
        LlmOutput {
            id: format!("{transcript_id}-{kind}"),
            transcript_id: transcript_id.to_string(),
            kind: kind.to_string(),
            provider: "ollama".to_string(),
            model: "llama3.1:8b".to_string(),
            prompt_version: 1,
            source_transcript_hash: hash.to_string(),
            generated_at: 1_755_000_100_000,
            content: content.to_string(),
        }
    }

    #[test]
    fn an_output_round_trips() {
        let db = Database::open_in_memory().expect("open");
        let fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");

        let generated = output(
            "a",
            "summary",
            &fixture.transcript_hash(),
            "Le chat dort déjà.",
        );
        db.save_output(&generated).expect("save output");

        assert_eq!(db.output("a", "summary").expect("read"), Some(generated));
        assert_eq!(db.outputs_of("a").expect("list").len(), 1);
    }

    #[test]
    fn regenerating_replaces_rather_than_accumulates() {
        let db = Database::open_in_memory().expect("open");
        let fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");
        let hash = fixture.transcript_hash();

        db.save_output(&output("a", "summary", &hash, "première"))
            .expect("save");
        db.save_output(&output("a", "summary", &hash, "seconde"))
            .expect("save");

        let outputs = db.outputs_of("a").expect("list");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].content, "seconde");
    }

    #[test]
    fn a_correction_makes_the_output_stale_without_erasing_it() {
        let db = Database::open_in_memory().expect("open");
        let mut fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");

        let generated = output(
            "a",
            "summary",
            &fixture.transcript_hash(),
            "Le chat dort déjà.",
        );
        db.save_output(&generated).expect("save output");
        assert!(db.stale_outputs("a").expect("stale").is_empty());

        fixture.edits.push(Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        });
        db.refresh_text(&fixture).expect("refresh");

        let stale = db.stale_outputs("a").expect("stale");
        assert_eq!(stale.len(), 1);
        assert_eq!(
            stale[0].content, "Le chat dort déjà.",
            "the stale content stays visible (decision #18)"
        );
        assert!(stale[0].is_stale(&fixture.transcript_hash()));
    }

    #[test]
    fn regenerating_lifts_the_badge() {
        let db = Database::open_in_memory().expect("open");
        let mut fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");
        db.save_output(&output("a", "summary", "vieux-hash", "ancien"))
            .expect("save output");

        fixture.edits.push(Edit::Delete { word: 0 });
        let hash = db.refresh_text(&fixture).expect("refresh");
        assert_eq!(db.stale_outputs("a").expect("stale").len(), 1);

        db.save_output(&output("a", "summary", &hash, "à jour"))
            .expect("regenerate");
        assert!(db.stale_outputs("a").expect("stale").is_empty());
    }

    #[test]
    fn custom_prompts_live_beside_the_three_fixed_kinds() {
        let db = Database::open_in_memory().expect("open");
        let fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");
        let hash = fixture.transcript_hash();

        for kind in ["cleaned", "summary", "notes", "custom:plan-d-action"] {
            db.save_output(&output("a", kind, &hash, kind))
                .expect("save");
        }

        let kinds: Vec<_> = db
            .outputs_of("a")
            .expect("list")
            .into_iter()
            .map(|output| output.kind)
            .collect();
        assert_eq!(
            kinds,
            vec!["cleaned", "custom:plan-d-action", "notes", "summary"]
        );
    }

    #[test]
    fn deleting_a_transcript_takes_its_outputs_with_it() {
        let db = Database::open_in_memory().expect("open");
        let fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");
        db.save_output(&output("a", "summary", &fixture.transcript_hash(), "x"))
            .expect("save output");

        db.delete_transcript("a").expect("delete");
        assert!(db.outputs_of("a").expect("list").is_empty());
    }

    #[test]
    fn re_indexing_a_transcript_keeps_its_outputs() {
        // `save_transcript` upserts for exactly this reason: an `INSERT OR REPLACE` would
        // cascade and quietly destroy every generated output on a simple re-index.
        let db = Database::open_in_memory().expect("open");
        let fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");
        db.save_output(&output("a", "summary", &fixture.transcript_hash(), "x"))
            .expect("save output");

        db.save_transcript(&fixture).expect("re-index");
        assert_eq!(db.outputs_of("a").expect("list").len(), 1);
    }

    #[test]
    fn deleting_one_output_leaves_the_others() {
        let db = Database::open_in_memory().expect("open");
        let fixture = transcript("a", "Une");
        db.save_transcript(&fixture).expect("save");
        let hash = fixture.transcript_hash();
        db.save_output(&output("a", "summary", &hash, "r"))
            .expect("save");
        db.save_output(&output("a", "cleaned", &hash, "n"))
            .expect("save");

        db.delete_output("a", "summary").expect("delete");
        let kinds: Vec<_> = db
            .outputs_of("a")
            .expect("list")
            .into_iter()
            .map(|output| output.kind)
            .collect();
        assert_eq!(kinds, vec!["cleaned"]);
    }
}
