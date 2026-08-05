//! Page export messages, download assets, write message-ir JSONL folders.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use std::io::Read;

use anyhow::{Context, Result, bail};
use message_ir::ConversationDocument;
use message_vault_io_core::{CancelFlag, check_cancel};
use serde::Serialize;
use sha2::{Digest, Sha256};
use vault_push::authenticate;

use crate::http::{ExportMessage, HttpSession};
use crate::project::{build_document, conversation_key, to_ir_message};

pub const DEFAULT_PAGE_LIMIT: usize = 100;

#[derive(Debug, Clone)]
pub struct VaultPullConfig {
    pub out_dir: PathBuf,
    pub base_url: String,
    pub username: String,
    pub key: String,
    /// Free-form Fastmail-style query (may be empty).
    pub query: String,
    pub after: Option<String>,
    pub before: Option<String>,
    pub source: Option<String>,
    pub skip_attachments: bool,
    pub page_limit: usize,
    /// When set (typically from a prior Query), progress logs include "of N".
    pub expected_messages: Option<u64>,
    pub cancel: Option<CancelFlag>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullReport {
    pub ok: bool,
    pub account: String,
    pub query: String,
    pub conversations: u64,
    pub messages: u64,
    pub attachments_downloaded: u64,
    pub attachments_skipped: u64,
    pub out_dir: String,
}

/// Counts from a dry-run export query (no downloads / no JSONL write).
#[derive(Debug, Clone, Serialize)]
pub struct QueryStats {
    pub messages: u64,
    pub attachments: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Log(String),
    Auth {
        account_id: String,
        username: String,
    },
    Page {
        messages: usize,
        total_so_far: u64,
    },
    Done(PullReport),
}

pub type ProgressFn<'a> = dyn FnMut(ProgressEvent) + 'a;

/// Compose `after:` / `before:` operators onto a base query string.
pub fn compose_query(base: &str, after: Option<&str>, before: Option<&str>) -> String {
    let mut parts = Vec::new();
    let base = base.trim();
    if !base.is_empty() {
        parts.push(base.to_string());
    }
    if let Some(a) = after.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("after:{a}"));
    }
    if let Some(b) = before.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("before:{b}"));
    }
    parts.join(" ")
}

/// Prefer `GET /v1/export/messages/count`; fall back to paging the export
/// endpoint on older vaults that lack the count route.
/// Does not download assets or write JSONL. `cfg.out_dir` is ignored.
pub fn query_stats(
    cfg: &VaultPullConfig,
    mut on_progress: Option<&mut ProgressFn<'_>>,
) -> Result<QueryStats> {
    let emit = |on_progress: &mut Option<&mut ProgressFn<'_>>, event: ProgressEvent| {
        if let Some(cb) = on_progress.as_mut() {
            cb(event);
        }
    };

    if cfg.key.trim().is_empty() {
        bail!("vault key is required");
    }

    let auth = authenticate(&cfg.base_url, &cfg.key, &cfg.username)
        .map_err(|e| anyhow::anyhow!("{}", e.detail()))?;
    let account = auth.account_id.clone();
    let username = auth
        .username
        .clone()
        .unwrap_or_else(|| account.clone());
    emit(
        &mut on_progress,
        ProgressEvent::Auth {
            account_id: account.clone(),
            username: username.clone(),
        },
    );
    emit(
        &mut on_progress,
        ProgressEvent::Log(format!("Authenticated as {username} ({account})")),
    );

    let q = compose_query(
        &cfg.query,
        cfg.after.as_deref(),
        cfg.before.as_deref(),
    );
    emit(
        &mut on_progress,
        ProgressEvent::Log(if q.is_empty() {
            "Query: (all messages)".into()
        } else {
            format!("Query: {q}")
        }),
    );

    let session = HttpSession::new()?;
    match session.export_message_count(
        &cfg.base_url,
        &cfg.key,
        &q,
        &account,
        cfg.source.as_deref(),
    )? {
        Some(count) => {
            let stats = QueryStats {
                messages: count.messages,
                attachments: count.attachments,
                total_bytes: count.total_bytes,
            };
            emit(
                &mut on_progress,
                ProgressEvent::Log(format!(
                    "Query result: {} message(s), {} attachment(s), {} byte(s)",
                    stats.messages, stats.attachments, stats.total_bytes
                )),
            );
            return Ok(stats);
        }
        None => {
            emit(
                &mut on_progress,
                ProgressEvent::Log(
                    "Count endpoint not available; paging export messages for stats…".into(),
                ),
            );
        }
    }

    query_stats_by_paging(cfg, &session, &account, &q, &mut on_progress)
}

