//! Decode-oriented WAP-209 / WAP-230 helpers for MMS binary PDUs.
//!
//! Algorithm reference (not a dependency / not copied): OMA WAP-209 MMS Encapsulation,
//! WAP-230 WSP, and the decode path concepts in python-messaging's `messaging.mms`.
//!
//! # GO SMS Pro fragments
//!
//! Backups often store a partial header (From/To + named text) rather than a full
//! `m-retrieve-conf`. [`decode_mms_best_effort`] merges a strict header walk with
//! fragment scanners.
//!
//! ## Wire byte `0x8e` (three meanings)
//!
//! | Context | Meaning |
//! |---------|---------|
//! | MMS header field `0x0e` | Message-Size (Long-integer) |
//! | GO fragment | Named part marker: `0x8e` + `filename\0` + payload ([`scan_named_parts`]) |
//! | WSP part header `0x0e` | Content-Location |
//!
//! Message-Size decode requires a valid Long-integer; on failure the header walk
//! soft-stops and named-part scanning still sees the raw `0x8e` in the buffer.
//!
//! ## Other quirks
//!
//! - **Content-Type terminates headers** (WAP-209): after CT, remaining bytes are
//!   the multipart body (or GO junk / named parts).
//! - **Content-Type general-form** starts with Value-length (`peek <= 31`) and must
//!   be tried before constrained-media text, or the length octet is misread as TEXT.
//! - **GO Value-length overshoot** on From/To/encoded-strings: declared length often
//!   swallows the next short-integer header; readers stop before known MMS field bytes.
//! - **Application headers** (Token-text name) land in
//!   [`StructuredMms::application_headers`] and CSV as `app:<name>` (see
//!   `docs/IMPORT_MAPPING.md`).

use std::collections::{BTreeMap, HashMap};

/// Well-known MMS field names (WAP-209 table 8). Stored as short-integer values
/// (MSB already cleared); on the wire they appear as `value | 0x80`.
const MMS_BCC: u8 = 0x01;
const MMS_CC: u8 = 0x02;
const MMS_CONTENT_LOCATION: u8 = 0x03;
const MMS_CONTENT_TYPE: u8 = 0x04;
const MMS_DATE: u8 = 0x05;
const MMS_DELIVERY_REPORT: u8 = 0x06;
const MMS_DELIVERY_TIME: u8 = 0x07;
const MMS_EXPIRY: u8 = 0x08;
const MMS_FROM: u8 = 0x09;
const MMS_MESSAGE_CLASS: u8 = 0x0a;
const MMS_MESSAGE_ID: u8 = 0x0b;
const MMS_MESSAGE_TYPE: u8 = 0x0c;
const MMS_VERSION: u8 = 0x0d;
const MMS_MESSAGE_SIZE: u8 = 0x0e;
const MMS_PRIORITY: u8 = 0x0f;
const MMS_READ_REPORT: u8 = 0x10;
const MMS_REPORT_ALLOWED: u8 = 0x11;
const MMS_RESPONSE_STATUS: u8 = 0x12;
const MMS_RESPONSE_TEXT: u8 = 0x13;
const MMS_SENDER_VISIBILITY: u8 = 0x14;
const MMS_STATUS: u8 = 0x15;
const MMS_SUBJECT: u8 = 0x16;
const MMS_TO: u8 = 0x17;
const MMS_TRANSACTION_ID: u8 = 0x18;

/// WSP well-known headers (table 39), short-integer form (MSB cleared).
const WSP_CONTENT_LOCATION: u8 = 0x0e;
const WSP_CONTENT_DISPOSITION: u8 = 0x2e;
const WSP_CONTENT_ID: u8 = 0x40;
/// IANA MIBEnum UTF-8 / UCS-2.
const CHARSET_UTF8: u64 = 106;
const CHARSET_UCS2: u64 = 1000;

/// Subset of WSP well-known content types (WAP-230 table 40) used for attachments.
const WELL_KNOWN_CONTENT_TYPES: &[&str] = &[
    "*/*",
    "text/*",
    "text/html",
    "text/plain",
    "multipart/*",
    "multipart/mixed",
    "multipart/form-data",
    "multipart/byteranges",
    "multipart/alternative",
    "application/*",
    "application/java-vm",
    "application/x-www-form-urlencoded",
    "application/hdmlc",
    "application/vnd.wap.wmlc",
    "application/vnd.wap.wmlscriptc",
    "application/vnd.wap.wta-eventc",
    "application/vnd.wap.uaprof",
    "application/vnd.wap.wtls-ca-certificate",
    "application/vnd.wap.wtls-user-certificate",
    "application/x-x509-ca-cert",
    "application/x-x509-user-cert",
    "image/*",
    "image/gif",
    "image/jpeg",
    "image/tiff",
    "image/png",
    "image/vnd.wap.wbmp",
    "application/vnd.wap.multipart.*",
    "application/vnd.wap.multipart.mixed",
    "application/vnd.wap.multipart.form-data",
    "application/vnd.wap.multipart.byteranges",
    "application/vnd.wap.multipart.alternative",
    "application/xml",
    "text/xml",
    "application/vnd.wap.wbxml",
    "application/x-x968-cross-cert",
    "application/x-x968-ca-cert",
    "application/x-x968-user-cert",
    "text/vnd.wap.si",
    "application/vnd.wap.sic",
    "text/vnd.wap.sl",
    "application/vnd.wap.slc",
    "text/vnd.wap.co",
    "application/vnd.wap.coc",
    "application/vnd.wap.multipart.related",
    "application/vnd.wap.sia",
    "text/vnd.wap.connectivity-xml",
    "application/vnd.wap.connectivity-wbxml",
    "application/pkcs7-mime",
    "application/vnd.wap.hashed-certificate",
    "application/vnd.wap.signed-certificate",
    "application/vnd.wap.cert-response",
    "application/xhtml+xml",
    "application/wml+xml",
    "text/css",
    "application/vnd.wap.mms-message",
    "application/vnd.wap.rollover-certificate",
    "application/vnd.wap.locc+wbxml",
    "application/vnd.wap.loc+xml",
    "application/vnd.syncml.dm+wbxml",
    "application/vnd.syncml.dm+xml",
    "application/vnd.syncml.notification",
    "application/vnd.wap.xhtml+xml",
    "application/vnd.wv.csp.cir",
    "application/vnd.oma.dd+xml",
    "application/vnd.oma.drm.message",
    "application/vnd.oma.drm.content",
    "application/vnd.oma.drm.rights+xml",
    "application/vnd.oma.drm.rights+wbxml",
];

/// One multipart body part (WSP headers + payload).
#[derive(Debug, Clone)]
pub(crate) struct MmsPart {
    pub content_type: String,
    pub content_location: Option<String>,
    pub content_id: Option<String>,
    /// From Content-Type `Filename` / Content-Disposition filename parameter.
    pub filename: Option<String>,
    /// IANA MIBEnum from Content-Type Charset parameter, when present.
    pub charset: Option<u64>,
    pub data: Vec<u8>,
}

/// GO Content-Location-style named payload (`0x8e` + `name\0` + bytes).
#[derive(Debug, Clone)]
pub(crate) struct NamedPart {
    pub name: String,
    pub data: Vec<u8>,
}

/// Best-effort decoded MMS headers, parts, and GO named fragments.
///
/// Unknown text application headers are in [`Self::application_headers`] and
/// exported to CSV as `app:<name>`.
#[derive(Debug, Clone, Default)]
pub(crate) struct StructuredMms {
    pub message_type: Option<String>,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub content_type: Option<String>,
    /// Content-Type `Start` parameter (root/SMIL Content-ID), when present.
    pub content_start: Option<String>,
    pub date_unix: Option<u64>,
    pub subject: Option<String>,
    pub message_id: Option<String>,
    pub delivery_report: Option<String>,
    pub read_report: Option<String>,
    pub priority: Option<String>,
    pub delivery_time: Option<String>,
    pub expiry: Option<String>,
    pub message_class: Option<String>,
    pub mms_version: Option<String>,
    /// WAP-209 Message-Size (octets); advisory / approximate.
    pub message_size: Option<u64>,
    pub report_allowed: Option<String>,
    pub response_status: Option<String>,
    pub response_text: Option<String>,
    pub sender_visibility: Option<String>,
    pub status: Option<String>,
    pub transaction_id: Option<String>,
    /// Non-well-known MMS application headers (text name → value).
    pub application_headers: BTreeMap<String, String>,
    pub parts: Vec<MmsPart>,
    pub named_parts: Vec<NamedPart>,
}

