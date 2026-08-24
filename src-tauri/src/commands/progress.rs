//! Progress parsing for `extract` log lines.
//!
//! Exporters write log lines with progress counts. This module turns the
//! lines that carry counts into [`ExtractProgressEvent`]s the UI can use.

use std::sync::{Arc, Mutex};

use super::events::ExtractProgressEvent;

/// Whether log lines are still about reading the backup, or already about
/// writing conversation files.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum ExtractProgressStage {
    Parse,
    Convert,
}

/// Turn an exporter log line into a progress event, if the line has counts.
pub(crate) fn extract_progress_from_log(
    line: &str,
    stage: &Arc<Mutex<ExtractProgressStage>>,
) -> Option<ExtractProgressEvent> {
    if is_writing_conversation_files_banner(line) {
        if let Ok(mut current_stage) = stage.lock() {
            *current_stage = ExtractProgressStage::Convert;
        }
        return Some(ExtractProgressEvent {
            step: "convert".into(),
            done: 0,
            total: 0,
            status: Some("included_in_extract".into()),
        });
    }

    let (done, total) = extract_progress_ratio(line)?;

    let current_stage = match stage.lock() {
        Ok(guard) => *guard,
        Err(_) => ExtractProgressStage::Parse,
    };
    let step = match current_stage {
        ExtractProgressStage::Parse => "parse",
        ExtractProgressStage::Convert => "convert",
    };

    Some(ExtractProgressEvent {
        step: step.into(),
        done,
        total,
        status: None,
    })
}

/// True for the log line that means "finished reading, now writing files".
fn is_writing_conversation_files_banner(line: &str) -> bool {
    line.contains("Writing ") && line.contains("conversation file(s)")
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

/// Read `done/total` from a message-progress log line.
fn extract_progress_ratio(line: &str) -> Option<(usize, usize)> {
    if has_bracketed_step_ratio(line) {
        return None;
    }

    let looks_like_message_progress = line.contains('…') || line.contains("wrote");
    if !looks_like_message_progress {
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
    fn extract_progress_parser_tracks_parse_and_convert() {
        let stage = Arc::new(Mutex::new(ExtractProgressStage::Parse));

        let parse = extract_progress_from_log("  …500/12345 messages", &stage).unwrap();
        assert_eq!(parse.step, "parse");
        assert_eq!(parse.done, 500);
        assert_eq!(parse.total, 12345);
        assert_eq!(parse.status, None);

        let banner =
            extract_progress_from_log("Writing 3 conversation file(s)...", &stage).unwrap();
        assert_eq!(banner.step, "convert");
        assert_eq!(banner.done, 0);
        assert_eq!(banner.total, 0);
        assert_eq!(banner.status.as_deref(), Some("included_in_extract"));

        let ignored = extract_progress_from_log("[1/5] Deriving backup keys...", &stage);
        assert!(ignored.is_none());

        let backup_step = extract_progress_from_log("[2/5] Resolving messages database...", &stage);
        assert!(backup_step.is_none());

        let convert = extract_progress_from_log("  wrote 2/3 messages", &stage).unwrap();
        assert_eq!(convert.step, "convert");
        assert_eq!(convert.done, 2);
        assert_eq!(convert.total, 3);
        assert_eq!(convert.status, None);
    }
}
