//! Why an ingestion was refused, and what to say about it.
//!
//! CONCEPTION.md §8 lists the cases; ROADMAP phase 03 task 7 asks for « chacune avec un message
//! français actionnable ». Two rules follow from that, and the tests enforce both:
//!
//! - **Every message is distinct.** « Fichier illisible » on a silent video and on a truncated
//!   MP4 tells the user nothing about which of the two happened.
//! - **Every message says what to do next.** A message that only names the fault leaves the
//!   user staring at a card in the library with nothing to act on.
//!
//! Only the ffmpeg case carries a machine-generated fragment, and it goes through
//! [`translate`] first: raw ffmpeg output is English, terse and frequently meaningless to the
//! person who dropped the file.

use std::io;
use std::path::{Path, PathBuf};

use crate::library::LibraryError;

use super::formats;

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("Fichier introuvable : {}. Il a peut-être été déplacé ou supprimé depuis.", .0.display())]
    NotFound(PathBuf),

    #[error(
        "Ce fichier n'a pas d'extension, Sillage ne peut donc pas déterminer son format. \
         Renommez-le en lui ajoutant l'extension qui convient, puis redéposez-le."
    )]
    NoExtension,

    #[error(
        "Format non pris en charge : « .{extension} ». Sillage accepte les fichiers audio \
         ({audio}) et vidéo ({video}). Convertissez le fichier dans l'un de ces formats."
    )]
    UnsupportedFormat {
        extension: String,
        audio: String,
        video: String,
    },

    #[error(
        "Le fichier {} n'a pas fini d'être écrit : sa taille change encore après {seconds} s \
         d'attente. Attendez la fin de la copie ou du téléchargement, puis redéposez-le.",
        .path.display()
    )]
    NeverStable { path: PathBuf, seconds: u64 },

    #[error(
        "Fichier illisible : {detail} Le fichier est probablement incomplet ou endommagé ; \
         essayez d'en récupérer une autre copie."
    )]
    Unreadable { detail: String },

    #[error(
        "Ce fichier ne contient aucune piste audio — c'est une vidéo muette. Sillage ne \
         transcrit que du son ; vérifiez que vous avez déposé le bon fichier."
    )]
    NoAudioTrack,

    #[error(
        "Ce fichier ne contient aucun son : sa piste audio est vide. Vérifiez l'enregistrement \
         d'origine avant de le redéposer."
    )]
    EmptyMedia,

    #[error(
        "Ce fichier ne dure que {}, or Sillage a besoin d'au moins une seconde de son pour \
         transcrire. Déposez un enregistrement plus long.",
        format_short_duration(*.duration_ms)
    )]
    TooShort { duration_ms: u64 },

    #[error(
        "L'outil {tool} fourni avec Sillage est introuvable ({}). L'installation est \
         incomplète : réinstallez l'application.",
        .path.display()
    )]
    ToolMissing { tool: &'static str, path: PathBuf },

    #[error("L'outil {tool} fourni avec Sillage n'a pas pu être lancé : {source}.")]
    ToolFailed {
        tool: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("Erreur de lecture ou d'écriture pendant l'ingestion : {0}.")]
    Io(#[from] io::Error),

    /// The library refused the write. Phase 02's messages are sentence fragments by design, so
    /// they are embedded rather than shown raw — the user reads one sentence either way.
    #[error("La bibliothèque n'a pas pu enregistrer le fichier : {0}.")]
    Library(#[from] LibraryError),
}

impl IngestError {
    /// The « format non pris en charge » error, with the accepted lists filled in from the one
    /// place they are declared.
    #[must_use]
    pub fn unsupported(extension: &str) -> Self {
        Self::UnsupportedFormat {
            // Callers hand over whatever they had — `psd`, `.psd`, `.PSD`. The message reads
            // « .psd » either way rather than « ..PSD ».
            extension: extension.trim_start_matches('.').to_ascii_lowercase(),
            audio: formats::audio_extensions().join(", "),
            video: formats::video_extensions().join(", "),
        }
    }

    /// The « fichier illisible » error, built from what ffmpeg or ffprobe wrote to stderr.
    #[must_use]
    pub fn unreadable(stderr: &str) -> Self {
        Self::Unreadable {
            detail: translate(stderr),
        }
    }

    /// A short, stable identifier for this kind of refusal.
    ///
    /// The message is for the user and is free to be rephrased; this is what code compares
    /// against, so that a wording change never silently breaks a branch. It is also what the
    /// library card of phase 05 will key its icon off.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "fichier-introuvable",
            Self::NoExtension => "extension-absente",
            Self::UnsupportedFormat { .. } => "format-non-pris-en-charge",
            Self::NeverStable { .. } => "fichier-instable",
            Self::Unreadable { .. } => "fichier-illisible",
            Self::NoAudioTrack => "aucune-piste-audio",
            Self::EmptyMedia => "media-vide",
            Self::TooShort { .. } => "trop-court",
            Self::ToolMissing { .. } => "outil-manquant",
            Self::ToolFailed { .. } => "outil-en-echec",
            Self::Io(_) => "erreur-disque",
            Self::Library(_) => "erreur-bibliotheque",
        }
    }
}

