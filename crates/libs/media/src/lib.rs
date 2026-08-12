//! Convert or compress attachment media under a converter export directory.
//!
//! Modes:
//! - **Disabled** — do not copy/write attachment files (CLI exporters)
//! - **Clone** — leave exported files as-is (post-process no-op)
//! - **Convert** — standardize images→`.jpg`, videos→`.mp4`, audio→`.mp3`
//! - **Compress** — size-oriented re-encode with optional video knobs
//!
//! Requires `ffmpeg` / `ffprobe` for convert/compress (beside the running
//! binary, in `MESSAGE_VAULT_IO_BIN`, or on `PATH`).

mod process;
mod size;
mod tools;

pub use process::{MediaReport, process_attachments_dir, process_attachments_dir_with_log};
use size::parse_size;
pub use tools::{FfmpegToolsProbe, ffmpeg_available, probe_ffmpeg_tools, set_tools_dir, tools_dir};

use std::fmt;
use std::str::FromStr;

/// Attachment media handling after export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaMode {
    /// Do not write attachment files during export.
    Disabled,
    #[default]
    Clone,
    Convert,
    Compress,
}

impl MediaMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Clone => "clone",
            Self::Convert => "convert",
            Self::Compress => "compress",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "disabled" | "none" | "skip" => Some(Self::Disabled),
            "clone" => Some(Self::Clone),
            "convert" => Some(Self::Convert),
            "compress" => Some(Self::Compress),
            _ => None,
        }
    }

    pub fn needs_tools(self) -> bool {
        matches!(self, Self::Convert | Self::Compress)
    }

    /// Whether exporters should write bytes under `attachments/`.
    pub fn copies_attachments(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

impl fmt::Display for MediaMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MediaMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!("invalid media-mode '{s}' (expected disabled, clone, convert, or compress)")
        })
    }
}

/// Max long-edge cap for video compress (no upscale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaxResolution {
    P720,
    #[default]
    P1080,
    P4k,
}

impl MaxResolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::P720 => "720p",
            Self::P1080 => "1080p",
            Self::P4k => "4k",
        }
    }

    pub fn max_long_edge(self) -> u32 {
        match self {
            Self::P720 => 1280,
            Self::P1080 => 1920,
            Self::P4k => 3840,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "720p" | "720" => Some(Self::P720),
            "1080p" | "1080" => Some(Self::P1080),
            "4k" | "2160p" | "2160" => Some(Self::P4k),
            _ => None,
        }
    }
}

impl fmt::Display for MaxResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MaxResolution {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!("invalid media-max-resolution '{s}' (expected 720p, 1080p, or 4k)")
        })
    }
}

/// Build compress options from CLI-style fields (min_size like `20M`).
pub fn compress_options_from_cli(
    max_resolution: MaxResolution,
    max_fps: f32,
    min_size: &str,
    skip_efficient: bool,
) -> anyhow::Result<CompressOptions> {
    Ok(CompressOptions {
        max_resolution,
        max_fps,
        min_size_bytes: parse_size(min_size)?,
        skip_efficient,
    })
}

/// Options applied only when [`MediaMode::Compress`].
#[derive(Debug, Clone, PartialEq)]
pub struct CompressOptions {
    pub max_resolution: MaxResolution,
    pub max_fps: f32,
    pub min_size_bytes: u64,
    pub skip_efficient: bool,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            max_resolution: MaxResolution::P1080,
            max_fps: 30.0,
            min_size_bytes: 20 * 1024 * 1024,
            skip_efficient: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_and_resolution() {
        assert_eq!(MediaMode::parse("Convert"), Some(MediaMode::Convert));
        assert_eq!(MaxResolution::parse("4k"), Some(MaxResolution::P4k));
        assert_eq!(MaxResolution::P720.max_long_edge(), 1280);
    }

    #[test]
    fn parse_size_units() {
        assert_eq!(parse_size("20M").unwrap(), 20 * 1024 * 1024);
        assert_eq!(parse_size("512k").unwrap(), 512 * 1024);
        assert_eq!(parse_size("100").unwrap(), 100);
    }
}
