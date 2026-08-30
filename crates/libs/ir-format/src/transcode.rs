//! Convert or compress a staged folder, patching the conversation files it wrote.
//!
//! This runs after the staging folder is complete and before anything is
//! uploaded, so the import can stop and ask between the two. It commits one
//! attachment at a time through a rename, which is what makes it resumable
//! with no progress record: a file under its final derivative name is fully
//! patched, and an original still on disk means work remains.
//!
//! ## Naming and resume
//!
//! The final name for a converted attachment is `{original_stem}-mv.{target_ext}`
//! — never the bare name [`media::derivative_name`] returns. A same-format
//! compress (`photo.jpg` → `photo.jpg`) or a video in either mode
//! (`clip.mp4` → `clip.mp4`) would otherwise produce a final name equal to
//! the source's own name, or equal to an already-committed derivative's own
//! name on a later resume — both break the "an original on disk means work
//! remains" invariant this pass depends on for resume. The `-mv` suffix
//! cannot collide with a staged original's own name: staged names come from
//! `attachment_dest_name` (`{local-date}-{digest16}{ext}`), whose stem is
//! hex, and hex digits never include `m` or `v`.
//!
//! An attachment is pending when its recorded path exists on disk,
//! [`media::derivative_name`] returns `Some` for that file, and the file's
//! stem does not end in `-mv` (a `-mv` stem marks an already-committed
//! derivative, which must never re-enter the pending list — for a video, or
//! a compressed same-format file, `derivative_name` would otherwise call it
//! pending forever and re-degrade it on every resume).
//!
//! When the recorded path's stem ends in `-mv` but the file is *not* on
//! disk, the previous run crashed between patching the conversation file and
//! renaming the derivative into place. The original is still there under its
//! old name, so the pass heals: it strips the `-mv` suffix and looks under
//! `attachments/` for a file with that stem (any extension) that
//! `derivative_name` still wants, and re-transcodes it — the recorded
//! digest/size/path are stale regardless (decision 29), so a heal re-patches
//! exactly like a fresh conversion. When no such file exists, the attachment
//! is unrecoverable and is marked `file_missing`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use media::{CompressOptions, MediaMode, TranscodeOutcome};
use message_ir::{ConversationDocument, IrAttachment};
use message_vault_io_core::{CancelFlag, check_cancel, mime_for_rel};

use crate::read_json::read_conversation_jsonl;
use crate::util::UNSAFE_ATTACHMENT_PATH_PREFIX;
use crate::write::write_conversation_jsonl_to;

/// Suffix on a derivative that is written but not yet committed.
///
/// Named so it survives the media crate's ffmpeg-scratch sweep, which matches
/// `.msgmedia.tmp.` — deleting this file would delete the resume signal.
const IN_PROGRESS_SUFFIX: &str = ".in_progress";

/// Suffix on a committed derivative's stem, marking it as already converted.
///
/// Staged attachment names come from `attachment_dest_name`
/// (`{local-date}-{digest16}{ext}`), whose stem never ends in `-mv`: `m` and
/// `v` are not hex digits. That makes this suffix an unambiguous "already
/// done" marker no staged original can wear by coincidence.
const COMMITTED_SUFFIX: &str = "-mv";

/// What the media pass should do.
#[derive(Debug, Clone)]
pub struct TranscodeOptions {
    /// Convert or Compress. Clone and Disabled make the pass a no-op.
    pub mode: MediaMode,
    /// Video targets, from the import form.
    pub compress: CompressOptions,
    /// A derivative larger than this is dropped rather than uploaded.
    pub asset_max_bytes: u64,
}

/// How far the pass has got.
#[derive(Debug, Clone, Copy)]
pub struct TranscodeProgress {
    /// Files finished, however they finished.
    pub done: usize,
    /// Files the pass found work for.
    pub total: usize,
}

/// What the pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscodeReport {
    /// Attachments replaced by a derivative.
    pub converted: usize,
    /// Attachments the media step left alone.
    pub skipped: usize,
    /// Derivatives that came out over the size limit and were dropped.
    pub too_large: usize,
    /// Attachments ffmpeg could not process.
    pub failed: usize,
    /// Attachments an interrupted prior run lost beyond recovery — neither
    /// the original nor a usable derivative survived. `missing_reason` is set
    /// to `file_missing`.
    pub missing: usize,
    /// Total bytes of the originals the pass replaced.
    pub bytes_before: u64,
    /// Total bytes of the derivatives it wrote.
    pub bytes_after: u64,
}

