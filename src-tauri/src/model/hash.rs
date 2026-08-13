//! The two hashes the library relies on.
//!
//! - [`sha256_file`] identifies a **media file**: it is what makes the duplicate detection of
//!   CONCEPTION.md §8 possible, and it is computed in a stream because phase 03 has to accept
//!   a two-hour file without holding it in memory.
//! - [`transcript_hash`] identifies the **text**: comparing it against
//!   `llm_outputs.source_transcript_hash` is the whole staleness mechanism of phase 08.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// 1 MiB — large enough that the syscall cost disappears, small enough that peak memory stays
/// flat whatever the file size.
const CHUNK: usize = 1024 * 1024;

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // `write!` on a String cannot fail, so formatting by hand avoids a pointless Result.
        out.push(char::from_digit(u32::from(byte >> 4), 16).expect("nibble"));
        out.push(char::from_digit(u32::from(byte & 0x0F), 16).expect("nibble"));
    }
    out
}

/// SHA-256 of a byte slice, lowercase hex.
#[must_use]
pub fn sha256_bytes(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// SHA-256 of a file, read in a stream, lowercase hex.
pub fn sha256_file(path: impl AsRef<Path>) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(to_hex(&hasher.finalize()))
}

/// Hash of the **displayed** text of a transcript.
///
/// Plain SHA-256 of the canonical form produced by [`crate::model::edits::flatten`] — no salt,
/// no version prefix. A version prefix would look prudent and would in fact be harmful: changing
/// it would mark every generated output in the library stale at once, for a text that never
/// moved.
#[must_use]
pub fn transcript_hash(displayed_text: &str) -> String {
    sha256_bytes(displayed_text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edit, Segment, Transcript, Word};

    fn transcript() -> Transcript {
        Transcript {
            id: "t".to_string(),
            title: "Entretien".to_string(),
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
                        prob: 0.9,
                    },
                    Word {
                        id: 1,
                        start_ms: 700,
                        end_ms: 1_400,
                        text: "chat".to_string(),
                        prob: 0.9,
                    },
                    Word {
                        id: 2,
                        start_ms: 1_400,
                        end_ms: 2_000,
                        text: "dort".to_string(),
                        prob: 0.9,
                    },
                ],
            }],
            next_word_id: 3,
            ..Transcript::default()
        }
    }

    #[test]
    fn sha256_matches_the_published_vectors() {
        // The two vectors everyone can check against: the empty string, and "abc".
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hashing_a_file_gives_the_same_answer_as_hashing_its_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("media.bin");

        // Over the 1 MiB chunk, so the streaming loop really runs more than once.
        let payload: Vec<u8> = (0..3_000_000_u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &payload).expect("write");

        assert_eq!(sha256_file(&path).expect("hash"), sha256_bytes(&payload));
    }

    #[test]
    fn hashing_a_missing_file_is_an_error_not_a_panic() {
        assert!(sha256_file("does-not-exist.m4a").is_err());
    }

    #[test]
    fn a_correction_moves_the_transcript_hash() {
        let plain = transcript();
        let mut corrected = transcript();
        corrected.edits.push(Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        });

        assert_ne!(plain.transcript_hash(), corrected.transcript_hash());
    }

    #[test]
    fn changing_a_correction_moves_the_transcript_hash_again() {
        let mut first = transcript();
        first.edits.push(Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        });
        let mut second = transcript();
        second.edits.push(Edit::Replace {
            word: 1,
            text: "cheval".to_string(),
        });

        assert_ne!(first.transcript_hash(), second.transcript_hash());
    }

    #[test]
    fn a_tag_does_not_move_the_transcript_hash() {
        // ROADMAP phase 02 states it explicitly: tagging must never mark a generated output
        // stale. Nor must renaming, or the queue state.
        let plain = transcript();
        let mut tagged = transcript();
        tagged.tags = vec!["client".to_string()];
        tagged.title = "Un autre titre".to_string();
        tagged.title_is_custom = true;
        tagged.status = crate::model::TranscriptStatus::Done;

        assert_eq!(plain.transcript_hash(), tagged.transcript_hash());
    }

    #[test]
    fn reverting_to_the_verbatim_restores_the_original_hash() {
        let original = transcript().transcript_hash();

        let mut edited = transcript();
        edited.edits.push(Edit::Delete { word: 0 });
        assert_ne!(edited.transcript_hash(), original);

        edited.edits.clear();
        assert_eq!(edited.transcript_hash(), original);
    }

    #[test]
    fn the_hash_covers_the_displayed_text_and_nothing_else() {
        let transcript = transcript();
        assert_eq!(
            transcript.transcript_hash(),
            transcript_hash(&transcript.displayed_text())
        );
        assert_eq!(transcript.displayed_text(), "le chat dort");
    }
}
