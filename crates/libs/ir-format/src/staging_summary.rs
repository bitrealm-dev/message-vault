//! Recompute what a staged folder holds, for the approval gates.
//!
//! Everything here is measured from the folder. The one estimate is what the
//! media step will do to a file's size, and it is labelled as an estimate all
//! the way to the screen.
//!
//! Decision 39: this is always recomputed from the folder, never read back
//! from a previously-written `summary_json` — the folder is the truth, and
//! that is what makes resuming at a gate work: reopening the session
//! recomputes rather than restoring.
//!
//! Contact matching is not done here — the vault answers which identifiers
//! it already knows, and this returns the distinct identifiers found on
//! disk for the caller to ask about.
//!
//! ## Aliasing
//!
//! Staged names are content-addressed, so two attachment records — in the
//! same document or different ones — can legitimately share one physical
//! file (see `transcode.rs`'s module docs for why). `attachments` counts
//! every reference, because that is what the documents actually contain, but
//! `attachment_bytes`, `verdict_counts`, and `forecasts` are per physical
//! file: the first reference to a given recorded path is measured and
//! classified, and every later reference at that same path is folded into
//! the `attachments` count alone. Mirrors `pending_in`'s dedup in
//! `transcode.rs`, which faces the identical fact about the folder.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::Result;
use media::{MediaMode, SizeVerdict, classify_probed, estimate_bytes, needs_probe, probe_media};

use crate::read_json::read_conversation_jsonl;
use crate::transcode::{
    COMMITTED_SUFFIX, TranscodeOptions, conversation_files, safe_attachment_path,
};

/// How often [`summarize_staging`] reports progress, over attachments.
///
/// Matches the media crate's own cadence (its private `MEDIA_PROGRESS_EVERY`
/// is 100 too) so a summary pass and a media pass over the same folder feel
/// the same to whatever is watching progress.
const SUMMARY_PROGRESS_EVERY: usize = 100;

/// One attachment the user should see before approving.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentForecast {
    /// Relative path inside the staging folder.
    pub path: String,
    /// Name to show on the screen: the attachment's original name when the
    /// document recorded one, else the staged (content-addressed) file
    /// name. The person approving this knows the file as `IMG_4821.MOV`,
    /// not as whatever hash-derived name it was staged under.
    pub name: String,
    /// Bytes on disk now.
    pub size_bytes: u64,
    /// Bytes expected after the media step. Equal to `size_bytes` when there
    /// is no media step.
    pub estimate_bytes: u64,
    /// How it is expected to land against the limit.
    pub verdict: SizeVerdict,
}

/// How many attachments landed in each verdict.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictCounts {
    /// Under the limit now, and expected to stay under.
    pub fits_as_is: usize,
    /// Over the limit now, expected to come under after the media step.
    pub likely_fits: usize,
    /// Under the limit now, expected to cross it during the media step.
    pub may_grow: usize,
    /// Over the limit now, and expected to stay over.
    pub probably_too_big: usize,
    /// The media step does not handle this kind of file, so its size is fixed.
    pub cannot_process: usize,
}

impl VerdictCounts {
    /// Tally one more attachment's verdict.
    fn record(&mut self, verdict: SizeVerdict) {
        match verdict {
            SizeVerdict::FitsAsIs => self.fits_as_is += 1,
            SizeVerdict::LikelyFits => self.likely_fits += 1,
            SizeVerdict::MayGrow => self.may_grow += 1,
            SizeVerdict::ProbablyTooBig => self.probably_too_big += 1,
            SizeVerdict::CannotProcess => self.cannot_process += 1,
        }
    }
}

