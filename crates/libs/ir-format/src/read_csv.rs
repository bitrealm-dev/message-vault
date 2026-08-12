//! Reverse projector: unified CSV → [`ConversationDocument`].

use crate::CSV_HEADERS;
use crate::normalize::{imessage_from_parts, source_from_parts};
use anyhow::{Context, Result, bail};
use message_csv::AttachmentCell;
use message_ir::{
    ConversationDocument, ConversationHeader, ConversationMeta, ConversationStats, ExportMeta,
    HandleType, IrAttachment, IrConversationType, IrDirection, IrImessage, IrMessage,
    IrMessageKind, IrParticipant, IrService, SCHEMA_VERSION, parse_android_type,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct ParticipantCell {
    handle: String,
    #[serde(default)]
    display_name: String,
    /// Absent (legacy cells) → `Some(HandleType::Other)`; explicit `null` →
    /// `None`; any other string is parsed leniently via [`HandleType::parse`].
    #[serde(
        default = "default_participant_handle_type",
        deserialize_with = "deserialize_handle_type"
    )]
    handle_type: Option<HandleType>,
}

fn default_participant_handle_type() -> Option<HandleType> {
    Some(HandleType::Other)
}

fn deserialize_handle_type<'de, D>(de: D) -> Result<Option<HandleType>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<String>::deserialize(de)?;
    Ok(s.map(|s| HandleType::parse(&s)))
}

/// Read a conversation CSV written by [`crate::write_conversation_csv`].
///
/// Conversation / export header is taken from the first data row.
pub fn read_conversation_csv(path: &Path) -> Result<ConversationDocument> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(BufReader::new(file));

    let headers = rdr
        .headers()
        .with_context(|| format!("read CSV headers {}", path.display()))?
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    validate_headers(&headers)?;

    let mut rows = Vec::new();
    for (i, result) in rdr.records().enumerate() {
        let record =
            result.with_context(|| format!("read CSV row {} in {}", i + 1, path.display()))?;
        rows.push(record);
    }
    if rows.is_empty() {
        bail!("CSV has no data rows: {}", path.display());
    }

    let header = header_from_row(&headers, &rows[0])?;
    let packaging_stem_suffix = path
        .file_stem()
        .and_then(|n| n.to_str())
        .and_then(crate::util::packaging_suffix_from_stem);

    let mut messages = Vec::with_capacity(rows.len());
    for (i, record) in rows.iter().enumerate() {
        messages.push(
            message_from_record(&headers, record)
                .with_context(|| format!("parse CSV row {} in {}", i + 1, path.display()))?,
        );
    }

    let mut doc = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: header.export,
        conversation: header.conversation,
        messages,
        packaging_stem_suffix,
    };
    doc.finalize_stats();
    Ok(doc)
}

fn header_from_row(headers: &[String], row: &csv::StringRecord) -> Result<ConversationHeader> {
    let get = |name: &str| cell(headers, row, name).unwrap_or("");
    let mut participants = parse_participants(get("participants_json"));
    // Legacy files predate handle_type in the participants cell. For
    // single-participant conversations, fall back to the per-row
    // `handle_type` column (the sender's inferred type) so the peer keeps
    // a type. Group chats have no single type, so they are left untouched.
    if participants.len() == 1 && participants[0].handle_type.is_none() {
        if let Some(t) = parse_handle_type_cell(get("handle_type")) {
            participants[0].handle_type = Some(t);
        }
    }
    let group_title = {
        let t = get("group_title");
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };
    Ok(ConversationHeader {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: get("export_source").to_string(),
            tool: get("export_tool").to_string(),
            tool_version: get("export_tool_version").to_string(),
            owner_handle: nonempty(get("owner_handle")),
            owner_display_name: nonempty(get("owner_display_name")),
        },
        conversation: ConversationMeta {
            chat_identifier: get("chat_identifier").to_string(),
            conversation_type: IrConversationType::parse(get("conversation_type")),
            group_title,
            participants,
            stats: ConversationStats::default(),
        },
    })
}

