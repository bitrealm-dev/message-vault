//! Per-conversation `.eml` / `.mbox` archive writer.
//!
//! Layout and headers follow the [mail archive format](../../../docs/maintainers/formats/mail-archive.md).
//! Canonical packaging is folders of `.eml`; [`append_message_mbox`] writes
//! derived **mboxrd** mailboxes for clients that prefer a single file.
//! SMS/MMS fill the core fields; iMessage Vault also set reply / tapback /
//! balloon / parts / edits extension fields.

mod parse;

use anyhow::{Context, Result, bail};
use chrono::{Local, TimeZone, Utc};
use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address;
use mail_builder::headers::date::Date;
use mail_builder::headers::text::Text;
use message_csv::conversation_filename;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub use parse::{mail_message_from_eml_bytes, mail_messages_from_mbox};

const MESSAGE_ID_DOMAIN_DEFAULT: &str = "message-vault-io.local";
const MESSAGE_ID_DOMAIN_IMESSAGE: &str = "imessage.local";
const SMS_ADDRESS_DOMAIN: &str = "sms.local";
const HANDLE_ADDRESS_DOMAIN: &str = "handle.local";
const CHAT_ADDRESS_DOMAIN: &str = "chat.local";
const OWNER_DISPLAY_NAME: &str = "Me";

/// Message direction for From/To mapping and `X-ME-Direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

/// One participant in a conversation roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Attachment bytes plus metadata for MIME parts / `X-ME-Attachment-Meta`.
#[derive(Debug, Clone)]
pub struct MailAttachment {
    pub bytes: Vec<u8>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub digest_sha256: Option<String>,
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
}

/// How to package a conversation for mail-archive export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailPackage {
    /// One folder of `.eml` files per conversation.
    EmlFolders,
    /// One `.mbox` (mboxrd) file per conversation.
    Mbox,
}

/// Core SMS/MMS fields for [`MailMessage::sms`] (iMessage extensions left unset).
#[derive(Debug, Clone)]
pub struct SmsMailFields {
    pub chat_identifier: String,
    pub conversation_type: String,
    pub group_title: Option<String>,
    pub participants: Vec<Participant>,
    pub guid: String,
    pub timestamp_unix_ms: i64,
    pub direction: Direction,
    pub service: String,
    pub message_kind: String,
    pub sender_handle: Option<String>,
    pub sender_display_name: Option<String>,
    pub owner_handle: String,
    pub subject: Option<String>,
    pub text: String,
    pub android_type: Option<String>,
    pub source_fields_json: Option<String>,
    pub export_source: String,
    pub export_tool: String,
    pub export_tool_version: String,
    pub attachments: Vec<MailAttachment>,
    /// Optional stem suffix (e.g. `"__whatsapp"`) for conversation folder / mbox names.
    pub filename_suffix: Option<String>,
}

/// One message ready to serialize as a single `.eml`.
#[derive(Debug, Clone)]
pub struct MailMessage {
    pub chat_identifier: String,
    /// `individual` or `group`
    pub conversation_type: String,
    pub group_title: Option<String>,
    pub participants: Vec<Participant>,
    pub guid: String,
    pub timestamp_unix_ms: i64,
    pub direction: Direction,
    pub service: String,
    /// `sms` / `mms` / `imessage` / `tapback` / `balloon` / …
    pub message_kind: String,
    pub sender_handle: Option<String>,
    pub sender_display_name: Option<String>,
    /// Owner E.164 (or handle) used for From/To mapping.
    pub owner_handle: String,
    /// Outgoing From display name; defaults to `"Me"` when absent.
    pub owner_display_name: Option<String>,
    pub subject: Option<String>,
    pub text: String,
    pub android_type: Option<String>,
    pub source_fields_json: Option<String>,
    pub export_source: String,
    pub export_tool: String,
    pub export_tool_version: String,
    pub attachments: Vec<MailAttachment>,
    /// Optional stem suffix (e.g. `"__whatsapp"`) for conversation folder / mbox names.
    pub filename_suffix: Option<String>,
    // --- iMessage extensions (SMS leaves these unset) ---
    pub is_reply: bool,
    pub in_reply_to_guid: Option<String>,
    pub thread_originator_part: Option<u32>,
    pub num_replies: Option<u32>,
    pub is_deleted: bool,
    pub send_effect: Option<String>,
    pub shared_location: Option<String>,
    pub announcement: Option<String>,
    pub read_receipt_rfc3339: Option<String>,
    pub parts_json: Option<String>,
    pub edits_json: Option<String>,
    pub app_json: Option<String>,
    pub balloon_bundle_id: Option<String>,
    pub balloon_kind: Option<String>,
    pub tapbacks_json: Option<String>,
    pub associated_guid: Option<String>,
    pub associated_part: Option<u32>,
    pub tapback_kind: Option<String>,
    pub tapback_emoji: Option<String>,
    pub tapback_action: Option<String>,
}