impl StructuredMms {
    pub fn is_useful(&self) -> bool {
        self.from.is_some()
            || !self.to.is_empty()
            || !self.cc.is_empty()
            || !self.bcc.is_empty()
            || !self.parts.is_empty()
            || !self.named_parts.is_empty()
            || self.subject.is_some()
            || self.message_id.is_some()
            || self.message_type.is_some()
            || self.content_type.is_some()
            || self.transaction_id.is_some()
            || !self.application_headers.is_empty()
    }

    pub fn address_strings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(from) = &self.from {
            out.push(from.clone());
        }
        out.extend(self.to.iter().cloned());
        out.extend(self.cc.iter().cloned());
        out.extend(self.bcc.iter().cloned());
        out
    }

    fn merge_opt(dst: &mut Option<String>, src: Option<String>) {
        if dst.is_none() {
            *dst = src;
        }
    }

    fn merge_from(&mut self, other: StructuredMms) {
        Self::merge_opt(&mut self.from, other.from);
        if self.to.is_empty() {
            self.to = other.to;
        }
        if self.cc.is_empty() {
            self.cc = other.cc;
        }
        if self.bcc.is_empty() {
            self.bcc = other.bcc;
        }
        if self.date_unix.is_none() {
            self.date_unix = other.date_unix;
        }
        Self::merge_opt(&mut self.message_type, other.message_type);
        Self::merge_opt(&mut self.content_type, other.content_type);
        Self::merge_opt(&mut self.content_start, other.content_start);
        Self::merge_opt(&mut self.subject, other.subject);
        Self::merge_opt(&mut self.message_id, other.message_id);
        Self::merge_opt(&mut self.delivery_report, other.delivery_report);
        Self::merge_opt(&mut self.read_report, other.read_report);
        Self::merge_opt(&mut self.priority, other.priority);
        Self::merge_opt(&mut self.delivery_time, other.delivery_time);
        Self::merge_opt(&mut self.expiry, other.expiry);
        Self::merge_opt(&mut self.message_class, other.message_class);
        Self::merge_opt(&mut self.mms_version, other.mms_version);
        if self.message_size.is_none() {
            self.message_size = other.message_size;
        }
        Self::merge_opt(&mut self.report_allowed, other.report_allowed);
        Self::merge_opt(&mut self.response_status, other.response_status);
        Self::merge_opt(&mut self.response_text, other.response_text);
        Self::merge_opt(&mut self.sender_visibility, other.sender_visibility);
        Self::merge_opt(&mut self.status, other.status);
        Self::merge_opt(&mut self.transaction_id, other.transaction_id);
        for (k, v) in other.application_headers {
            self.application_headers.entry(k).or_insert(v);
        }
        merge_parts_into(&mut self.parts, other.parts);
        if self.named_parts.is_empty() {
            self.named_parts = other.named_parts;
        }
    }
}

fn is_mms_short_integer_field(field: u8) -> bool {
    matches!(
        field,
        MMS_BCC
            | MMS_CC
            | MMS_CONTENT_LOCATION
            | MMS_CONTENT_TYPE
            | MMS_DATE
            | MMS_DELIVERY_REPORT
            | MMS_DELIVERY_TIME
            | MMS_EXPIRY
            | MMS_FROM
            | MMS_MESSAGE_CLASS
            | MMS_MESSAGE_ID
            | MMS_MESSAGE_TYPE
            | MMS_VERSION
            | MMS_MESSAGE_SIZE
            | MMS_PRIORITY
            | MMS_READ_REPORT
            | MMS_REPORT_ALLOWED
            | MMS_RESPONSE_STATUS
            | MMS_RESPONSE_TEXT
            | MMS_SENDER_VISIBILITY
            | MMS_STATUS
            | MMS_SUBJECT
            | MMS_TO
            | MMS_TRANSACTION_ID
    )
}

fn yes_no_token(v: u8) -> Option<&'static str> {
    match v {
        0x00 => Some("yes"),
        0x01 => Some("no"),
        _ => None,
    }
}

fn priority_token(v: u8) -> Option<&'static str> {
    match v {
        0x00 => Some("Low"),
        0x01 => Some("Normal"),
        0x02 => Some("High"),
        _ => None,
    }
}

fn part_dedupe_key(part: &MmsPart) -> (Option<&str>, Option<&str>, Option<&str>, usize, &[u8]) {
    (
        part.content_id.as_deref(),
        part.content_location.as_deref(),
        part.filename.as_deref(),
        part.data.len(),
        &part.data[..part.data.len().min(64)],
    )
}

fn merge_parts_into(dst: &mut Vec<MmsPart>, incoming: Vec<MmsPart>) {
    for part in incoming {
        let key = part_dedupe_key(&part);
        let already = dst.iter().any(|p| part_dedupe_key(p) == key);
        if !already {
            dst.push(part);
        }
    }
}

