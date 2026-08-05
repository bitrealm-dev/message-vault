use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use crate::state::AppState;

#[tauri::command]
pub async fn cancel(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn extract(
    _state: tauri::State<'_, Arc<Mutex<AppState>>>,
    _source: String,
    _path: String,
    _output_dir: String,
) -> Result<(), String> {
    Err("not implemented".to_string())
}
