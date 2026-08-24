//! Local log of which conversations and files were already uploaded.
//!
//! The file is `.vault-import-state.jsonl`. JSON Lines means one JSON object per
//! line. A later push can skip work that already succeeded. The Message Vault
//! HTTP server still ignores true duplicates if a line is sent again.

use std::collections::HashSet;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Filename of the local upload log, written next to the conversation files.
pub const JOURNAL_NAME: &str = ".vault-import-state.jsonl";
/// Filename of the JSON summary written at the end of a push.
pub const REPORT_NAME: &str = "vault-push-report.json";
/// Filename of the human-readable push log.
pub const LOG_NAME: &str = "vault-push.log";

/// One message identity recorded in the journal: conversation file plus guid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalMessage {
    pub file: String,
    pub guid: String,
}

/// One row in `.vault-import-state.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JournalEvent {
    AssetOk {
        url: String,
        username: String,
        source: String,
        sha256: String,
    },
    MessageOk {
        url: String,
        username: String,
        source: String,
        file: String,
        guid: String,
    },
    MessageBatchOk {
        url: String,
        username: String,
        source: String,
        messages: Vec<JournalMessage>,
    },
    FileOk {
        url: String,
        username: String,
        source: String,
        file: String,
    },
    Fail {
        url: String,
        username: String,
        source: String,
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        guid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
        stage: String,
        error: String,
    },
}

impl JournalEvent {
    /// Vault URL and username this event belongs to.
    fn target(&self) -> (&str, &str) {
        match self {
            Self::AssetOk { url, username, .. }
            | Self::MessageOk { url, username, .. }
            | Self::MessageBatchOk { url, username, .. }
            | Self::FileOk { url, username, .. }
            | Self::Fail { url, username, .. } => (url.as_str(), username.as_str()),
        }
    }
}

/// In-memory skip sets rebuilt from the journal for one vault URL and username.
#[derive(Debug, Default)]
pub struct JournalState {
    pub assets: HashSet<String>,
    pub messages: HashSet<String>,
    pub files: HashSet<String>,
}

impl JournalState {
    /// Join a conversation filename and message guid into one set key.
    pub fn message_key(file: &str, guid: &str) -> String {
        format!("{file}\0{guid}")
    }
}

/// Path of `.vault-import-state.jsonl` inside the export folder.
pub fn journal_path(input: &Path) -> PathBuf {
    input.join(JOURNAL_NAME)
}

/// Read the journal and keep events that match this vault URL and username.
///
/// A missing file is treated as an empty journal. A corrupt line is skipped
/// after a warning; those entries will be uploaded again. The server ignores
/// true duplicates.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or a line cannot be read.
pub fn load(path: &Path, url: &str, username: &str) -> Result<JournalState> {
    let mut state = JournalState::default();
    let events: Vec<JournalEvent> = jsonl_journal::load_events("journal", path, &mut |i, e| {
        eprintln!(
            "warning: journal {} line {} is corrupt ({}). \
             The affected entries will be re-submitted (server dedup is safe).",
            path.display(),
            i,
            e
        );
    })?;
    for event in events {
        match event {
            JournalEvent::AssetOk {
                url: u,
                username: a,
                sha256,
                ..
            } if u == url && a == username => {
                state.assets.insert(sha256);
            }
            JournalEvent::MessageOk {
                url: u,
                username: a,
                file,
                guid,
                ..
            } if u == url && a == username => {
                state
                    .messages
                    .insert(JournalState::message_key(&file, &guid));
            }
            JournalEvent::MessageBatchOk {
                url: u,
                username: a,
                messages,
                ..
            } if u == url && a == username => {
                for message in messages {
                    state
                        .messages
                        .insert(JournalState::message_key(&message.file, &message.guid));
                }
            }
            JournalEvent::FileOk {
                url: u,
                username: a,
                file,
                ..
            } if u == url && a == username => {
                state.files.insert(file);
            }
            _ => {}
        }
    }
    Ok(state)
}

/// Append one event as a JSON Lines row and flush it to disk.
///
/// # Errors
///
/// Returns an error when the parent folder cannot be created, the file cannot
/// be opened, or the write fails.
pub fn append(path: &Path, event: &JournalEvent) -> Result<()> {
    jsonl_journal::append("journal", path, event)
}

