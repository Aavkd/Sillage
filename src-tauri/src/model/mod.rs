//! The transcript record — the on-disk contract every later phase reads and writes.
//!
//! Two stores hold the same material on purpose (CONCEPTION.md §3.4):
//!
//! - `data/<id>.json` is the **complete** record: segments, words, probabilities, corrections.
//!   It is authoritative, and the database can be rebuilt from it.
//! - SQLite mirrors only what has to be *queried*: the library list, the tags, the full-text
//!   index, the queue.
//!
//! `transcript_hash` is deliberately **not** a stored field. It is derived from the displayed
//! text (verbatim + corrections) by [`Transcript::transcript_hash`], and only the database keeps
//! a copy, because that is where the staleness comparison of phase 08 happens. A stored copy in
//! the JSON could drift from the text it is supposed to describe; a derived one cannot.

use serde::{Deserialize, Serialize};

pub mod edits;
pub mod hash;
pub mod peaks;

pub use edits::{DisplayedSegment, DisplayedWord, Edit, EditSide, InsertedWord, WordOrigin};

/// Milliseconds from the start of the media.
pub type Ms = u64;

/// Identifier of a word, stable for the whole life of the transcript (CONCEPTION.md §3.5).
///
/// Verbatim words take `0..n` in reading order. Words inserted by a correction take identifiers
/// above every existing one, so an identifier is never reused and a correction never renumbers
/// its neighbours.
pub type WordId = u32;

/// Where a transcript stands in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptStatus {
    /// Ingested, waiting for its turn in the sequential queue.
    #[default]
    Queued,
    /// Whisper is running on it right now (only ever one, CONCEPTION.md §3.3).
    Running,
    /// Transcribed.
    Done,
    /// Ingestion or transcription failed; `Transcript::error` carries the French message.
    Failed,
}

impl TranscriptStatus {
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

/// One word of the verbatim, with its DTW-derived interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub id: WordId,
    pub start_ms: Ms,
    pub end_ms: Ms,
    pub text: String,
    /// Whisper's probability for the word, `0.0..=1.0` — drives the low-confidence marking of
    /// DESIGN.md §8.
    pub prob: f32,
}

/// One segment of the verbatim.
///
/// `start_ms` / `end_ms` are stored, but phase 04 must fill them from the **words**, never from
/// whisper's own segment timestamps: those are wrong after a silence, by up to 16,6 s measured
/// in the spike (spike/RESULTS.md §4.2). [`Segment::bounds_from_words`] is the derivation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: Ms,
    pub end_ms: Ms,
    pub text: String,
    pub words: Vec<Word>,
}

impl Segment {
    /// The interval actually covered by the words, or `None` for a wordless segment.
    #[must_use]
    pub fn bounds_from_words(&self) -> Option<(Ms, Ms)> {
        let first = self.words.first()?;
        let last = self.words.last()?;
        Some((first.start_ms, last.end_ms))
    }
}

/// A transcript, whole.
///
/// `Default` is derived rather than written out: it is only ever used to fill in the fields a
/// stored record does not carry, so « the default of each field » is exactly the right answer.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Transcript {
    pub id: String,
    /// Where the file came from, as the user gave it. Kept for display only.
    pub source_path: String,
    /// Copy inside the library, **relative** to the library root (`media/<id>.<ext>`), so that
    /// moving the library folder does not invalidate a single row.
    pub media_path: String,
    pub sha256: String,
    /// Epoch milliseconds.
    pub created_at: i64,
    pub duration_ms: Ms,
    pub title: String,
    /// Set as soon as the user edits the title; the generator never overwrites it afterwards
    /// (CONCEPTION.md §2, phase 08 task 7).
    pub title_is_custom: bool,
    /// Detected or forced; `None` until whisper has decided.
    pub language: Option<String>,
    pub model: String,
    pub status: TranscriptStatus,
    /// French, actionable message when `status` is `Failed`.
    pub error: Option<String>,
    /// The verbatim. Immutable once written — corrections live in `edits` (CONCEPTION.md §3.5).
    pub segments: Vec<Segment>,
    /// The corrections layer, applied over the verbatim to produce the displayed text.
    pub edits: Vec<Edit>,
    pub tags: Vec<String>,
    /// Next free word identifier. Kept explicitly so that deleting the last word of a transcript
    /// cannot make the allocator hand out an identifier that a correction already refers to.
    pub next_word_id: WordId,
}

