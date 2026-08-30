//! `summarize_staging`, `transcode_staging`, and `delete_staging` commands.
//!
//! These back the two approval gates a staged import stops at (Decision 16):
//! `summarize_staging` recomputes what a staged folder holds so the first
//! gate can show it, `transcode_staging` runs the convert/compress pass the
//! exporter deferred (see `extract::exporter_media_mode`), and
//! `delete_staging` is the decline path — closing a gate without approving
//! deletes the staging folder outright.
//!
//! `summarize_staging` and `transcode_staging` both build a
//! [`message_ir_format::TranscodeOptions`] from the same form fields
//! `extract` parses, reusing its parsing helpers rather than re-deriving
//! them, so a summary and the pass it forecasts always agree on what
//! `Convert`/`Compress` mean.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use message_ir_format::{StagingSummary, TranscodeOptions, transcode_staged};
use tauri::Emitter;

use super::events::ExtractProgressEvent;
use super::extract::{parse_attachment_media, parse_compress_options, parse_max_resolution};
use super::jobs::{reset_and_clone_cancel, spawn_job};
use super::paths::canonical_within_root;
use super::push::ASSET_MAX_BYTES;
use crate::state::AppState;

/// Form fields shared by `summarize_staging` and `transcode_staging` — the
/// same media fields the Extract form parses, addressed at an already-staged
/// folder instead of a fresh backup.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagingArgs {
    /// Staging folder written by an earlier `extract` run.
    pub staging_dir: String,
    /// Attachment handling choice: `copy`, `convert`, `compress`, or `skip`.
    pub attachment_media: Option<String>,
    /// Video/image size cap for convert and compress: `720p`, `1080p`, or `4k`.
    pub media_max_resolution: Option<String>,
    /// Frame-rate cap for compressed video, for example `30`.
    pub media_max_fps: Option<String>,
    /// Smallest media file size that still counts as an attachment, for example `20M`.
    pub media_min_size: Option<String>,
}

/// Build the [`TranscodeOptions`] a summary or media pass runs with, from the
/// same fields the Extract form parses.
///
/// # Errors
///
/// Returns an error if any field fails to parse (see
/// [`parse_attachment_media`], [`parse_max_resolution`], and
/// [`parse_compress_options`]).
fn build_transcode_options(args: &StagingArgs) -> Result<TranscodeOptions, String> {
    let chosen = parse_attachment_media(args.attachment_media.as_deref())?;
    let max_resolution = parse_max_resolution(args.media_max_resolution.as_deref())?;
    let max_fps = args.media_max_fps.as_deref().unwrap_or("30");
    let min_size = args.media_min_size.as_deref().unwrap_or("20M");
    let compress = parse_compress_options(chosen, max_resolution, max_fps, min_size)?;
    Ok(TranscodeOptions {
        mode: chosen.media_mode(),
        compress,
        asset_max_bytes: ASSET_MAX_BYTES,
    })
}

/// Recompute what a staged folder holds, for the first approval gate.
///
/// Reports progress on `extract:progress` with `step: "prepare"`, so a long
/// summary of a huge folder shows movement on the step the user is already
/// looking at.
///
/// # Errors
///
/// Returns an error if a form field is invalid or the folder cannot be read.
#[tauri::command]
pub async fn summarize_staging(
    app: tauri::AppHandle,
    args: StagingArgs,
) -> Result<StagingSummary, String> {
    let options = build_transcode_options(&args)?;
    let staging_dir = PathBuf::from(&args.staging_dir);

    let progress_app = app.clone();
    message_ir_format::summarize_staging(&staging_dir, &options, &mut |progress| {
        let _ = progress_app.emit(
            "extract:progress",
            ExtractProgressEvent {
                step: "prepare".into(),
                done: progress.done,
                total: progress.total,
                bytes_done: None,
                bytes_total: None,
                status: None,
            },
        );
    })
    .map_err(|error| format!("{error:#}"))
}

