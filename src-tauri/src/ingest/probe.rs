//! Asking ffprobe what a file is, before trying to decode it.
//!
//! The probe answers the three questions the refusals of CONCEPTION.md §8 turn on, and it does
//! so from a **JSON contract** rather than from ffmpeg's prose:
//!
//! | Question | Refusal it drives |
//! |---|---|
//! | Does the container parse at all? | « Fichier illisible » |
//! | Is there an audio stream? | « Aucune piste audio » — the silent `.mp4` |
//!
//! The declared duration is reported but **nothing is refused on it**. Containers lie — a VBR
//! mp3 written without a Xing header, a stream copied without a rewrite — and the only honest
//! number is the count of samples that actually came out of the decoder ([`super::decode`]).
//! Refusing on the declared value would be cheaper by exactly one decode of a file that is, by
//! definition, under a second long; that is not a trade worth making a wrong refusal for.

use std::ffi::OsStr;

use serde::Deserialize;

use super::errors::IngestError;
use super::tools::{Tool, Tools};

/// What ffprobe reports about a media file.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// Duration the container declares, in milliseconds. `None` when it declares none — common
    /// in a raw stream, and not in itself a reason to refuse the file.
    pub declared_duration_ms: Option<u64>,
    /// Short format name, as ffprobe spells it (`mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm`, …).
    pub format: String,
    /// Codec of the first audio stream, if there is one.
    pub audio_codec: Option<String>,
    pub audio_streams: usize,
    pub video_streams: usize,
}

impl Probe {
    #[must_use]
    pub fn has_audio(&self) -> bool {
        self.audio_streams > 0
    }
}

/// The slice of ffprobe's JSON we depend on. Every field is optional: ffprobe omits what it
/// cannot determine, and a missing `duration` must not turn a readable file into an error.
#[derive(Debug, Deserialize)]
struct ProbeJson {
    #[serde(default)]
    streams: Vec<StreamJson>,
    #[serde(default)]
    format: Option<FormatJson>,
}

#[derive(Debug, Deserialize)]
struct StreamJson {
    #[serde(default)]
    codec_type: Option<String>,
    #[serde(default)]
    codec_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FormatJson {
    #[serde(default)]
    format_name: Option<String>,
    /// Seconds, as a decimal **string** — ffprobe emits `"726.151825"`, not a number.
    #[serde(default)]
    duration: Option<String>,
}

/// Runs ffprobe on `path`.
///
/// A non-zero exit, or output that does not parse, means the file is not readable: that is the
/// « fichier corrompu » branch of CONCEPTION.md §8, and ffprobe's own words go through
/// [`super::errors::translate`] on the way to the user.
pub fn probe(tools: &Tools, path: &OsStr) -> Result<Probe, IngestError> {
    let finished = tools.run(
        Tool::Ffprobe,
        &[
            // Errors only: a banner on stdout would break the JSON parse.
            OsStr::new("-v"),
            OsStr::new("error"),
            OsStr::new("-print_format"),
            OsStr::new("json"),
            OsStr::new("-show_format"),
            OsStr::new("-show_streams"),
            // The path comes in absolute (see `Ingestor::ingest`). It has to: ffmpeg's option
            // parser has **no `--` separator** — passing one is accepted and does nothing, and
            // the next argument is still read as an option if it starts with a dash. A file
            // named `-mémo.wav` would come back as « Unrecognized option 'mémo.wav' ».
            path,
        ],
    )?;

    if !finished.succeeded {
        return Err(IngestError::unreadable(&finished.stderr));
    }

    let parsed: ProbeJson = serde_json::from_slice(&finished.stdout).map_err(|_| {
        // ffprobe exited 0 but wrote something that is not the JSON it promised. Treating it as
        // readable would send a file we know nothing about into the decoder.
        IngestError::unreadable(&finished.stderr)
    })?;

    Ok(from_json(parsed))
}

fn from_json(parsed: ProbeJson) -> Probe {
    let count = |wanted: &str| {
        parsed
            .streams
            .iter()
            .filter(|stream| stream.codec_type.as_deref() == Some(wanted))
            .count()
    };

    let audio_codec = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .and_then(|stream| stream.codec_name.clone());

    let format = parsed
        .format
        .as_ref()
        .and_then(|format| format.format_name.clone())
        .unwrap_or_default();

    let declared_duration_ms = parsed
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(seconds_to_ms);

    Probe {
        declared_duration_ms,
        format,
        audio_codec,
        audio_streams: count("audio"),
        video_streams: count("video"),
    }
}

/// `"726.151825"` → `726_152`. `"N/A"`, a negative value or anything unparseable → `None`.
///
/// A negative duration is not a rounding artefact but a broken container; letting it through as
/// zero would refuse the file for the wrong reason, with the wrong message.
fn seconds_to_ms(raw: &str) -> Option<u64> {
    let seconds: f64 = raw.trim().parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::tools::tests::bundled;

    fn parse(json: &str) -> Probe {
        from_json(serde_json::from_str(json).expect("json"))
    }

    #[test]
    fn a_plain_audio_file_reads_as_one_audio_stream() {
        let probe = parse(
            r#"{"streams":[{"codec_type":"audio","codec_name":"mp3"}],
                "format":{"format_name":"mp3","duration":"726.151825"}}"#,
        );

        assert!(probe.has_audio());
        assert_eq!(probe.audio_streams, 1);
        assert_eq!(probe.video_streams, 0);
        assert_eq!(probe.audio_codec.as_deref(), Some("mp3"));
        assert_eq!(probe.declared_duration_ms, Some(726_152));
    }