impl MailMessage {
    /// SMS/MMS-shaped message with all iMessage extension fields cleared.
    pub fn sms(fields: SmsMailFields) -> Self {
        Self {
            chat_identifier: fields.chat_identifier,
            conversation_type: fields.conversation_type,
            group_title: fields.group_title,
            participants: fields.participants,
            guid: fields.guid,
            timestamp_unix_ms: fields.timestamp_unix_ms,
            direction: fields.direction,
            service: fields.service,
            message_kind: fields.message_kind,
            sender_handle: fields.sender_handle,
            sender_display_name: fields.sender_display_name,
            owner_handle: fields.owner_handle,
            owner_display_name: None,
            subject: fields.subject,
            text: fields.text,
            android_type: fields.android_type,
            source_fields_json: fields.source_fields_json,
            export_source: fields.export_source,
            export_tool: fields.export_tool,
            export_tool_version: fields.export_tool_version,
            attachments: fields.attachments,
            filename_suffix: fields.filename_suffix,
            is_reply: false,
            in_reply_to_guid: None,
            thread_originator_part: None,
            num_replies: None,
            is_deleted: false,
            send_effect: None,
            shared_location: None,
            announcement: None,
            read_receipt_rfc3339: None,
            parts_json: None,
            edits_json: None,
            app_json: None,
            balloon_bundle_id: None,
            balloon_kind: None,
            tapbacks_json: None,
            associated_guid: None,
            associated_part: None,
            tapback_kind: None,
            tapback_emoji: None,
            tapback_action: None,
        }
    }
}

/// Remove prior mail-archive artifacts under `output_dir` (`.mbox` files and
/// directories that contain `.eml`). Leaves `attachments/` alone.
pub fn clean_previous_mail_output(output_dir: &Path) -> Result<()> {
    if !output_dir.is_dir() {
        return Ok(());
    }
    for entry in
        fs::read_dir(output_dir).with_context(|| format!("read {}", output_dir.display()))?
    {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mbox"))
        {
            fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
            continue;
        }
        if path.is_dir() && name != "attachments" {
            let has_eml = fs::read_dir(&path)?.filter_map(|e| e.ok()).any(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("eml"))
            });
            if has_eml {
                fs::remove_dir_all(&path).with_context(|| format!("remove {}", path.display()))?;
            }
        }
    }
    Ok(())
}

/// Write one conversation as EML folders or a single mboxrd file.
pub fn write_mail_package(
    output_root: &Path,
    package: MailPackage,
    messages: &[MailMessage],
) -> Result<PathBuf> {
    match package {
        MailPackage::EmlFolders => write_conversation(output_root, messages),
        MailPackage::Mbox => write_conversation_mbox(output_root, messages),
    }
}

#[derive(Serialize)]
struct AttachmentMetaCell<'a> {
    path: Option<&'a str>,
    original_name: Option<&'a str>,
    mime_type: Option<&'a str>,
    is_sticker: bool,
    transcription: Option<&'a str>,
    sticker_effect: Option<&'a str>,
    digest_sha256: Option<&'a str>,
}

/// Conversation directory stem (CSV filename without `.csv`).
fn conversation_stem(msg: &MailMessage) -> String {
    let participant_handles: Vec<String> =
        msg.participants.iter().map(|p| p.handle.clone()).collect();
    let csv_name = conversation_filename(
        &msg.conversation_type,
        &msg.chat_identifier,
        msg.group_title.as_deref(),
        &participant_handles,
        msg.filename_suffix.as_deref(),
    );
    csv_name
        .strip_suffix(".csv")
        .unwrap_or(csv_name.as_str())
        .to_string()
}