/// Convert or compress every original still staged under `staging_dir`.
///
/// Safe to call again after an interruption: it re-reads the folder and does
/// whatever is left.
///
/// # Errors
///
/// Returns an error when the folder cannot be read, a conversation file cannot
/// be parsed or written, or the pass is cancelled. A single attachment ffmpeg
/// cannot process is an item-level issue recorded in the report, never an error.
pub fn transcode_staged(
    staging_dir: &Path,
    options: &TranscodeOptions,
    cancel: Option<&CancelFlag>,
    on_progress: &mut dyn FnMut(TranscodeProgress),
) -> Result<TranscodeReport> {
    if matches!(options.mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok(TranscodeReport::default());
    }
    let files = conversation_files(staging_dir)?;
    // Counting up front costs a second parse of each conversation file and
    // buys an honest progress total. Decision 31 accepts the re-read.
    let total = count_remaining(staging_dir, &files, options.mode)?;
    on_progress(TranscodeProgress { done: 0, total });

    let mut report = TranscodeReport::default();
    let mut done = 0usize;
    for jsonl in &files {
        check_cancel(cancel).map_err(anyhow::Error::msg)?;
        let mut doc = read_conversation_jsonl(jsonl)?;
        let work = pending_in(staging_dir, &doc, options.mode)?;
        for (msg_idx, att_idx, src) in work {
            check_cancel(cancel).map_err(anyhow::Error::msg)?;
            match src {
                PendingSrc::Resumable(src_path) => {
                    apply_one(
                        staging_dir,
                        jsonl,
                        &mut doc,
                        msg_idx,
                        att_idx,
                        &src_path,
                        options,
                        &mut report,
                    )?;
                }
                PendingSrc::Unrecoverable => {
                    apply_unrecoverable(jsonl, &mut doc, msg_idx, att_idx, &mut report)?;
                }
            }
            done += 1;
            on_progress(TranscodeProgress { done, total });
        }
    }
    Ok(report)
}

/// `*.jsonl` files directly under `staging_dir`, sorted, non-recursive — the
/// sink writes them flat (`FormatSink::open_prepared`).
fn conversation_files(staging_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(staging_dir)
        .with_context(|| format!("read {}", staging_dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    Ok(files)
}

/// Sum of [`pending_in`] over every conversation file — the progress total.
fn count_remaining(staging_dir: &Path, files: &[PathBuf], mode: MediaMode) -> Result<usize> {
    let mut total = 0usize;
    for jsonl in files {
        let doc = read_conversation_jsonl(jsonl)?;
        total += pending_in(staging_dir, &doc, mode)?.len();
    }
    Ok(total)
}

/// One attachment still needing work, discovered by [`pending_in`].
enum PendingSrc {
    /// Transcode this file and commit the result — either a fresh original,
    /// or an original recovered by the crash heal.
    Resumable(PathBuf),
    /// Neither the original nor a recoverable derivative survived a prior
    /// interruption; mark it lost.
    Unrecoverable,
}

/// Attachments in `doc` still needing the media step, as
/// `(message index, attachment index, what to do)`.
///
/// See the module docs for the pending rule and the crash-heal rule.
fn pending_in(
    staging_dir: &Path,
    doc: &ConversationDocument,
    mode: MediaMode,
) -> Result<Vec<(usize, usize, PendingSrc)>> {
    let mut out = Vec::new();
    for (msg_idx, msg) in doc.messages.iter().enumerate() {
        for (att_idx, att) in msg.attachments.iter().enumerate() {
            let Some(rel) = att.path.as_deref() else {
                continue;
            };
            let abs = safe_attachment_path(staging_dir, rel)?;
            let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let committed = stem.ends_with(COMMITTED_SUFFIX);

            if abs.is_file() {
                // A committed derivative (or anything the media step does
                // not touch) is not work; only a fresh, still-original file
                // is.
                if !committed && media::derivative_name(&abs, mode).is_some() {
                    out.push((msg_idx, att_idx, PendingSrc::Resumable(abs)));
                }
                continue;
            }

            if !committed {
                // Missing for a reason the media pass has no business with
                // (never staged, dropped earlier). Not this pass's problem.
                continue;
            }
            // The recorded path is a committed derivative's name, but
            // nothing is there: the previous run crashed between patching
            // the conversation file and renaming the derivative into place.
            let orig_stem = &stem[..stem.len() - COMMITTED_SUFFIX.len()];
            match find_recoverable_original(staging_dir, orig_stem, mode)? {
                Some(found) => out.push((msg_idx, att_idx, PendingSrc::Resumable(found))),
                None => out.push((msg_idx, att_idx, PendingSrc::Unrecoverable)),
            }
        }
    }
    Ok(out)
}

/// Resolve `rel` (an attachment's recorded relative path) under `staging_dir`,
/// rejecting anything that could escape it.
fn safe_attachment_path(staging_dir: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        anyhow::bail!("{UNSAFE_ATTACHMENT_PATH_PREFIX}: {rel}");
    }
    for comp in rel_path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            anyhow::bail!("{UNSAFE_ATTACHMENT_PATH_PREFIX} (contains ..): {rel}");
        }
    }
    Ok(staging_dir.join(rel_path))
}