    #[test]
    fn a_video_with_sound_reads_as_both() {
        let probe = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264"},
                           {"codec_type":"audio","codec_name":"aac"}],
                "format":{"format_name":"mov,mp4,m4a","duration":"12.000000"}}"#,
        );

        assert!(probe.has_audio());
        assert_eq!(probe.video_streams, 1);
        assert_eq!(probe.audio_codec.as_deref(), Some("aac"));
    }

    #[test]
    fn a_silent_video_reads_as_having_no_audio() {
        // The case the acceptance criterion names: a `.mp4` that must be refused, and refused
        // for this reason rather than as a corrupt file.
        let probe = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264"}],
                "format":{"format_name":"mov,mp4,m4a","duration":"12.0"}}"#,
        );

        assert!(!probe.has_audio());
        assert_eq!(probe.audio_codec, None);
        assert_eq!(probe.video_streams, 1);
    }

    #[test]
    fn a_container_declaring_no_duration_is_not_an_error() {
        let probe = parse(r#"{"streams":[{"codec_type":"audio"}],"format":{"format_name":"wav"}}"#);
        assert_eq!(probe.declared_duration_ms, None);
        assert!(probe.has_audio());
    }

    #[test]
    fn unusable_durations_read_as_absent_rather_than_as_zero() {
        // `None` sends the file to the decoder, which measures it honestly. Zero would refuse it
        // as « durée nulle » — the wrong message for a container that simply says nothing.
        for raw in ["N/A", "", "  ", "-1.0", "nan", "inf", "beaucoup"] {
            assert_eq!(seconds_to_ms(raw), None, "{raw:?}");
        }
        assert_eq!(seconds_to_ms("0.0"), Some(0));
        assert_eq!(seconds_to_ms(" 1.5 "), Some(1_500));
    }

    #[test]
    fn an_embedded_cover_image_does_not_make_a_file_a_video() {
        // A great many mp3 files carry their artwork as a video stream. The count is reported,
        // but nothing in the ingestion refuses a file for having one.
        let probe = parse(
            r#"{"streams":[{"codec_type":"audio","codec_name":"mp3"},
                           {"codec_type":"video","codec_name":"mjpeg"}],
                "format":{"format_name":"mp3","duration":"180.0"}}"#,
        );
        assert!(probe.has_audio());
        assert_eq!(probe.video_streams, 1);
    }

    #[test]
    fn empty_output_parses_into_an_empty_probe_rather_than_panicking() {
        let probe = parse("{}");
        assert!(!probe.has_audio());
        assert_eq!(probe.declared_duration_ms, None);
        assert_eq!(probe.format, "");
    }

    #[test]
    fn probing_a_file_that_is_not_media_is_refused_as_unreadable() {
        let Some(tools) = bundled() else { return };
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("faux.mp3");
        std::fs::write(&path, b"ceci n'est pas du son").expect("write");

        let error = probe(&tools, path.as_os_str()).expect_err("must refuse");
        assert_eq!(error.kind(), "fichier-illisible");
    }

    #[test]
    fn probing_a_missing_file_is_refused_not_panicked_on() {
        let Some(tools) = bundled() else { return };
        let error = probe(&tools, OsStr::new("aucun-fichier.m4a")).expect_err("must refuse");
        assert_eq!(error.kind(), "fichier-illisible");
    }
}
