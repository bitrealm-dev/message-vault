//! Import-time media rewrite before content-addressed asset store.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::media_tools::{
    self, JPEG_MIN_BYTES, MP3_MIN_BYTES, MP4_MIN_BYTES, MediaKind, ext_of, kind_of, path_str,
    probe_video_efficient,
};

/// How attachment files are handled during import.
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
    /// Parse a `--media` value: `copy`, `none`, `convert`, or `compress`;
    /// anything else is an error. Accepts the aliases `clone`, `skip`, and
    /// `disabled`.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "copy" | "clone" => Ok(Self::Copy),
            "none" | "skip" | "disabled" => Ok(Self::None),
            "convert" => Ok(Self::Convert),
            "compress" => Ok(Self::Compress),
            other => bail!("invalid --media '{other}' (expected copy, none, convert, or compress)"),
        }
    }

    /// Canonical flag value for this mode (`copy`, `none`, `convert`, or `compress`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::None => "none",
            Self::Convert => "convert",
            Self::Compress => "compress",
        }
    }
}

/// The file to store for one attachment, after the mode's transformation.
#[derive(Debug)]
pub struct ResolvedMedia {
    /// Path of the file to store in the vault.
    pub path: PathBuf,
    /// MIME type of the attachment, when known.
    pub mime_type: Option<String>,
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
    media_tools::run_ffmpeg(
        &[
            "-i",
            path_str(source_path)?,
            "-frames:v",
            "1",
            "-update",
            "1",
            "-q:v",
            "2",
            path_str(&out)?,
        ],
        None,
    )?;
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
    media_tools::run_ffmpeg(
        &[
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
        ],
        None,
    )?;
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
    media_tools::run_ffmpeg(
        &[
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
        ],
        None,
    )?;
    Ok(Some(ResolvedMedia {
        path: out,
        mime_type: Some("audio/mpeg".into()),
    }))
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

fn ensure_ffmpeg() -> Result<()> {
    if media_tools::tool_on_path("ffmpeg") {
        return Ok(());
    }
    bail!("ffmpeg is required for --media convert|compress (not found on PATH)");
}
