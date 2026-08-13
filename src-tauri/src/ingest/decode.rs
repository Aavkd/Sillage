//! Decoding any accepted file to the PCM whisper expects, in a stream.
//!
//! CONCEPTION.md §3.2 fixes the target: **16 kHz mono f32**. ROADMAP phase 03 task 3 adds the
//! constraint that matters — « en flux, sans charger l'intégralité en mémoire ». Two hours of
//! 16 kHz mono f32 is 460 Mo on its own, against a budget of 500 Mo of RSS for the whole
//! ingestion, so holding the decoded audio is not an option that merely costs memory: it is one
//! that fails the phase.
//!
//! Hence [`PcmSink`]: the decoder hands out chunks and keeps none. Phase 03 folds them into a
//! [`crate::model::peaks::PeaksBuilder`]; phase 04 will feed the same stream to whisper, without
//! this module changing.
//!
//! The **honest duration of a media file is what comes out of here**, not what the container
//! declares. A VBR mp3 written without a Xing header, or a stream copied without a rewrite, will
//! happily claim a length it does not have.

use std::ffi::OsStr;
use std::io::Read;
use std::process::Stdio;

use super::errors::IngestError;
use super::tools::{Tool, Tools};

/// The rate whisper.cpp works at. Nothing downstream resamples, so this is not a preference.
pub const SAMPLE_RATE: u32 = 16_000;

/// How much is read from ffmpeg at a time. 64 Ko is 16 384 samples — about a second of audio,
/// small enough that peak memory is flat and large enough that the syscall cost disappears.
const READ_BUFFER: usize = 64 * 1024;

/// Cap on what is kept from ffmpeg's stderr. A file that fails on every frame can produce
/// megabytes of it, and only the tail ever reaches the user.
const STDERR_CAP: usize = 64 * 1024;

/// Anything that consumes decoded PCM as it arrives.
///
/// Implemented for every `FnMut(&[f32])`, which covers both the peak builder of this phase and
/// whatever phase 04 needs to hand whisper.
pub trait PcmSink {
    fn accept(&mut self, samples: &[f32]);
}

impl<F: FnMut(&[f32])> PcmSink for F {
    fn accept(&mut self, samples: &[f32]) {
        self(samples);
    }
}

/// What a decode measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    /// Mono samples produced, at [`SAMPLE_RATE`].
    pub samples: u64,
}

impl Decoded {
    /// The duration to record — derived from the samples, not from the container.
    #[must_use]
    pub fn duration_ms(self) -> u64 {
        self.samples * 1_000 / u64::from(SAMPLE_RATE)
    }
}

