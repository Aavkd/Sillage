//! Integration tests for phase 02, on a temporary folder cleaned up afterwards.
//!
//! The unit tests inside the crate check each piece; these check the acceptance criteria of
//! ROADMAP phase 02 end to end, against a real library folder on a real disk — the paths, the
//! WAL files, the atomic renames and the Windows file locking are all part of what is being
//! tested here, and none of them show up with an in-memory database.

use std::fs;

use sillage_lib::db::{Database, LlmOutput, QueueState};
use sillage_lib::library::{Library, LibraryPaths};
use sillage_lib::model::peaks::{Peaks, DEFAULT_BUCKET_MS};
use sillage_lib::model::{
    Edit, EditSide, InsertedWord, Segment, Transcript, TranscriptStatus, Word,
};

/// A complete transcript: two segments, word-level timings, probabilities, corrections and tags.
fn fixture(id: &str) -> Transcript {
    Transcript {
        id: id.to_string(),
        source_path: format!(r"D:\Enregistrements\{id}.m4a"),
        media_path: LibraryPaths::relative_media(id, "m4a"),
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        created_at: 1_755_000_000_000,
        duration_ms: 4_000,
        title: format!("Entretien {id}"),
        title_is_custom: false,
        language: Some("fr".to_string()),
        model: "large-v3-turbo".to_string(),
        status: TranscriptStatus::Done,
        error: None,
        segments: vec![
            Segment {
                start_ms: 0,
                end_ms: 2_000,
                text: "voici le résumé".to_string(),
                words: vec![
                    Word {
                        id: 0,
                        start_ms: 0,
                        end_ms: 700,
                        text: "voici".to_string(),
                        prob: 0.98,
                    },
                    Word {
                        id: 1,
                        start_ms: 700,
                        end_ms: 1_400,
                        text: "le".to_string(),
                        prob: 0.12,
                    },
                    Word {
                        id: 2,
                        start_ms: 1_400,
                        end_ms: 2_000,
                        text: "résumé".to_string(),
                        prob: 0.999_999,
                    },
                ],
            },
            Segment {
                start_ms: 2_000,
                end_ms: 4_000,
                text: "de la réunion déjà tenue".to_string(),
                words: vec![
                    Word {
                        id: 3,
                        start_ms: 2_000,
                        end_ms: 2_400,
                        text: "de".to_string(),
                        prob: 0.9,
                    },
                    Word {
                        id: 4,
                        start_ms: 2_400,
                        end_ms: 2_800,
                        text: "la".to_string(),
                        prob: 0.9,
                    },
                    Word {
                        id: 5,
                        start_ms: 2_800,
                        end_ms: 3_200,
                        text: "réunion".to_string(),
                        prob: 0.88,
                    },
                    Word {
                        id: 6,
                        start_ms: 3_200,
                        end_ms: 3_600,
                        text: "déjà".to_string(),
                        prob: 0.71,
                    },
                    Word {
                        id: 7,
                        start_ms: 3_600,
                        end_ms: 4_000,
                        text: "tenue".to_string(),
                        prob: 0.95,
                    },
                ],
            },
        ],
        edits: vec![
            Edit::Replace {
                word: 1,
                text: "un".to_string(),
            },
            Edit::Insert {
                anchor: 7,
                side: EditSide::After,
                words: vec![InsertedWord {
                    id: 8,
                    text: "hier".to_string(),
                }],
            },
        ],
        tags: vec!["client".to_string(), "réunion".to_string()],
        next_word_id: 9,
    }
}

fn library() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().expect("tempdir");
    let library = Library::open(dir.path().join("Sillage")).expect("open");
    (dir, library)
}

#[test]
fn migrations_apply_on_an_empty_and_on_an_existing_database_twice_over() {
    // ROADMAP phase 02, first criterion.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("library.db");

    let db = Database::open(&path).expect("first open, empty file");
    db.save_transcript(&fixture("a")).expect("save");
    db.close().expect("close");

    for pass in 1..=2 {
        let db = Database::open(&path).unwrap_or_else(|err| panic!("pass {pass}: {err}"));
        assert_eq!(
            db.count_transcripts().expect("count"),
            1,
            "pass {pass} must find the existing data"
        );
        db.close().expect("close");
    }
}

#[test]
fn a_transcript_round_trips_through_json_byte_for_byte() {
    // ROADMAP phase 02: « Un aller-retour transcription → JSON → transcription est exactement
    // identique ».
    let (_dir, library) = library();
    let original = fixture("a");

    library.save_transcript(&original).expect("save");
    let reloaded = library.load_transcript("a").expect("load");
    assert_eq!(reloaded, original);

    library.save_transcript(&reloaded).expect("save again");
    assert_eq!(
        fs::read_to_string(library.paths().transcript_json("a")).expect("read"),
        serde_json::to_string_pretty(&original).expect("serialize"),
        "writing the reloaded record produced different bytes"
    );
}

