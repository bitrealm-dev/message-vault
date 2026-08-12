//! Page export messages, download assets, write message-ir JSONL folders.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, bail};
use message_ir::ConversationDocument;
use message_vault_io_core::{CancelFlag, check_cancel};
use serde::Serialize;
use vault_push::authenticate;

use crate::http::{ExportMessage, HttpSession};
use crate::project::{build_document, conversation_key, to_ir_message};

pub const DEFAULT_PAGE_LIMIT: usize = 100;
/// Default number of parallel asset download workers.
pub const DEFAULT_ASSET_DOWNLOAD_WORKERS: usize = 8;

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
    /// Number of parallel asset download workers (default 8).
    pub asset_download_workers: usize,
    /// Ignore the journal and re-download everything.
    pub force: bool,
    /// Path to the pull journal file. Defaults to out_dir/.vault-pull-state.jsonl.
    pub journal_path: Option<PathBuf>,
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
    let username = auth.username.clone().unwrap_or_else(|| account.clone());
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

    let q = compose_query(&cfg.query, cfg.after.as_deref(), cfg.before.as_deref());
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
            Some(next) if !next.is_empty() => cursor = Some(next),
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

pub fn run(
    cfg: &VaultPullConfig,
    mut on_progress: Option<&mut ProgressFn<'_>>,
) -> Result<PullReport> {
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
    let username = auth.username.clone().unwrap_or_else(|| account.clone());
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

    let q = compose_query(&cfg.query, cfg.after.as_deref(), cfg.before.as_deref());
    emit(
        &mut on_progress,
        ProgressEvent::Log(if q.is_empty() {
            "Backup query: (all messages)".into()
        } else {
            format!("Backup query: {q}")
        }),
    );

    fs::create_dir_all(&cfg.out_dir)
        .with_context(|| format!("create {}", cfg.out_dir.display()))?;
    let attachments_dir = cfg.out_dir.join("attachments");
    if !cfg.skip_attachments {
        fs::create_dir_all(&attachments_dir)?;
    }

    // --- resume journal ---
    let journal_path = cfg
        .journal_path
        .clone()
        .unwrap_or_else(|| crate::journal::journal_path(&cfg.out_dir));
    let journal_state = if cfg.force {
        crate::journal::PullJournalState::default()
    } else {
        crate::journal::load(&journal_path, &cfg.base_url, &username)?
    };

    if journal_state.backup_complete && !cfg.force {
        emit(
            &mut on_progress,
            ProgressEvent::Log(
                "Previous backup completed successfully. Running to check for new messages…".into(),
            ),
        );
    }

    let session = HttpSession::new()?;
    let mut cursor: Option<String> = None;
    let mut by_conv: BTreeMap<String, (ExportMessage, Vec<message_ir::IrMessage>)> =
        BTreeMap::new();
    // sha256 -> (source, relative path under out_dir)
    let mut assets: HashMap<String, (String, String)> = HashMap::new();
    let mut total_messages = 0u64;

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
                        assets
                            .entry(sha.to_string())
                            .or_insert_with(|| (msg.source.clone(), rel));
                    }
                }
            }
            let key = conversation_key(&msg);
            let ir = to_ir_message(&msg, cfg.skip_attachments)?;
            let entry = by_conv
                .entry(key)
                .or_insert_with(|| (msg.clone(), Vec::new()));
            // Keep first message as seed for conversation metadata.
            entry.1.push(ir);
        }

        match page.next_cursor {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => break,
        }
    }

    let mut attachments_downloaded = 0u64;
    let mut attachments_skipped = 0u64;

    if !cfg.skip_attachments {
        // Filter out assets already in the journal that exist on disk.
        let total_assets = assets.len() as u64;
        let to_download: HashMap<String, (String, String)> = assets
            .iter()
            .filter(|(sha, (_source, rel))| {
                if journal_state.assets.contains(*sha) {
                    let dest = cfg.out_dir.join(rel);
                    if dest.is_file() {
                        return false; // skip: journaled + on disk
                    }
                }
                true
            })
            .map(|(sha, tuple)| (sha.clone(), tuple.clone()))
            .collect();
        let skipped_by_journal = total_assets - to_download.len() as u64;
        let assets = to_download;

        if !assets.is_empty() {
            emit(
                &mut on_progress,
                ProgressEvent::Log(format!(
                    "Downloading {} unique asset(s) with {} worker(s) ({} skipped from journal)…",
                    assets.len(),
                    cfg.asset_download_workers,
                    skipped_by_journal
                )),
            );
            let dl_stats = download_assets_parallel(
                &session,
                &cfg.base_url,
                &cfg.key,
                &account,
                &assets,
                &cfg.out_dir,
                cfg.asset_download_workers,
                cfg.cancel.as_ref(),
            )?;
            attachments_downloaded = dl_stats.downloaded;
            attachments_skipped = dl_stats.skipped + skipped_by_journal;

            // Journal each successfully present asset (downloaded or already on disk)
            // so a resume skips it.
            for (sha, (_source, _rel)) in &assets {
                if !journal_state.assets.contains(sha) {
                    let event = crate::journal::PullJournalEvent::AssetOk {
                        url: cfg.base_url.clone(),
                        username: username.clone(),
                        sha256: sha.clone(),
                        path: String::new(),
                        size_bytes: 0,
                    };
                    let _ = crate::journal::append(&journal_path, &event);
                }
            }

            emit(
                &mut on_progress,
                ProgressEvent::Log(format!(
                    "Assets: {} downloaded, {} skipped ({} total bytes)",
                    attachments_downloaded,
                    attachments_skipped,
                    format_bytes_human(dl_stats.bytes)
                )),
            );
        } else {
            attachments_skipped = skipped_by_journal;
        }
    }

    let mut conversations = 0u64;
    for (_key, (seed, messages)) in by_conv {
        let source = seed.source.clone();
        let doc = build_document(&source, &seed, messages);
        write_conversation_jsonl(&cfg.out_dir, &doc)?;
        conversations += 1;
    }

    // --- journal completion ---
    let event = crate::journal::PullJournalEvent::BackupComplete {
        url: cfg.base_url.clone(),
        username: username.clone(),
        conversations,
        messages: total_messages,
        assets: attachments_downloaded + attachments_skipped,
    };
    crate::journal::append(&journal_path, &event)?;
    // Compact after clean run
    let final_state = crate::journal::PullJournalState {
        assets: {
            let mut s = journal_state.assets.clone();
            s.extend(assets.keys().cloned());
            s
        },
        backup_complete: true,
    };
    let _ = crate::journal::compact(&journal_path, &cfg.base_url, &username, &final_state);

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

