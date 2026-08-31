//! Page exported messages, download attachments, and write JSON Lines folders.
//!
//! JSON Lines means one JSON object per line. Message Vault is the HTTP server
//! that stores imported messages.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use message_ir::ConversationDocument;
use message_vault_io_core::{CancelFlag, check_cancel, parallel_for_each};
use serde::Serialize;
use vault_http::{auth_check as authenticate, with_retries};

use crate::http::{ExportMessage, ExportMessagesArgs, HttpSession};
use crate::project::{build_document, conversation_key, to_ir_message};

/// Default page size for GET /v1/export/messages.
pub const DEFAULT_PAGE_LIMIT: usize = 100;
/// Default number of parallel asset download workers.
pub const DEFAULT_ASSET_DOWNLOAD_WORKERS: usize = 8;
/// Extra tries for transient HTTP failures, matching the vault-push default.
const MAX_RETRIES: u32 = 3;

/// Settings for one download run (output folder, URL, search, flags).
#[derive(Debug, Clone)]
pub struct VaultPullConfig {
    pub out_dir: PathBuf,
    pub base_url: String,
    pub username: String,
    pub key: String,
    /// Free-form Fastmail-style query (may be empty).
    pub query: String,
    pub source: Option<String>,
    pub skip_attachments: bool,
    pub page_limit: usize,
    pub cancel: Option<CancelFlag>,
    /// Number of parallel asset download workers (default 8).
    pub asset_download_workers: usize,
}

/// Final summary of a download (conversations, messages, attachment counts).
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

/// Live progress sent to the CLI or desktop app during a query or download.
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

/// Send one event to the caller's progress callback when it supplied one.
fn emit(on_progress: &mut Option<&mut ProgressFn<'_>>, event: ProgressEvent) {
    if let Some(callback) = on_progress.as_mut() {
        callback(event);
    }
}

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