fn message_from_record(headers: &[String], row: &csv::StringRecord) -> Result<IrMessage> {
    let get = |name: &str| cell(headers, row, name).unwrap_or("");
    let timestamp_unix_ms = get("timestamp_unix_ms")
        .parse::<i64>()
        .with_context(|| format!("bad timestamp_unix_ms {:?}", get("timestamp_unix_ms")))?;
    let direction = match get("direction").to_ascii_lowercase().as_str() {
        "outgoing" => IrDirection::Outgoing,
        _ => IrDirection::Incoming,
    };
    let attachments = parse_attachments(get("attachments_json"))?;
    let source = source_from_parts(
        parse_android_type(get("android_type")),
        get("source_fields_json"),
    );

    let is_reply = parse_bool(get("is_reply"));
    let is_deleted = parse_bool(get("is_deleted"));
    let thread_originator_part = {
        let s = get("thread_originator_part");
        if s.is_empty() { None } else { s.parse().ok() }
    };
    let num_replies = {
        let s = get("num_replies");
        if s.is_empty() { None } else { s.parse().ok() }
    };
    let associated_part = {
        let s = get("associated_part");
        if s.is_empty() { None } else { s.parse().ok() }
    };
    let imessage = imessage_from_parts(IrImessage {
        is_reply,
        in_reply_to_guid: nonempty(get("thread_originator_guid")),
        thread_originator_part,
        num_replies,
        is_deleted,
        send_effect: nonempty(get("send_effect")),
        shared_location: nonempty(get("shared_location")),
        announcement: nonempty(get("announcement")),
        read_receipt_rfc3339: nonempty(get("read_receipt")),
        parts: parse_json_cell(get("parts_json")),
        edits: parse_json_cell(get("edits_json")),
        tapbacks: parse_json_cell(get("tapbacks_json")),
        app: parse_json_cell(get("app_json")),
        balloon_bundle_id: nonempty(get("balloon_bundle_id")),
        balloon_kind: nonempty(get("balloon_kind")),
        associated_guid: nonempty(get("associated_guid")),
        associated_part,
        tapback_kind: nonempty(get("tapback_kind")),
        tapback_emoji: nonempty(get("tapback_emoji")),
        tapback_action: nonempty(get("tapback_action")),
    });

    Ok(IrMessage {
        guid: get("guid").to_string(),
        timestamp_unix_ms,
        direction,
        service: IrService::parse(get("service")),
        message_kind: IrMessageKind::parse(get("message_kind")),
        sender_handle: nonempty(get("sender_handle")),
        sender_display_name: nonempty(get("sender_display_name")),
        subject: nonempty(get("subject")),
        text: get("text").to_string(),
        attachments,
        imessage,
        source,
    })
}

fn validate_headers(headers: &[String]) -> Result<()> {
    let set: HashMap<&str, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.as_str(), i))
        .collect();
    for required in CSV_HEADERS {
        if !set.contains_key(required) {
            bail!("CSV missing required column `{required}`");
        }
    }
    Ok(())
}

fn cell<'a>(headers: &[String], row: &'a csv::StringRecord, name: &str) -> Option<&'a str> {
    let idx = headers.iter().position(|h| h == name)?;
    row.get(idx)
}

fn nonempty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

fn parse_json_cell(s: &str) -> Option<Value> {
    let t = s.trim();
    if t.is_empty() || t == "null" {
        return None;
    }
    serde_json::from_str(t).ok()
}

fn parse_participants(raw: &str) -> Vec<IrParticipant> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    let cells: Vec<ParticipantCell> = serde_json::from_str(raw).unwrap_or_default();
    cells
        .into_iter()
        .map(|p| IrParticipant {
            handle: p.handle,
            display_name: if p.display_name.is_empty() {
                None
            } else {
                Some(p.display_name)
            },
            handle_type: p.handle_type,
        })
        .collect()
}

/// Parse the dedicated `handle_type` column cell (empty → `None`).
fn parse_handle_type_cell(raw: &str) -> Option<HandleType> {
    let t = raw.trim();
    if t.is_empty() {
        None
    } else {
        Some(HandleType::parse(t))
    }
}

fn parse_attachments(raw: &str) -> Result<Vec<IrAttachment>> {
    if raw.trim().is_empty() || raw.trim() == "null" {
        return Ok(Vec::new());
    }
    let cells: Vec<AttachmentCell> =
        serde_json::from_str(raw).with_context(|| format!("parse attachments_json: {raw}"))?;
    Ok(cells
        .into_iter()
        .map(|a| IrAttachment {
            path: a.path,
            original_name: a.original_name,
            mime_type: a.mime_type,
            digest_sha256: a.digest_sha256,
            is_sticker: a.is_sticker,
            transcription: a.transcription,
            sticker_effect: a.sticker_effect,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        })
        .collect())
}
