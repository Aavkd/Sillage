//! The corrections layer, and its projection onto the verbatim.
//!
//! CONCEPTION.md §3.5: the verbatim is immutable, corrections live above it and are anchored to
//! **stable word identifiers**. Editing therefore never renumbers anything, and « revenir au
//! verbatim » is emptying a list rather than restoring a backup.
//!
//! Phase 02 owns the data structure and the *text* projection, because `transcript_hash` is
//! defined on the displayed text. The editor itself — and the time interpolation of inserted
//! words — is phase 07; that is why [`DisplayedWord::start_ms`] is `None` on an inserted word
//! instead of carrying an invented interval.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{Ms, Segment, WordId};

/// Which side of its anchor an insertion lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditSide {
    Before,
    After,
}

/// A word created by a correction. It carries its own identifier so that a later correction can
/// in turn replace or delete it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsertedWord {
    pub id: WordId,
    pub text: String,
}

/// One correction.
///
/// The list is ordered and replayed in order. Two corrections touching the same word are not an
/// error: the **last** replacement or deletion wins, and insertions accumulate in list order.
/// This keeps the projection total — there is no combination of corrections that has no
/// displayed form, so the editor can never wedge a transcript into an unrenderable state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Edit {
    /// Replaces the text of an existing word, keeping its interval.
    Replace { word: WordId, text: String },
    /// Hides a word. Its interval stays available to its neighbours (CONCEPTION.md §3.5).
    Delete { word: WordId },
    /// Adds words beside an existing one.
    Insert {
        anchor: WordId,
        side: EditSide,
        words: Vec<InsertedWord>,
    },
}

impl Edit {
    /// The identifiers this correction brings into existence.
    pub fn inserted_ids(&self) -> impl Iterator<Item = WordId> + '_ {
        let inserted = match self {
            Self::Insert { words, .. } => Some(words.as_slice()),
            Self::Replace { .. } | Self::Delete { .. } => None,
        };
        inserted.unwrap_or(&[]).iter().map(|word| word.id)
    }
}

/// Where a displayed word comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WordOrigin {
    /// Straight from whisper, untouched.
    Verbatim,
    /// A verbatim word whose text was replaced; the interval is the original one.
    Corrected,
    /// Created by a correction; it has no interval of its own yet.
    Inserted,
}

/// A word as the reader sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayedWord {
    pub id: WordId,
    pub text: String,
    pub origin: WordOrigin,
    /// `None` on an inserted word — phase 07 interpolates it from the neighbours.
    pub start_ms: Option<Ms>,
    pub end_ms: Option<Ms>,
}

/// A segment as the reader sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayedSegment {
    /// Position of the segment in the verbatim; the projection never reorders or drops segments.
    pub index: usize,
    pub words: Vec<DisplayedWord>,
    /// Drives the « corrigé » marker of DESIGN.md §8.
    pub is_edited: bool,
}

