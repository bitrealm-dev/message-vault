//! Parse flat EMLs: one text message per `.eml` file (not a multi-message archive).

use crate::assets::extract_attachments;
use crate::types::ParsedMessage;
use anyhow::Result;
use mailparse::{MailHeaderMap, ParsedMail};
use phone::sanitize_number;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

static SUBJECT_RE: OnceLock<Regex> = OnceLock::new();
static ADDRESS_SPLIT_RE: OnceLock<Regex> = OnceLock::new();
static ARCHIVE_SUBJECT_PREFIX_RE: OnceLock<Regex> = OnceLock::new();

/// Android SMS/MMS type codes SMS Backup+ puts in `X-smssync-type` for sent messages
/// (Telephony `MESSAGE_TYPE_SENT`/`OUTBOX`/… and common MMS PDU sent codes).
const SENT_TYPES: &[&str] = &["2", "128", "4", "135", "6", "5"];
/// Android SMS/MMS type codes for inbox / received messages.
const RECEIVED_TYPES: &[&str] = &["1", "132", "130"];

fn subject_re() -> &'static Regex {
    SUBJECT_RE.get_or_init(|| Regex::new(r"(?i)^SMS with (.+)$").expect("subject"))
}

fn address_split_re() -> &'static Regex {
    ADDRESS_SPLIT_RE.get_or_init(|| Regex::new(r"[~;,|]+").expect("split"))
}

fn archive_subject_prefix_re() -> &'static Regex {
    ARCHIVE_SUBJECT_PREFIX_RE
        .get_or_init(|| Regex::new(r"(?i)^SMS archive ").expect("archive subject"))
}

/// Cached headers read once per EML (avoids repeated `get_first_value` + alloc).
#[derive(Debug, Clone)]
pub(crate) struct MailHeaders {
    pub smssync_type: String,
    pub smssync_address: String,
    pub smssync_date: String,
    pub smssync_id: String,
    pub subject: String,
    pub from: String,
    pub to: String,
    pub date: String,
}

impl MailHeaders {
    pub(crate) fn from_mail(mail: &ParsedMail<'_>) -> Self {
        fn one(mail: &ParsedMail<'_>, name: &str) -> String {
            mail.headers
                .get_first_value(name)
                .unwrap_or_default()
                .trim()
                .to_string()
        }
        Self {
            smssync_type: one(mail, "X-smssync-type"),
            smssync_address: one(mail, "X-smssync-address"),
            smssync_date: one(mail, "X-smssync-date"),
            smssync_id: one(mail, "X-smssync-id"),
            subject: one(mail, "Subject"),
            from: one(mail, "From"),
            to: one(mail, "To"),
            date: one(mail, "Date"),
        }
    }
}

fn smssync_participant_numbers(raw_address: &str) -> Vec<String> {
    if raw_address.trim().is_empty() {
        return Vec::new();
    }
    let mut numbers = Vec::new();
    let mut seen = HashSet::new();
    for part in address_split_re().split(raw_address) {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let Some(num) = sanitize_number(token) else {
            continue;
        };
        if !seen.insert(num.clone()) {
            continue;
        }
        numbers.push(num);
    }
    numbers
}

fn contact_name_from_subject(subject: &str) -> Option<String> {
    let caps = subject_re().captures(subject.trim())?;
    let name = caps[1].trim();
    if name.starts_with('+') || name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(name.to_string())
}

fn timestamp_seconds(headers: &MailHeaders) -> Option<f64> {
    let raw = &headers.smssync_date;
    if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
        let value: i64 = raw.parse().ok()?;
        // Android uses epoch ms (~1e12 today). Seconds stay ~1e9 until year 5138.
        // Threshold 1e11 catches pre-2001 ms timestamps that the old 1e12 cutoff missed.
        return Some(if value >= 100_000_000_000 {
            value as f64 / 1000.0
        } else {
            value as f64
        });
    }
    if headers.date.is_empty() {
        return None;
    }
    // mailparse does not parse Date headers; try chrono RFC2822.
    chrono::DateTime::parse_from_rfc2822(&headers.date)
        .ok()
        .map(|d| d.timestamp() as f64)
}

/// `owner_emails` must already be trimmed + lowercased.
fn is_sent(headers: &MailHeaders, owner_emails: &[String]) -> bool {
    let typ = headers.smssync_type.as_str();
    if SENT_TYPES.contains(&typ) {
        return true;
    }
    if RECEIVED_TYPES.contains(&typ) {
        return false;
    }
    let from = headers.from.to_ascii_lowercase();
    // Compare against the bare addr-spec, not a substring: owner
    // `ce@example.com` would otherwise match `alice@example.com`.
    let from_addr = if let Some(start) = from.find('<') {
        if let Some(end) = from[start..].find('>') {
            &from[start + 1..start + end]
        } else {
            &from[start + 1..]
        }
    } else {
        &from
    };
    let from_addr = from_addr.trim();
    owner_emails
        .iter()
        .any(|e| !e.is_empty() && from_addr == e.as_str())
}