/// Turns the tail of an ffmpeg/ffprobe stderr into plain French.
///
/// CONCEPTION.md §8: « Rejet à l'ingestion avec l'erreur ffmpeg traduite en langage clair ».
/// The table covers what a user actually hits; anything unrecognised falls through to the last
/// meaningful line ffmpeg printed, quoted as-is. Dropping the unknown case entirely would be
/// worse than showing English: a message with no detail cannot be searched for or reported.
#[must_use]
pub fn translate(stderr: &str) -> String {
    const TABLE: &[(&str, &str)] = &[
        (
            "moov atom not found",
            "l'index du fichier MP4 est absent, signe d'une copie ou d'un téléchargement \
             interrompu.",
        ),
        (
            "Invalid data found when processing input",
            "le contenu du fichier ne correspond pas à son format.",
        ),
        (
            "Format not recognised",
            "le format de ce fichier n'a pas été reconnu.",
        ),
        (
            "Invalid argument",
            "le fichier a été refusé par le décodeur.",
        ),
        (
            "Permission denied",
            "l'accès au fichier a été refusé par Windows.",
        ),
        (
            "No such file or directory",
            "le fichier n'existe plus à cet emplacement.",
        ),
        (
            "End of file",
            "le fichier se termine avant la fin annoncée par son en-tête.",
        ),
        (
            "Header missing",
            "l'en-tête du fichier est absent ou illisible.",
        ),
        (
            "could not find codec parameters",
            "le contenu de la piste n'a pas pu être identifié.",
        ),
        (
            "Output file does not contain any stream",
            "aucune piste exploitable n'a pu être extraite du fichier.",
        ),
    ];

    for (needle, french) in TABLE {
        if stderr.to_lowercase().contains(&needle.to_lowercase()) {
            return (*french).to_string();
        }
    }

    match last_meaningful_line(stderr) {
        Some(line) => format!("ffmpeg signale « {line} »."),
        None => "le décodeur n'a donné aucune explication.".to_string(),
    }
}

/// The last line worth showing: ffmpeg ends with the real cause, after a wall of banner lines.
fn last_meaningful_line(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("frame=") && !line.starts_with("size="))
        .next_back()
        .map(|line| {
            // A path in the message is noise, and can be long enough to blow up the card.
            let line = line.strip_prefix("Error").unwrap_or(line);
            let line = line.trim_start_matches([':', ' ']);
            truncate(line, 160)
        })
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{}…", kept.trim_end())
}

/// `0,4 s`, `950 ms` — for durations that are, by definition, under a second.
fn format_short_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms} ms")
    } else {
        format!("{},{} s", ms / 1_000, (ms % 1_000) / 100)
    }
}

