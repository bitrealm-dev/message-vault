//! Media convert/compress and obfuscation applied before writing files.

use crate::util::read_attachment_file;
use anyhow::{Context, Result};
use media::{CompressOptions, MediaMode, MediaReport};
use message_ir::{ConversationDocument, IrAttachment, IrDirection, IrParticipant};
use message_vault_io_core::{LogSink, MediaConfig, ObfuscateConfig, emit_log};
use obfuscate::{
    Obfuscator, classify_attachment, materialize_placeholders, placeholder_rel_path,
    resolve_obfuscator_with_log,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Options passed into [`crate::FormatSink`] for media and obfuscation.
#[derive(Debug, Clone)]
pub struct ExportTransforms {
    /// Media mode applied at finish (clone/convert/compress/disabled).
    pub media: MediaMode,
    /// Video/audio compression options used with Compress mode.
    pub compress: CompressOptions,
    /// Whether to replace PII and media with obfuscated placeholders.
    pub obfuscate: bool,
    /// Seed for deterministic obfuscation; `None` generates one.
    pub obfuscate_seed: Option<String>,
    /// Mid-run notes (e.g. generated obfuscate seed). `None` → stderr.
    pub log: Option<LogSink>,
}

impl Default for ExportTransforms {
    fn default() -> Self {
        Self {
            media: MediaMode::Clone,
            compress: CompressOptions::default(),
            obfuscate: false,
            obfuscate_seed: None,
            log: None,
        }
    }
}

impl ExportTransforms {
    /// Build transforms from a `MediaConfig` and `ObfuscateConfig`
    /// (obfuscation is enabled when either obfuscation flag or the seed is set).
    pub fn from_configs(media: &MediaConfig, obfuscate: &ObfuscateConfig) -> Self {
        Self {
            media: media.mode,
            compress: media.compress.clone(),
            obfuscate: obfuscate.enabled || obfuscate.seed.is_some(),
            obfuscate_seed: obfuscate.seed.clone(),
            log: None,
        }
    }

    /// All-defaults transform set (clone, no obfuscation, no log).
    pub fn none() -> Self {
        Self::default()
    }

    /// True when ffmpeg/ffprobe will be required (false when obfuscating,
    /// which replaces media with placeholders).
    pub fn needs_media_tools(&self) -> bool {
        // Obfuscate replaces all media with placeholders — no ffmpeg work.
        !self.obfuscate && self.media.needs_tools()
    }

    /// True when attachment bytes should be staged under `attachments/`
    /// (false when obfuscating).
    pub fn copies_attachments(&self) -> bool {
        // Obfuscate discards real bytes; skip staging them in the first place.
        !self.obfuscate && self.media.copies_attachments()
    }
}

#[cfg(test)]
pub(crate) fn apply_media_remap(doc: &mut ConversationDocument, remap: &HashMap<String, String>) {
    if remap.is_empty() {
        return;
    }
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            if let Some(path) = att.path.as_mut()
                && let Some(new_rel) = remap.get(path.as_str())
            {
                *path = new_rel.clone();
                // Bytes on disk changed (possibly same path); drop stale digests.
                att.digest_sha256 = None;
                att.size_bytes = None;
                att.bytes = None;
                if let Some(mime) = mime_for_rel(new_rel) {
                    att.mime_type = Some(mime);
                }
            }
        }
    }
}

pub(crate) fn reload_attachment_bytes(doc: &mut ConversationDocument, output_dir: &Path) {
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            // Lenient: IO failures leave bytes unset so packaging can continue.
            if let Ok(Some(bytes)) = read_attachment_file(att, output_dir) {
                att.bytes = Some(bytes);
            }
        }
    }
}

pub(crate) fn clear_attachments_when_disabled(doc: &mut ConversationDocument, mode: MediaMode) {
    if !matches!(mode, MediaMode::Disabled) {
        return;
    }
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            att.path = None;
            att.bytes = None;
            att.digest_sha256 = None;
        }
    }
}

pub(crate) fn obfuscate_document(doc: &mut ConversationDocument, anon: &mut Obfuscator) {
    doc.conversation.chat_identifier = anon.obfuscate_handle(&doc.conversation.chat_identifier);
    if let Some(title) = doc.conversation.group_title.as_mut() {
        *title = anon.obfuscate_mixed_field(title);
    }
    for p in &mut doc.conversation.participants {
        obfuscate_participant(p, anon);
    }
    if let Some(h) = doc.export.owner_handle.as_mut() {
        *h = anon.obfuscate_handle(h);
    }
    if let Some(n) = doc.export.owner_display_name.as_mut() {
        *n = anon.obfuscate_display_name(n);
    }
    for msg in &mut doc.messages {
        if let Some(h) = msg.sender_handle.as_mut() {
            *h = anon.obfuscate_handle(h);
        }
        if let Some(n) = msg.sender_display_name.as_mut() {
            if msg.direction == IrDirection::Outgoing && n == "Me" {
                // Keep the conventional outgoing label.
            } else {
                *n = anon.obfuscate_display_name(n);
            }
        }
        if let Some(s) = msg.subject.as_mut() {
            *s = anon.obfuscate_text(s);
        }
        msg.text = anon.obfuscate_text(&msg.text);
        if let Some(im) = msg.imessage.as_mut()
            && let Some(a) = im.announcement.as_mut()
        {
            *a = anon.obfuscate_text(a);
        }
        for att in &mut msg.attachments {
            obfuscate_attachment(att);
        }
    }
}

