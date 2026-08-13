//! Reading and writing the application configuration.
//!
//! Phase 01 only persists appearance (theme + accent), but the file format is the one every
//! later phase extends: every field carries `#[serde(default)]` so an older config file keeps
//! loading once new sections are added, and an unknown/corrupt file degrades to defaults
//! instead of failing the launch.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::LibraryPaths;

/// Default accent, `#E08A4B` — DESIGN.md §2.5.
pub const DEFAULT_ACCENT: &str = "#E08A4B";

/// Basename of the configuration file inside the app config directory.
pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("impossible d'écrire les réglages : {0}")]
    Write(#[from] io::Error),
    #[error("impossible de sérialiser les réglages : {0}")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    pub theme: Theme,
    /// Accent as `#RRGGBB`, uppercase.
    pub accent: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            accent: DEFAULT_ACCENT.to_string(),
        }
    }
}

/// Where the library folder lives (CONCEPTION.md §3.4).
///
/// `None` means « wherever the default is », rather than a path frozen at first launch: the
/// default is resolved from the user's Documents folder at every start, so a machine whose
/// Documents folder has moved keeps working.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LibrarySettings {
    pub folder: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub appearance: Appearance,
    pub library: LibrarySettings,
}

/// Validates `#RRGGBB` and normalizes it to uppercase.
///
/// DESIGN.md §12 specifies this exact contract for the editable hex field of the Appearance
/// tab: `^#[0-9A-Fa-f]{6}$`, normalized uppercase. Anything else is refused rather than
/// silently coerced, so a bad value can never replace a working accent.
pub fn normalize_accent(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return None;
    }
    if !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    Some(raw.to_ascii_uppercase())
}

impl Settings {
    /// Replaces any value that would break the UI with its default, leaving the rest intact.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.appearance.accent =
            normalize_accent(&self.appearance.accent).unwrap_or_else(|| DEFAULT_ACCENT.to_string());
        self
    }

    /// The library folder actually to use: the chosen one, or `<Documents>/Sillage`.
    #[must_use]
    pub fn library_root(&self, documents_dir: impl AsRef<Path>) -> PathBuf {
        self.library
            .folder
            .clone()
            .unwrap_or_else(|| LibraryPaths::default_root(documents_dir))
    }
}