/// Decodes `path` to 16 kHz mono f32, handing every chunk to `sink`.
///
/// Video files have their audio track extracted (`-map 0:a:0`), which is decision #21 of
/// CONCEPTION.md §2 and the reason a `.mkv` is accepted at all.
pub fn decode(
    tools: &Tools,
    path: &OsStr,
    sink: &mut impl PcmSink,
) -> Result<Decoded, IngestError> {
    let mut child = tools
        .command(Tool::Ffmpeg)?
        .args([
            // No interactive keys, no banner, no progress: stderr must carry errors only.
            OsStr::new("-nostdin"),
            OsStr::new("-hide_banner"),
            OsStr::new("-loglevel"),
            OsStr::new("error"),
            OsStr::new("-i"),
            path,
            // Drop the picture, take the first audio track, mono, 16 kHz, raw float, to stdout.
            OsStr::new("-vn"),
            OsStr::new("-map"),
            OsStr::new("0:a:0"),
            OsStr::new("-ac"),
            OsStr::new("1"),
            OsStr::new("-ar"),
            OsStr::new("16000"),
            OsStr::new("-f"),
            OsStr::new("f32le"),
            OsStr::new("-"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| IngestError::ToolFailed {
            tool: Tool::Ffmpeg.name(),
            source,
        })?;

    // stderr is drained on its own thread. Reading the two pipes in sequence deadlocks the
    // moment ffmpeg fills the one that is not being read — which is exactly what a file that
    // errors on every frame does.
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stderr_reader = std::thread::spawn(move || {
        let mut collected = Vec::new();
        let mut buffer = [0_u8; 8 * 1024];
        while let Ok(read) = stderr_pipe.read(&mut buffer) {
            if read == 0 {
                break;
            }
            if collected.len() < STDERR_CAP {
                collected.extend_from_slice(&buffer[..read]);
            }
        }
        String::from_utf8_lossy(&collected).into_owned()
    });

    let mut stdout = child.stdout.take().expect("stdout was piped");
    let samples = read_samples(&mut stdout, sink);

    // The child is waited on whatever happened above: leaving it running would hold a handle on
    // the source file, and the next step of the ingestion wants to copy that file.
    let status = child.wait();
    let stderr = stderr_reader.join().unwrap_or_default();
    let samples = samples?;

    match status {
        Ok(status) if status.success() => Ok(Decoded { samples }),
        Ok(_) => Err(IngestError::unreadable(&stderr)),
        Err(source) => Err(IngestError::ToolFailed {
            tool: Tool::Ffmpeg.name(),
            source,
        }),
    }
}

/// Reads little-endian f32 from `source`, handing whole samples to `sink`.
///
/// A read can stop part-way through a sample, so the leftover bytes are carried into the next
/// round; converting only whole groups of four and discarding the rest would drop a sample every
/// time a read landed unaligned, and the waveform would drift against the audio.
fn read_samples(source: &mut impl Read, sink: &mut impl PcmSink) -> Result<u64, IngestError> {
    let mut raw = vec![0_u8; READ_BUFFER];
    let mut carry: Vec<u8> = Vec::with_capacity(4);
    let mut samples: Vec<f32> = Vec::with_capacity(READ_BUFFER / 4 + 1);
    let mut total = 0_u64;

    loop {
        let read = source.read(&mut raw)?;
        if read == 0 {
            break;
        }

        let mut bytes = &raw[..read];
        samples.clear();

        if !carry.is_empty() {
            let take = (4 - carry.len()).min(bytes.len());
            carry.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if carry.len() == 4 {
                samples.push(f32::from_le_bytes([carry[0], carry[1], carry[2], carry[3]]));
                carry.clear();
            }
        }

        let whole = bytes.len() - bytes.len() % 4;
        samples.extend(
            bytes[..whole]
                .chunks_exact(4)
                .map(|group| f32::from_le_bytes([group[0], group[1], group[2], group[3]])),
        );
        carry.extend_from_slice(&bytes[whole..]);

        total += samples.len() as u64;
        sink.accept(&samples);
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::tools::tests::bundled;
    use crate::model::peaks::{PeaksBuilder, DEFAULT_BUCKET_MS};

    /// A reader that hands out its bytes in fixed slices, to force unaligned reads.
    struct Dribble<'a> {
        bytes: &'a [u8],
        step: usize,
    }

    impl Read for Dribble<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let take = self.step.min(buffer.len()).min(self.bytes.len());
            buffer[..take].copy_from_slice(&self.bytes[..take]);
            self.bytes = &self.bytes[take..];
            Ok(take)
        }
    }

    fn le_bytes(samples: &[f32]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn samples_survive_reads_that_land_between_them() {
        // The failure this guards against is silent: a sample dropped at every unaligned read
        // shortens the waveform a little, and it drifts against the audio without ever erroring.
        let expected: Vec<f32> = (0..1_000).map(|i| (i as f32) / 1_000.0).collect();
        let bytes = le_bytes(&expected);

        for step in [1, 2, 3, 4, 5, 7, 13, 4_000] {
            let mut collected: Vec<f32> = Vec::new();
            let mut source = Dribble {
                bytes: &bytes,
                step,
            };
            let total = read_samples(&mut source, &mut |chunk: &[f32]| {
                collected.extend_from_slice(chunk)
            })
            .expect("read");

            assert_eq!(total, expected.len() as u64, "step {step}");
            assert_eq!(collected, expected, "step {step}");
        }
    }

    #[test]
    fn a_stream_cut_mid_sample_keeps_every_whole_sample_before_it() {
        let mut bytes = le_bytes(&[1.0, 2.0, 3.0]);
        bytes.truncate(bytes.len() - 2); // two whole samples, then half of a third

        let mut collected: Vec<f32> = Vec::new();
        let mut source = Dribble {
            bytes: &bytes,
            step: 64,
        };
        let total = read_samples(&mut source, &mut |chunk: &[f32]| {
            collected.extend_from_slice(chunk)
        })
        .expect("read");

        assert_eq!(total, 2);
        assert_eq!(collected, vec![1.0, 2.0]);
    }

    #[test]
    fn an_empty_stream_yields_nothing_rather_than_failing() {
        let mut collected: Vec<f32> = Vec::new();
        let mut source = Dribble {
            bytes: &[],
            step: 8,
        };
        assert_eq!(
            read_samples(&mut source, &mut |chunk: &[f32]| collected
                .extend_from_slice(chunk))
            .expect("read"),
            0
        );
        assert!(collected.is_empty());
    }

    #[test]
    fn duration_is_derived_from_the_sample_count() {
        assert_eq!(Decoded { samples: 16_000 }.duration_ms(), 1_000);
        assert_eq!(Decoded { samples: 0 }.duration_ms(), 0);
        // Two hours, the length ROADMAP phase 03 asks the ingestion to survive.
        assert_eq!(
            Decoded {
                samples: 16_000 * 7_200
            }
            .duration_ms(),
            7_200_000
        );
    }

    #[test]
    fn a_real_file_decodes_to_the_rate_whisper_needs() {
        let Some(tools) = bundled() else { return };
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sine.wav");

        // 3 s of tone, generated rather than versioned: no binary fixtures in git.
        let made = tools
            .run(
                Tool::Ffmpeg,
                &[
                    OsStr::new("-y"),
                    OsStr::new("-f"),
                    OsStr::new("lavfi"),
                    OsStr::new("-i"),
                    OsStr::new("sine=frequency=440:sample_rate=44100:duration=3"),
                    path.as_os_str(),
                ],
            )
            .expect("run");
        assert!(made.succeeded, "{}", made.stderr);

        let mut builder = PeaksBuilder::new(SAMPLE_RATE, DEFAULT_BUCKET_MS);
        let decoded = decode(&tools, path.as_os_str(), &mut |chunk: &[f32]| {
            builder.push(chunk);
        })
        .expect("decode");

        assert_eq!(decoded.samples, 48_000, "3 s resampled to 16 kHz");
        assert_eq!(decoded.duration_ms(), 3_000);

        let peaks = builder.finish();
        assert_eq!(peaks.len(), 150, "3 s in 20 ms buckets");

        // ffmpeg's `sine` is a test tone at 4095/32768 — exactly 0.125 of full scale, so every
        // bucket must land on 32/255. Checking the value rather than « not zero » is what proves
        // the amplitude survives the resample, the f32 conversion and the byte reassembly intact.
        assert!(
            peaks.values.iter().all(|&value| (31..=33).contains(&value)),
            "expected ~32 (0.125 of full scale), got {:?}",
            &peaks.values[..8]
        );
    }

    #[test]
    fn a_file_that_is_not_media_is_refused_with_ffmpegs_reason() {
        let Some(tools) = bundled() else { return };
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("faux.wav");
        std::fs::write(&path, b"pas du son du tout").expect("write");

        let mut nothing = |_: &[f32]| {};
        let error = decode(&tools, path.as_os_str(), &mut nothing).expect_err("must refuse");
        assert_eq!(error.kind(), "fichier-illisible");
    }

    #[test]
    fn a_missing_binary_refuses_before_anything_is_spawned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tools = Tools::in_directory(dir.path());

        let mut nothing = |_: &[f32]| {};
        let error = decode(&tools, OsStr::new("x.wav"), &mut nothing).expect_err("must refuse");
        assert_eq!(error.kind(), "outil-manquant");
    }
}
