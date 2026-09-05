//! Per-conversation `.eml` / `.mbox` archive writer.
//!
//! Layout and headers follow the [mail archive format](https://bitrealm.io/vault/developer/formats/mail-archive/).
//! The usual layout is one folder of `.eml` files per conversation.
//! [`write_mail_package`] writes **mboxrd** mailboxes for clients that prefer
//! a single file. SMS/MMS fill the core fields. iMessage also sets reply,
//! tapback, balloon, parts, and edits extension fields.

#![warn(missing_docs)]

mod headers;
mod parse;

use anyhow::{Context, Result, bail};
use chrono::{Local, TimeZone, Utc};
use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address;
use mail_builder::headers::date::Date;
use mail_builder::headers::text::Text;
use message_ir::{IrDirection, IrMessage};
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

/// One participant in a conversation roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Phone, email, or chat handle; also used for peer matching in From/To mapping.
    pub handle: String,
    /// Optional display name, omitted from the JSON header when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Attachment bytes plus metadata for MIME parts / `X-ME-Attachment-Meta`.
#[derive(Debug, Clone)]
pub struct MailAttachment {
    /// Raw file bytes attached as a MIME part.
    pub bytes: Vec<u8>,
    /// Shared attachment metadata (`path` is never serialized to the EML; readers
    /// restore the IR path separately).
    pub meta: message_ir::AttachmentMeta,
    /// Sticker flag serialized in the attachment meta JSON.
    pub is_sticker: bool,
    /// OCR/transcription text serialized in the attachment meta JSON.
    pub transcription: Option<String>,
    /// Sticker effect name serialized in the attachment meta JSON.
    pub sticker_effect: Option<String>,
}

impl From<&MailAttachment> for message_ir::IrAttachment {
    fn from(a: &MailAttachment) -> Self {
        Self {
            path: None,
            original_name: a.meta.original_name.clone(),
            mime_type: a.meta.mime_type.clone(),
            digest_sha256: a.meta.digest_sha256.clone(),
            is_sticker: a.is_sticker,
            transcription: a.transcription.clone(),
            sticker_effect: a.sticker_effect.clone(),
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }
}

/// How to package a conversation for mail-archive export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailPackage {
    /// One folder of `.eml` files per conversation.
    EmlFolders,
    /// One `.mbox` (mboxrd) file per conversation.
    Mbox,
}

/// One message ready to serialize as a single `.eml`: the conversation
/// context the headers need, plus the IR message itself.
///
/// The message-level fields (guid, timestamp, direction, text, the iMessage
/// extension bag, the Android source bag) live in [`message_ir::IrMessage`];
/// the writer reads them straight from the IR instead of a flattened copy.
#[derive(Debug, Clone)]
pub struct MailMessage {
    /// Conversation id → `X-ME-Chat-Identifier`, folder stem, group chat address local part.
    pub chat_identifier: String,
    /// `individual` or `group`.
    pub conversation_type: String,
    /// Group title → `X-ME-Group-Title`, To display name, subject label.
    pub group_title: Option<String>,
    /// Roster → `X-ME-Participants` JSON.
    pub participants: Vec<Participant>,
    /// Owner E.164 (or handle) used for From/To mapping.
    pub owner_handle: String,
    /// Outgoing From display name; defaults to `"Me"` when absent.
    pub owner_display_name: Option<String>,
    /// → `X-ME-Export-Source`.
    pub export_source: String,
    /// → `X-ME-Export-Tool`.
    pub export_tool: String,
    /// → `X-ME-Export-Tool-Version`.
    pub export_tool_version: String,
    /// Optional stem suffix (e.g. `"__whatsapp"`) for conversation folder / mbox names.
    pub filename_suffix: Option<String>,
    /// The message itself (headers read guid, timestamp, direction, service,
    /// kind, sender, subject, text, and the iMessage / source bags from here;
    /// its `attachments` list is ignored in favour of `attachments` below).
    pub message: IrMessage,
    /// MIME parts plus the `X-ME-Attachment-Meta` JSON (bytes loaded).
    pub attachments: Vec<MailAttachment>,
}

impl MailMessage {
    /// The iMessage extension bag, when present.
    fn im(&self) -> Option<&message_ir::IrImessage> {
        self.message.imessage.as_ref()
    }
}

