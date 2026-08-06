//! Settings Tauri commands — read and write `export.ini` sections
//! (vault credentials, default output directory).

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub vault_url: String,
    pub vault_username: String,
    pub vault_key: String,
    pub default_output_dir: String,
}

#[tauri::command]
pub async fn load_settings(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<AppSettings, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    Ok(AppSettings {
        vault_url: st.ini.vault.url.clone(),
        vault_username: st.ini.vault.username.clone(),
        vault_key: st.ini.vault.key.clone(),
        default_output_dir: st.ini.backup.output.clone(),
    })
}

#[tauri::command]
pub async fn save_settings(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    settings: AppSettings,
) -> Result<(), String> {
    let mut st = state.lock().map_err(|e| e.to_string())?;
    st.ini.vault.url = settings.vault_url;
    st.ini.vault.username = settings.vault_username;
    st.ini.vault.key = settings.vault_key;
    st.ini.backup.output = settings.default_output_dir;
    let form = st.form.clone();
    st.ini.save(&form).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_errors(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<String>, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    Ok(st.errors.clone())
}

#[tauri::command]
pub async fn clear_errors(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut st = state.lock().map_err(|e| e.to_string())?;
    st.errors.clear();
    Ok(())
}
