//! Turning a file the user dropped into an entry of the library.
//!
//! CONCEPTION.md §3.2 gives the pipeline; ROADMAP phase 03 gives the order in which it has to
//! hold together. The order below is not arbitrary — each step is placed so that a refusal costs
//! as little as possible and, more importantly, so that **nothing is written before the file is
//! known to be usable**:
//!
//! ```text
//! 1. stabilisation   the file has stopped being written           CONCEPTION §8
//! 2. extension       a format Sillage accepts                     ROADMAP 03.2
//! 3. probe           it parses, and it has an audio track         CONCEPTION §8
//! 4. decode          16 kHz mono f32 in a stream → peaks          CONCEPTION §3.2
//! 5. duration        not empty, at least a second                 CONCEPTION §8
//! 6. sha256          duplicate of something already here?         CONCEPTION §8
//! 7. copy            media/<id>.<ext>                             CONCEPTION §3.4
//! 8. persist         peaks, record, index, queue
//! ```
//!
//! Steps 1 to 6 touch nothing. A file refused at step 5 leaves the library exactly as it was,
//! with no orphan copy in `media/` to clean up later — which is why the decode happens *before*
//! the copy even though it means reading the source twice.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod decode;
pub mod errors;
pub mod formats;
pub mod probe;
pub mod stabilize;
pub mod tools;

pub use decode::{Decoded, PcmSink, SAMPLE_RATE};
pub use errors::IngestError;
pub use formats::MediaKind;
pub use probe::Probe;
pub use stabilize::Stabilization;
pub use tools::{Tool, Tools};

use crate::db::TranscriptRow;
use crate::library::{Library, LibraryError, LibraryPaths};
use crate::model::peaks::{Peaks, PeaksBuilder, DEFAULT_BUCKET_MS};
use crate::model::Transcript;

/// Shortest recording Sillage will take. CONCEPTION.md §8: « Fichier de durée nulle ou < 1 s :
/// rejet explicite ».
pub const MINIMUM_DURATION_MS: u64 = 1_000;

/// Title given to a file whose name carries nothing usable.
const FALLBACK_TITLE: &str = "Sans titre";

/// What to do when the file is already in the library.
///
/// CONCEPTION.md §8 asks for a choice — « ouvrir l'existante » or « transcrire à nouveau » —
/// so the ingestion cannot decide alone. It stops at [`Ingested::Duplicate`] and is called again
/// with [`DuplicatePolicy::TranscribeAgain`] if that is what the user picked. Opening the
/// existing entry needs no ingestion at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DuplicatePolicy {
    /// Stop and report the entries that share the hash.
    #[default]
    Ask,
    /// Ingest anyway, as a second, independent entry.
    TranscribeAgain,
}

/// The outcome of an ingestion that did not fail.
#[derive(Debug, Clone, PartialEq)]
pub enum Ingested {
    /// The file is in the library, queued for transcription.
    Added(Box<Transcript>),
    /// The exact same bytes are already here.
    Duplicate {
        sha256: String,
        /// Newest first. More than one is possible: transcribing the same file twice is allowed.
        existing: Vec<TranscriptRow>,
    },
}

/// The ingestion pipeline.
#[derive(Debug, Clone)]
pub struct Ingestor {
    tools: Tools,
    stabilization: Stabilization,
    bucket_ms: u16,
}

impl Ingestor {
    #[must_use]
    pub fn new(tools: Tools) -> Self {
        Self {
            tools,
            stabilization: Stabilization::default(),
            bucket_ms: DEFAULT_BUCKET_MS,
        }
    }

    /// Overrides how long to wait for a file that is still being written.
    #[must_use]
    pub fn with_stabilization(mut self, stabilization: Stabilization) -> Self {
        self.stabilization = stabilization;
        self
    }

    #[must_use]
    pub fn tools(&self) -> &Tools {
        &self.tools
    }

