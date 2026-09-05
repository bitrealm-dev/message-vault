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

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use anyhow::{Context, Result};
use media::{CompressOptions, MediaMode};
use message_ir::ConversationDocument;
use message_vault_io_core::{
    AttachmentJob, CancelFlag, LogSink, MediaConfig, OutputFormat, emit_log, run_attachment_jobs,
};

use crate::transcode::{TranscodeOptions, transcode_staged};
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

/// What one attachment added to the drain's totals.
///
/// Deltas, not running counts: a parallel drain folds them into shared
/// atomics, and a sequential one adds them to plain locals. Either way the
/// per-unit body does not need to know the global picture.
struct UnitProgress {
    done: usize,
    bytes_done: u64,
    bytes_total: u64,
}

/// What one unit did. Byte and file counts travel through the progress
/// callback instead, so both drains can fold them their own way.
struct UnitOutcome {
    written: bool,
    attachments_saved: usize,
}

/// Loads one attachment's bytes by source; `Ok(None)` marks it missing.
pub type AttachmentLoader<'a> =
    dyn FnMut(&mut AttachmentSource) -> Result<Option<Vec<u8>>, String> + 'a;

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
    load: &mut AttachmentLoader<'_>,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
) -> Result<WriteQueueReport> {
    check_headroom(output_dir, &units)?;
    let attachments_dir = output_dir.join("attachments");
    let mut report = WriteQueueReport::default();

    let total: usize = units.iter().map(|u| u.attachments.len()).sum();
    let bytes_total_base: u64 = units
        .iter()
        .flat_map(|u| u.attachments.iter())
        .filter_map(|a| a.size_hint)
        .sum();

    announce_start(log, units.len());

    let done = Cell::new(0usize);
    let bytes_done = Cell::new(0u64);
    let bytes_total = Cell::new(bytes_total_base);
    let report_progress = |p: UnitProgress| {
        done.set(done.get() + p.done);
        bytes_done.set(bytes_done.get() + p.bytes_done);
        bytes_total.set(bytes_total.get() + p.bytes_total);
        emit_log(
            log,
            format!(
                "  attachments {}/{} {}/{}",
                done.get(),
                total,
                bytes_done.get(),
                bytes_total.get()
            ),
        );
    };

    for unit in units {
        let outcome = write_one_unit(
            output_dir,
            &attachments_dir,
            unit,
            options,
            load,
            &report_progress,
            cancel,
        )?;
        report.attachments_saved += outcome.attachments_saved;
        if outcome.written {
            report.conversations_written += 1;
        } else {
            report.conversations_skipped += 1;
        }
    }

    report.media = run_media_post_pass(output_dir, options, log, cancel)?;
    announce_finish(log, &report, options.resume);
    Ok(report)
}

/// Writers scale with the machine: writing is IO and hashing, and past a
/// handful of threads the disk, not the CPU, sets the pace.
pub fn default_writer_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 8)
}

/// Drain `units` through the write queue, fold the written/skipped counts
/// into `report`, and return the `FormatSinkResult` the sink path would
/// have produced. The shared tail of every exporter's queue arm.
pub fn drain_units(
    output_dir: &Path,
    units: Vec<ConversationUnit>,
    options: &WriteQueueOptions,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
    report: &mut message_vault_io_core::ExportReport,
) -> Result<crate::FormatSinkResult> {
    let queue_report = drain_write_queue(output_dir, units, options, log, cancel)?;
    report.conversations +=
        (queue_report.conversations_written + queue_report.conversations_skipped) as u64;
    report.attachments_saved += queue_report.attachments_saved as u64;
    Ok(crate::FormatSinkResult {
        xml_path: None,
        media: queue_report.media,
        obfuscated_docs: 0,
    })
}