/// Remove prior mail-archive artifacts under `output_dir` (`.mbox` files and
/// directories that contain `.eml`). Leaves `attachments/` alone.
///
/// # Errors
///
/// Returns an error when a directory cannot be read or a file cannot be removed.
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
///
/// # Errors
///
/// Returns an error when the directory or file cannot be written.
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

/// Conversation directory stem (shared per-conversation filename stem).
fn conversation_stem(msg: &MailMessage) -> String {
    let participant_handles: Vec<String> =
        msg.participants.iter().map(|p| p.handle.clone()).collect();
    message_ir::conversation_stem(
        &msg.conversation_type,
        &msg.chat_identifier,
        msg.group_title.as_deref(),
        &participant_handles,
        msg.filename_suffix.as_deref(),
    )
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
    let secs = msg.message.timestamp_unix_ms.div_euclid(1000);
    let (date_part, time_part) = local_date_time_parts(secs).with_context(|| {
        format!(
            "invalid timestamp_unix_ms {}",
            msg.message.timestamp_unix_ms
        )
    })?;
    let guid8 = guid_prefix8(&msg.message.guid);
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
/// then guid, before writing.
fn write_conversation(output_root: &Path, messages: &[MailMessage]) -> Result<PathBuf> {
    if messages.is_empty() {
        bail!("write_conversation requires at least one message");
    }

    let stem = conversation_stem(&messages[0]);
    let conv_dir = output_root.join(&stem);

    let mut ordered: Vec<&MailMessage> = messages.iter().collect();
    ordered.sort_by(|a, b| {
        a.message
            .timestamp_unix_ms
            .cmp(&b.message.timestamp_unix_ms)
            .then_with(|| a.message.guid.cmp(&b.message.guid))
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
        a.message
            .timestamp_unix_ms
            .cmp(&b.message.timestamp_unix_ms)
            .then_with(|| a.message.guid.cmp(&b.message.guid))
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

/// Write one message as an mboxrd record: the `From_` line, then the EML with body `From ` lines escaped.
fn write_mboxrd_record(writer: &mut impl Write, msg: &MailMessage) -> Result<()> {
    let eml = build_eml(msg)?;
    let envelope = envelope_sender(msg);
    let asctime = mbox_asctime_utc(msg.message.timestamp_unix_ms.div_euclid(1000))?;
    writeln!(writer, "From {envelope} {asctime}").context("write mbox From_ line")?;

    let text = String::from_utf8_lossy(&eml);
    // Convert CRLF to LF. Strip a single trailing newline so the writer
    // can add the mbox record separator.
    let body = text.trim_end_matches(['\r', '\n']);
    for line in body.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        writeln!(writer, "{}", escape_mboxrd_line(line)).context("write mbox body line")?;
    }
    // Blank line between records (mbox convention).
    writeln!(writer).context("write mbox record separator")?;
    Ok(())
}

/// The address for the mbox `From_` line: the sender for incoming, the owner otherwise.
fn envelope_sender(msg: &MailMessage) -> String {
    let handle = match msg.message.direction {
        IrDirection::Incoming => msg
            .message
            .sender_handle
            .as_deref()
            .and_then(message_ir::trimmed)
            .or_else(|| peer_handle(msg).and_then(message_ir::trimmed))
            .unwrap_or("unknown"),
        IrDirection::Outgoing => {
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

/// The classic `Wed Jun 30 21:49:08 1993` form of a timestamp, in UTC.
fn mbox_asctime_utc(secs: i64) -> Result<String> {
    let dt = Utc
        .timestamp_opt(secs, 0)
        .single()
        .with_context(|| format!("invalid unix timestamp {secs}"))?;
    // Classic mbox asctime: "Wed Jun 30 21:49:08 1993" (UTC).
    Ok(dt.format("%a %b %e %H:%M:%S %Y").to_string())
}

/// Local date and time strings for a Unix timestamp, or `None` when it cannot be represented.
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

/// The first eight hex characters of a guid, for file names.
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
    let name = display_name.and_then(message_ir::nonempty);
    Address::new_address(name, email)
}

/// The owner's address: their handle (or `me`) with the display name `Me`.
fn owner_address(msg: &MailMessage) -> Address<'static> {
    let handle = msg.owner_handle.trim();
    let handle = if handle.is_empty() { "me" } else { handle };
    let display = msg
        .owner_display_name
        .as_deref()
        .and_then(message_ir::trimmed)
        .unwrap_or(OWNER_DISPLAY_NAME);
    synthetic_address(handle, Some(display))
}

/// One browseable address for a group chat (roster stays in `X-ME-Participants`).
fn conversation_address(msg: &MailMessage) -> Address<'static> {
    let display = msg
        .group_title
        .as_deref()
        .and_then(message_ir::trimmed)
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

/// An address local part with the characters an email local part cannot hold replaced.
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

/// The display name the participants list gives for `peer`.
fn peer_display_name<'a>(msg: &'a MailMessage, peer: &str) -> Option<&'a str> {
    msg.participants
        .iter()
        .find(|p| p.handle == peer)
        .and_then(|p| p.display_name.as_deref())
        .and_then(message_ir::trimmed)
        .or_else(|| {
            msg.message
                .sender_display_name
                .as_deref()
                .and_then(message_ir::trimmed)
                .filter(|_| {
                    msg.message
                        .sender_handle
                        .as_deref()
                        .is_some_and(|h| h == peer)
                })
        })
}

/// The Message-ID domain: `imessage.local` for iMessage rows, else the default.
fn message_id_domain(msg: &MailMessage) -> &'static str {
    if msg
        .message
        .service
        .as_str()
        .eq_ignore_ascii_case("imessage")
        || msg
            .message
            .message_kind
            .as_str()
            .eq_ignore_ascii_case("imessage")
    {
        MESSAGE_ID_DOMAIN_IMESSAGE
    } else {
        MESSAGE_ID_DOMAIN_DEFAULT
    }
}

/// The other party's handle in a 1:1 conversation; groups have none.
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

/// JSON text for a header cell; `None` for null/absent values.
fn value_as_string(v: Option<&serde_json::Value>) -> Option<String> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    Some(serde_json::to_string(v).unwrap_or_default()).filter(|s| !s.is_empty())
}