#[derive(Debug)]
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Result<u8, ()> {
        let b = self.peek().ok_or(())?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ()> {
        if self.remaining() < n {
            return Err(());
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
}

fn decode_uint_var(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    let mut value = 0u64;
    for _ in 0..5 {
        let byte = cur.next_byte()?;
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(())
}

fn decode_value_length(cur: &mut Cursor<'_>) -> Result<usize, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte <= 30 {
        cur.next_byte()?;
        Ok(usize::from(byte))
    } else if byte == 31 {
        cur.next_byte()?;
        Ok(decode_uint_var(cur)? as usize)
    } else {
        Err(())
    }
}

fn decode_text_string(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let start = cur.pos;
    while cur.pos < cur.data.len() && cur.data[cur.pos] != 0 {
        cur.pos += 1;
    }
    if cur.pos >= cur.data.len() {
        return Err(());
    }
    let s = String::from_utf8_lossy(&cur.data[start..cur.pos]).into_owned();
    cur.pos += 1; // skip NUL
    Ok(s)
}

fn decode_short_integer(cur: &mut Cursor<'_>) -> Result<u8, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte & 0x80 == 0 {
        return Err(());
    }
    cur.next_byte()?;
    Ok(byte & 0x7f)
}

fn decode_long_integer(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    let len = cur.peek().ok_or(())?;
    if len == 0 || len > 30 {
        return Err(());
    }
    cur.next_byte()?;
    let bytes = cur.take(usize::from(len))?;
    let mut value = 0u64;
    for b in bytes {
        value = (value << 8) | u64::from(*b);
    }
    Ok(value)
}

fn decode_integer_value(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    if let Ok(v) = decode_short_integer(cur) {
        return Ok(u64::from(v));
    }
    decode_long_integer(cur)
}

fn trim_encoded_string_junk(s: &str) -> String {
    // GO value-lengths often swallow the next header; keep a clean PLMN address.
    if let Some(idx) = s.find("/TYPE=PLMN") {
        return s[..idx + "/TYPE=PLMN".len()].to_string();
    }
    s.trim_end_matches('\0').to_string()
}

/// Decode raw part/header bytes with an optional IANA MIBEnum charset.
pub(crate) fn decode_bytes_with_charset(bytes: &[u8], charset: Option<u64>) -> String {
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    let text = match charset {
        Some(CHARSET_UCS2) if !bytes.is_empty() => {
            let even = &bytes[..bytes.len() - (bytes.len() % 2)];
            let mut units = Vec::with_capacity(even.len() / 2);
            for chunk in even.chunks_exact(2) {
                units.push(u16::from_be_bytes([chunk[0], chunk[1]]));
            }
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    trim_encoded_string_junk(text.trim_end_matches('\0'))
}

/// Encoded-string-value = Text-string | Value-length Char-set Text-string
///
/// GO SMS Pro PDUs often declare a Value-length that overlaps the next MMS
/// short-integer header. Read text until NUL or a high-bit header byte; do not
/// blindly consume through `end` when that would swallow the next field.
fn decode_encoded_string_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let saved = cur.pos;
    if let Ok(len) = decode_value_length(cur) {
        let end = cur.pos.checked_add(len).ok_or(())?;
        if end > cur.data.len() {
            cur.pos = saved;
            return Err(());
        }
        let after_len = cur.pos;
        let charset = match decode_integer_value(cur) {
            Ok(cs) if cur.pos <= end => Some(cs),
            _ => {
                cur.pos = after_len;
                None
            }
        };
        if cur.pos > end {
            cur.pos = saved;
            return Err(());
        }
        let start = cur.pos;
        match charset {
            Some(CHARSET_UCS2) => {
                while cur.pos + 1 < end {
                    if cur.data[cur.pos] == 0 && cur.data[cur.pos + 1] == 0 {
                        break;
                    }
                    cur.pos += 2;
                }
            }
            Some(CHARSET_UTF8) => {
                // Prefer value-length end (real UTF-8). Also stop before an obvious
                // next MMS header when GO overshoots (ASCII PLMN then 0x8x/0x9x).
                while cur.pos < end && cur.data[cur.pos] != 0 {
                    let b = cur.data[cur.pos];
                    if b & 0x80 != 0 && is_mms_short_integer_field(b & 0x7f) {
                        break;
                    }
                    cur.pos += 1;
                }
            }
            _ => {
                // Vendor overshoot: stop before the next short-integer header.
                while cur.pos < end {
                    let b = cur.data[cur.pos];
                    if b == 0 || b & 0x80 != 0 {
                        break;
                    }
                    cur.pos += 1;
                }
            }
        }
        let text = decode_bytes_with_charset(&cur.data[start..cur.pos.min(end)], charset);
        if cur.pos < end && cur.data[cur.pos] == 0 {
            cur.pos += 1;
        }
        while cur.pos < end && cur.data[cur.pos] == 0 {
            cur.pos += 1;
        }
        // Leave a following short-integer header in place when length overshoots.
        if cur.pos < end && cur.data[cur.pos] & 0x80 != 0 {
            // keep pos
        } else if cur.pos < end {
            cur.pos = end;
        }
        if !text.is_empty() {
            return Ok(text);
        }
        cur.pos = saved;
        return Err(());
    }
    cur.pos = saved;
    decode_text_string(cur)
}

fn decode_delta_seconds(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    decode_long_integer(cur).or_else(|_| decode_integer_value(cur))
}

/// Absolute-token Date-value | Relative-token Delta-seconds-value inside Value-length.
fn decode_expiry_or_delivery_time(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let saved = cur.pos;
    let len = decode_value_length(cur)?;
    let end = cur.pos.checked_add(len).ok_or(())?;
    if end > cur.data.len() {
        cur.pos = saved;
        return Err(());
    }
    let token = cur.next_byte()?;
    let result = if token == 0x80 {
        let d = decode_date_value(cur)?;
        format!("absolute:{d}")
    } else if token == 0x81 {
        let d = decode_delta_seconds(cur)?;
        format!("relative:{d}")
    } else {
        return Err(());
    };
    cur.pos = end.max(cur.pos);
    Ok(result)
}

fn decode_mms_version(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    let major = (v >> 4) & 0x0f;
    let minor = v & 0x0f;
    Ok(format!("{major}.{minor}"))
}

fn decode_message_class_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let saved = cur.pos;
    if let Ok(v) = decode_short_integer(cur) {
        let name = match v {
            0x00 => "Personal",
            0x01 => "Advertisement",
            0x02 => "Informational",
            0x03 => "Auto",
            other => return Ok(format!("unknown-0x{other:02x}")),
        };
        return Ok(name.into());
    }
    cur.pos = saved;
    decode_text_string(cur)
}

fn decode_status_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    Ok(match v {
        0x00 => "Expired".into(),
        0x01 => "Retrieved".into(),
        0x02 => "Rejected".into(),
        0x03 => "Deferred".into(),
        0x04 => "Unrecognized".into(),
        0x05 => "Indeterminate".into(),
        0x06 => "Forwarded".into(),
        0x07 => "Unreachable".into(),
        other => format!("unknown-0x{other:02x}"),
    })
}

fn decode_response_status_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    Ok(match v {
        0x00 => "Ok".into(),
        0x01 => "Error-unspecified".into(),
        0x02 => "Error-service-denied".into(),
        0x03 => "Error-message-format-corrupt".into(),
        0x04 => "Error-sending-address-unresolved".into(),
        0x05 => "Error-message-not-found".into(),
        0x06 => "Error-network-problem".into(),
        0x07 => "Error-content-not-accepted".into(),
        0x08 => "Error-unsupported-message".into(),
        other => format!("0x{other:02x}"),
    })
}

fn decode_sender_visibility_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    Ok(match v {
        0x00 => "Hide".into(),
        0x01 => "Show".into(),
        other => format!("unknown-0x{other:02x}"),
    })
}

pub(crate) fn normalize_content_id(raw: &str) -> String {
    let s = raw.trim();
    let s = s.strip_prefix("cid:").unwrap_or(s);
    let s = s.strip_prefix('<').unwrap_or(s);
    let s = s.strip_suffix('>').unwrap_or(s);
    s.trim().to_string()
}

/// From-value = Value-length (Address-present-token Encoded-string-value | Insert-address-token)
fn decode_from_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let saved = cur.pos;
    let len = decode_value_length(cur)?;
    let end = cur.pos.checked_add(len).ok_or(())?;
    if end > cur.data.len() {
        cur.pos = saved;
        return Err(());
    }
    let token = cur.next_byte()?;
    if token == 0x81 {
        // Insert-address-token
        cur.pos = end;
        return Ok(String::new());
    }
    if token != 0x80 {
        return Err(());
    }
    let addr = decode_encoded_string_value(cur)?;
    // Same Value-length overshoot quirk as Encoded-string-value.
    while cur.pos < end && cur.data[cur.pos] == 0 {
        cur.pos += 1;
    }
    if cur.pos < end && cur.data[cur.pos] & 0x80 != 0 {
        return Ok(addr);
    }
    cur.pos = end.max(cur.pos);
    Ok(addr)
}

fn decode_date_value(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    decode_long_integer(cur)
}

fn decode_message_type_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    let name = match v {
        0x00 => "m-send-req",
        0x01 => "m-send-conf",
        0x02 => "m-notification-ind",
        0x03 => "m-notifyresp-ind",
        0x04 => "m-retrieve-conf",
        0x05 => "m-acknowledge-ind",
        0x06 => "m-delivery-ind",
        0x07 => "m-read-rec-ind",
        0x08 => "m-read-orig-ind",
        0x09 => "m-forward-req",
        0x0a => "m-forward-conf",
        other => return Ok(format!("unknown-0x{other:02x}")),
    };
    Ok(name.to_string())
}

fn well_known_content_type(id: u64) -> Option<&'static str> {
    WELL_KNOWN_CONTENT_TYPES.get(id as usize).copied()
}

fn decode_constrained_media(cur: &mut Cursor<'_>) -> Result<String, ()> {
    if let Ok(id) = decode_short_integer(cur) {
        return well_known_content_type(u64::from(id))
            .map(str::to_string)
            .ok_or(());
    }
    decode_text_string(cur)
}

fn decode_wsp_text_param(cur: &mut Cursor<'_>) -> Result<String, ()> {
    decode_text_string(cur).or_else(|_| decode_encoded_string_value(cur))
}

/// Decode a well-known WSP parameter by token id (table 38).
/// Charset/Type are integer-like; Name/Filename/Start are text-like.
fn decode_wsp_typed_param(cur: &mut Cursor<'_>, name_id: u8) -> Result<(String, String), ()> {
    match name_id {
        0x08 => {
            // Charset = Well-known-charset (Integer-value)
            let v = decode_integer_value(cur)?;
            Ok(("Charset".into(), v.to_string()))
        }
        0x09 => {
            // Type (v1.2+) = Constrained-encoding
            let ct = decode_constrained_media(cur)?;
            Ok(("Type".into(), ct))
        }
        0x05 | 0x17 => Ok(("Name".into(), decode_wsp_text_param(cur)?)),
        0x06 | 0x18 => Ok(("Filename".into(), decode_wsp_text_param(cur)?)),
        0x0a | 0x19 => Ok(("Start".into(), decode_wsp_text_param(cur)?)),
        0x0b | 0x1a => Ok(("Start-info".into(), decode_wsp_text_param(cur)?)),
        _ => Err(()),
    }
}

