//! Commands that find ffmpeg and ffprobe, and remember which folder they live in.
//!
//! Attachment convert and compress need those programs. The WebView cannot
//! search the disk or set process environment variables, so this process does
//! both.

use std::path::{Path, PathBuf};

use super::optional_trimmed;

/// Paths the Settings screen shows after looking for ffmpeg and ffprobe.
#[derive(serde::Serialize)]
pub struct FfmpegToolsProbeDto {
    pub ok: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub error: Option<String>,
}

/// Copy probe results into the JSON shape the UI expects.
fn probe_to_dto(probe: media::FfmpegToolsProbe) -> FfmpegToolsProbeDto {
    let ffmpeg_path = probe.ffmpeg_path.map(|path| path.display().to_string());
    let ffprobe_path = probe.ffprobe_path.map(|path| path.display().to_string());
    FfmpegToolsProbeDto {
        ok: probe.ok,
        ffmpeg_path,
        ffprobe_path,
        error: probe.error,
    }
}

/// Treat a blank folder string as "search the default PATH instead".
fn optional_tools_dir(dir: Option<&str>) -> Option<&Path> {
    let dir = optional_trimmed(dir)?;
    Some(Path::new(dir))
}

/// Ask this process whether ffmpeg and ffprobe are available.
///
/// When `dir` is set, look in that folder. When it is empty, search the
/// process PATH.
#[tauri::command]
pub fn probe_ffmpeg_tools(dir: Option<String>) -> FfmpegToolsProbeDto {
    let tools_dir = optional_tools_dir(dir.as_deref());
    let probe = media::probe_ffmpeg_tools(tools_dir);
    probe_to_dto(probe)
}

/// Ask this process to remember where ffmpeg and ffprobe live for this session.
///
/// An empty `dir` clears the override and goes back to searching PATH.
///
/// # Errors
///
/// Returns an error if `dir` is set but ffmpeg or ffprobe cannot be found
/// there.
#[tauri::command]
pub fn set_ffmpeg_tools_dir(dir: Option<String>) -> Result<FfmpegToolsProbeDto, String> {
    let trimmed = optional_trimmed(dir.as_deref());
    match trimmed {
        None => {
            // SAFETY: this desktop process owns MESSAGE_VAULT_IO_BIN for the
            // session. Clearing it here is how Settings turns off a custom
            // tools folder.
            unsafe { std::env::remove_var("MESSAGE_VAULT_IO_BIN") };
            media::set_tools_dir(None);
            Ok(probe_ffmpeg_tools(None))
        }
        Some(dir) => {
            let path = PathBuf::from(dir);
            let probe = media::probe_ffmpeg_tools(Some(path.as_path()));
            if !probe.ok {
                return Err(probe
                    .error
                    .unwrap_or_else(|| "ffmpeg tools not found".into()));
            }
            // SAFETY: this desktop process owns MESSAGE_VAULT_IO_BIN for the
            // session. Later ffmpeg lookups in this process read that value.
            unsafe { std::env::set_var("MESSAGE_VAULT_IO_BIN", &path) };
            media::set_tools_dir(Some(path));
            Ok(probe_to_dto(probe))
        }
    }
}
