//! Append-only resumable outcome journal under the export folder.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const JOURNAL_NAME: &str = ".vault-import-state.jsonl";
pub const REPORT_NAME: &str = "vault-push-report.json";
pub const LOG_NAME: &str = "vault-push.log";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalMessage {
    pub file: String,
    pub guid: String,
}

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

#[derive(Debug, Default)]
pub struct JournalState {
    pub assets: HashSet<String>,
    pub messages: HashSet<String>,
    pub files: HashSet<String>,
}

impl JournalState {
    pub fn message_key(file: &str, guid: &str) -> String {
        format!("{file}\0{guid}")
    }
}

pub fn journal_path(input: &Path) -> PathBuf {
    input.join(JOURNAL_NAME)
}

pub fn load(path: &Path, url: &str, username: &str) -> Result<JournalState> {
    let mut state = JournalState::default();
    if !path.is_file() {
        return Ok(state);
    }
    let file = File::open(path).with_context(|| format!("open journal {}", path.display()))?;
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read journal line {}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: JournalEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                // Truncated/corrupt line — log a warning so users know
                // the journal is damaged rather than silently skipping state.
                eprintln!(
                    "warning: journal {} line {} is corrupt ({}). \
                     The affected entries will be re-submitted (server dedup is safe).",
                    path.display(),
                    i + 1,
                    e
                );
                continue;
            }
        };
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
                state.messages.extend(
                    messages
                        .into_iter()
                        .map(|message| JournalState::message_key(&message.file, &message.guid)),
                );
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

pub fn append(path: &Path, event: &JournalEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open journal for append {}", path.display()))?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

pub fn compact(path: &Path, url: &str, username: &str, state: &JournalState) -> Result<()> {
    let mut events = Vec::new();
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
    let mut messages: Vec<_> = state
        .messages
        .iter()
        .filter_map(|key| key.split_once('\0'))
        .map(|(file, guid)| JournalMessage {
            file: file.to_string(),
            guid: guid.to_string(),
        })
        .collect();
    messages.sort_unstable_by(|a, b| (&a.file, &a.guid).cmp(&(&b.file, &b.guid)));
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
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = File::create(&tmp)?;
        for event in events {
            serde_json::to_writer(&mut out, &event)?;
            out.write_all(b"\n")?;
        }
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
