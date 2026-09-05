//! Shared ffmpeg/ffprobe helpers for import convert and derived-asset processing.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// JPEGs at or below this size are left as-is (converting would not save space).
pub const JPEG_MIN_BYTES: u64 = 500 * 1024;
/// MP3s at or below this size are left as-is (converting would not save space).
pub const MP3_MIN_BYTES: u64 = 100 * 1024;
/// MP4s at or below this size are left as-is (converting would not save space).
pub const MP4_MIN_BYTES: u64 = 10 * 1024 * 1024;

/// Media category of a file, used to pick the conversion target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// Image, converted to JPEG.
    Image,
    /// Video, converted to MP4.
    Video,
    /// Audio, converted to MP3.
    Audio,
    /// Anything else (e.g. GIFs), left as-is.
    Other,
}

/// Lowercase file extension including the dot (`.jpg`), or empty when the
/// path has no extension.
pub fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()))
        .unwrap_or_default()
}

/// Media category for a file: extension first, then MIME type. `.gif` is
/// always [`MediaKind::Other`] so animated images are never converted.
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

/// True when a JPEG source should be kept as-is instead of re-encoded.
///
/// Without `compress`, every JPEG is kept; with it, only JPEGs at or below
/// [`JPEG_MIN_BYTES`] are kept. `ext` is the [`ext_of`] form (`.jpg`).
pub fn skip_image_conversion(ext: &str, size: u64, compress: bool) -> bool {
    let is_jpeg = ext == ".jpg" || ext == ".jpeg";
    is_jpeg && (!compress || size <= JPEG_MIN_BYTES)
}

/// True when an MP4 source should be kept as-is instead of re-encoded.
///
/// Without `compress`, every MP4 is kept; with it, MP4s at or below
/// [`MP4_MIN_BYTES`] are kept, as are larger ones that
/// [`probe_video_efficient`] says are already efficient (the probe only runs
/// when the size check alone does not decide).
pub fn skip_video_conversion(source_path: &Path, ext: &str, size: u64, compress: bool) -> bool {
    ext == ".mp4" && (!compress || size <= MP4_MIN_BYTES || probe_video_efficient(source_path))
}

/// True when an MP3 source should be kept as-is instead of re-encoded.
///
/// Without `compress`, every MP3 is kept; with it, only MP3s at or below
/// [`MP3_MIN_BYTES`] are kept.
pub fn skip_audio_conversion(ext: &str, size: u64, compress: bool) -> bool {
    ext == ".mp3" && (!compress || size <= MP3_MIN_BYTES)
}

/// ffmpeg args for a high-quality single-frame JPEG (`-q:v 2` ≈ quality ~85
/// intent); autorotate is ffmpeg's default.
pub fn image_to_jpeg_args<'a>(source: &'a str, dest: &'a str) -> Vec<&'a str> {
    vec![
        "-i",
        source,
        "-frames:v",
        "1",
        "-update",
        "1",
        "-q:v",
        "2",
        dest,
    ]
}

/// ffmpeg args for a browser-friendly MP4: ≤720p short side, 30 fps, h264 +
/// AAC, faststart. `compress` picks CRF 28 (smaller) over CRF 23.
pub fn video_to_mp4_args<'a>(source: &'a str, dest: &'a str, compress: bool) -> Vec<&'a str> {
    vec![
        "-i",
        source,
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
        dest,
    ]
}

/// ffmpeg args for a mono MP3 (`-q:a 6` VBR).
pub fn audio_to_mp3_args<'a>(source: &'a str, dest: &'a str) -> Vec<&'a str> {
    vec![
        "-i",
        source,
        "-vn",
        "-ac",
        "1",
        "-c:a",
        "libmp3lame",
        "-q:a",
        "6",
        dest,
    ]
}

/// The path as `&str`, or an error when it is not valid UTF-8.
pub fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("non-utf8 path {}", path.display()))
}

/// True when an executable named `name` is on `PATH` and runs with `-version`.
pub fn tool_on_path(name: &str) -> bool {
    Command::new(name)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Run ffmpeg with `args` (`-y` and quiet logging are added). On failure,
/// delete `cleanup_on_fail` if given and return ffmpeg's stderr as the error.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_skip_keeps_all_jpegs_without_compress_and_small_ones_with_it() {
        assert!(skip_image_conversion(".jpg", JPEG_MIN_BYTES + 1, false));
        assert!(skip_image_conversion(".jpeg", JPEG_MIN_BYTES + 1, false));
        assert!(skip_image_conversion(".jpg", JPEG_MIN_BYTES, true));
        assert!(!skip_image_conversion(".jpg", JPEG_MIN_BYTES + 1, true));
        assert!(!skip_image_conversion(".png", 1, false));
    }

    #[test]
    fn audio_skip_keeps_all_mp3s_without_compress_and_small_ones_with_it() {
        assert!(skip_audio_conversion(".mp3", MP3_MIN_BYTES + 1, false));
        assert!(skip_audio_conversion(".mp3", MP3_MIN_BYTES, true));
        assert!(!skip_audio_conversion(".mp3", MP3_MIN_BYTES + 1, true));
        assert!(!skip_audio_conversion(".m4a", 1, false));
    }

    #[test]
    fn video_skip_decides_on_extension_and_size_before_probing() {
        // These cases decide without running ffprobe (the path does not exist).
        let missing = Path::new("no-such-file.mp4");
        assert!(skip_video_conversion(
            missing,
            ".mp4",
            MP4_MIN_BYTES + 1,
            false
        ));
        assert!(skip_video_conversion(missing, ".mp4", MP4_MIN_BYTES, true));
        assert!(!skip_video_conversion(missing, ".mov", 1, false));
        // Large mp4 under compress: the probe runs, fails on a missing file,
        // and the video is converted.
        assert!(!skip_video_conversion(
            missing,
            ".mp4",
            MP4_MIN_BYTES + 1,
            true
        ));
    }

    #[test]
    fn image_args_build_single_frame_jpeg() {
        assert_eq!(
            image_to_jpeg_args("in.png", "out.jpg"),
            vec![
                "-i",
                "in.png",
                "-frames:v",
                "1",
                "-update",
                "1",
                "-q:v",
                "2",
                "out.jpg"
            ],
        );
    }

    #[test]
    fn video_args_pick_crf_by_compress_flag() {
        let base = |crf: &'static str| {
            vec![
                "-i",
                "in.mov",
                "-vf",
                "scale='if(gt(iw,ih),-2,min(720,iw))':'if(gt(iw,ih),min(720,ih),-2)',fps=30",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                crf,
                "-c:a",
                "aac",
                "-b:a",
                "96k",
                "-movflags",
                "+faststart",
                "out.mp4",
            ]
        };
        assert_eq!(video_to_mp4_args("in.mov", "out.mp4", true), base("28"));
        assert_eq!(video_to_mp4_args("in.mov", "out.mp4", false), base("23"));
    }

    #[test]
    fn audio_args_build_mono_mp3() {
        assert_eq!(
            audio_to_mp3_args("in.m4a", "out.mp3"),
            vec![
                "-i",
                "in.m4a",
                "-vn",
                "-ac",
                "1",
                "-c:a",
                "libmp3lame",
                "-q:a",
                "6",
                "out.mp3"
            ],
        );
    }
}