#[test]
fn the_verbatim_survives_corrections_and_can_be_restored() {
    // CONCEPTION.md §3.5: the verbatim is immutable, whatever the corrections layer says.
    let (_dir, library) = library();
    let original = fixture("a");
    library.save_transcript(&original).expect("save");

    let mut corrected = library.load_transcript("a").expect("load");
    assert_eq!(
        corrected.displayed_text(),
        "voici un résumé\nde la réunion déjà tenue hier"
    );

    corrected.edits.clear();
    assert_eq!(
        corrected.displayed_text(),
        "voici le résumé\nde la réunion déjà tenue",
        "clearing the layer gives the verbatim back"
    );
    assert_eq!(corrected.segments, original.segments);
}

#[test]
fn french_accents_are_searchable() {
    // ROADMAP phase 02: « Une recherche FTS5 sur un accent français (résumé, déjà) retourne le
    // bon résultat ».
    let (_dir, library) = library();
    library.save_transcript(&fixture("a")).expect("save");

    let mut other = fixture("b");
    other.segments = vec![Segment {
        start_ms: 0,
        end_ms: 500,
        text: "rien à voir".to_string(),
        words: vec![Word {
            id: 0,
            start_ms: 0,
            end_ms: 500,
            text: "rien".to_string(),
            prob: 0.9,
        }],
    }];
    other.edits.clear();
    other.title = "Autre".to_string();
    library.save_transcript(&other).expect("save");

    for query in ["résumé", "déjà", "resume", "DEJA", "réunion"] {
        let hits = library.db().search(query, 10).expect("search");
        let ids: Vec<_> = hits.iter().map(|hit| hit.row.id.as_str()).collect();
        assert_eq!(ids, ["a"], "query {query:?}");
    }
}

#[test]
fn the_transcript_hash_tracks_corrections_and_ignores_tags() {
    // ROADMAP phase 02: « transcript_hash change quand une correction change, ne change pas
    // quand un tag change ».
    let (_dir, library) = library();
    let mut transcript = fixture("a");
    library.save_transcript(&transcript).expect("save");

    let indexed = |library: &Library| {
        library
            .db()
            .transcript_row("a")
            .expect("read")
            .expect("present")
            .transcript_hash
    };
    let before = indexed(&library);
    assert_eq!(before, transcript.transcript_hash());

    // A generated output, produced from the text as it stands.
    library
        .db()
        .save_output(&LlmOutput {
            id: "o".to_string(),
            transcript_id: "a".to_string(),
            kind: "summary".to_string(),
            provider: "ollama".to_string(),
            model: "llama3.1:8b".to_string(),
            prompt_version: 1,
            source_transcript_hash: before.clone(),
            generated_at: 1_755_000_100_000,
            content: "Une réunion déjà tenue.".to_string(),
        })
        .expect("save output");
    assert!(library.db().stale_outputs("a").expect("stale").is_empty());

    // Tagging must not disturb anything.
    transcript.tags = vec!["client".to_string(), "urgent".to_string()];
    library.save_transcript(&transcript).expect("save tags");
    assert_eq!(indexed(&library), before, "a tag is not a correction");
    assert!(library.db().stale_outputs("a").expect("stale").is_empty());

    // Renaming must not either.
    library
        .db()
        .set_title("a", "Un autre titre", true)
        .expect("title");
    assert_eq!(indexed(&library), before);

    // Correcting must.
    transcript.edits.push(Edit::Replace {
        word: 5,
        text: "rencontre".to_string(),
    });
    library.save_transcript(&transcript).expect("save edit");
    let after = indexed(&library);
    assert_ne!(after, before);
    assert_eq!(after, transcript.transcript_hash());

    let stale = library.db().stale_outputs("a").expect("stale");
    assert_eq!(stale.len(), 1);
    assert_eq!(
        stale[0].content, "Une réunion déjà tenue.",
        "a stale output keeps its content"
    );
}