fn obfuscate_participant(p: &mut IrParticipant, anon: &mut Obfuscator) {
    p.handle = anon.obfuscate_handle(&p.handle);
    if let Some(n) = p.display_name.as_mut() {
        *n = anon.obfuscate_display_name(n);
    }
}

fn obfuscate_attachment(att: &mut IrAttachment) {
    let class = classify_attachment(att.mime_type.as_deref(), att.path.as_deref());
    let rel = placeholder_rel_path(class);
    att.path = Some(rel.to_string());
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    att.original_name = Some(format!("attachment.{ext}"));
    if att.transcription.as_deref().is_some_and(|s| !s.is_empty()) {
        att.transcription = Some("[redacted]".into());
    }
    att.digest_sha256 = None;
    att.bytes = None;
}

#[cfg(test)]
fn refresh_missing_attachment_digests(
    docs: &mut [ConversationDocument],
    output_dir: &Path,
) -> Result<()> {
    for doc in docs.iter_mut() {
        for msg in &mut doc.messages {
            for att in &mut msg.attachments {
                if att.digest_sha256.as_deref().is_some_and(|s| !s.is_empty()) {
                    continue;
                }
                let Some(rel) = att.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                    continue;
                };
                let abs = output_dir.join(rel);
                if !abs.is_file() {
                    continue;
                }
                let meta = fs::metadata(&abs)
                    .with_context(|| format!("stat attachment {}", abs.display()))?;
                att.size_bytes = Some(meta.len());
                att.digest_sha256 = Some(media::file_sha256(&abs)?);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn mime_for_rel(rel: &str) -> Option<String> {
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

pub(crate) struct TransformOutcome {
    pub media: MediaReport,
    pub obfuscated_docs: usize,
}

pub(crate) fn apply_transforms(
    docs: &mut [ConversationDocument],
    output_dir: &Path,
    transforms: &ExportTransforms,
    load_bytes: bool,
) -> Result<TransformOutcome> {
    // Keep MIME/path for placeholder classification when obfuscating.
    if !transforms.obfuscate {
        for doc in docs.iter_mut() {
            clear_attachments_when_disabled(doc, transforms.media);
        }
    }

    // Convert/compress runs in `run_attachment_jobs` before documents are
    // written. Finish only obfuscates and packages.
    let media = MediaReport::default();

    let mut obfuscated_docs = 0usize;
    if transforms.obfuscate {
        materialize_placeholders(output_dir)?;
        let log_fn = |line: &str| emit_log(transforms.log.as_ref(), line);
        let mut anon =
            resolve_obfuscator_with_log(transforms.obfuscate_seed.as_deref(), Some(&log_fn))?;
        for doc in docs.iter_mut() {
            obfuscate_document(doc, &mut anon);
            obfuscated_docs += 1;
        }
    }

    if load_bytes {
        for doc in docs.iter_mut() {
            reload_attachment_bytes(doc, output_dir);
        }
    }

    Ok(TransformOutcome {
        media,
        obfuscated_docs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::MediaMode;
    use message_ir::{
        ConversationMeta, ConversationStats, ExportMeta, IrConversationType, IrMessage,
        IrMessageKind, IrParticipant, IrService, SCHEMA_VERSION,
    };
    use sha2::{Digest, Sha256};
    use std::fs;

    fn doc_with_image_attachment() -> ConversationDocument {
        ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "test".into(),
                tool: "test".into(),
                tool_version: "0".into(),
                owner_handle: None,
                owner_display_name: None,
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                    handle_type: None,
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
                text: "hi".into(),
                attachments: vec![IrAttachment {
                    path: Some("photo.jpg".into()),
                    original_name: Some("photo.jpg".into()),
                    mime_type: Some("image/jpeg".into()),
                    digest_sha256: None,
                    is_sticker: false,
                    transcription: None,
                    sticker_effect: None,
                    size_bytes: None,
                    missing_reason: None,
                    bytes: None,
                }],
                imessage: None,
                source: None,
            }],
            packaging_stem_suffix: None,
        }
    }

    #[test]
    fn media_remap_clears_stale_digest() {
        let mut doc = doc_with_image_attachment();
        doc.messages[0].attachments[0].path = Some("attachments/photo.png".into());
        doc.messages[0].attachments[0].digest_sha256 = Some("a".repeat(64));
        doc.messages[0].attachments[0].size_bytes = Some(12);
        let mut remap = HashMap::new();
        remap.insert(
            "attachments/photo.png".into(),
            "attachments/photo.jpg".into(),
        );
        apply_media_remap(&mut doc, &remap);
        let att = &doc.messages[0].attachments[0];
        assert_eq!(att.path.as_deref(), Some("attachments/photo.jpg"));
        assert!(att.digest_sha256.is_none());
        assert!(att.size_bytes.is_none());
        assert_eq!(att.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn refresh_digests_fills_missing_after_remap() {
        let tmp = tempfile::tempdir().unwrap();
        let att_dir = tmp.path().join("attachments");
        fs::create_dir_all(&att_dir).unwrap();
        let bytes = b"fresh-jpeg-bytes";
        fs::write(att_dir.join("photo.jpg"), bytes).unwrap();

        let mut docs = vec![doc_with_image_attachment()];
        docs[0].messages[0].attachments[0].path = Some("attachments/photo.png".into());
        docs[0].messages[0].attachments[0].digest_sha256 = Some("b".repeat(64));
        let mut remap = HashMap::new();
        remap.insert(
            "attachments/photo.png".into(),
            "attachments/photo.jpg".into(),
        );
        apply_media_remap(&mut docs[0], &remap);
        refresh_missing_attachment_digests(&mut docs, tmp.path()).unwrap();
        let att = &docs[0].messages[0].attachments[0];
        let expected = hex::encode(Sha256::digest(bytes));
        assert_eq!(att.digest_sha256.as_deref(), Some(expected.as_str()));
        assert_eq!(att.size_bytes, Some(bytes.len() as u64));
    }

    #[test]
    fn obfuscate_disables_copy_and_media_tools() {
        let t = ExportTransforms {
            media: MediaMode::Convert,
            obfuscate: true,
            ..ExportTransforms::none()
        };
        assert!(!t.copies_attachments());
        assert!(!t.needs_media_tools());

        let t = ExportTransforms {
            media: MediaMode::Clone,
            obfuscate: false,
            ..ExportTransforms::none()
        };
        assert!(t.copies_attachments());
        assert!(!t.needs_media_tools());
    }

    #[test]
    fn obfuscate_skips_staged_media_and_writes_placeholders() {
        let tmp = tempfile::tempdir().unwrap();
        let att = tmp.path().join("attachments");
        fs::create_dir_all(&att).unwrap();
        // Pretend an exporter staged a real file (should be removed).
        fs::write(att.join("real-photo.jpg"), b"REAL_JPEG_BYTES_SHOULD_GO").unwrap();

        let mut docs = vec![doc_with_image_attachment()];
        let transforms = ExportTransforms {
            media: MediaMode::Convert,
            obfuscate: true,
            obfuscate_seed: Some("01234567".into()),
            ..ExportTransforms::none()
        };
        let outcome = apply_transforms(&mut docs, tmp.path(), &transforms, false).unwrap();
        assert_eq!(outcome.obfuscated_docs, 1);
        assert_eq!(outcome.media.processed, 0);

        assert!(!att.join("real-photo.jpg").exists());
        assert!(att.join("placeholder.jpg").is_file());
        assert!(att.join("placeholder.mp4").is_file());
        assert!(att.join("placeholder.bin").is_file());
        assert_eq!(
            docs[0].messages[0].attachments[0].path.as_deref(),
            Some("attachments/placeholder.jpg")
        );
    }

    #[test]
    fn obfuscate_keeps_mime_when_media_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let mut docs = vec![doc_with_image_attachment()];
        let transforms = ExportTransforms {
            media: MediaMode::Disabled,
            obfuscate: true,
            obfuscate_seed: Some("01234567".into()),
            ..ExportTransforms::none()
        };
        apply_transforms(&mut docs, tmp.path(), &transforms, false).unwrap();
        assert_eq!(
            docs[0].messages[0].attachments[0].path.as_deref(),
            Some("attachments/placeholder.jpg")
        );
    }

    #[test]
    fn convert_at_finish_leaves_cloned_file() {
        let tmp = tempfile::tempdir().unwrap();
        let att = tmp.path().join("attachments");
        fs::create_dir_all(&att).unwrap();
        fs::write(att.join("keep.bin"), b"already-cloned").unwrap();
        let mut docs = vec![doc_with_image_attachment()];
        docs[0].messages[0].attachments[0].path = Some("attachments/keep.bin".into());
        let transforms = ExportTransforms {
            media: MediaMode::Convert,
            ..ExportTransforms::none()
        };
        let outcome = apply_transforms(&mut docs, tmp.path(), &transforms, false).unwrap();
        assert_eq!(outcome.media.processed, 0);
        assert_eq!(
            docs[0].messages[0].attachments[0].path.as_deref(),
            Some("attachments/keep.bin")
        );
        assert_eq!(fs::read(att.join("keep.bin")).unwrap(), b"already-cloned");
    }
}
