use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::tools::{Probe, probe_video, require_ffmpeg, run_ffmpeg};
use crate::{CompressOptions, MediaMode};

#[derive(Debug, Default)]
pub struct MediaReport {
    pub processed: usize,
    pub skipped: usize,
    /// Total bytes under `attachments/` before convert/compress (non-temp files).
    pub bytes_before: u64,
    /// Total bytes under `attachments/` after convert/compress (non-temp files).
    pub bytes_after: u64,
    pub errors: Vec<String>,
}

/// How often to emit `…n/total` progress lines during convert/compress.
const MEDIA_PROGRESS_EVERY: usize = 100;

/// Convert or compress media under `output_dir/attachments`.
///
/// Returns a path remap (`old_rel` → `new_rel`, forward-slash relative to
/// `output_dir`) for callers that update IR / CSV themselves.
pub fn process_attachments_dir(
    output_dir: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<(MediaReport, HashMap<String, String>)> {
    process_attachments_dir_with_log(output_dir, mode, compress, None)
}

/// Same as [`process_attachments_dir`], with optional progress lines via `log`.
pub fn process_attachments_dir_with_log(
    output_dir: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    mut log: Option<&mut dyn FnMut(&str)>,
) -> Result<(MediaReport, HashMap<String, String>)> {
    if matches!(mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok((MediaReport::default(), HashMap::new()));
    }
    require_ffmpeg()?;

    let attachments = output_dir.join("attachments");
    if !attachments.is_dir() {
        return Ok((MediaReport::default(), HashMap::new()));
    }

    // Leftovers from a previous failed ffmpeg run.
    remove_msgmedia_temps(&attachments)?;

    let mut report = MediaReport::default();
    let mut remap = HashMap::new();
    let files = collect_media_files(&attachments)?;
    let total = files.len();
    if total == 0 {
        return Ok((report, remap));
    }

    report.bytes_before = attachments_dir_bytes(&attachments)?;
    let verb = match mode {
        MediaMode::Compress => "Compressing",
        _ => "Converting",
    };
    emit(&mut log, "");
    emit(
        &mut log,
        &format!(
            "{verb} attachments ({total} file(s), {})…",
            format_bytes(report.bytes_before)
        ),
    );

    let mut done = 0usize;
    for path in files {
        match process_one(output_dir, &path, mode, compress) {
            Ok(Outcome::Changed { old_rel, new_rel }) => {
                report.processed += 1;
                remap.insert(old_rel, new_rel);
            }
            Ok(Outcome::Skipped) => report.skipped += 1,
            Err(err) => report.errors.push(format!("{}: {err}", path.display())),
        }
        done += 1;
        if done.is_multiple_of(MEDIA_PROGRESS_EVERY) || done == total {
            emit(&mut log, &format!("  …{done}/{total}"));
        }
    }

    // Always sweep again so a failed convert cannot leave junk behind.
    remove_msgmedia_temps(&attachments)?;
    report.bytes_after = attachments_dir_bytes(&attachments)?;

    let mut summary = format!(
        "Attachment {mode} done: processed={} skipped={} size {} → {}",
        report.processed,
        report.skipped,
        format_bytes(report.bytes_before),
        format_bytes(report.bytes_after),
    );
    if !report.errors.is_empty() {
        summary.push_str(&format!(" errors={}", report.errors.len()));
    }
    emit(&mut log, &summary);

    Ok((report, remap))
}

fn emit(log: &mut Option<&mut dyn FnMut(&str)>, line: &str) {
    if let Some(log) = log.as_mut() {
        log(line);
    }
}

/// Sum sizes of non-temp files under `attachments/` (folder-level total).
fn attachments_dir_bytes(attachments: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![attachments.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if !is_msgmedia_temp(&path) {
                total = total.saturating_add(
                    entry
                        .metadata()
                        .with_context(|| format!("stat {}", path.display()))?
                        .len(),
                );
            }
        }
    }
    Ok(total)
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    const MB: f64 = KB * 1000.0;
    const GB: f64 = MB * 1000.0;
    let n = bytes as f64;
    if n >= GB {
        format!("{:.1} GB", n / GB)
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.1} KB", n / KB)
    } else {
        format!("{bytes} B")
    }
}

enum Outcome {
    Changed { old_rel: String, new_rel: String },
    Skipped,
}

#[derive(Clone, Copy)]
enum Kind {
    Image,
    Video,
    Audio,
}

fn collect_media_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if !is_msgmedia_temp(&path) && classify(&path).is_some() {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Sidecar written by ffmpeg before [`replace_original`] (must never remain on disk).
fn is_msgmedia_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains(".msgmedia.tmp."))
}

