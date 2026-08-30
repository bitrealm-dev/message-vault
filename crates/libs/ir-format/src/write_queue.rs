//! Drain a queue of conversations onto disk, one conversation at a time.
//!
//! Parse finishes before anything is written: every exporter buffers its
//! documents and collects attachment sources in one pass, then hands the
//! result here as a queue of [`ConversationUnit`]s. A worker writes a unit's
//! attachments first and its conversation file last, so a conversation file
//! on disk means everything it references is on disk too. That invariant is
//! what makes an interrupted write resumable: a resumed run skips any unit
//! whose conversation file it already finds.
//!
//! Writers never transcode. Convert and compress stage the originals here and
//! run afterwards as their own resumable pass.
//!
//! Only non-obfuscated JSONL exports are routed here. Obfuscation is stateful
//! across documents and the other formats merge or embed at finish, so those
//! keep the `FormatSink` path.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use media::{CompressOptions, MediaMode};
use message_ir::ConversationDocument;
use message_vault_io_core::{AttachmentJob, LogSink, OutputFormat, emit_log, run_attachment_jobs};

use crate::write::write_format;

/// Where a unit's attachment bytes come from at write time.
#[derive(Debug, Default)]
pub enum AttachmentSource {
    /// Read this file when the attachment is written. Worker-safe: a plain
    /// `fs::read`, no shared handle.
    Path(PathBuf),
    /// Bytes the exporter already holds (SBR blobs, handwriting SVG).
    Bytes(Vec<u8>),
    /// Nothing to read; the attachment becomes `file_missing` under a mode
    /// that copies files.
    #[default]
    Missing,
}

/// One attachment of a unit, pinned to its place in the document.
#[derive(Debug)]
pub struct UnitAttachment {
    /// Index into `doc.messages`.
    pub message_index: usize,
    /// Index into that message's `attachments`.
    pub attachment_index: usize,
    /// Where the bytes come from.
    pub source: AttachmentSource,
    /// Message timestamp, which dates the staged filename.
    pub timestamp_unix_ms: i64,
    /// Size from the backup when known; byte totals grow as unhinted files load.
    pub size_hint: Option<u64>,
}

/// One conversation and everything it references: the queue's unit of work.
#[derive(Debug)]
pub struct ConversationUnit {
    /// The conversation to write.
    pub doc: ConversationDocument,
    /// Its attachments, in message order.
    pub attachments: Vec<UnitAttachment>,
}

impl ConversationUnit {
    /// Pair every attachment in `doc` with a source and a size hint.
    ///
    /// The closure sees each attachment in message order and receives it as
    /// `&mut`, so an exporter carrying bytes on the document can move them
    /// out with `att.bytes.take()` instead of copying them.
    pub fn from_doc(
        mut doc: ConversationDocument,
        mut source_for: impl FnMut(
            usize,
            &mut message_ir::IrAttachment,
        ) -> (AttachmentSource, Option<u64>),
    ) -> Self {
        let mut attachments = Vec::new();
        let mut flat = 0usize;
        for (message_index, msg) in doc.messages.iter_mut().enumerate() {
            let timestamp_unix_ms = msg.timestamp_unix_ms;
            for (attachment_index, att) in msg.attachments.iter_mut().enumerate() {
                let (source, size_hint) = source_for(flat, att);
                attachments.push(UnitAttachment {
                    message_index,
                    attachment_index,
                    source,
                    timestamp_unix_ms,
                    size_hint,
                });
                flat += 1;
            }
        }
        Self { doc, attachments }
    }
}

/// How a drain stages files.
#[derive(Debug, Clone)]
pub struct WriteQueueOptions {
    /// The mode the user asked for. Convert and compress stage originals here
    /// and transcode afterwards — writers do not transcode.
    pub media: MediaMode,
    /// Compress settings, used by the post-pass.
    pub compress: CompressOptions,
    /// Skip units whose conversation file is already on disk.
    pub resume: bool,
    /// 0 picks a count from the machine. The sequential drain ignores it.
    pub writer_count: usize,
}

