pub mod commands;
pub mod db;
pub mod ingest;
pub mod library;
pub mod model;
pub mod settings;

use tauri::path::BaseDirectory;
use tauri::Manager;

use crate::commands::{IngestState, LibraryState, SettingsState};
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
            let settings = SettingsState::new(store);

            // The library folder is opened at launch, and a failure to open it is carried in the
            // state rather than raised here: an unreachable folder — an external drive left at
            // home — must leave the application usable enough to point at another one.
            let documents = app.path().document_dir()?;
            let root = settings.snapshot().library_root(documents);
            app.manage(LibraryState::open(root));
            app.manage(settings);

            // ffmpeg and ffprobe ship with the application (ROADMAP phase 03, task 1). The
            // resource directory is the only place they are ever looked for: a broken or absent
            // ffmpeg on the user's PATH must change nothing about how Sillage behaves.
            let resources = app
                .path()
                .resolve(crate::ingest::RESOURCE_DIR, BaseDirectory::Resource)?;
            let ingest = IngestState::new(resources);
            if let Some(missing) = ingest.missing_tool() {
                // Not fatal: the rest of the application works, and phase 05 shows the state of
                // the engine on the library screen. Failing the launch would leave the user with
                // an app that will not open and no way to find out why.
                eprintln!(
                    "Sillage : {} est introuvable dans les ressources ; l'ingestion est indisponible.",
                    missing.file_name()
                );
            }
            app.manage(ingest);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_appearance,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Sillage");
}
