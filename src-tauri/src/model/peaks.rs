//! The waveform peaks file — `data/<id>.peaks`.
//!
//! CONCEPTION.md §3.2: the peaks are computed **during** the ffmpeg decode of phase 03, from the
//! PCM already in memory, and stored beside the transcript. The frontend never decodes the audio
//! a second time to draw a waveform.
//!
//! The format is deliberately dull: one unsigned byte per bucket, holding the loudest absolute
//! amplitude of that bucket. At the default 20 ms bucket that is 50 bytes per second — 45 Ko for
//! a quarter of an hour, well under the 200 Ko that ROADMAP phase 03 allows — while keeping
//! enough resolution to zoom far past the 96 bars of the player (DESIGN.md §8).

use serde::{Deserialize, Serialize};

/// File signature: `S`illage `P`eaks.
const MAGIC: [u8; 4] = *b"SLGP";
/// Bumped only if the layout below changes.
const VERSION: u8 = 1;
/// `MAGIC` + version + `bucket_ms` + count.
const HEADER: usize = 4 + 1 + 2 + 4;

/// Default bucket width. 50 values per second: fine enough to zoom, coarse enough to stay small.
pub const DEFAULT_BUCKET_MS: u16 = 20;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PeaksError {
    #[error("ce fichier de pics n'est pas un fichier Sillage")]
    BadMagic,
    #[error("version de fichier de pics inconnue : {0}")]
    UnsupportedVersion(u8),
    #[error("fichier de pics tronqué")]
    Truncated,
    #[error("largeur de bloc nulle dans le fichier de pics")]
    ZeroBucket,
}

/// Waveform peaks for one media file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peaks {
    /// Width of one bucket, in milliseconds.
    pub bucket_ms: u16,
    /// Loudest absolute amplitude of each bucket, `0..=255`.
    pub values: Vec<u8>,
}

impl Peaks {
    /// Reduces PCM to peaks. `samples` is mono f32 in `-1.0..=1.0`, as ffmpeg hands it over.
    ///
    /// Anything outside the range is clamped rather than wrapped: a decoder that overshoots by a
    /// hair must not produce a bar pointing the wrong way.
    #[must_use]
    pub fn from_pcm(samples: &[f32], sample_rate: u32, bucket_ms: u16) -> Self {
        let per_bucket = samples_per_bucket(sample_rate, bucket_ms);

        let values = samples
            .chunks(per_bucket)
            .map(|chunk| {
                let loudest = chunk
                    .iter()
                    .map(|sample| sample.abs())
                    .fold(0.0_f32, f32::max)
                    .clamp(0.0, 1.0);
                (loudest * 255.0).round() as u8
            })
            .collect();

        Self { bucket_ms, values }
    }

    /// Duration covered by the peaks, in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.values.len() as u64 * u64::from(self.bucket_ms)
    }

    /// Reduces the peaks to exactly `bars` values — 96 for the player of DESIGN.md §8.
    ///
    /// Each output bar takes the loudest value of its slice, never the average: averaging a
    /// waveform flattens the transients and every recording ends up looking the same.
    #[must_use]
    pub fn downsample(&self, bars: usize) -> Vec<u8> {
        if bars == 0 {
            return Vec::new();
        }
        if self.values.is_empty() {
            return vec![0; bars];
        }

        (0..bars)
            .map(|bar| {
                let start = bar * self.values.len() / bars;
                let end = ((bar + 1) * self.values.len() / bars).max(start + 1);
                self.values[start..end.min(self.values.len())]
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0)
            })
            .collect()
    }

    /// Number of buckets held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Serializes to the compact binary form stored in `data/<id>.peaks`.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + self.values.len());
        out.extend_from_slice(&MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.bucket_ms.to_le_bytes());
        out.extend_from_slice(&(self.values.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.values);
        out
    }

    /// Reads the compact binary form back.
    pub fn decode(bytes: &[u8]) -> Result<Self, PeaksError> {
        if bytes.len() < HEADER {
            return Err(PeaksError::Truncated);
        }
        if bytes[0..4] != MAGIC {
            return Err(PeaksError::BadMagic);
        }
        if bytes[4] != VERSION {
            return Err(PeaksError::UnsupportedVersion(bytes[4]));
        }

        let bucket_ms = u16::from_le_bytes([bytes[5], bytes[6]]);
        if bucket_ms == 0 {
            return Err(PeaksError::ZeroBucket);
        }

        let count = u32::from_le_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]) as usize;
        let body = &bytes[HEADER..];
        if body.len() < count {
            return Err(PeaksError::Truncated);
        }

        Ok(Self {
            bucket_ms,
            values: body[..count].to_vec(),
        })
    }
}