struct AssetDownloadJob {
    sha256: String,
    source: String,
    dest: PathBuf,
}

#[derive(Default)]
struct AssetDownloadStats {
    bytes: u64,
    downloaded: u64,
    skipped: u64,
}

/// Download unique assets in parallel using work-stealing workers.
///
/// Mirrors vault-push's `upload_assets` pattern: jobs are collected, then
/// `asset_download_workers` threads pull from a shared `AtomicUsize` counter.
/// Assets already on disk are skipped (counted as `skipped`).
fn download_assets_parallel(
    session: &crate::http::HttpSession,
    base_url: &str,
    key: &str,
    account: &str,
    assets: &HashMap<String, (String, String)>, // sha256 -> (source, rel_path)
    out_dir: &Path,
    workers: usize,
    cancel: Option<&CancelFlag>,
) -> Result<AssetDownloadStats> {
    let mut jobs: Vec<AssetDownloadJob> = Vec::with_capacity(assets.len());
    let mut stats = AssetDownloadStats::default();

    for (sha256, (source, rel)) in assets {
        let dest = out_dir.join(rel);
        if dest.is_file() {
            let meta = fs::metadata(&dest).with_context(|| format!("stat {}", dest.display()))?;
            stats.bytes = stats.bytes.saturating_add(meta.len());
            stats.skipped += 1;
            continue;
        }
        jobs.push(AssetDownloadJob {
            sha256: sha256.clone(),
            source: source.clone(),
            dest,
        });
    }

    if jobs.is_empty() {
        return Ok(stats);
    }

    let worker_count = workers.max(1).min(jobs.len());
    let next_job = AtomicUsize::new(0);
    let results = Mutex::new(
        std::iter::repeat_with(|| None)
            .take(jobs.len())
            .collect::<Vec<Option<Result<u64, String>>>>(),
    );

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_job.fetch_add(1, Ordering::Relaxed);
                    if index >= jobs.len() {
                        break;
                    }
                    let job = &jobs[index];
                    let result = (|| -> Result<u64> {
                        check_cancel(cancel).map_err(|e| anyhow::anyhow!("{e}"))?;
                        session.download_asset(
                            base_url,
                            key,
                            account,
                            &job.source,
                            &job.sha256,
                            &job.dest,
                        )?;
                        let meta = fs::metadata(&job.dest).with_context(|| {
                            format!("stat after download {}", job.dest.display())
                        })?;
                        Ok(meta.len())
                    })()
                    .map_err(|e| e.to_string());
                    results.lock().expect("asset result mutex poisoned")[index] = Some(result);
                }
            });
        }
    });

    let mut results = results.into_inner().expect("asset result mutex poisoned");
    for result in results.drain(..) {
        match result.expect("every asset job has a result") {
            Ok(bytes) => {
                stats.bytes = stats.bytes.saturating_add(bytes);
                stats.downloaded += 1;
            }
            Err(error) => {
                bail!("asset download failed: {error}");
            }
        }
    }
    Ok(stats)
}

fn format_bytes_human(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
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
