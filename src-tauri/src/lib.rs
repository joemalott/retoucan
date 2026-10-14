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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