/// Rewrite the journal from in-memory `state` for one vault URL and username.
///
/// Events for other URL and username pairs are kept, so one export folder can
/// resume against more than one server.
///
/// # Errors
///
/// Returns an error when the existing file cannot be read, the temporary file
/// cannot be written, or the rename fails.
pub fn compact(path: &Path, url: &str, username: &str, state: &JournalState) -> Result<()> {
    jsonl_journal::compact_with::<JournalEvent, _>("journal", path, |mut events| {
        // Preserve other vault targets so one export folder can resume against
        // multiple servers without wiping their skip state.
        events.retain(|event| {
            let (u, a) = event.target();
            u != url || a != username
        });
        let mut assets: Vec<_> = state.assets.iter().collect();
        assets.sort_unstable();
        for sha in assets {
            events.push(JournalEvent::AssetOk {
                url: url.to_string(),
                username: username.to_string(),
                source: String::new(),
                sha256: sha.clone(),
            });
        }
        let messages = messages_from_state_keys(state);
        for batch in messages.chunks(1_000) {
            events.push(JournalEvent::MessageBatchOk {
                url: url.to_string(),
                username: username.to_string(),
                source: String::new(),
                messages: batch.to_vec(),
            });
        }
        let mut files: Vec<_> = state.files.iter().collect();
        files.sort_unstable();
        for file in files {
            events.push(JournalEvent::FileOk {
                url: url.to_string(),
                username: username.to_string(),
                source: String::new(),
                file: file.clone(),
            });
        }
        events
    })
}

/// Split stored `file\0guid` keys back into journal message records, sorted.
fn messages_from_state_keys(state: &JournalState) -> Vec<JournalMessage> {
    let mut messages: Vec<JournalMessage> = Vec::new();
    for key in &state.messages {
        let Some((file, guid)) = key.split_once('\0') else {
            continue;
        };
        messages.push(JournalMessage {
            file: file.to_string(),
            guid: guid.to_string(),
        });
    }
    messages.sort_unstable_by(|a, b| (&a.file, &a.guid).cmp(&(&b.file, &b.guid)));
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn loads_legacy_and_batch_message_success_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(JOURNAL_NAME);
        fs::write(
            &path,
            concat!(
                "{\"event\":\"message_ok\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"source\":\"sms\",\"file\":\"first.jsonl\",\"guid\":\"guid-1\"}\n",
                "{\"event\":\"message_batch_ok\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"source\":\"sms\",\"messages\":[{\"file\":\"second.jsonl\",\"guid\":\"guid-2\"}]}\n"
            ),
        )
        .unwrap();

        let state = load(&path, "http://vault", "alice").unwrap();

        assert!(
            state
                .messages
                .contains(&JournalState::message_key("first.jsonl", "guid-1"))
        );
        assert!(
            state
                .messages
                .contains(&JournalState::message_key("second.jsonl", "guid-2"))
        );
    }

    #[test]
    fn compact_preserves_other_vault_target_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(JOURNAL_NAME);
        fs::write(
            &path,
            concat!(
                r#"{"event":"asset_ok","url":"http://a","username":"alice","source":"sms","sha256":"aaa"}"#,
                "\n",
                r#"{"event":"file_ok","url":"http://b","username":"bob","source":"sms","file":"chat.jsonl"}"#,
                "\n",
            ),
        )
        .unwrap();

        let mut state = JournalState::default();
        state.assets.insert("bbb".into());
        compact(&path, "http://b", "bob", &state).unwrap();

        let a = load(&path, "http://a", "alice").unwrap();
        assert!(a.assets.contains("aaa"));
        let b = load(&path, "http://b", "bob").unwrap();
        assert!(b.assets.contains("bbb"));
        assert!(!b.files.contains("chat.jsonl"));
    }

    #[test]
    fn append_writes_complete_lines_under_contention() {
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join(JOURNAL_NAME));
        let mut handles = Vec::new();
        for i in 0..8 {
            let path = Arc::clone(&path);
            handles.push(thread::spawn(move || {
                for j in 0..50 {
                    let guid = format!("g-{i}-{j}");
                    let messages: Vec<_> = (0..200)
                        .map(|k| JournalMessage {
                            file: format!("f{i}.jsonl"),
                            guid: format!("{guid}-{k}"),
                        })
                        .collect();
                    append(
                        &path,
                        &JournalEvent::MessageBatchOk {
                            url: "http://vault".into(),
                            username: "alice".into(),
                            source: "sms".into(),
                            messages,
                        },
                    )
                    .unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let text = fs::read_to_string(&*path).unwrap();
        let mut lines = 0usize;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            serde_json::from_str::<JournalEvent>(line).expect("torn line");
            lines += 1;
        }
        assert_eq!(lines, 8 * 50);
    }
}
