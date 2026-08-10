//! Path helpers for desktop system settings.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HomeDirInfo {
    pub path: String,
    /// `std::env::consts::OS` (`linux`, `macos`, `windows`, …).
    pub os: String,
}

#[tauri::command]
pub fn home_dir() -> Result<HomeDirInfo, String> {
    let path = dirs::home_dir()
        .ok_or_else(|| "Could not determine the user home directory".to_string())?;
    Ok(HomeDirInfo {
        path: path.display().to_string(),
        os: std::env::consts::OS.to_string(),
    })
}