/// What a drain did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WriteQueueReport {
    /// Conversation files written by this run.
    pub conversations_written: usize,
    /// Conversation files a resumed run found already written.
    pub conversations_skipped: usize,
    /// Attachment records staged with a path and a digest, duplicates included.
    pub attachments_saved: usize,
    /// Filled by the convert/compress post-pass; default otherwise.
    pub media: media::MediaReport,
}

/// Read one attachment source.
///
/// `Bytes` are moved out of the source rather than copied — every source is
/// loaded at most once, so taking them is safe and spares a full copy of the
/// payload. `Missing` reads as an absent file, which the staging step turns
/// into `file_missing`.
///
/// # Errors
///
/// Returns the read error when a `Path` source cannot be read.
pub fn load_attachment_source(source: &mut AttachmentSource) -> Result<Option<Vec<u8>>, String> {
    match source {
        AttachmentSource::Path(path) => fs::read(&*path)
            .map(Some)
            .map_err(|e| format!("read {}: {e}", path.display())),
        AttachmentSource::Bytes(bytes) => Ok(Some(std::mem::take(bytes))),
        AttachmentSource::Missing => Ok(None),
    }
}

/// Running totals a drain reports after every attachment.
struct DrainProgress {
    done: usize,
    total: usize,
    bytes_done: u64,
    bytes_total: u64,
}

/// What one unit did.
struct UnitOutcome {
    written: bool,
    attachments_saved: usize,
    /// Attachments this unit accounts for, staged or skipped.
    attachment_count: usize,
    /// Bytes staged, and the byte total this unit turned out to need beyond
    /// its hints.
    bytes_done: u64,
    bytes_total: u64,
}