fn query_stats_by_paging(
    cfg: &VaultPullConfig,
    session: &HttpSession,
    account: &str,
    q: &str,
    on_progress: &mut Option<&mut ProgressFn<'_>>,
) -> Result<QueryStats> {
    let emit = |on_progress: &mut Option<&mut ProgressFn<'_>>, event: ProgressEvent| {
        if let Some(cb) = on_progress.as_mut() {
            cb(event);
        }
    };

    let mut cursor: Option<String> = None;
    let mut total_messages = 0u64;
    // sha256 -> size_bytes (None if unknown / older imports)
    let mut unique_assets: HashMap<String, Option<u64>> = HashMap::new();
    let mut seen_cursors: BTreeSet<String> = BTreeSet::new();
    const MAX_PAGES: usize = 10_000;

    loop {
        check_cancel(cfg.cancel.as_ref()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let page = session.export_messages(
            &cfg.base_url,
            &cfg.key,
            q,
            cfg.page_limit.max(1),
            cursor.as_deref(),
            account,
            cfg.source.as_deref(),
        )?;
        total_messages += page.messages.len() as u64;
        emit(
            on_progress,
            ProgressEvent::Page {
                messages: page.messages.len(),
                total_so_far: total_messages,
            },
        );
        emit(
            on_progress,
            ProgressEvent::Log(format!(
                "Fetched {} message(s) ({} total)",
                page.messages.len(),
                total_messages
            )),
        );

        for msg in &page.messages {
            for att in &msg.attachments {
                if let Some(sha) = att
                    .sha256
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    unique_assets
                        .entry(sha.to_string())
                        .and_modify(|existing| {
                            if existing.is_none() {
                                *existing = att.size_bytes;
                            }
                        })
                        .or_insert(att.size_bytes);
                }
            }
        }

        match page.next_cursor {
            Some(next) if !next.is_empty() => {
                if seen_cursors.contains(&next) {
                    bail!(
                        "pagination loop detected: cursor {next} was already seen. \
                         The vault may be returning the same cursor repeatedly."
                    );
                }
                if seen_cursors.len() >= MAX_PAGES {
                    bail!(
                        "pagination limit reached ({MAX_PAGES} pages). \
                         Use a narrower query to export fewer messages at once."
                    );
                }
                seen_cursors.insert(next.clone());
                cursor = Some(next);
            }
            _ => break,
        }
    }

    let attachments = unique_assets.len() as u64;
    let total_bytes = unique_assets.values().filter_map(|s| *s).sum();
    let stats = QueryStats {
        messages: total_messages,
        attachments,
        total_bytes,
    };
    emit(
        on_progress,
        ProgressEvent::Log(format!(
            "Query result: {} message(s), {} attachment(s), {} byte(s)",
            stats.messages, stats.attachments, stats.total_bytes
        )),
    );
    Ok(stats)
}

