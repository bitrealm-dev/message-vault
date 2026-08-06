//! Settings Tauri commands — read `export.ini` sections
//! (vault credentials, default output directory).

use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
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