/// Scan WSP typed/untyped parameters until `end`.
fn decode_wsp_parameters(cur: &mut Cursor<'_>, end: usize) -> HashMap<String, String> {
    let mut params = HashMap::new();
    while cur.pos < end {
        let pstart = cur.pos;
        if let Ok(name_id) = decode_short_integer(cur) {
            if let Ok((key, val)) = decode_wsp_typed_param(cur, name_id) {
                params.insert(key, val);
                continue;
            }
        } else if let Ok(name) = decode_text_string(cur) {
            // Untyped-parameter = Token-text Untyped-value
            if let Ok(val) = decode_wsp_text_param(cur).or_else(|_| decode_constrained_media(cur))
                && !name.is_empty()
            {
                params.insert(name, val);
                continue;
            }
        }
        cur.pos = pstart + 1;
        if cur.pos <= pstart {
            break;
        }
    }
    params
}

fn decode_content_type_value(
    cur: &mut Cursor<'_>,
) -> Result<(String, HashMap<String, String>), ()> {
    let saved = cur.pos;
    let peek = cur.peek().ok_or(())?;
    // Content-general-form starts with Value-length (0..=30 or 31+uintvar).
    // Must try before Constrained-media text, or length bytes look like TEXT.
    if peek <= 31 {
        let len = decode_value_length(cur)?;
        let end = cur.pos.checked_add(len).ok_or(())?;
        if end > cur.data.len() {
            cur.pos = saved;
            return Err(());
        }
        let media = if let Ok(id) = decode_integer_value(cur) {
            well_known_content_type(id)
                .map(str::to_string)
                .unwrap_or_else(|| format!("application/octet-stream;id={id}"))
        } else {
            decode_text_string(cur)?
        };
        let params = decode_wsp_parameters(cur, end);
        cur.pos = end;
        return Ok((media, params));
    }
    if let Ok(ct) = decode_constrained_media(cur) {
        return Ok((ct, HashMap::new()));
    }
    cur.pos = saved;
    Err(())
}

/// Content-Disposition-value = Value-length Disposition *(Parameter) | text
fn decode_content_disposition_value(
    cur: &mut Cursor<'_>,
) -> Result<(String, HashMap<String, String>), ()> {
    let saved = cur.pos;
    if let Ok(len) = decode_value_length(cur) {
        let end = cur.pos.checked_add(len).ok_or(())?;
        if end > cur.data.len() {
            cur.pos = saved;
            return Err(());
        }
        let token = cur.next_byte()?;
        let disposition = match token {
            0x80 => "form-data".into(),
            0x81 => "attachment".into(),
            0x82 => "inline".into(),
            _ => format!("0x{token:02x}"),
        };
        let params = decode_wsp_parameters(cur, end);
        cur.pos = end;
        return Ok((disposition, params));
    }
    cur.pos = saved;
    Ok((decode_text_string(cur)?, HashMap::new()))
}

fn decode_application_header_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let saved = cur.pos;
    if let Ok(s) = decode_text_string(cur)
        && !s.is_empty()
    {
        return Ok(s);
    }
    cur.pos = saved;
    if let Ok(s) = decode_encoded_string_value(cur)
        && !s.is_empty()
    {
        return Ok(s);
    }
    cur.pos = saved;
    if let Ok(v) = decode_short_integer(cur) {
        return Ok(format!("0x{v:02x}"));
    }
    cur.pos = saved;
    if let Ok(v) = decode_long_integer(cur) {
        return Ok(v.to_string());
    }
    cur.pos = saved;
    skip_unknown_mms_value(cur)?;
    Err(())
}

fn skip_unknown_mms_value(cur: &mut Cursor<'_>) -> Result<(), ()> {
    // Best-effort: value-length blob, short-integer, or text-string.
    let saved = cur.pos;
    if let Ok(len) = decode_value_length(cur) {
        if cur.take(len).is_err() {
            cur.pos = saved;
            return Err(());
        }
        return Ok(());
    }
    cur.pos = saved;
    if decode_short_integer(cur).is_ok() {
        return Ok(());
    }
    cur.pos = saved;
    let _ = decode_text_string(cur)?;
    Ok(())
}

fn decode_mms_header_field(cur: &mut Cursor<'_>, msg: &mut StructuredMms) -> Result<bool, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte & 0x80 == 0 {
        // Application-header = Token-text Text-string (or other value forms).
        let name = decode_text_string(cur)?;
        if let Ok(value) = decode_application_header_value(cur)
            && !name.is_empty()
            && !value.is_empty()
        {
            msg.application_headers.entry(name).or_insert(value);
        }
        return Ok(false);
    }
    let field = decode_short_integer(cur)?;
    match field {
        MMS_FROM => {
            msg.from = Some(decode_from_value(cur)?);
            Ok(false)
        }
        MMS_TO => {
            if let Ok(addr) = decode_encoded_string_value(cur)
                && !addr.is_empty()
            {
                msg.to.push(addr);
            }
            Ok(false)
        }
        MMS_CC => {
            if let Ok(addr) = decode_encoded_string_value(cur)
                && !addr.is_empty()
            {
                msg.cc.push(addr);
            }
            Ok(false)
        }
        MMS_BCC => {
            if let Ok(addr) = decode_encoded_string_value(cur)
                && !addr.is_empty()
            {
                msg.bcc.push(addr);
            }
            Ok(false)
        }
        MMS_MESSAGE_TYPE => {
            msg.message_type = Some(decode_message_type_value(cur)?);
            Ok(false)
        }
        MMS_DATE => {
            msg.date_unix = Some(decode_date_value(cur)?);
            Ok(false)
        }
        MMS_SUBJECT => {
            if let Ok(s) = decode_encoded_string_value(cur)
                && !s.is_empty()
            {
                msg.subject = Some(s);
            }
            Ok(false)
        }
        MMS_MESSAGE_ID => {
            msg.message_id =
                Some(decode_text_string(cur).or_else(|_| decode_encoded_string_value(cur))?);
            Ok(false)
        }
        MMS_TRANSACTION_ID => {
            msg.transaction_id = Some(decode_text_string(cur)?);
            Ok(false)
        }
        MMS_VERSION => {
            msg.mms_version = Some(decode_mms_version(cur)?);
            Ok(false)
        }
        MMS_MESSAGE_SIZE => {
            // Long-integer only. GO named parts reuse wire 0x8e + filename; do not
            // hard-fail the PDU — leave size unset and stop this header value.
            let saved = cur.pos;
            match decode_long_integer(cur) {
                Ok(sz) => {
                    msg.message_size = Some(sz);
                    Ok(false)
                }
                Err(()) => {
                    cur.pos = saved;
                    // Signal soft stop of the header section (see decode_mms_at).
                    Err(())
                }
            }
        }
        MMS_MESSAGE_CLASS => {
            msg.message_class = Some(decode_message_class_value(cur)?);
            Ok(false)
        }
        MMS_DELIVERY_TIME => {
            msg.delivery_time = Some(decode_expiry_or_delivery_time(cur)?);
            Ok(false)
        }
        MMS_EXPIRY => {
            msg.expiry = Some(decode_expiry_or_delivery_time(cur)?);
            Ok(false)
        }
        MMS_DELIVERY_REPORT => {
            let v = decode_short_integer(cur)?;
            msg.delivery_report = yes_no_token(v).map(str::to_string);
            Ok(false)
        }
        MMS_READ_REPORT => {
            let v = decode_short_integer(cur)?;
            msg.read_report = yes_no_token(v).map(str::to_string);
            Ok(false)
        }
        MMS_REPORT_ALLOWED => {
            let v = decode_short_integer(cur)?;
            msg.report_allowed = yes_no_token(v).map(str::to_string);
            Ok(false)
        }
        MMS_PRIORITY => {
            let v = decode_short_integer(cur)?;
            msg.priority = priority_token(v).map(str::to_string);
            Ok(false)
        }
        MMS_STATUS => {
            msg.status = Some(decode_status_value(cur)?);
            Ok(false)
        }
        MMS_RESPONSE_STATUS => {
            msg.response_status = Some(decode_response_status_value(cur)?);
            Ok(false)
        }
        MMS_RESPONSE_TEXT => {
            msg.response_text = Some(decode_encoded_string_value(cur)?);
            Ok(false)
        }
        MMS_SENDER_VISIBILITY => {
            msg.sender_visibility = Some(decode_sender_visibility_value(cur)?);
            Ok(false)
        }
        MMS_CONTENT_TYPE => {
            let (ct, params) = decode_content_type_value(cur)?;
            msg.content_type = Some(ct);
            if let Some(start) = params.get("Start").or_else(|| params.get("Start-info")) {
                msg.content_start = Some(normalize_content_id(start));
            }
            Ok(true) // Content-Type terminates the header section
        }
        MMS_CONTENT_LOCATION => {
            let _ = decode_encoded_string_value(cur)
                .or_else(|_| decode_text_string(cur))
                .or_else(|_| {
                    skip_unknown_mms_value(cur)?;
                    Ok(String::new())
                })?;
            Ok(false)
        }
        _ => {
            skip_unknown_mms_value(cur)?;
            Ok(false)
        }
    }
}

