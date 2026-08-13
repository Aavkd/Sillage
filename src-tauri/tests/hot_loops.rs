//! The measurement PROGRESS.md phase 01, debt n° 6, deferred to this phase.
//!
//! > `opt-level = "s"` optimise la taille, alors que la charge utile est déjà dominée par les
//! > DLL CUDA […]. À revoir en **phase 03**, où le calcul des pics et le hachage en flux sont
//! > des boucles chaudes en Rust.
//!
//! « À revoir **avec une mesure**, pas par principe » — so here is the measurement, on the two
//! loops phase 03 actually added. Run it under each profile and compare:
//!
//! ```text
//! cargo test --release --test hot_loops -- --ignored --nocapture
//! ```
//!
//! Both loops are given the two-hour workload of ROADMAP phase 03, which is the largest thing
//! the application is required to handle.

use std::time::Instant;

use sillage_lib::model::hash::sha256_file;
use sillage_lib::model::peaks::{PeaksBuilder, DEFAULT_BUCKET_MS};

/// 16 kHz mono for two hours — 115 200 000 samples, the full decode of the longest supported file.
const SAMPLES: usize = 16_000 * 7_200;
/// The chunk the decoder hands over, so the loop is exercised the way it really runs.
const CHUNK: usize = 16 * 1024;
/// Enough bytes for the hash to be measured rather than the file system.
const HASH_BYTES: usize = 256 * 1024 * 1024;

#[test]
#[ignore = "a benchmark, not an assertion — run it explicitly when choosing a profile"]
fn measure_the_two_hot_loops_phase_03_added() {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    eprintln!("\nprofil : {profile}");

    // --- Peaks -------------------------------------------------------------------------------
    let chunk: Vec<f32> = (0..CHUNK)
        .map(|i| ((i as f32) * 0.017).sin() * 0.8)
        .collect();

    let started = Instant::now();
    let mut builder = PeaksBuilder::new(16_000, DEFAULT_BUCKET_MS);
    let mut pushed = 0;
    while pushed < SAMPLES {
        let take = CHUNK.min(SAMPLES - pushed);
        builder.push(&chunk[..take]);
        pushed += take;
    }
    let peaks = builder.finish();
    let elapsed = started.elapsed();

    eprintln!(
        "pics    : {:>8.0} ms pour 2 h ({} blocs, {:.0}× temps réel)",
        elapsed.as_secs_f64() * 1000.0,
        peaks.len(),
        7_200.0 / elapsed.as_secs_f64()
    );
    assert_eq!(peaks.len(), 360_000);

    // --- Streaming SHA-256 -------------------------------------------------------------------
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("média.bin");
    {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
        let block: Vec<u8> = (0..1024 * 1024).map(|i| (i % 251) as u8).collect();
        for _ in 0..HASH_BYTES / block.len() {
            file.write_all(&block).expect("write");
        }
        file.flush().expect("flush");
    }

    let started = Instant::now();
    let digest = sha256_file(&path).expect("hash");
    let elapsed = started.elapsed();

    eprintln!(
        "sha-256 : {:>8.0} ms pour {} Mo ({:.0} Mo/s)",
        elapsed.as_secs_f64() * 1000.0,
        HASH_BYTES / 1024 / 1024,
        (HASH_BYTES as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64()
    );
    assert_eq!(digest.len(), 64);
    eprintln!();
}
