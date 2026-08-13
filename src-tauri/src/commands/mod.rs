//! Tauri commands exposed to the frontend.

use std::sync::Mutex;

use tauri::State;

use crate::settings::{Appearance, Settings, SettingsError, SettingsStore};

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
        let next = Settings { appearance }.sanitized();
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
    use crate::settings::{Theme, DEFAULT_ACCENT, SETTINGS_FILE};

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
}