fn decode_multipart_body(cur: &mut Cursor<'_>) -> Result<Vec<MmsPart>, ()> {
    let n = decode_uint_var(cur)? as usize;
    if n > 256 {
        return Err(());
    }
    let mut parts = Vec::with_capacity(n);
    for _ in 0..n {
        let headers_len = decode_uint_var(cur)? as usize;
        let data_len = decode_uint_var(cur)? as usize;
        if headers_len + data_len > cur.remaining() {
            return Err(());
        }
        let header_bytes = cur.take(headers_len)?;
        let mut hcur = Cursor::new(header_bytes);
        let (ctype, params) = decode_content_type_value(&mut hcur)
            .unwrap_or_else(|_| ("application/octet-stream".into(), HashMap::new()));
        let mut content_location = params
            .get("Name")
            .cloned()
            .or_else(|| params.get("Filename").cloned());
        let mut filename = params
            .get("Filename")
            .cloned()
            .or_else(|| params.get("Name").cloned());
        let charset = params.get("Charset").and_then(|s| s.parse::<u64>().ok());
        let mut content_id = None;
        while hcur.remaining() > 0 {
            let before = hcur.pos;
            if let Ok(field) = decode_short_integer(&mut hcur) {
                // Part headers use the WSP table: Content-Location is 0x0e, not
                // MMS 0x03 (Accept-Language in WSP).
                if field == WSP_CONTENT_LOCATION {
                    if let Ok(v) = decode_encoded_string_value(&mut hcur)
                        .or_else(|_| decode_text_string(&mut hcur))
                    {
                        content_location = Some(v);
                        continue;
                    }
                } else if field == WSP_CONTENT_ID {
                    if let Ok(v) = decode_encoded_string_value(&mut hcur)
                        .or_else(|_| decode_text_string(&mut hcur))
                    {
                        content_id = Some(normalize_content_id(&v));
                        continue;
                    }
                } else if field == WSP_CONTENT_DISPOSITION {
                    if let Ok((_disp, dparams)) = decode_content_disposition_value(&mut hcur) {
                        if let Some(fnm) = dparams.get("Filename").or_else(|| dparams.get("Name")) {
                            filename = Some(fnm.clone());
                            if content_location.is_none() {
                                content_location = Some(fnm.clone());
                            }
                        }
                        continue;
                    }
                } else {
                    let _ = skip_unknown_mms_value(&mut hcur);
                }
            } else if let Ok(name) = decode_text_string(&mut hcur) {
                if name.eq_ignore_ascii_case("Content-ID") {
                    if let Ok(v) = decode_encoded_string_value(&mut hcur)
                        .or_else(|_| decode_text_string(&mut hcur))
                    {
                        content_id = Some(normalize_content_id(&v));
                        continue;
                    }
                } else if name.eq_ignore_ascii_case("Content-Disposition") {
                    if let Ok((_disp, dparams)) = decode_content_disposition_value(&mut hcur) {
                        if let Some(fnm) = dparams.get("Filename").or_else(|| dparams.get("Name")) {
                            filename = Some(fnm.clone());
                            if content_location.is_none() {
                                content_location = Some(fnm.clone());
                            }
                        }
                        continue;
                    }
                } else if name.eq_ignore_ascii_case("Content-Location")
                    && let Ok(v) = decode_encoded_string_value(&mut hcur)
                        .or_else(|_| decode_text_string(&mut hcur))
                {
                    content_location = Some(v);
                    continue;
                }
                let _ = skip_unknown_mms_value(&mut hcur);
            }
            if hcur.pos == before {
                hcur.pos += 1;
            }
        }
        let data = cur.take(data_len)?.to_vec();
        parts.push(MmsPart {
            content_type: ctype,
            content_location,
            content_id,
            filename,
            charset,
            data,
        });
    }
    Ok(parts)
}

/// Attempt a full WAP-209 header + multipart decode starting at `start`.
pub(crate) fn decode_mms_at(data: &[u8], start: usize) -> Option<StructuredMms> {
    if start >= data.len() || data.len().saturating_sub(start) < 4 {
        return None;
    }
    let mut cur = Cursor { data, pos: start };
    let mut msg = StructuredMms::default();
    let mut saw_content_type = false;
    for _ in 0..64 {
        match decode_mms_header_field(&mut cur, &mut msg) {
            Ok(true) => {
                saw_content_type = true;
                break;
            }
            Ok(false) => {}
            // Soft-stop: keep headers decoded so far (e.g. GO 0x8e named part
            // misread as Message-Size). Scanners still see the raw bytes.
            Err(()) => break,
        }
    }
    if !saw_content_type && msg.from.is_none() && msg.to.is_empty() && msg.subject.is_none() {
        return None;
    }
    if let Some(ct) = &msg.content_type
        && ct.contains("multipart")
        && let Ok(parts) = decode_multipart_body(&mut cur)
    {
        msg.parts = parts;
    }
    if msg.is_useful() { Some(msg) } else { None }
}

/// Attempt a full decode from the start of `data`.
pub(crate) fn decode_mms(data: &[u8]) -> Option<StructuredMms> {
    decode_mms_at(data, 0)
}

/// Walk for Content-Type (`0x84`) and decode multipart bodies mid-file.
pub(crate) fn scan_multipart_bodies(data: &[u8]) -> Vec<MmsPart> {
    let mut parts = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] != 0x84 {
            i += 1;
            continue;
        }
        let mut cur = Cursor { data, pos: i + 1 };
        let Ok((ct, _params)) = decode_content_type_value(&mut cur) else {
            i += 1;
            continue;
        };
        if !ct.to_ascii_lowercase().contains("multipart") {
            i += 1;
            continue;
        }
        if let Ok(decoded) = decode_multipart_body(&mut cur) {
            merge_parts_into(&mut parts, decoded);
            i = cur.pos.max(i + 1);
        } else {
            i += 1;
        }
    }
    parts
}

fn scan_message_type_starts(data: &[u8]) -> Vec<StructuredMms> {
    let mut out = Vec::new();
    let mut attempts = 0usize;
    let mut i = 0;
    while i + 2 < data.len() && attempts < 32 {
        if data[i] != 0x8c {
            i += 1;
            continue;
        }
        attempts += 1;
        if let Some(msg) = decode_mms_at(data, i) {
            out.push(msg);
        }
        i += 1;
    }
    out
}

fn is_printable_name_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-')
}

fn looks_like_part_name(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    if !name.contains('.') {
        return false;
    }
    name.bytes().all(is_printable_name_byte)
}

fn try_parse_cloc_name_at(data: &[u8], at: usize) -> Option<(String, usize)> {
    // at points at 0x8e; returns (name, index of byte after NUL).
    if at >= data.len() || data[at] != 0x8e || at + 1 >= data.len() {
        return None;
    }
    let next = data[at + 1];
    if next & 0x80 != 0 || !is_printable_name_byte(next) {
        return None;
    }
    let name_start = at + 1;
    let mut name_end = name_start;
    while name_end < data.len() && data[name_end] != 0 {
        if !is_printable_name_byte(data[name_end]) {
            return None;
        }
        name_end += 1;
    }
    if name_end >= data.len() || data[name_end] != 0 {
        return None;
    }
    let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();
    if !looks_like_part_name(&name) {
        return None;
    }
    Some((name, name_end + 1))
}