/// Find a file under `staging_dir/attachments` whose stem is `orig_stem` and
/// that the media step would still touch — the crash-heal search.
fn find_recoverable_original(
    staging_dir: &Path,
    orig_stem: &str,
    mode: MediaMode,
) -> Result<Option<PathBuf>> {
    let attachments_dir = staging_dir.join("attachments");
    if !attachments_dir.is_dir() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(&attachments_dir)
        .with_context(|| format!("read {}", attachments_dir.display()))?
    {
        let path = entry
            .with_context(|| format!("read entry in {}", attachments_dir.display()))?
            .path();
        if !path.is_file() {
            continue;
        }
        if path.file_stem().and_then(|s| s.to_str()) == Some(orig_stem)
            && media::derivative_name(&path, mode).is_some()
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// The name a committed derivative of `src` gets: `{original_stem}-mv.{target_ext}`.
///
/// Never the bare name [`media::derivative_name`] returns — see the module
/// docs for why a same-format compress or a video in either mode makes that
/// name collide with the source, or with an already-committed derivative on
/// a later resume.
fn final_derivative_name(src: &Path, mode: MediaMode) -> Option<String> {
    let forecast = media::derivative_name(src, mode)?;
    let target_ext = Path::new(&forecast)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let orig_stem = src.file_stem().and_then(|s| s.to_str())?;
    Some(format!("{orig_stem}{COMMITTED_SUFFIX}.{target_ext}"))
}

/// Transcode one attachment and commit it, in the order decision 28 fixes:
/// derivative written, conversation file patched, derivative renamed into its
/// final name, original deleted. Reversing any pair leaves the folder lying
/// about itself.
#[allow(clippy::too_many_arguments)]
fn apply_one(
    staging_dir: &Path,
    jsonl: &Path,
    doc: &mut ConversationDocument,
    msg_idx: usize,
    att_idx: usize,
    src: &Path,
    options: &TranscodeOptions,
    report: &mut TranscodeReport,
) -> Result<()> {
    let Some(name) = final_derivative_name(src, options.mode) else {
        report.skipped += 1;
        return Ok(());
    };
    let attachments_dir = staging_dir.join("attachments");
    let final_path = attachments_dir.join(&name);
    let marker = attachments_dir.join(format!("{name}{IN_PROGRESS_SUFFIX}"));
    // The `-mv` suffix in `final_derivative_name` makes this structurally
    // impossible, but a future naming change must fail loudly rather than
    // silently destroy the original before it is committed.
    debug_assert_ne!(
        final_path.as_path(),
        src,
        "final derivative name collided with the source"
    );
    let original_len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);

    match media::transcode_file(src, &marker, options.mode, &options.compress) {
        Err(err) => {
            // The closed reason set, with the detail that only this reason
            // carries. Phase 1 made the display side keep it.
            let detail = err.to_string();
            set_missing(
                doc,
                msg_idx,
                att_idx,
                &format!("convert_failed: {detail}"),
                None,
            );
            write_conversation_jsonl_to(jsonl, doc)?;
            report.failed += 1;
            Ok(())
        }
        Ok(TranscodeOutcome::Skipped) => {
            report.skipped += 1;
            Ok(())
        }
        Ok(TranscodeOutcome::Produced) => {
            let produced_len = std::fs::metadata(&marker)
                .with_context(|| format!("stat {}", marker.display()))?
                .len();
            if produced_len > options.asset_max_bytes {
                // Decision 45: skipped, not reverted.
                set_missing(doc, msg_idx, att_idx, "too_large", Some(produced_len));
                write_conversation_jsonl_to(jsonl, doc)?;
                let _ = std::fs::remove_file(&marker);
                let _ = std::fs::remove_file(src);
                report.too_large += 1;
                return Ok(());
            }
            // Decision 29: read the file on disk. A replayed digest can be
            // stale, and the vault dedupes assets by sha256.
            let digest = media::file_sha256(&marker)?;
            {
                let att = attachment_at(doc, msg_idx, att_idx)?;
                att.path = Some(format!("attachments/{name}"));
                att.digest_sha256 = Some(digest);
                att.size_bytes = Some(produced_len);
                att.mime_type = mime_for_rel(&format!("attachments/{name}"));
                att.missing_reason = None;
            }
            write_conversation_jsonl_to(jsonl, doc)?;
            if final_path.exists() {
                // A previous run may have crashed between this rename and
                // the delete-original step below; the freshly recomputed
                // derivative replaces whatever is already sitting there.
                std::fs::remove_file(&final_path)
                    .with_context(|| format!("remove stale {}", final_path.display()))?;
            }
            std::fs::rename(&marker, &final_path)
                .with_context(|| format!("commit {}", final_path.display()))?;
            let _ = std::fs::remove_file(src);
            report.converted += 1;
            report.bytes_before += original_len;
            report.bytes_after += produced_len;
            Ok(())
        }
    }
}

/// Mark an attachment `file_missing`: nothing survived an interrupted prior
/// run's crash window closely enough to recover.
fn apply_unrecoverable(
    jsonl: &Path,
    doc: &mut ConversationDocument,
    msg_idx: usize,
    att_idx: usize,
    report: &mut TranscodeReport,
) -> Result<()> {
    set_missing(doc, msg_idx, att_idx, "file_missing", None);
    write_conversation_jsonl_to(jsonl, doc)?;
    report.missing += 1;
    Ok(())
}

/// Index into `doc` at the position `pending_in` found. A failure here is a
/// bug (the indices came from this same document), not an input to validate.
fn attachment_at(
    doc: &mut ConversationDocument,
    msg_idx: usize,
    att_idx: usize,
) -> Result<&mut IrAttachment> {
    doc.messages
        .get_mut(msg_idx)
        .and_then(|m| m.attachments.get_mut(att_idx))
        .context("attachment index out of range (pending_in and apply_one must agree)")
}

/// Clear `path` and `digest_sha256`, set `missing_reason`, and record
/// `size_bytes` (or clear it, for a reason with no meaningful size).
fn set_missing(
    doc: &mut ConversationDocument,
    msg_idx: usize,
    att_idx: usize,
    reason: &str,
    size_bytes: Option<u64>,
) {
    if let Ok(att) = attachment_at(doc, msg_idx, att_idx) {
        att.path = None;
        att.digest_sha256 = None;
        att.missing_reason = Some(reason.to_string());
        att.size_bytes = size_bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::ffmpeg_available;
    use std::sync::atomic::Ordering;

    /// A staging folder holding one conversation and one attachment.
    ///
    /// Writes `attachments/<name>` with `bytes`, and one `.jsonl` whose
    /// single message has non-empty text and one attachment pointing at
    /// `attachments/<name>`. Built with `message_ir::testutil::sample_document`
    /// and written with `write_conversation_jsonl_to`, so the fixture and the
    /// code under test agree on the on-disk shape.
    ///
    /// Returns (staging dir, conversation file path, attachment path).
    fn staged_one(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let attachments_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&attachments_dir).unwrap();
        let rel = format!("attachments/{name}");
        let original = dir.path().join(&rel);
        std::fs::write(&original, bytes).unwrap();

        let mut doc = message_ir::testutil::sample_document("hello from the fixture");
        doc.messages[0].attachments = vec![IrAttachment {
            path: Some(rel),
            original_name: Some(name.to_string()),
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: Some(bytes.len() as u64),
            missing_reason: None,
            bytes: None,
        }];
        doc.finalize_stats();

        let jsonl = dir.path().join(format!("{}.jsonl", doc.filename_stem()));
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        (dir, jsonl, original)
    }

    fn options(mode: MediaMode, limit: u64) -> TranscodeOptions {
        TranscodeOptions {
            mode,
            compress: CompressOptions::default(),
            asset_max_bytes: limit,
        }
    }

    fn test_png_bytes() -> Vec<u8> {
        #[rustfmt::skip]
        const PNG_1X1_RGB: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
            0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
            0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
            0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        PNG_1X1_RGB.to_vec()
    }

    fn hex_sha256(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn a_converted_attachment_is_patched_before_its_final_name_exists() {
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(report.converted, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        assert_eq!(att.path.as_deref(), Some("attachments/photo-mv.jpg"));
        assert!(
            !original.exists(),
            "original deleted after the patch committed"
        );
        assert!(dir.path().join("attachments/photo-mv.jpg").exists());
        assert!(
            std::fs::read_dir(dir.path().join("attachments"))
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".in_progress")),
            "no marker survives a completed file"
        );
    }

    #[test]
    fn the_digest_and_size_are_recomputed_from_the_derivative() {
        if !ffmpeg_available() {
            return;
        }
        // Decision 29: ffmpeg output is not byte-identical across runs, so a
        // replayed digest would be a silent corruption — the vault dedupes
        // assets by sha256.
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        let derivative = dir.path().join("attachments/photo-mv.jpg");
        let on_disk = std::fs::read(&derivative).unwrap();
        assert_eq!(
            att.digest_sha256.as_deref(),
            Some(hex_sha256(&on_disk).as_str())
        );
        assert_eq!(att.size_bytes, Some(on_disk.len() as u64));
        assert_eq!(att.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn an_interrupted_file_is_re_transcoded_not_adopted() {
        if !ffmpeg_available() {
            return;
        }
        // Decision 28: nothing distinguishes a complete .in_progress from a
        // truncated one without hashing it, so the marker's bytes are never used.
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        let marker = dir.path().join("attachments/photo-mv.jpg.in_progress");
        std::fs::write(&marker, b"truncated garbage from a killed run").unwrap();

        transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        let derivative = dir.path().join("attachments/photo-mv.jpg");
        assert_ne!(
            std::fs::read(&derivative).unwrap(),
            b"truncated garbage from a killed run".to_vec(),
            "the marker's bytes must never be adopted"
        );
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].path.as_deref(),
            Some("attachments/photo-mv.jpg")
        );
    }

    #[test]
    fn an_already_converted_attachment_is_left_alone_on_a_second_run() {
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();
        let after_first = std::fs::read(dir.path().join("attachments/photo-mv.jpg")).unwrap();

        let second = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(second.converted, 0, "resume must not redo finished work");
        assert_eq!(
            std::fs::read(dir.path().join("attachments/photo-mv.jpg")).unwrap(),
            after_first
        );
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].path.as_deref(),
            Some("attachments/photo-mv.jpg")
        );
    }

    #[test]
    fn a_derivative_over_the_limit_becomes_too_large_and_keeps_the_message() {
        if !ffmpeg_available() {
            return;
        }
        // Decision 45: skipped, not reverted. Falling back to the original
        // would store the format the user asked to be rid of.
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, 1),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(report.too_large, 1);
        assert_eq!(report.converted, 0);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let msg = &doc.messages[0];
        assert!(!msg.text.is_empty(), "the message keeps its text");
        let att = &msg.attachments[0];
        assert_eq!(att.missing_reason.as_deref(), Some("too_large"));
        assert_eq!(att.path, None, "nothing to upload");
        assert!(!original.exists(), "the original is not kept as a fallback");
        assert!(!dir.path().join("attachments/photo-mv.jpg").exists());
    }

    #[test]
    fn a_conversion_failure_becomes_a_per_item_reason_carrying_the_detail() {
        let (dir, jsonl, original) = staged_one("broken.png", b"not a png at all");
        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        );

        // ffmpeg failing on one file is an issue, never a failed pass.
        let report = report.unwrap();
        assert_eq!(report.failed, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let reason = doc.messages[0].attachments[0]
            .missing_reason
            .clone()
            .unwrap();
        assert!(
            reason.starts_with("convert_failed: "),
            "reason must stay inside the closed set: {reason}"
        );
        assert!(
            reason.len() > "convert_failed: ".len(),
            "the detail must survive"
        );
        assert!(
            original.exists(),
            "a file that failed to convert is still there"
        );
    }

    #[test]
    fn cancelling_stops_the_pass_without_corrupting_the_folder() {
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        let cancel = CancelFlag::default();
        cancel.store(true, Ordering::SeqCst);

        let err = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            Some(&cancel),
            &mut |_| {},
        );

        assert!(err.is_err());
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].path.as_deref(),
            Some("attachments/photo.png"),
            "an untouched attachment still points at its original"
        );
    }

    #[test]
    fn progress_counts_the_work_it_actually_has() {
        let (dir, _, _) = staged_one("notes.pdf", b"%PDF-1.4");
        let mut seen = Vec::new();
        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |p| seen.push((p.done, p.total)),
        )
        .unwrap();
        // A file the media step does not handle is not work.
        assert_eq!(report.converted, 0);
        assert!(seen.iter().all(|(_, total)| *total == 0));
    }

    #[test]
    fn a_crash_between_the_patch_and_the_rename_heals_by_re_transcoding_the_original() {
        if !ffmpeg_available() {
            return;
        }
        // Hand-simulate the crash window between decision 28's steps 4-5
        // (patch committed, conversation file written) and step 6 (marker
        // renamed into its final name): the doc already points at the -mv
        // name, a marker sits under .in_progress, and the original is still
        // on disk under its old name because the delete never ran.
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let mut doc = read_conversation_jsonl(&jsonl).unwrap();
        {
            let att = &mut doc.messages[0].attachments[0];
            att.path = Some("attachments/photo-mv.jpg".into());
            att.digest_sha256 = Some("deadbeef".repeat(8));
            att.size_bytes = Some(1234);
            att.mime_type = Some("image/jpeg".into());
        }
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        let marker = dir.path().join("attachments/photo-mv.jpg.in_progress");
        std::fs::write(&marker, b"leftover bytes from the crashed run").unwrap();
        assert!(
            original.exists(),
            "the original is still there before the pass runs"
        );

        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(report.converted, 1);
        let derivative = dir.path().join("attachments/photo-mv.jpg");
        assert!(
            derivative.exists(),
            "the derivative exists under the -mv name"
        );
        assert_ne!(
            std::fs::read(&derivative).unwrap(),
            b"leftover bytes from the crashed run".to_vec(),
            "the heal re-transcodes rather than adopting the marker's bytes"
        );
        assert!(!marker.exists(), "no marker survives a completed heal");
        assert!(
            !original.exists(),
            "the original is gone once the heal commits"
        );
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].path.as_deref(),
            Some("attachments/photo-mv.jpg"),
            "the doc points at the healed derivative"
        );
    }

    #[test]
    fn a_crash_that_lost_both_the_marker_and_the_original_is_unrecoverable() {
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let mut doc = read_conversation_jsonl(&jsonl).unwrap();
        doc.messages[0].attachments[0].path = Some("attachments/photo-mv.jpg".into());
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        // Nothing recoverable is left: no marker, and the original itself is gone too.
        std::fs::remove_file(&original).unwrap();

        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(report.missing, 1);
        assert_eq!(report.converted, 0);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        assert_eq!(att.missing_reason.as_deref(), Some("file_missing"));
        assert_eq!(att.path, None);
        assert_eq!(att.digest_sha256, None);
    }
}
