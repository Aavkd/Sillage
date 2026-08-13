//! Phase 00 spike.
//!
//! Single question: can whisper-rs produce per-word timestamps good enough to build
//! Sillage's audio-synced editor on? Everything else here exists to answer that.
//!
//! Usage:
//!   spike --model <ggml.bin> --audio <file> [--out results.json]
//!         [--lang fr|en|auto] [--vad <ggml-silero.bin>] [--threads N]

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// How many times whisper.cpp actually invoked the streaming callback.
static STREAM_CALLS: AtomicUsize = AtomicUsize::new(0);
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

/// whisper.cpp reports timestamps in centiseconds.
const CS_TO_MS: i64 = 10;
const SAMPLE_RATE: usize = 16_000;

#[derive(Serialize)]
struct Word {
    text: String,
    start_ms: i64,
    end_ms: i64,
    prob: f32,
    /// false when whisper returned no DTW time for this token and we fell back to t0/t1.
    dtw: bool,
}

#[derive(Serialize)]
struct Segment {
    index: i32,
    start_ms: i64,
    end_ms: i64,
    text: String,
    no_speech_prob: f32,
    words: Vec<Word>,
}

#[derive(Serialize)]
struct Report {
    model: String,
    audio: String,
    backend: &'static str,
    language: String,
    vad: bool,
    dtw_enabled: bool,
    /// Streaming callback invocations. 0 with DTW on is the whisper.cpp 1.8.3 conflict.
    stream_callbacks: usize,
    audio_ms: i64,
    transcribe_ms: u128,
    realtime_factor: f64,
    segment_count: usize,
    word_count: usize,
    /// Share of words carrying a real DTW timestamp. Below ~0.95 the editor is in trouble.
    dtw_coverage: f64,
    monotonic: bool,
    within_segment_bounds: bool,
    segments: Vec<Segment>,
}

struct Args {
    model: PathBuf,
    audio: PathBuf,
    out: PathBuf,
    lang: String,
    vad: Option<PathBuf>,
    threads: i32,
    no_dtw: bool,
}

