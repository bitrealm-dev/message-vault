//! Parse `.eml` / mboxrd back into [`MailMessage`].

use crate::{Direction, MailAttachment, MailMessage, Participant};
use anyhow::{Context, Result};
use mailparse::{MailHeader, MailHeaderMap, ParsedMail};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct AttachmentMetaCell {
    path: Option<String>,
    original_name: Option<String>,
    mime_type: Option<String>,
    #[serde(default)]
    is_sticker: bool,
    transcription: Option<String>,
    sticker_effect: Option<String>,
    digest_sha256: Option<String>,
}

/// Parse one RFC 5322 / MIME message (EML bytes) into [`MailMessage`].
///
/// # Errors
///
/// Returns an error when the bytes are not a valid email or a required
/// `X-ME-*` header is missing.
pub fn mail_message_from_eml_bytes(bytes: &[u8]) -> Result<MailMessage> {
    let mail = mailparse::parse_mail(bytes).context("parse eml bytes")?;
    let headers = &mail.headers;

    let chat_identifier = required_header(headers, "X-ME-Chat-Identifier")?;
    let conversation_type = header_or(headers, "X-ME-Conversation-Type", "individual");
    let group_title = optional_header(headers, "X-ME-Group-Title");
    let participants = parse_participants(headers);
    let guid = required_header(headers, "X-ME-Guid")?;
    let timestamp_unix_ms = required_header(headers, "X-ME-Timestamp-Unix-Ms")?
        .parse::<i64>()
        .context("parse X-ME-Timestamp-Unix-Ms")?;
    let direction = match header_or(headers, "X-ME-Direction", "incoming")
        .to_ascii_lowercase()
        .as_str()
    {
        "outgoing" => Direction::Outgoing,
        _ => Direction::Incoming,
    };
    let service = header_or(headers, "X-ME-Service", "sms");
    let message_kind = header_or(headers, "X-ME-Message-Kind", "sms");
    let sender_handle = optional_header(headers, "X-ME-Sender-Handle");
    let sender_display_name = optional_header(headers, "X-ME-Sender-Display-Name");
    let owner_handle = optional_header(headers, "X-ME-Owner-Handle").unwrap_or_default();
    let owner_display_name = optional_header(headers, "X-ME-Owner-Display-Name");
    let subject = optional_header(headers, "X-ME-Subject");
    let android_type = optional_header(headers, "X-ME-Android-Type");
    let source_fields_json = optional_header(headers, "X-ME-Source-Fields");
    let export_source = header_or(headers, "X-ME-Export-Source", "");
    let export_tool = header_or(headers, "X-ME-Export-Tool", "");
    let export_tool_version = header_or(headers, "X-ME-Export-Tool-Version", "");

    let text = extract_text_body(&mail).unwrap_or_default();
    let attachments = merge_attachments(&mail, headers)?;

    Ok(MailMessage {
        chat_identifier,
        conversation_type,
        group_title,
        participants,
        guid,
        timestamp_unix_ms,
        direction,
        service,
        message_kind,
        sender_handle,
        sender_display_name,
        owner_handle,
        owner_display_name,
        subject,
        text,
        android_type,
        source_fields_json,
        export_source,
        export_tool,
        export_tool_version,
        attachments,
        filename_suffix: None,
        is_reply: header_bool(headers, "X-ME-Is-Reply"),
        in_reply_to_guid: optional_header(headers, "X-ME-Thread-Originator-Guid"),
        thread_originator_part: header_u32(headers, "X-ME-Thread-Originator-Part"),
        num_replies: header_u32(headers, "X-ME-Num-Replies"),
        is_deleted: header_bool(headers, "X-ME-Is-Deleted"),
        send_effect: optional_header(headers, "X-ME-Send-Effect"),
        shared_location: optional_header(headers, "X-ME-Shared-Location"),
        announcement: optional_header(headers, "X-ME-Announcement"),
        read_receipt_rfc3339: optional_header(headers, "X-ME-Read-Receipt"),
        parts_json: optional_header(headers, "X-ME-Parts"),
        edits_json: optional_header(headers, "X-ME-Edits"),
        app_json: optional_header(headers, "X-ME-App"),
        balloon_bundle_id: optional_header(headers, "X-ME-Balloon-Bundle-Id"),
        balloon_kind: optional_header(headers, "X-ME-Balloon-Kind"),
        tapbacks_json: optional_header(headers, "X-ME-Tapbacks"),
        associated_guid: optional_header(headers, "X-ME-Associated-Guid"),
        associated_part: header_u32(headers, "X-ME-Associated-Part"),
        tapback_kind: optional_header(headers, "X-ME-Tapback-Kind"),
        tapback_emoji: optional_header(headers, "X-ME-Tapback-Emoji"),
        tapback_action: optional_header(headers, "X-ME-Tapback-Action"),
    })
}