fn remove_msgmedia_temps(root: &Path) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_msgmedia_temp(&path) {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

fn temp_sibling(path: &Path, ext: &str) -> PathBuf {
    path.with_extension(format!("msgmedia.tmp.{ext}"))
}

/// Run work that writes `tmp`. Deletes `tmp` on any error (success must rename it away).
fn with_temp_output<T>(tmp: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match f() {
        Ok(v) => Ok(v),
        Err(err) => {
            let _ = fs::remove_file(tmp);
            Err(err)
        }
    }
}

fn try_remux_replace(path: &Path) -> Result<Option<PathBuf>> {
    let tmp = temp_sibling(path, "mp4");
    if remux_mp4(path, &tmp).is_err() {
        let _ = fs::remove_file(&tmp);
        return Ok(None);
    }
    match replace_original(path, &tmp) {
        Ok(p) => Ok(Some(p)),
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            Err(err)
        }
    }
}

fn classify(path: &Path) -> Option<Kind> {
    if is_msgmedia_temp(path) {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "gif" => {
            Some(Kind::Image)
        }
        "mp4" | "m4v" | "mov" | "3gp" | "3gpp" | "webm" | "mpeg" | "mpg" | "mkv" | "avi" => {
            Some(Kind::Video)
        }
        "mp3" | "m4a" | "aac" | "caf" | "amr" | "wav" | "ogg" | "opus" => Some(Kind::Audio),
        _ => None,
    }
}

