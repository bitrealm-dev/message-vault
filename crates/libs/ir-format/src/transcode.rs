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
//!
//! ## Aliasing
//!
//! Staged names are content-addressed (`attachment_dest_name`), and both
//! writers are idempotent by name, so two attachments — in the same document
//! or in different ones — can legitimately record the identical `path` when
//! they carry the same bytes. Every patch here therefore applies to *every*
//! attachment recorded at a given path, not just the one that happened to be
//! scanned first, and a recorded path that has gone missing without a `-mv`
//! stem is checked against its would-be derivative before being written off:
//! another attachment (in this document or another) may already have
//! transcoded and deleted the shared original, in which case this one is
//! repointed at the existing derivative rather than failing or re-encoding.
//!
//! ## Tool availability and failure
//!
//! ffmpeg/ffprobe are probed once, before any document is touched — parity
//! with `media::process_attachments_dir`. A missing pair fails the whole
//! pass; it must never brand every attachment `convert_failed: ffmpeg not
//! found`. An attachment ffmpeg genuinely fails on, by contrast, is an
//! item-level issue: `missing_reason` gets `convert_failed: <detail>` and the
//! original — still on disk, untouched — keeps its `path` and
//! `digest_sha256` so a resume retries it rather than treating a transient
//! failure as permanent loss.

use std::collections::HashSet;
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
    /// Attachments repointed at an already-existing derivative without a
    /// transcode — the aliasing case: another attachment (this document or
    /// another) already converted and deleted the shared original.
    pub repointed: usize,
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
/// Returns an error when the pass is cancelled, ffmpeg/ffprobe are
/// unavailable, or the folder cannot be read, or a conversation file cannot
/// be parsed or written. A single attachment ffmpeg cannot process is an
/// item-level issue recorded in the report, never an error.
pub fn transcode_staged(
    staging_dir: &Path,
    options: &TranscodeOptions,
    cancel: Option<&CancelFlag>,
    on_progress: &mut dyn FnMut(TranscodeProgress),
) -> Result<TranscodeReport> {
    if matches!(options.mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok(TranscodeReport::default());
    }
    // A cancel already requested short-circuits before we even ask whether
    // the tools are there.
    check_cancel_now(cancel)?;
    // Parity with `process_attachments_dir`: fail the whole pass up front
    // when the tools are missing, rather than branding every attachment
    // `convert_failed: ffmpeg not found`.
    if !media::ffmpeg_available() {
        let probe = media::probe_ffmpeg_tools(None);
        anyhow::bail!(
            "ffmpeg/ffprobe are required to convert or compress attachments: {}",
            probe.error.unwrap_or_else(|| "not found".to_string())
        );
    }

    let files = conversation_files(staging_dir)?;
    // Counting up front costs a second parse of each conversation file and
    // buys an honest progress total. Decision 31 accepts the re-read.
    let total = count_remaining(staging_dir, &files, options.mode)?;
    on_progress(TranscodeProgress { done: 0, total });

    let mut report = TranscodeReport::default();
    let mut done = 0usize;
    for jsonl in &files {
        check_cancel_now(cancel)?;
        let mut doc = read_conversation_jsonl(jsonl)?;
        let work = pending_in(staging_dir, &doc, options.mode)?;
        for item in work {
            check_cancel_now(cancel)?;
            match item {
                PendingWork::Transcode { recorded_rel, src } => {
                    apply_transcode(
                        staging_dir,
                        jsonl,
                        &mut doc,
                        &recorded_rel,
                        &src,
                        false,
                        options,
                        &mut report,
                    )?;
                }
                PendingWork::HealTranscode { recorded_rel, src } => {
                    apply_transcode(
                        staging_dir,
                        jsonl,
                        &mut doc,
                        &recorded_rel,
                        &src,
                        true,
                        options,
                        &mut report,
                    )?;
                }
                PendingWork::Repoint {
                    recorded_rel,
                    derivative,
                } => {
                    apply_repoint(jsonl, &mut doc, &recorded_rel, &derivative, &mut report)?;
                }
                PendingWork::Unrecoverable { recorded_rel } => {
                    apply_unrecoverable(jsonl, &mut doc, &recorded_rel, &mut report)?;
                }
            }
            done += 1;
            on_progress(TranscodeProgress { done, total });
        }
    }
    Ok(report)
}