    /// Runs the whole pipeline on one file.
    pub fn ingest(
        &self,
        library: &Library,
        source: &Path,
        policy: DuplicatePolicy,
    ) -> Result<Ingested, IngestError> {
        // Absolute from here on. ffmpeg's option parser has no `--` separator, so a relative path
        // beginning with a dash — `-mémo.wav` — would be read as an option and the file refused
        // for a reason that has nothing to do with it. An absolute Windows path starts with a
        // drive letter or a backslash and cannot be mistaken for one.
        let source = &absolute(source)?;

        let extension = supported_extension(source)?;

        stabilize::wait_until_stable(source, &self.stabilization)?;

        let probe = probe::probe(&self.tools, source.as_os_str())?;
        if !probe.has_audio() {
            return Err(IngestError::NoAudioTrack);
        }

        let (decoded, peaks) = self.decode_with_peaks(source)?;
        let duration_ms = decoded.duration_ms();
        if decoded.samples == 0 {
            return Err(IngestError::EmptyMedia);
        }
        if duration_ms < MINIMUM_DURATION_MS {
            return Err(IngestError::TooShort { duration_ms });
        }

        let sha256 = crate::model::hash::sha256_file(source)
            .map_err(|error| errors::from_source_io(source, error))?;
        let existing = library
            .db()
            .transcripts_with_sha256(&sha256)
            .map_err(LibraryError::from)?;
        if !existing.is_empty() && policy == DuplicatePolicy::Ask {
            return Ok(Ingested::Duplicate { sha256, existing });
        }

        let id = Transcript::new_id();
        let media_path = LibraryPaths::relative_media(&id, &extension);
        let transcript = Transcript {
            id: id.clone(),
            source_path: source.to_string_lossy().into_owned(),
            media_path: media_path.clone(),
            sha256,
            created_at: now_ms(),
            duration_ms,
            title: title_from(source),
            title_is_custom: false,
            // Language, model and the verbatim itself are phase 04's to fill in.
            ..Transcript::default()
        };

        // Everything above this line is read-only. From here on a failure has to undo what it
        // has already written, or the library keeps an orphan copy of a file it never ingested.
        let written = self.persist(library, source, &transcript, &peaks);
        if written.is_err() {
            let _ = std::fs::remove_file(library.paths().resolve(&media_path));
            let _ = std::fs::remove_file(library.paths().peaks(&id));
        }
        written?;

        Ok(Ingested::Added(Box::new(transcript)))
    }

    /// Decodes to PCM and folds the peaks in as the samples go past.
    ///
    /// CONCEPTION.md §3.2: the peaks are computed **during** the decode, from the PCM already in
    /// memory. The frontend never decodes the audio a second time to draw a waveform, and
    /// neither does this function — the samples are dropped as soon as they are counted.
    fn decode_with_peaks(&self, source: &Path) -> Result<(Decoded, Peaks), IngestError> {
        let mut builder = PeaksBuilder::new(SAMPLE_RATE, self.bucket_ms);
        let decoded = decode::decode(&self.tools, source.as_os_str(), &mut |chunk: &[f32]| {
            builder.push(chunk);
        })?;
        Ok((decoded, builder.finish()))
    }

    fn persist(
        &self,
        library: &Library,
        source: &Path,
        transcript: &Transcript,
        peaks: &Peaks,
    ) -> Result<(), IngestError> {
        // The copy first: a record pointing at audio that is not there is worse than audio with
        // no record, which is merely a file to delete.
        //
        // The copy reads `source`, not `transcript.source_path`: the latter went through
        // `to_string_lossy` for display, and a Windows path that is not valid UTF-16 would come
        // back with replacement characters and name a file that does not exist.
        let destination = library.paths().resolve(&transcript.media_path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, &destination)?;

        library.save_peaks(&transcript.id, peaks)?;
        library.save_transcript(transcript)?;
        library
            .db()
            .enqueue(&transcript.id, transcript.created_at)
            .map_err(LibraryError::from)?;
        Ok(())
    }
}

/// Makes `path` absolute against the current directory, without touching the file system.
///
/// `fs::canonicalize` is deliberately not used: on Windows it returns the `\\?\` extended-length
/// form, which is correct but is passed on to ffmpeg and shown to the user in the `source_path`
/// of the record.
fn absolute(path: &Path) -> Result<PathBuf, IngestError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// The extension of `source`, lowercase, if Sillage accepts it.
fn supported_extension(source: &Path) -> Result<String, IngestError> {
    let extension = source
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .ok_or(IngestError::NoExtension)?;

    if formats::is_supported(&extension) {
        Ok(extension)
    } else {
        Err(IngestError::unsupported(&extension))
    }
}