/// Builds [`Peaks`] from PCM arriving a chunk at a time.
///
/// [`Peaks::from_pcm`] needs the whole recording in memory, which phase 03 cannot afford: two
/// hours of 16 kHz mono f32 is 460 Mo on its own, and the budget for the entire ingestion is
/// 500 Mo of RSS (ROADMAP phase 03). The builder keeps one partial bucket instead, so peak
/// memory is the same for a two-minute memo and a two-hour recording.
///
/// It produces exactly what [`Peaks::from_pcm`] would have produced on the concatenation of the
/// chunks — including the last, incomplete bucket, which `chunks` also emits.
#[derive(Debug, Clone)]
pub struct PeaksBuilder {
    bucket_ms: u16,
    samples_per_bucket: usize,
    values: Vec<u8>,
    /// Loudest sample of the bucket being filled.
    loudest: f32,
    /// How many samples that bucket already holds.
    filled: usize,
    total_samples: u64,
}

impl PeaksBuilder {
    #[must_use]
    pub fn new(sample_rate: u32, bucket_ms: u16) -> Self {
        Self {
            bucket_ms,
            samples_per_bucket: samples_per_bucket(sample_rate, bucket_ms),
            values: Vec::new(),
            loudest: 0.0,
            filled: 0,
            total_samples: 0,
        }
    }

    /// Folds one chunk of mono f32 PCM in.
    pub fn push(&mut self, samples: &[f32]) {
        self.total_samples += samples.len() as u64;

        let mut rest = samples;
        while !rest.is_empty() {
            let room = self.samples_per_bucket - self.filled;
            let take = room.min(rest.len());
            let (head, tail) = rest.split_at(take);

            self.loudest = head
                .iter()
                .map(|sample| sample.abs())
                .fold(self.loudest, f32::max);
            self.filled += take;

            if self.filled == self.samples_per_bucket {
                self.close_bucket();
            }
            rest = tail;
        }
    }

    /// Total samples seen so far.
    #[must_use]
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Closes the last, partial bucket and hands over the peaks.
    #[must_use]
    pub fn finish(mut self) -> Peaks {
        if self.filled > 0 {
            self.close_bucket();
        }
        Peaks {
            bucket_ms: self.bucket_ms,
            values: self.values,
        }
    }

    fn close_bucket(&mut self) {
        self.values
            .push((self.loudest.clamp(0.0, 1.0) * 255.0).round() as u8);
        self.loudest = 0.0;
        self.filled = 0;
    }
}