fn process_one(
    output_dir: &Path,
    path: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<Outcome> {
    let kind = classify(path).context("unknown media kind")?;
    let old_rel = rel_path(output_dir, path)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match (kind, mode) {
        (Kind::Image, MediaMode::Convert) => {
            // Keep GIF as-is (animation); jpg already in target form.
            if matches!(ext.as_str(), "jpg" | "jpeg" | "gif") {
                return Ok(Outcome::Skipped);
            }
            convert_image(path, false).map(|new_path| changed(output_dir, &old_rel, &new_path))?
        }
        (Kind::Image, MediaMode::Compress) => {
            if ext == "gif" {
                return Ok(Outcome::Skipped);
            }
            let meta = fs::metadata(path)?;
            if matches!(ext.as_str(), "jpg" | "jpeg") && meta.len() <= 500 * 1024 {
                return Ok(Outcome::Skipped);
            }
            convert_image(path, true).map(|new_path| changed(output_dir, &old_rel, &new_path))?
        }
        (Kind::Audio, MediaMode::Convert) => {
            if ext == "mp3" {
                return Ok(Outcome::Skipped);
            }
            convert_audio(path, false).map(|new_path| changed(output_dir, &old_rel, &new_path))?
        }
        (Kind::Audio, MediaMode::Compress) => {
            let meta = fs::metadata(path)?;
            if ext == "mp3" && meta.len() <= 100 * 1024 {
                return Ok(Outcome::Skipped);
            }
            convert_audio(path, true).map(|new_path| changed(output_dir, &old_rel, &new_path))?
        }
        (Kind::Video, MediaMode::Convert) => {
            convert_video(path).map(|new_path| changed(output_dir, &old_rel, &new_path))?
        }
        (Kind::Video, MediaMode::Compress) => {
            compress_video(path, compress).map(|outcome| match outcome {
                Some(new_path) => changed(output_dir, &old_rel, &new_path),
                None => Ok(Outcome::Skipped),
            })?
        }
        (_, MediaMode::Clone | MediaMode::Disabled) => Ok(Outcome::Skipped),
    }
}

fn changed(output_dir: &Path, old_rel: &str, new_path: &Path) -> Result<Outcome> {
    let new_rel = rel_path(output_dir, new_path)?;
    // Always report Changed — even when the relative path is unchanged (e.g. JPG
    // recompressed in place). Callers must invalidate digest_sha256 for remapped
    // paths; treating same-path rewrites as Skipped left stale digests in JSONL
    // and caused vault-push sha256 mismatches after upload.
    Ok(Outcome::Changed {
        old_rel: old_rel.to_string(),
        new_rel,
    })
}

fn rel_path(output_dir: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(output_dir)
        .with_context(|| format!("{} not under {}", path.display(), output_dir.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn sibling_with_ext(path: &Path, ext: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default();
    let mut dest = path.with_file_name(stem);
    dest.set_extension(ext);
    if dest == path {
        return dest;
    }
    if !dest.exists() {
        return dest;
    }
    // collision: stem_converted.ext
    let mut n = 1u32;
    loop {
        let name = format!("{}_{n}.{ext}", stem.to_string_lossy());
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

fn replace_original(original: &Path, produced: &Path) -> Result<PathBuf> {
    if produced == original {
        return Ok(original.to_path_buf());
    }
    let target_ext = produced
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let final_path = if original.extension().and_then(|e| e.to_str()) == Some(target_ext) {
        // overwrite same extension via temp
        let tmp = original.with_extension(format!("{target_ext}.tmp"));
        if tmp.exists() {
            let _ = fs::remove_file(&tmp);
        }
        fs::rename(produced, &tmp)?;
        let _ = fs::remove_file(original);
        fs::rename(&tmp, original)?;
        original.to_path_buf()
    } else {
        let dest = sibling_with_ext(original, target_ext);
        if dest.exists() && dest != produced {
            let _ = fs::remove_file(&dest);
        }
        fs::rename(produced, &dest)?;
        let _ = fs::remove_file(original);
        dest
    };
    Ok(final_path)
}

fn convert_image(path: &Path, compress: bool) -> Result<PathBuf> {
    let tmp = temp_sibling(path, "jpg");
    let quality = if compress { "5" } else { "2" }; // ffmpeg -q:v (2 best … 31 worst for mjpeg)
    // `-frames:v 1 -update 1`: animated GIF/WebP must write a single still, not an
    // image2 sequence (otherwise ffmpeg leaves a partial tmp and exits non-zero).
    let args = vec![
        "-y".into(),
        "-i".into(),
        path_str(path),
        "-frames:v".into(),
        "1".into(),
        "-update".into(),
        "1".into(),
        "-q:v".into(),
        quality.into(),
        path_str(&tmp),
    ];
    with_temp_output(&tmp, || {
        run_ffmpeg(&args).with_context(|| format!("convert image {}", path.display()))?;
        replace_original(path, &tmp)
    })
}

fn convert_audio(path: &Path, compress: bool) -> Result<PathBuf> {
    let tmp = temp_sibling(path, "mp3");
    let mut args = vec![
        "-y".into(),
        "-i".into(),
        path_str(path),
        "-vn".into(),
        "-acodec".into(),
        "libmp3lame".into(),
    ];
    if compress {
        args.extend(["-ac".into(), "1".into(), "-b:a".into(), "96k".into()]);
    } else {
        args.extend(["-q:a".into(), "4".into()]);
    }
    args.push(path_str(&tmp));
    with_temp_output(&tmp, || {
        run_ffmpeg(&args).with_context(|| format!("convert audio {}", path.display()))?;
        replace_original(path, &tmp)
    })
}

fn convert_video(path: &Path) -> Result<PathBuf> {
    let tmp = temp_sibling(path, "mp4");

    with_temp_output(&tmp, || {
        // Prefer remux into mp4 when already a video file.
        if remux_mp4(path, &tmp).is_ok() {
            return replace_original(path, &tmp);
        }
        let _ = fs::remove_file(&tmp);

        // Light standardize encode (H.264, 30fps, no aggressive downscale).
        let args = vec![
            "-y".into(),
            "-i".into(),
            path_str(path),
            "-vf".into(),
            "fps=30".into(),
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            "23".into(),
            "-preset".into(),
            "medium".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "128k".into(),
            "-movflags".into(),
            "+faststart".into(),
            path_str(&tmp),
        ];
        run_ffmpeg(&args).with_context(|| format!("convert video {}", path.display()))?;
        replace_original(path, &tmp)
    })
}

fn remux_mp4(path: &Path, tmp: &Path) -> Result<()> {
    let args = vec![
        "-y".into(),
        "-i".into(),
        path_str(path),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        path_str(tmp),
    ];
    run_ffmpeg(&args)
}

fn compress_video(path: &Path, opts: &CompressOptions) -> Result<Option<PathBuf>> {
    let meta = fs::metadata(path)?;
    if meta.len() < opts.min_size_bytes {
        // Still remux non-mp4 small files for container consistency.
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "mp4" {
            return Ok(None);
        }
        return try_remux_replace(path);
    }

    let probe = probe_video(path).unwrap_or_default();
    if opts.skip_efficient && is_efficient(&probe, opts) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "mp4" {
            return Ok(None);
        }
        return try_remux_replace(path);
    }

    let max_edge = opts.max_resolution.max_long_edge();
    let fps = if opts.max_fps > 0.0 {
        opts.max_fps
    } else {
        30.0
    };
    let vf = format!(
        "scale='if(gt(iw,ih),min({max_edge},iw),-2)':'if(gt(iw,ih),-2,min({max_edge},ih))',fps={fps}"
    );
    let tmp = temp_sibling(path, "mp4");

    with_temp_output(&tmp, || {
        // Prefer libx265; fall back to libx264.
        let mut hevc_args = base_video_args(path, &tmp, &vf);
        hevc_args.extend([
            "-c:v".into(),
            "libx265".into(),
            "-crf".into(),
            "22".into(),
            "-preset".into(),
            "medium".into(),
            "-tag:v".into(),
            "hvc1".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "128k".into(),
            "-movflags".into(),
            "+faststart".into(),
            path_str(&tmp),
        ]);
        if run_ffmpeg(&hevc_args).is_err() {
            let _ = fs::remove_file(&tmp);
            let mut avc_args = base_video_args(path, &tmp, &vf);
            avc_args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-crf".into(),
                "28".into(),
                "-preset".into(),
                "medium".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "96k".into(),
                "-movflags".into(),
                "+faststart".into(),
                path_str(&tmp),
            ]);
            run_ffmpeg(&avc_args).with_context(|| format!("compress video {}", path.display()))?;
        }
        Ok(Some(replace_original(path, &tmp)?))
    })
}

fn base_video_args(path: &Path, _tmp: &Path, vf: &str) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        path_str(path),
        "-vf".into(),
        vf.into(),
    ]
}

fn is_efficient(probe: &Probe, opts: &CompressOptions) -> bool {
    let hevc = matches!(probe.codec.as_str(), "hevc" | "h265");
    if !hevc {
        return false;
    }
    let long = probe.width.max(probe.height);
    if long > opts.max_resolution.max_long_edge() {
        return false;
    }
    // ~12 Mbps threshold (archive-tools style)
    if probe.bitrate > 12_000_000 {
        return false;
    }
    true
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_kinds() {
        assert!(matches!(classify(Path::new("a.HEIC")), Some(Kind::Image)));
        assert!(matches!(classify(Path::new("v.mov")), Some(Kind::Video)));
        assert!(matches!(classify(Path::new("x.caf")), Some(Kind::Audio)));
        assert!(classify(Path::new("doc.pdf")).is_none());
        assert!(classify(Path::new("a.msgmedia.tmp.jpg")).is_none());
    }

    #[test]
    fn detects_msgmedia_temp_names() {
        assert!(is_msgmedia_temp(Path::new(
            "20150917_095137-I_1.msgmedia.tmp.jpg"
        )));
        assert!(!is_msgmedia_temp(Path::new("20150917_095137-I_1.jpg")));
    }

    #[test]
    fn sweeps_leftover_msgmedia_temps() {
        let dir = tempfile::tempdir().unwrap();
        let att = dir.path().join("attachments");
        fs::create_dir_all(&att).unwrap();
        let junk = att.join("photo.msgmedia.tmp.jpg");
        fs::write(&junk, b"partial").unwrap();
        fs::write(att.join("keep.jpg"), b"ok").unwrap();

        remove_msgmedia_temps(&att).unwrap();
        assert!(!junk.exists());
        assert!(att.join("keep.jpg").exists());
    }

    #[test]
    fn clone_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let (report, remap) =
            process_attachments_dir(dir.path(), MediaMode::Clone, &CompressOptions::default())
                .unwrap();
        assert_eq!(report.processed, 0);
        assert!(remap.is_empty());
    }

    #[test]
    fn same_path_rewrite_reports_changed() {
        let dir = tempfile::tempdir().unwrap();
        let att = dir.path().join("attachments");
        fs::create_dir_all(&att).unwrap();
        let file = att.join("photo.jpg");
        fs::write(&file, b"jpeg-bytes").unwrap();
        let outcome = changed(dir.path(), "attachments/photo.jpg", &file).unwrap();
        match outcome {
            Outcome::Changed { old_rel, new_rel } => {
                assert_eq!(old_rel, "attachments/photo.jpg");
                assert_eq!(new_rel, "attachments/photo.jpg");
            }
            Outcome::Skipped => panic!("in-place rewrite must not look like Skipped"),
        }
    }

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(12_500), "12.5 KB");
        assert_eq!(format_bytes(1_500_000), "1.5 MB");
        assert_eq!(format_bytes(2_500_000_000), "2.5 GB");
    }

    #[test]
    fn attachments_dir_bytes_sums_non_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let att = dir.path().join("attachments");
        fs::create_dir_all(&att).unwrap();
        fs::write(att.join("a.jpg"), vec![0u8; 1000]).unwrap();
        fs::write(att.join("b.mp4"), vec![0u8; 2500]).unwrap();
        fs::write(att.join("orphan.msgmedia.tmp.jpg"), vec![0u8; 9999]).unwrap();
        assert_eq!(attachments_dir_bytes(&att).unwrap(), 3500);
    }

    #[test]
    fn clone_with_log_emits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut lines = Vec::new();
        let mut log = |line: &str| lines.push(line.to_string());
        let _ = process_attachments_dir_with_log(
            dir.path(),
            MediaMode::Clone,
            &CompressOptions::default(),
            Some(&mut log),
        )
        .unwrap();
        assert!(lines.is_empty());
    }
}