impl Transcript {
    /// A fresh identifier: UUID v7, so identifiers sort by creation time and the `media/` and
    /// `data/` folders stay browsable in order.
    #[must_use]
    pub fn new_id() -> String {
        uuid::Uuid::now_v7().to_string()
    }

    /// The text as the reader sees it: verbatim with the corrections applied.
    #[must_use]
    pub fn displayed(&self) -> Vec<DisplayedSegment> {
        edits::project(&self.segments, &self.edits)
    }

    /// The displayed text, flattened — one line per segment, words separated by a single space.
    ///
    /// This is what feeds the full-text index and what [`Self::transcript_hash`] hashes.
    #[must_use]
    pub fn displayed_text(&self) -> String {
        edits::flatten(&self.displayed())
    }

    /// Hash of the **displayed** text (ROADMAP phase 02: not the verbatim alone).
    ///
    /// Compared against `llm_outputs.source_transcript_hash` to raise the `OBSOLÈTE` badge of
    /// phase 08. Tags, titles and statuses are outside it by construction: only the text the
    /// model was given can make a generated output stale.
    #[must_use]
    pub fn transcript_hash(&self) -> String {
        hash::transcript_hash(&self.displayed_text())
    }

    /// Hands out a word identifier that no correction can already be referring to.
    pub fn allocate_word_id(&mut self) -> WordId {
        let id = self.next_word_id;
        self.next_word_id = self.next_word_id.saturating_add(1);
        id
    }

