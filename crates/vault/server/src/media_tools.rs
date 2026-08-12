//! Shared ffmpeg/ffprobe helpers for import convert and derived-asset processing.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

pub const JPEG_MIN_BYTES: u64 = 500 * 1024;
pub const MP3_MIN_BYTES: u64 = 100 * 1024;
pub const MP4_MIN_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Other,
}

pub fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()))
        .unwrap_or_default()
}

pub fn kind_of(path: &Path, mime: Option<&str>) -> MediaKind {
    let ext = ext_of(path);
    if ext == ".gif" || mime == Some("image/gif") {
        return MediaKind::Other;
    }
    const IMAGE: &[&str] = &[
        ".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff", ".heic", ".heif",
    ];
    const VIDEO: &[&str] = &[
        ".mp4", ".m4v", ".mov", ".3gp", ".3gpp", ".webm", ".mpeg", ".mpg", ".mkv",
    ];
    const AUDIO: &[&str] = &[".mp3", ".m4a", ".aac", ".caf", ".amr", ".wav", ".ogg"];
    if IMAGE.contains(&ext.as_str()) || mime.is_some_and(|m| m.starts_with("image/")) {
        return MediaKind::Image;
    }
    if VIDEO.contains(&ext.as_str()) || mime.is_some_and(|m| m.starts_with("video/")) {
        return MediaKind::Video;
    }
    if AUDIO.contains(&ext.as_str()) || mime.is_some_and(|m| m.starts_with("audio/")) {
        return MediaKind::Audio;
    }
    MediaKind::Other
}

pub fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("non-utf8 path {}", path.display()))
}

pub fn tool_on_path(name: &str) -> bool {
    Command::new(name)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run_ffmpeg(args: &[&str], cleanup_on_fail: Option<&Path>) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .context("spawn ffmpeg")?;
    if output.status.success() {
        return Ok(());
    }
    if let Some(path) = cleanup_on_fail {
        let _ = fs::remove_file(path);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    bail!(
        "ffmpeg failed: {}",
        if !stderr.trim().is_empty() {
            stderr
        } else {
            stdout
        }
    )
}

/// True when the video is already h264, ≤720p on the short side, and ≤30.01 fps.
pub fn probe_video_efficient(source_path: &Path) -> bool {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,avg_frame_rate",
            "-of",
            "json",
        ])
        .arg(source_path)
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return false;
    };
    let Some(s) = v
        .get("streams")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
    else {
        return false;
    };
    if s.get("codec_name").and_then(|c| c.as_str()) != Some("h264") {
        return false;
    }
    let w = s.get("width").and_then(|x| x.as_u64()).unwrap_or(0);
    let h = s.get("height").and_then(|x| x.as_u64()).unwrap_or(0);
    if w.min(h) > 720 {
        return false;
    }
    let rate = s
        .get("avg_frame_rate")
        .and_then(|x| x.as_str())
        .unwrap_or("0/1");
    let mut parts = rate.split('/');
    let num: f64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let den: f64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1.0);
    let fps = if den == 0.0 { 0.0 } else { num / den };
    fps > 0.0 && fps <= 30.01
}
