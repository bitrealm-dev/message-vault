//! Write conversations in one output format (JSON, JSON Lines, CSV, EML, MBOX, or XML).

use crate::clean::clean_previous_ir_output;
use crate::export_transforms::{ExportTransforms, apply_transforms};
use crate::write::write_format;
use crate::write_sbr::SbrBackupSession;
use anyhow::{Context, Result};
use media::MediaReport;
use message_ir::ConversationDocument;
use message_vault_io_core::OutputFormat;
use std::fs;
use std::path::{Path, PathBuf};

/// Result of [`FormatSink::finish`].
#[derive(Debug, Default)]
pub struct FormatSinkResult {
    /// Path of the written `smses.xml` when the format is XML.
    pub xml_path: Option<PathBuf>,
    /// Media pass report from the finish step.
    pub media: MediaReport,
    /// Number of documents obfuscated.
    pub obfuscated_docs: usize,
}

impl FormatSinkResult {
    /// Human-readable lines for CLI / GUI logs.
    pub fn log_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.media.processed > 0 || self.media.skipped > 0 || !self.media.errors.is_empty() {
            lines.push(format!(
                "Media: processed {} file(s), skipped {}",
                self.media.processed, self.media.skipped
            ));
            for err in self.media.errors.iter().take(10) {
                lines.push(format!("  media warning: {err}"));
            }
            if self.media.errors.len() > 10 {
                lines.push(format!("  …and {} more", self.media.errors.len() - 10));
            }
        }
        if self.obfuscated_docs > 0 {
            lines.push(format!(
                "Obfuscated {} conversation(s)",
                self.obfuscated_docs
            ));
        }
        if let Some(path) = &self.xml_path {
            lines.push(format!("Wrote {}", path.display()));
        }
        lines
    }
}

/// Writes conversations in the requested [`OutputFormat`].
///
/// Documents are buffered until [`finish`](Self::finish), which applies
/// attachment media transforms and obfuscation, then projects all chats.
pub struct FormatSink {
    output_dir: PathBuf,
    format: OutputFormat,
    transforms: ExportTransforms,
    docs: Vec<ConversationDocument>,
}

impl FormatSink {
    /// Open a sink that buffers documents until [`finish`](Self::finish).
    ///
    /// # Errors
    ///
    /// Currently always succeeds. The `Result` matches the other constructors.
    pub fn open(
        output_dir: &Path,
        format: OutputFormat,
        transforms: ExportTransforms,
    ) -> Result<Self> {
        Ok(Self {
            output_dir: output_dir.to_path_buf(),
            format,
            transforms,
            docs: Vec::new(),
        })
    }

    /// Prepare `output` for a fresh export, then open a sink into it.
    ///
    /// Creates the output directory, removes files from previous exports via
    /// [`clean_previous_ir_output`], creates `attachments/` when the
    /// transforms copy media, and returns the sink together with the
    /// attachments directory path (created or not).
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created or previous
    /// export files cannot be removed.
    pub fn open_prepared(
        output: &Path,
        format: OutputFormat,
        transforms: ExportTransforms,
    ) -> Result<(Self, PathBuf)> {
        fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
        clean_previous_ir_output(output)?;
        let att_dir = output.join("attachments");
        if transforms.copies_attachments() {
            fs::create_dir_all(&att_dir)
                .with_context(|| format!("create {}", att_dir.display()))?;
        }
        let sink = Self::open(output, format, transforms)?;
        Ok((sink, att_dir))
    }

    /// Reopen `output` to continue an interrupted export.
    ///
    /// Unlike [`open_prepared`](Self::open_prepared), nothing is cleaned: the
    /// conversation files and staged attachments the interrupted run left
    /// behind are exactly the work a resumed run gets to skip. The directory
    /// must already be an export folder — it carries the sentinel — because
    /// resuming into anything else is a caller bug, not something to repair
    /// by cleaning.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory or its sentinel is missing, or the
    /// attachments directory cannot be created.
    pub fn open_resume(
        output: &Path,
        format: OutputFormat,
        transforms: ExportTransforms,
    ) -> Result<(Self, PathBuf)> {
        if !output.join(crate::clean::EXPORT_SENTINEL).is_file() {
            anyhow::bail!(
                "cannot resume into {}: it is not a staging folder from a previous run",
                output.display()
            );
        }
        let att_dir = output.join("attachments");
        if transforms.copies_attachments() {
            fs::create_dir_all(&att_dir)
                .with_context(|| format!("create {}", att_dir.display()))?;
        }
        let sink = Self::open(output, format, transforms)?;
        Ok((sink, att_dir))
    }

