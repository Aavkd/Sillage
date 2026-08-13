//! Tauri commands exposed to the frontend, and the state they read.

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::State;

use crate::ingest::{Ingestor, Tool, Tools};
use crate::library::Library;
use crate::settings::{Appearance, Settings, SettingsError, SettingsStore};

/// The open library, or the French message explaining why it could not be opened.
///
/// Phase 02 has no screen, so nothing reads it yet; it is here so that the folder is created and
/// the migrations run at the first launch after this phase, and so that phases 03 and after have
/// a single place to reach the library from.
pub struct LibraryState {
    inner: Mutex<Result<Library, String>>,
}

impl LibraryState {
    /// Opens the library, keeping the failure rather than propagating it.
    #[must_use]
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let inner = Library::open(root).map_err(|err| err.to_string());
        Self {
            inner: Mutex::new(inner),
        }
    }

    /// Runs `f` against the open library, or returns the French message saying why it is not.
    pub fn with<T>(&self, f: impl FnOnce(&Library) -> T) -> Result<T, String> {
        match &*self.inner.lock().expect("library mutex poisoned") {
            Ok(library) => Ok(f(library)),
            Err(message) => Err(message.clone()),
        }
    }

    /// The root of the open library, if there is one.
    #[must_use]
    pub fn root(&self) -> Option<PathBuf> {
        self.with(|library| library.paths().root().to_path_buf())
            .ok()
    }

    /// Whether the library opened.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.with(|_| ()).is_ok()
    }

    /// The message to show if it did not.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.with(|_| ()).err()
    }
}

/// The ingestion pipeline, bound to the binaries this installation shipped with.
///
/// Resolved once at launch from the Tauri resource directory. ROADMAP phase 03 task 1 forbids
/// depending on the PATH, and the way that stays true is that there is exactly one place where
/// the paths are decided — here — and it never looks anywhere else.
pub struct IngestState {
    ingestor: Ingestor,
}

impl IngestState {
    #[must_use]
    pub fn new(resources_dir: impl Into<PathBuf>) -> Self {
        Self {
            ingestor: Ingestor::new(Tools::in_directory(resources_dir.into())),
        }
    }

    #[must_use]
    pub fn ingestor(&self) -> &Ingestor {
        &self.ingestor
    }

    /// Whether ffmpeg and ffprobe are where they should be.
    ///
    /// Read at launch so the library screen of phase 05 can say the installation is incomplete,
    /// rather than letting the first dropped file be what discovers it.
    #[must_use]
    pub fn tools_available(&self) -> bool {
        self.ingestor.tools().are_present()
    }

    /// The binary that is missing, if any.
    #[must_use]
    pub fn missing_tool(&self) -> Option<Tool> {
        self.ingestor.tools().missing()
    }
}

/// Settings held in memory, kept in sync with the file on every write.
pub struct SettingsState {
    pub store: SettingsStore,
    pub current: Mutex<Settings>,
}

impl SettingsState {
    pub fn new(store: SettingsStore) -> Self {
        let current = Mutex::new(store.load());
        Self { store, current }
    }

    pub fn snapshot(&self) -> Settings {
        self.current
            .lock()
            .expect("settings mutex poisoned")
            .clone()
    }