/// Drain `units` with a caller-supplied loader.
///
/// Exporters whose attachment loader cannot cross threads — an encrypted iOS
/// backup holds a SQLite connection that is not `Sync` — use this and get one
/// writer. Everyone else wants `drain_write_queue`.
///
/// # Errors
///
/// Returns the first unit error, which stops the drain. A cancel surfaces as
/// `"canceled"`.
pub fn drain_write_queue_with_loader(
    output_dir: &Path,
    units: Vec<ConversationUnit>,
    options: &WriteQueueOptions,
    load: &mut dyn FnMut(&mut AttachmentSource) -> Result<Option<Vec<u8>>, String>,
    log: Option<&LogSink>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<WriteQueueReport> {
    let attachments_dir = output_dir.join("attachments");
    let mut report = WriteQueueReport::default();

    let total: usize = units.iter().map(|u| u.attachments.len()).sum();
    let mut progress = DrainProgress {
        done: 0,
        total,
        bytes_done: 0,
        bytes_total: units
            .iter()
            .flat_map(|u| u.attachments.iter())
            .filter_map(|a| a.size_hint)
            .sum(),
    };

    announce_start(log, units.len());

    for unit in units {
        let outcome = write_one_unit(
            output_dir,
            &attachments_dir,
            unit,
            options,
            load,
            &mut |done, unit_bytes_done, unit_bytes_total, base| {
                let line = format!(
                    "  attachments {}/{} {}/{}",
                    base.done + done,
                    base.total,
                    base.bytes_done + unit_bytes_done,
                    base.bytes_total + unit_bytes_total,
                );
                emit_log(log, line);
            },
            &progress,
            cancel,
        )?;

        apply_outcome(&mut progress, &mut report, &outcome);
    }

    announce_finish(log, &report, options.resume);
    Ok(report)
}

fn announce_start(log: Option<&LogSink>, units: usize) {
    emit_log(log, "");
    emit_log(log, format!("Preparing {units} conversation file(s)..."));
}

fn announce_finish(log: Option<&LogSink>, report: &WriteQueueReport, resume: bool) {
    emit_log(
        log,
        format!(
            "Prepared {} conversation file(s)",
            report.conversations_written
        ),
    );
    if resume && report.conversations_skipped > 0 {
        emit_log(
            log,
            format!(
                "Skipped {} already staged conversation(s)",
                report.conversations_skipped
            ),
        );
    }
}

fn apply_outcome(
    progress: &mut DrainProgress,
    report: &mut WriteQueueReport,
    outcome: &UnitOutcome,
) {
    progress.done += outcome.attachment_count;
    progress.bytes_done += outcome.bytes_done;
    progress.bytes_total += outcome.bytes_total;
    report.attachments_saved += outcome.attachments_saved;
    if outcome.written {
        report.conversations_written += 1;
    } else {
        report.conversations_skipped += 1;
    }
}

/// Stage one conversation's attachments, then write the conversation file.
///
/// The order is the engine's whole contract: the conversation file lands last,
/// so its presence on disk vouches for everything it points at.
fn write_one_unit(
    output_dir: &Path,
    attachments_dir: &Path,
    unit: ConversationUnit,
    options: &WriteQueueOptions,
    load: &mut dyn FnMut(&mut AttachmentSource) -> Result<Option<Vec<u8>>, String>,
    on_progress: &mut dyn FnMut(usize, u64, u64, &DrainProgress),
    base: &DrainProgress,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<UnitOutcome> {
    use std::sync::atomic::Ordering;
    if cancel.is_some_and(|f| f.load(Ordering::SeqCst)) {
        anyhow::bail!("canceled");
    }

    let ConversationUnit {
        mut doc,
        attachments,
    } = unit;
    let attachment_count = attachments.len();
    let hint_sum: u64 = attachments.iter().filter_map(|a| a.size_hint).sum();

    let path = output_dir.join(format!("{}.jsonl", doc.filename_stem()));
    if options.resume && path.is_file() {
        // Already written by an earlier run, attachments and all. Count its
        // attachments as done — progress describes the whole import, not just
        // this run's share of it — and load nothing.
        on_progress(attachment_count, 0, 0, base);
        return Ok(UnitOutcome {
            written: false,
            attachments_saved: 0,
            attachment_count,
            bytes_done: 0,
            bytes_total: 0,
        });
    }

    // Writers copy originals; convert and compress run later as their own pass.
    let stage_mode = match options.media {
        MediaMode::Disabled => MediaMode::Disabled,
        _ => MediaMode::Clone,
    };

    let timestamps: Vec<i64> = doc.messages.iter().map(|m| m.timestamp_unix_ms).collect();
    let mut slots: HashMap<(usize, usize), UnitAttachment> = attachments
        .into_iter()
        .map(|a| ((a.message_index, a.attachment_index), a))
        .collect();

    let mut sources: Vec<AttachmentSource> = Vec::with_capacity(attachment_count);
    let mut jobs: Vec<AttachmentJob<'_>> = Vec::new();
    for (message_index, msg) in doc.messages.iter_mut().enumerate() {
        let fallback_ts = timestamps.get(message_index).copied().unwrap_or(0);
        for (attachment_index, att) in msg.attachments.iter_mut().enumerate() {
            let slot = slots.remove(&(message_index, attachment_index));
            let (source, size_hint, timestamp_unix_ms) = match slot {
                Some(a) => (a.source, a.size_hint, a.timestamp_unix_ms),
                None => (AttachmentSource::Missing, None, fallback_ts),
            };
            sources.push(source);
            jobs.push(AttachmentJob {
                attachment: att,
                timestamp_unix_ms,
                size_hint,
            });
        }
    }

    let mut unit_bytes_done = 0_u64;
    let mut unit_bytes_extra = 0_u64;
    {
        let sources = &mut sources;
        run_attachment_jobs(
            &mut jobs,
            attachments_dir,
            stage_mode,
            &options.compress,
            |i| match sources.get_mut(i) {
                Some(source) => load(source),
                None => Ok(None),
            },
            |p| {
                unit_bytes_done = p.bytes_done;
                unit_bytes_extra = p.bytes_total.saturating_sub(hint_sum);
                on_progress(p.done, p.bytes_done, unit_bytes_extra, base);
            },
            None,
            cancel,
        )
        .map_err(anyhow::Error::msg)?;
    }

    let attachments_saved = jobs
        .iter()
        .filter(|j| j.attachment.path.is_some() && j.attachment.digest_sha256.is_some())
        .count();
    drop(jobs);

    crate::export_transforms::clear_attachments_when_disabled(&mut doc, options.media);
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            att.bytes = None;
        }
    }

    write_format(output_dir, OutputFormat::Jsonl, doc)
        .with_context(|| format!("write {}", path.display()))?;

    Ok(UnitOutcome {
        written: true,
        attachments_saved,
        attachment_count,
        bytes_done: unit_bytes_done,
        bytes_total: unit_bytes_extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_json::read_conversation_jsonl;
    use media::{CompressOptions, MediaMode};
    use message_ir::{ConversationDocument, IrAttachment};
    use message_vault_io_core::LogSink;
    use std::fs;
    use std::sync::{Arc, Mutex};

    fn att(name: &str) -> IrAttachment {
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

    /// A one-message document with `count` attachments, keyed by `who` so
    /// each unit lands on its own conversation file.
    fn doc_with(who: &str, count: usize) -> ConversationDocument {
        let mut doc = message_ir::testutil::sample_document("hello");
        doc.conversation.chat_identifier = who.into();
        doc.conversation.participants[0].handle = who.into();
        doc.messages[0].attachments = (0..count).map(|i| att(&format!("f{i}.jpg"))).collect();
        doc
    }

    fn unit_from(doc: ConversationDocument, sources: Vec<AttachmentSource>) -> ConversationUnit {
        let mut it = sources.into_iter();
        ConversationUnit::from_doc(doc, |_, _att| {
            let source = it.next().unwrap_or(AttachmentSource::Missing);
            let hint = match &source {
                AttachmentSource::Bytes(b) => Some(b.len() as u64),
                _ => None,
            };
            (source, hint)
        })
    }

    fn options(media: MediaMode, resume: bool) -> WriteQueueOptions {
        WriteQueueOptions {
            media,
            compress: CompressOptions::default(),
            resume,
            writer_count: 1,
        }
    }

    fn drain(
        dir: &Path,
        units: Vec<ConversationUnit>,
        options: &WriteQueueOptions,
    ) -> anyhow::Result<WriteQueueReport> {
        drain_write_queue_with_loader(dir, units, options, &mut load_attachment_source, None, None)
    }

    #[test]
    fn drains_units_and_writes_conversation_files_last() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("source.jpg");
        fs::write(&src, b"path-bytes").unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();

        let units = vec![
            unit_from(
                doc_with("+15550000001", 1),
                vec![AttachmentSource::Bytes(b"inline-bytes".to_vec())],
            ),
            unit_from(
                doc_with("+15550000002", 1),
                vec![AttachmentSource::Path(src.clone())],
            ),
        ];
        let report = drain(&out, units, &options(MediaMode::Clone, false)).unwrap();

        assert_eq!(report.conversations_written, 2);
        assert_eq!(report.conversations_skipped, 0);
        assert_eq!(report.attachments_saved, 2);

        let files: Vec<_> = fs::read_dir(&out)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".jsonl"))
            .collect();
        assert_eq!(files.len(), 2, "one conversation file per unit");

        for name in files {
            let doc = read_conversation_jsonl(&out.join(&name)).unwrap();
            let a = &doc.messages[0].attachments[0];
            assert!(a.path.as_deref().unwrap().starts_with("attachments/"));
            assert_eq!(a.digest_sha256.as_ref().unwrap().len(), 64);
            assert!(a.size_bytes.unwrap() > 0);
            assert!(a.bytes.is_none(), "bytes never reach the written file");
            assert!(
                out.join(a.path.as_ref().unwrap()).is_file(),
                "a conversation file on disk means its attachments are too"
            );
        }
    }

    #[test]
    fn resume_skips_a_unit_whose_conversation_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let build = || {
            vec![
                unit_from(
                    doc_with("+15550000001", 1),
                    vec![AttachmentSource::Bytes(b"a".to_vec())],
                ),
                unit_from(
                    doc_with("+15550000002", 1),
                    vec![AttachmentSource::Bytes(b"b".to_vec())],
                ),
            ]
        };
        drain(&out, build(), &options(MediaMode::Clone, false)).unwrap();

        let mut never = |_: &mut AttachmentSource| -> Result<Option<Vec<u8>>, String> {
            panic!("a skipped unit must not load anything")
        };
        let report = drain_write_queue_with_loader(
            &out,
            build(),
            &options(MediaMode::Clone, true),
            &mut never,
            None,
            None,
        )
        .unwrap();

        assert_eq!(report.conversations_skipped, 2);
        assert_eq!(report.conversations_written, 0);
    }

    #[test]
    fn resume_rewrites_a_unit_whose_conversation_file_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let build = || {
            vec![
                unit_from(
                    doc_with("+15550000001", 1),
                    vec![AttachmentSource::Bytes(b"a".to_vec())],
                ),
                unit_from(
                    doc_with("+15550000002", 1),
                    vec![AttachmentSource::Bytes(b"b".to_vec())],
                ),
            ]
        };
        drain(&out, build(), &options(MediaMode::Clone, false)).unwrap();

        let doomed = out.join(format!(
            "{}.jsonl",
            doc_with("+15550000002", 0).filename_stem()
        ));
        assert!(doomed.is_file());
        fs::remove_file(&doomed).unwrap();

        let report = drain(&out, build(), &options(MediaMode::Clone, true)).unwrap();
        assert_eq!(report.conversations_written, 1);
        assert_eq!(report.conversations_skipped, 1);
        assert!(doomed.is_file(), "the missing conversation file came back");
    }

    #[test]
    fn disabled_mode_marks_not_copied_and_clears_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let units = vec![unit_from(
            doc_with("+15550000001", 1),
            vec![AttachmentSource::Bytes(b"ignored".to_vec())],
        )];
        drain(&out, units, &options(MediaMode::Disabled, false)).unwrap();

        let stem = doc_with("+15550000001", 0).filename_stem();
        let doc = read_conversation_jsonl(&out.join(format!("{stem}.jsonl"))).unwrap();
        let a = &doc.messages[0].attachments[0];
        assert_eq!(a.missing_reason.as_deref(), Some("not_copied"));
        assert!(a.path.is_none());
        assert!(a.digest_sha256.is_none());
        let staged = out.join("attachments");
        let empty = !staged.is_dir() || fs::read_dir(&staged).unwrap().next().is_none();
        assert!(empty, "disabled mode writes no attachment files");
    }

    #[test]
    fn missing_source_becomes_file_missing_and_the_drain_continues() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let units = vec![unit_from(
            doc_with("+15550000001", 2),
            vec![
                AttachmentSource::Missing,
                AttachmentSource::Bytes(b"present".to_vec()),
            ],
        )];
        let report = drain(&out, units, &options(MediaMode::Clone, false)).unwrap();
        assert_eq!(report.conversations_written, 1);

        let stem = doc_with("+15550000001", 0).filename_stem();
        let doc = read_conversation_jsonl(&out.join(format!("{stem}.jsonl"))).unwrap();
        let atts = &doc.messages[0].attachments;
        assert_eq!(atts[0].missing_reason.as_deref(), Some("file_missing"));
        assert!(atts[1].path.is_some(), "the readable one still landed");
    }

    #[test]
    fn progress_lines_cover_all_units_with_global_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink_lines = Arc::clone(&lines);
        let sink = LogSink::new(move |l: &str| sink_lines.lock().unwrap().push(l.to_string()));

        let units = vec![
            unit_from(
                doc_with("+15550000001", 1),
                vec![AttachmentSource::Bytes(b"a".to_vec())],
            ),
            unit_from(
                doc_with("+15550000002", 1),
                vec![AttachmentSource::Bytes(b"b".to_vec())],
            ),
        ];
        drain_write_queue_with_loader(
            &out,
            units,
            &options(MediaMode::Clone, false),
            &mut load_attachment_source,
            Some(&sink),
            None,
        )
        .unwrap();

        let lines = lines.lock().unwrap().clone();
        assert!(
            lines
                .iter()
                .any(|l| l == "Preparing 2 conversation file(s)..."),
            "banner missing from {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("  attachments 2/2 ")),
            "counts run across units, not per unit: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "Prepared 2 conversation file(s)"),
            "closing line missing from {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("preparing 1/")),
            "per-conversation count lines would confuse the desktop scraper"
        );
    }
}