fn parse_args() -> Result<Args> {
    let mut model = None;
    let mut audio = None;
    let mut out = PathBuf::from("results.json");
    let mut lang = "fr".to_string();
    let mut vad = None;
    let mut threads = std::thread::available_parallelism().map_or(4, |n| n.get() as i32);
    let mut no_dtw = false;

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let need = |i: usize| -> Result<String> {
            argv.get(i + 1)
                .cloned()
                .with_context(|| format!("missing value after {}", argv[i]))
        };
        match argv[i].as_str() {
            "--model" => {
                model = Some(PathBuf::from(need(i)?));
                i += 2;
            }
            "--audio" => {
                audio = Some(PathBuf::from(need(i)?));
                i += 2;
            }
            "--out" => {
                out = PathBuf::from(need(i)?);
                i += 2;
            }
            "--lang" => {
                lang = need(i)?;
                i += 2;
            }
            "--vad" => {
                vad = Some(PathBuf::from(need(i)?));
                i += 2;
            }
            "--threads" => {
                threads = need(i)?.parse().context("--threads must be a number")?;
                i += 2;
            }
            "--no-dtw" => {
                no_dtw = true;
                i += 1;
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(Args {
        model: model.context("--model is required")?,
        audio: audio.context("--audio is required")?,
        out,
        lang,
        vad,
        threads,
        no_dtw,
    })
}

/// Decode anything ffmpeg understands into the 16 kHz mono f32 whisper expects.
/// The real app will bundle ffmpeg; for the spike, PATH is fine.
fn decode_pcm(path: &PathBuf) -> Result<Vec<f32>> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-f",
            "f32le",
            "-ac",
            "1",
            "-ar",
            &SAMPLE_RATE.to_string(),
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run ffmpeg — is it on PATH?")?;

    if !out.status.success() {
        bail!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    if out.stdout.len() % 4 != 0 {
        bail!("ffmpeg returned a truncated f32 stream");
    }

    Ok(out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// whisper emits sub-word tokens; a leading space marks the start of a new word.
/// Accumulate raw bytes rather than strings so multi-byte French characters split
/// across two tokens (é, à, ç) survive.
struct WordBuilder {
    bytes: Vec<u8>,
    start_ms: i64,
    end_ms: i64,
    prob_sum: f32,
    token_count: u32,
    had_dtw: bool,
}

impl WordBuilder {
    fn finish(self) -> Option<Word> {
        let text = String::from_utf8_lossy(&self.bytes).trim().to_string();
        if text.is_empty() {
            return None;
        }
        Some(Word {
            text,
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            prob: self.prob_sum / self.token_count.max(1) as f32,
            dtw: self.had_dtw,
        })
    }
}

fn main() -> Result<()> {
    let args = parse_args()?;

    let backend = if cfg!(feature = "cuda") { "cuda" } else { "cpu" };
    println!("backend        : {backend}");
    println!("model          : {}", args.model.display());
    println!("audio          : {}", args.audio.display());

    let pcm = decode_pcm(&args.audio)?;
    let audio_ms = (pcm.len() as f64 / SAMPLE_RATE as f64 * 1000.0) as i64;
    println!("decoded        : {} samples ({} ms)", pcm.len(), audio_ms);

    // The whole point of the spike: DTW alignment heads for large-v3-turbo.
    let mut ctx_params = WhisperContextParameters::default();
    ctx_params.dtw_parameters(DtwParameters {
        mode: if args.no_dtw {
            DtwMode::None
        } else {
            DtwMode::ModelPreset {
                model_preset: DtwModelPreset::LargeV3Turbo,
            }
        },
        dtw_mem_size: 1024 * 1024 * 128,
    });
    println!("dtw            : {}", !args.no_dtw);

    let ctx = WhisperContext::new_with_params(
        args.model
            .to_str()
            .context("model path is not valid UTF-8")?,
        ctx_params,
    )
    .context("failed to load the model — wrong path, or a non-turbo model with turbo DTW heads?")?;

    let mut state = ctx.create_state().context("failed to create state")?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
    params.set_n_threads(args.threads);
    params.set_token_timestamps(true); // required for per-token times
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    if args.lang != "auto" {
        params.set_language(Some(&args.lang));
    } else {
        params.set_detect_language(true);
    }

    let vad_path_str;
    if let Some(vad) = &args.vad {
        vad_path_str = vad.to_str().context("vad path is not valid UTF-8")?.to_string();
        params.set_vad_model_path(Some(&vad_path_str));
        println!("vad            : {}", vad.display());
    }

    // Streaming proof: this is the callback phase 04 would bridge to Tauri events.
    // whisper.cpp 1.8.3 gates the normal call sites behind `!dtw_token_timestamps`,
    // so this is expected to stay silent whenever DTW is enabled. --no-dtw isolates that.
    params.set_segment_callback_safe_lossy(|data: whisper_rs::SegmentCallbackData| {
        STREAM_CALLS.fetch_add(1, Ordering::Relaxed);
        println!(
            "  [stream] {:>7} ms  {}",
            data.start_timestamp * CS_TO_MS,
            data.text.trim()
        );
    });

    println!("transcribing…");
    let started = Instant::now();
    state.full(params, &pcm).context("transcription failed")?;
    let transcribe_ms = started.elapsed().as_millis();

    // ---- collect ----
    let mut segments = Vec::new();
    let mut word_count = 0usize;
    let mut dtw_words = 0usize;
    let mut monotonic = true;
    let mut within_bounds = true;
    let mut last_end = i64::MIN;

    let eot = ctx.token_eot();

    for seg in state.as_iter() {
        let seg_start = seg.start_timestamp() * CS_TO_MS;
        let seg_end = seg.end_timestamp() * CS_TO_MS;

        let mut words: Vec<Word> = Vec::new();
        let mut current: Option<WordBuilder> = None;

        for t in 0..seg.n_tokens() {
            let Some(token) = seg.get_token(t) else {
                continue;
            };
            if token.token_id() >= eot {
                continue; // special token
            }
            let Ok(bytes) = token.to_bytes() else { continue };
            if bytes.starts_with(b"[_") {
                continue; // whisper marker token
            }

            let data = token.token_data();
            let has_dtw = data.t_dtw >= 0;
            let start_ms = if has_dtw { data.t_dtw } else { data.t0 } * CS_TO_MS;
            let end_ms = if has_dtw { data.t_dtw } else { data.t1 } * CS_TO_MS;

            let starts_word = bytes.first() == Some(&b' ') || current.is_none();
            if starts_word {
                if let Some(w) = current.take().and_then(WordBuilder::finish) {
                    words.push(w);
                }
                current = Some(WordBuilder {
                    bytes: bytes.to_vec(),
                    start_ms,
                    end_ms,
                    prob_sum: data.p,
                    token_count: 1,
                    had_dtw: has_dtw,
                });
            } else if let Some(w) = current.as_mut() {
                w.bytes.extend_from_slice(bytes);
                w.end_ms = end_ms.max(w.end_ms);
                w.prob_sum += data.p;
                w.token_count += 1;
                w.had_dtw &= has_dtw;
            }
        }
        if let Some(w) = current.take().and_then(WordBuilder::finish) {
            words.push(w);
        }

        // DTW gives one onset point per token, not an interval. A word therefore ends
        // where the next one begins (last word ends with its segment). Highlighting in
        // the editor needs intervals, so derive them here rather than in the UI.
        for i in 0..words.len() {
            let next_start = words
                .get(i + 1)
                .map(|w| w.start_ms)
                .unwrap_or(seg_end)
                .min(seg_end);
            if words[i].end_ms <= words[i].start_ms {
                words[i].end_ms = next_start.max(words[i].start_ms);
            }
        }

        for w in &words {
            word_count += 1;
            if w.dtw {
                dtw_words += 1;
            }
            if w.start_ms < last_end {
                monotonic = false;
            }
            last_end = w.end_ms;
            // 50 ms of slack: DTW may nudge a word marginally past its segment edge.
            if w.start_ms + 50 < seg_start || w.end_ms > seg_end + 50 {
                within_bounds = false;
            }
        }

        segments.push(Segment {
            index: seg.segment_index(),
            start_ms: seg_start,
            end_ms: seg_end,
            text: seg.to_str_lossy().unwrap_or_default().trim().to_string(),
            no_speech_prob: seg.no_speech_probability(),
            words,
        });
    }

    let dtw_coverage = if word_count == 0 {
        0.0
    } else {
        dtw_words as f64 / word_count as f64
    };

    let report = Report {
        model: args.model.display().to_string(),
        audio: args.audio.display().to_string(),
        backend,
        language: args.lang.clone(),
        vad: args.vad.is_some(),
        dtw_enabled: !args.no_dtw,
        stream_callbacks: STREAM_CALLS.load(Ordering::Relaxed),
        audio_ms,
        transcribe_ms,
        realtime_factor: audio_ms as f64 / transcribe_ms.max(1) as f64,
        segment_count: segments.len(),
        word_count,
        dtw_coverage,
        monotonic,
        within_segment_bounds: within_bounds,
        segments,
    };

    std::fs::write(&args.out, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", args.out.display()))?;

    println!("\n─── verdict ───");
    println!("stream calls   : {}", report.stream_callbacks);
    println!("segments       : {}", report.segment_count);
    println!("words          : {}", report.word_count);
    println!("dtw coverage   : {:.1} %", report.dtw_coverage * 100.0);
    println!("monotonic      : {}", report.monotonic);
    println!("within bounds  : {}", report.within_segment_bounds);
    println!(
        "speed          : {} ms for {} ms of audio ({:.1}× realtime)",
        report.transcribe_ms, report.audio_ms, report.realtime_factor
    );
    println!("written to     : {}", args.out.display());

    Ok(())
}