/// Add `name: value` when the value is present and non-empty.
fn opt_header<'m>(
    builder: MessageBuilder<'m>,
    name: &'static str,
    value: Option<&str>,
) -> MessageBuilder<'m> {
    match value.filter(|s| !s.is_empty()) {
        Some(v) => builder.header(name, Text::new(v.to_string())),
        None => builder,
    }
}

/// Serialize one message as an RFC 5322 `.eml`: envelope addresses, the
/// Message Vault headers every source carries, the iMessage-only headers,
/// then the text body and one MIME part per attachment.
fn build_eml(msg: &MailMessage) -> Result<Vec<u8>> {
    let (from, to) = envelope_addresses(msg);
    let date_secs = msg.message.timestamp_unix_ms.div_euclid(1000);
    let message_id = format!("{}@{}", msg.message.guid, message_id_domain(msg));
    let mut builder = MessageBuilder::new()
        .from(from)
        .to(to)
        .subject(mail_subject(msg))
        .date(Date::new(date_secs))
        .message_id(message_id);
    builder = conversation_headers(builder, msg);
    builder = imessage_headers(builder, msg);
    builder = attachment_meta_header(builder, msg);
    builder = builder.text_body(msg.message.text.clone());
    for (i, att) in msg.attachments.iter().enumerate() {
        let mime = att
            .meta
            .mime_type
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or("application/octet-stream");
        let filename = att
            .meta
            .original_name
            .clone()
            .unwrap_or_else(|| format!("attachment-{i}"));
        builder = builder.attachment(mime, filename, att.bytes.clone());
    }
    builder
        .write_to_vec()
        .context("serialize message with mail-builder")
}