/// Drain `units` across a pool of writer threads.
///
/// Each worker pops the next conversation, stages its attachments, and writes
/// its conversation file. The first error stops the pool and is what the
/// caller sees.
///
/// # Errors
///
/// Returns the first unit error, or the headroom error when the staging disk
/// cannot hold what the backup needs. A cancel surfaces as `"canceled"`.
pub fn drain_write_queue(
    output_dir: &Path,
    units: Vec<ConversationUnit>,
    options: &WriteQueueOptions,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
) -> Result<WriteQueueReport> {
    check_headroom(output_dir, &units)?;

    let attachments_dir = output_dir.join("attachments");
    // Idempotent, but doing it once here keeps every worker's first write
    // from racing the same create.
    fs::create_dir_all(&attachments_dir)
        .with_context(|| format!("create {}", attachments_dir.display()))?;

    let unit_count = units.len();
    let total: usize = units.iter().map(|u| u.attachments.len()).sum();
    let bytes_total_base: u64 = units
        .iter()
        .flat_map(|u| u.attachments.iter())
        .filter_map(|a| a.size_hint)
        .sum();

    announce_start(log, unit_count);

    let done = AtomicUsize::new(0);
    let bytes_done = AtomicU64::new(0);
    let bytes_total = AtomicU64::new(bytes_total_base);
    let attachments_saved = AtomicUsize::new(0);
    let written = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    let first_error: Mutex<Option<String>> = Mutex::new(None);
    let queue: Mutex<VecDeque<ConversationUnit>> = Mutex::new(VecDeque::from(units));

    let report_progress = |p: UnitProgress| {
        let d = done.fetch_add(p.done, Ordering::Relaxed) + p.done;
        let bd = bytes_done.fetch_add(p.bytes_done, Ordering::Relaxed) + p.bytes_done;
        let bt = bytes_total.fetch_add(p.bytes_total, Ordering::Relaxed) + p.bytes_total;
        emit_log(log, format!("  attachments {d}/{total} {bd}/{bt}"));
    };

    let writer_count = if options.writer_count == 0 {
        default_writer_count()
    } else {
        options.writer_count
    }
    .min(unit_count.max(1));

    std::thread::scope(|scope| {
        for _ in 0..writer_count {
            scope.spawn(|| {
                loop {
                    if abort.load(Ordering::SeqCst) {
                        return;
                    }
                    let Some(unit) = queue.lock().expect("write queue lock").pop_front() else {
                        return;
                    };
                    let mut load = |source: &mut AttachmentSource| {
                        // Name the file before the failure turns into a chip:
                        // otherwise a systemic problem (a revoked permission, a
                        // failing disk) reads as a run's worth of unexplained
                        // missing attachments.
                        let named = match source {
                            AttachmentSource::Path(path) => Some(path.display().to_string()),
                            _ => None,
                        };
                        load_attachment_source(source).map_err(|e| {
                            if let Some(path) = named {
                                emit_log(
                                    log,
                                    format!("warning: attachment {path} could not be read: {e}"),
                                );
                            }
                            e
                        })
                    };
                    match write_one_unit(
                        output_dir,
                        &attachments_dir,
                        unit,
                        options,
                        &mut load,
                        &report_progress,
                        cancel,
                    ) {
                        Ok(outcome) => {
                            attachments_saved
                                .fetch_add(outcome.attachments_saved, Ordering::Relaxed);
                            if outcome.written {
                                written.fetch_add(1, Ordering::Relaxed);
                            } else {
                                skipped.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(err) => {
                            let mut slot = first_error.lock().expect("write queue error slot");
                            if slot.is_none() {
                                *slot = Some(format!("{err:#}"));
                            }
                            abort.store(true, Ordering::SeqCst);
                            return;
                        }
                    }
                }
            });
        }
    });

    if let Some(msg) = first_error.into_inner().expect("write queue error slot") {
        anyhow::bail!(msg);
    }

    let mut report = WriteQueueReport {
        conversations_written: written.load(Ordering::Relaxed),
        conversations_skipped: skipped.load(Ordering::Relaxed),
        attachments_saved: attachments_saved.load(Ordering::Relaxed),
        media: media::MediaReport::default(),
    };
    report.media = run_media_post_pass(output_dir, options, log, cancel)?;
    announce_finish(log, &report, options.resume);
    Ok(report)
}

/// Convert or compress the staged originals, once every writer is done.
///
/// Writers stage originals and nothing else, so this is where convert and
/// compress actually happen. Running it as its own pass buys the CLI what the
/// desktop already had: per-file commits, so an interruption keeps every
/// derivative already finished, and progress worth printing.
fn run_media_post_pass(
    output_dir: &Path,
    options: &WriteQueueOptions,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
) -> Result<media::MediaReport> {
    if !matches!(options.media, MediaMode::Convert | MediaMode::Compress) {
        return Ok(media::MediaReport::default());
    }

    let transcode_options = TranscodeOptions {
        mode: options.media,
        compress: options.compress.clone(),
        // No vault limit applies to a local export, so nothing here is
        // written off as too large. The desktop's own media pass, which does
        // enforce the real limit, never reaches this code: it stages with
        // Clone and converts on its own.
        asset_max_bytes: u64::MAX,
    };
    // Deliberately inert to the desktop's log scraper: no `attachments`
    // prefix and no `preparing` keyword. The desktop never runs this branch;
    // the CLI just prints it.
    let report = transcode_staged(output_dir, &transcode_options, cancel, &mut |p| {
        emit_log(log, format!("  media {}/{}", p.done, p.total));
    })?;

    let mut media = media::MediaReport {
        processed: report.converted,
        skipped: report.skipped + report.repointed,
        bytes_before: report.bytes_before,
        bytes_after: report.bytes_after,
        errors: Vec::new(),
    };
    if report.failed > 0 {
        // The per-file reasons are already on the attachments themselves.
        media.errors.push(format!(
            "{} file(s) could not be converted; their conversation entries say why",
            report.failed
        ));
    }
    emit_log(
        log,
        format!(
            "Attachment {} done: converted={} skipped={} size {} → {}",
            options.media,
            media.processed,
            media.skipped,
            human_bytes(media.bytes_before),
            human_bytes(media.bytes_after)
        ),
    );
    Ok(media)
}

/// Slack above the measured need, for the derivative a convert holds in
/// flight and for whatever else shares the disk.
const DISK_HEADROOM_SLACK: u64 = 64 * 1024 * 1024;

/// Refuse a drain the staging disk plainly cannot hold.
///
/// `needed` counts the originals the units name. Peak usage is those plus one
/// in-flight derivative, since the media pass commits per file, so the sum
/// plus a fixed slack is the honest requirement.
fn check_headroom(output_dir: &Path, units: &[ConversationUnit]) -> Result<()> {
    // Summed before any resume skip: over-asking on a resumed run is the
    // conservative direction, and such a run usually has most of those bytes
    // on disk already.
    let needed: u64 = units
        .iter()
        .flat_map(|u| u.attachments.iter())
        .filter_map(|a| a.size_hint)
        .sum();
    // A filesystem that cannot answer must not block an export.
    let Ok(available) = fs2::available_space(output_dir) else {
        return Ok(());
    };
    match headroom_shortfall(needed, available) {
        Some(message) => anyhow::bail!(message),
        None => Ok(()),
    }
}

/// `None` when `available` covers `needed` plus slack; otherwise what to say.
fn headroom_shortfall(needed: u64, available: u64) -> Option<String> {
    let required = needed.saturating_add(DISK_HEADROOM_SLACK);
    if available >= required {
        return None;
    }
    Some(format!(
        "Not enough space on the staging disk: this backup needs about {}, and {} is free.",
        human_bytes(required),
        human_bytes(available)
    ))
}

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let n = bytes as f64;
    if n >= GIB {
        format!("{:.1} GiB", n / GIB)
    } else if n >= MIB {
        format!("{:.1} MiB", n / MIB)
    } else if n >= KIB {
        format!("{:.1} KiB", n / KIB)
    } else {
        format!("{bytes} B")
    }
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

/// Stage one conversation's attachments, then write the conversation file.
///
/// The order is the engine's whole contract: the conversation file lands last,
/// so its presence on disk vouches for everything it points at.
fn write_one_unit(
    output_dir: &Path,
    attachments_dir: &Path,
    unit: ConversationUnit,
    options: &WriteQueueOptions,
    load: &mut AttachmentLoader<'_>,
    on_progress: &dyn Fn(UnitProgress),
    cancel: Option<&CancelFlag>,
) -> Result<UnitOutcome> {
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
        on_progress(UnitProgress {
            done: attachment_count,
            bytes_done: 0,
            bytes_total: 0,
        });
        return Ok(UnitOutcome {
            written: false,
            attachments_saved: 0,
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
    let mut reported_done = 0_usize;
    {
        let sources = &mut sources;
        run_attachment_jobs(
            &mut jobs,
            attachments_dir,
            &MediaConfig {
                mode: stage_mode,
                compress: options.compress.clone(),
            },
            |i| match sources.get_mut(i) {
                Some(source) => load(source),
                None => Ok(None),
            },
            |p| {
                // run_attachment_jobs reports this unit's running totals; the
                // drain wants what each attachment added.
                let extra = p.bytes_total.saturating_sub(hint_sum);
                on_progress(UnitProgress {
                    done: p.done.saturating_sub(reported_done),
                    bytes_done: p.bytes_done.saturating_sub(unit_bytes_done),
                    bytes_total: extra.saturating_sub(unit_bytes_extra),
                });
                reported_done = p.done;
                unit_bytes_done = p.bytes_done;
                unit_bytes_extra = extra;
            },
            None,
            cancel.map(|flag| flag.as_ref()),
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
        doc.conversation.participants[0].handle = Some(who.into());
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
    #[test]
    fn parallel_drain_writes_every_unit() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let units: Vec<_> = (0..12)
            .map(|i| {
                unit_from(
                    doc_with(&format!("+1555000{i:04}"), 1),
                    vec![AttachmentSource::Bytes(format!("payload-{i}").into_bytes())],
                )
            })
            .collect();
        let mut options = options(MediaMode::Clone, false);
        options.writer_count = 4;

        let report = drain_write_queue(&out, units, &options, None, None).unwrap();

        assert_eq!(report.conversations_written, 12);
        assert_eq!(report.attachments_saved, 12);
        let written = fs::read_dir(&out)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
            .count();
        assert_eq!(written, 12);
    }

    #[test]
    fn parallel_drain_stops_on_the_first_error() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        // A directory sitting where a conversation file must go: the write
        // fails for that unit, and the drain reports it rather than
        // finishing quietly.
        let blocked = doc_with("+15550000003", 0).filename_stem();
        fs::create_dir_all(out.join(format!("{blocked}.jsonl"))).unwrap();

        let units: Vec<_> = (1..=4)
            .map(|i| {
                unit_from(
                    doc_with(&format!("+1555000000{i}"), 1),
                    vec![AttachmentSource::Bytes(b"x".to_vec())],
                )
            })
            .collect();
        let mut options = options(MediaMode::Clone, false);
        options.writer_count = 2;

        let err = drain_write_queue(&out, units, &options, None, None).unwrap_err();
        assert!(
            format!("{err:#}").contains(&blocked),
            "the error should name the conversation that failed: {err:#}"
        );
    }

    #[test]
    fn an_unreadable_attachment_is_logged_before_it_becomes_a_chip() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let missing = tmp.path().join("gone.jpg");

        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink_lines = Arc::clone(&lines);
        let sink = LogSink::new(move |l: &str| sink_lines.lock().unwrap().push(l.to_string()));

        let units = vec![unit_from(
            doc_with("+15550000001", 1),
            vec![AttachmentSource::Path(missing.clone())],
        )];
        let report = drain_write_queue(
            &out,
            units,
            &options(MediaMode::Clone, false),
            Some(&sink),
            None,
        )
        .unwrap();

        assert_eq!(report.conversations_written, 1, "the drain carries on");
        let lines = lines.lock().unwrap().clone();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("warning: attachment ") && l.contains("could not be read")),
            "an unreadable attachment says why before it turns into a chip: {lines:?}"
        );

        let stem = doc_with("+15550000001", 0).filename_stem();
        let doc = read_conversation_jsonl(&out.join(format!("{stem}.jsonl"))).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].missing_reason.as_deref(),
            Some("file_missing")
        );
    }

    #[test]
    fn headroom_shortfall_speaks_when_space_is_short() {
        assert!(headroom_shortfall(10 * 1024 * 1024 * 1024, 1024).is_some());
        assert_eq!(headroom_shortfall(1024, 10 * 1024 * 1024 * 1024), None);
        let msg = headroom_shortfall(2 * 1024 * 1024 * 1024, 1024).unwrap();
        assert!(msg.contains("free"), "{msg}");
        assert!(msg.contains("GiB"), "{msg}");
    }

    #[test]
    fn default_writer_count_is_bounded() {
        let n = default_writer_count();
        assert!((1..=8).contains(&n));
    }
    /// A minimal valid 1x1 RGB PNG that ffmpeg reads cleanly.
    #[rustfmt::skip]
    const PNG_1X1_RGB: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
        0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
        0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn clone_mode_runs_no_media_pass() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let units = vec![unit_from(
            doc_with("+15550000001", 1),
            vec![AttachmentSource::Bytes(b"plain".to_vec())],
        )];
        let report = drain(&out, units, &options(MediaMode::Clone, false)).unwrap();
        assert_eq!(report.media, media::MediaReport::default());
    }

    #[test]
    fn convert_runs_as_a_pass_after_the_drain_stages_originals() {
        if !media::ffmpeg_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();

        let mut doc = doc_with("+15550000001", 1);
        doc.messages[0].attachments[0].original_name = Some("shot.png".into());
        let units = vec![unit_from(
            doc,
            vec![AttachmentSource::Bytes(PNG_1X1_RGB.to_vec())],
        )];

        let report = drain(&out, units, &options(MediaMode::Convert, false)).unwrap();

        assert_eq!(report.conversations_written, 1);
        assert_eq!(
            report.media.processed, 1,
            "the post-pass converted the staged original"
        );

        let stem = doc_with("+15550000001", 0).filename_stem();
        let written = read_conversation_jsonl(&out.join(format!("{stem}.jsonl"))).unwrap();
        let path = written.messages[0].attachments[0].path.as_deref().unwrap();
        assert!(
            path.ends_with(".jpg"),
            "convert repoints the attachment at its derivative: {path}"
        );
        assert!(
            out.join(path).is_file(),
            "the derivative the conversation names is on disk"
        );
    }
}
