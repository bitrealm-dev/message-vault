// crates/vault-pull/src/journal.rs
//! Append-only resume journal for vault-pull (mirrors vault-push/src/journal.rs).

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const PULL_JOURNAL_NAME: &str = ".vault-pull-state.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PullJournalEvent {
    AssetOk {
        url: String,
        username: String,
        sha256: String,
        path: String,
        size_bytes: u64,
    },
    BackupComplete {
        url: String,
        username: String,
        conversations: u64,
        messages: u64,
        assets: u64,
    },
}

#[derive(Debug, Default)]
pub struct PullJournalState {
    /// SHA-256 digests of assets already downloaded.
    pub assets: HashSet<String>,
    /// True if the last run completed cleanly (a `backup_complete` event was written).
    pub backup_complete: bool,
}

pub fn journal_path(out_dir: &Path) -> PathBuf {
    out_dir.join(PULL_JOURNAL_NAME)
}

pub fn load(path: &Path, url: &str, username: &str) -> Result<PullJournalState> {
    let mut state = PullJournalState::default();
    if !path.is_file() {
        return Ok(state);
    }
    let file = File::open(path).with_context(|| format!("open pull journal {}", path.display()))?;
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read pull journal line {}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: PullJournalEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue, // skip unparseable lines (forward compat)
        };
        match event {
            PullJournalEvent::AssetOk {
                url: u,
                username: a,
                sha256,
                ..
            } if u == url && a == username => {
                state.assets.insert(sha256);
            }
            PullJournalEvent::BackupComplete {
                url: u,
                username: a,
                ..
            } if u == url && a == username => {
                state.backup_complete = true;
            }
            _ => {}
        }
    }
    Ok(state)
}

pub fn append(path: &Path, event: &PullJournalEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open pull journal for append {}", path.display()))?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

pub fn compact(path: &Path, url: &str, username: &str, state: &PullJournalState) -> Result<()> {
    let mut events: Vec<PullJournalEvent> = Vec::new();
    let mut assets: Vec<_> = state.assets.iter().collect();
    assets.sort_unstable();
    for sha in assets {
        events.push(PullJournalEvent::AssetOk {
            url: url.to_string(),
            username: username.to_string(),
            sha256: sha.clone(),
            path: String::new(),   // path not needed for resume (uses attachments/{sha})
            size_bytes: 0,         // size not needed for resume
        });
    }
    if state.backup_complete {
        // backup_complete is rewritten during compact — conversations/messages/assets set to 0
        // since the compacted form only needs to signal "was complete"
        events.push(PullJournalEvent::BackupComplete {
            url: url.to_string(),
            username: username.to_string(),
            conversations: 0,
            messages: 0,
            assets: 0,
        });
    }
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = File::create(&tmp)?;
        for event in &events {
            serde_json::to_writer(&mut out, event)?;
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
    fn loads_asset_and_backup_complete_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PULL_JOURNAL_NAME);
        fs::write(
            &path,
            concat!(
                "{\"event\":\"asset_ok\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"sha256\":\"aaabbbccc\",\"path\":\"attachments/aaabbbccc\",\"size_bytes\":12345}\n",
                "{\"event\":\"asset_ok\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"sha256\":\"dddeeefff\",\"path\":\"attachments/dddeeefff\",\"size_bytes\":67890}\n",
                "{\"event\":\"backup_complete\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"conversations\":2,\"messages\":100,\"assets\":2}\n",
            ),
        )
        .unwrap();

        let state = load(&path, "http://vault", "alice").unwrap();

        assert!(state.assets.contains("aaabbbccc"));
        assert!(state.assets.contains("dddeeefff"));
        assert!(state.backup_complete);
    }

    #[test]
    fn filters_by_url_and_username() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PULL_JOURNAL_NAME);
        fs::write(
            &path,
            concat!(
                "{\"event\":\"asset_ok\",\"url\":\"http://vault-a\",\"username\":\"alice\",",
                "\"sha256\":\"aaa\",\"path\":\"attachments/aaa\",\"size_bytes\":1}\n",
                "{\"event\":\"asset_ok\",\"url\":\"http://vault-b\",\"username\":\"bob\",",
                "\"sha256\":\"bbb\",\"path\":\"attachments/bbb\",\"size_bytes\":2}\n",
            ),
        )
        .unwrap();

        let state = load(&path, "http://vault-a", "alice").unwrap();
        assert!(state.assets.contains("aaa"));
        assert!(!state.assets.contains("bbb"));
    }

    #[test]
    fn compact_sorts_assets_and_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PULL_JOURNAL_NAME);
        let mut state = PullJournalState::default();
        state.assets.insert("ccc".into());
        state.assets.insert("aaa".into());
        state.assets.insert("bbb".into());
        state.backup_complete = true;

        compact(&path, "http://vault", "alice", &state).unwrap();

        let reloaded = load(&path, "http://vault", "alice").unwrap();
        assert_eq!(reloaded.assets.len(), 3);
        assert!(reloaded.assets.contains("aaa"));
        assert!(reloaded.assets.contains("bbb"));
        assert!(reloaded.assets.contains("ccc"));
        assert!(reloaded.backup_complete);
    }
}