/// Who the mail is from and to.
///
/// A group chat is addressed as the chat itself, with the sender (or the
/// owner) on the other side. A 1:1 chat is addressed peer-to-owner or
/// owner-to-peer by direction.
fn envelope_addresses(msg: &MailMessage) -> (Address<'static>, Address<'static>) {
    if msg.conversation_type.eq_ignore_ascii_case("group") {
        let from = match msg.message.direction {
            IrDirection::Incoming => {
                let sender = msg
                    .message
                    .sender_handle
                    .as_deref()
                    .and_then(message_ir::trimmed)
                    .unwrap_or("unknown");
                synthetic_address(sender, msg.message.sender_display_name.as_deref())
            }
            IrDirection::Outgoing => owner_address(msg),
        };
        return (from, conversation_address(msg));
    }
    let peer = peer_handle(msg)
        .and_then(message_ir::trimmed)
        .unwrap_or_else(|| {
            let id = msg.chat_identifier.trim();
            if id.is_empty() { "unknown" } else { id }
        });
    let peer_name = peer_display_name(msg, peer);
    match msg.message.direction {
        IrDirection::Incoming => (
            synthetic_address(
                peer,
                peer_name.or(msg.message.sender_display_name.as_deref()),
            ),
            owner_address(msg),
        ),
        IrDirection::Outgoing => (owner_address(msg), synthetic_address(peer, peer_name)),
    }
}

/// Append every `Some` value as a header, in the order given.
fn optional_headers<'m>(
    mut builder: MessageBuilder<'m>,
    values: impl IntoIterator<Item = (&'static str, Option<String>)>,
) -> MessageBuilder<'m> {
    for (name, value) in values {
        builder = opt_header(builder, name, value.as_deref());
    }
    builder
}

/// The headers that identify the conversation, the export, and the people in
/// it. The first block is always present; the rest appear when the source
/// recorded them.
fn conversation_headers<'m>(builder: MessageBuilder<'m>, msg: &MailMessage) -> MessageBuilder<'m> {
    let mut builder = builder
        .header(
            headers::CHAT_IDENTIFIER,
            Text::new(msg.chat_identifier.clone()),
        )
        .header(
            headers::CONVERSATION_TYPE,
            Text::new(msg.conversation_type.clone()),
        )
        .header(
            headers::DIRECTION,
            Text::new(msg.message.direction.as_str()),
        )
        .header(
            headers::SERVICE,
            Text::new(msg.message.service.as_str().to_string()),
        )
        .header(
            headers::MESSAGE_KIND,
            Text::new(msg.message.message_kind.as_str().to_string()),
        )
        .header(
            headers::TIMESTAMP_UNIX_MS,
            Text::new(msg.message.timestamp_unix_ms.to_string()),
        )
        .header(headers::GUID, Text::new(msg.message.guid.clone()))
        .header(headers::EXPORT_SOURCE, Text::new(msg.export_source.clone()))
        .header(headers::EXPORT_TOOL, Text::new(msg.export_tool.clone()))
        .header(
            headers::EXPORT_TOOL_VERSION,
            Text::new(msg.export_tool_version.clone()),
        );
    builder = opt_header(builder, headers::GROUP_TITLE, msg.group_title.as_deref());
    if msg.conversation_type.eq_ignore_ascii_case("group") || !msg.participants.is_empty() {
        let participants_json =
            serde_json::to_string(&msg.participants).unwrap_or_else(|_| "[]".into());
        builder = builder.header(headers::PARTICIPANTS, Text::new(participants_json));
    }
    let source = msg.message.source.as_ref();
    optional_headers(
        builder,
        [
            (headers::SENDER_HANDLE, msg.message.sender_handle.clone()),
            (
                headers::SENDER_DISPLAY_NAME,
                msg.message.sender_display_name.clone(),
            ),
            (
                headers::OWNER_HANDLE,
                Some(msg.owner_handle.trim().to_string()),
            ),
            (headers::OWNER_DISPLAY_NAME, msg.owner_display_name.clone()),
            (headers::SUBJECT, msg.message.subject.clone()),
            (
                headers::ANDROID_TYPE,
                source
                    .and_then(|src| src.android_type)
                    .map(|t| t.to_string()),
            ),
            (
                headers::SOURCE_FIELDS,
                source
                    .filter(|src| !src.fields.is_empty())
                    .map(|src| serde_json::to_string(&src.fields).unwrap_or_default()),
            ),
        ],
    )
}