/// Write a single `.eml` into an existing conversation directory.
///
/// `sequence` is 1-based (`000001_…`). Creates `conv_dir` if missing.
fn write_message_file(conv_dir: &Path, sequence: u32, msg: &MailMessage) -> Result<PathBuf> {
    if sequence == 0 {
        bail!("write_message_file sequence must be >= 1");
    }
    fs::create_dir_all(conv_dir)
        .with_context(|| format!("create conversation dir {}", conv_dir.display()))?;
    let secs = msg.timestamp_unix_ms.div_euclid(1000);
    let (date_part, time_part) = local_date_time_parts(secs)
        .with_context(|| format!("invalid timestamp_unix_ms {}", msg.timestamp_unix_ms))?;
    let guid8 = guid_prefix8(&msg.guid);
    let filename = format!("{sequence:06}_{date_part}_{time_part}_{guid8}.eml");
    let path = conv_dir.join(&filename);
    let bytes = build_eml(msg)?;
    let mut file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

/// Write one conversation folder of `.eml` files under `output_root`.
///
/// Returns the conversation directory path. Messages are sorted by timestamp,
/// then guid, before emit.
fn write_conversation(output_root: &Path, messages: &[MailMessage]) -> Result<PathBuf> {
    if messages.is_empty() {
        bail!("write_conversation requires at least one message");
    }

    let stem = conversation_stem(&messages[0]);
    let conv_dir = output_root.join(&stem);

    let mut ordered: Vec<&MailMessage> = messages.iter().collect();
    ordered.sort_by(|a, b| {
        a.timestamp_unix_ms
            .cmp(&b.timestamp_unix_ms)
            .then_with(|| a.guid.cmp(&b.guid))
    });

    for (idx, msg) in ordered.iter().enumerate() {
        write_message_file(&conv_dir, (idx + 1) as u32, msg)?;
    }

    Ok(conv_dir)
}

/// Path to the per-conversation mboxrd file (`<stem>.mbox` under `output_root`).
fn conversation_mbox_path(output_root: &Path, msg: &MailMessage) -> PathBuf {
    output_root.join(format!("{}.mbox", conversation_stem(msg)))
}

/// Append one message to a conversation `.mbox` in mboxrd form.
///
/// Creates parent directories and the file if missing. Messages should be
/// appended in chronological order for a usable mailbox.
fn append_message_mbox(mbox_path: &Path, msg: &MailMessage) -> Result<()> {
    if let Some(parent) = mbox_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create mbox parent {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(mbox_path)
        .with_context(|| format!("open mbox {}", mbox_path.display()))?;
    let mut writer = BufWriter::new(file);
    write_mboxrd_record(&mut writer, msg)?;
    writer
        .flush()
        .with_context(|| format!("flush mbox {}", mbox_path.display()))?;
    Ok(())
}

/// Write one conversation `.mbox` under `output_root` (mboxrd).
///
/// Returns the `.mbox` path. Messages are sorted by timestamp, then guid.
fn write_conversation_mbox(output_root: &Path, messages: &[MailMessage]) -> Result<PathBuf> {
    if messages.is_empty() {
        bail!("write_conversation_mbox requires at least one message");
    }

    let path = conversation_mbox_path(output_root, &messages[0]);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("replace existing mbox {}", path.display()))?;
    }

    let mut ordered: Vec<&MailMessage> = messages.iter().collect();
    ordered.sort_by(|a, b| {
        a.timestamp_unix_ms
            .cmp(&b.timestamp_unix_ms)
            .then_with(|| a.guid.cmp(&b.guid))
    });

    for msg in ordered {
        append_message_mbox(&path, msg)?;
    }

    Ok(path)
}

/// Escape a single line for mboxrd: lines matching `^>*From ` get a leading `>`.
fn escape_mboxrd_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'>' {
        i += 1;
    }
    if bytes[i..].starts_with(b"From ") {
        format!(">{line}")
    } else {
        line.to_string()
    }
}

fn write_mboxrd_record(writer: &mut impl Write, msg: &MailMessage) -> Result<()> {
    let eml = build_eml(msg)?;
    let envelope = envelope_sender(msg);
    let asctime = mbox_asctime_utc(msg.timestamp_unix_ms.div_euclid(1000))?;
    writeln!(writer, "From {envelope} {asctime}").context("write mbox From_ line")?;

    let text = String::from_utf8_lossy(&eml);
    // Normalize to LF; strip a single trailing newline so we control the separator.
    let body = text.trim_end_matches(['\r', '\n']);
    for line in body.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        writeln!(writer, "{}", escape_mboxrd_line(line)).context("write mbox body line")?;
    }
    // Blank line between records (mbox convention).
    writeln!(writer).context("write mbox record separator")?;
    Ok(())
}

fn envelope_sender(msg: &MailMessage) -> String {
    let handle = match msg.direction {
        Direction::Incoming => msg
            .sender_handle
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| peer_handle(msg).map(str::trim).filter(|s| !s.is_empty()))
            .unwrap_or("unknown"),
        Direction::Outgoing => {
            let owner = msg.owner_handle.trim();
            if owner.is_empty() { "me" } else { owner }
        }
    };
    // Envelope address must not contain spaces.
    if handle.contains('@') {
        format!("{}@{HANDLE_ADDRESS_DOMAIN}", handle.replace('@', "="))
    } else if handle
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '_' | '.'))
    {
        format!("{handle}@{SMS_ADDRESS_DOMAIN}")
    } else {
        "MAILER-DAEMON@message-vault-io.local".into()
    }
}

fn mbox_asctime_utc(secs: i64) -> Result<String> {
    let dt = Utc
        .timestamp_opt(secs, 0)
        .single()
        .with_context(|| format!("invalid unix timestamp {secs}"))?;
    // Classic mbox asctime: "Wed Jun 30 21:49:08 1993" (UTC).
    Ok(dt.format("%a %b %e %H:%M:%S %Y").to_string())
}

fn local_date_time_parts(secs: i64) -> Option<(String, String)> {
    let local = Local.timestamp_opt(secs, 0).single().or_else(|| {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|utc| Local.from_utc_datetime(&utc.naive_utc()))
    })?;
    Some((
        local.format("%Y-%m-%d").to_string(),
        local.format("%H%M%S").to_string(),
    ))
}