/// Run the convert/compress pass over a staged folder, after the first gate
/// approves it.
///
/// Follows `extract`'s job shape: the cancel flag is reset through
/// [`reset_and_clone_cancel`], the pass runs on a background thread, and
/// progress/log/finished go back as `extract:*` events so the UI reuses one
/// progress view. A cancelled pass ends quietly (an `extract:log` line, no
/// `extract:error`) rather than surfacing the cancellation as a failure.
///
/// The report only carries counts, not per-file reasons (those are written
/// into the conversation files' `missing_reason` instead), so a nonzero
/// `failed`/`too_large` count is surfaced as one summarizing `extract:log`
/// line rather than invented per-file `extract:issue` events.
///
/// # Errors
///
/// Returns an error if a form field is invalid or another thread panicked
/// while holding the shared state lock. Failures during the pass — including
/// ffmpeg/ffprobe being unavailable — are sent as `extract:error`, verbatim,
/// not returned here.
#[tauri::command]
pub async fn transcode_staging(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    args: StagingArgs,
) -> Result<(), String> {
    let options = build_transcode_options(&args)?;
    let staging_dir = PathBuf::from(&args.staging_dir);
    let cancel = reset_and_clone_cancel(&state)?;

    let app_handle = app.clone();
    let progress_app = app.clone();
    spawn_job(app, move || {
        let _ = app_handle.emit(
            "extract:log",
            "Converting and compressing attachments…".to_string(),
        );

        let outcome = transcode_staged(&staging_dir, &options, Some(&cancel), &mut |progress| {
            let _ = progress_app.emit(
                "extract:progress",
                ExtractProgressEvent {
                    step: "media".into(),
                    done: progress.done,
                    total: progress.total,
                    bytes_done: None,
                    bytes_total: None,
                    status: None,
                },
            );
        });

        let report = match outcome {
            Ok(report) => report,
            // `transcode_staged` spells its cancellation error "canceled"
            // (one L) precisely so this layer can tell it apart from a real
            // failure — see the doc comment on `transcode.rs`'s
            // `check_cancel_now`. A cancelled pass is not an error: the user
            // asked for it, so it ends quietly rather than through
            // `extract:error`.
            Err(error) if error.to_string() == "canceled" => {
                let _ = app_handle.emit("extract:log", "Canceled.".to_string());
                return Ok(());
            }
            Err(error) => return Err(error),
        };

        let issues = report.failed + report.too_large;
        if issues > 0 {
            let _ = app_handle.emit(
                "extract:log",
                format!(
                    "{issues} file{plural} could not be converted; details are recorded in the staged files",
                    plural = if issues == 1 { "" } else { "s" }
                ),
            );
        }

        let payload = serde_json::json!({
            "converted": report.converted,
            "skipped": report.skipped,
            "too_large": report.too_large,
            "failed": report.failed,
            "missing": report.missing,
            "repointed": report.repointed,
            "bytes_before": report.bytes_before,
            "bytes_after": report.bytes_after,
        });
        let _ = app_handle.emit("extract:finished", payload.to_string());
        Ok(())
    });

    Ok(())
}

/// Arguments for [`delete_staging`].
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteStagingArgs {
    /// Staging folder to remove.
    pub staging_dir: String,
    /// Import Staging Directory root every staging folder must live under —
    /// the same root `open_path` guards.
    pub staging_root: String,
}

/// Delete a staging folder — the decline path's terminal action (Decision
/// 16): closing an approval gate without approving deletes the folder
/// outright.
///
/// # Errors
///
/// Returns an error when `staging_dir` resolves outside `staging_root` or
/// the folder cannot be removed. Refuses rather than silently doing nothing,
/// so a path bug here cannot turn into a delete somewhere else on disk.
#[tauri::command]
pub fn delete_staging(args: DeleteStagingArgs) -> Result<(), String> {
    delete_staging_dir(Path::new(&args.staging_root), Path::new(&args.staging_dir))
}

/// Delete `path`, refusing anything that does not resolve inside
/// `staging_root`.
///
/// A `path` that no longer exists is treated as already deleted — the
/// decline path may run after a crash that already removed it.
///
/// # Errors
///
/// Returns an error when `path` resolves outside `staging_root` or the
/// folder cannot be removed.
fn delete_staging_dir(staging_root: &Path, path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let resolved = canonical_within_root(path, staging_root)?;
    std::fs::remove_dir_all(&resolved)
        .map_err(|error| format!("Could not delete {}: {error}", resolved.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::MediaMode;

    #[test]
    fn delete_staging_refuses_a_path_outside_the_staging_root() {
        // This command deletes a directory tree. The only thing standing
        // between a path bug and someone's home folder is this check.
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("keep-me");
        std::fs::create_dir_all(&victim).unwrap();

        let err = delete_staging_dir(root.path(), &victim).unwrap_err();

        assert!(err.contains("staging"), "the refusal should say why: {err}");
        assert!(victim.exists());
    }

    #[test]
    fn delete_staging_removes_a_folder_inside_the_root() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("staging-run-1");
        std::fs::create_dir_all(staged.join("attachments")).unwrap();
        std::fs::write(staged.join("a.jsonl"), b"{}").unwrap();

        delete_staging_dir(root.path(), &staged).unwrap();

        assert!(!staged.exists());
    }

    #[test]
    fn delete_staging_is_quiet_about_a_folder_that_is_already_gone() {
        // The decline path may run after a crash that already removed it.
        let root = tempfile::tempdir().unwrap();
        assert!(delete_staging_dir(root.path(), &root.path().join("never-existed")).is_ok());
    }

    #[test]
    fn transcode_options_use_the_shared_asset_max_bytes() {
        let args = StagingArgs {
            staging_dir: "/tmp/staging".into(),
            attachment_media: Some("compress".into()),
            media_max_resolution: Some("720p".into()),
            media_max_fps: Some("24".into()),
            media_min_size: Some("5M".into()),
        };
        let options = build_transcode_options(&args).unwrap();
        assert_eq!(options.asset_max_bytes, ASSET_MAX_BYTES);
        assert_eq!(options.mode, MediaMode::Compress);
        assert_eq!(options.compress.max_fps, 24.0);
    }

    #[test]
    fn transcode_options_default_the_media_fields_like_extract_does() {
        let args = StagingArgs {
            staging_dir: "/tmp/staging".into(),
            attachment_media: Some("convert".into()),
            media_max_resolution: None,
            media_max_fps: None,
            media_min_size: None,
        };
        let options = build_transcode_options(&args).unwrap();
        assert_eq!(options.mode, MediaMode::Convert);
        // Convert does not use CompressOptions, but defaulting must still
        // succeed rather than error on missing fields.
        assert_eq!(options.compress, media::CompressOptions::default());
    }
}