/// A settings file on disk.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the settings, falling back to defaults when the file is absent or unreadable.
    ///
    /// A corrupt config must never prevent the app from starting: the user would have no way
    /// to repair it from inside an app that refuses to open.
    pub fn load(&self) -> Settings {
        let Ok(raw) = fs::read_to_string(&self.path) else {
            return Settings::default();
        };
        serde_json::from_str::<Settings>(&raw)
            .unwrap_or_default()
            .sanitized()
    }

    /// Writes the settings atomically: a crash mid-write leaves the previous file intact.
    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(settings)?;

        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, body)?;
        // `rename` replaces the destination on Windows as well as on unix.
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SettingsStore::new(dir.path().join(SETTINGS_FILE));
        (dir, store)
    }

    #[test]
    fn defaults_match_the_design_reference() {
        let settings = Settings::default();
        assert_eq!(settings.appearance.theme, Theme::Dark);
        assert_eq!(settings.appearance.accent, "#E08A4B");
    }

    #[test]
    fn accent_is_validated_and_uppercased() {
        assert_eq!(normalize_accent("#e08a4b").as_deref(), Some("#E08A4B"));
        assert_eq!(normalize_accent("#8E9A5B").as_deref(), Some("#8E9A5B"));

        assert_eq!(normalize_accent("E08A4B"), None, "missing #");
        assert_eq!(normalize_accent("#E08A4"), None, "too short");
        assert_eq!(normalize_accent("#E08A4BB"), None, "too long");
        assert_eq!(normalize_accent("#GGGGGG"), None, "not hex");
        assert_eq!(normalize_accent(""), None);
    }

    #[test]
    fn the_library_folder_defaults_to_documents_and_is_overridable() {
        let documents = Path::new(r"C:\Users\x\Documents");

        let default = Settings::default();
        assert_eq!(default.library.folder, None);
        assert_eq!(default.library_root(documents), documents.join("Sillage"));

        let chosen = Settings {
            library: LibrarySettings {
                folder: Some(PathBuf::from(r"E:\Archives\Sillage")),
            },
            ..Settings::default()
        };
        assert_eq!(
            chosen.library_root(documents),
            PathBuf::from(r"E:\Archives\Sillage")
        );
    }

    #[test]
    fn a_config_written_before_the_library_section_existed_still_loads() {
        // Exactly the file phase 01 wrote. It must keep working, and pick up the default folder.
        let (_dir, store) = store();
        fs::write(
            store.path(),
            r##"{"appearance":{"theme":"light","accent":"#8E9A5B"}}"##,
        )
        .expect("write");

        let loaded = store.load();
        assert_eq!(loaded.appearance.accent, "#8E9A5B");
        assert_eq!(loaded.library.folder, None);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let (_dir, store) = store();
        assert_eq!(store.load(), Settings::default());
    }

    #[test]
    fn corrupt_file_loads_defaults_instead_of_failing() {
        let (_dir, store) = store();
        fs::write(store.path(), "{ not json at all").expect("write");
        assert_eq!(store.load(), Settings::default());
    }

    #[test]
    fn invalid_accent_in_file_falls_back_to_default() {
        let (_dir, store) = store();
        fs::write(
            store.path(),
            r#"{"appearance":{"theme":"light","accent":"chartreuse"}}"#,
        )
        .expect("write");

        let loaded = store.load();
        assert_eq!(loaded.appearance.theme, Theme::Light, "theme is kept");
        assert_eq!(
            loaded.appearance.accent, DEFAULT_ACCENT,
            "accent is repaired"
        );
    }

    #[test]
    fn partial_file_keeps_defaults_for_absent_fields() {
        let (_dir, store) = store();
        fs::write(store.path(), r#"{"appearance":{"theme":"light"}}"#).expect("write");

        let loaded = store.load();
        assert_eq!(loaded.appearance.theme, Theme::Light);
        assert_eq!(loaded.appearance.accent, DEFAULT_ACCENT);
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_dir, store) = store();
        let settings = Settings {
            appearance: Appearance {
                theme: Theme::Light,
                accent: "#8E9A5B".to_string(),
            },
            library: LibrarySettings {
                folder: Some(PathBuf::from(r"E:\Sillage")),
            },
        };

        store.save(&settings).expect("save");
        assert_eq!(store.load(), settings);
    }

    #[test]
    fn saving_over_an_existing_file_replaces_it() {
        // The atomic write renames onto an existing path, which is the ordinary case every
        // time the user picks a new accent. `rename` must replace rather than fail.
        let (_dir, store) = store();
        store.save(&Settings::default()).expect("first save");

        let updated = Settings {
            appearance: Appearance {
                theme: Theme::Light,
                accent: "#D8D83C".to_string(),
            },
            ..Settings::default()
        };
        store.save(&updated).expect("second save");

        assert_eq!(store.load(), updated);
    }

    #[test]
    fn repeated_saves_leave_a_single_file() {
        let (dir, store) = store();
        for accent in ["#E08A4B", "#D9694E", "#8E9A5B"] {
            store
                .save(&Settings {
                    appearance: Appearance {
                        theme: Theme::Dark,
                        accent: accent.to_string(),
                    },
                    ..Settings::default()
                })
                .expect("save");
        }

        let files: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(files.len(), 1, "unexpected files: {files:?}");
        assert_eq!(store.load().appearance.accent, "#8E9A5B");
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SettingsStore::new(dir.path().join("nested").join("deep").join(SETTINGS_FILE));

        store.save(&Settings::default()).expect("save");
        assert!(store.path().exists());
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let (dir, store) = store();
        store.save(&Settings::default()).expect("save");

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name != SETTINGS_FILE)
            .collect();
        assert!(leftovers.is_empty(), "unexpected files: {leftovers:?}");
    }
}
