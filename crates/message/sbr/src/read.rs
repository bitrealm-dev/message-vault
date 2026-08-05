//! Streaming reader and domain parsing for SMS Backup & Restore XML.

use anyhow::{Context, Result, bail};
use base64::Engine;
use phone::{sanitize_number, to_e164};
use quick_xml::{Reader, XmlVersion, events::Event};
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufRead;
use std::path::Path;
use std::sync::{Arc, OnceLock};

const INSERT_ADDRESS_TOKEN: &str = "insert-address-token";
const MMS_ADDR_FROM: &str = "137";
const MMS_BOX_SENT: &str = "2";
const MMS_BOX_DRAFT: &str = "3";
const MMS_BOX_OUTBOX: &str = "4";
const MMS_BOX_FAILED: &str = "5";
const MMS_BOX_QUEUED: &str = "6";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversationKind {
    #[default]
    Individual,
    Group,
}

#[derive(Debug, Clone, Default)]
pub struct MmsPart {
    pub ct: String,
    pub name: String,
    pub cl: String,
    pub fn_attr: String,
    pub text: String,
    pub data: String,
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
struct MmsAddr {
    address: String,
    addr_type: String,
    attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AttachmentBlob {
    pub filename: String,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub data: Arc<[u8]>,
    pub digest_hex: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum SourceFields {
    #[serde(rename = "sms")]
    Sms { attrs: BTreeMap<String, String> },
    #[serde(rename = "mms")]
    Mms {
        attrs: BTreeMap<String, String>,
        parts: Vec<BTreeMap<String, String>>,
        addrs: Vec<BTreeMap<String, String>>,
    },
}

#[derive(Debug, Clone)]
pub struct Record {
    pub chat_key: String,
    pub conversation_kind: ConversationKind,
    pub group_title: Option<String>,
    pub participant_digits: Vec<(String, Option<String>)>,
    pub timestamp_secs: f64,
    pub is_from_me: bool,
    pub sender_digits: Option<String>,
    pub sender_display_name: Option<String>,
    pub text: String,
    pub subject: String,
    pub attachments: Vec<AttachmentBlob>,
    pub message_kind: &'static str,
    pub date_ms: String,
    pub contact_name: String,
    pub android_type: String,
    pub source_fields: SourceFields,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ParseStats {
    pub sms_seen: u64,
    pub mms_seen: u64,
    pub skipped_invalid_date: u64,
    pub skipped_unknown_address: u64,
    pub skipped_unknown_type: u64,
    pub skipped_draft_or_outbox: u64,
    pub skipped_empty_participants: u64,
    pub skipped_bad_attachment: u64,
}

fn attrs(e: &quick_xml::events::BytesStart<'_>) -> HashMap<String, String> {
    e.attributes()
        .flatten()
        .map(|a| {
            let key = String::from_utf8_lossy(a.key.as_ref()).to_ascii_lowercase();
            let value = a
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|v| v.into_owned())
                .unwrap_or_default();
            (key, value)
        })
        .collect()
}

fn get<'a>(attrs: &'a HashMap<String, String>, key: &str) -> &'a str {
    attrs.get(key).map(String::as_str).unwrap_or("")
}

fn btree(attrs: &HashMap<String, String>) -> BTreeMap<String, String> {
    attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn part(attrs: &HashMap<String, String>) -> MmsPart {
    MmsPart {
        ct: get(attrs, "ct").into(),
        name: get(attrs, "name").into(),
        cl: get(attrs, "cl").into(),
        fn_attr: get(attrs, "fn").into(),
        text: get(attrs, "text").into(),
        data: get(attrs, "data").into(),
        attrs: btree(attrs),
    }
}

fn addr(attrs: &HashMap<String, String>) -> MmsAddr {
    MmsAddr {
        address: get(attrs, "address").into(),
        addr_type: get(attrs, "type").into(),
        attrs: btree(attrs),
    }
}

fn decode_body(raw: &str) -> String {
    html_escape::decode_html_entities(raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn name_hint(attrs: &HashMap<String, String>) -> Option<String> {
    let value = if get(attrs, "contact_name").is_empty() {
        get(attrs, "name")
    } else {
        get(attrs, "contact_name")
    };
    let value = value.trim();
    (!value.is_empty() && !value.eq_ignore_ascii_case("null")).then(|| value.to_string())
}

fn raw_name(attrs: &HashMap<String, String>) -> String {
    let value = get(attrs, "contact_name");
    if value.is_empty() {
        get(attrs, "name").into()
    } else {
        value.into()
    }
}

fn non_null(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("null") {
        String::new()
    } else {
        value.into()
    }
}

fn content_keys(part: &MmsPart) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for raw in [&part.name, &part.cl, &part.fn_attr] {
        let value = raw.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("null")
            || value.eq_ignore_ascii_case("none")
        {
            continue;
        }
        keys.insert(value.into());
        if let Some(base) = value.rsplit('/').next().filter(|s| !s.is_empty()) {
            keys.insert(base.into());
        }
    }
    keys
}

static TEXT_SRC: OnceLock<Regex> = OnceLock::new();
static IMG_SRC: OnceLock<Regex> = OnceLock::new();

fn smil_refs(parts: &[MmsPart]) -> (Vec<String>, Vec<String>) {
    let smil = parts
        .iter()
        .find(|p| p.ct.eq_ignore_ascii_case("application/smil"))
        .map(|p| {
            if !p.text.trim().is_empty() {
                html_escape::decode_html_entities(p.text.trim()).into_owned()
            } else {
                base64::engine::general_purpose::STANDARD
                    .decode(p.data.trim())
                    .map(|v| String::from_utf8_lossy(&v).into_owned())
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default();
    let text = TEXT_SRC
        .get_or_init(|| Regex::new(r#"(?i)<text[^>]+src=["']([^"']+)["']"#).expect("valid regex"));
    let image = IMG_SRC
        .get_or_init(|| Regex::new(r#"(?i)<img[^>]+src=["']([^"']+)["']"#).expect("valid regex"));
    let captures = |re: &Regex| {
        re.captures_iter(&smil)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect()
    };
    (captures(text), captures(image))
}

fn valid_filename(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && !value.eq_ignore_ascii_case("null")
        && !value.eq_ignore_ascii_case("none"))
    .then(|| value.into())
}

fn extension(part: &MmsPart) -> String {
    match part.ct.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => ".jpg".into(),
        "image/png" => ".png".into(),
        "image/gif" => ".gif".into(),
        "image/webp" => ".webp".into(),
        "video/mp4" => ".mp4".into(),
        "video/3gpp" | "video/3gp" => ".3gp".into(),
        "audio/amr" => ".amr".into(),
        "audio/mpeg" => ".mp3".into(),
        "audio/mp4" => ".m4a".into(),
        ct => [&part.name, &part.cl, &part.fn_attr]
            .iter()
            .find_map(|n| {
                valid_filename(n).and_then(|n| {
                    Path::new(&n)
                        .extension()?
                        .to_str()
                        .map(|e| format!(".{}", e.to_ascii_lowercase()))
                })
            })
            .unwrap_or_else(|| {
                if ct.starts_with("image/") {
                    ".jpg".into()
                } else if ct.starts_with("video/") {
                    ".mp4".into()
                } else if ct.starts_with("audio/") {
                    ".amr".into()
                } else {
                    ".bin".into()
                }
            }),
    }
}

fn attachments(parts: &[MmsPart], refs: &[String], stats: &mut ParseStats) -> Vec<AttachmentBlob> {
    let mut by_key = HashMap::new();
    let mut order = Vec::new();
    for part in parts {
        let ct = part.ct.to_ascii_lowercase();
        if ct.starts_with("text/") || ct == "application/smil" {
            continue;
        }
        let Ok(payload) = base64::engine::general_purpose::STANDARD.decode(part.data.trim()) else {
            if !part.data.trim().is_empty() && !part.data.trim().eq_ignore_ascii_case("null") {
                stats.skipped_bad_attachment += 1;
            }
            continue;
        };
        if payload.is_empty() {
            continue;
        }
        let digest = hex::encode(Sha256::digest(&payload));
        let original = valid_filename(&part.name)
            .or_else(|| valid_filename(&part.cl))
            .or_else(|| valid_filename(&part.fn_attr));
        let ext = extension(part);
        let filename = format!("{digest}{ext}");
        let blob = AttachmentBlob {
            filename: filename.clone(),
            original_name: original,
            mime_type: Some(if part.ct.trim().is_empty() {
                "application/octet-stream".into()
            } else {
                part.ct.clone()
            }),
            data: Arc::from(payload),
            digest_hex: digest,
        };
        let keys = content_keys(part);
        if keys.is_empty() {
            order.push(filename.clone());
            by_key.insert(filename, blob);
        } else {
            for (index, key) in keys.into_iter().enumerate() {
                if index == 0 {
                    order.push(key.clone());
                }
                by_key.entry(key).or_insert_with(|| blob.clone());
            }
        }
    }
    let mut seen = HashSet::new();
    refs.iter()
        .chain(order.iter())
        .filter_map(|k| by_key.get(k))
        .chain(by_key.values())
        .filter(|b| seen.insert(b.filename.clone()))
        .cloned()
        .collect()
}

fn part_fields(part: &MmsPart) -> BTreeMap<String, String> {
    let mut attrs = part.attrs.clone();
    if let Some(data) = attrs
        .remove("data")
        .filter(|d| !d.trim().is_empty() && !d.eq_ignore_ascii_case("null"))
    {
        match base64::engine::general_purpose::STANDARD.decode(data.trim()) {
            Ok(bytes) => {
                attrs.insert("data_len".into(), bytes.len().to_string());
                attrs.insert("data_sha256".into(), hex::encode(Sha256::digest(bytes)));
            }
            Err(_) => {
                attrs.insert("data_len".into(), data.len().to_string());
                attrs.insert(
                    "data_sha256".into(),
                    hex::encode(Sha256::digest(data.as_bytes())),
                );
                attrs.insert("data_decode_error".into(), "true".into());
            }
        }
    }
    attrs
}

fn parse_sms(attrs: &HashMap<String, String>, stats: &mut ParseStats) -> Option<Record> {
    stats.sms_seen += 1;
    let date_ms = get(attrs, "date").to_string();
    let timestamp_secs = date_ms
        .parse::<f64>()
        .ok()
        .map(|v| v / 1000.0)
        .or_else(|| {
            stats.skipped_invalid_date += 1;
            None
        })?;
    let address = sanitize_number(get(attrs, "address")).or_else(|| {
        stats.skipped_unknown_address += 1;
        None
    })?;
    let android_type = get(attrs, "type").trim().to_string();
    let (is_from_me, sender_digits) = match android_type.as_str() {
        "1" => (false, Some(address.clone())),
        "2" => (true, None),
        _ => {
            stats.skipped_unknown_type += 1;
            return None;
        }
    };
    let hint = name_hint(attrs);
    Some(Record {
        chat_key: address.clone(),
        conversation_kind: ConversationKind::Individual,
        group_title: None,
        participant_digits: vec![(address, hint.clone())],
        timestamp_secs,
        is_from_me,
        sender_digits,
        sender_display_name: if is_from_me { None } else { hint },
        text: decode_body(get(attrs, "body")),
        subject: non_null(get(attrs, "subject")),
        attachments: Vec::new(),
        message_kind: "sms",
        date_ms,
        contact_name: raw_name(attrs),
        android_type,
        source_fields: SourceFields::Sms {
            attrs: btree(attrs),
        },
    })
}

fn parse_mms(
    attrs: &HashMap<String, String>,
    parts: &[MmsPart],
    addrs: &[MmsAddr],
    owners: &HashSet<String>,
    stats: &mut ParseStats,
) -> Option<Record> {
    stats.mms_seen += 1;
    let date_ms = get(attrs, "date").to_string();
    let timestamp_secs = date_ms
        .parse::<f64>()
        .ok()
        .map(|v| v / 1000.0)
        .or_else(|| {
            stats.skipped_invalid_date += 1;
            None
        })?;
    let msg_box = get(attrs, "msg_box").trim().to_string();
    if matches!(
        msg_box.as_str(),
        MMS_BOX_DRAFT | MMS_BOX_OUTBOX | MMS_BOX_FAILED | MMS_BOX_QUEUED
    ) {
        stats.skipped_draft_or_outbox += 1;
        return None;
    }
    let mut participants: Vec<String> = get(attrs, "address")
        .split('~')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().into())
        .collect();
    participants.extend(
        addrs
            .iter()
            .filter(|a| !a.address.trim().is_empty())
            .map(|a| a.address.trim().into()),
    );
    if participants.is_empty() {
        stats.skipped_empty_participants += 1;
        return None;
    }
    let is_from_me = msg_box == MMS_BOX_SENT;
    let sender_digits = if is_from_me {
        None
    } else {
        addrs
            .iter()
            .find(|a| a.addr_type == MMS_ADDR_FROM)
            .and_then(|a| sanitize_number(&a.address))
            .filter(|d| !owners.contains(d))
            .or_else(|| {
                participants
                    .iter()
                    .filter_map(|p| sanitize_number(p))
                    .find(|d| !owners.contains(d))
            })
    };
    let mut peers: Vec<String> = participants
        .iter()
        .filter_map(|p| sanitize_number(p))
        .filter(|p| !owners.contains(p))
        .collect();
    peers.sort();
    peers.dedup();
    if peers.is_empty() {
        stats.skipped_unknown_address += 1;
        return None;
    }
    let (text_refs, image_refs) = smil_refs(parts);
    let mut text_by_key = HashMap::new();
    for part in parts
        .iter()
        .filter(|p| p.ct.to_ascii_lowercase().starts_with("text/"))
    {
        let text = decode_body(&part.text);
        if !text.is_empty() && !text.eq_ignore_ascii_case("null") {
            for key in content_keys(part) {
                text_by_key.entry(key).or_insert_with(|| text.clone());
            }
        }
    }
    let text = if text_refs.is_empty() {
        let mut values: Vec<String> = text_by_key.into_values().collect();
        values.sort();
        values.dedup();
        values.join("\n")
    } else {
        text_refs
            .iter()
            .filter_map(|r| text_by_key.get(r))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    };
    let blobs = attachments(parts, &image_refs, stats);
    let hint = name_hint(attrs);
    let source_fields = SourceFields::Mms {
        attrs: btree(attrs),
        parts: parts.iter().map(part_fields).collect(),
        addrs: addrs.iter().map(|a| a.attrs.clone()).collect(),
    };
    if peers.len() == 1 {
        let peer = peers.remove(0);
        return Some(Record {
            chat_key: peer.clone(),
            conversation_kind: ConversationKind::Individual,
            group_title: None,
            participant_digits: vec![(peer, hint.clone())],
            timestamp_secs,
            is_from_me,
            sender_digits,
            sender_display_name: if is_from_me { None } else { hint },
            text,
            subject: non_null(get(attrs, "sub")),
            attachments: blobs,
            message_kind: "mms",
            date_ms,
            contact_name: raw_name(attrs),
            android_type: msg_box,
            source_fields,
        });
    }
    let title = if peers.len() <= 4 {
        format!(
            "Group: {}",
            peers
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "Group: {}, and {} others",
            peers[..4]
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", "),
            peers.len() - 4
        )
    };
    let raw_key = format!("group-{}", peers.join("_"));
    let chat_key = if raw_key.len() > 180 {
        format!(
            "group-{}",
            &hex::encode(Sha256::digest(raw_key.as_bytes()))[..16]
        )
    } else {
        raw_key
    };
    Some(Record {
        chat_key,
        conversation_kind: ConversationKind::Group,
        group_title: Some(title),
        participant_digits: peers.into_iter().map(|d| (d, None)).collect(),
        timestamp_secs,
        is_from_me,
        sender_digits,
        sender_display_name: if is_from_me { None } else { hint },
        text,
        subject: non_null(get(attrs, "sub")),
        attachments: blobs,
        message_kind: "mms",
        date_ms,
        contact_name: raw_name(attrs),
        android_type: msg_box,
        source_fields,
    })
}

pub fn parse_file(path: &Path, owners: &HashSet<String>) -> Result<(Vec<Record>, ParseStats)> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    parse_reader(std::io::BufReader::new(file), owners)
}

fn parse_reader<R: BufRead>(
    reader: R,
    owners: &HashSet<String>,
) -> Result<(Vec<Record>, ParseStats)> {
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);
    let (mut stats, mut records, mut buf) = (ParseStats::default(), Vec::new(), Vec::new());
    let (mut sms, mut mms, mut parts, mut addrs) =
        (HashMap::new(), HashMap::new(), Vec::new(), Vec::new());
    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref().to_ascii_lowercase().as_slice() {
                b"sms" => sms = attrs(&e),
                b"mms" => {
                    mms = attrs(&e);
                    parts.clear();
                    addrs.clear();
                }
                b"part" => parts.push(part(&attrs(&e))),
                b"addr" => addrs.push(addr(&attrs(&e))),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref().to_ascii_lowercase().as_slice() {
                b"sms" => {
                    if let Some(r) = parse_sms(&attrs(&e), &mut stats) {
                        records.push(r)
                    }
                }
                b"part" => parts.push(part(&attrs(&e))),
                b"addr" => addrs.push(addr(&attrs(&e))),
                b"mms" => {
                    if let Some(r) = parse_mms(&attrs(&e), &[], &[], owners, &mut stats) {
                        records.push(r)
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref().to_ascii_lowercase().as_slice() {
                b"sms" => {
                    if let Some(r) = parse_sms(&sms, &mut stats) {
                        records.push(r)
                    }
                }
                b"mms" => {
                    if let Some(r) = parse_mms(&mms, &parts, &addrs, owners, &mut stats) {
                        records.push(r)
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(error).context("XML parse error"),
            _ => {}
        }
        buf.clear();
    }
    Ok((records, stats))
}

/// Infer owner phones from nested `<addr type="137">` elements in sent MMS.
pub fn infer_owner_phones(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut xml = Reader::from_reader(std::io::BufReader::new(file));
    let (mut buf, mut in_sent, mut counts) = (Vec::new(), false, HashMap::<String, u64>::new());
    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                match e.name().as_ref().to_ascii_lowercase().as_slice() {
                    b"mms" => in_sent = get(&attrs(&e), "msg_box").trim() == MMS_BOX_SENT,
                    b"addr" if in_sent => {
                        let a = attrs(&e);
                        if get(&a, "type").trim() == MMS_ADDR_FROM {
                            let raw = get(&a, "address");
                            if !raw.eq_ignore_ascii_case(INSERT_ADDRESS_TOKEN) {
                                if let Some(digits) = sanitize_number(raw).filter(|d| d != "0") {
                                    *counts.entry(to_e164(&digits)).or_default() += 1;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) if e.name().as_ref().eq_ignore_ascii_case(b"mms") => in_sent = false,
            Ok(Event::Eof) => break,
            Err(error) => bail!("parse {}: {error}", path.display()),
            _ => {}
        }
        buf.clear();
    }
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(ranked.into_iter().map(|(phone, _)| phone).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_owner_from_nested_addr() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("smses.xml");
        std::fs::write(&path, r#"<smses><mms msg_box="2"><parts/><addrs><addr address="+15555550100" type="137"/></addrs></mms></smses>"#).unwrap();
        assert_eq!(infer_owner_phones(&path).unwrap(), vec!["+15555550100"]);
    }

    #[test]
    fn parses_attachment_and_preserves_fields() {
        let xml = br#"<smses><mms date="1400773400000" msg_box="1" address="+15555550101" extra="x"><parts><part seq="0" ct="image/jpeg" name="pic.jpg" data="aGVsbG8="/></parts><addrs><addr address="+15555550101" type="137" charset="106"/></addrs></mms></smses>"#;
        let (records, stats) = parse_reader(xml.as_slice(), &HashSet::new()).unwrap();
        assert_eq!(stats.mms_seen, 1);
        assert_eq!(records[0].attachments[0].data.as_ref(), b"hello");
        let SourceFields::Mms {
            attrs,
            parts,
            addrs,
        } = &records[0].source_fields
        else {
            panic!("mms")
        };
        assert_eq!(attrs.get("extra").map(String::as_str), Some("x"));
        assert!(parts[0].contains_key("data_sha256"));
        assert_eq!(addrs[0].get("charset").map(String::as_str), Some("106"));
    }

    #[test]
    fn attachment_filename_is_content_addressed() {
        let xml = br#"<smses><mms date="1" msg_box="1" address="+15555550101"><parts><part ct="image/jpeg" name="first.jpg" data="aGVsbG8="/><part ct="image/jpeg" name="second.jpg" data="aGVsbG8="/></parts><addrs><addr address="+15555550101" type="137"/></addrs></mms></smses>"#;
        let (records, _) = parse_reader(xml.as_slice(), &HashSet::new()).unwrap();
        assert_eq!(records[0].attachments.len(), 1);
        let attachment = &records[0].attachments[0];
        assert!(attachment.filename.starts_with(&attachment.digest_hex));
        assert_eq!(attachment.digest_hex.len(), 64);
    }
}
