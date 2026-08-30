use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::tools::{probe_video, require_ffmpeg, run_ffmpeg};
use crate::{CompressOptions, MediaMode};

/// Aggregate counts and errors from one media convert/compress pass.
#[derive(Debug, Default)]
pub struct MediaReport {
    /// Number of files converted or compressed.
    pub processed: usize,
    /// Number of files left unchanged.
    pub skipped: usize,
    /// Total bytes under `attachments/` before convert/compress (non-temp files).
    pub bytes_before: u64,
    /// Total bytes under `attachments/` after convert/compress (non-temp files).
    pub bytes_after: u64,
    /// Per-file error messages (`path: error`) from the pass.
    pub errors: Vec<String>,
}

/// How often to write `…n/total` progress lines during convert/compress.
const MEDIA_PROGRESS_EVERY: usize = 100;

/// JPEGs at or under this size are left alone in compress mode: re-encoding
/// them buys nothing.
const JPEG_COMPRESS_FLOOR: u64 = 500 * 1024;
/// MP3s at or under this size are left alone in compress mode.
const MP3_COMPRESS_FLOOR: u64 = 100 * 1024;

/// Convert or compress the given attachment files in place.
///
/// The caller builds `files` (usually via [`collect_media_files`]), so a
/// resumed or scoped pass can name exactly the files it means instead of
/// sweeping the whole directory. Paths must live under `output_dir`'s
/// `attachments/` directory.
///
/// Returns a path remap (`old_rel` → `new_rel`, forward-slash relative to
/// `output_dir`) for callers that update IR / CSV themselves.
///
/// # Errors
///
/// Returns an error when ffmpeg/ffprobe are missing or fail, an input path
/// escapes the output directory, or IO fails.
pub fn process_attachment_files(
    output_dir: &Path,
    files: &[PathBuf],
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
        match process_one(output_dir, path, mode, compress) {
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
pub(crate) enum Kind {
    Image,
    Video,
    Audio,
}

/// List the files a media pass would touch under `root`.
///
/// Every non-temp file [`classify`] recognizes, recursively, sorted so two
/// runs enumerate in the same order. Callers hand the result — or a subset of
/// it — to [`process_attachment_files`].
///
/// # Errors
///
/// Returns an error when a directory under `root` cannot be read.
pub fn collect_media_files(root: &Path) -> Result<Vec<PathBuf>> {
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

/// Delete ffmpeg scratch left beside `path` by an earlier interrupted run.
///
/// Matched on the exact name a transcode of `path` could have written (see
/// `temp_sibling`), not a stem prefix, and scoped to `path`'s own kind: a
/// given kind only ever writes one scratch extension (`jpg` for images,
/// `mp3` for audio, `mp4` for video). Precision here matters because two
/// source files can share a stem — an iOS Live Photo's `IMG_0001.HEIC` and
/// `IMG_0001.MOV`, for instance — and a coarser, stem-only match would delete
/// one file's in-flight scratch while converting the other.
///
/// `path` itself can never be swept: `classify` (and so this function)
/// treats a `.msgmedia.tmp.` path as having no kind, so a caller that passes
/// scratch as `path` gets a no-op, not a self-delete.
fn remove_temps_beside(path: &Path) {
    let Some(kind) = classify(path) else {
        return;
    };
    let ext = match kind {
        Kind::Image => "jpg",
        Kind::Audio => "mp3",
        Kind::Video => "mp4",
    };
    let _ = fs::remove_file(temp_sibling(path, ext));
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

fn try_remux_replace(path: &Path, commit: Commit<'_>) -> Result<Option<PathBuf>> {
    let tmp = temp_sibling(path, "mp4");
    if remux_mp4(path, &tmp).is_err() {
        let _ = fs::remove_file(&tmp);
        return Ok(None);
    }
    match commit_produced(commit, path, &tmp) {
        Ok(p) => Ok(Some(p)),
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            Err(err)
        }
    }
}

pub(crate) fn classify(path: &Path) -> Option<Kind> {
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

/// Run the media step over one file, committing however `commit` says.
///
/// Returns the produced path, or `None` when the media step leaves this file
/// alone — either because the mode does not touch it, or because a same-format
/// re-encode came out no smaller (decision 44).
fn run_one(
    path: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    commit: Commit<'_>,
) -> Result<Option<PathBuf>> {
    let kind = classify(path).context("unknown media kind")?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match (kind, mode) {
        (Kind::Image, MediaMode::Convert) => {
            // Keep GIF as-is (animation); jpg already in target form.
            if matches!(ext.as_str(), "jpg" | "jpeg" | "gif") {
                return Ok(None);
            }
            convert_image(path, false, false, commit)
        }
        (Kind::Image, MediaMode::Compress) => {
            if ext == "gif" {
                return Ok(None);
            }
            let same_format = matches!(ext.as_str(), "jpg" | "jpeg");
            if same_format && fs::metadata(path)?.len() <= JPEG_COMPRESS_FLOOR {
                return Ok(None);
            }
            convert_image(path, true, same_format, commit)
        }
        (Kind::Audio, MediaMode::Convert) => {
            if ext == "mp3" {
                return Ok(None);
            }
            convert_audio(path, false, false, commit)
        }
        (Kind::Audio, MediaMode::Compress) => {
            let same_format = ext == "mp3";
            if same_format && fs::metadata(path)?.len() <= MP3_COMPRESS_FLOOR {
                return Ok(None);
            }
            convert_audio(path, true, same_format, commit)
        }
        (Kind::Video, MediaMode::Convert) => convert_video(path, commit).map(Some),
        (Kind::Video, MediaMode::Compress) => compress_video(path, compress, commit),
        (_, MediaMode::Clone | MediaMode::Disabled) => Ok(None),
    }
}

fn process_one(
    output_dir: &Path,
    path: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<Outcome> {
    let old_rel = rel_path(output_dir, path)?;
    match run_one(path, mode, compress, Commit::InPlace)? {
        Some(new_path) => changed(output_dir, &old_rel, &new_path),
        None => Ok(Outcome::Skipped),
    }
}

/// What [`transcode_file`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscodeOutcome {
    /// Nothing was written: the mode does not touch this file, or a
    /// same-format re-encode came out no smaller.
    Skipped,
    /// A derivative was written to the destination the caller named.
    Produced,
}

/// File name the media step would produce for `src`, or `None` when it leaves
/// the file alone.
///
/// Reads the same decision tree as the pass itself, so the name a caller
/// patches into a conversation file is the name the pass writes when it does
/// write one. For video this is a forecast, not a promise: `derivative_name`
/// cannot see `CompressOptions`, so it always answers `mp4` for a video in
/// either mode, even though `compress_video` may skip a small or
/// already-efficient file and `try_remux_replace` may fall through on a
/// remux failure. Callers must treat [`TranscodeOutcome::Skipped`] from
/// [`transcode_file`] as authoritative over whatever this function predicted.
///
/// Stats `src` for the two size floors (compress-mode same-format JPEG/MP3).
/// When `src` may not exist on disk, use [`derivative_name_for_missing`]
/// instead — a stat failure here silently reads as size 0, which is under
/// both floors and answers `None`, the wrong answer for "is there a
/// candidate name to look for", not "is this live file worth touching".
#[must_use]
pub fn derivative_name(src: &Path, mode: MediaMode) -> Option<String> {
    derivative_name_impl(src, mode, |floor| {
        fs::metadata(src).map(|m| m.len()).unwrap_or(0) <= floor
    })
}

/// Same decision tree as [`derivative_name`], but never stats `src` — for a
/// recorded path already known to be missing from disk.
///
/// The two size floors exist to skip a small file that is still there to
/// measure; a missing file's size is unknowable and irrelevant to the
/// question this variant answers ("what name would a committed derivative of
/// this file carry, if one exists"), so both floors are treated as never
/// crossed and the candidate name is always produced. The caller is
/// expected to check the filesystem for that name itself.
#[must_use]
pub fn derivative_name_for_missing(src: &Path, mode: MediaMode) -> Option<String> {
    derivative_name_impl(src, mode, |_floor| false)
}

fn derivative_name_impl(
    src: &Path,
    mode: MediaMode,
    under_floor: impl Fn(u64) -> bool,
) -> Option<String> {
    let kind = classify(src)?;
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = src.file_stem().and_then(|s| s.to_str())?;
    let target = match (kind, mode) {
        (_, MediaMode::Clone | MediaMode::Disabled) => return None,
        (Kind::Image, MediaMode::Convert) => {
            if matches!(ext.as_str(), "jpg" | "jpeg" | "gif") {
                return None;
            }
            "jpg"
        }
        (Kind::Image, MediaMode::Compress) => {
            if ext == "gif" {
                return None;
            }
            if matches!(ext.as_str(), "jpg" | "jpeg") && under_floor(JPEG_COMPRESS_FLOOR) {
                return None;
            }
            "jpg"
        }
        (Kind::Audio, MediaMode::Convert) => {
            if ext == "mp3" {
                return None;
            }
            "mp3"
        }
        (Kind::Audio, MediaMode::Compress) => {
            if ext == "mp3" && under_floor(MP3_COMPRESS_FLOOR) {
                return None;
            }
            "mp3"
        }
        // Forecast only: whether a video is actually rewritten depends on
        // CompressOptions and probed efficiency, neither visible here. See
        // the function doc.
        (Kind::Video, _) => "mp4",
    };
    Some(format!("{stem}.{target}"))
}

/// Transcode `src` and write the derivative to exactly `dest`.
///
/// `src` is never modified or deleted: committing is the caller's, because it
/// has to patch whatever points at the original first (decision 28). Scratch
/// left beside `src` by an interrupted run is cleared; scratch belonging to
/// other files, and any `.in_progress` marker, is left alone.
///
/// # Errors
///
/// Returns an error when ffmpeg/ffprobe are missing or fail, or IO fails.
pub fn transcode_file(
    src: &Path,
    dest: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<TranscodeOutcome> {
    // Clear this file's own scratch before checking the mode: an interrupted
    // run can leave scratch beside a file regardless of what mode retries it.
    remove_temps_beside(src);
    if matches!(mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok(TranscodeOutcome::Skipped);
    }
    require_ffmpeg()?;
    match run_one(src, mode, compress, Commit::To(dest))? {
        Some(_) => Ok(TranscodeOutcome::Produced),
        None => Ok(TranscodeOutcome::Skipped),
    }
}

fn changed(output_dir: &Path, old_rel: &str, new_path: &Path) -> Result<Outcome> {
    let new_rel = rel_path(output_dir, new_path)?;
    // Always report Changed — even when the relative path is unchanged (e.g. JPG
    // recompressed in place). Callers must invalidate digest_sha256 for remapped
    // paths; treating same-path rewrites as Skipped left stale fingerprints in
    // JSON Lines files and caused vault-push sha256 mismatches after upload.
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

/// Where a freshly produced derivative goes.
#[derive(Debug, Clone, Copy)]
enum Commit<'a> {
    /// Replace the original in place, deleting it. The directory pass's
    /// behaviour, unchanged.
    InPlace,
    /// Move the derivative to exactly this path and leave the original alone.
    ///
    /// The caller commits: it patches whatever points at the original, renames
    /// this file into its final name, and only then deletes the original
    /// (decision 28).
    To(&'a Path),
}

fn commit_produced(commit: Commit<'_>, original: &Path, produced: &Path) -> Result<PathBuf> {
    match commit {
        Commit::InPlace => replace_original(original, produced),
        Commit::To(dest) => {
            if dest == original {
                // The whole point of Commit::To is that the final name never
                // exists until the caller has patched whatever points at the
                // original and renamed this derivative into place itself
                // (decision 28). A destination equal to the original would
                // overwrite it here, before any of that has happened — for
                // example `derivative_name` returning the source's own name
                // (a same-format compress) joined onto the source's directory
                // without a caller-added suffix like `.in_progress`.
                bail!(
                    "transcode destination {} is the original file: write the \
                     derivative to a distinct temporary name (e.g. suffixed with \
                     `.in_progress`) and rename it into place only after patching \
                     whatever points at {}",
                    dest.display(),
                    original.display()
                );
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::rename(produced, dest)
                .with_context(|| format!("rename {} to {}", produced.display(), dest.display()))?;
            Ok(dest.to_path_buf())
        }
    }
}

/// Is `produced` actually smaller than `original`?
///
/// Only meaningful for a same-format re-encode. Where the format changes the
/// user asked for the target format, and a smaller file in the source format
/// is not a substitute for it.
fn is_smaller(produced: &Path, original: &Path) -> Result<bool> {
    Ok(fs::metadata(produced)?.len() < fs::metadata(original)?.len())
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

fn convert_image(
    path: &Path,
    compress: bool,
    keep_smaller: bool,
    commit: Commit<'_>,
) -> Result<Option<PathBuf>> {
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
        if keep_smaller && !is_smaller(&tmp, path)? {
            let _ = fs::remove_file(&tmp);
            return Ok(None);
        }
        commit_produced(commit, path, &tmp).map(Some)
    })
}

fn convert_audio(
    path: &Path,
    compress: bool,
    keep_smaller: bool,
    commit: Commit<'_>,
) -> Result<Option<PathBuf>> {
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
        if keep_smaller && !is_smaller(&tmp, path)? {
            let _ = fs::remove_file(&tmp);
            return Ok(None);
        }
        commit_produced(commit, path, &tmp).map(Some)
    })
}

fn convert_video(path: &Path, commit: Commit<'_>) -> Result<PathBuf> {
    let tmp = temp_sibling(path, "mp4");

    with_temp_output(&tmp, || {
        // Prefer remux into mp4 when already a video file.
        if remux_mp4(path, &tmp).is_ok() {
            return commit_produced(commit, path, &tmp);
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
        commit_produced(commit, path, &tmp)
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

fn compress_video(
    path: &Path,
    opts: &CompressOptions,
    commit: Commit<'_>,
) -> Result<Option<PathBuf>> {
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
        return try_remux_replace(path, commit);
    }

    let probe = probe_video(path).unwrap_or_default();
    if opts.skip_efficient
        && is_efficient(&probe.codec, probe.width, probe.height, probe.bitrate, opts)
    {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "mp4" {
            return Ok(None);
        }
        return try_remux_replace(path, commit);
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
        Ok(Some(commit_produced(commit, path, &tmp)?))
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

/// Would `compress_video` skip re-encoding this stream and only remux it?
///
/// Takes plain fields rather than [`crate::tools::Probe`] so the size
/// forecast in `estimate.rs` (which has its own [`crate::MediaProbe`] from a
/// public ffprobe call, not this module's private `Probe`) can call the exact
/// predicate `compress_video` uses instead of copying its thresholds — one
/// place decides what counts as "already efficient enough."
pub(crate) fn is_efficient(
    codec: &str,
    width: u32,
    height: u32,
    bitrate: u64,
    opts: &CompressOptions,
) -> bool {
    let hevc = matches!(codec, "hevc" | "h265");
    if !hevc {
        return false;
    }
    let long = width.max(height);
    if long > opts.max_resolution.max_long_edge() {
        return false;
    }
    // ~12 Mbps threshold (archive-tools style)
    if bitrate > 12_000_000 {
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
    use crate::tools::ffmpeg_available;

    /// Write a minimal valid 1x1 PNG, readable by ffmpeg, for conversion tests.
    ///
    /// Plain RGB (PNG color type 2), not RGBA: this build's ffmpeg PNG decoder
    /// chokes on a 1x1 RGBA image ("chunk too big" / decode error) but reads
    /// this one cleanly.
    fn write_test_png(path: &Path) {
        #[rustfmt::skip]
        const PNG_1X1_RGB: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
            0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
            0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
            0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        fs::write(path, PNG_1X1_RGB).unwrap();
    }

    /// Write a coarsely-quantized JPEG through ffmpeg at `-q:v 20` that grows
    /// when re-encoded at compress mode's finer `-q:v 5`.
    ///
    /// Calibrated empirically against this repo's ffmpeg build: random noise
    /// written to independent Y/Cb/Cr planes (`nullsrc`'s default `yuv420p`,
    /// fed by `geq`) runs about 0.44 bytes/pixel at `-q:v 20`, and
    /// re-encoding it at `-q:v 5` (much less quantization) comes out roughly
    /// 50% *larger* — noise has no redundancy for the finer quantization
    /// step to exploit, so asking for more detail just spends more bits
    /// recording the same randomness. That is the opposite of the usual
    /// "worse quality = smaller file" case a typical photo re-encode hits,
    /// which is exactly why it exercises the keep-smaller guard. (An earlier
    /// version of this helper tried the reverse — a `-q:v 2` source
    /// re-encoded at `-q:v 5` — expecting noise's incompressibility to make
    /// it a wash; it consistently shrank by ~25% instead, at every
    /// resolution tried. Coarser quantization shrinks even incompressible
    /// content, so don't retry that direction.)
    fn write_jpeg_that_grows_on_finer_reencode(path: &Path, target_size: u64) {
        let pixels = (target_size as f64 / 0.44).max(4.0);
        let mut width = ((pixels * 4.0 / 3.0).sqrt() as u32).max(2);
        width -= width % 2;
        let mut height = width * 3 / 4;
        height -= height % 2;
        let args = vec![
            "-y".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("nullsrc=size={width}x{height},geq=random(1)*255:random(1)*255:random(1)*255"),
            "-frames:v".into(),
            "1".into(),
            "-update".into(),
            "1".into(),
            "-q:v".into(),
            "20".into(),
            path_str(path),
        ];
        run_ffmpeg(&args).expect("generate incompressible jpeg fixture");
    }

    #[test]
    fn compress_keeps_the_original_jpeg_when_the_re_encode_is_not_smaller() {
        if !ffmpeg_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let attachments = dir.path().join("attachments");
        fs::create_dir_all(&attachments).unwrap();

        // A JPEG that is already tight for its pixel count: re-encoding at -q:v 5
        // produces a file no smaller than the source. Over 500 KB so the size gate
        // in process_one does not skip it outright.
        let jpeg = attachments.join("already-tight.jpg");
        write_jpeg_that_grows_on_finer_reencode(&jpeg, 900 * 1024);
        let before = fs::read(&jpeg).unwrap();
        assert!(
            fs::metadata(&jpeg).unwrap().len() > JPEG_COMPRESS_FLOOR,
            "fixture must clear the floor gate: otherwise run_one skips at the \
             floor and every assertion below holds whether or not the \
             keep-smaller guard exists"
        );

        let files = collect_media_files(&attachments).unwrap();
        let (report, remap) = process_attachment_files(
            dir.path(),
            &files,
            MediaMode::Compress,
            &CompressOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(fs::read(&jpeg).unwrap(), before, "original bytes replaced");
        assert!(
            !remap.contains_key("attachments/already-tight.jpg"),
            "a kept file must not be remapped: a remap entry tells the caller to \
             recompute a digest that did not change"
        );
        assert_eq!(report.processed, 0);
        assert_eq!(report.skipped, 1);
    }

    #[test]
    fn transcode_file_writes_the_derivative_and_leaves_the_original_alone() {
        if !ffmpeg_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.png");
        write_test_png(&src);
        let before = fs::read(&src).unwrap();

        let name = derivative_name(&src, MediaMode::Convert).expect("png is converted");
        assert_eq!(name, "photo.jpg");
        let dest = dir.path().join(format!("{name}.in_progress"));

        let outcome =
            transcode_file(&src, &dest, MediaMode::Convert, &CompressOptions::default()).unwrap();

        assert_eq!(outcome, TranscodeOutcome::Produced);
        assert!(dest.exists(), "derivative written where the caller asked");
        assert!(
            !dir.path().join("photo.jpg").exists(),
            "the final name must not exist until the caller renames it: a file \
             under its final name means fully patched"
        );
        assert_eq!(
            fs::read(&src).unwrap(),
            before,
            "the original is the caller's to delete, after it commits"
        );
    }

    #[test]
    fn derivative_name_is_none_for_a_file_the_mode_leaves_alone() {
        let dir = tempfile::tempdir().unwrap();
        let gif = dir.path().join("loop.gif");
        fs::write(&gif, b"not really a gif").unwrap();
        assert_eq!(derivative_name(&gif, MediaMode::Convert), None);

        let jpeg = dir.path().join("photo.jpg");
        fs::write(&jpeg, b"not really a jpeg").unwrap();
        assert_eq!(derivative_name(&jpeg, MediaMode::Convert), None);

        let doc = dir.path().join("notes.pdf");
        fs::write(&doc, b"%PDF").unwrap();
        assert_eq!(derivative_name(&doc, MediaMode::Convert), None);
    }

    #[test]
    fn derivative_name_matches_what_the_media_step_actually_produces() {
        if !ffmpeg_available() {
            return;
        }
        // The forecast and the patch both trust derivative_name. If it disagrees
        // with the pass, a conversation file points at a name nothing wrote.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.png");
        write_test_png(&src);
        let name = derivative_name(&src, MediaMode::Convert).unwrap();
        let dest = dir.path().join("out").join(&name);
        let outcome =
            transcode_file(&src, &dest, MediaMode::Convert, &CompressOptions::default()).unwrap();
        // dest is built from name, so the file-name equality below would hold
        // even if transcode_file wrote nothing. Pin down that it actually ran.
        assert_eq!(outcome, TranscodeOutcome::Produced);
        assert!(
            dest.exists(),
            "derivative_name promised a name nothing wrote"
        );
        assert_eq!(
            dest.file_name().and_then(|n| n.to_str()),
            Some(name.as_str())
        );
    }

    #[test]
    fn transcode_file_clears_scratch_beside_the_source_only() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.png");
        write_test_png(&src);
        let own_scratch = dir.path().join("photo.msgmedia.tmp.jpg");
        fs::write(&own_scratch, b"leftover").unwrap();
        let other_scratch = dir.path().join("other.msgmedia.tmp.jpg");
        fs::write(&other_scratch, b"in flight").unwrap();
        // Same stem as `src`, but the scratch extension a video producer
        // would write (e.g. an iOS Live Photo's IMG_0001.MOV, mid-encode,
        // sharing photo's stem). A stem-only match would wrongly sweep this;
        // photo.png is Kind::Image, so only its own "jpg" scratch is a
        // candidate.
        let same_stem_video_scratch = dir.path().join("photo.msgmedia.tmp.mp4");
        fs::write(&same_stem_video_scratch, b"another kind, in flight").unwrap();
        let marker = dir.path().join("photo.jpg.in_progress");
        fs::write(&marker, b"a previous attempt").unwrap();

        // Clone mode returns before any ffmpeg work, which is enough to show what
        // the entry point sweeps.
        let _ = transcode_file(
            &src,
            &dir.path().join("photo.jpg.in_progress"),
            MediaMode::Clone,
            &CompressOptions::default(),
        );

        assert!(!own_scratch.exists(), "this file's own leftovers go");
        assert!(
            other_scratch.exists(),
            "another file's in-flight scratch must survive: a folder-wide sweep \
             destroys work that is still running"
        );
        assert!(
            same_stem_video_scratch.exists(),
            "a same-stem sibling's scratch of a different kind must survive: a \
             stem-only match would delete an in-flight Live-Photo pair's video \
             scratch while converting the image half"
        );
        assert!(
            marker.exists(),
            "the .in_progress marker is the resume signal and must survive the \
             scratch sweep (decision 30)"
        );
    }

    #[test]
    fn commit_produced_refuses_a_destination_equal_to_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("photo.jpg");
        fs::write(&original, b"jpeg-bytes").unwrap();
        let produced = dir.path().join("photo.msgmedia.tmp.jpg");
        fs::write(&produced, b"re-encoded-bytes").unwrap();

        // A caller that joined `derivative_name`'s output onto the source
        // directory without adding a distinct temp suffix (e.g. forgot
        // `.in_progress`) would ask to overwrite the original before any
        // commit has happened. That must be refused, not silently done.
        let err = commit_produced(Commit::To(&original), &original, &produced).unwrap_err();
        assert!(
            err.to_string().contains("original file"),
            "error should explain why: {err}"
        );
        assert!(original.exists(), "original must be untouched");
        assert_eq!(fs::read(&original).unwrap(), b"jpeg-bytes");
        assert!(
            produced.exists(),
            "the would-be derivative is left for the caller to clean up"
        );
    }

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
        let (report, remap) = process_attachment_files(
            dir.path(),
            &[],
            MediaMode::Clone,
            &CompressOptions::default(),
            None,
        )
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
        let _ = process_attachment_files(
            dir.path(),
            &[],
            MediaMode::Clone,
            &CompressOptions::default(),
            Some(&mut log),
        )
        .unwrap();
        assert!(lines.is_empty());
    }
    #[test]
    fn process_attachment_files_touches_only_the_listed_files() {
        if !ffmpeg_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let attachments = dir.path().join("attachments");
        fs::create_dir_all(&attachments).unwrap();
        let listed = attachments.join("a.png");
        let unlisted = attachments.join("b.png");
        write_test_png(&listed);
        write_test_png(&unlisted);
        let unlisted_before = fs::read(&unlisted).unwrap();

        let (_report, remap) = process_attachment_files(
            dir.path(),
            std::slice::from_ref(&listed),
            MediaMode::Convert,
            &CompressOptions::default(),
            None,
        )
        .unwrap();

        assert!(
            remap.contains_key("attachments/a.png"),
            "the listed file must be converted"
        );
        assert!(
            !remap.contains_key("attachments/b.png"),
            "a file the caller did not list must be left alone: scoping the pass \
             to an explicit list is the whole point of taking one"
        );
        assert!(unlisted.is_file(), "unlisted file must survive the pass");
        assert_eq!(
            fs::read(&unlisted).unwrap(),
            unlisted_before,
            "unlisted file was rewritten"
        );
    }
}