/// Download matching messages into `cfg.out_dir` as JSON Lines plus attachments.
///
/// JSON Lines means one JSON object per line. A local journal
/// (`.vault-pull-state.jsonl`) records which files were already downloaded so a
/// later run can skip them.
///
/// # Errors
///
/// Returns an error when the key or output folder is missing, login fails, a
/// page or download fails, or a conversation file cannot be written.
pub fn run(
    cfg: &VaultPullConfig,
    mut on_progress: Option<&mut ProgressFn<'_>>,
) -> Result<PullReport> {
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

    let q = cfg.query.trim().to_string();
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

    // Load the local skip log so a later run does not re-download files already on disk.
    let journal_path = crate::journal::journal_path(&cfg.out_dir);
    let journal_state = crate::journal::load(&journal_path, &cfg.base_url, &username)?;

    if journal_state.backup_complete {
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
        check_cancel(cfg.cancel.as_ref())?;
        let page = with_retries(MAX_RETRIES, || {
            crate::http::export_messages(
                &session,
                ExportMessagesArgs {
                    base_url: &cfg.base_url,
                    key: &cfg.key,
                    q: &q,
                    limit: cfg.page_limit.max(1),
                    cursor: cursor.as_deref(),
                    account: &account,
                    source: cfg.source.as_deref(),
                },
            )
        })?;
        total_messages += page.messages.len() as u64;
        emit(
            &mut on_progress,
            ProgressEvent::Page {
                messages: page.messages.len(),
                total_so_far: total_messages,
            },
        );

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
        let to_download = assets_needing_download(&assets, &journal_state.assets, &cfg.out_dir);
        let skipped_by_journal = assets.len() as u64 - to_download.len() as u64;

        if !to_download.is_empty() {
            emit(
                &mut on_progress,
                ProgressEvent::Log(format!(
                    "Downloading {} unique asset(s) with {} worker(s) ({} skipped from journal)…",
                    to_download.len(),
                    cfg.asset_download_workers,
                    skipped_by_journal
                )),
            );
            let dl_stats = download_assets_parallel(DownloadAssetsParallelArgs {
                session: &session,
                base_url: &cfg.base_url,
                key: &cfg.key,
                account: &account,
                assets: &to_download,
                out_dir: &cfg.out_dir,
                workers: cfg.asset_download_workers,
                cancel: cfg.cancel.as_ref(),
            })?;
            attachments_downloaded = dl_stats.downloaded;
            attachments_skipped = dl_stats.skipped + skipped_by_journal;

            // Journal each successfully present asset (downloaded or already on disk)
            // so a resume skips it.
            for sha in to_download.keys() {
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
        let mut doc = build_document(&source, &seed, messages);
        // Disambiguate same chat across sources.
        if !doc.export.source.trim().is_empty() {
            doc.packaging_stem_suffix =
                Some(format!("__{}", sanitize_source_suffix(&doc.export.source)));
        }
        write_conversation_jsonl(&cfg.out_dir, &doc)?;
        conversations += 1;
    }

    // Record that this download finished, then rewrite the journal in its shortest form.
    let event = crate::journal::PullJournalEvent::BackupComplete {
        url: cfg.base_url.clone(),
        username: username.clone(),
        conversations,
        messages: total_messages,
        assets: attachments_downloaded + attachments_skipped,
    };
    crate::journal::append(&journal_path, &event)?;
    // Rewrite the journal in its shortest form now that the run finished cleanly.
    // Every asset this run saw is on disk: it was downloaded above, or an earlier
    // run had already fetched it.
    let mut recorded_assets = journal_state.assets;
    recorded_assets.extend(assets.into_keys());
    let final_state = crate::journal::PullJournalState {
        assets: recorded_assets,
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

/// Attachments whose SHA-256 fingerprint is not already on disk from a prior run.
///
/// SHA-256 is a short hex fingerprint of the file bytes. The journal lists
/// fingerprints already downloaded; those files are skipped when they still exist.
fn assets_needing_download(
    assets: &HashMap<String, (String, String)>,
    journal_assets: &HashSet<String>,
    out_dir: &Path,
) -> HashMap<String, (String, String)> {
    let mut to_download = HashMap::new();
    for (sha, entry) in assets {
        let (_source, rel) = entry;
        if journal_assets.contains(sha) && out_dir.join(rel).is_file() {
            continue;
        }
        to_download.insert(sha.clone(), entry.clone());
    }
    to_download
}

/// Download unique attachments in parallel using work-stealing workers.
///
/// Same pattern as vault-push `upload_assets`: jobs are collected, then
/// [`parallel_for_each`] runs them on `asset_download_workers` threads.
/// Files already on disk are skipped (counted as `skipped`); each download
/// retries transient HTTP failures like push does.
///
/// # Errors
///
/// Returns an error when a download fails after retries, a dest path cannot be
/// created, or cancel is requested.
struct DownloadAssetsParallelArgs<'a> {
    session: &'a crate::http::HttpSession,
    base_url: &'a str,
    key: &'a str,
    account: &'a str,
    assets: &'a HashMap<String, (String, String)>, // sha256 -> (source, rel_path)
    out_dir: &'a Path,
    workers: usize,
    cancel: Option<&'a CancelFlag>,
}

fn download_assets_parallel(args: DownloadAssetsParallelArgs<'_>) -> Result<AssetDownloadStats> {
    let DownloadAssetsParallelArgs {
        session,
        base_url,
        key,
        account,
        assets,
        out_dir,
        workers,
        cancel,
    } = args;
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

    let results = parallel_for_each(&jobs, workers, cancel, |job| {
        with_retries(MAX_RETRIES, || {
            crate::http::download_asset(
                session,
                base_url,
                key,
                account,
                &job.source,
                &job.sha256,
                &job.dest,
            )?;
            let meta = fs::metadata(&job.dest)
                .with_context(|| format!("stat after download {}", job.dest.display()))?;
            Ok(meta.len())
        })
        .map_err(|e| e.to_string())
    });

    for result in results {
        match result {
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

/// Format a byte count as GB, MB, KB, or B with one decimal place when scaled.
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

/// Write one conversation as a JSON Lines file (header, then one message per line).
///
/// The stem comes from [`ConversationDocument::filename_stem`], which appends
/// [`ConversationDocument::packaging_stem_suffix`] (the sanitized source, set
/// by the caller). The write is atomic: ir-format writes a `.tmp` sibling and
/// renames it, so a crash never leaves a truncated conversation file.
///
/// # Errors
///
/// Returns an error when the file cannot be created, serialized, or renamed.
fn write_conversation_jsonl(out_dir: &Path, doc: &ConversationDocument) -> Result<()> {
    let path = out_dir.join(format!("{}.jsonl", doc.filename_stem()));
    message_ir_format::write_conversation_jsonl_to(&path, doc)
}

/// Keep letters, digits, `-`, and `_`; replace every other character with `_`.
fn sanitize_source_suffix(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for c in source.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}