/// The provisional title: the file name without its extension.
///
/// Phase 08 replaces it with a generated one unless the user has edited it. Nothing clever
/// happens here on purpose — turning `entretien_marchand` into `Entretien Marchand` would be
/// inventing, and would be wrong the first time a file is named `re_2024_v2_FINAL`.
fn title_from(source: &Path) -> String {
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().trim().to_string())
        .unwrap_or_default();

    if stem.is_empty() {
        FALLBACK_TITLE.to_string()
    } else {
        stem
    }
}

/// Now, in epoch milliseconds. A clock before 1970 gives 0 rather than a panic.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Where the bundled binaries sit, relative to the Tauri resource directory.
///
/// Matches `bundle.resources` in `tauri.conf.json`, which is what puts them there. The two have
/// to agree, and there is no build-time check that they do — a mismatch shows up as an
/// « installation incomplète » on the first dropped file.
pub const RESOURCE_DIR: &str = "resources";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TranscriptStatus;

    #[test]
    fn a_relative_path_becomes_absolute_before_it_reaches_ffmpeg() {
        // ffmpeg has no `--` separator, so `-mémo.wav` handed over as-is is read as an option.
        let made = absolute(Path::new("-mémo.wav")).expect("absolute");
        assert!(made.is_absolute());
        assert!(made.ends_with("-mémo.wav"));

        let already = Path::new(r"D:\Audio\mémo.wav");
        assert_eq!(absolute(already).expect("absolute"), already);
    }

    #[test]
    fn every_accepted_extension_gets_through_the_gate() {
        for extension in formats::all_extensions() {
            let path = PathBuf::from(format!(r"D:\Audio\Entretien.{extension}"));
            assert_eq!(
                supported_extension(&path).expect("accepted"),
                extension,
                "{extension}"
            );
        }
    }

    #[test]
    fn the_extension_is_read_case_insensitively_and_stored_lowercase() {
        // Windows hands over whatever the user typed; `media/<id>.MP3` and `media/<id>.mp3`
        // must not be two different things.
        assert_eq!(
            supported_extension(Path::new(r"D:\Audio\Entretien.MP3")).expect("accepted"),
            "mp3"
        );
    }

    #[test]
    fn a_file_with_no_extension_is_refused_with_its_own_message() {
        let error = supported_extension(Path::new(r"D:\Audio\enregistrement")).expect_err("refuse");
        assert_eq!(error.kind(), "extension-absente");
    }

    #[test]
    fn an_unaccepted_extension_is_refused_and_the_message_names_it() {
        let error = supported_extension(Path::new(r"D:\Images\maquette.psd")).expect_err("refuse");
        assert_eq!(error.kind(), "format-non-pris-en-charge");
        assert!(error.to_string().contains("« .psd »"));
    }

    #[test]
    fn the_provisional_title_is_the_file_name_without_its_extension() {
        assert_eq!(
            title_from(Path::new(r"D:\Audio\Entretien Marchand.m4a")),
            "Entretien Marchand"
        );
        assert_eq!(title_from(Path::new(r"D:\Audio\  espacé  .mp3")), "espacé");
        assert_eq!(
            title_from(Path::new(r"D:\Audio\réunion du 12 août.mp3")),
            "réunion du 12 août"
        );
    }

    #[test]
    fn a_file_with_nothing_but_an_extension_still_gets_a_title() {
        // `.mp3` on its own: `file_stem` returns `.mp3`, and an empty title would render as a
        // blank card in the library.
        assert!(!title_from(Path::new(r"D:\Audio\.mp3")).is_empty());
        // A path with no file component at all — the only case that reaches the fallback.
        assert_eq!(title_from(Path::new(r"D:\")), FALLBACK_TITLE);
        assert_eq!(title_from(Path::new("")), FALLBACK_TITLE);
    }

    #[test]
    fn the_clock_is_in_epoch_milliseconds() {
        // 13 August 2026 is 1 786 000 000 000 ms; anything below that is not a real clock.
        assert!(now_ms() > 1_700_000_000_000);
    }

    #[test]
    fn a_transcript_created_by_ingestion_starts_queued_and_empty() {
        // Phase 04 fills in language, model and the verbatim. What matters here is that none of
        // them is invented: an empty model on the card is honest, `large-v3-turbo` would not be.
        let transcript = Transcript::default();
        assert_eq!(transcript.status, TranscriptStatus::Queued);
        assert_eq!(transcript.language, None);
        assert!(transcript.model.is_empty());
        assert!(transcript.segments.is_empty());
    }
}