    /// Output format this sink will write.
    pub fn format(&self) -> OutputFormat {
        self.format
    }

    /// Directory conversations are written into.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Media and obfuscation settings applied at [`finish`](Self::finish).
    pub fn transforms(&self) -> &ExportTransforms {
        &self.transforms
    }

    /// Buffer one conversation until [`finish`](Self::finish).
    ///
    /// # Errors
    ///
    /// Currently always succeeds.
    pub fn write_document(&mut self, doc: ConversationDocument) -> Result<()> {
        self.docs.push(doc);
        Ok(())
    }

    /// Apply media and obfuscation transforms, then write all buffered documents.
    ///
    /// For EML, MBOX, and XML, media is transformed then embedded. The staged
    /// `attachments/` directory is removed so the output folder holds only the
    /// archive.
    ///
    /// # Errors
    ///
    /// Returns an error when a transform or a write fails.
    pub fn finish(mut self) -> Result<FormatSinkResult> {
        let embeds_media = self.format.is_mail_archive() || self.format.is_sbr_xml();
        let outcome = apply_transforms(
            &mut self.docs,
            &self.output_dir,
            &self.transforms,
            embeds_media,
        )?;

        let mut result = FormatSinkResult {
            xml_path: None,
            // Convert/compress runs in `run_attachment_jobs` before documents
            // reach the sink; finish itself does no media work.
            media: MediaReport::default(),
            obfuscated_docs: outcome.obfuscated_docs,
        };

        if self.format.is_sbr_xml() {
            let mut session = SbrBackupSession::create(&self.output_dir)?;
            for doc in &self.docs {
                session.append_document(doc)?;
            }
            result.xml_path = Some(session.finish()?);
        } else {
            for doc in self.docs {
                write_format(&self.output_dir, self.format, doc)?;
            }
        }

        if embeds_media {
            remove_staged_attachments(&self.output_dir)?;
        }
        Ok(result)
    }
}

/// Drop transform staging under `attachments/` after media has been embedded.
fn remove_staged_attachments(output_dir: &Path) -> Result<()> {
    let att_dir = output_dir.join("attachments");
    if att_dir.is_dir() {
        fs::remove_dir_all(&att_dir)?;
    }
    Ok(())
}

