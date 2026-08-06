//! Parse archive EMLs: one email file that holds many texts in its body.
//!
//! Example subject: `SMS archive with Alice`.
//! Example body lines:
//! ```text
//! 2012-05-24 14:20:31 - Alice
//! Hello from Alice
//!
//! 2012-05-24 14:21:05 - Me
//! See you later
//! ```
//!
//! Each dated block becomes one [`ParsedMessage`]. Attachments are paired by
//! guesswork — see [`assign_archive_attachments`].

use crate::assets::extract_attachments;
use crate::flat_eml::{MailHeaders, extract_plain_text_body, is_archive_eml};
use crate::types::{AttachmentBlob, ParsedMessage};
use anyhow::{Context, Result};
use phone::sanitize_number;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::OnceLock;

static ARCHIVE_SUBJECT_RE: OnceLock<Regex> = OnceLock::new();
static MESSAGE_HEADER_RE: OnceLock<Regex> = OnceLock::new();
static DATE_ONLY_RE: OnceLock<Regex> = OnceLock::new();

fn archive_subject_re() -> &'static Regex {
    ARCHIVE_SUBJECT_RE.get_or_init(|| Regex::new(r"(?i)^SMS archive (.+)$").expect("arch subj"))
}

/// Clean contact name from an archive subject capture.
///
/// Subjects are often `SMS archive with Alice` or `SMS archive with Alice (2011-2013)`.
/// Strip a leading `with` and a trailing `(year-year)` so contacts lookup can match.
fn clean_archive_contact_name(raw: &str) -> String {
    let mut name = raw.trim();
    // `name[..5]` would panic when byte 5 lands mid-character in a multi-byte
    // UTF-8 name (e.g. `SMS archive 中文名`). "with " is 5 ASCII bytes, so a
    // match at a char boundary is always a real prefix.
    if name.len() >= 5
        && name.is_char_boundary(5)
        && name[..5].eq_ignore_ascii_case("with ")
    {
        name = name[5..].trim();
    }
    static YEAR_RANGE: OnceLock<Regex> = OnceLock::new();
    let re = YEAR_RANGE
        .get_or_init(|| Regex::new(r"\s*\(\d{4}\s*[-–—]\s*\d{4}\)\s*$").expect("year range"));
    re.replace(name, "").trim().to_string()
}

fn message_header_re() -> &'static Regex {
    MESSAGE_HEADER_RE
        .get_or_init(|| Regex::new(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) - (.+)$").expect("hdr"))
}

fn date_only_re() -> &'static Regex {
    DATE_ONLY_RE.get_or_init(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("date"))
}

fn phone_from_from_header(from_hdr: &str) -> String {
    // parseaddr-ish: extract email local-part or display
    if let Some(start) = from_hdr.find('<')
        && let Some(end) = from_hdr[start..].find('>')
    {
        let addr = &from_hdr[start + 1..start + end];
        let local = addr.split('@').next().unwrap_or(addr);
        if let Some(digits) = sanitize_number(local) {
            return digits;
        }
    }
    sanitize_number(from_hdr).unwrap_or_default()
}

fn parse_archive_timestamp(date_str: &str) -> Option<f64> {
    use chrono::{Local, LocalResult, TimeZone};
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%m/%d/%Y %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_str, fmt) {
            // Archive body times are local wall-clock, not UTC. DST transitions
            // make some wall-clock times ambiguous (fall-back) or nonexistent
            // (spring-forward); keep the earliest interpretation instead of
            // silently dropping the message.
            return match Local.from_local_datetime(&naive) {
                LocalResult::Single(dt) => Some(dt.timestamp() as f64),
                LocalResult::Ambiguous(earliest, _) => Some(earliest.timestamp() as f64),
                LocalResult::None => None,
            };
        }
    }
    None
}

/// Guess which MIME attachments belong to which archive body lines.
///
/// The archive file lists attachments in order, but does not say “this JPEG
/// goes with the 2:15pm message.” Perfect matching is impossible, so we guess:
///
/// 1. **Empty-body first** — if a message has no text (common for photo-only
///    MMS), give it the next unused attachment.
/// 2. **First come, first served** — walk remaining messages in order; each
///    that still has no attachment gets the next unused one.
/// 3. **Leftovers on the last message** — if attachments remain, pile them on
///    the final message rather than drop the files.
///
/// Tiny example: three messages (text, empty, text) and two images → the empty
/// one gets image 1; the first text still without an attachment gets image 2.
fn assign_archive_attachments(messages: &mut [ParsedMessage], att_queue: Vec<AttachmentBlob>) {
    if messages.is_empty() || att_queue.is_empty() {
        return;
    }
    let mut att_queue: VecDeque<AttachmentBlob> = att_queue.into();

    // Pass 1: empty-body messages first.
    for msg in messages.iter_mut() {
        if att_queue.is_empty() {
            break;
        }
        if msg.text.trim().is_empty()
            && let Some(att) = att_queue.pop_front()
        {
            msg.attachments.push(att);
        }
    }

    // Pass 2: first come, first served for messages that still lack an attachment.
    for msg in messages.iter_mut() {
        if att_queue.is_empty() {
            break;
        }
        if msg.attachments.is_empty()
            && let Some(att) = att_queue.pop_front()
        {
            msg.attachments.push(att);
        }
    }

    // Pass 3: leftovers → last message.
    if !att_queue.is_empty()
        && let Some(last) = messages.last_mut()
    {
        last.attachments.extend(att_queue);
    }
}

