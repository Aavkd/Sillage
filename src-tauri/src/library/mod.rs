//! The library folder — the one place the application keeps user data.
//!
//! Layout, straight from CONCEPTION.md §3.4:
//!
//! ```text
//! library/
//!   library.db                  SQLite: index, metadata, tags, FTS5
//!   media/<id>.<ext>            the copied original audio
//!   data/<id>.json              the complete transcript
//!   data/<id>.peaks             waveform peaks
//!   outputs/<id>.<kind>.md      generated outputs
//! ```
//!
//! Everything the index stores about a file is **relative to this root**, so the folder can be
//! moved without rewriting a single row (ROADMAP phase 02, task 6).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::db::{Database, DbError};
use crate::model::peaks::{Peaks, PeaksError};
use crate::model::Transcript;

/// Folder name used under the user's Documents directory when no location has been chosen.
///
/// CONCEPTION.md §3.4 still says `Transcript`, from before the application was named; ROADMAP
/// phase 02 says `Sillage`, and so does the rest of the document. Flagged to the user in
/// PROGRESS.md rather than settled silently.
pub const DEFAULT_FOLDER_NAME: &str = "Sillage";

const DATABASE_FILE: &str = "library.db";
const MEDIA_DIR: &str = "media";
const DATA_DIR: &str = "data";
const OUTPUTS_DIR: &str = "outputs";

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("erreur d'accès au dossier de la bibliothèque : {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Db(#[from] DbError),
    #[error("transcription illisible : {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Peaks(#[from] PeaksError),
    #[error("la transcription {0} est introuvable dans la bibliothèque")]
    MissingTranscript(String),
    #[error("le dossier de destination existe déjà et n'est pas vide : {0}")]
    DestinationNotEmpty(PathBuf),
    #[error("le nouveau dossier ne peut pas se trouver à l'intérieur de l'actuel")]
    DestinationInsideSource,
}

/// Where everything lives, given a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPaths {
    root: PathBuf,
}