/// Read an mboxrd file and parse each record into [`MailMessage`].
///
/// # Errors
///
/// Returns an error when the file cannot be read or a record cannot be parsed.
pub fn mail_messages_from_mbox(path: &Path) -> Result<Vec<MailMessage>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let records = split_mboxrd(&text);
    let mut out = Vec::with_capacity(records.len());
    for (i, eml) in records.iter().enumerate() {
        let msg = mail_message_from_eml_bytes(eml)
            .with_context(|| format!("parse mbox record {} in {}", i + 1, path.display()))?;
        out.push(msg);
    }
    Ok(out)
}

/// Split mboxrd text into raw EML payloads (envelope `From ` lines removed).
pub(crate) fn split_mboxrd(text: &str) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with("From ") {
            if let Some(cur) = current.take() {
                records.push(join_eml_lines(&cur));
            }
            current = Some(Vec::new());
            continue;
        }
        if let Some(ref mut cur) = current {
            cur.push(unescape_mboxrd_line(line).to_string());
        }
    }
    if let Some(cur) = current {
        records.push(join_eml_lines(&cur));
    }
    records
}

fn join_eml_lines(lines: &[String]) -> Vec<u8> {
    let mut body = lines.join("\n");
    while body.ends_with('\n') {
        body.pop();
    }
    body.push('\n');
    body.into_bytes()
}

fn unescape_mboxrd_line(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'>' {
        i += 1;
    }
    if i > 0 && bytes[i..].starts_with(b"From ") {
        &line[1..]
    } else {
        line
    }
}

