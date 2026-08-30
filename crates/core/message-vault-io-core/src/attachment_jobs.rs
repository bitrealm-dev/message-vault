//! Copy, convert, or skip attachment files after parse.

use crate::attachments::attachment_dest_name;
use media::{CompressOptions, MediaMode};
use message_ir::IrAttachment;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

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
    mode: MediaMode,
    compress: &CompressOptions,
    mut load: impl FnMut(usize) -> Result<Option<Vec<u8>>, String>,
    mut on_progress: impl FnMut(AttachmentProgress),
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
    if matches!(mode, MediaMode::Disabled) {
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

    if matches!(mode, MediaMode::Convert | MediaMode::Compress) {
        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err("canceled".into());
        }
        apply_convert_or_compress(jobs, attachments_dir, mode, compress)?;
        on_progress(AttachmentProgress {
            done: total,
            total,
            bytes_done,
            bytes_total,
        });
    }

    Ok(())
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
    let tmp = attachments_dir.join(format!("{name}.tmp"));
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
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<(), String> {
    let Some(output_dir) = attachments_dir.parent() else {
        return Err("attachments directory has no parent".into());
    };
    let (report, remap) =
        media::process_attachments_dir(output_dir, mode, compress).map_err(|e| e.to_string())?;
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
/// Covers the formats the media convert/compress step can produce or leave in
/// place; `None` for anything else. Shared so a second, drifting
/// extension-to-mime table doesn't grow up beside this one.
pub fn mime_for_rel(rel: &str) -> Option<String> {
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "mp4" | "m4v" => "video/mp4",
            "mov" => "video/quicktime",
            "mp3" => "audio/mpeg",
            "m4a" => "audio/mp4",
            _ => return None,
        }
        .into(),
    )
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
                MediaMode::Clone,
                &CompressOptions::default(),
                |_| Ok(Some(bytes.to_vec())),
                |p| progress.lock().unwrap().push(p),
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
                MediaMode::Disabled,
                &CompressOptions::default(),
                |_| {
                    loaded.store(true, Ordering::SeqCst);
                    Ok(Some(b"x".to_vec()))
                },
                |_| {},
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
                MediaMode::Clone,
                &CompressOptions::default(),
                |i| {
                    if i == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(b"data".to_vec()))
                    }
                },
                |_| {},
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
                MediaMode::Clone,
                &CompressOptions::default(),
                |i| {
                    if i == 0 {
                        Err("permission denied".into())
                    } else {
                        Ok(Some(b"data".to_vec()))
                    }
                },
                |_| {},
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
                MediaMode::Clone,
                &CompressOptions::default(),
                |_| Err("canceled".into()),
                |_| {},
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
                MediaMode::Clone,
                &CompressOptions::default(),
                |i| {
                    if i == 0 {
                        cancel.store(true, Ordering::SeqCst);
                    }
                    Ok(Some(b"x".to_vec()))
                },
                |_| {},
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
            MediaMode::Clone,
            &CompressOptions::default(),
            |_| Ok(None),
            |p| progress.lock().unwrap().push(p),
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
}
