//! Reverse projectors: JSON / JSONL → [`ConversationDocument`].

use anyhow::{Context, Result, bail};
use message_ir::{ConversationDocument, ConversationHeader, IrMessage, SCHEMA_VERSION};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Read a conversation JSON file written by [`crate::write_conversation_json`].
pub fn read_conversation_json(path: &Path) -> Result<ConversationDocument> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut doc: ConversationDocument = serde_json::from_str(&raw)
        .with_context(|| format!("parse ConversationDocument {}", path.display()))?;
    if doc.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported schema_version {} in {} (expected {})",
            doc.schema_version,
            path.display(),
            SCHEMA_VERSION
        );
    }
    if doc.packaging_stem_suffix.is_none() {
        doc.packaging_stem_suffix = path
            .file_stem()
            .and_then(|n| n.to_str())
            .and_then(crate::util::packaging_suffix_from_stem);
    }
    doc.finalize_stats();
    Ok(doc)
}

/// Read a conversation JSONL file written by [`crate::write_conversation_jsonl`].
///
/// Line 1 is a [`ConversationHeader`]; each following line is one [`IrMessage`].
pub fn read_conversation_jsonl(path: &Path) -> Result<ConversationDocument> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut lines = BufReader::new(file).lines();
    let header_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty JSONL: {}", path.display()))?
        .with_context(|| format!("read JSONL header {}", path.display()))?;
    let header: ConversationHeader = serde_json::from_str(&header_line)
        .with_context(|| format!("parse JSONL header {}", path.display()))?;
    if header.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported schema_version {} in {} (expected {})",
            header.schema_version,
            path.display(),
            SCHEMA_VERSION
        );
    }

    let mut messages = Vec::new();
    for (i, line) in lines.enumerate() {
        let line =
            line.with_context(|| format!("read JSONL line {} in {}", i + 2, path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: IrMessage = serde_json::from_str(&line)
            .with_context(|| format!("parse JSONL message line {} in {}", i + 2, path.display()))?;
        messages.push(msg);
    }
    if messages.is_empty() {
        bail!("JSONL has no message lines: {}", path.display());
    }

    let packaging_stem_suffix = path
        .file_stem()
        .and_then(|n| n.to_str())
        .and_then(crate::util::packaging_suffix_from_stem);

    let mut doc = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: header.export,
        conversation: header.conversation,
        messages,
        packaging_stem_suffix,
    };
    doc.finalize_stats();
    Ok(doc)
}