#[test]
fn moving_the_library_keeps_every_entry_the_audio_and_the_search() {
    // ROADMAP phase 02: « Le déplacement du dossier bibliothèque conserve toutes les entrées et
    // l'audio ».
    let dir = tempfile::tempdir().expect("tempdir");
    let library = Library::open(dir.path().join("avant")).expect("open");

    for id in ["a", "b", "c"] {
        let transcript = fixture(id);
        library.save_transcript(&transcript).expect("save");
        library
            .save_peaks(
                id,
                &Peaks {
                    bucket_ms: DEFAULT_BUCKET_MS,
                    values: vec![1, 2, 3, 4],
                },
            )
            .expect("peaks");
        fs::write(
            library.paths().resolve(&transcript.media_path),
            format!("audio de {id}"),
        )
        .expect("media");
    }
    library.db().enqueue("b", 1_000).expect("enqueue");
    library
        .db()
        .set_queue_state("b", QueueState::Running, Some(1_100), None)
        .expect("running");

    let before = library.db().list_transcripts().expect("list");
    let hits_before = library.db().search("résumé", 10).expect("search").len();

    let destination = dir.path().join("après");
    let moved = library.relocate(&destination).expect("relocate");

    assert!(!dir.path().join("avant").exists());
    assert_eq!(moved.db().list_transcripts().expect("list"), before);
    assert_eq!(
        moved.db().search("résumé", 10).expect("search").len(),
        hits_before,
        "the FTS index moved with the rest"
    );

    for id in ["a", "b", "c"] {
        let transcript = moved.load_transcript(id).expect("load");
        assert_eq!(transcript, fixture(id));
        assert_eq!(
            fs::read_to_string(moved.paths().resolve(&transcript.media_path)).expect("audio"),
            format!("audio de {id}"),
            "the audio must have followed"
        );
        assert_eq!(
            moved.load_peaks(id).expect("peaks").values,
            vec![1, 2, 3, 4]
        );
    }

    // The queue is part of the library, and it was mid-flight.
    let item = moved.db().queue_item("b").expect("read").expect("present");
    assert_eq!(item.state, QueueState::Running);
    assert_eq!(moved.db().requeue_interrupted().expect("requeue"), 1);
}

#[test]
fn everything_written_stays_inside_the_library_folder() {
    // ROADMAP §B: the application owns one folder and writes nowhere else.
    let (dir, library) = library();
    let transcript = fixture("a");
    library.save_transcript(&transcript).expect("save");
    library
        .save_peaks(
            "a",
            &Peaks {
                bucket_ms: DEFAULT_BUCKET_MS,
                values: vec![9; 64],
            },
        )
        .expect("peaks");
    fs::write(library.paths().resolve(&transcript.media_path), b"audio").expect("media");

    let root = dir.path().join("Sillage");
    let top_level: Vec<_> = fs::read_dir(dir.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(top_level, vec!["Sillage".to_string()]);

    assert!(root.join("library.db").is_file());
    assert!(root.join("media").join("a.m4a").is_file());
    assert!(root.join("data").join("a.json").is_file());
    assert!(root.join("data").join("a.peaks").is_file());
    assert!(root.join("outputs").is_dir());
}

#[test]
fn a_library_survives_a_process_that_never_closed_it() {
    // Observed on the built binary: Tauri ends the process without running the destructors of
    // its managed state, so the connection is never closed and the write-ahead log is never
    // checkpointed — `library.db` stays at its initial 4096 bytes with `user_version` at 0, and
    // everything written sits in `library.db-wal`.
    //
    // `mem::forget` reproduces exactly that. SQLite recovers the log at the next open, which is
    // what this asserts — and it is also the mechanism behind « fermeture pendant un traitement »
    // in CONCEPTION.md §8.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("Sillage");

    let library = Library::open(&root).expect("open");
    library.save_transcript(&fixture("a")).expect("save");
    std::mem::forget(library);

    assert!(
        root.join("library.db-wal").metadata().expect("wal").len() > 0,
        "the work must be in the log, since nothing checkpointed it"
    );

    let recovered = Library::open(&root).expect("reopen after an abrupt exit");
    assert_eq!(recovered.db().count_transcripts().expect("count"), 1);
    assert_eq!(recovered.load_transcript("a").expect("load"), fixture("a"));
    assert_eq!(
        recovered.db().search("résumé", 10).expect("search").len(),
        1,
        "the search index was recovered too"
    );
}

#[test]
fn a_library_survives_being_closed_and_reopened() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("Sillage");

    {
        let library = Library::open(&root).expect("open");
        library.save_transcript(&fixture("a")).expect("save");
        library.db().enqueue("a", 1_000).expect("enqueue");
    }

    let library = Library::open(&root).expect("reopen");
    assert_eq!(library.db().count_transcripts().expect("count"), 1);
    assert_eq!(library.load_transcript("a").expect("load"), fixture("a"));
    assert_eq!(library.db().queue().expect("queue").len(), 1);
    assert_eq!(
        library.db().tags_of("a").expect("tags"),
        vec!["client".to_string(), "réunion".to_string()]
    );
}
