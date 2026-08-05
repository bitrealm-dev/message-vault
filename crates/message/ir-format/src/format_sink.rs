//! Unified per-export writer for IR packaging formats.

use crate::clean::clean_previous_ir_output;
use crate::export_transforms::{ExportTransforms, apply_transforms};
use crate::write_sbr::SbrBackupSession;
use crate::write::write_format;
use message_ir::ConversationDocument;
use anyhow::{Context, Result};
use message_vault_io_core::OutputFormat;
use media::MediaReport;
use std::fs;
use std::path::{Path, PathBuf};

/// Result of [`FormatSink::finish`].
#[derive(Debug, Default)]
pub struct FormatSinkResult {
    pub xml_path: Option<PathBuf>,
    pub media: MediaReport,
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
    /// Creates the output directory, removes artifacts from previous exports
    /// via [`clean_previous_ir_output`], creates `attachments/` when the
    /// transforms copy media, and returns the sink together with the
    /// attachments directory path (created or not).
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

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn transforms(&self) -> &ExportTransforms {
        &self.transforms
    }

    pub fn write_document(&mut self, doc: ConversationDocument) -> Result<()> {
        self.docs.push(doc);
        Ok(())
    }

    /// Apply media/obfuscate transforms, then write all buffered documents.
    ///
    /// For EML / MBOX / XML, media is transformed then embedded; the staged
    /// `attachments/` directory is removed so the output folder is the archive.
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
            media: outcome.media,
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

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::{
    ConversationMeta,
    ConversationStats,
    ExportMeta,
    IrAttachment,
    IrConversationType,
    IrDirection,
    IrMessage,
    IrMessageKind,
    IrParticipant,
    IrService,
    SCHEMA_VERSION,
};
    use media::MediaMode;
    use std::fs;

    fn tiny_doc(text: &str) -> ConversationDocument {
        let mut doc = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "test".into(),
                tool: "test".into(),
                tool_version: "0".into(),
                owner_handle: Some("+15555550100".into()),
                owner_display_name: Some("Me".into()),
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                }],
                stats: ConversationStats::default(),
            },
            messages: vec![IrMessage {
                guid: "guid-1".into(),
                timestamp_unix_ms: 1_400_773_261_000,
                direction: IrDirection::Incoming,
                service: IrService::Sms,
                message_kind: IrMessageKind::Sms,
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: text.into(),
                attachments: vec![],
                imessage: None,
                source: None,
            }],
            packaging_stem_suffix: None,
        };
        doc.finalize_stats();
        doc
    }

    #[test]
    fn format_sink_csv_writes_per_conversation() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink =
            FormatSink::open(tmp.path(), OutputFormat::Csv, ExportTransforms::none()).unwrap();
        sink.write_document(tiny_doc("hello")).unwrap();
        sink.finish().unwrap();
        assert!(tmp.path().join("+15555550101.csv").is_file());
    }

    #[test]
    fn format_sink_xml_merges_documents() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink =
            FormatSink::open(tmp.path(), OutputFormat::Xml, ExportTransforms::none()).unwrap();
        sink.write_document(tiny_doc("one")).unwrap();
        let mut doc2 = tiny_doc("two");
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

        let mut doc = tiny_doc("with media");
        doc.messages[0].attachments = vec![IrAttachment {
            path: Some(rel.into()),
            original_name: Some("photo.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: None,
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
        sink.write_document(tiny_doc("hello")).unwrap();
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
        sink.write_document(tiny_doc("secret")).unwrap();
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
}
