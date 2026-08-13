//! Parse GO SMS Pro MMS PDU backup files (`I_<timestamp>_*.pdu`).
//!
//! Prefers WAP-209 / Content-Location / SMIL structured fields ([`crate::mms_enc`]),
//! then falls back to text-marker / magic-byte heuristics only when a field is empty.

use crate::emoji::decode_gosms_emojis;
use crate::mms_enc::{
    MmsPart, NamedPart, StructuredMms, content_type_from_filename, decode_bytes_with_charset,
    decode_mms_best_effort, extension_for_content_type, normalize_content_id,
};
use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use phone::sanitize_number;
use quick_xml::Reader;
use quick_xml::events::Event;
use regex::Regex;
use regex::bytes::Regex as BytesRegex;
use std::path::Path;
use std::sync::OnceLock;

static PDU_FILENAME_RE: OnceLock<Regex> = OnceLock::new();
static PLMN_RE: OnceLock<BytesRegex> = OnceLock::new();
static TEXT_CONTENT_RE: OnceLock<BytesRegex> = OnceLock::new();
static MMS_PART_JUNK_RE: OnceLock<Regex> = OnceLock::new();
static PRINTABLE_RUN_RE: OnceLock<BytesRegex> = OnceLock::new();
static TRAILING_GARBAGE_RE: OnceLock<Regex> = OnceLock::new();
static TEXT_PART_NAME_RE: OnceLock<Regex> = OnceLock::new();

const TEXT_PART_END_MARKERS: &[&[u8]] = &[
    b"\x8c",
    b"\xa0\x85",
    b"\x00\x85IMG",
    b"\x85IMG",
    b"\xff\xd8\xff",
    b"\x00\x8e",
    b"\x00\x85",
];

const ATTACHMENT_MAGICS: &[(&[u8], &str)] = &[
    (b"\xff\xd8\xff", ".jpg"),
    (b"\x89PNG\r\n\x1a\n", ".png"),
    (b"GIF87a", ".gif"),
    (b"GIF89a", ".gif"),
    (b"\x00\x00\x00\x18ftyp3gp", ".3gp"),
    (b"ftypmp42", ".mp4"),
    (b"#!AMR", ".amr"),
    (b"RIFF", ".wav"),
];

#[derive(Debug, Clone)]
pub struct ParsedAttachment {
    pub ext: String,
    pub data: Vec<u8>,
    pub smil_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldSource {
    Structured,
    Heuristic,
}

#[derive(Debug, Clone)]
pub struct ParsedPdu {
    pub path: std::path::PathBuf,
    pub timestamp: i64,
    pub participants: Vec<String>,
    pub body: String,
    pub attachments: Vec<ParsedAttachment>,
    pub is_sent: bool,
    pub is_group: bool,
    pub sender_number: String,
    /// Structured MMS From header was present (before digit sanitize).
    pub has_from: bool,
    /// Structured MMS To header list was non-empty.
    pub has_to: bool,
    /// Optional MMS headers (subject, message_id, …).
    pub pdu_fields: BTreeMap<String, String>,
    /// `structured` | `mixed` | `heuristic`
    pub decode_quality: &'static str,
}

#[derive(Debug, Default)]
struct SmilRefs {
    text_srcs: Vec<String>,
    media_srcs: Vec<String>,
}

fn timestamp_from_filename(name: &str) -> Option<i64> {
    let re = PDU_FILENAME_RE.get_or_init(|| Regex::new(r"^I_(?P<ts>\d+)_").expect("pdu name"));
    re.captures(name)
        .and_then(|c| c.name("ts"))
        .and_then(|m| m.as_str().parse().ok())
}

fn extract_plmn_numbers(data: &[u8]) -> Vec<String> {
    let re = PLMN_RE.get_or_init(|| BytesRegex::new(r"\+(\d{10,15})/TYPE=PLMN").expect("plmn"));
    let mut seen = std::collections::HashSet::new();
    let mut numbers = Vec::new();
    for caps in re.captures_iter(data) {
        let digits = String::from_utf8_lossy(&caps[1]).into_owned();
        if seen.insert(digits.clone()) {
            numbers.push(digits);
        }
    }
    numbers
}

/// Digits from an MMS address (`+1…/TYPE=PLMN` or bare digits).
fn digits_from_mms_address(addr: &str) -> Option<String> {
    let base = addr.split('/').next().unwrap_or(addr).trim();
    let trimmed = base.trim_start_matches('+');
    sanitize_number(trimmed).or_else(|| sanitize_number(base))
}

fn participants_from_structured(msg: &StructuredMms) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut numbers = Vec::new();
    for addr in msg.address_strings() {
        if let Some(digits) = digits_from_mms_address(&addr)
            && seen.insert(digits.clone())
        {
            numbers.push(digits);
        }
    }
    numbers
}