fn guid_prefix8(guid: &str) -> String {
    let hex: String = guid
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect();
    if hex.len() >= 8 {
        hex[..8].to_string()
    } else {
        // Fall back to first 8 chars (not bytes) to avoid panicking on
        // multi-byte UTF-8 characters. Pad with zeros if shorter.
        let prefix: String = guid.chars().take(8).collect();
        if prefix.chars().count() >= 8 {
            prefix
        } else {
            format!("{prefix:0<8}")
        }
    }
}

/// Synthetic RFC5322 address for a phone or Apple handle.
///
/// Phones → `+E164@sms.local`. Email / other handles containing `@` →
/// `local=domain@handle.local` (MAIL_ARCHIVE encoding).
fn synthetic_address(handle: &str, display_name: Option<&str>) -> Address<'static> {
    let handle = handle.trim();
    let email = if handle.is_empty() {
        format!("unknown@{SMS_ADDRESS_DOMAIN}")
    } else if handle.contains('@') {
        let encoded = handle.replace('@', "=");
        format!("{encoded}@{HANDLE_ADDRESS_DOMAIN}")
    } else {
        format!("{handle}@{SMS_ADDRESS_DOMAIN}")
    };
    let name = display_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Address::new_address(name, email)
}

fn owner_address(msg: &MailMessage) -> Address<'static> {
    let handle = msg.owner_handle.trim();
    let handle = if handle.is_empty() { "me" } else { handle };
    let display = msg
        .owner_display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(OWNER_DISPLAY_NAME);
    synthetic_address(handle, Some(display))
}

/// One browseable address for a group chat (roster stays in `X-ME-Participants`).
fn conversation_address(msg: &MailMessage) -> Address<'static> {
    let display = msg
        .group_title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let id = msg.chat_identifier.trim();
            if id.is_empty() { "group" } else { id }
        });
    let local = sanitize_addr_local(msg.chat_identifier.trim()).unwrap_or_else(|| "group".into());
    Address::new_address(
        Some(display.to_string()),
        format!("{local}@{CHAT_ADDRESS_DOMAIN}"),
    )
}

fn sanitize_addr_local(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '_' | '.' | '=') {
            out.push(ch);
        } else if ch == '@' {
            out.push('=');
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn peer_display_name<'a>(msg: &'a MailMessage, peer: &str) -> Option<&'a str> {
    msg.participants
        .iter()
        .find(|p| p.handle == peer)
        .and_then(|p| p.display_name.as_deref())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .or_else(|| {
            msg.sender_display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .filter(|_| msg.sender_handle.as_deref().is_some_and(|h| h == peer))
        })
}

fn message_id_domain(msg: &MailMessage) -> &'static str {
    if msg.service.eq_ignore_ascii_case("imessage")
        || msg.message_kind.eq_ignore_ascii_case("imessage")
    {
        MESSAGE_ID_DOMAIN_IMESSAGE
    } else {
        MESSAGE_ID_DOMAIN_DEFAULT
    }
}

fn peer_handle(msg: &MailMessage) -> Option<&str> {
    if msg.conversation_type.eq_ignore_ascii_case("group") {
        return None;
    }
    msg.participants
        .iter()
        .map(|p| p.handle.as_str())
        .find(|h| *h != msg.owner_handle)
        .or_else(|| {
            let id = msg.chat_identifier.as_str();
            if id != msg.owner_handle {
                Some(id)
            } else {
                None
            }
        })
}

