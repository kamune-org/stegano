//! Tauri commands. All steganography lives in `stegano-core`; this layer only
//! moves bytes between the filesystem and the core.
//!
//! Files are read and written in Rust through the fs plugin, which also handles
//! Android `content://` URIs.

mod commands;
mod error;
mod jobs;
mod progress;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(android_picker())
        .setup(|app| {
            state::reset_work(app);
            app.manage(state::AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::inspect_carrier,
            commands::carrier_preview,
            commands::stat_files,
            commands::estimate_payload,
            commands::prepare_carrier,
            commands::import_begin,
            commands::import_chunk,
            commands::import_finish,
            commands::import_abort,
            commands::hide,
            commands::save_output,
            commands::discard_output,
            commands::reveal,
            commands::save_entry,
            commands::clear_revealed,
            commands::cancel_job,
            commands::pick_files,
            commands::pick_save_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn android_picker<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("stegano-android")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin("com.kamune_org.stegano", "PickerPlugin")?;
                app.manage(commands::Picker { handle });
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .build()
}
