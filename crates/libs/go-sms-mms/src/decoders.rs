//! WAP-209 / WAP-230 unit decoders moved out of `mms_enc`.

use crate::mms_enc::*;
use std::collections::HashMap;

pub(crate) fn is_mms_short_integer_field(field: u8) -> bool {
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

pub(crate) fn yes_no_token(v: u8) -> Option<&'static str> {
    match v {
        0x00 => Some("yes"),
        0x01 => Some("no"),
        _ => None,
    }
}

pub(crate) fn priority_token(v: u8) -> Option<&'static str> {
    match v {
        0x00 => Some("Low"),
        0x01 => Some("Normal"),
        0x02 => Some("High"),
        _ => None,
    }
}

#[derive(Debug)]
pub(crate) struct Cursor<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    pub(crate) fn next_byte(&mut self) -> Result<u8, ()> {
        let b = self.peek().ok_or(())?;
        self.pos += 1;
        Ok(b)
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], ()> {
        if self.remaining() < n {
            return Err(());
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
}

pub(crate) fn decode_uint_var(cur: &mut Cursor<'_>) -> Result<u64, ()> {
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

pub(crate) fn decode_value_length(cur: &mut Cursor<'_>) -> Result<usize, ()> {
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

pub(crate) fn decode_text_string(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_short_integer(cur: &mut Cursor<'_>) -> Result<u8, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte & 0x80 == 0 {
        return Err(());
    }
    cur.next_byte()?;
    Ok(byte & 0x7f)
}

pub(crate) fn decode_long_integer(cur: &mut Cursor<'_>) -> Result<u64, ()> {
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

pub(crate) fn decode_integer_value(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    if let Ok(v) = decode_short_integer(cur) {
        return Ok(u64::from(v));
    }
    decode_long_integer(cur)
}

pub(crate) fn trim_encoded_string_junk(s: &str) -> String {
    // GO value-lengths often swallow the next header; keep a clean PLMN address.
    if let Some(idx) = s.find("/TYPE=PLMN") {
        return s[..idx + "/TYPE=PLMN".len()].to_string();
    }
    s.trim_end_matches('\0').to_string()
}

/// Encoded-string-value = Text-string | Value-length Char-set Text-string
///
/// GO SMS Pro PDUs often declare a Value-length that overlaps the next MMS
/// short-integer header. Read text until NUL or a high-bit header byte; do not
/// blindly consume through `end` when that would swallow the next field.
pub(crate) fn decode_encoded_string_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_delta_seconds(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    decode_long_integer(cur).or_else(|_| decode_integer_value(cur))
}

/// Absolute-token Date-value | Relative-token Delta-seconds-value inside Value-length.
pub(crate) fn decode_expiry_or_delivery_time(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_mms_version(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    let major = (v >> 4) & 0x0f;
    let minor = v & 0x0f;
    Ok(format!("{major}.{minor}"))
}

pub(crate) fn decode_message_class_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_status_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_response_status_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_sender_visibility_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    Ok(match v {
        0x00 => "Hide".into(),
        0x01 => "Show".into(),
        other => format!("unknown-0x{other:02x}"),
    })
}

/// From-value = Value-length (Address-present-token Encoded-string-value | Insert-address-token)
pub(crate) fn decode_from_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn decode_date_value(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    decode_long_integer(cur)
}

pub(crate) fn decode_message_type_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn well_known_content_type(id: u64) -> Option<&'static str> {
    WELL_KNOWN_CONTENT_TYPES.get(id as usize).copied()
}

pub(crate) fn decode_constrained_media(cur: &mut Cursor<'_>) -> Result<String, ()> {
    if let Ok(id) = decode_short_integer(cur) {
        return well_known_content_type(u64::from(id))
            .map(str::to_string)
            .ok_or(());
    }
    decode_text_string(cur)
}

pub(crate) fn decode_wsp_text_param(cur: &mut Cursor<'_>) -> Result<String, ()> {
    decode_text_string(cur).or_else(|_| decode_encoded_string_value(cur))
}

/// Decode a well-known WSP parameter by token id (table 38).
/// Charset/Type are integer-like; Name/Filename/Start are text-like.
pub(crate) fn decode_wsp_typed_param(
    cur: &mut Cursor<'_>,
    name_id: u8,
) -> Result<(String, String), ()> {
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
pub(crate) fn decode_wsp_parameters(cur: &mut Cursor<'_>, end: usize) -> HashMap<String, String> {
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

pub(crate) fn decode_content_type_value(
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
pub(crate) fn decode_content_disposition_value(
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

pub(crate) fn decode_application_header_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
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

pub(crate) fn skip_unknown_mms_value(cur: &mut Cursor<'_>) -> Result<(), ()> {
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

pub(crate) fn decode_mms_header_field(
    cur: &mut Cursor<'_>,
    msg: &mut StructuredMms,
) -> Result<bool, ()> {
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

pub(crate) fn decode_multipart_body(cur: &mut Cursor<'_>) -> Result<Vec<MmsPart>, ()> {
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

/// Returns `true` when the field was decoded and the outer scan cursor should advance to `cur.pos`.
pub(crate) fn apply_mms_header_field(
    field: u8,
    cur: &mut Cursor<'_>,
    msg: &mut StructuredMms,
) -> bool {
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
