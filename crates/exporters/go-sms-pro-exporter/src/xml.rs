//! Parse GO SMS Pro `gosms_sys*.xml` SMS backups.

use crate::emit::MAX_SKIP_DETAILS;
use crate::phone::parse_google_voice_voicemail_caller;
use anyhow::{Context, Result};
use go_sms_mms::decode_gosms_emojis;
use phone::sanitize_number;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename = "GoSms")]
struct GoSmsFile {
    #[serde(rename = "SMS", default)]
    sms: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct XmlMessage {
    /// Other-party digits (sanitized).
    pub other_digits: String,
    pub name_alias: Option<String>,
    pub timestamp_secs: f64,
    pub is_from_me: bool,
    /// Sender digits when not from me.
    pub sender_digits: Option<String>,
    pub text: String,
    /// Raw Android `<type>` (`1` received, `2` sent).
    pub android_type: String,
    /// Raw `<date>` milliseconds string.
    pub date_ms: String,
    /// Raw `<contactName>`.
    pub contact_name: String,
    /// Every `<SMS>` child element name → text.
    pub xml_fields: BTreeMap<String, String>,
}

/// Diagnostic row when an XML SMS has no usable `<address>` digits.
#[derive(Debug, Clone)]
pub(crate) struct SkippedBadAddrDetail {
    pub xml_file: String,
    pub address: String,
    pub contact_name: String,
    pub android_type: String,
    pub date_ms: String,
    pub body: String,
}

#[derive(Debug, Default)]
pub(crate) struct XmlParseStats {
    pub messages: u64,
    pub sent: u64,
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_unknown_type: u64,
    pub skipped_unknown_address: u64,
    /// Capped at [`MAX_SKIP_DETAILS`] entries; overflow counted here.
    pub skipped_unknown_address_details: Vec<SkippedBadAddrDetail>,
    pub skipped_unknown_address_details_more: u64,
}

/// Parse one GO SMS Pro XML backup file.
///
/// # Errors
///
/// Returns an error when the file cannot be read or the XML cannot be parsed.
pub(crate) fn parse_xml_file(path: &Path) -> Result<(Vec<XmlMessage>, XmlParseStats)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let (msgs, mut stats) = parse_xml_str(&text)?;
    let xml_file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    for d in &mut stats.skipped_unknown_address_details {
        d.xml_file = xml_file.clone();
    }
    Ok((msgs, stats))
}

/// Parse GO SMS Pro XML from a string.
///
/// # Errors
///
/// Returns an error when the XML cannot be parsed.
pub(crate) fn parse_xml_str(text: &str) -> Result<(Vec<XmlMessage>, XmlParseStats)> {
    let file: GoSmsFile = quick_xml::de::from_str(text).context("failed to parse GoSms XML")?;
    let mut stats = XmlParseStats::default();
    let mut out = Vec::new();

    for fields in file.sms {
        stats.messages += 1;
        let addr = sanitize_number(fields.get("address").map(String::as_str).unwrap_or(""));
        let contact = fields.get("contactName").cloned().unwrap_or_default();
        let body_raw = fields.get("body").map(String::as_str).unwrap_or("");
        let body = decode_gosms_emojis(body_raw);
        // A missing `<date>` must not become a fake 1970-01-01 row: all such
        // messages would share timestamp 0 and could falsely deduplicate.
        let Some(date_ms) = fields.get("date").cloned() else {
            stats.skipped_invalid_date += 1;
            continue;
        };
        let timestamp_secs = match date_ms.parse::<f64>() {
            Ok(ms) => ms / 1000.0,
            Err(_) => {
                stats.skipped_invalid_date += 1;
                continue;
            }
        };
        let typ = fields
            .get("type")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let msg = match typ.as_str() {
            "2" => {
                let Some(other) = addr else {
                    push_bad_addr(&mut stats, &fields, &contact, &typ, &date_ms, &body);
                    continue;
                };
                stats.sent += 1;
                XmlMessage {
                    other_digits: other,
                    name_alias: message_ir::nonempty(&contact),
                    timestamp_secs,
                    is_from_me: true,
                    sender_digits: None,
                    text: body,
                    android_type: typ.clone(),
                    date_ms: date_ms.clone(),
                    contact_name: contact.clone(),
                    xml_fields: fields,
                }
            }
            "1" => {
                if let Some(caller) = parse_google_voice_voicemail_caller(&body) {
                    stats.received += 1;
                    XmlMessage {
                        other_digits: caller.clone(),
                        name_alias: Some(caller.clone()),
                        timestamp_secs,
                        is_from_me: false,
                        sender_digits: Some(caller),
                        text: body,
                        android_type: typ.clone(),
                        date_ms: date_ms.clone(),
                        contact_name: contact.clone(),
                        xml_fields: fields,
                    }
                } else {
                    let Some(other) = addr else {
                        push_bad_addr(&mut stats, &fields, &contact, &typ, &date_ms, &body);
                        continue;
                    };
                    stats.received += 1;
                    let hint = if contact.is_empty() {
                        None
                    } else {
                        Some(contact.clone())
                    };
                    XmlMessage {
                        other_digits: other.clone(),
                        name_alias: hint,
                        timestamp_secs,
                        is_from_me: false,
                        sender_digits: Some(other),
                        text: body,
                        android_type: typ.clone(),
                        date_ms: date_ms.clone(),
                        contact_name: contact,
                        xml_fields: fields,
                    }
                }
            }
            _ => {
                stats.skipped_unknown_type += 1;
                continue;
            }
        };

        out.push(msg);
    }

    Ok((out, stats))
}