pub fn run(cfg: &VaultPullConfig, mut on_progress: Option<&mut ProgressFn<'_>>) -> Result<PullReport> {
    let emit = |on_progress: &mut Option<&mut ProgressFn<'_>>, event: ProgressEvent| {
        if let Some(cb) = on_progress.as_mut() {
            cb(event);
        }
    };

    if cfg.key.trim().is_empty() {
        bail!("vault key is required");
    }
    if cfg.out_dir.as_os_str().is_empty() {
        bail!("output directory is required");
    }

    let auth = authenticate(&cfg.base_url, &cfg.key, &cfg.username)
        .map_err(|e| anyhow::anyhow!("{}", e.detail()))?;
    let account = auth.account_id.clone();
    let username = auth
        .username
        .clone()
        .unwrap_or_else(|| account.clone());
    emit(
        &mut on_progress,
        ProgressEvent::Auth {
            account_id: account.clone(),
            username: username.clone(),
        },
    );
    emit(
        &mut on_progress,
        ProgressEvent::Log(format!("Authenticated as {username} ({account})")),
    );

    let q = compose_query(
        &cfg.query,
        cfg.after.as_deref(),
        cfg.before.as_deref(),
    );
    emit(
        &mut on_progress,
        ProgressEvent::Log(if q.is_empty() {
            "Export query: (all messages)".into()
        } else {
            format!("Export query: {q}")
        }),
    );

    fs::create_dir_all(&cfg.out_dir)
        .with_context(|| format!("create {}", cfg.out_dir.display()))?;
    // Warn if the output directory already contains conversation JSONL files
    // (re-pulling truncates existing conversations — there is no append/resume).
    let existing_jsonl: Vec<_> = std::fs::read_dir(&cfg.out_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.ends_with(".jsonl"))
        })
        .collect();
    if !existing_jsonl.is_empty() {
        emit(
            &mut on_progress,
            ProgressEvent::Log(format!(
                "Note: {} existing .jsonl file(s) in output directory will be overwritten. \
                 vault-pull always writes full conversations from the current query. \
                 Use a fresh output directory to keep old exports.",
                existing_jsonl.len()
            )),
        );
    }
    let attachments_dir = cfg.out_dir.join("attachments");
    if !cfg.skip_attachments {
        fs::create_dir_all(&attachments_dir)?;
    }

    let session = HttpSession::new()?;
    let mut cursor: Option<String> = None;
    let mut by_conv: BTreeMap<String, (ExportMessage, Vec<message_ir::IrMessage>)> =
        BTreeMap::new();
    // sha256 -> (source, relative path under out_dir)
    let mut assets: HashMap<String, (String, String)> = HashMap::new();
    let mut total_messages = 0u64;
    let mut seen_cursors: BTreeSet<String> = BTreeSet::new();
    const MAX_PAGES: usize = 10_000;

    loop {
        check_cancel(cfg.cancel.as_ref()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let page = session.export_messages(
            &cfg.base_url,
            &cfg.key,
            &q,
            cfg.page_limit.max(1),
            cursor.as_deref(),
            &account,
            cfg.source.as_deref(),
        )?;
        total_messages += page.messages.len() as u64;
        emit(
            &mut on_progress,
            ProgressEvent::Page {
                messages: page.messages.len(),
                total_so_far: total_messages,
            },
        );
        let page_log = match cfg.expected_messages {
            Some(n) => format!(
                "Fetched {} message(s) ({} of {})",
                page.messages.len(),
                total_messages,
                n
            ),
            None => format!(
                "Fetched {} message(s) ({} total)",
                page.messages.len(),
                total_messages
            ),
        };
        emit(&mut on_progress, ProgressEvent::Log(page_log));

        for msg in page.messages {
            if !cfg.skip_attachments {
                for att in &msg.attachments {
                    if let Some(sha) = att
                        .sha256
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        let rel = att
                            .path
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(|p| p.trim_start_matches('/').to_string())
                            .unwrap_or_else(|| format!("attachments/{sha}"));
                        // Validate the server-provided path before use.
                        if let Err(e) = safe_rel(&rel) {
                            emit(
                                &mut on_progress,
                                ProgressEvent::Log(format!(
                                    "Skipping asset {sha}: {e}"
                                )),
                            );
                            continue;
                        }
                        assets
                            .entry(sha.to_string())
                            .or_insert_with(|| (msg.source.clone(), rel));
                    }
                }
            }
            let key = conversation_key(&msg);
            let ir = to_ir_message(&msg, cfg.skip_attachments)?;
            let entry = by_conv.entry(key).or_insert_with(|| (msg.clone(), Vec::new()));
            // Keep first message as seed for conversation metadata.
            entry.1.push(ir);
        }

        match page.next_cursor {
            Some(next) if !next.is_empty() => {
                if seen_cursors.contains(&next) {
                    bail!(
                        "pagination loop detected: cursor {next} was already seen"
                    );
                }
                if seen_cursors.len() >= MAX_PAGES {
                    bail!(
                        "pagination limit reached ({MAX_PAGES} pages)"
                    );
                }
                seen_cursors.insert(next.clone());
                cursor = Some(next);
            }
            _ => break,
        }
    }

    let mut attachments_downloaded = 0u64;
    let mut attachments_skipped = 0u64;
    let mut downloaded: BTreeSet<String> = BTreeSet::new();

    if !cfg.skip_attachments {
        for (sha, (source, rel)) in &assets {
            check_cancel(cfg.cancel.as_ref()).map_err(|e| anyhow::anyhow!("{e}"))?;
            if !downloaded.insert(sha.clone()) {
                continue;
            }
            let dest = cfg.out_dir.join(rel);

            // Only skip if the file exists AND matches the expected digest.
            if dest.is_file() {
                match verify_sha256(&dest, sha) {
                    Ok(()) => {
                        attachments_skipped += 1;
                        continue;
                    }
                    Err(_) => {
                        // Hash mismatch — re-download. Write to temp file
                        // to avoid leaving a partial download in place.
                    }
                }
            }
            emit(
                &mut on_progress,
                ProgressEvent::Log(format!("Downloading asset {sha}…")),
            );
            // Download to a temp file, verify the hash, then rename into place.
            let tmp = dest.with_extension("tmp.download");
            session.download_asset(
                &cfg.base_url,
                &cfg.key,
                &account,
                source,
                sha,
                &tmp,
            )?;
            if let Err(e) = verify_sha256(&tmp, sha) {
                let _ = std::fs::remove_file(&tmp);
                bail!("downloaded asset {sha} failed hash check: {e}");
            }
            std::fs::rename(&tmp, &dest)
                .with_context(|| format!("rename {} → {}", tmp.display(), dest.display()))?;
            attachments_downloaded += 1;
        }
    }

    let mut conversations = 0u64;
    for (_key, (seed, messages)) in by_conv {
        let source = seed.source.clone();
        let doc = build_document(&source, &seed, messages);
        write_conversation_jsonl(&cfg.out_dir, &doc)?;
        conversations += 1;
    }

    let report = PullReport {
        ok: true,
        account,
        query: q,
        conversations,
        messages: total_messages,
        attachments_downloaded,
        attachments_skipped,
        out_dir: cfg.out_dir.display().to_string(),
    };
    emit(
        &mut on_progress,
        ProgressEvent::Log(format!(
            "Wrote {} conversation(s), {} message(s) → {}",
            report.conversations, report.messages, report.out_dir
        )),
    );
    emit(&mut on_progress, ProgressEvent::Done(report.clone()));
    Ok(report)
}