fn is_text_part_name(name: &str) -> bool {
    let re = TEXT_PART_NAME_RE
        .get_or_init(|| Regex::new(r"(?i)^text(?:_\d+)?\.txt$").expect("text name"));
    re.is_match(name)
}

fn text_from_part_data(data: &[u8], charset: Option<u64>) -> Option<String> {
    let text = decode_bytes_with_charset(data, charset)
        .replace('\0', "")
        .trim()
        .to_string();
    let text = truncate_mms_binary_tail(&text);
    if text.is_empty() || is_mms_part_junk(&text) {
        return None;
    }
    Some(decode_gosms_emojis(&text))
}

fn smil_src_matches_name(src: &str, name: &str) -> bool {
    let a = normalize_content_id(src).to_ascii_lowercase();
    let b = normalize_content_id(name).to_ascii_lowercase();
    !a.is_empty() && (a == b || src.eq_ignore_ascii_case(name))
}

fn part_matches_smil_src(part: &MmsPart, src: &str) -> bool {
    if let Some(cid) = &part.content_id
        && smil_src_matches_name(src, cid)
    {
        return true;
    }
    if let Some(loc) = &part.content_location
        && smil_src_matches_name(src, loc)
    {
        return true;
    }
    if let Some(name) = &part.filename
        && smil_src_matches_name(src, name)
    {
        return true;
    }
    false
}

fn part_display_name(part: &MmsPart) -> Option<String> {
    part.content_location
        .clone()
        .or_else(|| part.filename.clone())
        .or_else(|| part.content_id.clone())
}

fn is_smil_content_type(ct: &str) -> bool {
    let base = ct
        .split(';')
        .next()
        .unwrap_or(ct)
        .trim()
        .to_ascii_lowercase();
    base.contains("smil") || base == "application/smil"
}

fn looks_like_smil_bytes(data: &[u8]) -> bool {
    let lower = data.to_ascii_lowercase();
    lower.windows(5).any(|w| w == b"<smil")
}