fn build_eml(msg: &MailMessage) -> Result<Vec<u8>> {
    let is_group = msg.conversation_type.eq_ignore_ascii_case("group");
    let (from, to) = if is_group {
        let from = match msg.direction {
            Direction::Incoming => {
                let sender = msg
                    .sender_handle
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("unknown");
                synthetic_address(sender, msg.sender_display_name.as_deref())
            }
            Direction::Outgoing => owner_address(msg),
        };
        (from, conversation_address(msg))
    } else {
        let peer = peer_handle(msg)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let id = msg.chat_identifier.trim();
                if id.is_empty() { "unknown" } else { id }
            });
        let peer_name = peer_display_name(msg, peer);
        match msg.direction {
            Direction::Incoming => (
                synthetic_address(peer, peer_name.or(msg.sender_display_name.as_deref())),
                owner_address(msg),
            ),
            Direction::Outgoing => (owner_address(msg), synthetic_address(peer, peer_name)),
        }
    };

    let subject = mail_subject(msg);
    let date_secs = msg.timestamp_unix_ms.div_euclid(1000);
    let message_id = format!("{}@{}", msg.guid, message_id_domain(msg));

    let mut builder = MessageBuilder::new()
        .from(from)
        .to(to)
        .subject(subject)
        .date(Date::new(date_secs))
        .message_id(message_id)
        .header(
            "X-ME-Chat-Identifier",
            Text::new(msg.chat_identifier.clone()),
        )
        .header(
            "X-ME-Conversation-Type",
            Text::new(msg.conversation_type.clone()),
        )
        .header("X-ME-Direction", Text::new(msg.direction.as_str()))
        .header("X-ME-Service", Text::new(msg.service.clone()))
        .header("X-ME-Message-Kind", Text::new(msg.message_kind.clone()))
        .header(
            "X-ME-Timestamp-Unix-Ms",
            Text::new(msg.timestamp_unix_ms.to_string()),
        )
        .header("X-ME-Guid", Text::new(msg.guid.clone()))
        .header("X-ME-Export-Source", Text::new(msg.export_source.clone()))
        .header("X-ME-Export-Tool", Text::new(msg.export_tool.clone()))
        .header(
            "X-ME-Export-Tool-Version",
            Text::new(msg.export_tool_version.clone()),
        );

    if let Some(title) = msg.group_title.as_deref().filter(|t| !t.is_empty()) {
        builder = builder.header("X-ME-Group-Title", Text::new(title.to_string()));
    }

    if msg.conversation_type.eq_ignore_ascii_case("group") || !msg.participants.is_empty() {
        let participants_json =
            serde_json::to_string(&msg.participants).unwrap_or_else(|_| "[]".into());
        builder = builder.header("X-ME-Participants", Text::new(participants_json));
    }

    if let Some(h) = msg.sender_handle.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Sender-Handle", Text::new(h.to_string()));
    }
    if let Some(n) = msg.sender_display_name.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Sender-Display-Name", Text::new(n.to_string()));
    }
    if !msg.owner_handle.trim().is_empty() {
        builder = builder.header(
            "X-ME-Owner-Handle",
            Text::new(msg.owner_handle.trim().to_string()),
        );
    }
    if let Some(n) = msg.owner_display_name.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Owner-Display-Name", Text::new(n.to_string()));
    }

    if let Some(subj) = msg.subject.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Subject", Text::new(subj.to_string()));
    }
    if let Some(android) = msg.android_type.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Android-Type", Text::new(android.to_string()));
    }
    if let Some(fields) = msg.source_fields_json.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Source-Fields", Text::new(fields.to_string()));
    }

    if msg.is_reply {
        builder = builder.header("X-ME-Is-Reply", Text::new("true"));
    }
    if let Some(guid) = msg.in_reply_to_guid.as_deref().filter(|s| !s.is_empty()) {
        let mid = format!("{guid}@{}", message_id_domain(msg));
        builder = builder
            .in_reply_to(mid.clone())
            .references(mid)
            .header("X-ME-Thread-Originator-Guid", Text::new(guid.to_string()));
    }
    if let Some(part) = msg.thread_originator_part {
        builder = builder.header("X-ME-Thread-Originator-Part", Text::new(part.to_string()));
    }
    if let Some(n) = msg.num_replies {
        builder = builder.header("X-ME-Num-Replies", Text::new(n.to_string()));
    }
    if msg.is_deleted {
        builder = builder.header("X-ME-Is-Deleted", Text::new("true"));
    }
    if let Some(effect) = msg.send_effect.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Send-Effect", Text::new(effect.to_string()));
    }
    if let Some(loc) = msg.shared_location.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Shared-Location", Text::new(loc.to_string()));
    }
    if let Some(ann) = msg.announcement.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Announcement", Text::new(ann.to_string()));
    }
    if let Some(rr) = msg
        .read_receipt_rfc3339
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        builder = builder.header("X-ME-Read-Receipt", Text::new(rr.to_string()));
    }
    if let Some(parts) = msg.parts_json.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Parts", Text::new(parts.to_string()));
    }
    if let Some(edits) = msg.edits_json.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Edits", Text::new(edits.to_string()));
    }
    if let Some(app) = msg.app_json.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-App", Text::new(app.to_string()));
    }
    if let Some(bid) = msg.balloon_bundle_id.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Balloon-Bundle-Id", Text::new(bid.to_string()));
    }
    if let Some(kind) = msg.balloon_kind.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Balloon-Kind", Text::new(kind.to_string()));
    }
    if let Some(tapbacks) = msg.tapbacks_json.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Tapbacks", Text::new(tapbacks.to_string()));
    }
    if let Some(guid) = msg.associated_guid.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Associated-Guid", Text::new(guid.to_string()));
    }
    if let Some(part) = msg.associated_part {
        builder = builder.header("X-ME-Associated-Part", Text::new(part.to_string()));
    }
    if let Some(kind) = msg.tapback_kind.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Tapback-Kind", Text::new(kind.to_string()));
    }
    if let Some(emoji) = msg.tapback_emoji.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Tapback-Emoji", Text::new(emoji.to_string()));
    }
    if let Some(action) = msg.tapback_action.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Tapback-Action", Text::new(action.to_string()));
    }

    if !msg.attachments.is_empty() {
        let meta: Vec<AttachmentMetaCell<'_>> = msg
            .attachments
            .iter()
            .map(|a| AttachmentMetaCell {
                path: None,
                original_name: a.original_name.as_deref(),
                mime_type: a.mime_type.as_deref(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.as_deref(),
                sticker_effect: a.sticker_effect.as_deref(),
                digest_sha256: a.digest_sha256.as_deref(),
            })
            .collect();
        let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "[]".into());
        builder = builder.header("X-ME-Attachment-Meta", Text::new(meta_json));
    }

    builder = builder.text_body(msg.text.clone());

    for (i, att) in msg.attachments.iter().enumerate() {
        let mime = att
            .mime_type
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or("application/octet-stream");
        let filename = att
            .original_name
            .clone()
            .unwrap_or_else(|| format!("attachment-{i}"));
        builder = builder.attachment(mime, filename, att.bytes.clone());
    }

    builder
        .write_to_vec()
        .context("serialize message with mail-builder")
}