fn write_conversation_jsonl(out_dir: &Path, doc: &ConversationDocument) -> Result<()> {
    let stem = doc.filename_stem();
    // Disambiguate same chat across sources.
    let stem = if doc.export.source.trim().is_empty() {
        stem
    } else {
        format!("{stem}__{}", sanitize_source_suffix(&doc.export.source))
    };
    let path = out_dir.join(format!("{stem}.jsonl"));
    let mut file = File::create(&path).with_context(|| format!("create {}", path.display()))?;

    let header = message_ir::ConversationHeader::from_document(doc);
    writeln!(
        file,
        "{}",
        serde_json::to_string(&header).context("serialize conversation header")?
    )?;
    for msg in &doc.messages {
        writeln!(
            file,
            "{}",
            serde_json::to_string(msg).context("serialize message")?
        )?;
    }
    Ok(())
}

/// Reject paths that could escape the output directory (absolute paths or `..`).
fn safe_rel(rel: &str) -> Result<()> {
    let path = std::path::Path::new(rel);
    if path.is_absolute() {
        bail!("attachment path must be relative: {rel}");
    }
    for comp in path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            bail!("unsafe attachment path (contains ..): {rel}");
        }
    }
    Ok(())
}

/// Verify that a downloaded file matches the expected SHA-256 digest.
fn verify_sha256(path: &std::path::Path, expected_hex: &str) -> Result<()> {
    let expected = expected_hex.trim();
    if expected.is_empty() || expected.len() != 64 {
        bail!("invalid SHA-256 digest: {expected}");
    }
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("open for hash check: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hex::encode(hasher.finalize());
    if actual != expected {
        bail!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}",
            path.display()
        );
    }
    Ok(())
}

fn sanitize_source_suffix(source: &str) -> String {
    source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