fn body_from_named_parts(named: &[NamedPart], smil: &SmilRefs) -> Option<String> {
    for src in &smil.text_srcs {
        for part in named {
            if smil_src_matches_name(src, &part.name)
                && let Some(text) = text_from_part_data(&part.data, None)
            {
                return Some(text);
            }
        }
    }
    let mut texts = Vec::new();
    let mut seen = HashSet::new();
    for part in named {
        if !is_text_part_name(&part.name)
            && !content_type_from_filename(&part.name).starts_with("text/")
        {
            continue;
        }
        if let Some(text) = text_from_part_data(&part.data, None)
            && seen.insert(text.clone())
        {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn body_from_structured(msg: &StructuredMms, smil: &SmilRefs) -> Option<String> {
    if msg.parts.is_empty() {
        return None;
    }
    for src in &smil.text_srcs {
        for part in &msg.parts {
            if part_matches_smil_src(part, src)
                && let Some(text) = text_from_part_data(&part.data, part.charset)
            {
                return Some(text);
            }
        }
    }
    if let Some(start) = &msg.content_start {
        for part in &msg.parts {
            if !part_matches_smil_src(part, start) {
                continue;
            }
            if is_smil_content_type(&part.content_type) || looks_like_smil_bytes(&part.data) {
                continue;
            }
            let ct = part.content_type.to_ascii_lowercase();
            let base = ct.split(';').next().unwrap_or(&ct).trim();
            if base.starts_with("text/")
                && let Some(text) = text_from_part_data(&part.data, part.charset)
            {
                return Some(text);
            }
        }
    }
    let mut texts = Vec::new();
    let mut seen = HashSet::new();
    for part in &msg.parts {
        let ct = part.content_type.to_ascii_lowercase();
        let base = ct.split(';').next().unwrap_or(&ct).trim();
        if !(base.starts_with("text/plain") || base == "text/*" || base == "text/html") {
            continue;
        }
        if let Some(text) = text_from_part_data(&part.data, part.charset)
            && seen.insert(text.clone())
        {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn ext_from_filename(name: &str) -> Option<String> {
    let ct = content_type_from_filename(name);
    extension_for_content_type(&ct).map(|e| e.to_string())
}

fn attachment_ok(ext: &str, len: usize) -> bool {
    if len < 64 && matches!(ext, ".jpg" | ".png" | ".gif") {
        return false;
    }
    if ext == ".wav" && len < 10000 {
        return false;
    }
    true
}

fn attachments_from_named_parts(named: &[NamedPart], smil: &SmilRefs) -> Vec<ParsedAttachment> {
    let use_smil = !smil.media_srcs.is_empty();
    let mut out = Vec::new();
    for part in named {
        if is_text_part_name(&part.name) {
            continue;
        }
        let Some(ext) = ext_from_filename(&part.name) else {
            continue;
        };
        if ext == ".txt" {
            continue;
        }
        let smil_name = if use_smil {
            smil.media_srcs
                .iter()
                .find(|src| smil_src_matches_name(src, &part.name))
                .cloned()
        } else {
            Some(part.name.clone())
        };
        if use_smil && smil_name.is_none() {
            continue;
        }
        if !attachment_ok(&ext, part.data.len()) {
            continue;
        }
        out.push(ParsedAttachment {
            ext,
            data: part.data.clone(),
            smil_name: smil_name.or_else(|| Some(part.name.clone())),
        });
    }
    out
}

fn attachments_from_structured(msg: &StructuredMms, smil: &SmilRefs) -> Vec<ParsedAttachment> {
    let use_smil = !smil.media_srcs.is_empty();
    let mut out = Vec::new();
    for part in &msg.parts {
        if is_smil_content_type(&part.content_type) || looks_like_smil_bytes(&part.data) {
            continue;
        }
        let ext = part
            .filename
            .as_deref()
            .and_then(ext_from_filename)
            .or_else(|| part.content_location.as_deref().and_then(ext_from_filename))
            .or_else(|| extension_for_content_type(&part.content_type).map(str::to_string));
        let Some(ext) = ext else {
            continue;
        };
        if ext == ".txt" {
            continue;
        }
        let smil_name = if use_smil {
            smil.media_srcs
                .iter()
                .find(|src| part_matches_smil_src(part, src))
                .cloned()
        } else {
            part_display_name(part)
        };
        if use_smil && smil_name.is_none() {
            continue;
        }
        if !attachment_ok(&ext, part.data.len()) {
            continue;
        }
        out.push(ParsedAttachment {
            ext,
            data: part.data.clone(),
            smil_name,
        });
    }
    out
}

fn extract_smil_region(data: &[u8]) -> Option<&[u8]> {
    let lower = data.to_ascii_lowercase();
    let start = lower.windows(5).position(|w| w == b"<smil")?;
    let end_rel = lower[start..]
        .windows(7)
        .position(|w| w == b"</smil>")
        .map(|p| start + p + 7)?;
    Some(&data[start..end_rel])
}

fn parse_smil_refs(data: &[u8]) -> SmilRefs {
    let mut refs = SmilRefs::default();
    let Some(smil_bytes) = extract_smil_region(data) else {
        return refs;
    };
    let Ok(text) = std::str::from_utf8(smil_bytes) else {
        return refs;
    };
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                let mut src = None;
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                    if key == "src" {
                        src = Some(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
                if let Some(s) = src {
                    if s.is_empty() {
                        continue;
                    }
                    match tag.as_str() {
                        "text" => refs.text_srcs.push(s),
                        "img" | "audio" | "video" => refs.media_srcs.push(s),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    refs
}

fn truncate_mms_binary_tail(text: &str) -> String {
    let mut text = text.to_string();
    if let Some(img_idx) = text.find("IMG_")
        && img_idx > 0
    {
        text.truncate(img_idx);
    }
    let trailing =
        TRAILING_GARBAGE_RE.get_or_init(|| Regex::new(r"^(.+!!)[^\w\s]{0,12}$").expect("trail"));
    if let Some(caps) = trailing.captures(&text) {
        text = caps[1].to_string();
    }
    for (index, ch) in text.char_indices() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            continue;
        }
        let code = ch as u32;
        if code < 32 || code == 127 {
            return text[..index].trim_end().to_string();
        }
    }
    text.trim().to_string()
}

fn is_mms_part_junk(text: &str) -> bool {
    let re = MMS_PART_JUNK_RE.get_or_init(|| {
        Regex::new(
            r#"(?i)^(?:text_\d+\.txt|"?<text_\d+>?|"<\d+>|"<text_\d+\.txt>|IMG_\d+\.[A-Za-z]{3,4})$"#,
        )
        .expect("junk")
    });
    re.is_match(text)
}

fn extract_text_after_marker(data: &[u8], start: usize) -> String {
    let mut end = data.len();
    for sep in TEXT_PART_END_MARKERS {
        if let Some(pos) = find_bytes(data, sep, start) {
            end = end.min(pos);
        }
    }
    text_from_part_data(&data[start..end], None).unwrap_or_default()
}

fn find_bytes(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack[start..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| start + p)
}

/// Last-resort body when Content-Location / multipart text is missing.
fn extract_wap_text_body_fallback(data: &[u8]) -> String {
    let re = TEXT_CONTENT_RE
        .get_or_init(|| BytesRegex::new(r"(?-u)\x8etext(?:_\d+)?\.txt\x00").expect("txt"));
    let mut texts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in re.find_iter(data) {
        let text = extract_text_after_marker(data, m.end());
        if !text.is_empty() && seen.insert(text.clone()) {
            texts.push(text);
        }
    }
    if !texts.is_empty() {
        return decode_gosms_emojis(&texts.join("\n"));
    }

    if let Some(smil_end) = find_bytes(data, b"</smil>", 0) {
        let tail = &data[smil_end + 7..];
        let run_re = PRINTABLE_RUN_RE
            .get_or_init(|| BytesRegex::new(r"(?-u)[\x20-\x7e\n\r\t]{8,}").expect("run"));
        if let Some(m) = run_re.find(tail) {
            let text = String::from_utf8_lossy(m.as_bytes()).trim().to_string();
            if !text.is_empty() && !text.starts_with('<') && !is_mms_part_junk(&text) {
                return decode_gosms_emojis(&text);
            }
        }
    }
    String::new()
}

fn detect_attachment_blobs(data: &[u8]) -> Vec<(String, usize, usize)> {
    if data.len() < 32 {
        return Vec::new();
    }
    let mut hits: Vec<(usize, &str)> = Vec::new();
    for &(sig, ext) in ATTACHMENT_MAGICS {
        let mut start = 0;
        while let Some(rel) = find_bytes(data, sig, start) {
            hits.push((rel, ext));
            start = rel + 1;
        }
    }
    if hits.is_empty() {
        return Vec::new();
    }
    hits.sort_by_key(|(idx, _)| *idx);
    let mut merged = Vec::new();
    for (idx, (start, ext)) in hits.iter().enumerate() {
        let next_start = hits.get(idx + 1).map(|(s, _)| *s).unwrap_or(data.len());
        let size = next_start - start;
        if !attachment_ok(ext, size) {
            continue;
        }
        merged.push((ext.to_string(), *start, next_start));
    }
    merged
}

fn unique_participants(parts: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for p in parts {
        if seen.insert(p.clone()) {
            unique.push(p.clone());
        }
    }
    unique
}

fn is_owner_digit(digits: &str, owners: &HashSet<String>) -> bool {
    sanitize_number(digits).is_some_and(|d| owners.contains(&d))
}

fn roles_from_structured(
    msg: &StructuredMms,
    owners: &HashSet<String>,
) -> (Option<String>, bool, bool) {
    let from_digits = msg.from.as_ref().and_then(|a| digits_from_mms_address(a));
    let my_is_from = from_digits
        .as_ref()
        .is_some_and(|d| is_owner_digit(d, owners));
    let my_is_to = msg
        .to
        .iter()
        .chain(msg.cc.iter())
        .filter_map(|a| digits_from_mms_address(a))
        .any(|d| is_owner_digit(&d, owners));
    (from_digits, my_is_from, my_is_to)
}

/// Direction from decoded From/To/Cc when present; otherwise owner/participant rules
/// (no byte-prefix markers — sent fixtures often lack From/To headers entirely).
fn infer_pdu_direction(
    structured: &StructuredMms,
    unique_parts: &[String],
    owners: &HashSet<String>,
    primary_digits: &str,
) -> (bool, String) {
    if unique_parts.is_empty() {
        return (false, String::new());
    }

    let has_roles = structured.from.is_some()
        || !structured.to.is_empty()
        || !structured.cc.is_empty()
        || !structured.bcc.is_empty();

    if has_roles {
        let (from_digits, my_is_from, my_is_to) = roles_from_structured(structured, owners);
        if my_is_from {
            return (true, primary_digits.to_string());
        }
        if let Some(from) = from_digits {
            if !is_owner_digit(&from, owners) {
                return (false, from);
            }
            return (true, primary_digits.to_string());
        }
        if my_is_to {
            let sender = unique_parts
                .iter()
                .find(|p| !is_owner_digit(p, owners))
                .cloned()
                .unwrap_or_else(|| unique_parts[0].clone());
            return (false, sender);
        }
    }

    // Raw PLMN lists without From/To headers (e.g. sent one-to-one dumps).
    // Owner presence is the primary direction signal: a sent group MMS (owner +
    // ≥ 2 recipients, no headers) must not be read as received just because the
    // participant list is long. "Received" is only inferred when no participant
    // is an owner number, so the >= 3 branch below never sees an all-owner list.
    if unique_parts.iter().any(|p| is_owner_digit(p, owners)) {
        return (true, primary_digits.to_string());
    }

    if unique_parts.len() >= 3 {
        let sender = unique_parts
            .iter()
            .find(|p| !is_owner_digit(p, owners))
            .cloned()
            .unwrap_or_else(|| unique_parts[0].clone());
        return (false, sender);
    }

    (false, unique_parts[0].clone())
}

fn resolve_timestamp(filename_ts: i64, structured: &StructuredMms) -> (i64, FieldSource) {
    match structured.date_unix {
        Some(d) if d > 0 && d <= i64::MAX as u64 => (d as i64, FieldSource::Structured),
        _ => (filename_ts, FieldSource::Heuristic),
    }
}

fn insert_nonempty(fields: &mut BTreeMap<String, String>, key: &str, value: &Option<String>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        fields.insert(key.into(), v.clone());
    }
}

fn pdu_fields_from_structured(msg: &StructuredMms) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    insert_nonempty(&mut fields, "subject", &msg.subject);
    insert_nonempty(&mut fields, "message_id", &msg.message_id);
    insert_nonempty(&mut fields, "delivery_report", &msg.delivery_report);
    insert_nonempty(&mut fields, "read_report", &msg.read_report);
    insert_nonempty(&mut fields, "priority", &msg.priority);
    insert_nonempty(&mut fields, "message_type", &msg.message_type);
    insert_nonempty(&mut fields, "delivery_time", &msg.delivery_time);
    insert_nonempty(&mut fields, "expiry", &msg.expiry);
    insert_nonempty(&mut fields, "message_class", &msg.message_class);
    insert_nonempty(&mut fields, "mms_version", &msg.mms_version);
    if let Some(sz) = msg.message_size {
        fields.insert("message_size".into(), sz.to_string());
    }
    insert_nonempty(&mut fields, "report_allowed", &msg.report_allowed);
    insert_nonempty(&mut fields, "response_status", &msg.response_status);
    insert_nonempty(&mut fields, "response_text", &msg.response_text);
    insert_nonempty(&mut fields, "sender_visibility", &msg.sender_visibility);
    insert_nonempty(&mut fields, "status", &msg.status);
    insert_nonempty(&mut fields, "transaction_id", &msg.transaction_id);
    if !msg.bcc.is_empty() {
        fields.insert("bcc".into(), msg.bcc.join(","));
    }
    for (name, value) in &msg.application_headers {
        if !value.is_empty() {
            fields.insert(format!("app:{name}"), value.clone());
        }
    }
    fields
}

fn score_decode_quality(
    body: FieldSource,
    attachments: FieldSource,
    direction: FieldSource,
    timestamp: FieldSource,
) -> &'static str {
    // Filename timestamps are normal for GO fragments; they alone do not demote
    // a row from `structured` when body/attachments/direction are structured.
    let content = [body, attachments, direction];
    if content.iter().all(|s| *s == FieldSource::Structured) {
        return "structured";
    }
    if body == FieldSource::Heuristic && attachments == FieldSource::Heuristic {
        return "heuristic";
    }
    if content.iter().all(|s| *s == FieldSource::Heuristic) && timestamp == FieldSource::Heuristic {
        return "heuristic";
    }
    "mixed"
}

/// Parse one PDU file. Returns `None` for unparseable files or bad filenames.
///
/// # Errors
///
/// Returns an error when the file cannot be read.
pub fn parse_pdu_file(
    path: &Path,
    owners: &HashSet<String>,
    primary_digits: &str,
) -> Result<Option<ParsedPdu>> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if data.len() < 10 {
        return Ok(None);
    }
    let Some(filename_ts) =
        timestamp_from_filename(path.file_name().and_then(|s| s.to_str()).unwrap_or(""))
    else {
        return Ok(None);
    };

    let structured = decode_mms_best_effort(&data);
    let smil = parse_smil_refs(&data);
    let (timestamp, ts_src) = resolve_timestamp(filename_ts, &structured);
    let pdu_fields = pdu_fields_from_structured(&structured);

    let participants_raw = {
        let mut parts = participants_from_structured(&structured);
        if parts.is_empty() {
            extract_plmn_numbers(&data)
        } else {
            let mut seen: HashSet<String> = parts.iter().cloned().collect();
            for n in extract_plmn_numbers(&data) {
                if seen.insert(n.clone()) {
                    parts.push(n);
                }
            }
            parts
        }
    };

    let (body, body_src) = if let Some(b) = body_from_named_parts(&structured.named_parts, &smil) {
        (b, FieldSource::Structured)
    } else if let Some(b) = body_from_structured(&structured, &smil) {
        (b, FieldSource::Structured)
    } else if let Some(subject) = structured
        .subject
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        (decode_gosms_emojis(subject), FieldSource::Structured)
    } else {
        let b = extract_wap_text_body_fallback(&data);
        let src = if b.is_empty() {
            FieldSource::Structured
        } else {
            FieldSource::Heuristic
        };
        (b, src)
    };

    let mut attachments = attachments_from_named_parts(&structured.named_parts, &smil);
    let mut atts_src = FieldSource::Structured;
    if attachments.is_empty() {
        attachments = attachments_from_structured(&structured, &smil);
    }
    if attachments.is_empty() {
        let blobs = detect_attachment_blobs(&data);
        for (i, (ext, start, end)) in blobs.into_iter().enumerate() {
            let smil_name = smil.media_srcs.get(i).cloned();
            attachments.push(ParsedAttachment {
                ext,
                data: data[start..end].to_vec(),
                smil_name,
            });
        }
        if !attachments.is_empty() {
            atts_src = FieldSource::Heuristic;
        }
    }

    let normalized_parts: Vec<String> = participants_raw
        .iter()
        .filter_map(|p| sanitize_number(p))
        .collect();
    let unique_parts = unique_participants(&normalized_parts);
    let is_group = unique_parts.len() >= 3;
    let has_roles = structured.from.is_some()
        || !structured.to.is_empty()
        || !structured.cc.is_empty()
        || !structured.bcc.is_empty();
    let dir_src = if has_roles {
        FieldSource::Structured
    } else {
        FieldSource::Heuristic
    };
    let (is_sent, sender_number) =
        infer_pdu_direction(&structured, &unique_parts, owners, primary_digits);

    let decode_quality = score_decode_quality(body_src, atts_src, dir_src, ts_src);

    Ok(Some(ParsedPdu {
        path: path.to_path_buf(),
        timestamp,
        participants: unique_parts,
        body,
        attachments,
        is_sent,
        is_group,
        sender_number,
        has_from: structured.from.is_some(),
        has_to: !structured.to.is_empty(),
        pdu_fields,
        decode_quality,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdu")
            .join(name)
    }

    fn test_owners() -> (HashSet<String>, String) {
        let primary = "5555550100".to_string();
        let mut owners = HashSet::new();
        owners.insert(primary.clone());
        (owners, primary)
    }

    #[test]
    fn invalid_filename_returns_none() {
        let (owners, primary) = test_owners();
        let r = parse_pdu_file(&fixture("bad_name.pdu"), &owners, &primary).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn received_one_to_one() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_recv.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "Hello one to one");
        assert_eq!(
            parsed.participants,
            vec!["4075551234".to_string(), "5555550100".to_string()]
        );
        assert!(!parsed.is_sent);
        assert!(!parsed.is_group);
        assert_eq!(parsed.sender_number, "4075551234");
        assert_eq!(parsed.timestamp, 1609459200);
        assert_eq!(parsed.decode_quality, "structured");
    }

    #[test]
    fn sent_one_to_one() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_sent.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "Sent MMS");
        assert!(parsed.is_sent);
        assert!(!parsed.is_group);
        assert_eq!(parsed.sender_number, "5555550100");
        // No From/To headers → direction falls back to owner rules.
        assert_eq!(parsed.decode_quality, "mixed");
    }

    #[test]
    fn group_pdu() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_group.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "Group MMS body");
        assert!(parsed.is_group);
        assert_eq!(
            parsed.participants,
            vec![
                "5551112222".to_string(),
                "5552223333".to_string(),
                "5553334444".to_string(),
                "5555550100".to_string()
            ]
        );
        assert!(!parsed.is_sent);
        assert_eq!(parsed.sender_number, "5551112222");
    }

    #[test]
    fn sent_group_without_headers_is_sent() {
        // Owner + 2 recipients, no From/To/Cc headers. The long participant
        // list alone must not flip the direction to "received".
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("I_1609459200_sentgrp.pdu");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN"); // owner
        bytes.extend_from_slice(b"+15551112222/TYPE=PLMN"); // recipient 1
        bytes.extend_from_slice(b"+15552223333/TYPE=PLMN"); // recipient 2
        bytes.extend_from_slice(&[0x8e]);
        bytes.extend_from_slice(b"text.txt\0Group sent MMS");
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert!(parsed.is_group);
        assert!(parsed.is_sent);
        assert_eq!(
            parsed.participants,
            vec![
                "5555550100".to_string(),
                "5551112222".to_string(),
                "5552223333".to_string(),
            ]
        );
    }

    #[test]
    fn received_group_without_headers_stays_received() {
        // No owner among the participants: still a received group MMS from the
        // first listed number.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("I_1609459200_recvgrp.pdu");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"+15551112222/TYPE=PLMN");
        bytes.extend_from_slice(b"+15552223333/TYPE=PLMN");
        bytes.extend_from_slice(b"+15553334444/TYPE=PLMN");
        bytes.extend_from_slice(&[0x8e]);
        bytes.extend_from_slice(b"text.txt\0Group recv MMS");
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert!(parsed.is_group);
        assert!(!parsed.is_sent);
        assert_eq!(parsed.sender_number, "5551112222");
    }

    #[test]
    fn jpeg_attachment() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_att.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].ext, ".jpg");
        assert!(parsed.attachments[0].data.len() >= 256);
        // Named text body + magic-byte JPEG.
        assert_eq!(parsed.decode_quality, "mixed");
    }

    #[test]
    fn message_size_in_pdu_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("I_1609459200_msize.pdu");
        let mut bytes = Vec::new();
        // Message-Size 5000
        bytes.extend_from_slice(&[0x8e, 0x02, 0x13, 0x88]);
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.push(0x8c); // pad
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(
            parsed.pdu_fields.get("message_size").map(String::as_str),
            Some("5000")
        );
    }

    #[test]
    fn recv_fixture_0x8e_is_named_part_not_message_size() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_recv.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert!(!parsed.pdu_fields.contains_key("message_size"));
        assert_eq!(parsed.body, "Hello one to one");
    }

    #[test]
    fn subject_used_as_body_when_no_text_part() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("I_1609459200_subject.pdu");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.extend_from_slice(&[0x8e]); // overshoot pad for To length
        // Subject "SubjOnly" as text-string (0x96 = Subject 0x16|0x80)
        bytes.push(0x96);
        bytes.extend_from_slice(b"SubjOnly\0");
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "SubjOnly");
        assert_eq!(
            parsed.pdu_fields.get("subject").map(String::as_str),
            Some("SubjOnly")
        );
    }

    #[test]
    fn body_from_content_location_without_marker_regex() {
        let data = std::fs::read(fixture("I_1609459200_recv.pdu")).unwrap();
        let structured = decode_mms_best_effort(&data);
        let smil = SmilRefs::default();
        let body = body_from_named_parts(&structured.named_parts, &smil).expect("named body");
        assert_eq!(body, "Hello one to one");
    }

    #[test]
    fn smil_binds_text_and_image_parts() {
        let mut data = Vec::new();
        data.extend_from_slice(
            b"<smil><body><text src=\"text.txt\"/><img src=\"IMG_1.jpg\"/></body></smil>",
        );
        data.extend_from_slice(&[0x8e]);
        data.extend_from_slice(b"text.txt\0Hello from SMIL");
        data.extend_from_slice(&[0x8e]);
        data.extend_from_slice(b"IMG_1.jpg\0");
        // Minimal JPEG large enough to pass size guard
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x00, 80));
        data.extend_from_slice(&jpeg);

        let structured = decode_mms_best_effort(&data);
        let smil = parse_smil_refs(&data);
        assert_eq!(smil.text_srcs, vec!["text.txt".to_string()]);
        assert_eq!(smil.media_srcs, vec!["IMG_1.jpg".to_string()]);
        let body = body_from_named_parts(&structured.named_parts, &smil).unwrap();
        assert_eq!(body, "Hello from SMIL");
        let atts = attachments_from_named_parts(&structured.named_parts, &smil);
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].ext, ".jpg");
        assert_eq!(atts[0].smil_name.as_deref(), Some("IMG_1.jpg"));
    }

    #[test]
    fn smil_src_matches_filename_case_insensitively() {
        let mut data = Vec::new();
        data.extend_from_slice(b"<smil><body><img src=\"img_1.jpg\"/></body></smil>");
        data.extend_from_slice(&[0x8e]);
        data.extend_from_slice(b"IMG_1.jpg\0");
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x00, 80));
        data.extend_from_slice(&jpeg);

        let structured = decode_mms_best_effort(&data);
        let smil = parse_smil_refs(&data);
        let atts = attachments_from_named_parts(&structured.named_parts, &smil);
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].smil_name.as_deref(), Some("img_1.jpg"));
    }

    #[test]
    fn smil_cid_binds_to_part_content_id() {
        let text = b"Body via cid";
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x11u8, 80));

        // Multipart related: text + jpeg with Content-ID headers
        let mut body = Vec::new();
        body.push(0x02); // nEntries
        // text/plain + Content-ID <text1>
        let text_headers = {
            let mut h = vec![0x83]; // text/plain
            h.push(0xc0); // Content-ID
            h.extend_from_slice(b"<text1>\0");
            h
        };
        body.push(text_headers.len() as u8);
        body.push(text.len() as u8);
        body.extend_from_slice(&text_headers);
        body.extend_from_slice(text);
        // image/jpeg + Content-ID <img1>
        let img_headers = {
            let mut h = vec![0x97]; // image/jpeg
            h.push(0xc0);
            h.extend_from_slice(b"<img1>\0");
            h
        };
        body.push(img_headers.len() as u8);
        body.push(jpeg.len() as u8);
        body.extend_from_slice(&img_headers);
        body.extend_from_slice(&jpeg);

        let mut data = Vec::new();
        data.extend_from_slice(
            b"<smil><body><text src=\"cid:text1\"/><img src=\"cid:img1\"/></body></smil>",
        );
        data.push(0x84);
        // multipart.related short-int (well-known index 0x2c)
        data.push(0xac);
        data.extend_from_slice(&body);

        let structured = decode_mms_best_effort(&data);
        let smil = parse_smil_refs(&data);
        assert!(smil.media_srcs.iter().any(|s| s.contains("img1")));
        let body_text = body_from_structured(&structured, &smil).expect("cid body");
        assert_eq!(body_text, "Body via cid");
        let atts = attachments_from_structured(&structured, &smil);
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].ext, ".jpg");
        assert_eq!(atts[0].smil_name.as_deref(), Some("cid:img1"));
    }

    #[test]
    fn application_header_in_pdu_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("I_1609459200_app.pdu");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x8c, 0x84]); // m-retrieve-conf
        bytes.extend_from_slice(b"X-Go-Extra\0abc\0");
        bytes.extend_from_slice(&[0x84, 0x83]); // text/plain CT ends headers
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(
            parsed.pdu_fields.get("app:X-Go-Extra").map(String::as_str),
            Some("abc")
        );
    }

    #[test]
    fn bcc_and_extra_headers_in_pdu_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("I_1609459200_headers.pdu");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        // Bcc
        bytes.push(0x81);
        bytes.push(0x18);
        bytes.push(0xea);
        bytes.extend_from_slice(b"+15559876543/TYPE=PLMN");
        // Transaction-Id / Message-Class / Version
        bytes.push(0x98);
        bytes.extend_from_slice(b"txn-1\0");
        bytes.push(0x8a);
        bytes.push(0x80); // Personal
        bytes.push(0x8d);
        bytes.push(0x92); // 1.2
        bytes.push(0x8e); // pad for overshoot
        bytes.extend_from_slice(b"text.txt\0hello");
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert!(parsed.participants.iter().any(|p| p.contains("5559876543")));
        assert_eq!(
            parsed.pdu_fields.get("transaction_id").map(String::as_str),
            Some("txn-1")
        );
        assert_eq!(
            parsed.pdu_fields.get("message_class").map(String::as_str),
            Some("Personal")
        );
        assert_eq!(
            parsed.pdu_fields.get("mms_version").map(String::as_str),
            Some("1.2")
        );
        assert!(
            parsed
                .pdu_fields
                .get("bcc")
                .is_some_and(|b| b.contains("15559876543"))
        );
    }

    #[test]
    fn mms_date_overrides_filename_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        // Filename says 1609459200; Date header says 1700000000
        let path = dir.path().join("I_1609459200_dated.pdu");
        let mut bytes = vec![0x85, 0x04, 0x65, 0x53, 0xf1, 0x00]; // 1700000000
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.extend_from_slice(&[0x8e]);
        bytes.extend_from_slice(b"text.txt\0Dated body");
        // Pad so To value-length overshoot has a following header byte
        bytes.push(0x8c);
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.timestamp, 1700000000);
        assert_eq!(parsed.body, "Dated body");
    }
}