/// First `text/plain` body in the MIME tree, with newlines normalized to `\n`.
pub(crate) fn extract_plain_text_body(mail: &ParsedMail<'_>) -> String {
    fn walk(m: &ParsedMail<'_>) -> Option<String> {
        let ctype = m.ctype.mimetype.to_ascii_lowercase();
        if ctype == "text/plain"
            && let Ok(body) = m.get_body()
        {
            return Some(body.replace("\r\n", "\n").replace('\r', "\n"));
        }
        for part in &m.subparts {
            if let Some(b) = walk(part) {
                return Some(b);
            }
        }
        None
    }
    walk(mail).unwrap_or_default()
}

fn is_single_sms_eml(headers: &MailHeaders) -> bool {
    if !headers.smssync_type.is_empty() {
        return true;
    }
    let headers_blob = format!("{} {}", headers.from, headers.to);
    subject_re().is_match(&headers.subject) && headers_blob.contains("@sms-backup-plus.local")
}

/// True when this looks like a flat single-message SMS Backup+ EML.
pub(crate) fn is_flat_sms_eml(headers: &MailHeaders) -> bool {
    is_single_sms_eml(headers)
}

/// Format digit strings as E.164 when unambiguous, then join with `", "`.
fn join_usa_phones(digits: &[String]) -> String {
    digits
        .iter()
        .map(|d| phone::normalize_guarded(d, phone::PhoneRegion::Usa).normalized)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Length-prefix each number so `["12","34"]` and `["123","4"]` cannot both
/// become `12_34`.
fn group_id_slug(digits: &[String]) -> String {
    digits
        .iter()
        .map(|d| format!("{}:{}", d.len(), d))
        .collect::<Vec<_>>()
        .join("_")
}

/// Group chat id (`group-…`) and display title from non-owner participant digits.
fn group_chat_id(others: &[String]) -> (String, String) {
    let mut sorted = others.to_vec();
    sorted.sort();
    sorted.dedup();
    let title = if sorted.is_empty() {
        "Group".to_string()
    } else if sorted.len() <= 4 {
        format!("Group: {}", join_usa_phones(&sorted))
    } else {
        format!(
            "Group: {}, and {} others",
            join_usa_phones(&sorted[..4]),
            sorted.len() - 4
        )
    };
    // Length-prefix each number so `["12","34"]` and `["123","4"]` cannot
    // both become `group-12_34`.
    let key = format!("group-{}", group_id_slug(&sorted));
    let key = if key.len() > 180 {
        let digest = hex::encode(Sha256::digest(key.as_bytes()));
        format!("group-{}", &digest[..16])
    } else {
        key
    };
    (key, title)
}

/// Parse an already-loaded flat SMS Backup+ EML (avoids a second disk read).
///
/// `owner_emails` must already be trimmed + lowercased.
pub(crate) fn parse_flat_eml_mail(
    path: &Path,
    mail: &ParsedMail<'_>,
    headers: &MailHeaders,
    owner_digits: &HashSet<String>,
    owner_emails: &[String],
) -> Result<Option<ParsedMessage>> {
    if !is_single_sms_eml(headers) {
        return Ok(None);
    }

    let Some(ts) = timestamp_seconds(headers) else {
        return Ok(None);
    };

    let subject_name = contact_name_from_subject(&headers.subject);
    let mut addr_raw = headers.smssync_address.clone();
    if addr_raw.is_empty()
        && let Some(ref name) = subject_name
    {
        addr_raw = name.clone();
    }
    let participant_numbers = smssync_participant_numbers(&addr_raw);
    let addr = participant_numbers
        .first()
        .cloned()
        .or_else(|| sanitize_number(&addr_raw))
        .unwrap_or_default();
    if addr.is_empty() && addr_raw.is_empty() {
        return Ok(None);
    }

    let name_alias = subject_name.clone();
    let sent = is_sent(headers, owner_emails);
    let body = extract_plain_text_body(mail);

    let file_key = hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()));
    let file_key = &file_key[..12.min(file_key.len())];
    let attachments = extract_attachments(mail, ts * 1000.0, Some(file_key));

    let non_owner: Vec<String> = participant_numbers
        .iter()
        .filter(|n| !owner_digits.contains(*n))
        .cloned()
        .collect();

    if non_owner.len() >= 2 {
        let (is_from_me, sender_digits) = if sent {
            (true, None)
        } else {
            // Prefer From header digits among participants
            let from_nums = smssync_participant_numbers(&headers.from);
            let sender = from_nums
                .into_iter()
                .find(|n| !owner_digits.contains(n) && non_owner.contains(n))
                .or_else(|| non_owner.first().cloned());
            (false, sender)
        };
        let (chat_key, title) = group_chat_id(&non_owner);
        let participant_digits: Vec<_> = non_owner.into_iter().map(|d| (d, None)).collect();
        let smssync_id = if headers.smssync_id.is_empty() {
            None
        } else {
            Some(headers.smssync_id.clone())
        };
        return Ok(Some(ParsedMessage {
            chat_key,
            conversation_type: "group".into(),
            group_title: Some(title),
            participant_digits,
            timestamp_secs: ts,
            is_from_me,
            sender_digits,
            text: body,
            attachments,
            name_alias,
            smssync_id,
            source_kind: "flat".into(),
            android_type: headers.smssync_type.clone(),
            eml_path: String::new(),
        }));
    }

    // Prefer the first non-owner address (groups already use this rule). An
    // owner-first `owner~peer` list must not key the CSV to the owner's number.
    let conv_number = if let Some(peer) = non_owner.first() {
        peer.clone()
    } else if !addr.is_empty() {
        addr.clone()
    } else {
        sanitize_number(&addr_raw).unwrap_or_default()
    };
    // Keep an empty chat_key when a display name exists so contacts reverse-lookup can fill it.
    if conv_number.is_empty()
        && name_alias
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Ok(None);
    }

    let smssync_id = if headers.smssync_id.is_empty() {
        None
    } else {
        Some(headers.smssync_id.clone())
    };
    Ok(Some(ParsedMessage {
        chat_key: conv_number.clone(),
        conversation_type: "individual".into(),
        group_title: None,
        participant_digits: if conv_number.is_empty() {
            vec![]
        } else {
            vec![(conv_number.clone(), name_alias.clone())]
        },
        timestamp_secs: ts,
        is_from_me: sent,
        sender_digits: if sent || conv_number.is_empty() {
            None
        } else {
            Some(conv_number)
        },
        text: body,
        attachments,
        name_alias,
        smssync_id,
        source_kind: "flat".into(),
        android_type: headers.smssync_type.clone(),
        eml_path: String::new(),
    }))
}