/// Parse a consolidated `SMS archive …` EML into multiple messages.
/// Parse an already-loaded archive EML (avoids a second disk read).
pub(crate) fn parse_archive_eml_mail(
    path: &Path,
    mail: &mailparse::ParsedMail<'_>,
    headers: &MailHeaders,
) -> Result<(Vec<ParsedMessage>, u64)> {
    if !is_archive_eml(headers) {
        return Ok((Vec::new(), 0));
    }

    let caps = archive_subject_re()
        .captures(headers.subject.trim())
        .context("archive subject")?;
    let export_name = clean_archive_contact_name(&caps[1]);
    let phone_raw = phone_from_from_header(&headers.from);
    // Empty peer phones are kept and written under the `unknown` chat stem.
    let conv_number = if !phone_raw.is_empty() {
        phone_raw.clone()
    } else if export_name.starts_with('+') || export_name.chars().all(|c| c.is_ascii_digit()) {
        sanitize_number(&export_name).unwrap_or_default()
    } else {
        String::new()
    };

    let file_key = hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()));
    let file_key = &file_key[..12.min(file_key.len())];
    let mime_atts = extract_attachments(mail, 0.0, Some(file_key));
    let att_queue: Vec<AttachmentBlob> = mime_atts;

    let body = extract_plain_text_body(mail);
    let lines: Vec<&str> = body.lines().collect();

    let mut messages = Vec::new();
    let mut skipped_invalid_date = 0u64;
    let mut current_date: Option<String> = None;
    let mut current_sender: Option<String> = None;
    let mut body_lines: Vec<String> = Vec::new();
    let mut skipped_header = false;
    let contact_name = export_name.clone();

    let flush = |current_date: &mut Option<String>,
                 current_sender: &mut Option<String>,
                 body_lines: &mut Vec<String>,
                 skipped_invalid_date: &mut u64,
                 messages: &mut Vec<ParsedMessage>| {
        let Some(date) = current_date.take() else {
            body_lines.clear();
            *current_sender = None;
            return;
        };
        let Some(sender) = current_sender.take() else {
            body_lines.clear();
            return;
        };
        let text = body_lines
            .iter()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        body_lines.clear();
        if text.is_empty() {
            // May still receive an attachment in assign_archive_attachments.
            // Keep empty-text placeholders so MMS-only lines can get media.
        }
        let Some(ts) = parse_archive_timestamp(&date) else {
            *skipped_invalid_date += 1;
            return;
        };
        let sender_key = sender.trim().to_ascii_lowercase();
        let (is_from_me, sender_digits) = if sender_key == "me" {
            (true, None)
        } else if conv_number.is_empty() {
            (false, None)
        } else {
            (false, Some(conv_number.clone()))
        };
        let name = Some(contact_name.clone());
        messages.push(ParsedMessage {
            chat_key: conv_number.clone(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: if conv_number.is_empty() {
                vec![]
            } else {
                vec![(conv_number.clone(), name.clone())]
            },
            timestamp_secs: ts,
            is_from_me,
            sender_digits,
            text,
            attachments: Vec::new(),
            name_hint: name,
            smssync_id: None,
            source_kind: "archive".into(),
            android_type: String::new(),
            eml_path: String::new(),
        });
    };

    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            if current_date.is_some() {
                body_lines.push(String::new());
            }
            continue;
        }

        if !skipped_header {
            if stripped.eq_ignore_ascii_case(&export_name)
                || stripped.eq_ignore_ascii_case(&contact_name)
            {
                skipped_header = true;
                continue;
            }
            if date_only_re().is_match(stripped) && current_date.is_none() {
                continue;
            }
            skipped_header = true;
        }

        if let Some(caps) = message_header_re().captures(stripped) {
            flush(
                &mut current_date,
                &mut current_sender,
                &mut body_lines,
                &mut skipped_invalid_date,
                &mut messages,
            );
            current_date = Some(caps[1].to_string());
            current_sender = Some(caps[2].trim().to_string());
            continue;
        }

        // A date-only line is a separator only between messages (or in the
        // preamble, handled above). Once a message body is open the line is
        // content — a text that is just a date must not lose its text.
        if date_only_re().is_match(stripped) && current_date.is_none() {
            continue;
        }

        if current_date.is_some() {
            body_lines.push(line.trim_end().to_string());
        }
    }
    flush(
        &mut current_date,
        &mut current_sender,
        &mut body_lines,
        &mut skipped_invalid_date,
        &mut messages,
    );

    assign_archive_attachments(&mut messages, att_queue);

    // Drop messages that ended up with neither text nor attachments.
    messages.retain(|m| !m.text.trim().is_empty() || !m.attachments.is_empty());

    Ok((messages, skipped_invalid_date))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_archive_thread() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.eml");
        std::fs::write(
            &path,
            b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive with Alice (2011-2013)\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-01-01 12:00:00 - Me\r\n\
Check this\r\n\
2020-01-01 12:01:00 - Alice\r\n\
Thanks\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let (msgs, _) = parse_archive_eml_mail(&path, &mail, &headers).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].is_from_me);
        assert_eq!(msgs[0].text, "Check this");
        assert!(!msgs[1].is_from_me);
        assert_eq!(msgs[1].text, "Thanks");
        assert_eq!(msgs[0].name_hint.as_deref(), Some("Alice"));
    }

    #[test]
    fn clean_archive_contact_name_strips_with_and_years() {
        assert_eq!(clean_archive_contact_name("with Alice"), "Alice");
        assert_eq!(
            clean_archive_contact_name("with Alice (2011-2013)"),
            "Alice"
        );
        assert_eq!(clean_archive_contact_name("Alice"), "Alice");
        // Non-ASCII names: byte 5 can land mid-character (H5). Must not panic
        // and must not strip a `with` that is not actually there.
        assert_eq!(clean_archive_contact_name("with 中文名"), "中文名");
        assert_eq!(clean_archive_contact_name("SMS 中文名"), "SMS 中文名");
    }

    #[test]
    fn date_only_line_inside_message_body_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.eml");
        std::fs::write(
            &path,
            b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive Alice\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-01-01 12:00:00 - Me\r\n\
2020-01-02\r\n\
2020-01-01 12:01:00 - Alice\r\n\
Thanks\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let (msgs, _) = parse_archive_eml_mail(&path, &mail, &headers).unwrap();
        assert_eq!(msgs.len(), 2);
        // A text that is just a date is content, not a separator.
        assert_eq!(msgs[0].text, "2020-01-02");
        assert_eq!(msgs[1].text, "Thanks");
    }

    #[test]
    fn parses_archive_with_unknown_phone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.eml");
        std::fs::write(
            &path,
            b"From: someone@example.com\r\n\
To: me@example.com\r\n\
Subject: SMS archive Mystery\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Mystery\r\n\
2020-01-01 12:00:00 - Me\r\n\
Hi there\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let (msgs, _) = parse_archive_eml_mail(&path, &mail, &headers).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].chat_key, "");
        assert_eq!(crate::identity::chat_id_for(&msgs[0]), "unknown");
        assert_eq!(msgs[0].text, "Hi there");
    }

    #[test]
    fn skips_unparseable_archive_dates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.eml");
        std::fs::write(
            &path,
            b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive Alice\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-13-01 12:00:00 - Me\r\n\
Bad stamp\r\n\
2020-01-01 12:00:00 - Alice\r\n\
Thanks\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = MailHeaders::from_mail(&mail);
        let (msgs, skipped) = parse_archive_eml_mail(&path, &mail, &headers).unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text, "Thanks");
        assert!(msgs[0].timestamp_secs > 1.0);
    }

    #[test]
    fn empty_body_message_prefers_attachment() {
        let mut messages = vec![
            ParsedMessage {
                chat_key: "4075551234".into(),
                conversation_type: "individual".into(),
                timestamp_secs: 1.0,
                text: "hello".into(),
                ..Default::default()
            },
            ParsedMessage {
                chat_key: "4075551234".into(),
                conversation_type: "individual".into(),
                timestamp_secs: 2.0,
                text: "".into(),
                ..Default::default()
            },
        ];
        let att = AttachmentBlob {
            filename: "a.jpg".into(),
            original_name: Some("a.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_hex: "abc".into(),
            data: vec![1, 2, 3],
        };
        assign_archive_attachments(&mut messages, vec![att]);
        assert!(messages[0].attachments.is_empty());
        assert_eq!(messages[1].attachments.len(), 1);
    }
}
