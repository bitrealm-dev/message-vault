// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;
use std::sync::{Arc, Mutex};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Arc::new(Mutex::new(AppState::new())))
        .invoke_handler(tauri::generate_handler![
            commands::extract::extract,
            commands::extract::cancel,
            commands::format::format,
            commands::ffmpeg::probe_ffmpeg_tools,
            commands::ffmpeg::set_ffmpeg_tools_dir,
            commands::push::push,
            commands::pull::pull,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
