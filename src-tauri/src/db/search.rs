//! Full-text search over titles, verbatim and summaries.
//!
//! The index is an FTS5 table fed by triggers (see `schema/001_initial.sql`). What lives here is
//! the part FTS5 cannot do for us: turning what the user typed into a query the engine accepts.

use super::transcripts::{row_from, select_list, TranscriptRow};
use super::{Database, DbError};

/// One result, with the relevance FTS5 gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub row: TranscriptRow,
    /// bm25 score: **lower is better**, and always negative. Kept as FTS5 returns it rather than
    /// inverted, so that the ordering here and in SQL cannot drift apart.
    pub rank: f64,
}

/// Turns what the user typed into an FTS5 query, or `None` if there is nothing to search for.
///
/// Everything that is not a letter or a digit is treated as a separator and dropped. That is
/// blunt, and it is the point: the search box is not a query language, and a stray quote or
/// parenthesis must never turn into a syntax error the user cannot make sense of. Each word is
/// then quoted — so that `AND`, `OR`, `NOT` and `NEAR` are searched for rather than obeyed — and
/// given a `*`, so that the results narrow as the user types instead of appearing on the last
/// keystroke.
#[must_use]
pub fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\"*"))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

impl Database {
    /// Searches titles, verbatim and summaries, best match first.
    ///
    /// An empty or punctuation-only query returns nothing rather than everything: the library
    /// list is what shows everything, and a search that silently means « tout » would look like
    /// a bug the moment the user clears the field.
    pub fn search(&self, input: &str, limit: u32) -> Result<Vec<SearchHit>, DbError> {
        let Some(query) = fts_query(input) else {
            return Ok(Vec::new());
        };

        // Weights: a hit in the title counts for much more than one in the body, and a hit in
        // the summary sits in between — the summary is already a condensation of the body, so a
        // word appearing in it is a stronger signal than the same word buried in an hour of
        // speech.
        let mut statement = self.conn().prepare(&format!(
            "SELECT {}, bm25(transcripts_fts, 10.0, 1.0, 4.0) AS rank
             FROM transcripts_fts
             JOIN transcripts ON transcripts.rowid = transcripts_fts.rowid
             WHERE transcripts_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
            select_list("transcripts")
        ))?;

        let hits = statement
            .query_map(rusqlite::params![query, limit], |row| {
                Ok(SearchHit {
                    row: row_from(row)?,
                    rank: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::transcripts::tests::transcript;
    use crate::db::LlmOutput;
    use crate::model::{Edit, Segment, Word};

    /// Builds a transcript whose verbatim is `text`, one word per space-separated token.
    fn with_text(id: &str, title: &str, text: &str) -> crate::model::Transcript {
        let mut fixture = transcript(id, title);
        fixture.segments = vec![Segment {
            start_ms: 0,
            end_ms: 1_000,
            text: text.to_string(),
            words: text
                .split_whitespace()
                .enumerate()
                .map(|(index, word)| Word {
                    id: index as u32,
                    start_ms: index as u64 * 100,
                    end_ms: index as u64 * 100 + 100,
                    text: word.to_string(),
                    prob: 0.9,
                })
                .collect(),
        }];
        fixture.next_word_id = fixture.segments[0].words.len() as u32;
        fixture
    }

    fn ids(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.row.id.as_str()).collect()
    }

    #[test]
    fn a_query_is_built_from_words_alone() {
        assert_eq!(fts_query("résumé"), Some("\"résumé\"*".to_string()));
        assert_eq!(
            fts_query("  chat   dort "),
            Some("\"chat\"* AND \"dort\"*".to_string())
        );
        assert_eq!(
            fts_query("l'été"),
            Some("\"l\"* AND \"été\"*".to_string()),
            "an apostrophe separates rather than breaking the query"
        );
        assert_eq!(
            fts_query("AND OR NOT"),
            Some("\"AND\"* AND \"OR\"* AND \"NOT\"*".to_string()),
            "FTS5 keywords are searched for, not obeyed"
        );
    }

    #[test]
    fn a_query_with_nothing_to_search_for_is_none() {
        assert_eq!(fts_query(""), None);
        assert_eq!(fts_query("   "), None);
        assert_eq!(fts_query("\"()*"), None);
    }

    #[test]
    fn punctuation_cannot_produce_a_syntax_error() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text("a", "Une", "le chat dort"))
            .expect("save");

        for input in ["\"", "*", "(", "chat)", "NEAR(", "-- ; DROP", "a\"\"b"] {
            db.search(input, 10)
                .unwrap_or_else(|err| panic!("query {input:?} failed: {err}"));
        }
    }

    #[test]
    fn an_empty_query_returns_nothing_rather_than_everything() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text("a", "Une", "le chat dort"))
            .expect("save");

        assert!(db.search("", 10).expect("search").is_empty());
        assert!(db.search("   ", 10).expect("search").is_empty());
    }

    #[test]
    fn french_accents_are_found_however_they_are_typed() {
        // ROADMAP phase 02: a search on « résumé » or « déjà » must return the right entry.
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text(
            "a",
            "Entretien",
            "voici le résumé de la réunion déjà tenue",
        ))
        .expect("save");
        db.save_transcript(&with_text("b", "Autre", "rien à voir ici"))
            .expect("save");