/// Classify whether this EML looks like a consolidated archive thread.
pub(crate) fn is_archive_eml(headers: &MailHeaders) -> bool {
    archive_subject_prefix_re().is_match(headers.subject.trim()) && headers.smssync_type.is_empty()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_received() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("msg.eml");
        std::fs::write(
            &path,
            b"From: alice@unknown.email\r\n\
To: me@example.com\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 1\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: 1609459200000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hello from Alice\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let owners = HashSet::from(["5555550100".to_string()]);
        let msg = parse_flat_eml_mail(&path, &mail, &headers, &owners, &[])
            .unwrap()
            .unwrap();
        assert!(!msg.is_from_me);
        assert_eq!(msg.text.trim(), "Hello from Alice");
        assert_eq!(msg.chat_key, "4075551234");
        assert!((msg.timestamp_secs - 1_609_459_200.0).abs() < 0.001);
    }

    #[test]
    fn individual_chat_uses_first_non_owner_address() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("msg.eml");
        std::fs::write(
            &path,
            b"From: me@example.com\r\n\
To: alice@unknown.email\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 2\r\n\
X-smssync-address: 5555550100~4075551234\r\n\
X-smssync-date: 1609459200000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hello\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let owners = HashSet::from(["5555550100".to_string()]);
        let msg = parse_flat_eml_mail(&path, &mail, &headers, &owners, &["me@example.com".into()])
            .unwrap()
            .unwrap();
        assert_eq!(msg.chat_key, "4075551234");
        assert!(msg.is_from_me);
    }

    #[test]
    fn early_2000s_ms_dates_are_not_treated_as_seconds() {
        // 2001-01-01T00:00:00Z as epoch ms is < 1e12; the old >1e12 cutoff
        // would have treated this as seconds (~year 32995).
        let ms = 978_307_200_000_i64;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("msg.eml");
        std::fs::write(
            &path,
            format!(
                "From: alice@unknown.email\r\n\
To: me@example.com\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 1\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: {ms}\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
old message\r\n"
            ),
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let owners = HashSet::from(["5555550100".to_string()]);
        let msg = parse_flat_eml_mail(&path, &mail, &headers, &owners, &[])
            .unwrap()
            .unwrap();
        assert!((msg.timestamp_secs - 978_307_200.0).abs() < 0.001);
    }

    #[test]
    fn sent_detection_uses_exact_owner_email() {
        fn headers_with_from(from: &str) -> MailHeaders {
            MailHeaders {
                smssync_type: String::new(),
                smssync_address: String::new(),
                smssync_date: String::new(),
                smssync_id: String::new(),
                subject: String::new(),
                from: from.into(),
                to: String::new(),
                date: String::new(),
            }
        }
        // `ce@example.com` is a substring of `alice@example.com`; substring
        // matching would misclassify this received message as sent.
        assert!(!is_sent(
            &headers_with_from("alice@example.com"),
            &["ce@example.com".into()]
        ));
        // Exact addr-spec matches still detect sent mail, including the
        // `Name <addr>` form.
        assert!(is_sent(
            &headers_with_from("alice@example.com"),
            &["alice@example.com".into()]
        ));
        assert!(is_sent(
            &headers_with_from("Alice <alice@example.com>"),
            &["alice@example.com".into()]
        ));
    }

    #[test]
    fn group_chat_id_does_not_collide_on_digit_split() {
        let (k1, _) = group_chat_id(&["12".to_string(), "34".to_string()]);
        let (k2, _) = group_chat_id(&["123".to_string(), "4".to_string()]);
        assert_ne!(k1, k2);
        assert_eq!(k1, "group-2:12_2:34");
        assert_eq!(k2, "group-3:123_1:4");
    }
}