impl LibraryPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The default location: `<Documents>/Sillage`.
    pub fn default_root(documents_dir: impl AsRef<Path>) -> PathBuf {
        documents_dir.as_ref().join(DEFAULT_FOLDER_NAME)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.root.join(DATABASE_FILE)
    }

    #[must_use]
    pub fn media_dir(&self) -> PathBuf {
        self.root.join(MEDIA_DIR)
    }

    #[must_use]
    pub fn data_dir(&self) -> PathBuf {
        self.root.join(DATA_DIR)
    }

    #[must_use]
    pub fn outputs_dir(&self) -> PathBuf {
        self.root.join(OUTPUTS_DIR)
    }

    /// Absolute path of a media copy, from the relative path held in the index.
    #[must_use]
    pub fn resolve(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// The relative media path to store in the index — `media/<id>.<ext>`.
    ///
    /// The extension of the original is kept (CONCEPTION.md §3.4): the copy must stay openable
    /// by any other program, and renaming a `.m4a` to `.bin` would achieve nothing.
    #[must_use]
    pub fn relative_media(id: &str, extension: &str) -> String {
        let extension = extension.trim_start_matches('.');
        if extension.is_empty() {
            format!("{MEDIA_DIR}/{id}")
        } else {
            format!("{MEDIA_DIR}/{id}.{extension}")
        }
    }

    #[must_use]
    pub fn transcript_json(&self, id: &str) -> PathBuf {
        self.data_dir().join(format!("{id}.json"))
    }

    #[must_use]
    pub fn peaks(&self, id: &str) -> PathBuf {
        self.data_dir().join(format!("{id}.peaks"))
    }

    /// `outputs/<id>.<kind>.md`.
    ///
    /// Custom prompts are keyed `custom:<slug>`, and `:` is not allowed in a Windows filename —
    /// it would silently create an alternate data stream instead of a file. It becomes `-` here,
    /// and the database keeps the real kind.
    #[must_use]
    pub fn output(&self, id: &str, kind: &str) -> PathBuf {
        let kind = kind.replace(':', "-");
        self.outputs_dir().join(format!("{id}.{kind}.md"))
    }

    /// Creates the folder tree if it is not already there.
    pub fn ensure(&self) -> io::Result<()> {
        for dir in [self.media_dir(), self.data_dir(), self.outputs_dir()] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// An open library: the folder and its index.
pub struct Library {
    paths: LibraryPaths,
    db: Database,
}

impl Library {
    /// Opens a library, creating the folder tree and the database if needed.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, LibraryError> {
        let paths = LibraryPaths::new(root);
        paths.ensure()?;
        let db = Database::open(paths.database())?;
        Ok(Self { paths, db })
    }

    #[must_use]
    pub fn paths(&self) -> &LibraryPaths {
        &self.paths
    }

    #[must_use]
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Writes the complete record to `data/<id>.json` **and** indexes it.
    ///
    /// The JSON goes first: it is the authoritative copy, and an index that lags behind a record
    /// can be rebuilt, while a record that lags behind its index is lost work.
    pub fn save_transcript(&self, transcript: &Transcript) -> Result<(), LibraryError> {
        let path = self.paths.transcript_json(&transcript.id);
        write_atomically(&path, serde_json::to_string_pretty(transcript)?.as_bytes())?;
        self.db.save_transcript(transcript)?;
        Ok(())
    }

    /// Reads the complete record back from its JSON file.
    pub fn load_transcript(&self, id: &str) -> Result<Transcript, LibraryError> {
        let path = self.paths.transcript_json(id);
        let raw = fs::read_to_string(&path)
            .map_err(|_| LibraryError::MissingTranscript(id.to_string()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Removes a transcript: its index row, its record, its peaks and its media copy.
    ///
    /// The one place in the application allowed to delete user data, and only on an explicit
    /// request (ROADMAP §B.3).
    pub fn delete_transcript(&self, id: &str) -> Result<(), LibraryError> {
        let media = self
            .db
            .transcript_row(id)?
            .map(|row| self.paths.resolve(&row.media_path));

        self.db.delete_transcript(id)?;
        remove_if_present(&self.paths.transcript_json(id))?;
        remove_if_present(&self.paths.peaks(id))?;
        if let Some(media) = media {
            remove_if_present(&media)?;
        }
        Ok(())
    }

    pub fn save_peaks(&self, id: &str, peaks: &Peaks) -> Result<(), LibraryError> {
        write_atomically(&self.paths.peaks(id), &peaks.encode())?;
        Ok(())
    }

    pub fn load_peaks(&self, id: &str) -> Result<Peaks, LibraryError> {
        let raw = fs::read(self.paths.peaks(id))
            .map_err(|_| LibraryError::MissingTranscript(id.to_string()))?;
        Ok(Peaks::decode(&raw)?)
    }

    /// Moves the whole library to another folder and reopens it there.
    ///
    /// Consuming `self` is what makes this safe: the connection has to be closed before the files
    /// can move on Windows, so there must be no way to keep using the old handle afterwards.
    pub fn relocate(self, new_root: impl AsRef<Path>) -> Result<Self, LibraryError> {
        let new_root = new_root.as_ref();
        let old_root = self.paths.root().to_path_buf();

        if new_root == old_root {
            return Ok(self);
        }
        if new_root.starts_with(&old_root) {
            return Err(LibraryError::DestinationInsideSource);
        }
        if is_non_empty_dir(new_root)? {
            return Err(LibraryError::DestinationNotEmpty(new_root.to_path_buf()));
        }

        self.db.close()?;
        move_tree(&old_root, new_root)?;
        Self::open(new_root)
    }
}

fn is_non_empty_dir(path: &Path) -> io::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    Ok(fs::read_dir(path)?.next().is_some())
}

/// Moves a folder, falling back to a copy when the two ends are on different volumes.
///
/// `rename` is the fast path and is atomic; it fails across volumes, which is exactly the case
/// the user is most likely to be after — moving the library to another drive.
fn move_tree(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_tree(from, to)?;
            fs::remove_dir_all(from)
        }
    }
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Writes through a temporary file: a crash mid-write leaves the previous version intact rather
/// than a half-written record.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::transcripts::tests::transcript;
    use crate::model::peaks::DEFAULT_BUCKET_MS;
    use crate::model::Edit;

    fn library() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library::open(dir.path().join("Sillage")).expect("open");
        (dir, library)
    }

    #[test]
    fn opening_creates_the_tree_of_conception_3_4() {
        let (_dir, library) = library();
        let paths = library.paths();

        assert!(paths.media_dir().is_dir());
        assert!(paths.data_dir().is_dir());
        assert!(paths.outputs_dir().is_dir());
        assert!(paths.database().is_file());
    }

    #[test]
    fn opening_an_existing_library_again_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("Sillage");

        let library = Library::open(&root).expect("first open");
        library
            .save_transcript(&transcript("a", "Une"))
            .expect("save");
        drop(library);

        let library = Library::open(&root).expect("second open");
        assert_eq!(library.db().count_transcripts().expect("count"), 1);
    }

    #[test]
    fn the_default_root_sits_under_documents() {
        let root = LibraryPaths::default_root(r"C:\Users\x\Documents");
        assert!(root.ends_with("Sillage"));
    }

    #[test]
    fn media_paths_are_relative_and_keep_their_extension() {
        assert_eq!(LibraryPaths::relative_media("abc", "m4a"), "media/abc.m4a");
        assert_eq!(LibraryPaths::relative_media("abc", ".mp4"), "media/abc.mp4");
        assert_eq!(LibraryPaths::relative_media("abc", ""), "media/abc");
    }

    #[test]
    fn a_relative_media_path_resolves_under_the_current_root() {
        let paths = LibraryPaths::new(r"D:\Sillage");
        assert_eq!(
            paths.resolve("media/abc.m4a"),
            Path::new(r"D:\Sillage").join("media/abc.m4a")
        );
    }

    #[test]
    fn an_output_filename_is_valid_on_windows() {
        let paths = LibraryPaths::new(r"D:\Sillage");
        assert!(paths
            .output("abc", "custom:plan-d-action")
            .ends_with("abc.custom-plan-d-action.md"));
        assert!(paths.output("abc", "summary").ends_with("abc.summary.md"));
    }

    #[test]
    fn a_transcript_round_trips_through_its_json_file() {
        let (_dir, library) = library();
        let mut original = transcript("a", "Entretien");
        original.edits.push(Edit::Replace {
            word: 1,
            text: "chien".to_string(),
        });

        library.save_transcript(&original).expect("save");
        assert_eq!(library.load_transcript("a").expect("load"), original);
    }

    #[test]
    fn saving_a_transcript_indexes_it_too() {
        let (_dir, library) = library();
        library
            .save_transcript(&transcript("a", "Entretien"))
            .expect("save");

        let row = library
            .db()
            .transcript_row("a")
            .expect("read")
            .expect("present");
        assert_eq!(row.title, "Entretien");
    }

    #[test]
    fn an_unknown_transcript_is_an_error_not_a_panic() {
        let (_dir, library) = library();
        assert!(matches!(
            library.load_transcript("nope"),
            Err(LibraryError::MissingTranscript(_))
        ));
    }

    #[test]
    fn peaks_round_trip_through_their_file() {
        let (_dir, library) = library();
        let peaks = Peaks {
            bucket_ms: DEFAULT_BUCKET_MS,
            values: vec![0, 128, 255, 64],
        };

        library.save_peaks("a", &peaks).expect("save");
        assert_eq!(library.load_peaks("a").expect("load"), peaks);
    }

    #[test]
    fn deleting_a_transcript_removes_its_files_as_well_as_its_row() {
        let (_dir, library) = library();
        let fixture = transcript("a", "Entretien");
        library.save_transcript(&fixture).expect("save");
        library
            .save_peaks(
                "a",
                &Peaks {
                    bucket_ms: DEFAULT_BUCKET_MS,
                    values: vec![1, 2, 3],
                },
            )
            .expect("peaks");

        let media = library.paths().resolve(&fixture.media_path);
        fs::write(&media, b"fake audio").expect("media");

        library.delete_transcript("a").expect("delete");

        assert!(!library.paths().transcript_json("a").exists());
        assert!(!library.paths().peaks("a").exists());
        assert!(!media.exists());
        assert_eq!(library.db().count_transcripts().expect("count"), 0);
    }

    #[test]
    fn deleting_a_transcript_whose_files_are_already_gone_still_works() {
        let (_dir, library) = library();
        library
            .save_transcript(&transcript("a", "Entretien"))
            .expect("save");
        fs::remove_file(library.paths().transcript_json("a")).expect("remove");

        library.delete_transcript("a").expect("delete");
    }

    #[test]
    fn a_partial_write_cannot_replace_a_good_record() {
        let (_dir, library) = library();
        library
            .save_transcript(&transcript("a", "Entretien"))
            .expect("save");

        // The temporary file of the atomic write must never be left behind, in particular not in
        // `data/`, where phase 03 will be listing files.
        let leftovers: Vec<_> = fs::read_dir(library.paths().data_dir())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftovers: {leftovers:?}");
    }

    #[test]
    fn relocating_keeps_every_entry_and_the_audio() {
        // ROADMAP phase 02: « Le déplacement du dossier bibliothèque conserve toutes les entrées
        // et l'audio ».
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library::open(dir.path().join("avant")).expect("open");

        for id in ["a", "b", "c"] {
            let fixture = transcript(id, id);
            library.save_transcript(&fixture).expect("save");
            fs::write(
                library.paths().resolve(&fixture.media_path),
                format!("audio de {id}"),
            )
            .expect("media");
        }
        library.db().enqueue("c", 1_000).expect("enqueue");
        let before = library.db().list_transcripts().expect("list");

        let moved = library
            .relocate(dir.path().join("après"))
            .expect("relocate");

        assert!(!dir.path().join("avant").exists(), "the old folder is gone");
        assert_eq!(moved.paths().root(), dir.path().join("après"));
        assert_eq!(moved.db().list_transcripts().expect("list"), before);
        assert_eq!(moved.db().queue().expect("queue").len(), 1);

        for id in ["a", "b", "c"] {
            let fixture = moved.load_transcript(id).expect("load");
            assert_eq!(
                fs::read_to_string(moved.paths().resolve(&fixture.media_path)).expect("audio"),
                format!("audio de {id}")
            );
        }
    }

    #[test]
    fn the_library_stays_usable_after_a_move() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library::open(dir.path().join("avant")).expect("open");
        library
            .save_transcript(&transcript("a", "Une"))
            .expect("save");

        let moved = library
            .relocate(dir.path().join("après"))
            .expect("relocate");
        moved
            .save_transcript(&transcript("b", "Deux"))
            .expect("save after move");

        assert_eq!(moved.db().count_transcripts().expect("count"), 2);
        assert_eq!(moved.db().search("chat", 10).expect("search").len(), 2);
    }

    #[test]
    fn moving_onto_an_occupied_folder_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library::open(dir.path().join("avant")).expect("open");
        library
            .save_transcript(&transcript("a", "Une"))
            .expect("save");

        let occupied = dir.path().join("occupé");
        fs::create_dir_all(&occupied).expect("mkdir");
        fs::write(occupied.join("important.txt"), b"ne pas perdre").expect("write");

        assert!(matches!(
            library.relocate(&occupied),
            Err(LibraryError::DestinationNotEmpty(_))
        ));
        assert!(
            occupied.join("important.txt").exists(),
            "the refusal must not have touched anything"
        );
        assert!(dir.path().join("avant").join("library.db").exists());
    }

    #[test]
    fn moving_into_the_library_itself_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("avant");
        let library = Library::open(&root).expect("open");

        assert!(matches!(
            library.relocate(root.join("dedans")),
            Err(LibraryError::DestinationInsideSource)
        ));
        assert!(root.join("library.db").exists());
    }

    #[test]
    fn moving_to_the_same_place_is_a_no_op() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("avant");
        let library = Library::open(&root).expect("open");
        library
            .save_transcript(&transcript("a", "Une"))
            .expect("save");

        let same = library.relocate(&root).expect("relocate");
        assert_eq!(same.db().count_transcripts().expect("count"), 1);
    }

    #[test]
    fn moving_to_an_empty_existing_folder_is_allowed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library::open(dir.path().join("avant")).expect("open");
        library
            .save_transcript(&transcript("a", "Une"))
            .expect("save");

        let empty = dir.path().join("vide");
        fs::create_dir_all(&empty).expect("mkdir");

        let moved = library.relocate(&empty).expect("relocate");
        assert_eq!(moved.db().count_transcripts().expect("count"), 1);
    }
}
