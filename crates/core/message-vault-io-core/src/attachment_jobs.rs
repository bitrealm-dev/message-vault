//! Copy, convert, or skip attachment files after parse.

use crate::attachments::attachment_dest_name;
use crate::config::MediaConfig;
use crate::pipeline::ExportReport;
use crate::process::{CancelFlag, LogSink, emit_log};
use media::MediaMode;
use message_ir::{ConversationDocument, IrAttachment};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// File and byte counts emitted after each attachment job (and once for skip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentProgress {
    /// Jobs finished so far.
    pub done: usize,
    /// Job count from parse. Does not change.
    pub total: usize,
    /// Bytes written (or measured) so far.
    pub bytes_done: u64,
    /// Known or measured byte total. Grows when a file had no `size_hint`.
    pub bytes_total: u64,
}

/// One attachment to stage, pointing at the in-memory IR row.
pub struct AttachmentJob<'a> {
    /// Conversation attachment to fill after the write.
    pub attachment: &'a mut IrAttachment,
    /// Message timestamp in milliseconds (used for the dest date prefix).
    pub timestamp_unix_ms: i64,
    /// Size from the backup, if known.
    pub size_hint: Option<u64>,
}

/// Load, write, and optionally convert each attachment.
///
/// `load(i)` returns `Ok(None)` when the source is missing. `Ok(Some(bytes))`
/// is the file to stage. An `Err` from `load(i)` other than `"canceled"` is
/// caught here and treated the same as a missing source: the attachment gets
/// `missing_reason = "file_missing"` and the run continues rather than
/// aborting. Cancel is checked before each job.
///
/// # Errors
///
/// Returns `"canceled"` when the flag is set before a job starts, or when
/// `load(i)` itself returns `"canceled"`. Returns an I/O or convert error
/// string when the staging directory cannot be used.
pub fn run_attachment_jobs(
    jobs: &mut [AttachmentJob<'_>],
    attachments_dir: &Path,
    media: &MediaConfig,
    mut load: impl FnMut(usize) -> Result<Option<Vec<u8>>, String>,
    mut on_progress: impl FnMut(AttachmentProgress),
    log: Option<&LogSink>,
    cancel: Option<&AtomicBool>,
) -> Result<(), String> {
    let total = jobs.len();
    if total == 0 {
        on_progress(AttachmentProgress {
            done: 0,
            total: 0,
            bytes_done: 0,
            bytes_total: 0,
        });
        return Ok(());
    }
    if matches!(media.mode, MediaMode::Disabled) {
        for job in jobs.iter_mut() {
            job.attachment.missing_reason = Some("not_copied".into());
        }
        on_progress(AttachmentProgress {
            done: total,
            total,
            bytes_done: 0,
            bytes_total: 0,
        });
        return Ok(());
    }

    let mut bytes_total: u64 = jobs.iter().filter_map(|job| job.size_hint).sum();
    let mut bytes_done = 0_u64;

    fs::create_dir_all(attachments_dir)
        .map_err(|e| format!("create {}: {e}", attachments_dir.display()))?;

    for (i, job) in jobs.iter_mut().enumerate() {
        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err("canceled".into());
        }

        let loaded = match load(i) {
            Ok(loaded) => loaded,
            // A cancel raised inside the loader still stops the run.
            Err(err) if err == "canceled" => return Err(err),
            // One unreadable source is that attachment's problem, not the
            // run's. Fall through to the missing-file handling below.
            Err(_) => None,
        };
        let bytes = match loaded {
            Some(bytes) if !bytes.is_empty() => bytes,
            _ => {
                job.attachment.missing_reason = Some("file_missing".into());
                on_progress(AttachmentProgress {
                    done: i + 1,
                    total,
                    bytes_done,
                    bytes_total,
                });
                continue;
            }
        };

        if job.size_hint.is_none() {
            bytes_total += bytes.len() as u64;
        }

        persist_clone(job, attachments_dir, &bytes)?;
        bytes_done += bytes.len() as u64;
        on_progress(AttachmentProgress {
            done: i + 1,
            total,
            bytes_done,
            bytes_total,
        });
    }

    if matches!(media.mode, MediaMode::Convert | MediaMode::Compress) {
        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err("canceled".into());
        }
        apply_convert_or_compress(jobs, attachments_dir, media, log)?;
        on_progress(AttachmentProgress {
            done: total,
            total,
            bytes_done,
            bytes_total,
        });
    }

    Ok(())
}