/// Maps an `io::Error` on a source file to the error the user should read.
pub(super) fn from_source_io(path: &Path, error: io::Error) -> IngestError {
    if error.kind() == io::ErrorKind::NotFound {
        IngestError::NotFound(path.to_path_buf())
    } else {
        IngestError::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// One of every variant, so the properties below are checked on the whole taxonomy rather
    /// than on whichever variants a test author remembered.
    fn every_variant() -> Vec<IngestError> {
        vec![
            IngestError::NotFound(PathBuf::from(r"D:\Audio\perdu.m4a")),
            IngestError::NoExtension,
            IngestError::unsupported("psd"),
            IngestError::NeverStable {
                path: PathBuf::from(r"D:\Audio\en-cours.mp4"),
                seconds: 30,
            },
            IngestError::unreadable("moov atom not found"),
            IngestError::NoAudioTrack,
            IngestError::EmptyMedia,
            IngestError::TooShort { duration_ms: 420 },
            IngestError::ToolMissing {
                tool: "ffmpeg",
                path: PathBuf::from(r"C:\Sillage\resources\ffmpeg.exe"),
            },
            IngestError::ToolFailed {
                tool: "ffprobe",
                source: io::Error::new(io::ErrorKind::PermissionDenied, "refusé"),
            },
            IngestError::Io(io::Error::other("disque plein")),
            IngestError::Library(LibraryError::MissingTranscript("a".to_string())),
        ]
    }

    #[test]
    fn every_case_of_conception_8_has_its_own_message() {
        // ROADMAP phase 03: « rejetés, messages distincts ». Two cases sharing a message would
        // leave the user unable to tell a silent video from a corrupt one.
        let messages: HashSet<String> = every_variant().iter().map(ToString::to_string).collect();
        assert_eq!(messages.len(), every_variant().len());

        let kinds: HashSet<&str> = every_variant().iter().map(IngestError::kind).collect();
        assert_eq!(kinds.len(), every_variant().len());
    }

    #[test]
    fn every_message_is_in_french_and_says_what_to_do() {
        for error in every_variant() {
            let message = error.to_string();
            assert!(
                message.chars().next().is_some_and(char::is_uppercase),
                "message should read as a sentence: {message}"
            );
            assert!(
                message.ends_with('.') || message.ends_with('…'),
                "message should be a complete sentence: {message}"
            );
        }
    }

    #[test]
    fn the_refusals_the_user_can_act_on_tell_them_how() {
        // The four cases of CONCEPTION §8 that are the user's to fix. Disk and library failures
        // are excluded on purpose: there is nothing sensible to advise there.
        for error in [
            IngestError::NoExtension,
            IngestError::unsupported("psd"),
            IngestError::NoAudioTrack,
            IngestError::TooShort { duration_ms: 420 },
            IngestError::NeverStable {
                path: PathBuf::from("x.mp4"),
                seconds: 30,
            },
        ] {
            let message = error.to_string().to_lowercase();
            assert!(
                [
                    "renommez",
                    "convertissez",
                    "vérifiez",
                    "déposez",
                    "attendez"
                ]
                .iter()
                .any(|verb| message.contains(verb)),
                "no course of action in: {message}"
            );
        }
    }

    #[test]
    fn the_unsupported_format_message_lists_what_is_accepted() {
        let message = IngestError::unsupported(".PSD").to_string();
        assert!(message.contains("« .psd »"), "{message}");
        for accepted in ["mp3", "m4a", "wav", "flac", "ogg", "opus", "aac", "wma"] {
            assert!(
                message.contains(accepted),
                "{accepted} missing from {message}"
            );
        }
        for accepted in ["mp4", "mov", "mkv", "avi", "webm"] {
            assert!(
                message.contains(accepted),
                "{accepted} missing from {message}"
            );
        }
    }

    #[test]
    fn known_ffmpeg_failures_are_translated_rather_than_quoted() {
        assert!(translate("[mov,mp4] moov atom not found\n").contains("index du fichier MP4"));
        assert!(translate("Invalid data found when processing input").contains("ne correspond pas"));
        assert!(translate("Permission denied").contains("refusé par Windows"));
    }

    #[test]
    fn an_unknown_failure_still_shows_what_ffmpeg_said() {
        // Silently dropping it would leave a message with no detail at all — unsearchable and
        // unreportable. The English is a last resort, not the normal path.
        let detail = translate("something entirely new went wrong");
        assert!(
            detail.contains("something entirely new went wrong"),
            "{detail}"
        );
    }

    #[test]
    fn the_progress_lines_ffmpeg_prints_are_not_mistaken_for_the_cause() {
        let stderr = "Something failed early\nframe=  120 fps=0.0\nsize=    1024kB time=00:00:04";
        assert!(translate(stderr).contains("Something failed early"));
    }

    #[test]
    fn empty_output_still_produces_a_sentence() {
        assert!(translate("").contains("aucune explication"));
        assert!(translate("   \n\n  ").contains("aucune explication"));
    }

    #[test]
    fn a_very_long_ffmpeg_line_is_cut_rather_than_shown_whole() {
        let detail = translate(&"z".repeat(500));
        assert!(
            detail.chars().count() < 220,
            "{} chars",
            detail.chars().count()
        );
        assert!(detail.contains('…'));
    }

    #[test]
    fn a_sub_second_duration_reads_naturally() {
        assert_eq!(format_short_duration(0), "0 ms");
        assert_eq!(format_short_duration(420), "420 ms");
        assert_eq!(format_short_duration(999), "999 ms");
        // Only reachable if the threshold ever moves; still must not read as « 1, s ».
        assert_eq!(format_short_duration(1_400), "1,4 s");
    }

    #[test]
    fn a_missing_source_reads_as_missing_rather_than_as_a_disk_error() {
        let path = Path::new(r"D:\Audio\perdu.m4a");
        let missing = from_source_io(path, io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(missing.kind(), "fichier-introuvable");

        let other = from_source_io(path, io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(other.kind(), "erreur-disque");
    }
}
