pub mod catalog;
pub mod cli;
pub mod commands;
pub mod engine;
pub mod features;
pub mod lrlua;
pub mod mcp;
pub mod presets;
pub mod raw;
pub mod style;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(desktop)]
            register_edit_shortcut(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_styles,
            commands::delete_style,
            commands::train_style,
            commands::list_looks,
            commands::look_params,
            commands::lightroom_state,
            commands::install_plugin,
            commands::sample_photos,
            commands::load_preview,
            commands::predict_one,
            commands::edit_selection,
            commands::job_status,
            commands::retouch_options,
            commands::mcp_command,
            commands::catalog_photos,
            commands::thumbnail,
            commands::edit_photos,
            commands::photo_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Lightroom plugins can't define shortcuts or add to the right-click menu,
/// so the app registers a system-wide one: edit whatever is selected in
/// Lightroom. Works while Lightroom is in front, as long as Retoucan is running.
#[cfg(desktop)]
pub const EDIT_SHORTCUT: &str = if cfg!(target_os = "macos") { "Control+Alt+Super+E" } else { "Control+Alt+Shift+E" };

#[cfg(desktop)]
fn register_edit_shortcut(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{Emitter, Manager};
    use tauri_plugin_global_shortcut::ShortcutState;

    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_shortcuts([EDIT_SHORTCUT])?
            .with_handler(|app, _shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }
                eprintln!("retoucan: edit shortcut pressed");
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let result = commands::edit_selection_blocking();
                    if let Err(e) = &result {
                        eprintln!("retoucan: shortcut edit failed: {e}");
                    }
                    // Come to the front so the progress or error is visible;
                    // the window switches to the Edit tab and follows the job.
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                    let _ = app.emit("shortcut-edit", result);
                });
            })
            .build(),
    )?;
    Ok(())
}