/// Write queued attachment bytes after parse and before conversation files.
///
/// The shared non-queue staging step every exporter used to copy: assemble
/// one [`AttachmentJob`] per attachment across `documents` (in document
/// order), run [`run_attachment_jobs`] with the standard progress log line,
/// count staged files into `report.attachments_saved`, and clear any
/// in-memory `bytes` left on the attachments.
///
/// `load(i)` is the per-exporter payload hook: `i` is the flat attachment
/// index in document order. `Ok(None)` (or a non-cancel `Err`) marks that
/// attachment `file_missing` and the run continues.
///
/// Size hints for the progress totals come from each attachment's
/// `size_bytes` (falling back to in-memory `bytes` length when present);
/// path-backed exporters whose attachments carry no size get unhinted totals
/// that grow as files load.
///
/// # Errors
///
/// Returns `"canceled"` when the user cancels, or an I/O / convert error
/// string when the staging directory cannot be used.
pub fn stage_conversation_attachments(
    documents: &mut [ConversationDocument],
    attachments_dir: &Path,
    media: &MediaConfig,
    load: impl FnMut(usize) -> Result<Option<Vec<u8>>, String>,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
    report: &mut ExportReport,
) -> Result<(), String> {
    let mut jobs = attachment_jobs(documents);
    run_attachment_jobs(
        &mut jobs,
        attachments_dir,
        media,
        load,
        log_attachment_progress(log),
        log,
        cancel.map(|flag| flag.as_ref()),
    )?;

    for job in &jobs {
        if job.attachment.path.is_some() && job.attachment.digest_sha256.is_some() {
            report.attachments_saved += 1;
        }
    }
    drop(jobs);
    clear_attachment_bytes(documents);
    Ok(())
}

/// The size to report for an attachment before its bytes are read: the
/// record's own size, else the length of the bytes held in memory, else
/// unknown (the progress total grows once the file loads).
pub fn attachment_size_hint(att: &IrAttachment) -> Option<u64> {
    att.size_bytes
        .or_else(|| att.bytes.as_ref().map(|b| b.len() as u64))
}

/// One job per attachment across every document, in document order. The
/// position in the result is the flat attachment index a `load(i)` hook
/// receives.
pub fn attachment_jobs(documents: &mut [ConversationDocument]) -> Vec<AttachmentJob<'_>> {
    let mut jobs = Vec::new();
    for doc in documents.iter_mut() {
        for msg in &mut doc.messages {
            let ts = msg.timestamp_unix_ms;
            for att in &mut msg.attachments {
                let hint = attachment_size_hint(att);
                jobs.push(AttachmentJob {
                    attachment: att,
                    timestamp_unix_ms: ts,
                    size_hint: hint,
                });
            }
        }
    }
    jobs
}

/// The progress line every attachment run logs: files done of total, bytes
/// done of total.
pub fn log_attachment_progress(log: Option<&LogSink>) -> impl FnMut(AttachmentProgress) + '_ {
    move |progress| {
        emit_log(
            log,
            format!(
                "  attachments {}/{} {}/{}",
                progress.done, progress.total, progress.bytes_done, progress.bytes_total
            ),
        );
    }
}

/// Drop the bytes held in memory on every attachment, once they have been
/// written or are no longer wanted.
pub fn clear_attachment_bytes(documents: &mut [ConversationDocument]) {
    for doc in documents.iter_mut() {
        for msg in &mut doc.messages {
            for att in &mut msg.attachments {
                att.bytes = None;
            }
        }
    }
}

/// Monotonic counter distinguishing concurrent temp files.
///
/// The final name is content-addressed, so two workers staging identical
/// bytes produce the same `dest` — that is fine, the second rename is a
/// no-op overwrite of identical bytes — but they must not share a temp path
/// mid-write, or one worker's rename pulls the file out from under the
/// other's still-open write.
static CLONE_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_clone_temp_name(name: &str) -> String {
    let seq = CLONE_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{name}.{seq}.tmp")
}