/// The headers only iMessage rows carry: reply threading, effects, edits,
/// tapbacks, and app balloons. Rows from other services add nothing here.
fn imessage_headers<'m>(builder: MessageBuilder<'m>, msg: &MailMessage) -> MessageBuilder<'m> {
    let Some(im) = msg.im() else {
        return builder;
    };
    let mut builder = opt_header(builder, headers::IS_REPLY, im.is_reply.then_some("true"));
    if let Some(guid) = im.in_reply_to_guid.as_deref().filter(|s| !s.is_empty()) {
        let mid = format!("{guid}@{}", message_id_domain(msg));
        builder = builder
            .in_reply_to(mid.clone())
            .references(mid)
            .header(headers::THREAD_ORIGINATOR_GUID, Text::new(guid.to_string()));
    }
    optional_headers(
        builder,
        [
            (
                headers::THREAD_ORIGINATOR_PART,
                im.thread_originator_part.map(|p| p.to_string()),
            ),
            (headers::NUM_REPLIES, im.num_replies.map(|n| n.to_string())),
            (
                headers::IS_DELETED,
                im.is_deleted.then(|| "true".to_string()),
            ),
            (headers::SEND_EFFECT, im.send_effect.clone()),
            (headers::SHARED_LOCATION, im.shared_location.clone()),
            (headers::ANNOUNCEMENT, im.announcement.clone()),
            (headers::READ_RECEIPT, im.read_receipt_rfc3339.clone()),
            (headers::PARTS, value_as_string(im.parts.as_ref())),
            (headers::EDITS, value_as_string(im.edits.as_ref())),
            (headers::APP, value_as_string(im.app.as_ref())),
            (headers::BALLOON_BUNDLE_ID, im.balloon_bundle_id.clone()),
            (headers::BALLOON_KIND, im.balloon_kind.clone()),
            (headers::TAPBACKS, value_as_string(im.tapbacks.as_ref())),
            (headers::ASSOCIATED_GUID, im.associated_guid.clone()),
            (
                headers::ASSOCIATED_PART,
                im.associated_part.map(|p| p.to_string()),
            ),
            (headers::TAPBACK_KIND, im.tapback_kind.clone()),
            (headers::TAPBACK_EMOJI, im.tapback_emoji.clone()),
            (headers::TAPBACK_ACTION, im.tapback_action.clone()),
        ],
    )
}

/// One JSON header listing every attachment's metadata, so a reader can see
/// what was attached without decoding the MIME parts.
fn attachment_meta_header<'m>(
    builder: MessageBuilder<'m>,
    msg: &MailMessage,
) -> MessageBuilder<'m> {
    if msg.attachments.is_empty() {
        return builder;
    }
    let meta: Vec<AttachmentMetaCell<'_>> = msg
        .attachments
        .iter()
        .map(|a| AttachmentMetaCell {
            path: None,
            original_name: a.meta.original_name.as_deref(),
            mime_type: a.meta.mime_type.as_deref(),
            is_sticker: a.is_sticker,
            transcription: a.transcription.as_deref(),
            sticker_effect: a.sticker_effect.as_deref(),
            digest_sha256: a.meta.digest_sha256.as_deref(),
        })
        .collect();
    let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "[]".into());
    builder.header(headers::ATTACHMENT_META, Text::new(meta_json))
}

/// Stable conversation label for mail `Subject` (never message-body preview).
///
/// Shape: `Message with {peer|group title|chat id}`. SMS/MMS `subject` still
/// goes to `X-ME-Subject` when present.
fn mail_subject(msg: &MailMessage) -> String {
    let with = conversation_subject_label(msg);
    format!("Message with {with}")
}