/// What a staged folder holds.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagingSummary {
    /// Conversation files found in the folder.
    pub conversations: usize,
    /// Messages across every conversation.
    pub messages: u64,
    /// Distinct participant identifiers, sorted. The vault decides which of
    /// these it already knows.
    pub contact_identifiers: Vec<String>,
    /// Attachments referenced by the documents, including ones already marked
    /// missing and every reference to a shared, content-addressed file.
    pub attachments: usize,
    /// Bytes on disk under `attachments/` for the files that are actually
    /// there, counted once per physical file — see the module docs on
    /// aliasing.
    pub attachment_bytes: u64,
    /// How many physical files landed in each size verdict — see the module
    /// docs on aliasing.
    pub verdict_counts: VerdictCounts,
    /// One row per physical file whose verdict is not `fits_as_is` — see the
    /// module docs on aliasing.
    pub forecasts: Vec<AttachmentForecast>,
}

/// How far [`summarize_staging`] has got, reported over attachments.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryProgress {
    /// Attachments visited so far.
    pub done: usize,
    /// Attachments total.
    pub total: usize,
}

/// Recompute a staged folder's summary: exact conversation/message/attachment
/// counts plus a per-attachment size forecast, for the approval gates.
///
/// Walks the same `*.jsonl` list the media pass walks. For each attachment
/// already carrying a `missing_reason` — settled, whether or not its `path`
/// still points at a file on disk (a `convert_failed` original keeps its
/// path so a resume can retry it, but it has already been flagged and must
/// not be forecast again) — this counts it and stops there: no bytes, no
/// forecast row. The same is true for an attachment with no `path`, or whose
/// recorded file is not on disk.
///
/// Everything else reads its length from disk (never the document's stale
/// `size_bytes`). A recorded path whose stem already ends in `-mv` names a
/// derivative the media pass has committed and will never touch again
/// (`pending_in`'s own exclusion rule, in `transcode.rs`); it is classified
/// on that size alone, with no probe and `estimate_bytes` equal to
/// `size_bytes` — applying a mode's growth or shrink factor to it would
/// forecast a transcode that can never happen. Every other file is probed
/// when [`media::needs_probe`] says it is close enough to the limit to
/// matter and `options.mode` has a media step, then classified with
/// [`media::classify_probed`]. Under [`MediaMode::Clone`] and
/// [`MediaMode::Disabled`] there is no media step at all: probing is skipped
/// entirely — not an optimization, since probing would forecast work that
/// will never run — `estimate_bytes` equals `size_bytes`, and the file is
/// classified on its current size alone.
///
/// The probe is best-effort: a failed ffprobe call on one file means
/// classifying it with no probe in hand, never failing the summary — a gate
/// that cannot render because one file is unreadable is worse than a gate
/// with one rougher estimate.
///
/// Content-addressed staging means two attachment records can share one
/// physical file (see the module docs on aliasing); `attachments` counts
/// every reference, but a shared file's bytes, verdict, and forecast row are
/// each counted exactly once, on the first reference to its recorded path.
///
/// `on_progress` reports [`SummaryProgress`] over attachments — every
/// reference, aliased or not, since that is the count the caller sees
/// growing — at the same cadence the media crate uses (every 100) plus a
/// final call.
///
/// # Errors
///
/// Returns an error when the folder cannot be read or a conversation file
/// cannot be parsed.
pub fn summarize_staging(
    staging_dir: &Path,
    options: &TranscodeOptions,
    on_progress: &mut dyn FnMut(SummaryProgress),
) -> Result<StagingSummary> {
    let files = conversation_files(staging_dir)?;

    let mut summary = StagingSummary::default();
    let mut contacts = BTreeSet::new();
    // Gathered while walking the documents for their conversation/message/
    // contact counts, so the classification pass below can run over a flat
    // list with a known total up front, matching `on_progress`'s contract.
    let mut attachments: Vec<AttachmentRef> = Vec::new();

    for jsonl in &files {
        let doc = read_conversation_jsonl(jsonl)?;
        summary.conversations += 1;
        summary.messages += doc.messages.len() as u64;
        for participant in &doc.conversation.participants {
            contacts.insert(participant.handle.clone());
        }
        for msg in &doc.messages {
            for att in &msg.attachments {
                attachments.push(AttachmentRef {
                    path: att.path.clone(),
                    missing_reason: att.missing_reason.clone(),
                    original_name: att.original_name.clone(),
                });
            }
        }
    }
    summary.contact_identifiers = contacts.into_iter().collect();

    let total = attachments.len();
    on_progress(SummaryProgress { done: 0, total });

    let has_media_step = matches!(options.mode, MediaMode::Convert | MediaMode::Compress);
    // Recorded paths already measured and classified, so an aliased second
    // (or third, …) reference to the same physical file contributes to
    // `attachments` only — see the module docs on aliasing.
    let mut classified_paths: HashSet<String> = HashSet::new();
    let mut done = 0usize;
    for att in attachments {
        summary.attachments += 1;
        if att.missing_reason.is_none()
            && let Some(rel) = att.path.as_deref()
            && classified_paths.insert(rel.to_string())
        {
            classify_one(
                staging_dir,
                rel,
                att.original_name.as_deref(),
                options,
                has_media_step,
                &mut summary,
            );
        }
        done += 1;
        if done.is_multiple_of(SUMMARY_PROGRESS_EVERY) || done == total {
            on_progress(SummaryProgress { done, total });
        }
    }

    Ok(summary)
}