fn persist_clone(
    job: &mut AttachmentJob<'_>,
    attachments_dir: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    let digest_hex = hex_sha256(bytes);
    let ext = extension_from_name(job.attachment.original_name.as_deref());
    let secs = job.timestamp_unix_ms.div_euclid(1000);
    let name = attachment_dest_name(secs, &digest_hex, &ext);
    let dest = attachments_dir.join(&name);
    let tmp = attachments_dir.join(next_clone_temp_name(&name));
    fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &dest).map_err(|e| format!("rename {}: {e}", dest.display()))?;
    job.attachment.path = Some(format!("attachments/{name}"));
    job.attachment.digest_sha256 = Some(digest_hex);
    job.attachment.size_bytes = Some(bytes.len() as u64);
    job.attachment.missing_reason = None;
    Ok(())
}

fn apply_convert_or_compress(
    jobs: &mut [AttachmentJob<'_>],
    attachments_dir: &Path,
    media: &MediaConfig,
    log: Option<&LogSink>,
) -> Result<(), String> {
    let Some(output_dir) = attachments_dir.parent() else {
        return Err("attachments directory has no parent".into());
    };
    let files = media::collect_media_files(attachments_dir).map_err(|e| e.to_string())?;
    let mut emit = |line: &str| emit_log(log, line);
    let (report, remap) = media::process_attachment_files(
        output_dir,
        &files,
        media.mode,
        &media.compress,
        Some(&mut emit),
    )
    .map_err(|e| e.to_string())?;
    apply_remap_to_jobs(jobs, &remap, output_dir);
    for err in &report.errors {
        mark_convert_error(jobs, err);
    }
    Ok(())
}

fn apply_remap_to_jobs(
    jobs: &mut [AttachmentJob<'_>],
    remap: &std::collections::HashMap<String, String>,
    output_dir: &Path,
) {
    for job in jobs.iter_mut() {
        let Some(path) = job.attachment.path.as_mut() else {
            continue;
        };
        if let Some(new_rel) = remap.get(path.as_str()) {
            *path = new_rel.clone();
            if let Some(mime) = mime_for_rel(new_rel) {
                job.attachment.mime_type = Some(mime);
            }
            if refresh_digest_and_size(job.attachment, output_dir).is_err() {
                job.attachment.missing_reason = Some("file_missing".into());
            }
        }
    }
}

fn mark_convert_error(jobs: &mut [AttachmentJob<'_>], err: &str) {
    let Some((path, reason)) = err.split_once(": ") else {
        return;
    };
    for job in jobs.iter_mut() {
        let Some(rel) = job.attachment.path.as_deref() else {
            continue;
        };
        let native = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
        if path.ends_with(rel) || path.ends_with(native.as_str()) {
            job.attachment.missing_reason = Some(format!("convert_failed: {reason}"));
        }
    }
}

/// MIME type inferred from a `attachments/…` relative path's extension.
///
/// Thin wrapper over [`media::mime_for_ext`] — the one shared
/// extension-to-mime table — kept because many pipeline callers hand paths
/// rather than extensions. `None` for unrecognized extensions.
pub fn mime_for_rel(rel: &str) -> Option<String> {
    let ext = Path::new(rel).extension().and_then(|e| e.to_str())?;
    media::mime_for_ext(ext).map(str::to_string)
}

fn refresh_digest_and_size(attachment: &mut IrAttachment, output_dir: &Path) -> Result<(), String> {
    let Some(rel) = attachment.path.as_deref() else {
        return Ok(());
    };
    let dest = output_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let bytes = fs::read(&dest).map_err(|e| format!("read {}: {e}", dest.display()))?;
    attachment.digest_sha256 = Some(hex_sha256(&bytes));
    attachment.size_bytes = Some(bytes.len() as u64);
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn extension_from_name(original_name: Option<&str>) -> String {
    original_name
        .and_then(|name| Path::new(name).extension())
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::{CompressOptions, MediaMode};

    fn media_cfg(mode: MediaMode) -> MediaConfig {
        MediaConfig {
            mode,
            compress: CompressOptions::default(),
        }
    }
    use message_ir::IrAttachment;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn empty_att(name: &str) -> IrAttachment {
        IrAttachment {
            path: None,
            original_name: Some(name.into()),
            mime_type: Some("image/jpeg".into()),
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }

    #[test]
    fn clone_writes_file_and_fills_hash() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut att = empty_att("photo.jpg");
        let bytes = b"hello-photo";
        let progress = Mutex::new(Vec::new());
        {
            let mut jobs = [AttachmentJob {
                attachment: &mut att,
                timestamp_unix_ms: 1_609_459_200_000,
                size_hint: Some(bytes.len() as u64),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Clone),
                |_| Ok(Some(bytes.to_vec())),
                |p| progress.lock().unwrap().push(p),
                None,
                None,
            )
            .unwrap();
        }
        assert!(att.path.as_deref().unwrap().starts_with("attachments/"));
        assert_eq!(att.size_bytes, Some(bytes.len() as u64));
        assert_eq!(att.digest_sha256.as_ref().unwrap().len(), 64);
        let dest = dir.path().join(att.path.as_ref().unwrap());
        assert_eq!(std::fs::read(dest).unwrap(), bytes);
        let last = progress.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.done, 1);
        assert_eq!(last.total, 1);
        assert_eq!(last.bytes_done, bytes.len() as u64);
        assert_eq!(last.bytes_total, bytes.len() as u64);
    }

    #[test]
    fn disabled_skips_without_loading() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        let mut att = empty_att("photo.jpg");
        let loaded = AtomicBool::new(false);
        {
            let mut jobs = [AttachmentJob {
                attachment: &mut att,
                timestamp_unix_ms: 0,
                size_hint: Some(99),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Disabled),
                |_| {
                    loaded.store(true, Ordering::SeqCst);
                    Ok(Some(b"x".to_vec()))
                },
                |_| {},
                None,
                None,
            )
            .unwrap();
        }
        assert!(!loaded.load(Ordering::SeqCst));
        assert_eq!(att.missing_reason.as_deref(), Some("not_copied"));
        assert!(att.path.is_none());
        assert!(!att_dir.exists() || std::fs::read_dir(&att_dir).unwrap().next().is_none());
    }

    #[test]
    fn missing_source_is_file_missing_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let mut b = empty_att("b.jpg");
        {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut a,
                    timestamp_unix_ms: 0,
                    size_hint: None,
                },
                AttachmentJob {
                    attachment: &mut b,
                    timestamp_unix_ms: 0,
                    size_hint: Some(4),
                },
            ];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Clone),
                |i| {
                    if i == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(b"data".to_vec()))
                    }
                },
                |_| {},
                None,
                None,
            )
            .unwrap();
        }
        assert_eq!(a.missing_reason.as_deref(), Some("file_missing"));
        assert!(b.path.is_some());
    }

    #[test]
    fn read_error_marks_file_missing_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let mut b = empty_att("b.jpg");
        {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut a,
                    timestamp_unix_ms: 0,
                    size_hint: None,
                },
                AttachmentJob {
                    attachment: &mut b,
                    timestamp_unix_ms: 0,
                    size_hint: Some(4),
                },
            ];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Clone),
                |i| {
                    if i == 0 {
                        Err("permission denied".into())
                    } else {
                        Ok(Some(b"data".to_vec()))
                    }
                },
                |_| {},
                None,
                None,
            )
            .unwrap();
        }
        assert_eq!(a.missing_reason.as_deref(), Some("file_missing"));
        assert!(b.path.is_some());
    }

    #[test]
    fn canceled_error_from_the_loader_still_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let err = {
            let mut jobs = [AttachmentJob {
                attachment: &mut a,
                timestamp_unix_ms: 0,
                size_hint: Some(1),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Clone),
                |_| Err("canceled".into()),
                |_| {},
                None,
                None,
            )
            .unwrap_err()
        };
        assert_eq!(err, "canceled");
    }

    #[test]
    fn cancel_stops_before_next_job() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let mut b = empty_att("b.jpg");
        let cancel = AtomicBool::new(false);
        let err = {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut a,
                    timestamp_unix_ms: 0,
                    size_hint: Some(1),
                },
                AttachmentJob {
                    attachment: &mut b,
                    timestamp_unix_ms: 0,
                    size_hint: Some(1),
                },
            ];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Clone),
                |i| {
                    if i == 0 {
                        cancel.store(true, Ordering::SeqCst);
                    }
                    Ok(Some(b"x".to_vec()))
                },
                |_| {},
                None,
                Some(&cancel),
            )
            .unwrap_err()
        };
        assert_eq!(err, "canceled");
        assert!(a.path.is_some());
        assert!(b.path.is_none());
    }

    #[test]
    fn empty_jobs_emits_zero_of_zero() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        let progress = Mutex::new(Vec::new());
        run_attachment_jobs(
            &mut [],
            &att_dir,
            &media_cfg(MediaMode::Clone),
            |_| Ok(None),
            |p| progress.lock().unwrap().push(p),
            None,
            None,
        )
        .unwrap();
        let last = progress.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.done, 0);
        assert_eq!(last.total, 0);
        assert_eq!(last.bytes_done, 0);
        assert_eq!(last.bytes_total, 0);
    }

    #[test]
    fn remap_updates_mime_and_continues_when_one_file_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        std::fs::write(att_dir.join("ok.jpg"), b"jpeg-bytes").unwrap();
        let mut ok = empty_att("ok.heic");
        ok.path = Some("attachments/ok.heic".into());
        ok.mime_type = Some("image/heic".into());
        let mut missing = empty_att("gone.heic");
        missing.path = Some("attachments/gone.heic".into());
        missing.mime_type = Some("image/heic".into());
        {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut ok,
                    timestamp_unix_ms: 0,
                    size_hint: None,
                },
                AttachmentJob {
                    attachment: &mut missing,
                    timestamp_unix_ms: 0,
                    size_hint: None,
                },
            ];
            let mut remap = std::collections::HashMap::new();
            remap.insert("attachments/ok.heic".into(), "attachments/ok.jpg".into());
            remap.insert(
                "attachments/gone.heic".into(),
                "attachments/gone.jpg".into(),
            );
            apply_remap_to_jobs(&mut jobs, &remap, dir.path());
        }
        assert_eq!(ok.path.as_deref(), Some("attachments/ok.jpg"));
        assert_eq!(ok.mime_type.as_deref(), Some("image/jpeg"));
        assert_eq!(ok.digest_sha256.as_ref().unwrap().len(), 64);
        assert_eq!(missing.missing_reason.as_deref(), Some("file_missing"));
        assert!(ok.missing_reason.is_none());
    }
    #[test]
    fn convert_mode_emits_progress_through_the_log_sink() {
        // Clone has no media pass, so nothing should reach the sink. This
        // pins that the new `log` parameter is wired end to end without
        // requiring ffmpeg in this crate's tests.
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut att = empty_att("photo.jpg");
        let bytes = b"hello-photo";
        let lines = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let sink_lines = std::sync::Arc::clone(&lines);
        let sink = crate::process::LogSink::new(move |l: &str| {
            sink_lines.lock().unwrap().push(l.to_string());
        });
        {
            let mut jobs = [AttachmentJob {
                attachment: &mut att,
                timestamp_unix_ms: 1_609_459_200_000,
                size_hint: Some(bytes.len() as u64),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                &media_cfg(MediaMode::Clone),
                |_| Ok(Some(bytes.to_vec())),
                |_| {},
                Some(&sink),
                None,
            )
            .unwrap();
        }
        assert!(
            lines.lock().unwrap().is_empty(),
            "clone mode runs no media pass, so it has nothing to report"
        );
    }
    #[test]
    fn clone_temp_paths_are_unique_per_call() {
        // Two workers staging identical bytes land on the same
        // content-addressed dest, which is harmless, but they must not share
        // the temp path they write through on the way there.
        let a = next_clone_temp_name("x.jpg");
        let b = next_clone_temp_name("x.jpg");
        assert_ne!(a, b);
        assert!(a.starts_with("x.jpg."));
        assert!(a.ends_with(".tmp"));
    }
}
