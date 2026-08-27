//! Progress parsing for `extract` log lines.
//!
//! Exporters write log lines with progress counts. This module turns the
//! lines that carry counts into [`ExtractProgressEvent`]s the UI can use.

use std::sync::{Arc, Mutex};

use super::events::ExtractProgressEvent;

/// Whether log lines are about reading the backup, copying attachments, or
/// writing conversation files.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum ExtractProgressStage {
    Parse,
    Attachments,
    Prepare,
}

/// Turn an exporter log line into a progress event, if the line has counts.
pub(crate) fn extract_progress_from_log(
    line: &str,
    stage: &Arc<Mutex<ExtractProgressStage>>,
) -> Option<ExtractProgressEvent> {
    if let Some(total) = preparing_conversation_files_total(line) {
        if let Ok(mut current_stage) = stage.lock() {
            *current_stage = ExtractProgressStage::Prepare;
        }
        return Some(ExtractProgressEvent {
            step: "prepare".into(),
            done: 0,
            total,
            bytes_done: None,
            bytes_total: None,
            status: None,
        });
    }

    if let Some(event) = attachment_progress_from_log(line) {
        if let Ok(mut current_stage) = stage.lock() {
            *current_stage = ExtractProgressStage::Attachments;
        }
        return Some(event);
    }

    let (done, total) = extract_progress_ratio(line)?;

    let current_stage = match stage.lock() {
        Ok(guard) => *guard,
        Err(_) => ExtractProgressStage::Parse,
    };
    let step = match current_stage {
        ExtractProgressStage::Parse => "parse",
        ExtractProgressStage::Attachments => "attachments",
        ExtractProgressStage::Prepare => "prepare",
    };

    Some(ExtractProgressEvent {
        step: step.into(),
        done,
        total,
        bytes_done: None,
        bytes_total: None,
        status: None,
    })
}

/// `Preparing 3 conversation file(s)...` → 3.
fn preparing_conversation_files_total(line: &str) -> Option<usize> {
    if !(line.contains("Preparing ") && line.contains("conversation file(s)")) {
        return None;
    }
    let after = line.split_once("Preparing ")?.1;
    leading_usize(after)
}

/// `  attachments 2/3 100/500` → attachments event with byte counts.
fn attachment_progress_from_log(line: &str) -> Option<ExtractProgressEvent> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("attachments ")?;
    let (files, bytes) = rest.split_once(' ')?;
    let (done, total) = split_ratio(files)?;
    let (bytes_done, bytes_total) = split_u64_ratio(bytes)?;
    Some(ExtractProgressEvent {
        step: "attachments".into(),
        done,
        total,
        bytes_done: Some(bytes_done),
        bytes_total: Some(bytes_total),
        status: None,
    })
}

fn split_ratio(text: &str) -> Option<(usize, usize)> {
    let (left, right) = text.split_once('/')?;
    Some((left.parse().ok()?, right.parse().ok()?))
}

fn split_u64_ratio(text: &str) -> Option<(u64, u64)> {
    let (left, right) = text.split_once('/')?;
    Some((left.parse().ok()?, right.parse().ok()?))
}

/// True for backup-setup lines like `[1/5] Deriving backup keys...`.
///
/// Those counts are setup steps, not message progress, so they must not
/// move the progress bar.
fn has_bracketed_step_ratio(line: &str) -> bool {
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        rest = &rest[open + 1..];
        let Some((left, after_left)) = rest.split_once('/') else {
            continue;
        };
        if left.is_empty() || !left.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some((right, after_right)) = after_left.split_once(']') else {
            continue;
        };
        if !right.is_empty() && right.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        rest = after_right;
    }
    false
}

/// Read `done/total` from a message-progress or prepare-progress log line.
fn extract_progress_ratio(line: &str) -> Option<(usize, usize)> {
    if has_bracketed_step_ratio(line) {
        return None;
    }

    let looks_like_counts =
        line.contains('…') || line.contains("wrote") || line.contains("preparing");
    if !looks_like_counts {
        return None;
    }

    let (left, right) = line.split_once('/')?;
    let done = trailing_usize(left)?;
    let total = leading_usize(right)?;
    Some((done, total))
}

/// Parse the integer at the end of `text`, if any.
fn trailing_usize(text: &str) -> Option<usize> {
    let mut reversed_digits = String::new();
    for ch in text.chars().rev() {
        if !ch.is_ascii_digit() {
            break;
        }
        reversed_digits.push(ch);
    }
    if reversed_digits.is_empty() {
        return None;
    }
    let digits: String = reversed_digits.chars().rev().collect();
    digits.parse().ok()
}

/// Parse the integer at the start of `text`, if any.
fn leading_usize(text: &str) -> Option<usize> {
    let mut digits = String::new();
    for ch in text.chars() {
        if !ch.is_ascii_digit() {
            break;
        }
        digits.push(ch);
    }
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_progress_parser_tracks_parse_attachments_and_prepare() {
        let stage = Arc::new(Mutex::new(ExtractProgressStage::Parse));

        let parse = extract_progress_from_log("  …500/12345 messages", &stage).unwrap();
        assert_eq!(parse.step, "parse");
        assert_eq!(parse.done, 500);
        assert_eq!(parse.total, 12345);
        assert_eq!(parse.status, None);

        let attachments = extract_progress_from_log("  attachments 2/3 100/500", &stage).unwrap();
        assert_eq!(attachments.step, "attachments");
        assert_eq!(attachments.done, 2);
        assert_eq!(attachments.total, 3);
        assert_eq!(attachments.bytes_done, Some(100));
        assert_eq!(attachments.bytes_total, Some(500));

        let ignored = extract_progress_from_log("[1/5] Deriving backup keys...", &stage);
        assert!(ignored.is_none());

        let backup_step = extract_progress_from_log("[2/5] Resolving messages database...", &stage);
        assert!(backup_step.is_none());

        let banner =
            extract_progress_from_log("Preparing 3 conversation file(s)...", &stage).unwrap();
        assert_eq!(banner.step, "prepare");
        assert_eq!(banner.done, 0);
        assert_eq!(banner.total, 3);
        assert_eq!(banner.status, None);

        let prepare = extract_progress_from_log("  preparing 2/3", &stage).unwrap();
        assert_eq!(prepare.step, "prepare");
        assert_eq!(prepare.done, 2);
        assert_eq!(prepare.total, 3);
        assert_eq!(prepare.status, None);
    }
}
