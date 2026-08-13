pub mod commands;
pub mod settings;

use tauri::Manager;

use crate::commands::SettingsState;
use crate::settings::{SettingsStore, SETTINGS_FILE};

/// Brings the existing window forward. Used by `single-instance`: a second launch must never
/// open a second window (CONCEPTION.md §6 — the Explorer context menu of phase 09 depends on
/// this routing, and the plugin has to be registered before every other one to work).
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // Must be the first plugin registered, per the plugin's own requirement.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Phase 09 will route `_argv` (the file passed by the Explorer context menu)
            // into the queue. For now a second launch just surfaces the running window.
            focus_main_window(app);
        }));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let store = SettingsStore::new(config_dir.join(SETTINGS_FILE));
            app.manage(SettingsState::new(store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_appearance,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Sillage");
}