fn find_next_cloc_name(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < data.len() {
        if data[i] == 0x8e && try_parse_cloc_name_at(data, i).is_some() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Matches `text.txt` or `text_<digits>.txt` (same rule as `pdu`).
fn is_text_part_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "text.txt" {
        return true;
    }
    let Some(rest) = lower.strip_prefix("text_") else {
        return false;
    };
    let Some(stem) = rest.strip_suffix(".txt") else {
        return false;
    };
    !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit())
}

/// Text-part payload end: advance through valid UTF-8 bytes so non-ASCII text
/// (accented Latin, Cyrillic, CJK) survives. Stops at the first byte that cannot
/// be part of a UTF-8 sequence — in GO dumps that is the next MMS header byte
/// (e.g. `0x8c`), a next named-part marker (`0x8e`), or a truncated sequence.
fn text_part_payload_end(data: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < data.len() {
        let b = data[end];
        if b < 0x80 {
            end += 1;
        } else if (0xc2..=0xdf).contains(&b)
            && end + 1 < data.len()
            && data[end + 1] & 0xc0 == 0x80
        {
            end += 2;
        } else if (0xe0..=0xef).contains(&b)
            && end + 2 < data.len()
            && data[end + 1] & 0xc0 == 0x80
            && data[end + 2] & 0xc0 == 0x80
        {
            end += 3;
        } else if (0xf0..=0xf4).contains(&b)
            && end + 3 < data.len()
            && data[end + 1] & 0xc0 == 0x80
            && data[end + 2] & 0xc0 == 0x80
            && data[end + 3] & 0xc0 == 0x80
        {
            end += 4;
        } else {
            break;
        }
    }
    end
}

/// Scan GO named parts: wire `0x8e` + NUL-terminated filename + payload.
///
/// This is **not** WAP-209 Message-Size (same wire id). Text parts end at the
/// next byte that cannot be part of a UTF-8 text payload (a header byte or the
/// next `0x8e` marker); media parts end at the next `0x8e` name (or EOF) so
/// JPEG high bytes are kept intact.
pub(crate) fn scan_named_parts(data: &[u8]) -> Vec<NamedPart> {
    let mut parts = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        let Some((name, payload_start)) = try_parse_cloc_name_at(data, i) else {
            i += 1;
            continue;
        };
        let payload_end = if is_text_part_name(&name) {
            text_part_payload_end(data, payload_start)
        } else {
            find_next_cloc_name(data, payload_start).unwrap_or(data.len())
        };
        let payload = data[payload_start..payload_end].to_vec();
        parts.push(NamedPart {
            name,
            data: payload,
        });
        i = payload_end.max(i + 1);
    }
    parts
}

pub(crate) fn content_type_from_filename(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "txt" => "text/plain".into(),
        "html" | "htm" => "text/html".into(),
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "gif" => "image/gif".into(),
        "amr" => "audio/amr".into(),
        "mp3" => "audio/mpeg".into(),
        "wav" => "audio/wav".into(),
        "3gp" => "video/3gpp".into(),
        "mp4" => "video/mp4".into(),
        "smil" => "application/smil".into(),
        _ => "application/octet-stream".into(),
    }
}

fn merge_named_parts(msg: &mut StructuredMms, named: Vec<NamedPart>) {
    if named.is_empty() {
        return;
    }
    let mut out_named = Vec::with_capacity(named.len());
    for np in named {
        let already = msg.parts.iter().any(|p| {
            p.content_location.as_deref() == Some(np.name.as_str())
                || (p.data == np.data && !np.data.is_empty())
        });
        let NamedPart { name, data } = np;
        if !already {
            msg.parts.push(MmsPart {
                content_type: content_type_from_filename(&name),
                content_location: Some(name.clone()),
                content_id: None,
                filename: Some(name.clone()),
                charset: None,
                data,
            });
            let data = msg.parts.last().expect("just pushed").data.clone();
            out_named.push(NamedPart { name, data });
        } else {
            out_named.push(NamedPart { name, data });
        }
    }
    msg.named_parts = out_named;
}