/// Stable conversation label for mail `Subject` (never message-body preview).
///
/// Shape: `Message with {peer|group title|chat id}`. SMS/MMS `subject` still
/// goes to `X-ME-Subject` when present.
fn mail_subject(msg: &MailMessage) -> String {
    let with = conversation_subject_label(msg);
    format!("Message with {with}")
}

fn conversation_subject_label(msg: &MailMessage) -> String {
    if msg.conversation_type.eq_ignore_ascii_case("group") {
        if let Some(t) = msg
            .group_title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            return t.to_string();
        }
        let id = msg.chat_identifier.trim();
        if !id.is_empty() {
            return id.to_string();
        }
        return "group".to_string();
    }

    if let Some(peer) = peer_handle(msg).map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(n) = peer_display_name(msg, peer) {
            return n.to_string();
        }
        return peer.to_string();
    }

    let id = msg.chat_identifier.trim();
    if id.is_empty() {
        "unknown".to_string()
    } else {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailparse::MailHeaderMap;

    fn base_sms() -> MailMessage {
        MailMessage {
            chat_identifier: "+15555550101".into(),
            conversation_type: "individual".into(),
            group_title: None,
            participants: vec![Participant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
            }],
            guid: "aabbccddeeff00112233445566778899".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: Direction::Incoming,
            service: "SMS".into(),
            message_kind: "sms".into(),
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            owner_handle: "+15555550100".into(),
            owner_display_name: None,
            subject: None,
            text: "hello from sms".into(),
            android_type: Some("1".into()),
            source_fields_json: Some(r#"{"address":"+15555550101"}"#.into()),
            export_source: "sms-backup-restore".into(),
            export_tool: "SMS Backup & Restore".into(),
            export_tool_version: "10.26.003".into(),
            attachments: vec![],
            filename_suffix: None,
            is_reply: false,
            in_reply_to_guid: None,
            thread_originator_part: None,
            num_replies: None,
            is_deleted: false,
            send_effect: None,
            shared_location: None,
            announcement: None,
            read_receipt_rfc3339: None,
            parts_json: None,
            edits_json: None,
            app_json: None,
            balloon_bundle_id: None,
            balloon_kind: None,
            tapbacks_json: None,
            associated_guid: None,
            associated_part: None,
            tapback_kind: None,
            tapback_emoji: None,
            tapback_action: None,
        }
    }

    #[test]
    fn writes_individual_sms_text_only() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_conversation(tmp.path(), &[base_sms()]).unwrap();
        assert_eq!(dir.file_name().unwrap(), "+15555550101");

        let mut emls: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("eml"))
            .collect();
        emls.sort();
        assert_eq!(emls.len(), 1);
        assert!(
            emls[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("000001_")
        );

        let bytes = fs::read(&emls[0]).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        assert_eq!(
            headers.get_first_value("X-ME-Chat-Identifier").as_deref(),
            Some("+15555550101")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Direction").as_deref(),
            Some("incoming")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Message-Kind").as_deref(),
            Some("sms")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Guid").as_deref(),
            Some("aabbccddeeff00112233445566778899")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Export-Source").as_deref(),
            Some("sms-backup-restore")
        );
        let mid = headers.get_first_value("Message-ID").unwrap();
        assert!(mid.contains("aabbccddeeff00112233445566778899@message-vault-io.local"));
        assert!(!headers.get_first_value("In-Reply-To").is_some());
        let from = headers.get_first_value("From").unwrap();
        assert!(from.contains("Sam"), "From was {from}");
        assert!(from.contains("+15555550101@sms.local"), "From was {from}");
        let to = headers.get_first_value("To").unwrap();
        assert!(to.contains("Me"), "To was {to}");
        assert!(to.contains("+15555550100@sms.local"), "To was {to}");
        assert_eq!(
            headers.get_first_value("Subject").as_deref(),
            Some("Message with Sam")
        );
        let body = mail.get_body().unwrap();
        assert!(body.contains("hello from sms"));
        assert!(!mail.ctype.mimetype.starts_with("multipart/"));
    }

    #[test]
    fn writes_group_mms_with_image_part() {
        let mut msg = base_sms();
        msg.chat_identifier = "chat-group1".into();
        msg.conversation_type = "group".into();
        msg.group_title = Some("Family".into());
        msg.message_kind = "mms".into();
        msg.participants = vec![
            Participant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
            },
            Participant {
                handle: "+15555550102".into(),
                display_name: Some("Alex".into()),
            },
        ];
        msg.attachments = vec![MailAttachment {
            bytes: b"\xff\xd8\xfffakejpeg".to_vec(),
            original_name: Some("photo.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_sha256: Some("deadbeef".into()),
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
        }];

        let tmp = tempfile::tempdir().unwrap();
        let dir = write_conversation(tmp.path(), &[msg]).unwrap();
        assert_eq!(dir.file_name().unwrap(), "Family");

        let eml = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let bytes = fs::read(&eml).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        assert_eq!(
            headers.get_first_value("X-ME-Conversation-Type").as_deref(),
            Some("group")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Group-Title").as_deref(),
            Some("Family")
        );
        assert_eq!(
            headers.get_first_value("Subject").as_deref(),
            Some("Message with Family")
        );
        let to = headers.get_first_value("To").unwrap();
        assert!(to.contains("Family"), "To was {to}");
        assert!(to.contains("chat-group1@chat.local"), "To was {to}");
        assert!(
            !to.contains("+15555550102"),
            "group To should be the chat, not the full roster: {to}"
        );
        let participants = headers.get_first_value("X-ME-Participants").unwrap();
        assert!(participants.contains("+15555550101"));
        assert!(participants.contains("+15555550102"));
        let meta = headers.get_first_value("X-ME-Attachment-Meta").unwrap();
        assert!(meta.contains("photo.jpg"));
        assert!(meta.contains("deadbeef"));
        assert!(mail.ctype.mimetype.starts_with("multipart/"));

        let mut found_image = false;
        fn walk(m: &mailparse::ParsedMail<'_>, found: &mut bool) {
            if m.ctype.mimetype == "image/jpeg" {
                *found = true;
            }
            for sub in &m.subparts {
                walk(sub, found);
            }
        }
        walk(&mail, &mut found_image);
        assert!(found_image, "expected image/jpeg MIME part");
    }

    #[test]
    fn encodes_email_handles_and_imessage_message_id() {
        let mut msg = base_sms();
        msg.chat_identifier = "friend@icloud.com".into();
        msg.participants = vec![Participant {
            handle: "friend@icloud.com".into(),
            display_name: Some("Friend".into()),
        }];
        msg.sender_handle = Some("friend@icloud.com".into());
        msg.owner_handle = "me@icloud.com".into();
        msg.service = "iMessage".into();
        msg.message_kind = "imessage".into();
        msg.guid = "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into();
        msg.export_source = "imessage".into();

        let tmp = tempfile::tempdir().unwrap();
        let path = write_message_file(&tmp.path().join("chat"), 1, &msg).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        let from = headers.get_first_value("From").unwrap();
        assert!(
            from.contains("friend=icloud.com@handle.local"),
            "From was {from}"
        );
        let to = headers.get_first_value("To").unwrap();
        assert!(to.contains("me=icloud.com@handle.local"), "To was {to}");
        let mid = headers.get_first_value("Message-ID").unwrap();
        assert!(
            mid.contains("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE@imessage.local"),
            "Message-ID was {mid}"
        );
    }

    #[test]
    fn outgoing_uses_me_and_stable_subject() {
        let mut msg = base_sms();
        msg.direction = Direction::Outgoing;
        msg.sender_handle = Some("+15555550100".into());
        msg.sender_display_name = Some("Me".into());
        msg.text = "body must not become subject".into();

        let tmp = tempfile::tempdir().unwrap();
        let path = write_message_file(&tmp.path().join("chat"), 1, &msg).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        let from = headers.get_first_value("From").unwrap();
        assert!(from.contains("Me"), "From was {from}");
        let to = headers.get_first_value("To").unwrap();
        assert!(to.contains("Sam"), "To was {to}");
        assert_eq!(
            headers.get_first_value("Subject").as_deref(),
            Some("Message with Sam")
        );
        assert!(
            !headers
                .get_first_value("Subject")
                .unwrap()
                .contains("body must not")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Sender-Handle").as_deref(),
            Some("+15555550100")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Owner-Handle").as_deref(),
            Some("+15555550100")
        );
        assert_eq!(
            headers
                .get_first_value("X-ME-Owner-Display-Name")
                .as_deref(),
            None // unset on base_sms unless set
        );
    }

    #[test]
    fn caller_id_owner_display_and_imessage_extension_headers() {
        let mut msg = base_sms();
        msg.direction = Direction::Outgoing;
        msg.sender_handle = Some("+15555550100".into());
        msg.sender_display_name = Some("+15555550100".into());
        msg.owner_display_name = Some("+15555550100".into());
        msg.export_source = "imessage".into();
        msg.message_kind = "imessage".into();
        msg.is_reply = true;
        msg.in_reply_to_guid = Some("parent-guid-1111".into());
        msg.thread_originator_part = Some(0);
        msg.num_replies = Some(2);
        msg.send_effect = Some("Sent with Balloons".into());
        msg.text = "hello\n\nSent with Balloons".into();
        msg.tapbacks_json = Some(r#"[{"part_index":0,"kind":"loved"}]"#.into());
        msg.parts_json = Some(r#"[{"index":0,"kind":"run","text":"hello"}]"#.into());
        msg.announcement = None;

        let tmp = tempfile::tempdir().unwrap();
        let path = write_message_file(&tmp.path().join("chat"), 1, &msg).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        let from = headers.get_first_value("From").unwrap();
        assert!(from.contains("+15555550100"), "From was {from}");
        assert!(!from.contains("Me <"), "From was {from}");
        assert_eq!(
            headers.get_first_value("X-ME-Sender-Handle").as_deref(),
            Some("+15555550100")
        );
        assert_eq!(
            headers
                .get_first_value("X-ME-Owner-Display-Name")
                .as_deref(),
            Some("+15555550100")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Is-Reply").as_deref(),
            Some("true")
        );
        assert_eq!(
            headers
                .get_first_value("X-ME-Thread-Originator-Guid")
                .as_deref(),
            Some("parent-guid-1111")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Send-Effect").as_deref(),
            Some("Sent with Balloons")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Num-Replies").as_deref(),
            Some("2")
        );
        let irt = headers.get_first_value("In-Reply-To").unwrap();
        assert!(irt.contains("parent-guid-1111@imessage.local"), "{irt}");
        assert!(mail.get_body().unwrap().contains("Sent with Balloons"));
    }

    #[test]
    fn tapback_and_handwriting_svg_headers() {
        let mut msg = base_sms();
        msg.export_source = "imessage".into();
        msg.message_kind = "tapback".into();
        msg.associated_guid = Some("parent-guid".into());
        msg.associated_part = Some(0);
        msg.tapback_kind = Some("loved".into());
        msg.tapback_action = Some("add".into());
        msg.in_reply_to_guid = Some("parent-guid".into());
        msg.text = "Loved a message".into();
        msg.attachments = vec![MailAttachment {
            bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>".to_vec(),
            original_name: Some("handwriting.svg".into()),
            mime_type: Some("image/svg+xml".into()),
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
        }];
        // Handwriting would normally be message_kind=balloon; this only checks MIME.
        let tmp = tempfile::tempdir().unwrap();
        let path = write_message_file(&tmp.path().join("chat"), 1, &msg).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        assert_eq!(
            headers.get_first_value("X-ME-Tapback-Kind").as_deref(),
            Some("loved")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Associated-Guid").as_deref(),
            Some("parent-guid")
        );
        let mut found_svg = false;
        fn walk(m: &mailparse::ParsedMail<'_>, found: &mut bool) {
            if m.ctype.mimetype == "image/svg+xml" {
                *found = true;
            }
            for sub in &m.subparts {
                walk(sub, found);
            }
        }
        walk(&mail, &mut found_svg);
        assert!(found_svg, "expected image/svg+xml MIME part");
    }

    #[test]
    fn escape_mboxrd_from_lines() {
        assert_eq!(escape_mboxrd_line("Hello"), "Hello");
        assert_eq!(escape_mboxrd_line("From me"), ">From me");
        assert_eq!(escape_mboxrd_line(">From me"), ">>From me");
        assert_eq!(escape_mboxrd_line("Fromage"), "Fromage");
    }

    #[test]
    fn writes_conversation_mboxrd() {
        let mut a = base_sms();
        a.text = "first\nFrom spoofed\nlast".into();
        a.timestamp_unix_ms = 1_400_773_261_000;
        let mut b = base_sms();
        b.guid = "bbccddeeff00112233445566778899aa".into();
        b.text = "second".into();
        b.timestamp_unix_ms = 1_400_773_361_000;

        let tmp = tempfile::tempdir().unwrap();
        let path = write_conversation_mbox(tmp.path(), &[b.clone(), a.clone()]).unwrap();
        assert_eq!(path.file_name().unwrap(), "+15555550101.mbox");

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("From "));
        assert!(text.contains(">From spoofed"));
        // Chronological: first then second
        let first_pos = text.find("first").unwrap();
        let second_pos = text.find("second").unwrap();
        assert!(first_pos < second_pos);
        assert_eq!(text.matches("\nFrom ").count(), 1); // one additional From_ between records
        assert!(text.contains("X-ME-Guid: aabbccddeeff00112233445566778899"));
        assert!(text.contains("X-ME-Guid: bbccddeeff00112233445566778899aa"));
    }
}