/// Write every document through the sink with the shared "Preparing N
/// conversation file(s)" progress log, bumping `report.conversations` per
/// document, then finish the sink. The shared tail of every exporter's
/// non-queue arm.
pub fn write_documents_through_sink(
    documents: Vec<message_ir::ConversationDocument>,
    mut sink: FormatSink,
    log: Option<&message_vault_io_core::LogSink>,
    cancel: Option<&message_vault_io_core::CancelFlag>,
    report: &mut message_vault_io_core::ExportReport,
) -> anyhow::Result<FormatSinkResult> {
    use message_vault_io_core::emit_log;
    let total_conversations = documents.len() as u64;
    emit_log(log, "");
    emit_log(
        log,
        format!("Preparing {total_conversations} conversation file(s)..."),
    );
    let mut written = 0u64;
    for doc in documents {
        message_vault_io_core::check_cancel(cancel)?;
        written += 1;
        sink.write_document(doc)?;
        report.conversations += 1;
        // `%` instead of `u64::is_multiple_of`: that method needs Rust 1.87.
        #[allow(clippy::manual_is_multiple_of)]
        if written % 100 == 0 || written == total_conversations {
            emit_log(log, format!("  preparing {written}/{total_conversations}"));
        }
    }
    sink.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::MediaMode;
    use message_ir::IrAttachment;
    use std::fs;

    #[test]
    fn format_sink_csv_writes_per_conversation() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink =
            FormatSink::open(tmp.path(), OutputFormat::Csv, ExportTransforms::none()).unwrap();
        sink.write_document(message_ir::testutil::sample_document("hello"))
            .unwrap();
        sink.finish().unwrap();
        assert!(tmp.path().join("+15555550101.csv").is_file());
    }

    #[test]
    fn format_sink_xml_merges_documents() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink =
            FormatSink::open(tmp.path(), OutputFormat::Xml, ExportTransforms::none()).unwrap();
        sink.write_document(message_ir::testutil::sample_document("one"))
            .unwrap();
        let mut doc2 = message_ir::testutil::sample_document("two");
        doc2.messages[0].guid = "guid-2".into();
        doc2.messages[0].timestamp_unix_ms = 1_400_773_262_000;
        sink.write_document(doc2).unwrap();
        let result = sink.finish().unwrap();
        let path = result.xml_path.expect("smses.xml");
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains(r#"count="2""#));
        assert!(text.contains("one"));
        assert!(text.contains("two"));
    }

    #[test]
    fn format_sink_eml_embeds_media_and_drops_attachments_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let att_dir = tmp.path().join("attachments");
        fs::create_dir_all(&att_dir).unwrap();
        let rel = "attachments/photo.jpg";
        fs::write(tmp.path().join(rel), b"jpeg-bytes").unwrap();

        let mut doc = message_ir::testutil::sample_document("with media");
        doc.messages[0].attachments = vec![IrAttachment {
            path: Some(rel.into()),
            original_name: Some("photo.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }];

        let transforms = ExportTransforms {
            media: MediaMode::Clone,
            ..ExportTransforms::none()
        };
        let mut sink = FormatSink::open(tmp.path(), OutputFormat::Eml, transforms).unwrap();
        sink.write_document(doc).unwrap();
        sink.finish().unwrap();

        assert!(!tmp.path().join("attachments").exists());
        let eml_dir = tmp.path().join("+15555550101");
        assert!(eml_dir.is_dir());
        let eml = fs::read_dir(&eml_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let body = fs::read_to_string(&eml).unwrap();
        assert!(body.contains("jpeg-bytes") || body.contains("photo.jpg"));
    }

    #[test]
    fn format_sink_csv_keeps_attachments_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let att_dir = tmp.path().join("attachments");
        fs::create_dir_all(&att_dir).unwrap();
        fs::write(att_dir.join("photo.jpg"), b"jpeg-bytes").unwrap();

        let mut sink =
            FormatSink::open(tmp.path(), OutputFormat::Csv, ExportTransforms::none()).unwrap();
        sink.write_document(message_ir::testutil::sample_document("hello"))
            .unwrap();
        sink.finish().unwrap();
        assert!(tmp.path().join("attachments/photo.jpg").is_file());
    }

    #[test]
    fn format_sink_obfuscate_rewrites_handles() {
        let tmp = tempfile::tempdir().unwrap();
        let transforms = ExportTransforms {
            obfuscate: true,
            obfuscate_seed: Some("01234567".into()),
            ..ExportTransforms::none()
        };
        let mut sink = FormatSink::open(tmp.path(), OutputFormat::Csv, transforms).unwrap();
        // Obfuscation does not touch `source` bags (vendor leftovers), so drop
        // the fixture's bag before asserting covered fields are rewritten.
        let mut doc = message_ir::testutil::sample_document("secret");
        doc.messages[0].source = None;
        sink.write_document(doc).unwrap();
        let result = sink.finish().unwrap();
        assert_eq!(result.obfuscated_docs, 1);
        let mut found = false;
        for entry in fs::read_dir(tmp.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) == Some("csv") {
                let body = fs::read_to_string(&path).unwrap();
                assert!(!body.contains("+15555550101"));
                assert!(!body.contains("secret"));
                found = true;
            }
        }
        assert!(found, "expected obfuscated csv");
    }
    #[test]
    fn open_resume_keeps_previous_output_and_requires_the_sentinel() {
        let tmp = tempfile::tempdir().unwrap();
        // A directory that was never an export folder is refused: resuming
        // into one is a caller bug, not something to repair by cleaning.
        assert!(
            FormatSink::open_resume(tmp.path(), OutputFormat::Jsonl, ExportTransforms::none())
                .is_err()
        );

        let (_sink, att_dir) =
            FormatSink::open_prepared(tmp.path(), OutputFormat::Jsonl, ExportTransforms::none())
                .unwrap();
        std::fs::create_dir_all(&att_dir).unwrap();
        std::fs::write(tmp.path().join("keep.jsonl"), "x").unwrap();
        std::fs::write(att_dir.join("keep.jpg"), "y").unwrap();

        let (_sink, att_dir2) =
            FormatSink::open_resume(tmp.path(), OutputFormat::Jsonl, ExportTransforms::none())
                .unwrap();

        assert!(
            tmp.path().join("keep.jsonl").is_file(),
            "a resumed run keeps the conversations the interrupted one wrote"
        );
        assert!(
            att_dir2.join("keep.jpg").is_file(),
            "and the attachments they point at"
        );
    }
}