fn required_header(headers: &[MailHeader<'_>], name: &str) -> Result<String> {
    optional_header(headers, name)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing required header {name}"))
}

fn optional_header(headers: &[MailHeader<'_>], name: &str) -> Option<String> {
    headers
        .get_first_value(name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn header_or(headers: &[MailHeader<'_>], name: &str, default: &str) -> String {
    optional_header(headers, name).unwrap_or_else(|| default.to_string())
}

fn header_bool(headers: &[MailHeader<'_>], name: &str) -> bool {
    optional_header(headers, name)
        .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
        .unwrap_or(false)
}

fn header_u32(headers: &[MailHeader<'_>], name: &str) -> Option<u32> {
    optional_header(headers, name)?.parse().ok()
}

fn parse_participants(headers: &[MailHeader<'_>]) -> Vec<Participant> {
    let Some(raw) = optional_header(headers, "X-ME-Participants") else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn extract_text_body(mail: &ParsedMail<'_>) -> Option<String> {
    if mail.subparts.is_empty() {
        return mail.get_body().ok().map(|s| trim_body(&s));
    }
    for part in mail.parts() {
        let mime = part.ctype.mimetype.to_ascii_lowercase();
        if mime == "text/plain" {
            if let Ok(body) = part.get_body() {
                return Some(trim_body(&body));
            }
        }
    }
    // Fallback: first non-multipart body.
    for part in mail.parts() {
        if part.subparts.is_empty() && part.ctype.mimetype.starts_with("text/") {
            if let Ok(body) = part.get_body() {
                return Some(trim_body(&body));
            }
        }
    }
    None
}

fn trim_body(s: &str) -> String {
    s.trim_end_matches(['\r', '\n']).to_string()
}

fn merge_attachments(
    mail: &ParsedMail<'_>,
    headers: &[MailHeader<'_>],
) -> Result<Vec<MailAttachment>> {
    let meta: Vec<AttachmentMetaCell> = optional_header(headers, "X-ME-Attachment-Meta")
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    let mut mime_atts = Vec::new();
    collect_mime_attachments(mail, &mut mime_atts);

    let n = meta.len().max(mime_atts.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let m = meta.get(i);
        let (bytes, mime_fallback, name_fallback) = mime_atts
            .get(i)
            .cloned()
            .unwrap_or_else(|| (Vec::new(), None, None));
        out.push(MailAttachment {
            bytes,
            original_name: m.and_then(|c| c.original_name.clone()).or(name_fallback),
            mime_type: m.and_then(|c| c.mime_type.clone()).or(mime_fallback),
            digest_sha256: m.and_then(|c| c.digest_sha256.clone()),
            is_sticker: m.map(|c| c.is_sticker).unwrap_or(false),
            transcription: m.and_then(|c| c.transcription.clone()),
            sticker_effect: m.and_then(|c| c.sticker_effect.clone()),
        });
        // path from meta is unused on MailAttachment (IR restores path separately)
        let _ = m.and_then(|c| c.path.clone());
    }
    Ok(out)
}

fn collect_mime_attachments(
    mail: &ParsedMail<'_>,
    out: &mut Vec<(Vec<u8>, Option<String>, Option<String>)>,
) {
    if mail.subparts.is_empty() {
        return;
    }
    for part in &mail.subparts {
        if !part.subparts.is_empty() {
            collect_mime_attachments(part, out);
            continue;
        }
        let mime = part.ctype.mimetype.to_ascii_lowercase();
        if mime == "text/plain" || mime == "text/html" {
            continue;
        }
        let disp = part.get_content_disposition();
        let is_attachment = disp.disposition == mailparse::DispositionType::Attachment
            || disp
                .params
                .get("filename")
                .map(|s| !s.is_empty())
                .unwrap_or(false)
            || (!mime.starts_with("text/") && !mime.starts_with("multipart/"));
        if !is_attachment {
            continue;
        }
        let bytes = part.get_body_raw().unwrap_or_default();
        let name = disp
            .params
            .get("filename")
            .cloned()
            .or_else(|| part.ctype.params.get("name").cloned());
        out.push((bytes, Some(part.ctype.mimetype.clone()), name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Direction, MailMessage, Participant, SmsMailFields, write_message_file};

    #[test]
    fn roundtrip_eml_headers_and_body() {
        let msg = MailMessage::sms(SmsMailFields {
            chat_identifier: "+15555550101".into(),
            conversation_type: "individual".into(),
            group_title: None,
            participants: vec![Participant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
            }],
            guid: "aabbccddeeff00112233445566778899".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: Direction::Outgoing,
            service: "sms".into(),
            message_kind: "sms".into(),
            sender_handle: Some("+15555550100".into()),
            sender_display_name: Some("Me".into()),
            owner_handle: "+15555550100".into(),
            subject: None,
            text: "hello roundtrip".into(),
            android_type: Some("2".into()),
            source_fields_json: Some(r#"{"address":"+15555550101"}"#.into()),
            export_source: "sms-backup-restore".into(),
            export_tool: "SMS Backup & Restore".into(),
            export_tool_version: "10.26.003".into(),
            attachments: vec![],
            filename_suffix: None,
        });
        let mut msg = msg;
        msg.owner_display_name = Some("Me".into());

        let tmp = tempfile::tempdir().unwrap();
        let path = write_message_file(&tmp.path().join("chat"), 1, &msg).unwrap();
        let bytes = fs::read(&path).unwrap();
        let parsed = mail_message_from_eml_bytes(&bytes).unwrap();
        assert_eq!(parsed.text, "hello roundtrip");
        assert_eq!(parsed.direction, Direction::Outgoing);
        assert_eq!(parsed.sender_handle.as_deref(), Some("+15555550100"));
        assert_eq!(parsed.owner_handle, "+15555550100");
        assert_eq!(parsed.owner_display_name.as_deref(), Some("Me"));
        assert_eq!(parsed.android_type.as_deref(), Some("2"));
    }

    #[test]
    fn split_mboxrd_unescapes_from() {
        let text = "From me@x Tue May 20 00:00:00 2014\nX-ME-Guid: a\n\n>From spoofed\nbody\n\nFrom me@x Tue May 20 00:01:00 2014\nX-ME-Guid: b\n\nsecond\n\n";
        let records = split_mboxrd(text);
        assert_eq!(records.len(), 2);
        let a = String::from_utf8_lossy(&records[0]);
        assert!(a.contains("From spoofed"));
        assert!(!a.contains(">From spoofed"));
    }
}