/// Record an XML SMS whose `<address>` had no usable digits.
fn push_bad_addr(
    stats: &mut XmlParseStats,
    fields: &BTreeMap<String, String>,
    contact: &str,
    typ: &str,
    date_ms: &str,
    body: &str,
) {
    stats.skipped_unknown_address += 1;
    let body_preview: String = body.chars().take(160).collect();
    if stats.skipped_unknown_address_details.len() < MAX_SKIP_DETAILS {
        stats
            .skipped_unknown_address_details
            .push(SkippedBadAddrDetail {
                xml_file: String::new(),
                address: fields.get("address").cloned().unwrap_or_default(),
                contact_name: contact.to_string(),
                android_type: typ.to_string(),
                date_ms: date_ms.to_string(),
                body: body_preview,
            });
    } else {
        stats.skipped_unknown_address_details_more += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sent_and_received() {
        let xml = r#"<?xml version="1.0"?>
<GoSms>
  <SMSCount>2</SMSCount>
  <SMS>
    <address>+14075551234</address>
    <contactName>Alice</contactName>
    <date>1400773261000</date>
    <type>1</type>
    <body>hello +g1f602</body>
  </SMS>
  <SMS>
    <address>+14075551234</address>
    <contactName>Alice</contactName>
    <date>1400773321000</date>
    <type>2</type>
    <body>hi back</body>
  </SMS>
</GoSms>"#;
        let (msgs, stats) = parse_xml_str(xml).unwrap();
        assert_eq!(stats.messages, 2);
        assert_eq!(stats.received, 1);
        assert_eq!(stats.sent, 1);
        assert_eq!(msgs.len(), 2);
        assert!(!msgs[0].is_from_me);
        assert_eq!(msgs[0].text, "hello 😂");
        assert_eq!(msgs[0].other_digits, "4075551234");
        assert!(msgs[1].is_from_me);
    }

    #[test]
    fn missing_date_skips_message() {
        let xml = r#"<?xml version="1.0"?>
<GoSms>
  <SMS>
    <address>+14075551234</address>
    <contactName>Alice</contactName>
    <type>1</type>
    <body>no date here</body>
  </SMS>
  <SMS>
    <address>+14075551234</address>
    <contactName>Alice</contactName>
    <date>1400773261000</date>
    <type>1</type>
    <body>dated</body>
  </SMS>
</GoSms>"#;
        let (msgs, stats) = parse_xml_str(xml).unwrap();
        assert_eq!(stats.messages, 2);
        assert_eq!(stats.skipped_invalid_date, 1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text, "dated");
        assert!(msgs[0].date_ms.starts_with("1400773261"));
    }

    #[test]
    fn preserves_extra_xml_fields() {
        let xml = r#"<?xml version="1.0"?>
<GoSms>
  <SMS>
    <address>+14075551234</address>
    <contactName>Alice</contactName>
    <date>1400773261000</date>
    <type>1</type>
    <body>hello</body>
    <read>1</read>
    <status>-1</status>
  </SMS>
</GoSms>"#;
        let (msgs, _) = parse_xml_str(xml).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].xml_fields.get("read").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            msgs[0].xml_fields.get("status").map(String::as_str),
            Some("-1")
        );
        assert_eq!(msgs[0].android_type, "1");
        assert_eq!(msgs[0].date_ms, "1400773261000");
        assert_eq!(msgs[0].contact_name, "Alice");
    }
}
