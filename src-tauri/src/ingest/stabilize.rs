//! Waiting for a file that is still being written.
//!
//! CONCEPTION.md §8: « Fichier encore en cours d'écriture → attente de stabilisation de la
//! taille avant ingestion ». The case is ordinary rather than exotic — dropping a recording onto
//! Sillage while OBS is still writing it, or while Windows is still copying it from a phone.
//! Ingesting it immediately gives a truncated transcription and no error at all, which is the
//! worst of the possible outcomes.
//!
//! Both the size **and** the modification time are watched. A download that pre-allocates its
//! full size — which is what most downloaders do — never changes length while it is being
//! filled, so size alone would call it stable straight away.

use std::path::Path;
use std::time::{Duration, SystemTime};

use super::errors::{from_source_io, IngestError};

/// How patient to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stabilization {
    /// Delay between two readings.
    pub poll: Duration,
    /// How many identical readings in a row mean « finished ».
    pub settle: u32,
    /// Total time after which the file is declared to be going nowhere.
    pub timeout: Duration,
}

impl Default for Stabilization {
    fn default() -> Self {
        Self {
            // 250 ms is imperceptible on a finished file — two readings and it is through — and
            // frequent enough to notice a file that stops growing mid-copy.
            poll: Duration::from_millis(250),
            settle: 2,
            // Thirty seconds is a slow network copy of a long recording, not a hung one. Beyond
            // that the user is better told than left watching a card that never moves.
            timeout: Duration::from_secs(30),
        }
    }
}

impl Stabilization {
    /// No waiting at all — for tests that are not about waiting.
    #[must_use]
    pub fn immediate() -> Self {
        Self {
            poll: Duration::ZERO,
            settle: 1,
            timeout: Duration::ZERO,
        }
    }
}

/// A file's observable state: what changes while it is being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reading {
    len: u64,
    modified: Option<SystemTime>,
}

fn read(path: &Path) -> Result<Reading, IngestError> {
    let metadata = std::fs::metadata(path).map_err(|error| from_source_io(path, error))?;
    Ok(Reading {
        len: metadata.len(),
        // Some filesystems do not report it. Absent on both readings compares equal, which is
        // the right answer: it just falls back to watching the size.
        modified: metadata.modified().ok(),
    })
}

/// Blocks until `path` has stopped changing, and returns its size.
///
/// A file that never settles is refused rather than ingested half-written.
pub fn wait_until_stable(path: &Path, policy: &Stabilization) -> Result<u64, IngestError> {
    let started = std::time::Instant::now();
    let mut previous = read(path)?;
    let mut steady = 1_u32;

    while steady < policy.settle {
        if started.elapsed() >= policy.timeout {
            return Err(IngestError::NeverStable {
                path: path.to_path_buf(),
                seconds: policy.timeout.as_secs(),
            });
        }

        std::thread::sleep(policy.poll);

        let current = read(path)?;
        if current == previous {
            steady += 1;
        } else {
            // Any change restarts the count: a file written in bursts must not be caught
            // between two of them.
            steady = 1;
            previous = current;
        }
    }

    Ok(previous.len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast() -> Stabilization {
        Stabilization {
            poll: Duration::from_millis(10),
            settle: 2,
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn a_finished_file_goes_through_at_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fini.wav");
        std::fs::write(&path, vec![0_u8; 4_096]).expect("write");

        assert_eq!(wait_until_stable(&path, &fast()).expect("stable"), 4_096);
    }

    #[test]
    fn an_empty_file_is_stable_too() {
        // Emptiness is not this module's business: « durée nulle » is a separate refusal, with
        // its own message. Blocking here would give the user the wrong one.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vide.wav");
        std::fs::write(&path, b"").expect("write");

        assert_eq!(wait_until_stable(&path, &fast()).expect("stable"), 0);
    }

    #[test]
    fn a_file_still_growing_is_waited_for_and_its_final_size_returned() {
        use std::io::Write;

        const APPENDS: usize = 40;
        const CHUNK: usize = 64;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("en-cours.wav");
        std::fs::write(&path, vec![0_u8; 128]).expect("write");

        // Appends every 5 ms against a 40 ms poll: every reading the waiter takes differs from
        // the last, so the test turns on the behaviour rather than on the timing landing right.
        // Any polling scheme can be fooled by writes slower than its own period — which is what
        // the 250 ms default of `Stabilization` is sized against.
        let writing = {
            let path = path.clone();
            std::thread::spawn(move || {
                for _ in 0..APPENDS {
                    let mut file = std::fs::OpenOptions::new()
                        .append(true)
                        .open(&path)
                        .expect("open");
                    file.write_all(&[0_u8; CHUNK]).expect("append");
                    drop(file);
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        };

        let size = wait_until_stable(
            &path,
            &Stabilization {
                poll: Duration::from_millis(40),
                settle: 2,
                timeout: Duration::from_secs(10),
            },
        )
        .expect("stable");
        writing.join().expect("writer");

        assert_eq!(
            size,
            (128 + APPENDS * CHUNK) as u64,
            "the wait must have outlasted every append"
        );
        assert_eq!(size, std::fs::metadata(&path).expect("metadata").len());
    }

    #[test]
    fn a_file_that_never_settles_is_refused_rather_than_ingested_truncated() {
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sans-fin.wav");
        std::fs::write(&path, vec![0_u8; 128]).expect("write");

        let stop = Arc::new(AtomicBool::new(false));
        let writing = {
            let (path, stop) = (path.clone(), Arc::clone(&stop));
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let mut file = std::fs::OpenOptions::new()
                        .append(true)
                        .open(&path)
                        .expect("open");
                    file.write_all(&[0_u8; 64]).expect("append");
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        };

        let error = wait_until_stable(
            &path,
            &Stabilization {
                poll: Duration::from_millis(10),
                settle: 3,
                timeout: Duration::from_millis(200),
            },
        )
        .expect_err("must give up");

        stop.store(true, Ordering::Relaxed);
        writing.join().expect("writer");

        assert_eq!(error.kind(), "fichier-instable");
        assert!(error.to_string().contains("pas fini d'être écrit"));
    }

    #[test]
    fn a_missing_file_is_reported_as_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error =
            wait_until_stable(&dir.path().join("absent.wav"), &fast()).expect_err("must refuse");
        assert_eq!(error.kind(), "fichier-introuvable");
    }

    #[test]
    fn the_immediate_policy_never_sleeps() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fini.wav");
        std::fs::write(&path, vec![0_u8; 16]).expect("write");

        let started = std::time::Instant::now();
        assert_eq!(
            wait_until_stable(&path, &Stabilization::immediate()).expect("stable"),
            16
        );
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn the_default_policy_is_patient_but_not_indefinite() {
        let policy = Stabilization::default();
        assert!(policy.settle >= 2, "one reading proves nothing");
        assert!(policy.timeout >= Duration::from_secs(10));
        assert!(policy.timeout <= Duration::from_secs(60));
    }
}