/// Samples covered by one bucket. Never zero: a zero-width bucket would divide by nothing and
/// the loop above would never advance.
fn samples_per_bucket(sample_rate: u32, bucket_ms: u16) -> usize {
    (u64::from(sample_rate) * u64::from(bucket_ms) / 1000).max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(len: usize) -> Peaks {
        Peaks {
            bucket_ms: DEFAULT_BUCKET_MS,
            values: (0..len).map(|i| (i % 256) as u8).collect(),
        }
    }

    #[test]
    fn encode_decode_round_trips() {
        let peaks = ramp(5_000);
        assert_eq!(Peaks::decode(&peaks.encode()), Ok(peaks));
    }

    #[test]
    fn an_empty_waveform_round_trips_too() {
        let peaks = Peaks {
            bucket_ms: DEFAULT_BUCKET_MS,
            values: Vec::new(),
        };
        assert_eq!(Peaks::decode(&peaks.encode()), Ok(peaks));
    }

    #[test]
    fn a_corrupt_file_is_reported_rather_than_misread() {
        assert_eq!(Peaks::decode(b""), Err(PeaksError::Truncated));
        assert_eq!(
            Peaks::decode(b"NOPE\x01\x14\x00\x00\x00\x00\x00"),
            Err(PeaksError::BadMagic)
        );

        let mut wrong_version = ramp(4).encode();
        wrong_version[4] = 9;
        assert_eq!(
            Peaks::decode(&wrong_version),
            Err(PeaksError::UnsupportedVersion(9))
        );

        let mut zero_bucket = ramp(4).encode();
        zero_bucket[5] = 0;
        zero_bucket[6] = 0;
        assert_eq!(Peaks::decode(&zero_bucket), Err(PeaksError::ZeroBucket));

        let complete = ramp(100).encode();
        assert_eq!(
            Peaks::decode(&complete[..complete.len() - 10]),
            Err(PeaksError::Truncated),
            "a file cut short must not decode as a shorter waveform"
        );
    }

    #[test]
    fn pcm_becomes_one_value_per_bucket() {
        // 1 s of 16 kHz at full scale, in 20 ms buckets: 50 buckets, all at 255.
        let samples = vec![1.0_f32; 16_000];
        let peaks = Peaks::from_pcm(&samples, 16_000, DEFAULT_BUCKET_MS);

        assert_eq!(peaks.values.len(), 50);
        assert!(peaks.values.iter().all(|&value| value == 255));
        assert_eq!(peaks.duration_ms(), 1_000);
    }

    #[test]
    fn a_bucket_keeps_its_loudest_sample() {
        let mut samples = vec![0.0_f32; 640]; // two 20 ms buckets at 16 kHz
        samples[10] = 0.5;
        samples[500] = -1.0; // sign must not matter

        let peaks = Peaks::from_pcm(&samples, 16_000, DEFAULT_BUCKET_MS);
        assert_eq!(peaks.values, vec![128, 255]);
    }

    #[test]
    fn samples_outside_the_nominal_range_are_clamped() {
        let peaks = Peaks::from_pcm(&[2.5, -7.0], 16_000, DEFAULT_BUCKET_MS);
        assert_eq!(peaks.values, vec![255]);
    }

    #[test]
    fn a_quarter_of_an_hour_stays_well_under_the_budget() {
        // ROADMAP phase 03: peaks for a 15 min file must weigh less than 200 Ko.
        let quarter_hour_ms = 15 * 60 * 1_000;
        let peaks = Peaks {
            bucket_ms: DEFAULT_BUCKET_MS,
            values: vec![128; quarter_hour_ms / DEFAULT_BUCKET_MS as usize],
        };

        let encoded = peaks.encode().len();
        assert_eq!(peaks.duration_ms(), quarter_hour_ms as u64);
        assert!(encoded < 200 * 1024, "peaks weigh {encoded} bytes");
    }

    #[test]
    fn downsampling_gives_exactly_the_number_of_bars_asked_for() {
        // The player draws 96 bars (DESIGN.md §8), whatever the length of the recording.
        for len in [0, 1, 7, 95, 96, 97, 45_000] {
            assert_eq!(ramp(len).downsample(96).len(), 96, "length {len}");
        }
        assert!(ramp(100).downsample(0).is_empty());
    }

    #[test]
    fn downsampling_keeps_the_peaks() {
        let mut peaks = ramp(960);
        peaks.values = vec![0; 960];
        peaks.values[500] = 200;

        let bars = peaks.downsample(96);
        assert_eq!(bars.iter().copied().max(), Some(200), "the peak survived");
        assert_eq!(bars[50], 200, "and stayed where it belongs");
    }

    /// Deterministic pseudo-audio: no crate needed, and the same samples every run.
    fn wobble(len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| ((i as f32) * 0.017).sin() * ((i % 977) as f32 / 977.0))
            .collect()
    }

    #[test]
    fn the_builder_agrees_with_the_whole_slice_whatever_the_chunking() {
        // The property that matters: streaming the decode must not change a single bar. Chunk
        // sizes are chosen around the 320-sample bucket — under it, over it, and coprime with it.
        let samples = wobble(50_000);
        let expected = Peaks::from_pcm(&samples, 16_000, DEFAULT_BUCKET_MS);

        for chunk in [1, 7, 319, 320, 321, 1_024, 4_096, 50_000] {
            let mut builder = PeaksBuilder::new(16_000, DEFAULT_BUCKET_MS);
            for slice in samples.chunks(chunk) {
                builder.push(slice);
            }
            let built = builder.finish();

            assert_eq!(built, expected, "chunk size {chunk}");
        }
    }

    #[test]
    fn the_builder_keeps_the_last_partial_bucket() {
        // 480 samples at 16 kHz is one full 20 ms bucket plus half of another. Dropping the
        // remainder would shorten every recording by up to 20 ms.
        let mut builder = PeaksBuilder::new(16_000, DEFAULT_BUCKET_MS);
        builder.push(&vec![1.0_f32; 480]);
        let peaks = builder.finish();

        assert_eq!(peaks.values, vec![255, 255]);
        assert_eq!(
            peaks,
            Peaks::from_pcm(&vec![1.0_f32; 480], 16_000, DEFAULT_BUCKET_MS)
        );
    }

    #[test]
    fn the_builder_counts_every_sample_it_was_given() {
        // The exact duration of a media file is this count, not what the container claims.
        let mut builder = PeaksBuilder::new(16_000, DEFAULT_BUCKET_MS);
        for _ in 0..10 {
            builder.push(&[0.0; 1_234]);
        }
        assert_eq!(builder.total_samples(), 12_340);
    }

    #[test]
    fn a_builder_given_nothing_produces_nothing() {
        let peaks = PeaksBuilder::new(16_000, DEFAULT_BUCKET_MS).finish();
        assert!(peaks.is_empty());
        assert_eq!(peaks.duration_ms(), 0);
    }

    #[test]
    fn the_builder_clamps_like_the_whole_slice_does() {
        let mut builder = PeaksBuilder::new(16_000, DEFAULT_BUCKET_MS);
        builder.push(&[2.5]);
        builder.push(&[-7.0]);
        assert_eq!(builder.finish().values, vec![255]);
    }

    #[test]
    fn downsampling_a_short_waveform_repeats_rather_than_dropping_bars() {
        let peaks = Peaks {
            bucket_ms: DEFAULT_BUCKET_MS,
            values: vec![10, 20, 30],
        };
        let bars = peaks.downsample(6);
        assert_eq!(bars, vec![10, 10, 20, 20, 30, 30]);
    }
}