    /// Applies an appearance change and persists it before acknowledging it.
    ///
    /// The file is written first: if the write fails, the in-memory value is left alone so the
    /// running app and the config file cannot disagree. Kept free of Tauri types so the whole
    /// path — sanitize, persist, re-read on next launch — is covered by ordinary tests.
    pub fn update_appearance(&self, appearance: Appearance) -> Result<Settings, SettingsError> {
        // Only the appearance changes: every other section is carried over untouched, so a
        // theme switch can never reset a setting the user made elsewhere.
        let mut next = self.snapshot();
        next.appearance = appearance;
        let next = next.sanitized();
        self.store.save(&next)?;

        *self.current.lock().expect("settings mutex poisoned") = next.clone();
        Ok(next)
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Settings {
    state.snapshot()
}

/// Persists an appearance change.
///
/// Returns the value actually stored: the accent comes back normalized (uppercase), or replaced
/// by the default if the frontend ever sent something malformed. The frontend renders what it
/// gets back rather than what it sent, so the two can never drift.
#[tauri::command]
pub fn set_appearance(
    state: State<'_, SettingsState>,
    appearance: Appearance,
) -> Result<Settings, String> {
    state
        .update_appearance(appearance)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{LibrarySettings, Theme, DEFAULT_ACCENT, SETTINGS_FILE};

    fn state() -> (tempfile::TempDir, SettingsState) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SettingsStore::new(dir.path().join(SETTINGS_FILE));
        (dir, SettingsState::new(store))
    }

    #[test]
    fn starts_from_the_defaults_when_there_is_no_file() {
        let (_dir, state) = state();
        assert_eq!(state.snapshot(), Settings::default());
    }

    #[test]
    fn an_appearance_change_is_visible_immediately() {
        let (_dir, state) = state();
        let stored = state
            .update_appearance(Appearance {
                theme: Theme::Light,
                accent: "#8E9A5B".to_string(),
            })
            .expect("update");

        assert_eq!(stored, state.snapshot());
        assert_eq!(stored.appearance.theme, Theme::Light);
    }

    #[test]
    fn the_stored_accent_comes_back_normalized() {
        let (_dir, state) = state();
        let stored = state
            .update_appearance(Appearance {
                theme: Theme::Dark,
                accent: "#d8d83c".to_string(),
            })
            .expect("update");

        assert_eq!(stored.appearance.accent, "#D8D83C", "uppercased");
    }

    #[test]
    fn a_malformed_accent_is_replaced_rather_than_stored() {
        let (_dir, state) = state();
        let stored = state
            .update_appearance(Appearance {
                theme: Theme::Dark,
                accent: "not a colour".to_string(),
            })
            .expect("update");

        assert_eq!(stored.appearance.accent, DEFAULT_ACCENT);
    }

    #[test]
    fn theme_and_accent_survive_a_restart() {
        // ROADMAP.md phase 01, last acceptance criterion. A second SettingsState over the same
        // file is exactly what the next launch builds.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(SETTINGS_FILE);

        let chosen = Appearance {
            theme: Theme::Light,
            accent: "#B9755F".to_string(),
        };

        let first = SettingsState::new(SettingsStore::new(&path));
        first.update_appearance(chosen.clone()).expect("update");
        drop(first);

        let relaunched = SettingsState::new(SettingsStore::new(&path));
        assert_eq!(relaunched.snapshot().appearance, chosen);
    }

    #[test]
    fn successive_changes_each_survive_a_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(SETTINGS_FILE);

        for (theme, accent) in [
            (Theme::Dark, "#E08A4B"),
            (Theme::Light, "#C98A2E"),
            (Theme::Dark, "#8E9A5B"),
        ] {
            let state = SettingsState::new(SettingsStore::new(&path));
            state
                .update_appearance(Appearance {
                    theme,
                    accent: accent.to_string(),
                })
                .expect("update");

            let relaunched = SettingsState::new(SettingsStore::new(&path));
            assert_eq!(relaunched.snapshot().appearance.theme, theme);
            assert_eq!(relaunched.snapshot().appearance.accent, accent);
        }
    }

    #[test]
    fn changing_the_appearance_leaves_the_other_sections_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SettingsStore::new(dir.path().join(SETTINGS_FILE));
        store
            .save(&Settings {
                library: LibrarySettings {
                    folder: Some(PathBuf::from(r"E:\Archives\Sillage")),
                },
                ..Settings::default()
            })
            .expect("save");

        let state = SettingsState::new(store);
        let stored = state
            .update_appearance(Appearance {
                theme: Theme::Light,
                accent: "#8E9A5B".to_string(),
            })
            .expect("update");

        assert_eq!(
            stored.library.folder,
            Some(PathBuf::from(r"E:\Archives\Sillage")),
            "the library folder must survive a theme change"
        );
    }

    #[test]
    fn the_library_state_holds_an_open_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = LibraryState::open(dir.path().join("Sillage"));

        assert!(state.is_open());
        assert_eq!(state.error(), None);
        assert_eq!(state.root(), Some(dir.path().join("Sillage")));
        let count = state
            .with(|library| library.db().count_transcripts())
            .expect("library open")
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn a_library_that_cannot_be_opened_keeps_its_message_instead_of_panicking() {
        // A file where the folder should be: the closest reproducible stand-in for the external
        // drive left at home.
        let dir = tempfile::tempdir().expect("tempdir");
        let occupied = dir.path().join("Sillage");
        std::fs::write(&occupied, b"pas un dossier").expect("write");

        let state = LibraryState::open(&occupied);
        assert!(!state.is_open());
        assert!(state.error().is_some(), "the failure must be reportable");
        assert_eq!(state.root(), None);
    }
}
