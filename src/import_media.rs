//! Import-time media rewrite before content-addressed asset store.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

const JPEG_MIN_BYTES: u64 = 500 * 1024;
const MP3_MIN_BYTES: u64 = 100 * 1024;
const MP4_MIN_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaMode {
    /// Hash/copy attachment files as-is (default).
    #[default]
    Copy,
    /// Skip attachment files and metadata.
    None,
    /// Convert non-browser-friendly media to JPEG/MP4/MP3.
    Convert,
    /// Convert and recompress oversized media (process-assets thresholds).
    Compress,
}

impl MediaMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "copy" | "clone" => Ok(Self::Copy),
            "none" | "skip" | "disabled" => Ok(Self::None),
            "convert" => Ok(Self::Convert),
            "compress" => Ok(Self::Compress),
            other => bail!(
                "invalid --media '{other}' (expected copy, none, convert, or compress)"
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::None => "none",
            Self::Convert => "convert",
            Self::Compress => "compress",
        }
    }
}

#[derive(Debug)]
pub struct ResolvedMedia {
    pub path: PathBuf,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Video,
    Audio,
    Other,
}

/// Resolve the file bytes to store for one attachment.
///
/// Returns `Ok(None)` when the attachment should be omitted (`MediaMode::None`).
pub fn resolve_for_store(
    source_path: &Path,
    mime: Option<&str>,
    mode: MediaMode,
    work_dir: &Path,
) -> Result<Option<ResolvedMedia>> {
    match mode {
        MediaMode::None => Ok(None),
        MediaMode::Copy => Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: mime.map(str::to_string),
        })),
        MediaMode::Convert | MediaMode::Compress => {
            transform(source_path, mime, mode == MediaMode::Compress, work_dir)
        }
    }
}

fn transform(
    source_path: &Path,
    mime: Option<&str>,
    compress: bool,
    work_dir: &Path,
) -> Result<Option<ResolvedMedia>> {
    if !source_path.is_file() {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: mime.map(str::to_string),
        }));
    }
    let kind = kind_of(source_path, mime);
    match kind {
        MediaKind::Other => Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: mime.map(str::to_string),
        })),
        MediaKind::Image => transform_image(source_path, compress, work_dir),
        MediaKind::Video => transform_video(source_path, compress, work_dir),
        MediaKind::Audio => transform_audio(source_path, compress, work_dir),
    }
}

fn transform_image(
    source_path: &Path,
    compress: bool,
    work_dir: &Path,
) -> Result<Option<ResolvedMedia>> {
    let ext = ext_of(source_path);
    let size = fs::metadata(source_path)?.len();
    let is_jpeg = ext == ".jpg" || ext == ".jpeg";
    if is_jpeg && (!compress || size <= JPEG_MIN_BYTES) {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: Some("image/jpeg".into()),
        }));
    }
    if ext == ".gif" {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: Some("image/gif".into()),
        }));
    }
    ensure_ffmpeg()?;
    let out = work_dir.join(format!("img-{}.jpg", stem_token(source_path)));
    run_ffmpeg(&[
        "-i",
        path_str(source_path)?,
        "-frames:v",
        "1",
        "-update",
        "1",
        "-q:v",
        "2",
        path_str(&out)?,
    ])?;
    Ok(Some(ResolvedMedia {
        path: out,
        mime_type: Some("image/jpeg".into()),
    }))
}

fn transform_video(
    source_path: &Path,
    compress: bool,
    work_dir: &Path,
) -> Result<Option<ResolvedMedia>> {
    let ext = ext_of(source_path);
    let size = fs::metadata(source_path)?.len();
    if ext == ".mp4" && (!compress || size <= MP4_MIN_BYTES || probe_video_efficient(source_path)) {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: Some("video/mp4".into()),
        }));
    }
    ensure_ffmpeg()?;
    let out = work_dir.join(format!("vid-{}.mp4", stem_token(source_path)));
    run_ffmpeg(&[
        "-i",
        path_str(source_path)?,
        "-vf",
        "scale='if(gt(iw,ih),-2,min(720,iw))':'if(gt(iw,ih),min(720,ih),-2)',fps=30",
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-crf",
        if compress { "28" } else { "23" },
        "-c:a",
        "aac",
        "-b:a",
        "96k",
        "-movflags",
        "+faststart",
        path_str(&out)?,
    ])?;
    Ok(Some(ResolvedMedia {
        path: out,
        mime_type: Some("video/mp4".into()),
    }))
}

fn transform_audio(
    source_path: &Path,
    compress: bool,
    work_dir: &Path,
) -> Result<Option<ResolvedMedia>> {
    let ext = ext_of(source_path);
    let size = fs::metadata(source_path)?.len();
    if ext == ".mp3" && (!compress || size <= MP3_MIN_BYTES) {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: Some("audio/mpeg".into()),
        }));
    }
    ensure_ffmpeg()?;
    let out = work_dir.join(format!("aud-{}.mp3", stem_token(source_path)));
    run_ffmpeg(&[
        "-i",
        path_str(source_path)?,
        "-vn",
        "-ac",
        "1",
        "-c:a",
        "libmp3lame",
        "-q:a",
        "6",
        path_str(&out)?,
    ])?;
    Ok(Some(ResolvedMedia {
        path: out,
        mime_type: Some("audio/mpeg".into()),
    }))
}

fn kind_of(path: &Path, mime: Option<&str>) -> MediaKind {
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

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn stem_token(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(24)
        .collect()
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("non-utf8 path {}", path.display()))
}

fn ensure_ffmpeg() -> Result<()> {
    if Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    bail!("ffmpeg is required for --media convert|compress (not found on PATH)");
}

fn run_ffmpeg(args: &[&str]) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .context("spawn ffmpeg")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ffmpeg failed: {}", stderr.trim());
    }
    Ok(())
}

fn probe_video_efficient(source_path: &Path) -> bool {
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