    /// Raises the allocator above every identifier in use.
    ///
    /// Belt and braces for records written by an earlier version, or assembled by hand in a
    /// test: an allocator that lags behind would hand out a duplicate identifier and silently
    /// attach a correction to the wrong word.
    pub fn reseat_word_allocator(&mut self) {
        let highest = self
            .segments
            .iter()
            .flat_map(|segment| segment.words.iter())
            .map(|word| word.id)
            .chain(self.edits.iter().flat_map(Edit::inserted_ids))
            .max();

        if let Some(highest) = highest {
            self.next_word_id = self.next_word_id.max(highest.saturating_add(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record with every field populated, corrections included — the fixture the round-trip
    /// criterion of ROADMAP phase 02 needs.
    pub(crate) fn sample() -> Transcript {
        Transcript {
            id: "0192f3a0-0000-7000-8000-000000000001".to_string(),
            source_path: r"D:\Enregistrements\Entretien Marchand.m4a".to_string(),
            media_path: "media/0192f3a0-0000-7000-8000-000000000001.m4a".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            created_at: 1_755_000_000_000,
            duration_ms: 726_000,
            title: "Entretien Marchand".to_string(),
            title_is_custom: true,
            language: Some("fr".to_string()),
            model: "large-v3-turbo".to_string(),
            status: TranscriptStatus::Done,
            error: None,
            segments: vec![
                Segment {
                    start_ms: 0,
                    end_ms: 2_400,
                    text: "Bonjour, déjà merci".to_string(),
                    words: vec![
                        Word {
                            id: 0,
                            start_ms: 0,
                            end_ms: 800,
                            text: "Bonjour,".to_string(),
                            prob: 0.98,
                        },
                        Word {
                            id: 1,
                            start_ms: 800,
                            end_ms: 1_600,
                            text: "déjà".to_string(),
                            prob: 0.1,
                        },
                        Word {
                            id: 2,
                            start_ms: 1_600,
                            end_ms: 2_400,
                            text: "merci".to_string(),
                            prob: 0.999_999,
                        },
                    ],
                },
                Segment {
                    start_ms: 2_400,
                    end_ms: 4_000,
                    text: "pour le résumé".to_string(),
                    words: vec![
                        Word {
                            id: 3,
                            start_ms: 2_400,
                            end_ms: 3_000,
                            text: "pour".to_string(),
                            prob: 0.91,
                        },
                        Word {
                            id: 4,
                            start_ms: 3_000,
                            end_ms: 3_400,
                            text: "le".to_string(),
                            prob: 0.87,
                        },
                        Word {
                            id: 5,
                            start_ms: 3_400,
                            end_ms: 4_000,
                            text: "résumé".to_string(),
                            prob: 0.76,
                        },
                    ],
                },
            ],
            edits: vec![Edit::Replace {
                word: 1,
                text: "bien".to_string(),
            }],
            tags: vec!["client".to_string(), "entretien".to_string()],
            next_word_id: 6,
        }
    }

    #[test]
    fn json_round_trip_is_exact() {
        let original = sample();
        let json = serde_json::to_string_pretty(&original).expect("serialize");
        let parsed: Transcript = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed, original, "record differs after a round trip");
        assert_eq!(
            serde_json::to_string_pretty(&parsed).expect("re-serialize"),
            json,
            "re-serializing the parsed record produced different bytes"
        );
    }

    #[test]
    fn an_absent_field_falls_back_to_its_default() {
        // Every field carries `#[serde(default)]`, so a record written before a field existed
        // still loads. Losing a whole library to one added field is not an option.
        let parsed: Transcript = serde_json::from_str(r#"{"id":"x"}"#).expect("deserialize");
        assert_eq!(parsed.id, "x");
        assert_eq!(parsed.status, TranscriptStatus::Queued);
        assert!(parsed.segments.is_empty());
    }

    #[test]
    fn statuses_survive_the_trip_through_sql() {
        for status in [
            TranscriptStatus::Queued,
            TranscriptStatus::Running,
            TranscriptStatus::Done,
            TranscriptStatus::Failed,
        ] {
            assert_eq!(TranscriptStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(TranscriptStatus::parse("terminé"), None);
    }

    #[test]
    fn segment_bounds_come_from_the_words() {
        let transcript = sample();
        assert_eq!(
            transcript.segments[0].bounds_from_words(),
            Some((0, 2_400)),
            "bounds must be derived from the first and last word"
        );

        let empty = Segment {
            start_ms: 0,
            end_ms: 100,
            text: String::new(),
            words: Vec::new(),
        };
        assert_eq!(empty.bounds_from_words(), None);
    }

    #[test]
    fn identifiers_are_unique_and_time_ordered() {
        let first = Transcript::new_id();
        let second = Transcript::new_id();
        assert_ne!(first, second);
        assert!(first < second, "v7 identifiers must sort by creation time");
    }

    #[test]
    fn the_allocator_never_hands_out_an_identifier_in_use() {
        let mut transcript = sample();
        transcript.next_word_id = 0;
        transcript.reseat_word_allocator();

        assert_eq!(transcript.next_word_id, 6, "highest verbatim id is 5");
        assert_eq!(transcript.allocate_word_id(), 6);
        assert_eq!(transcript.allocate_word_id(), 7);
    }

    #[test]
    fn the_allocator_also_clears_identifiers_created_by_corrections() {
        let mut transcript = sample();
        transcript.edits.push(Edit::Insert {
            anchor: 2,
            side: EditSide::After,
            words: vec![InsertedWord {
                id: 40,
                text: "beaucoup".to_string(),
            }],
        });
        transcript.next_word_id = 6;
        transcript.reseat_word_allocator();

        assert_eq!(transcript.next_word_id, 41);
    }
}
