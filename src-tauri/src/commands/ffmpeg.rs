//! Tauri commands for probing and setting ffmpeg/ffprobe tool locations.

use std::path::{Path, PathBuf};

#[derive(serde::Serialize)]
pub struct FfmpegToolsProbeDto {
    pub ok: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub error: Option<String>,
}

fn probe_to_dto(p: media::FfmpegToolsProbe) -> FfmpegToolsProbeDto {
    FfmpegToolsProbeDto {
        ok: p.ok,
        ffmpeg_path: p.ffmpeg_path.map(|x| x.display().to_string()),
        ffprobe_path: p.ffprobe_path.map(|x| x.display().to_string()),
        error: p.error,
    }
}

#[tauri::command]
pub fn probe_ffmpeg_tools(dir: Option<String>) -> FfmpegToolsProbeDto {
    let path = dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Path::new);
    let p = media::probe_ffmpeg_tools(path);
    probe_to_dto(p)
}

#[tauri::command]
pub fn set_ffmpeg_tools_dir(dir: Option<String>) -> Result<FfmpegToolsProbeDto, String> {
    let trimmed = dir.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match trimmed {
        None => {
            // SAFETY: desktop process owns this env for the session.
            unsafe { std::env::remove_var("MESSAGE_VAULT_IO_BIN") };
            media::set_tools_dir(None);
            Ok(probe_ffmpeg_tools(None))
        }
        Some(s) => {
            let path = PathBuf::from(s);
            let probe = media::probe_ffmpeg_tools(Some(path.as_path()));
            if !probe.ok {
                return Err(probe
                    .error
                    .unwrap_or_else(|| "ffmpeg tools not found".into()));
            }
            unsafe { std::env::set_var("MESSAGE_VAULT_IO_BIN", &path) };
            media::set_tools_dir(Some(path));
            Ok(probe_to_dto(probe))
        }
    }
}