        assert_eq!(ids(&db.search("résumé", 10).expect("search")), ["a"]);
        assert_eq!(ids(&db.search("déjà", 10).expect("search")), ["a"]);
        assert_eq!(
            ids(&db.search("resume", 10).expect("search")),
            ["a"],
            "typed without accents, it must still be found"
        );
        assert_eq!(
            ids(&db.search("DÉJÀ", 10).expect("search")),
            ["a"],
            "and whatever the case"
        );
    }

    #[test]
    fn a_word_present_only_in_the_body_is_found() {
        // The criterion of phase 05, and the reason the body is indexed at all.
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text(
            "a",
            "Titre sans rapport",
            "il a parlé de photosynthèse",
        ))
        .expect("save");

        assert_eq!(ids(&db.search("photosynthèse", 10).expect("search")), ["a"]);
    }

    #[test]
    fn a_word_present_only_in_the_summary_is_found() {
        let db = Database::open_in_memory().expect("open");
        let fixture = with_text("a", "Titre", "le chat dort");
        db.save_transcript(&fixture).expect("save");
        db.save_output(&LlmOutput {
            id: "o".to_string(),
            transcript_id: "a".to_string(),
            kind: "summary".to_string(),
            provider: "ollama".to_string(),
            model: "llama3.1:8b".to_string(),
            prompt_version: 1,
            source_transcript_hash: fixture.transcript_hash(),
            generated_at: 0,
            content: "Une sieste prolongée.".to_string(),
        })
        .expect("save output");

        assert_eq!(
            ids(&db.search("sieste", 10).expect("search")),
            ["a"],
            "the trigger must have carried the summary into the index"
        );
    }

    #[test]
    fn deleting_a_summary_takes_it_out_of_the_index() {
        let db = Database::open_in_memory().expect("open");
        let fixture = with_text("a", "Titre", "le chat dort");
        db.save_transcript(&fixture).expect("save");
        db.save_output(&LlmOutput {
            id: "o".to_string(),
            transcript_id: "a".to_string(),
            kind: "summary".to_string(),
            provider: "ollama".to_string(),
            model: "llama3.1:8b".to_string(),
            prompt_version: 1,
            source_transcript_hash: fixture.transcript_hash(),
            generated_at: 0,
            content: "Une sieste prolongée.".to_string(),
        })
        .expect("save output");

        db.delete_output("a", "summary").expect("delete");
        assert!(db.search("sieste", 10).expect("search").is_empty());
    }

    #[test]
    fn a_correction_is_searchable_and_the_word_it_replaced_is_not() {
        // The index follows the *displayed* text, like the hash does.
        let db = Database::open_in_memory().expect("open");
        let mut fixture = with_text("a", "Titre", "le chat dort");
        db.save_transcript(&fixture).expect("save");
        assert_eq!(ids(&db.search("chat", 10).expect("search")), ["a"]);

        fixture.edits.push(Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        });
        db.refresh_text(&fixture).expect("refresh");

        assert_eq!(ids(&db.search("chien", 10).expect("search")), ["a"]);
        assert!(db.search("chat", 10).expect("search").is_empty());
    }

    #[test]
    fn renaming_a_transcript_reindexes_its_title() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text("a", "Ancien titre", "le chat dort"))
            .expect("save");

        db.set_title("a", "Entretien Marchand", true)
            .expect("title");
        assert_eq!(ids(&db.search("Marchand", 10).expect("search")), ["a"]);
        assert!(db.search("Ancien", 10).expect("search").is_empty());
    }

    #[test]
    fn a_deleted_transcript_leaves_the_index() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text("a", "Une", "le chat dort"))
            .expect("save");
        db.delete_transcript("a").expect("delete");

        assert!(db.search("chat", 10).expect("search").is_empty());
    }

    #[test]
    fn several_words_narrow_the_search_instead_of_widening_it() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text("a", "Une", "le chat dort"))
            .expect("save");
        db.save_transcript(&with_text("b", "Deux", "le chien court"))
            .expect("save");

        assert_eq!(ids(&db.search("chat dort", 10).expect("search")), ["a"]);
        assert!(db.search("chat court", 10).expect("search").is_empty());
    }

    #[test]
    fn a_prefix_matches_while_the_user_is_still_typing() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text("a", "Une", "photosynthèse chlorophyllienne"))
            .expect("save");

        assert_eq!(ids(&db.search("photo", 10).expect("search")), ["a"]);
    }

    #[test]
    fn a_title_hit_outranks_a_body_hit() {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&with_text(
            "body",
            "Sans rapport",
            "un mot sur Marchand ici",
        ))
        .expect("save");
        db.save_transcript(&with_text("title", "Marchand", "un texte quelconque"))
            .expect("save");

        assert_eq!(
            ids(&db.search("Marchand", 10).expect("search")),
            ["title", "body"]
        );
    }

    #[test]
    fn the_limit_is_honoured() {
        let db = Database::open_in_memory().expect("open");
        for id in ["a", "b", "c"] {
            db.save_transcript(&with_text(id, id, "le chat dort"))
                .expect("save");
        }
        assert_eq!(db.search("chat", 2).expect("search").len(), 2);
    }
}