/// Applies the corrections layer to the verbatim.
#[must_use]
pub fn project(segments: &[Segment], edits: &[Edit]) -> Vec<DisplayedSegment> {
    // `None` marks a deletion, `Some(text)` a replacement. Later corrections overwrite earlier
    // ones, which is what makes replaying the list from the start idempotent.
    let mut rewritten: HashMap<WordId, Option<&str>> = HashMap::new();
    let mut before: HashMap<WordId, Vec<&InsertedWord>> = HashMap::new();
    let mut after: HashMap<WordId, Vec<&InsertedWord>> = HashMap::new();

    for edit in edits {
        match edit {
            Edit::Replace { word, text } => {
                rewritten.insert(*word, Some(text.as_str()));
            }
            Edit::Delete { word } => {
                rewritten.insert(*word, None);
            }
            Edit::Insert {
                anchor,
                side,
                words,
            } => {
                let target = match side {
                    EditSide::Before => &mut before,
                    EditSide::After => &mut after,
                };
                target.entry(*anchor).or_default().extend(words.iter());
            }
        }
    }

    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let mut displayed = Vec::with_capacity(segment.words.len());
            let mut is_edited = false;

            for word in &segment.words {
                // An insertion anchored on a deleted word still shows: the anchor is a position,
                // not a dependency. Deleting a word one has just corrected must not silently
                // swallow the words typed next to it.
                for inserted in before.get(&word.id).into_iter().flatten() {
                    displayed.push(DisplayedWord {
                        id: inserted.id,
                        text: inserted.text.clone(),
                        origin: WordOrigin::Inserted,
                        start_ms: None,
                        end_ms: None,
                    });
                    is_edited = true;
                }

                match rewritten.get(&word.id) {
                    Some(None) => is_edited = true, // deleted: nothing to display
                    Some(Some(text)) => {
                        displayed.push(DisplayedWord {
                            id: word.id,
                            text: (*text).to_string(),
                            origin: WordOrigin::Corrected,
                            start_ms: Some(word.start_ms),
                            end_ms: Some(word.end_ms),
                        });
                        is_edited = true;
                    }
                    None => displayed.push(DisplayedWord {
                        id: word.id,
                        text: word.text.clone(),
                        origin: WordOrigin::Verbatim,
                        start_ms: Some(word.start_ms),
                        end_ms: Some(word.end_ms),
                    }),
                }

                for inserted in after.get(&word.id).into_iter().flatten() {
                    displayed.push(DisplayedWord {
                        id: inserted.id,
                        text: inserted.text.clone(),
                        origin: WordOrigin::Inserted,
                        start_ms: None,
                        end_ms: None,
                    });
                    is_edited = true;
                }
            }

            DisplayedSegment {
                index,
                words: displayed,
                is_edited,
            }
        })
        .collect()
}