/// Who the subject names: the group title (or member list), or the peer.
fn conversation_subject_label(msg: &MailMessage) -> String {
    if msg.conversation_type.eq_ignore_ascii_case("group") {
        if let Some(t) = msg.group_title.as_deref().and_then(message_ir::trimmed) {
            return t.to_string();
        }
        let id = msg.chat_identifier.trim();
        if !id.is_empty() {
            return id.to_string();
        }
        return "group".to_string();
    }

    if let Some(peer) = peer_handle(msg).and_then(message_ir::trimmed) {
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
            owner_handle: "+15555550100".into(),
            owner_display_name: None,
            export_source: "sms-backup-restore".into(),
            export_tool: "SMS Backup & Restore".into(),
            export_tool_version: "10.26.003".into(),
            filename_suffix: None,
            message: IrMessage {
                guid: "aabbccddeeff00112233445566778899".into(),
                timestamp_unix_ms: 1_400_773_261_000,
                direction: IrDirection::Incoming,
                service: message_ir::IrService::Sms,
                message_kind: message_ir::IrMessageKind::Sms,
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: "hello from sms".into(),
                attachments: Vec::new(),
                imessage: None,
                source: Some(message_ir::IrSource {
                    android_type: Some(1),
                    fields: serde_json::from_str(r#"{"address":"+15555550101"}"#).unwrap(),
                }),
            },
            attachments: vec![],
        }
    }

    /// The extension bag, creating it when the base message has none.
    fn im_mut(msg: &mut MailMessage) -> &mut message_ir::IrImessage {
        msg.message.imessage.get_or_insert_with(Default::default)
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
        assert!(headers.get_first_value("In-Reply-To").is_none());
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
        msg.message.message_kind = message_ir::IrMessageKind::Mms;
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
            meta: message_ir::AttachmentMeta {
                path: None,
                original_name: Some("photo.jpg".into()),
                mime_type: Some("image/jpeg".into()),
                digest_sha256: Some("deadbeef".into()),
            },
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
        msg.message.sender_handle = Some("friend@icloud.com".into());
        msg.owner_handle = "me@icloud.com".into();
        msg.message.service = message_ir::IrService::IMessage;
        msg.message.message_kind = message_ir::IrMessageKind::IMessage;
        msg.message.guid = "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into();
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
        msg.message.direction = IrDirection::Outgoing;
        msg.message.sender_handle = Some("+15555550100".into());
        msg.message.sender_display_name = Some("Me".into());
        msg.message.text = "body must not become subject".into();

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
        msg.message.direction = IrDirection::Outgoing;
        msg.message.sender_handle = Some("+15555550100".into());
        msg.message.sender_display_name = Some("+15555550100".into());
        msg.owner_display_name = Some("+15555550100".into());
        msg.export_source = "imessage".into();
        msg.message.message_kind = message_ir::IrMessageKind::IMessage;
        im_mut(&mut msg).is_reply = true;
        im_mut(&mut msg).in_reply_to_guid = Some("parent-guid-1111".into());
        im_mut(&mut msg).thread_originator_part = Some(0);
        im_mut(&mut msg).num_replies = Some(2);
        im_mut(&mut msg).send_effect = Some("Sent with Balloons".into());
        msg.message.text = "hello\n\nSent with Balloons".into();
        im_mut(&mut msg).tapbacks =
            serde_json::from_str(r#"[{"part_index":0,"kind":"loved"}]"#).ok();
        im_mut(&mut msg).parts =
            serde_json::from_str(r#"[{"index":0,"kind":"run","text":"hello"}]"#).ok();
        im_mut(&mut msg).announcement = None;

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
        msg.message.message_kind = message_ir::IrMessageKind::Tapback;
        im_mut(&mut msg).associated_guid = Some("parent-guid".into());
        im_mut(&mut msg).associated_part = Some(0);
        im_mut(&mut msg).tapback_kind = Some("loved".into());
        im_mut(&mut msg).tapback_action = Some("add".into());
        im_mut(&mut msg).in_reply_to_guid = Some("parent-guid".into());
        msg.message.text = "Loved a message".into();
        msg.attachments = vec![MailAttachment {
            bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>".to_vec(),
            meta: message_ir::AttachmentMeta {
                path: None,
                original_name: Some("handwriting.svg".into()),
                mime_type: Some("image/svg+xml".into()),
                digest_sha256: None,
            },
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
        a.message.text = "first\nFrom spoofed\nlast".into();
        a.message.timestamp_unix_ms = 1_400_773_261_000;
        let mut b = base_sms();
        b.message.guid = "bbccddeeff00112233445566778899aa".into();
        b.message.text = "second".into();
        b.message.timestamp_unix_ms = 1_400_773_361_000;

        let tmp = tempfile::tempdir().unwrap();
        let path = write_conversation_mbox(tmp.path(), &[b, a]).unwrap();
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
