//! Import-time media rewrite before content-addressed asset store.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::media_tools::{self, MediaKind, ext_of, kind_of, path_str};

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
    if media_tools::skip_image_conversion(&ext, size, compress) {
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
        &media_tools::image_to_jpeg_args(path_str(source_path)?, path_str(&out)?),
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
    if media_tools::skip_video_conversion(source_path, &ext, size, compress) {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: Some("video/mp4".into()),
        }));
    }
    ensure_ffmpeg()?;
    let out = work_dir.join(format!("vid-{}.mp4", stem_token(source_path)));
    media_tools::run_ffmpeg(
        &media_tools::video_to_mp4_args(path_str(source_path)?, path_str(&out)?, compress),
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
    if media_tools::skip_audio_conversion(&ext, size, compress) {
        return Ok(Some(ResolvedMedia {
            path: source_path.to_path_buf(),
            mime_type: Some("audio/mpeg".into()),
        }));
    }
    ensure_ffmpeg()?;
    let out = work_dir.join(format!("aud-{}.mp3", stem_token(source_path)));
    media_tools::run_ffmpeg(
        &media_tools::audio_to_mp3_args(path_str(source_path)?, path_str(&out)?),
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