/// Flattens a projection: one line per segment, words separated by a single space.
///
/// This canonical form is what gets hashed and what gets indexed, so the two can never disagree
/// about what the text of a transcript is.
#[must_use]
pub fn flatten(segments: &[DisplayedSegment]) -> String {
    segments
        .iter()
        .map(|segment| {
            segment
                .words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Transcript, Word};

    fn verbatim() -> Vec<Segment> {
        vec![Segment {
            start_ms: 0,
            end_ms: 3_000,
            text: "le chat dort".to_string(),
            words: vec![
                Word {
                    id: 0,
                    start_ms: 0,
                    end_ms: 1_000,
                    text: "le".to_string(),
                    prob: 0.9,
                },
                Word {
                    id: 1,
                    start_ms: 1_000,
                    end_ms: 2_000,
                    text: "chat".to_string(),
                    prob: 0.9,
                },
                Word {
                    id: 2,
                    start_ms: 2_000,
                    end_ms: 3_000,
                    text: "dort".to_string(),
                    prob: 0.9,
                },
            ],
        }]
    }

    #[test]
    fn without_corrections_the_projection_is_the_verbatim() {
        let projected = project(&verbatim(), &[]);
        assert_eq!(flatten(&projected), "le chat dort");
        assert!(!projected[0].is_edited);
        assert!(projected[0]
            .words
            .iter()
            .all(|word| word.origin == WordOrigin::Verbatim));
    }

    #[test]
    fn a_replacement_keeps_the_interval_of_the_word_it_replaces() {
        let edits = vec![Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        }];
        let projected = project(&verbatim(), &edits);

        assert_eq!(flatten(&projected), "le chien dort");
        assert!(projected[0].is_edited);

        let corrected = &projected[0].words[1];
        assert_eq!(corrected.origin, WordOrigin::Corrected);
        assert_eq!(corrected.start_ms, Some(1_000), "interval must be kept");
        assert_eq!(corrected.end_ms, Some(2_000));
    }

    #[test]
    fn a_deletion_removes_the_word_and_leaves_the_others_synchronised() {
        let projected = project(&verbatim(), &[Edit::Delete { word: 1 }]);

        assert_eq!(flatten(&projected), "le dort");
        assert_eq!(projected[0].words[0].start_ms, Some(0));
        assert_eq!(
            projected[0].words[1].start_ms,
            Some(2_000),
            "the surviving neighbours keep their own timings"
        );
    }

    #[test]
    fn insertions_land_on_the_requested_side() {
        let edits = vec![
            Edit::Insert {
                anchor: 0,
                side: EditSide::Before,
                words: vec![InsertedWord {
                    id: 10,
                    text: "alors".to_string(),
                }],
            },
            Edit::Insert {
                anchor: 2,
                side: EditSide::After,
                words: vec![
                    InsertedWord {
                        id: 11,
                        text: "encore".to_string(),
                    },
                    InsertedWord {
                        id: 12,
                        text: "aujourd'hui".to_string(),
                    },
                ],
            },
        ];

        let projected = project(&verbatim(), &edits);
        assert_eq!(flatten(&projected), "alors le chat dort encore aujourd'hui");

        let inserted = &projected[0].words[0];
        assert_eq!(inserted.origin, WordOrigin::Inserted);
        assert_eq!(
            inserted.start_ms, None,
            "phase 07 interpolates; phase 02 must not invent an interval"
        );
    }

    #[test]
    fn an_insertion_anchored_on_a_deleted_word_still_shows() {
        let edits = vec![
            Edit::Insert {
                anchor: 1,
                side: EditSide::After,
                words: vec![InsertedWord {
                    id: 10,
                    text: "chien".to_string(),
                }],
            },
            Edit::Delete { word: 1 },
        ];

        assert_eq!(flatten(&project(&verbatim(), &edits)), "le chien dort");
    }

    #[test]
    fn the_last_correction_on_a_word_wins() {
        let edits = vec![
            Edit::Replace {
                word: 1,
                text: "chien".to_string(),
            },
            Edit::Replace {
                word: 1,
                text: "cheval".to_string(),
            },
        ];
        assert_eq!(flatten(&project(&verbatim(), &edits)), "le cheval dort");
    }

    #[test]
    fn a_correction_anchored_on_an_unknown_word_is_ignored() {
        let edits = vec![Edit::Replace {
            word: 999,
            text: "jamais".to_string(),
        }];
        let projected = project(&verbatim(), &edits);

        assert_eq!(flatten(&projected), "le chat dort");
        assert!(!projected[0].is_edited);
    }

    #[test]
    fn emptying_the_layer_restores_the_verbatim_exactly() {
        // « Revenir au verbatim » (CONCEPTION.md §3.5) is exactly this: drop the corrections.
        let before = project(&verbatim(), &[]);
        let edited = project(
            &verbatim(),
            &[
                Edit::Delete { word: 0 },
                Edit::Replace {
                    word: 2,
                    text: "ronfle".to_string(),
                },
            ],
        );
        assert_ne!(before, edited);
        assert_eq!(project(&verbatim(), &[]), before);
    }

    #[test]
    fn segments_are_never_reordered_or_dropped() {
        let mut segments = verbatim();
        segments.push(Segment {
            start_ms: 3_000,
            end_ms: 3_500,
            text: "fin".to_string(),
            words: vec![Word {
                id: 3,
                start_ms: 3_000,
                end_ms: 3_500,
                text: "fin".to_string(),
                prob: 0.9,
            }],
        });

        let projected = project(&segments, &[Edit::Delete { word: 3 }]);
        assert_eq!(projected.len(), 2);
        assert_eq!(projected[1].index, 1);
        assert!(projected[1].words.is_empty());
        assert_eq!(flatten(&projected), "le chat dort\n");
    }

    #[test]
    fn the_edit_marker_only_lights_up_on_edited_segments() {
        let mut segments = verbatim();
        segments.push(Segment {
            start_ms: 3_000,
            end_ms: 3_500,
            text: "fin".to_string(),
            words: vec![Word {
                id: 3,
                start_ms: 3_000,
                end_ms: 3_500,
                text: "fin".to_string(),
                prob: 0.9,
            }],
        });

        let projected = project(
            &segments,
            &[Edit::Replace {
                word: 3,
                text: "terminé".to_string(),
            }],
        );
        assert!(!projected[0].is_edited);
        assert!(projected[1].is_edited);
    }

    #[test]
    fn a_transcript_projects_through_its_own_edits() {
        let transcript = Transcript {
            segments: verbatim(),
            edits: vec![Edit::Delete { word: 0 }],
            ..Transcript::default()
        };
        assert_eq!(transcript.displayed_text(), "chat dort");
    }
}