/// One attachment reference gathered from a document, stripped to the fields
/// [`summarize_staging`]'s classification pass needs.
struct AttachmentRef {
    path: Option<String>,
    missing_reason: Option<String>,
    original_name: Option<String>,
}

/// Measure and classify one physical file, folding its bytes and verdict
/// into `summary`.
///
/// Called only for a reference not already settled (no `missing_reason`) and
/// not already classified via an earlier reference to the same recorded
/// path — the caller (`summarize_staging`) filters both before calling.
/// `rel` unsafe, or its file missing from disk, contributes nothing —
/// silently, since a document recording a path with no bytes behind it is
/// exactly the "not there" case this whole function exists to skip.
fn classify_one(
    staging_dir: &Path,
    rel: &str,
    original_name: Option<&str>,
    options: &TranscodeOptions,
    has_media_step: bool,
    summary: &mut StagingSummary,
) {
    let Ok(abs) = safe_attachment_path(staging_dir, rel) else {
        return;
    };
    let Ok(meta) = std::fs::metadata(&abs) else {
        return;
    };
    let size_bytes = meta.len();
    summary.attachment_bytes += size_bytes;

    let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let committed = stem.ends_with(COMMITTED_SUFFIX);

    let (verdict, estimate) = if committed {
        // A committed derivative: `pending_in` (transcode.rs) excludes a
        // `-mv` stem from work unconditionally, so applying `options.mode`'s
        // growth or shrink factor here would forecast a transcode that will
        // never run. Judged on its own size, exactly as `pending_in` treats
        // it as done.
        let verdict = if size_bytes <= options.asset_max_bytes {
            SizeVerdict::FitsAsIs
        } else {
            SizeVerdict::ProbablyTooBig
        };
        (verdict, size_bytes)
    } else {
        let ext = abs
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let probe = if has_media_step && needs_probe(size_bytes, options.asset_max_bytes) {
            // Best-effort: an ffprobe failure classifies without a probe
            // rather than failing the whole summary.
            probe_media(&abs).ok()
        } else {
            None
        };
        let verdict = classify_probed(
            size_bytes,
            probe.as_ref(),
            &ext,
            options.mode,
            &options.compress,
            options.asset_max_bytes,
        );
        let estimate = if has_media_step {
            estimate_bytes(
                size_bytes,
                probe.as_ref(),
                &ext,
                options.mode,
                &options.compress,
            )
        } else {
            size_bytes
        };
        (verdict, estimate)
    };

    summary.verdict_counts.record(verdict);
    if verdict == SizeVerdict::FitsAsIs {
        return;
    }
    let name = original_name.map(ToString::to_string).unwrap_or_else(|| {
        abs.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(rel)
            .to_string()
    });
    summary.forecasts.push(AttachmentForecast {
        path: rel.to_string(),
        name,
        size_bytes,
        estimate_bytes: estimate,
        verdict,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::write_conversation_jsonl_to;
    use media::CompressOptions;
    use message_ir::{HandleType, IrAttachment, IrParticipant};

    fn summary_options() -> TranscodeOptions {
        TranscodeOptions {
            mode: MediaMode::Convert,
            compress: CompressOptions::default(),
            asset_max_bytes: 50 * 1024 * 1024,
        }
    }

    /// Two conversations sharing one participant, five messages total, and a
    /// single attachment (in the first conversation) recorded at
    /// `attachments/photo.png` — a path that may or may not have bytes
    /// behind it yet, left to each test to decide.
    ///
    /// The shared participant (`+15550101`) is the *first* one inserted (via
    /// conversation A), and conversation B's own new identifier
    /// (`+15550100`) sorts before it — so a reader that returned identifiers
    /// in first-seen order rather than sorted order would produce
    /// `["+15550101", "+15550100"]` here, which fails the sorted-order
    /// assertion in `counts_conversations_messages_and_distinct_contacts`.
    fn staged_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("attachments")).unwrap();

        let mut doc_a = message_ir::testutil::sample_document("hi from conversation A");
        doc_a.conversation.chat_identifier = "+15550101".into();
        doc_a.conversation.participants = vec![IrParticipant {
            handle: "+15550101".into(),
            display_name: Some("A".into()),
            handle_type: Some(HandleType::Phone),
        }];
        let mut second = doc_a.messages[0].clone();
        second.guid = "guid-a2".into();
        second.timestamp_unix_ms += 1000;
        let mut third = doc_a.messages[0].clone();
        third.guid = "guid-a3".into();
        third.timestamp_unix_ms += 2000;
        third.attachments = vec![IrAttachment {
            path: Some("attachments/photo.png".into()),
            original_name: Some("photo.png".into()),
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            // Deliberately stale: the reader must measure the file on disk,
            // never trust this.
            size_bytes: Some(1),
            missing_reason: None,
            bytes: None,
        }];
        doc_a.messages.push(second);
        doc_a.messages.push(third);
        doc_a.finalize_stats();
        let jsonl_a = dir.path().join(format!("{}.jsonl", doc_a.filename_stem()));
        write_conversation_jsonl_to(&jsonl_a, &doc_a).unwrap();

        let mut doc_b = message_ir::testutil::sample_document("hi from conversation B");
        doc_b.conversation.chat_identifier = "+15550100".into();
        doc_b.conversation.participants = vec![
            IrParticipant {
                handle: "+15550101".into(),
                display_name: None,
                handle_type: Some(HandleType::Phone),
            },
            IrParticipant {
                handle: "+15550100".into(),
                display_name: Some("B".into()),
                handle_type: Some(HandleType::Phone),
            },
        ];
        let mut second_b = doc_b.messages[0].clone();
        second_b.guid = "guid-b2".into();
        second_b.timestamp_unix_ms += 1000;
        doc_b.messages.push(second_b);
        doc_b.finalize_stats();
        let jsonl_b = dir.path().join(format!("{}.jsonl", doc_b.filename_stem()));
        write_conversation_jsonl_to(&jsonl_b, &doc_b).unwrap();

        dir
    }

    /// One conversation, one attachment already carrying `missing_reason`.
    /// Its recorded path points at a file that *is* on disk (the
    /// `convert_failed` shape: the original survives so a resume can retry
    /// it), so a reader that forgets to check `missing_reason` first would
    /// wrongly count its bytes and forecast it.
    fn staged_fixture_with_missing_reason(reason: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("attachments")).unwrap();
        let attachment = dir.path().join("attachments/broken.png");
        std::fs::write(&attachment, vec![9u8; 2048]).unwrap();

        let mut doc = message_ir::testutil::sample_document("hello");
        doc.messages[0].attachments = vec![IrAttachment {
            path: Some("attachments/broken.png".into()),
            original_name: Some("broken.png".into()),
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: Some(2048),
            missing_reason: Some(reason.to_string()),
            bytes: None,
        }];
        doc.finalize_stats();
        let jsonl = dir.path().join(format!("{}.jsonl", doc.filename_stem()));
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        dir
    }

    /// One conversation whose single message carries one attachment per
    /// `(name, size)` pair, each backed by a sparse file of exactly that
    /// length under `attachments/` — cheap even for a size in the hundreds
    /// of megabytes, since only the metadata length is exercised.
    fn staged_fixture_with_sizes(specs: &[(&str, u64)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let attachments_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&attachments_dir).unwrap();

        let mut doc = message_ir::testutil::sample_document("attachments only");
        doc.messages[0].attachments = specs
            .iter()
            .map(|(name, size)| {
                let file = std::fs::File::create(attachments_dir.join(name)).unwrap();
                file.set_len(*size).unwrap();
                IrAttachment {
                    path: Some(format!("attachments/{name}")),
                    original_name: Some((*name).to_string()),
                    mime_type: None,
                    digest_sha256: None,
                    is_sticker: false,
                    transcription: None,
                    sticker_effect: None,
                    size_bytes: Some(*size),
                    missing_reason: None,
                    bytes: None,
                }
            })
            .collect();
        doc.finalize_stats();
        let jsonl = dir.path().join(format!("{}.jsonl", doc.filename_stem()));
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        dir
    }

    /// One physical file, sized `size`, referenced by two attachment records
    /// in two separate conversation documents — the legitimate aliasing case
    /// content-addressed staging produces (see the module docs).
    fn staged_fixture_with_aliased_attachment(size: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let attachments_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&attachments_dir).unwrap();
        std::fs::File::create(attachments_dir.join("shared.png"))
            .unwrap()
            .set_len(size)
            .unwrap();

        let shared_attachment = || IrAttachment {
            path: Some("attachments/shared.png".into()),
            original_name: Some("shared.png".into()),
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: Some(size),
            missing_reason: None,
            bytes: None,
        };

        let mut doc_a = message_ir::testutil::sample_document("conversation A, shared photo");
        doc_a.conversation.chat_identifier = "+15550100".into();
        doc_a.messages[0].attachments = vec![shared_attachment()];
        doc_a.finalize_stats();
        let jsonl_a = dir.path().join(format!("{}.jsonl", doc_a.filename_stem()));
        write_conversation_jsonl_to(&jsonl_a, &doc_a).unwrap();

        let mut doc_b = message_ir::testutil::sample_document("conversation B, same photo");
        doc_b.conversation.chat_identifier = "+15550199".into();
        doc_b.messages[0].attachments = vec![shared_attachment()];
        doc_b.finalize_stats();
        let jsonl_b = dir.path().join(format!("{}.jsonl", doc_b.filename_stem()));
        write_conversation_jsonl_to(&jsonl_b, &doc_b).unwrap();

        dir
    }

    /// One conversation whose single message carries `count` attachments,
    /// each a distinct small file, for pinning the progress cadence.
    fn staged_fixture_with_many_attachments(count: usize) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let attachments_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&attachments_dir).unwrap();

        let mut doc = message_ir::testutil::sample_document("lots of attachments");
        doc.messages[0].attachments = (0..count)
            .map(|i| {
                let name = format!("f{i:04}.bin");
                std::fs::File::create(attachments_dir.join(&name))
                    .unwrap()
                    .set_len(10)
                    .unwrap();
                IrAttachment {
                    path: Some(format!("attachments/{name}")),
                    original_name: Some(name.clone()),
                    mime_type: None,
                    digest_sha256: None,
                    is_sticker: false,
                    transcription: None,
                    sticker_effect: None,
                    size_bytes: Some(10),
                    missing_reason: None,
                    bytes: None,
                }
            })
            .collect();
        doc.finalize_stats();
        let jsonl = dir.path().join(format!("{}.jsonl", doc.filename_stem()));
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();
        dir
    }

    #[test]
    fn counts_conversations_messages_and_distinct_contacts() {
        let dir = staged_fixture(); // two conversations, one shared participant
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.conversations, 2);
        assert_eq!(summary.messages, 5);
        assert_eq!(
            summary.contact_identifiers,
            vec!["+15550100".to_string(), "+15550101".to_string()],
            "sorted and de-duplicated across conversations, not first-seen order"
        );
    }

    #[test]
    fn attachment_bytes_are_measured_on_disk_not_read_from_the_document() {
        // size_bytes in the document is what the writer recorded. The folder is
        // the truth, and a resumed run must not trust a stale field.
        let dir = staged_fixture();
        let attachment = dir.path().join("attachments/photo.png");
        std::fs::write(&attachment, vec![7u8; 4096]).unwrap();
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.attachment_bytes, 4096);
    }

    #[test]
    fn an_attachment_that_is_already_missing_is_counted_but_not_forecast() {
        let dir = staged_fixture_with_missing_reason("not_copied");
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.attachments, 1);
        assert_eq!(summary.attachment_bytes, 0);
        assert!(
            summary.forecasts.is_empty(),
            "nothing to forecast about a file that is not there"
        );
        assert_eq!(
            summary.verdict_counts.fits_as_is
                + summary.verdict_counts.likely_fits
                + summary.verdict_counts.may_grow
                + summary.verdict_counts.probably_too_big
                + summary.verdict_counts.cannot_process,
            0,
            "a settled attachment is never classified at all"
        );
    }

    #[test]
    fn only_files_worth_reporting_get_a_forecast_row() {
        // Every attachment is classified; a row is returned only where the
        // verdict is something other than "fits as-is", because that is the
        // whole content of the report. The counts cover the rest.
        let dir =
            staged_fixture_with_sizes(&[("small.png", 1024), ("huge.png", 900 * 1024 * 1024)]);
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.verdict_counts.fits_as_is, 1);
        assert_eq!(summary.verdict_counts.probably_too_big, 1);
        assert_eq!(summary.forecasts.len(), 1);
        assert_eq!(summary.forecasts[0].name, "huge.png");
    }

    #[test]
    fn copy_and_skip_modes_forecast_nothing_because_nothing_will_change() {
        // There is no media step under these modes, so every file is judged on
        // the size it already has and no probing happens at all.
        let dir = staged_fixture_with_sizes(&[("huge.png", 900 * 1024 * 1024)]);
        let mut options = summary_options();
        options.mode = MediaMode::Clone;
        let summary = summarize_staging(dir.path(), &options, &mut |_| {}).unwrap();
        assert_eq!(summary.verdict_counts.probably_too_big, 1);
        assert_eq!(
            summary.forecasts[0].estimate_bytes,
            summary.forecasts[0].size_bytes
        );
    }

    #[test]
    fn a_jpeg_convert_leaves_alone_forecasts_its_own_size_not_a_shrink() {
        // Convert mode leaves an already-JPEG file untouched (`untouched_by`
        // in the media crate). The row must say so: an estimate lower than
        // the current size would promise a shrink that convert_video/image
        // never performs on a file already in its target format.
        let dir = staged_fixture_with_sizes(&[("big.jpg", 55 * 1024 * 1024)]);
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.forecasts.len(), 1);
        assert_eq!(
            summary.forecasts[0].estimate_bytes,
            summary.forecasts[0].size_bytes
        );
    }

    #[test]
    fn a_committed_derivative_is_judged_on_its_own_size_not_a_mode_factor() {
        // `pending_in` (transcode.rs) excludes a `-mv` stem from work
        // unconditionally, so a `.mp4` already committed as a derivative
        // must never have a mode's growth/shrink factor applied to it here —
        // that would forecast a transcode that will never run. Sized inside
        // the probe band (so a version of this code that still probed would
        // exercise the probe path) but under the limit.
        let dir = staged_fixture_with_sizes(&[("clip-mv.mp4", 30 * 1024 * 1024)]);
        for mode in [MediaMode::Convert, MediaMode::Compress] {
            let mut options = summary_options();
            options.mode = mode;
            let summary = summarize_staging(dir.path(), &options, &mut |_| {}).unwrap();
            assert_eq!(summary.verdict_counts.fits_as_is, 1, "mode {mode:?}");
            assert!(
                summary.forecasts.is_empty(),
                "a committed derivative under the limit gets no forecast row (mode {mode:?})"
            );
        }
    }

    #[test]
    fn aliased_attachments_sharing_one_file_count_bytes_and_forecast_once() {
        // Content-addressed staging: two records, one physical file.
        // `attachments` is per-reference (2); everything else measured from
        // the file itself is per-physical-file (1).
        let dir = staged_fixture_with_aliased_attachment(900 * 1024 * 1024);
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.attachments, 2, "one reference per document");
        assert_eq!(
            summary.attachment_bytes,
            900 * 1024 * 1024,
            "the shared file's bytes are counted once, not once per reference"
        );
        assert_eq!(summary.forecasts.len(), 1, "one row for the one file");
    }

    #[test]
    fn two_attachments_in_one_document_sharing_one_file_count_bytes_and_forecast_once() {
        // Same aliasing fact as the cross-document test above, but both
        // references live in ONE document — the dedup loop (`classified_paths`)
        // walks a single flattened list across every document, so it is
        // document-agnostic by construction, but the same-document case had
        // no test pinning it (deferred from Task 4's review as a coverage
        // gap, closed here at the final review alongside the transcode.rs
        // aliasing fix, which faces the identical blind spot).
        let dir = tempfile::tempdir().unwrap();
        let attachments_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&attachments_dir).unwrap();
        std::fs::File::create(attachments_dir.join("shared.png"))
            .unwrap()
            .set_len(900 * 1024 * 1024)
            .unwrap();

        let shared_attachment = || IrAttachment {
            path: Some("attachments/shared.png".into()),
            original_name: Some("shared.png".into()),
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: Some(900 * 1024 * 1024),
            missing_reason: None,
            bytes: None,
        };

        let mut doc = message_ir::testutil::sample_document("one conversation, two references");
        doc.messages[0].attachments = vec![shared_attachment()];
        let mut second = doc.messages[0].clone();
        second.guid = "second-message-guid".into();
        second.timestamp_unix_ms += 1000;
        second.attachments = vec![shared_attachment()];
        doc.messages.push(second);
        doc.finalize_stats();
        let jsonl = dir.path().join(format!("{}.jsonl", doc.filename_stem()));
        write_conversation_jsonl_to(&jsonl, &doc).unwrap();

        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(
            summary.attachments, 2,
            "one reference per message, both in the same document"
        );
        assert_eq!(
            summary.attachment_bytes,
            900 * 1024 * 1024,
            "the shared file's bytes are counted once, not once per reference"
        );
        assert_eq!(summary.forecasts.len(), 1, "one row for the one file");
    }

    #[test]
    fn a_folder_with_no_conversation_files_is_an_empty_summary_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.conversations, 0);
        assert_eq!(summary.messages, 0);
    }

    #[test]
    fn progress_reports_a_final_call_matching_the_attachment_total() {
        let dir = staged_fixture_with_sizes(&[("a.png", 10), ("b.png", 20), ("c.png", 30)]);
        let mut seen = Vec::new();
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |p| {
            seen.push((p.done, p.total))
        })
        .unwrap();
        assert_eq!(seen.first(), Some(&(0, 3)));
        assert_eq!(seen.last(), Some(&(3, 3)));
        assert_eq!(summary.attachments, 3);
    }

    #[test]
    fn progress_cadence_is_pinned_at_every_hundred_plus_a_final_call() {
        let dir = staged_fixture_with_many_attachments(250);
        let mut seen = Vec::new();
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |p| {
            seen.push((p.done, p.total))
        })
        .unwrap();
        assert_eq!(
            seen,
            vec![(0, 250), (100, 250), (200, 250), (250, 250)],
            "initial, every 100, and a final call — no more, no fewer"
        );
        assert_eq!(summary.attachments, 250);
    }
}