/// `check_cancel`, spelled the way `run_attachment_jobs` spells it —
/// `"canceled"`, one L — since the Tauri command layer string-matches on
/// that convention.
fn check_cancel_now(cancel: Option<&CancelFlag>) -> Result<()> {
    check_cancel(cancel).map_err(|_| anyhow::anyhow!("canceled"))
}

/// `*.jsonl` files directly under `staging_dir`, sorted, non-recursive — the
/// sink writes them flat (`FormatSink::open_prepared`).
///
/// `pub(crate)`: shared with `staging_summary`, which walks the same list.
pub(crate) fn conversation_files(staging_dir: &Path) -> Result<Vec<PathBuf>> {
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

/// One deduplicated unit of work found in a document.
///
/// A recorded path is a literal string: every attachment carrying the same
/// one (in the same document) names the same physical file and therefore
/// shares the same fate, so [`pending_in`] queues each recorded path once.
enum PendingWork {
    /// Transcode `src` — untouched, still at its recorded name — and commit.
    Transcode { recorded_rel: String, src: PathBuf },
    /// The recorded name is a crash-heal `-mv` name with nothing on disk;
    /// `src` is the recovered original.
    HealTranscode { recorded_rel: String, src: PathBuf },
    /// The recorded path is gone, but its would-be derivative already
    /// exists — the aliasing case, or the crash window right after the
    /// shared original's delete. Repoint, no transcode needed.
    Repoint {
        recorded_rel: String,
        derivative: PathBuf,
    },
    /// Nothing recoverable survived.
    Unrecoverable { recorded_rel: String },
}

/// Attachments in `doc` still needing the media step, deduplicated by
/// recorded path.
///
/// See the module docs for the pending rule, the crash-heal rule, and the
/// aliasing repoint rule.
fn pending_in(
    staging_dir: &Path,
    doc: &ConversationDocument,
    mode: MediaMode,
) -> Result<Vec<PendingWork>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for msg in &doc.messages {
        for att in &msg.attachments {
            let Some(rel) = att.path.as_deref() else {
                continue;
            };
            if !seen.insert(rel.to_string()) {
                // Already classified via an earlier attachment recorded at
                // the same path in this document.
                continue;
            }
            let abs = safe_attachment_path(staging_dir, rel)?;
            let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let committed = stem.ends_with(COMMITTED_SUFFIX);

            if abs.is_file() {
                // A committed derivative (or anything the media step does
                // not touch) is not work; only a fresh, still-original file
                // is.
                if !committed && media::derivative_name(&abs, mode).is_some() {
                    out.push(PendingWork::Transcode {
                        recorded_rel: rel.to_string(),
                        src: abs,
                    });
                }
                continue;
            }

            if committed {
                // The recorded path is a committed derivative's name, but
                // nothing is there: the previous run crashed between
                // patching the conversation file and renaming the
                // derivative into place.
                let orig_stem = &stem[..stem.len() - COMMITTED_SUFFIX.len()];
                match find_recoverable_original(staging_dir, orig_stem, mode)? {
                    Some(found) => out.push(PendingWork::HealTranscode {
                        recorded_rel: rel.to_string(),
                        src: found,
                    }),
                    None => out.push(PendingWork::Unrecoverable {
                        recorded_rel: rel.to_string(),
                    }),
                }
                continue;
            }

            // Not committed, not on disk: maybe another attachment sharing
            // the same bytes (this document or another) already converted
            // it and deleted the shared original.
            if let Some(name) = final_derivative_name(&abs, mode) {
                let derivative = staging_dir.join("attachments").join(&name);
                if derivative.is_file() {
                    out.push(PendingWork::Repoint {
                        recorded_rel: rel.to_string(),
                        derivative,
                    });
                }
                // else: missing for a reason the media pass has no business
                // with (never staged, dropped earlier).
            }
        }
    }
    Ok(out)
}

