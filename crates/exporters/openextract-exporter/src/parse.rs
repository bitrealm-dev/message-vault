//! Parse OpenExtract conversation CSV (per-chat or all-conversations).

use anyhow::{Context, Result, bail};
use message_vault_io_core::discover_files;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    PerChat,
    AllConversations,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::PerChat => "per-chat",
            SourceKind::AllConversations => "all-conversations",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RawRow {
    pub date: String,
    pub conversation: Option<String>,
    pub direction: Option<String>,
    pub sender: String,
    pub text: String,
    pub is_from_me: bool,
    pub has_attachments: bool,
    pub source_kind: SourceKind,
}

/// Discover OpenExtract conversation CSV files under `input` (file or directory).
pub(crate) fn discover_csv_files(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_file() {
        if is_conversation_csv(input) {
            return Ok(vec![input.to_path_buf()]);
        }
        bail!(
            "{} is not an OpenExtract conversation CSV (skip *_attachments.csv)",
            input.display()
        );
    }
    if !input.is_dir() {
        bail!("input path not found: {}", input.display());
    }
    let mut files = discover_files(input, &|p| is_conversation_csv(p))
        .with_context(|| format!("read_dir {}", input.display()))?;
    files.sort();
    if files.is_empty() {
        bail!(
            "no OpenExtract conversation CSV files found under {}",
            input.display()
        );
    }
    Ok(files)
}

fn is_conversation_csv(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".csv") {
        return false;
    }
    if lower.contains("attachment") {
        return false;
    }
    true
}

pub(crate) fn parse_csv_file(path: &Path) -> Result<Vec<RawRow>> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    // Strip UTF-8 BOM if present.
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(bytes.as_slice());
    let headers = rdr
        .headers()
        .with_context(|| format!("headers {}", path.display()))?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();

    let date_i = col(&headers, "date")?;
    let sender_i = col(&headers, "sender")?;
    let text_i = col(&headers, "text")?;
    let is_from_me_i = col(&headers, "is from me")?;
    let has_att_i = col(&headers, "has attachments")?;
    let conversation_i = headers.iter().position(|h| h == "conversation");
    let direction_i = headers.iter().position(|h| h == "direction");

    let source_kind = if conversation_i.is_some() {
        SourceKind::AllConversations
    } else {
        SourceKind::PerChat
    };

    let mut rows = Vec::new();
    for (idx, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("row {} in {}", idx + 2, path.display()))?;
        let date = field(&rec, date_i);
        let sender = field(&rec, sender_i);
        let text = field(&rec, text_i);
        if date.is_empty() && sender.is_empty() && text.is_empty() {
            continue;
        }
        let is_from_me = parse_bool(&field(&rec, is_from_me_i));
        let has_attachments = parse_bool(&field(&rec, has_att_i));
        let conversation = conversation_i
            .map(|i| field(&rec, i))
            .filter(|s| !s.is_empty());
        let direction = direction_i
            .map(|i| field(&rec, i))
            .filter(|s| !s.is_empty());
        rows.push(RawRow {
            date,
            conversation,
            direction,
            sender,
            text,
            is_from_me,
            has_attachments,
            source_kind,
        });
    }
    Ok(rows)
}

fn col(headers: &[String], name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .with_context(|| format!("missing column {name:?} (have {headers:?})"))
}

fn field(rec: &csv::StringRecord, idx: usize) -> String {
    rec.get(idx).unwrap_or("").trim().to_string()
}

fn parse_bool(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}