/// Returns `true` when the field was decoded and the outer scan cursor should advance to `cur.pos`.
fn apply_mms_header_field(field: u8, cur: &mut Cursor<'_>, msg: &mut StructuredMms) -> bool {
    match field {
        MMS_FROM => {
            if let Ok(addr) = decode_from_value(cur)
                && !addr.is_empty()
            {
                msg.from = Some(addr);
                return true;
            }
        }
        MMS_TO => {
            if let Ok(addr) = decode_encoded_string_value(cur)
                && !addr.is_empty()
            {
                msg.to.push(addr);
                return true;
            }
        }
        MMS_CC => {
            if let Ok(addr) = decode_encoded_string_value(cur)
                && !addr.is_empty()
            {
                msg.cc.push(addr);
                return true;
            }
        }
        MMS_BCC => {
            if let Ok(addr) = decode_encoded_string_value(cur)
                && !addr.is_empty()
            {
                msg.bcc.push(addr);
                return true;
            }
        }
        MMS_DATE => {
            if let Ok(d) = decode_date_value(cur)
                && d > 0
                && msg.date_unix.is_none()
            {
                msg.date_unix = Some(d);
                return true;
            }
        }
        MMS_SUBJECT => {
            if let Ok(s) = decode_encoded_string_value(cur)
                && !s.is_empty()
                && msg.subject.is_none()
            {
                msg.subject = Some(s);
                return true;
            }
        }
        MMS_STATUS => {
            if let Ok(s) = decode_status_value(cur)
                && msg.status.is_none()
            {
                msg.status = Some(s);
                return true;
            }
        }
        MMS_MESSAGE_ID => {
            if let Ok(id) = decode_text_string(cur).or_else(|_| decode_encoded_string_value(cur))
                && !id.is_empty()
                && msg.message_id.is_none()
            {
                msg.message_id = Some(id);
                return true;
            }
        }
        MMS_TRANSACTION_ID => {
            if let Ok(id) = decode_text_string(cur)
                && !id.is_empty()
                && msg.transaction_id.is_none()
            {
                msg.transaction_id = Some(id);
                return true;
            }
        }
        MMS_VERSION => {
            if let Ok(v) = decode_mms_version(cur)
                && msg.mms_version.is_none()
            {
                msg.mms_version = Some(v);
                return true;
            }
        }
        MMS_MESSAGE_SIZE => {
            // Only accept a real Long-integer. GO `\x8etext.txt\0` fails
            // (length byte > 30) so scan_named_parts keeps those payloads.
            if let Ok(sz) = decode_long_integer(cur)
                && msg.message_size.is_none()
            {
                msg.message_size = Some(sz);
                return true;
            }
        }
        MMS_MESSAGE_CLASS => {
            if let Ok(v) = decode_message_class_value(cur)
                && msg.message_class.is_none()
            {
                msg.message_class = Some(v);
                return true;
            }
        }
        MMS_DELIVERY_TIME => {
            if let Ok(v) = decode_expiry_or_delivery_time(cur)
                && msg.delivery_time.is_none()
            {
                msg.delivery_time = Some(v);
                return true;
            }
        }
        MMS_EXPIRY => {
            if let Ok(v) = decode_expiry_or_delivery_time(cur)
                && msg.expiry.is_none()
            {
                msg.expiry = Some(v);
                return true;
            }
        }
        MMS_DELIVERY_REPORT => {
            if let Ok(v) = decode_short_integer(cur)
                && msg.delivery_report.is_none()
            {
                msg.delivery_report = yes_no_token(v).map(str::to_string);
                if msg.delivery_report.is_some() {
                    return true;
                }
            }
        }
        MMS_READ_REPORT => {
            if let Ok(v) = decode_short_integer(cur)
                && msg.read_report.is_none()
            {
                msg.read_report = yes_no_token(v).map(str::to_string);
                if msg.read_report.is_some() {
                    return true;
                }
            }
        }
        MMS_REPORT_ALLOWED => {
            if let Ok(v) = decode_short_integer(cur)
                && msg.report_allowed.is_none()
            {
                msg.report_allowed = yes_no_token(v).map(str::to_string);
                if msg.report_allowed.is_some() {
                    return true;
                }
            }
        }
        MMS_PRIORITY => {
            if let Ok(v) = decode_short_integer(cur)
                && msg.priority.is_none()
            {
                msg.priority = priority_token(v).map(str::to_string);
                if msg.priority.is_some() {
                    return true;
                }
            }
        }
        MMS_RESPONSE_STATUS => {
            if let Ok(v) = decode_response_status_value(cur)
                && msg.response_status.is_none()
            {
                msg.response_status = Some(v);
                return true;
            }
        }
        MMS_RESPONSE_TEXT => {
            if let Ok(v) = decode_encoded_string_value(cur)
                && msg.response_text.is_none()
            {
                msg.response_text = Some(v);
                return true;
            }
        }
        MMS_SENDER_VISIBILITY => {
            if let Ok(v) = decode_sender_visibility_value(cur)
                && msg.sender_visibility.is_none()
            {
                msg.sender_visibility = Some(v);
                return true;
            }
        }
        MMS_MESSAGE_TYPE => {
            if let Ok(mt) = decode_message_type_value(cur)
                && msg.message_type.is_none()
            {
                msg.message_type = Some(mt);
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Scan for embedded From/To/Cc/Date short-integer headers (GO SMS Pro fragments).
pub(crate) fn scan_mms_addresses(data: &[u8]) -> StructuredMms {
    let mut msg = StructuredMms::default();
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        if byte & 0x80 == 0 {
            i += 1;
            continue;
        }
        let field = byte & 0x7f;
        if i + 1 >= data.len() {
            break;
        }
        let mut cur = Cursor { data, pos: i + 1 };
        if apply_mms_header_field(field, &mut cur, &mut msg) {
            i = cur.pos;
            continue;
        }
        i += 1;
    }
    msg
}

/// Merge full/offset decode, address/header scan, mid-file multipart, and GO named parts.
///
/// Prefer this entry point for GO backup files: individual paths alone are incomplete.
pub(crate) fn decode_mms_best_effort(data: &[u8]) -> StructuredMms {
    let named = scan_named_parts(data);
    let mut msg = decode_mms(data).unwrap_or_default();
    msg.merge_from(scan_mms_addresses(data));
    for candidate in scan_message_type_starts(data) {
        msg.merge_from(candidate);
    }
    merge_parts_into(&mut msg.parts, scan_multipart_bodies(data));
    merge_named_parts(&mut msg, named);
    msg
}

pub(crate) fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    let ct = content_type.to_ascii_lowercase();
    let base = ct.split(';').next().unwrap_or(&ct).trim();
    match base {
        "image/jpeg" | "image/jpg" => Some(".jpg"),
        "image/png" => Some(".png"),
        "image/gif" => Some(".gif"),
        "image/tiff" => Some(".tiff"),
        "image/vnd.wap.wbmp" => Some(".wbmp"),
        "audio/amr" | "audio/3gpp" => Some(".amr"),
        "audio/mpeg" | "audio/mp3" => Some(".mp3"),
        "audio/wav" | "audio/x-wav" => Some(".wav"),
        "video/3gpp" => Some(".3gp"),
        "video/mp4" => Some(".mp4"),
        "text/plain" | "text/*" => Some(".txt"),
        "application/smil" | "application/vnd.wap.multipart.related" => None,
        _ if base.starts_with("image/") => Some(".bin"),
        _ if base.starts_with("audio/") => Some(".bin"),
        _ if base.starts_with("video/") => Some(".bin"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn uint_var_single_and_multi() {
        let mut c = Cursor::new(&[0x05]);
        assert_eq!(decode_uint_var(&mut c).unwrap(), 5);
        // 0x81 0x02 => (1<<7)|2 = 130
        let mut c = Cursor::new(&[0x81, 0x02]);
        assert_eq!(decode_uint_var(&mut c).unwrap(), 130);
    }

    #[test]
    fn value_length_short_and_quoted() {
        let mut c = Cursor::new(&[0x1a]);
        assert_eq!(decode_value_length(&mut c).unwrap(), 26);
        let mut c = Cursor::new(&[0x1f, 0x20]);
        assert_eq!(decode_value_length(&mut c).unwrap(), 32);
    }

    #[test]
    fn scan_from_to_on_recv_fixture_shape() {
        // From + To fragment matching I_1609459200_recv.pdu: Value-lengths overshoot
        // into the next short-integer header (no NULs after PLMN).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.extend_from_slice(&[0x8e]); // next header byte overlapped by To length
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.from.as_deref(), Some("+4075551234/TYPE=PLMN"));
        assert_eq!(msg.to, vec!["+15555550100/TYPE=PLMN".to_string()]);
    }

    #[test]
    fn scan_real_recv_fixture() {
        let data = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pdu/I_1609459200_recv.pdu"),
        )
        .unwrap();
        let msg = decode_mms_best_effort(&data);
        assert_eq!(msg.from.as_deref(), Some("+4075551234/TYPE=PLMN"));
        assert!(msg.to.iter().any(|t| t.contains("15555550100")));
        assert!(
            msg.named_parts
                .iter()
                .any(|p| p.name == "text.txt" && p.data.starts_with(b"Hello one to one"))
        );
    }

    #[test]
    fn scan_date_header_fragment() {
        // Date = 0x85, long-integer length 4, value 0x5fee6600 = 1609459200
        let mut bytes = vec![0x85, 0x04, 0x5f, 0xee, 0x66, 0x00];
        bytes.extend_from_slice(&[0x8e]);
        bytes.extend_from_slice(b"text.txt\0hi");
        let msg = decode_mms_best_effort(&bytes);
        assert_eq!(msg.date_unix, Some(1609459200));
        assert_eq!(msg.named_parts[0].name, "text.txt");
        assert_eq!(msg.named_parts[0].data, b"hi");
    }

    #[test]
    fn midfile_multipart_content_type() {
        let related_idx = WELL_KNOWN_CONTENT_TYPES
            .iter()
            .position(|s| *s == "application/vnd.wap.multipart.related")
            .expect("related ct");
        let related_si = (related_idx as u8) | 0x80;

        // Junk prefix, then Content-Type multipart.related with text + jpeg parts.
        let text = b"Hello multipart";
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x11, 80));

        let mut body = vec![
            0x02,             // nEntries
            0x01,             // headersLen
            text.len() as u8, // dataLen
            0x83,             // text/plain
        ];
        body.extend_from_slice(text);
        body.push(0x01);
        body.push(jpeg.len() as u8);
        body.push(0x97); // image/jpeg
        body.extend_from_slice(&jpeg);

        let mut bytes = vec![0x00, 0x01, 0x02, 0x03, 0x04];
        bytes.push(0x84);
        bytes.push(related_si);
        bytes.extend_from_slice(&body);

        let parts = scan_multipart_bodies(&bytes);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].content_type.contains("text/plain"));
        assert_eq!(parts[0].data, text);
        assert!(parts[1].content_type.contains("image/jpeg"));
        assert!(parts[1].data.starts_with(b"\xff\xd8\xff"));

        let msg = decode_mms_best_effort(&bytes);
        assert!(msg.parts.iter().any(|p| p.data == text));
    }

    #[test]
    fn scan_subject_header() {
        // Subject 0x96 (field 0x16), encoded-string: value-length 4, charset UTF-8, "Hi\0"
        let bytes = [0x96u8, 0x04, 0xea, b'H', b'i', 0x00];
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.subject.as_deref(), Some("Hi"));
    }

    #[test]
    fn scan_status_header() {
        // Status 0x95 (field 0x15), Retrieved = short-int 0x81
        let bytes = [0x95u8, 0x81];
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.status.as_deref(), Some("Retrieved"));
    }

    #[test]
    fn scan_response_status_and_text() {
        // Response-Status Ok (0x92, short-int 0x80) + Response-Text "Error"
        let mut bytes = vec![0x92u8, 0x80];
        bytes.push(0x93);
        bytes.extend_from_slice(b"Error\0");
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.response_status.as_deref(), Some("Ok"));
        assert_eq!(msg.response_text.as_deref(), Some("Error"));
    }

    #[test]
    fn scan_message_size_header() {
        // Message-Size 0x8e + long-int length 2, value 0x1388 = 5000
        let bytes = [0x8eu8, 0x02, 0x13, 0x88];
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.message_size, Some(5000));
    }

    #[test]
    fn go_0x8e_named_part_not_message_size() {
        let mut bytes = Vec::new();
        bytes.push(0x8e);
        bytes.extend_from_slice(b"text.txt\0Hello body");
        let msg = decode_mms_best_effort(&bytes);
        assert!(msg.message_size.is_none());
        assert_eq!(msg.named_parts.len(), 1);
        assert_eq!(msg.named_parts[0].name, "text.txt");
        assert_eq!(msg.named_parts[0].data, b"Hello body");
    }

    #[test]
    fn named_text_part_keeps_non_ascii_utf8() {
        // Continuation bytes (>= 0x80) are part of the text, not part terminators.
        let mut bytes = Vec::new();
        bytes.push(0x8e);
        bytes.extend_from_slice(b"text.txt\0");
        bytes.extend_from_slice("Héllo — привет, 世界 🌍".as_bytes());
        bytes.push(0x8c); // trailing header byte, as in real GO dumps
        let named = scan_named_parts(&bytes);
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].name, "text.txt");
        assert_eq!(named[0].data, "Héllo — привет, 世界 🌍".as_bytes());
    }

    #[test]
    fn named_text_part_stops_at_header_byte() {
        // ASCII payload followed by a high-bit header byte: payload ends at the
        // header byte, which is not valid UTF-8 on its own.
        let mut bytes = Vec::new();
        bytes.push(0x8e);
        bytes.extend_from_slice(b"text.txt\0Hello");
        bytes.push(0x8c);
        bytes.extend_from_slice(&[0xff, 0xd8]); // JPEG SOI after the header byte
        let named = scan_named_parts(&bytes);
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].name, "text.txt");
        assert_eq!(named[0].data, b"Hello");
    }

    #[test]
    fn scan_subject_ucs2() {
        // Subject + UCS-2 charset (MIBEnum 1000 as long-int) + "Hi" UTF-16BE
        let bytes = [0x96u8, 0x07, 0x02, 0x03, 0xe8, 0x00, 0x48, 0x00, 0x69];
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.subject.as_deref(), Some("Hi"));
    }

    #[test]
    fn scan_bcc_transaction_class_version() {
        let mut bytes = Vec::new();
        // Bcc +15551234567/TYPE=PLMN (UTF-8 encoded-string)
        bytes.push(0x81); // Bcc
        bytes.push(0x18);
        bytes.push(0xea);
        bytes.extend_from_slice(b"+15551234567/TYPE=PLMN");
        // Transaction-Id
        bytes.push(0x98);
        bytes.extend_from_slice(b"tx-abc\0");
        // Message-Class Personal
        bytes.push(0x8a);
        bytes.push(0x80);
        // MMS-Version 1.2
        bytes.push(0x8d);
        bytes.push(0x92); // 0x12 | 0x80
        let msg = scan_mms_addresses(&bytes);
        assert!(msg.bcc.iter().any(|a| a.contains("15551234567")));
        assert_eq!(msg.transaction_id.as_deref(), Some("tx-abc"));
        assert_eq!(msg.message_class.as_deref(), Some("Personal"));
        assert_eq!(msg.mms_version.as_deref(), Some("1.2"));
    }

    #[test]
    fn application_header_captured() {
        // Full-ish header: Message-Type m-retrieve-conf, app header, Content-Type text/plain
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x8c, 0x84]); // Message-Type retrieve-conf
        bytes.extend_from_slice(b"X-Custom\0hello-app\0");
        bytes.extend_from_slice(&[0x84, 0x83]); // Content-Type text/plain
        let msg = decode_mms(&bytes).expect("decode");
        assert_eq!(
            msg.application_headers.get("X-Custom").map(String::as_str),
            Some("hello-app")
        );
    }

    #[test]
    fn part_filename_from_content_type_param() {
        let related_idx = WELL_KNOWN_CONTENT_TYPES
            .iter()
            .position(|s| *s == "application/vnd.wap.multipart.related")
            .expect("related ct");
        let related_si = (related_idx as u8) | 0x80;
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x11u8, 80));

        // Content-Type general form: value-length, image/jpeg, Filename param
        // length = 1 (media si) + 1 (Filename si) + 9 ("photo.jpg\0") = 11 = 0x0b
        let mut headers = vec![
            0x0b, // value-length
            0x97, // image/jpeg
            0x86, // Filename (0x06|0x80)
        ];
        headers.extend_from_slice(b"photo.jpg\0");

        let mut body = vec![0x01, headers.len() as u8, jpeg.len() as u8];
        body.extend_from_slice(&headers);
        body.extend_from_slice(&jpeg);

        let mut bytes = vec![0x84, related_si];
        bytes.extend_from_slice(&body);
        let parts = scan_multipart_bodies(&bytes);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].filename.as_deref(), Some("photo.jpg"));
        assert_eq!(parts[0].content_location.as_deref(), Some("photo.jpg"));
    }

    #[test]
    fn part_filename_from_content_disposition() {
        let related_idx = WELL_KNOWN_CONTENT_TYPES
            .iter()
            .position(|s| *s == "application/vnd.wap.multipart.related")
            .expect("related ct");
        let related_si = (related_idx as u8) | 0x80;
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x22u8, 80));

        // CT short jpeg + Content-Disposition attachment with Filename
        // CD: value-length = 1 (token) + 1 (Filename) + 8 ("pic.png\0") = 10
        let mut headers = vec![
            0x97, // image/jpeg
            0xae, // Content-Disposition
            0x0a, // value-length
            0x81, // attachment
            0x86, // Filename
        ];
        headers.extend_from_slice(b"pic.png\0");

        let mut body = vec![0x01, headers.len() as u8, jpeg.len() as u8];
        body.extend_from_slice(&headers);
        body.extend_from_slice(&jpeg);

        let mut bytes = vec![0x84, related_si];
        bytes.extend_from_slice(&body);
        let parts = scan_multipart_bodies(&bytes);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].filename.as_deref(), Some("pic.png"));
    }

    #[test]
    fn content_type_charset_and_filename_params() {
        // value-length: jpeg(1) + Charset si(1)+UTF-8(1) + Filename(1)+name(9) = 13
        let mut headers = vec![
            0x0d, 0x97, // image/jpeg
            0x88, // Charset
            0xea, // UTF-8
            0x86, // Filename
        ];
        headers.extend_from_slice(b"photo.jpg\0");
        let mut cur = Cursor::new(&headers);
        let (ct, params) = decode_content_type_value(&mut cur).expect("ct");
        assert!(ct.contains("jpeg"));
        assert_eq!(params.get("Charset").map(String::as_str), Some("106"));
        assert_eq!(
            params.get("Filename").map(String::as_str),
            Some("photo.jpg")
        );
    }

    #[test]
    fn multipart_0x83_is_not_content_location() {
        let related_idx = WELL_KNOWN_CONTENT_TYPES
            .iter()
            .position(|s| *s == "application/vnd.wap.multipart.related")
            .expect("related");
        let related_si = (related_idx as u8) | 0x80;
        let text = b"hi";
        // CT text/plain + spurious 0x83 (WSP Accept-Language id) + short-int value
        let headers = vec![0x83, 0x83, 0x80];
        let mut body = vec![0x01, headers.len() as u8, text.len() as u8];
        body.extend_from_slice(&headers);
        body.extend_from_slice(text);
        let mut bytes = vec![0x84, related_si];
        bytes.extend_from_slice(&body);
        let parts = scan_multipart_bodies(&bytes);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].content_location.is_none());
    }

    #[test]
    fn go_0x8e_after_from_soft_stops_header_decode() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x8c, 0x84]); // m-retrieve-conf
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.push(0x8e);
        bytes.extend_from_slice(b"text.txt\0Hello soft");
        let msg = decode_mms_best_effort(&bytes);
        assert!(msg.from.is_some());
        assert!(msg.message_size.is_none());
        assert_eq!(msg.named_parts.len(), 1);
        assert_eq!(msg.named_parts[0].data, b"Hello soft");
    }

    #[test]
    fn empty_subject_does_not_drop_following_to() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        // Empty Subject text-string
        bytes.extend_from_slice(&[0x96, 0x00]);
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.push(0x8c); // pad
        let msg = scan_mms_addresses(&bytes);
        assert!(msg.from.is_some());
        assert!(msg.to.iter().any(|t| t.contains("5555550100")));
        assert!(msg.subject.is_none());
    }
}