/// Resolve `rel` (an attachment's recorded relative path) under `staging_dir`,
/// rejecting anything that could escape it.
///
/// `pub(crate)`: shared with `staging_summary`, which resolves the same
/// recorded paths to measure them.
pub(crate) fn safe_attachment_path(staging_dir: &Path, rel: &str) -> Result<PathBuf> {
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

/// `attachments/{file name of path}` — the doc-relative form every patch
/// records.
fn attachment_rel(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no valid UTF-8 file name", path.display()))?;
    Ok(format!("attachments/{name}"))
}

/// Apply `patch` to every attachment across `doc` recorded at `recorded_rel`.
///
/// Content-addressed staging means more than one attachment — in the same
/// document — can legitimately share a path; every one of them must move
/// together; see the module docs.
fn patch_all_matching(
    doc: &mut ConversationDocument,
    recorded_rel: &str,
    patch: impl Fn(&mut IrAttachment),
) {
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            if att.path.as_deref() == Some(recorded_rel) {
                patch(att);
            }
        }
    }
}

/// What a heal recovery repoints an attachment at: the recovered original,
/// read fresh off disk.
struct RecoveredOriginal {
    rel: String,
    digest: String,
    size: u64,
    mime: Option<String>,
}

/// Compute the fields a heal repoint writes, from `src` (the recovered
/// original) on disk.
///
/// Shared by both places a heal has to fall back to the original rather than
/// a derivative: the media step declining the file (`Skipped`), and ffmpeg
/// failing on it (`Err`). In both cases `recorded_rel` — the phantom `-mv`
/// name a crashed prior run already wrote into the document — must not be
/// left standing, since nothing will ever exist under it.
fn recovered_original_fields(src: &Path) -> Result<RecoveredOriginal> {
    let rel = attachment_rel(src)?;
    let digest = media::file_sha256(src)?;
    let size = std::fs::metadata(src)
        .with_context(|| format!("stat {}", src.display()))?
        .len();
    let mime = mime_for_rel(&rel);
    Ok(RecoveredOriginal {
        rel,
        digest,
        size,
        mime,
    })
}

/// Transcode `src` and commit it, in the order decision 28 fixes: derivative
/// written, conversation file patched, derivative renamed into its final
/// name, original deleted. Reversing any pair leaves the folder lying about
/// itself.
///
/// `is_heal` marks a crash-heal recovery: `recorded_rel` currently points at
/// a `-mv` name nothing produced yet, rather than at `src` itself. That
/// distinction only matters when the media step declines the file (see the
/// `Skipped` arm) — everywhere else a heal behaves exactly like a fresh
/// transcode.
#[allow(clippy::too_many_arguments)]
fn apply_transcode(
    staging_dir: &Path,
    jsonl: &Path,
    doc: &mut ConversationDocument,
    recorded_rel: &str,
    src: &Path,
    is_heal: bool,
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
    let original_len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);

    match media::transcode_file(src, &marker, options.mode, &options.compress) {
        Err(err) => {
            let reason = format!("convert_failed: {err}");
            if is_heal {
                // `recorded_rel` is the phantom `-mv` name a crashed prior
                // run already wrote into the document; nothing will ever
                // exist under it. Keeping it here — the way a non-heal
                // failure keeps its path — would strand the document
                // pointing at a name that can never exist. Repoint at the
                // recovered original first, exactly like the `Skipped` heal
                // arm below, then record the failure on it.
                let r = recovered_original_fields(src)?;
                patch_all_matching(doc, recorded_rel, |att| {
                    att.path = Some(r.rel.clone());
                    att.digest_sha256 = Some(r.digest.clone());
                    att.size_bytes = Some(r.size);
                    att.mime_type = r.mime.clone();
                    att.missing_reason = Some(reason.clone());
                });
            } else {
                // The original is untouched on disk. Clearing `path` here
                // would sever the only reference to bytes that still exist,
                // turning a transient failure into permanent loss, and
                // `pending_in` (which skips `path == None`) would never
                // retry it — so only the reason changes; everything else
                // (path, digest, size, mime) stays exactly as recorded.
                patch_all_matching(doc, recorded_rel, |att| {
                    att.missing_reason = Some(reason.clone());
                });
            }
            write_conversation_jsonl_to(jsonl, doc)?;
            report.failed += 1;
            Ok(())
        }
        Ok(TranscodeOutcome::Skipped) => {
            if is_heal {
                // The doc currently points at a phantom `-mv` name that
                // nothing will ever produce — the media step declined this
                // file. Repoint it back at the recovered original so a
                // resume does not chase a name that can never exist.
                let r = recovered_original_fields(src)?;
                patch_all_matching(doc, recorded_rel, |att| {
                    att.path = Some(r.rel.clone());
                    att.digest_sha256 = Some(r.digest.clone());
                    att.size_bytes = Some(r.size);
                    att.mime_type = r.mime.clone();
                    att.missing_reason = None;
                });
                write_conversation_jsonl_to(jsonl, doc)?;
            }
            report.skipped += 1;
            Ok(())
        }
        Ok(TranscodeOutcome::Produced) => {
            let produced_len = std::fs::metadata(&marker)
                .with_context(|| format!("stat {}", marker.display()))?
                .len();
            if produced_len > options.asset_max_bytes {
                // Decision 45: skipped, not reverted. Both the derivative and
                // the original go, so nothing survives to point at.
                patch_all_matching(doc, recorded_rel, |att| {
                    att.path = None;
                    att.digest_sha256 = None;
                    att.missing_reason = Some("too_large".to_string());
                    att.size_bytes = Some(produced_len);
                });
                write_conversation_jsonl_to(jsonl, doc)?;
                let _ = std::fs::remove_file(&marker);
                let _ = std::fs::remove_file(src);
                report.too_large += 1;
                return Ok(());
            }
            // Decision 29: read the file on disk. A replayed digest can be
            // stale, and the vault dedupes assets by sha256.
            let digest = media::file_sha256(&marker)?;
            let rel = attachment_rel(&final_path)?;
            let mime = mime_for_rel(&rel);
            patch_all_matching(doc, recorded_rel, |att| {
                att.path = Some(rel.clone());
                att.digest_sha256 = Some(digest.clone());
                att.size_bytes = Some(produced_len);
                att.mime_type = mime.clone();
                att.missing_reason = None;
            });
            write_conversation_jsonl_to(jsonl, doc)?;
            // The `-mv` suffix in `final_derivative_name` makes this
            // structurally impossible, but a future naming change must fail
            // loudly here rather than silently destroy the original before
            // it is committed. `fs::rename` itself replaces an existing
            // destination atomically, so there is nothing to remove first.
            assert_ne!(
                final_path.as_path(),
                src,
                "final derivative name collided with the source"
            );
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

/// Repoint every attachment recorded at `recorded_rel` to `derivative`,
/// which already exists — no transcode needed. The aliasing case: another
/// attachment already converted and deleted the shared original.
fn apply_repoint(
    jsonl: &Path,
    doc: &mut ConversationDocument,
    recorded_rel: &str,
    derivative: &Path,
    report: &mut TranscodeReport,
) -> Result<()> {
    let rel = attachment_rel(derivative)?;
    let digest = media::file_sha256(derivative)?;
    let size = std::fs::metadata(derivative)
        .with_context(|| format!("stat {}", derivative.display()))?
        .len();
    let mime = mime_for_rel(&rel);
    patch_all_matching(doc, recorded_rel, |att| {
        att.path = Some(rel.clone());
        att.digest_sha256 = Some(digest.clone());
        att.size_bytes = Some(size);
        att.mime_type = mime.clone();
        att.missing_reason = None;
    });
    write_conversation_jsonl_to(jsonl, doc)?;
    report.repointed += 1;
    Ok(())
}

/// Mark every attachment recorded at `recorded_rel` `file_missing`: nothing
/// survived an interrupted prior run's crash window closely enough to
/// recover.
fn apply_unrecoverable(
    jsonl: &Path,
    doc: &mut ConversationDocument,
    recorded_rel: &str,
    report: &mut TranscodeReport,
) -> Result<()> {
    patch_all_matching(doc, recorded_rel, |att| {
        att.path = None;
        att.digest_sha256 = None;
        att.missing_reason = Some("file_missing".to_string());
        att.size_bytes = None;
    });
    write_conversation_jsonl_to(jsonl, doc)?;
    report.missing += 1;
    Ok(())
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
        // Needs ffmpeg present and failing on this specific input: after the
        // ffmpeg preflight check, an *absent* ffmpeg now fails the whole
        // pass (see ffmpeg_unavailable_fails_the_whole_pass_up_front) rather
        // than reaching this per-item path.
        if !ffmpeg_available() {
            return;
        }
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
    fn a_convert_failed_attachment_keeps_its_path_and_is_retried_on_resume() {
        if !ffmpeg_available() {
            return;
        }
        // The original is still on disk after a transient ffmpeg failure;
        // clearing `path` would sever the only reference to bytes that still
        // exist and stop `pending_in` from ever retrying it.
        let (dir, jsonl, original) = staged_one("broken.png", b"not a png at all");
        let first = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(first.failed, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        assert_eq!(
            att.path.as_deref(),
            Some("attachments/broken.png"),
            "the path survives a transient failure"
        );
        assert!(original.exists());

        // Resume: pending_in must still see this as work, because the path
        // exists, derivative_name says Some, and the stem carries no -mv.
        let second = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(
            second.failed, 1,
            "a resume must retry a convert_failed file, not skip it"
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

        let err = err.expect_err("a cancel requested before the call must surface as Err");
        assert_eq!(
            err.to_string(),
            "canceled",
            "spelled to match run_attachment_jobs; the Tauri layer string-matches it"
        );
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].path.as_deref(),
            Some("attachments/photo.png"),
            "an untouched attachment still points at its original"
        );
    }

    #[test]
    fn progress_counts_the_work_it_actually_has() {
        // Convert mode still probes for ffmpeg up front (parity with
        // process_attachments_dir) even though a PDF alone needs no
        // transcode, so this needs the tools present to reach that far.
        if !ffmpeg_available() {
            return;
        }
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
    fn a_heal_that_fails_to_transcode_repoints_at_the_original_before_recording_the_failure() {
        if !ffmpeg_available() {
            return;
        }
        // Same crash-window simulation as the other heal tests, but this
        // time the recovered original is garbage ffmpeg will fail on. The
        // Err arm must not simply "keep the path" the way a non-heal
        // failure does: `recorded_rel` here is the phantom -mv name from the
        // crashed run, which will never exist. It must repoint at the
        // recovered original first, then record the failure on it.
        let (dir, jsonl, original) = staged_one("broken.png", b"not a png at all");
        let mut doc = read_conversation_jsonl(&jsonl).unwrap();
        {
            let att = &mut doc.messages[0].attachments[0];
            att.path = Some("attachments/broken-mv.jpg".into());
            att.digest_sha256 = Some("deadbeef".repeat(8));
            att.size_bytes = Some(1234);
            att.mime_type = Some("image/jpeg".into());
        }
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
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

        assert_eq!(report.failed, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        assert_eq!(
            att.path.as_deref(),
            Some("attachments/broken.png"),
            "repointed at the recovered original, not left on the phantom -mv name"
        );
        let reason = att.missing_reason.clone().unwrap();
        assert!(
            reason.starts_with("convert_failed: "),
            "reason must stay inside the closed set: {reason}"
        );
        assert_eq!(
            att.digest_sha256.as_deref(),
            Some(hex_sha256(&std::fs::read(&original).unwrap()).as_str()),
            "digest recomputed from the recovered original, not the stale pre-crash value"
        );
        assert!(
            original.exists(),
            "the recovered original is untouched by a failed transcode"
        );
    }

    #[test]
    fn a_heal_that_the_media_step_skips_repoints_at_the_original_deterministically() {
        if !ffmpeg_available() {
            return;
        }
        // A small mp4 under compress's min_size_bytes returns
        // TranscodeOutcome::Skipped without looking at the video's content
        // at all — compress_video's `ext == "mp4"` branch short-circuits
        // before any ffmpeg probe or encode — so this Skip is deterministic
        // regardless of the installed ffmpeg's version or behaviour, unlike
        // (say) an already-efficient-codec skip, which depends on what that
        // ffmpeg actually reports.
        let (dir, jsonl, original) = staged_one("clip.mp4", b"not really a video, but small");
        let mut doc = read_conversation_jsonl(&jsonl).unwrap();
        {
            let att = &mut doc.messages[0].attachments[0];
            att.path = Some("attachments/clip-mv.mp4".into());
            att.digest_sha256 = Some("deadbeef".repeat(8));
            att.size_bytes = Some(1234);
            att.mime_type = Some("video/mp4".into());
        }
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        assert!(
            original.exists(),
            "the original is still there before the pass runs"
        );
        // Default min_size_bytes is 20 MB; our fixture is a few dozen bytes.
        assert!(CompressOptions::default().min_size_bytes > 1000);

        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Compress, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(report.skipped, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        assert_eq!(
            att.path.as_deref(),
            Some("attachments/clip.mp4"),
            "repointed at the recovered original, not left on the phantom -mv name"
        );
        assert!(att.missing_reason.is_none());
        assert_eq!(
            att.digest_sha256.as_deref(),
            Some(hex_sha256(&std::fs::read(&original).unwrap()).as_str())
        );
        assert!(original.exists(), "a skipped file's original is left alone");
    }

    #[test]
    fn a_crash_that_lost_both_the_marker_and_the_original_is_unrecoverable() {
        // The whole pass still needs ffmpeg present up front (the preflight
        // check runs before any per-attachment classification), even though
        // no transcode is ever attempted for this particular attachment.
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let mut doc = read_conversation_jsonl(&jsonl).unwrap();
        {
            let att = &mut doc.messages[0].attachments[0];
            att.path = Some("attachments/photo-mv.jpg".into());
            // Seed non-None digest/size so the clearing assertions below can
            // actually fail if `set_missing` stops clearing them.
            att.digest_sha256 = Some("cafebabe".repeat(8));
            att.size_bytes = Some(999);
        }
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

    #[test]
    fn two_attachments_in_one_document_sharing_a_path_are_patched_together() {
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        // A second message in the same document, carrying an attachment
        // recorded at the exact same content-addressed path — a legitimate
        // state, not a fixture error.
        let mut doc = read_conversation_jsonl(&jsonl).unwrap();
        let mut second_msg = doc.messages[0].clone();
        second_msg.guid = "second-message-guid".into();
        second_msg.timestamp_unix_ms += 1000;
        doc.messages.push(second_msg);
        doc.finalize_stats();
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();

        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(
            report.converted, 1,
            "one physical file, one transcode, however many attachments reference it"
        );
        assert!(!original.exists());
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(doc.messages.len(), 2);
        for msg in &doc.messages {
            let att = &msg.attachments[0];
            assert_eq!(
                att.path.as_deref(),
                Some("attachments/photo-mv.jpg"),
                "every attachment sharing the path gets patched"
            );
            assert!(att.digest_sha256.is_some());
        }
    }

    #[test]
    fn two_documents_sharing_one_original_both_end_pointing_at_the_committed_derivative() {
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl_a, original) = staged_one("shared.png", &test_png_bytes());

        // A second, independent conversation staged in the same folder whose
        // attachment happens to record the identical path — two different
        // chats that received the same bytes.
        let mut doc_b = message_ir::testutil::sample_document("second conversation, same photo");
        doc_b.conversation.chat_identifier = "+15555550199".into();
        doc_b.messages[0].guid = "doc-b-guid".into();
        doc_b.messages[0].attachments = vec![IrAttachment {
            path: Some("attachments/shared.png".into()),
            original_name: Some("shared.png".into()),
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: Some(test_png_bytes().len() as u64),
            missing_reason: None,
            bytes: None,
        }];
        doc_b.finalize_stats();
        let jsonl_b = dir.path().join(format!("{}.jsonl", doc_b.filename_stem()));
        write_conversation_jsonl_to(&jsonl_b, &doc_b).unwrap();

        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(report.converted, 1, "one physical file is transcoded once");
        assert_eq!(
            report.repointed, 1,
            "the second document is repointed, not re-transcoded"
        );
        assert!(!original.exists());

        let final_doc_a = read_conversation_jsonl(&jsonl_a).unwrap();
        let final_doc_b = read_conversation_jsonl(&jsonl_b).unwrap();
        let att_a = &final_doc_a.messages[0].attachments[0];
        let att_b = &final_doc_b.messages[0].attachments[0];
        assert_eq!(att_a.path.as_deref(), Some("attachments/shared-mv.jpg"));
        assert_eq!(att_b.path.as_deref(), Some("attachments/shared-mv.jpg"));
        assert!(att_a.digest_sha256.is_some());
        assert_eq!(
            att_a.digest_sha256, att_b.digest_sha256,
            "both documents recompute the same digest from the same on-disk derivative"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_write_failure_leaves_the_final_name_uncommitted_and_the_original_untouched() {
        if !ffmpeg_available() {
            return;
        }
        // The headline "patched before the final name exists" test only
        // checks terminal state, which would pass even if the patch and the
        // rename were swapped. This makes the ordering falsifiable: force
        // the conversation-file write to fail (a read-only staging dir, so
        // `write_conversation_jsonl_to`'s `.tmp` sibling can't be created)
        // after the transcode has already produced a derivative, and assert
        // the final name was never created and the original is untouched.
        use std::os::unix::fs::PermissionsExt;
        let (dir, _jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(dir.path(), perms).unwrap();

        let result = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |_| {},
        );

        // Restore before any assertion can panic, so the TempDir can still
        // clean itself up.
        let mut restore = std::fs::metadata(dir.path()).unwrap().permissions();
        restore.set_mode(0o755);
        std::fs::set_permissions(dir.path(), restore).unwrap();

        assert!(
            result.is_err(),
            "the conversation-file write failure must surface, not be swallowed"
        );
        assert!(
            !dir.path().join("attachments/photo-mv.jpg").exists(),
            "the final name must never exist without a committed patch"
        );
        assert!(
            original.exists(),
            "the original is untouched when the patch never committed"
        );
    }

    #[test]
    fn the_committed_suffix_guard_excludes_an_already_final_video_from_pending() {
        // No ffmpeg needed: pending_in decides this from names alone.
        // media::derivative_name always answers Some("…mp4") for a video in
        // either mode (it cannot see CompressOptions), so without the -mv
        // exclusion a committed video derivative would look pending forever
        // and get re-degraded on every resume.
        let (dir, jsonl, _original) = staged_one("clip-mv.mp4", b"");
        let doc = read_conversation_jsonl(&jsonl).unwrap();

        let work = pending_in(dir.path(), &doc, MediaMode::Convert).unwrap();

        assert!(
            work.is_empty(),
            "a committed -mv name must never re-enter the pending list"
        );
    }
}
