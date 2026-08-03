//! Folder push: stream message-ir JSONL, upload assets by digest, import batches.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use message_ir::{
    ConversationHeader,
};
use message_ir_format::{
    read_conversation_jsonl,
};
use message_vault_io_core::{CancelFlag, check_cancel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::http::{self, AssetPutRequest, AuthInfo, HttpSession};
use crate::journal::{self, JournalEvent, JournalMessage, JournalState};
use crate::project;

/// Default messages per HTTP import request.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;
/// Soft target for one import NDJSON body. Kept well under Cloudflare's 100 MB
/// proxy upload limit (Free/Pro) so large threads are split into many requests.
pub const MAX_IMPORT_BODY_BYTES: usize = 8 * 1024 * 1024;
/// Hard ceiling for any single non-chunked request body through a typical
/// Cloudflare proxy. Assets larger than this use multipart upload.
pub const MAX_PROXY_BODY_BYTES: usize = 90 * 1024 * 1024;
/// Default max size of one attachment (must match vault `server.asset_max_bytes`).
pub const DEFAULT_ASSET_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Default number of simultaneous attachment uploads.
pub const DEFAULT_ASSET_UPLOAD_WORKERS: usize = 4;

#[derive(Debug, Clone)]
pub struct VaultPushConfig {
    pub input: PathBuf,
    pub base_url: String,
    pub username: String,
    pub key: String,
    /// "append" (default) or "replace"
    pub mode: String,
    pub continue_on_error: bool,
    pub force: bool,
    /// Import message text only; do not upload or reference attachments.
    pub skip_attachments: bool,
    /// When true, re-hash attachment files and compare to export `digest_sha256`.
    /// Default false: trust export digests (server still verifies on store).
    pub verify_digests: bool,
    pub max_retries: u32,
    pub batch_size: usize,
    /// Maximum simultaneous attachment uploads. Message import requests remain serialized.
    pub asset_upload_workers: usize,
    /// Assets larger than this (bytes) use vault multipart upload instead of a single PUT.
    /// Defaults to [`MAX_PROXY_BODY_BYTES`]; tests may lower it to force the multipart path.
    pub asset_multipart_threshold: usize,
    /// Refuse attachments larger than this even with multipart (match vault `asset_max_bytes`).
    pub asset_max_bytes: u64,
    pub report_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub journal_path: Option<PathBuf>,
    pub cancel: Option<CancelFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResult {
    pub file: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub messages: u64,
    pub attachments: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<UploadProfile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UploadProfile {
    pub read_ms: u64,
    pub attachment_scan_hash_ms: u64,
    pub asset_upload_ms: u64,
    pub message_import_ms: u64,
    pub total_ms: u64,
    pub unique_assets: u64,
    pub asset_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushReport {
    pub ok: bool,
    pub account: String,
    pub username: String,
    pub mode: String,
    pub started_at: String,
    pub finished_at: String,
    /// Wall-clock duration of the full push (auth through last import).
    pub elapsed_ms: u64,
    pub conversations_total: u64,
    pub conversations_ok: u64,
    pub conversations_failed: u64,
    pub conversations_skipped: u64,
    pub messages: u64,
    pub assets_uploaded: u64,
    pub assets_skipped: u64,
    pub results: Vec<FileResult>,
}

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Log(String),
    Auth {
        account_id: String,
        username: String,
    },
    FileStart {
        index: usize,
        total: usize,
        file: String,
    },
    FileDone {
        file: String,
        status: String,
    },
    Finished(PushReport),
}

pub type ProgressFn<'a> = dyn FnMut(ProgressEvent) + Send + 'a;

struct LogWriter {
    file: File,
}

impl LogWriter {
    fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("open log {}", path.display()))?;
        Ok(Self { file })
    }

    fn line(&mut self, msg: &str) {
        let _ = writeln!(self.file, "{msg}");
        let _ = self.file.flush();
    }
}

fn now_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Human-readable duration for summary lines, e.g. `34m12s` or `1h02m03s`.
pub fn format_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{hours}h{minutes:02}m{seconds:02}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds:02}s")
    } else if total_secs > 0 || ms == 0 {
        format!("{seconds}s")
    } else {
        format!("{ms}ms")
    }
}

fn is_push_artifact(name: &str) -> bool {
    name.eq_ignore_ascii_case(journal::JOURNAL_NAME)
        || name.eq_ignore_ascii_case(journal::REPORT_NAME)
        || name.eq_ignore_ascii_case(journal::LOG_NAME)
        || name.ends_with(".jsonl.tmp")
        || name.starts_with('.')
}

fn list_jsonl_files(dir: &Path, exclude: &[&Path]) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            if exclude.iter().any(|x| *x == p) {
                return false;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                return false;
            };
            if is_push_artifact(name) {
                return false;
            }
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        })
        .collect();
    paths.sort();
    Ok(paths)
}

fn normalize_digest_sha256(digest: &str) -> Result<String> {
    let s = digest.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid sha256 digest (expected 64 hex digits)");
    }
    Ok(s)
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 64];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn resolve_attachment(export_root: &Path, rel: &str) -> Option<PathBuf> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let under = export_root.join(candidate);
    under.is_file().then_some(under)
}

fn safe_rel(rel: &str) -> Result<()> {
    let path = Path::new(rel);
    if path.is_absolute() {
        bail!("attachment path must be relative: {rel}");
    }
    for comp in path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            bail!("unsafe attachment path: {rel}");
        }
    }
    Ok(())
}

/// Authenticate without importing.
pub fn authenticate(base_url: &str, key: &str, username: &str) -> Result<AuthInfo> {
    http::auth_check(base_url, key, username)
}

/// Peek `export.source` from the first JSONL header in a directory.
pub fn detect_source(input: &Path) -> Result<Option<String>> {
    let dir = if input.is_file() {
        input.parent().unwrap_or(input)
    } else {
        input
    };
    let files = list_jsonl_files(dir, &[])?;
    let Some(path) = files.first() else {
        return Ok(None);
    };
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let header_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty JSONL"))??;
    let header: ConversationHeader = serde_json::from_str(&header_line)?;
    Ok(Some(project::validate_header(&header)?))
}

struct RunSetup {
    input: PathBuf,
    report_path: PathBuf,
    journal_path: PathBuf,
    log: LogWriter,
    url: String,
    username: String,
    http: HttpSession,
    auth: AuthInfo,
    journal: JournalState,
    files: Vec<PathBuf>,
    total: usize,
    batch_size: usize,
}

fn prepare_run_setup(
    cfg: &VaultPushConfig,
    progress: &mut Option<&mut ProgressFn<'_>>,
) -> Result<RunSetup> {
    let input = if cfg.input.is_file() {
        cfg.input
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        cfg.input.clone()
    };
    if !input.is_dir() {
        bail!("input directory does not exist: {}", input.display());
    }

    let report_path = cfg
        .report_path
        .clone()
        .unwrap_or_else(|| input.join(journal::REPORT_NAME));
    let log_path = cfg
        .log_path
        .clone()
        .unwrap_or_else(|| input.join(journal::LOG_NAME));
    let journal_path = cfg
        .journal_path
        .clone()
        .unwrap_or_else(|| journal::journal_path(&input));

    let mut log = LogWriter::open(&log_path)?;
    let url = cfg.base_url.trim_end_matches('/').to_string();
    let username = cfg.username.trim().to_string();
    let http = HttpSession::new()?;

    check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
    let auth = http.auth_check(&url, &cfg.key, &username)?;
    // Token binds the account; Username field is optional.
    let username = auth
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(auth.account_id.as_str())
        .to_string();
    let account_label = username.clone();
    log.line(&format!(
        "authenticated username={username} account={}",
        auth.account_id
    ));
    if let Some(cb) = progress.as_mut() {
        cb(ProgressEvent::Auth {
            account_id: auth.account_id.clone(),
            username: account_label.clone(),
        });
        cb(ProgressEvent::Log(format!(
            "Authenticated as {account_label}"
        )));
    }
    if cfg.skip_attachments {
        log.line("skip_attachments=true (text-only import)");
        if let Some(cb) = progress.as_mut() {
            cb(ProgressEvent::Log(
                "Skipping attachments (text-only import)".into(),
            ));
        }
    }

    let journal = if cfg.force || cfg.mode == "replace" {
        JournalState::default()
    } else {
        journal::load(&journal_path, &url, &username)?
    };

    let files = list_jsonl_files(&input, &[&journal_path, &report_path, &log_path])?;
    if files.is_empty() {
        bail!(
            "no .jsonl files under {} (export with JSONL in the Export tab first)",
            input.display()
        );
    }

    Ok(RunSetup {
        input,
        report_path,
        journal_path,
        log,
        url,
        username,
        http,
        auth,
        journal,
        total: files.len(),
        files,
        batch_size: cfg.batch_size.max(1),
    })
}

struct FinishRunArgs<'a> {
    cfg: &'a VaultPushConfig,
    run_started: Instant,
    started_at: String,
    report_path: PathBuf,
    auth: AuthInfo,
    url: String,
    username: String,
    journal_path: PathBuf,
    journal: JournalState,
    total: usize,
    results: Vec<Option<FileResult>>,
    assets_uploaded: u64,
    assets_skipped: u64,
    aborted: bool,
}

fn finish_run(
    args: FinishRunArgs<'_>,
    progress: &mut Option<&mut ProgressFn<'_>>,
    log: &mut LogWriter,
) -> Result<PushReport> {
    let FinishRunArgs {
        cfg,
        run_started,
        started_at,
        report_path,
        auth,
        url,
        username,
        journal_path,
        journal,
        total,
        results,
        assets_uploaded,
        assets_skipped,
        aborted,
    } = args;

    let results: Vec<FileResult> = results.into_iter().flatten().collect();
    let ok_n = results
        .iter()
        .filter(|result| result.status == "ok")
        .count() as u64;
    let fail_n = results
        .iter()
        .filter(|result| result.status == "failed")
        .count() as u64;
    let skip_n = results
        .iter()
        .filter(|result| result.status == "skipped")
        .count() as u64;
    let messages = results
        .iter()
        .filter(|result| result.status == "ok")
        .map(|result| result.messages)
        .sum();
    if fail_n == 0 && !aborted {
        let _ = journal::compact(&journal_path, &url, &username, &journal);
    }

    let elapsed = elapsed_ms(run_started);
    let report = PushReport {
        ok: fail_n == 0 && !aborted,
        account: auth.account_id,
        username,
        mode: cfg.mode.clone(),
        started_at,
        finished_at: now_stamp(),
        elapsed_ms: elapsed,
        conversations_total: total as u64,
        conversations_ok: ok_n,
        conversations_failed: fail_n,
        conversations_skipped: skip_n,
        messages,
        assets_uploaded,
        assets_skipped,
        results,
    };
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).context("serialize report")?,
    )
    .with_context(|| format!("write report {}", report_path.display()))?;
    log.line(&format!(
        "finished ok={} conversations_ok={ok_n} failed={fail_n} skipped={skip_n} messages={messages} \
         elapsed_ms={elapsed} ({})",
        report.ok,
        format_duration_ms(elapsed)
    ));
    if let Some(cb) = progress.as_mut() {
        cb(ProgressEvent::Finished(report.clone()));
    }
    Ok(report)
}

/// Push every `.jsonl` conversation under `cfg.input`.
pub fn run(cfg: &VaultPushConfig, mut progress: Option<&mut ProgressFn<'_>>) -> Result<PushReport> {
    let run_started = Instant::now();
    let started_at = now_stamp();
    let RunSetup {
        input,
        report_path,
        journal_path,
        mut log,
        url,
        username,
        http,
        auth,
        mut journal,
        files,
        total,
        batch_size,
    } = prepare_run_setup(cfg, &mut progress)?;

    let mut results: Vec<Option<FileResult>> = vec![None; total];
    let mut assets_uploaded = 0u64;
    let mut assets_skipped = 0u64;
    let mut first_import = true;
    let mut aborted = false;
    let mut trackers: Vec<Option<FileTracker>> =
        std::iter::repeat_with(|| None).take(total).collect();
    let mut pending: Option<ImportBatch> = None;
    let mut inflight: Option<InFlightImport> = None;

    for (idx, path) in files.iter().enumerate() {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        if let Some(cb) = progress.as_mut() {
            cb(ProgressEvent::FileStart {
                index: idx + 1,
                total,
                file: name.clone(),
            });
        }

        if cfg.mode == "append" && !cfg.force && journal.files.contains(&name) {
            let msg = format!(
                "PROGRESS {}/{total} skip {name} (already imported)",
                idx + 1
            );
            log.line(&msg);
            if let Some(cb) = progress.as_mut() {
                cb(ProgressEvent::Log(msg));
                cb(ProgressEvent::FileDone {
                    file: name.clone(),
                    status: "skipped".into(),
                });
            }
            results[idx] = Some(FileResult {
                file: name,
                status: "skipped".into(),
                error: None,
                messages: 0,
                attachments: 0,
                profile: None,
            });
            continue;
        }

        let prepared = prepare_file(PrepareFileArgs {
            input: &input,
            path,
            name: &name,
            cfg,
            http: &http,
            url: &url,
            username: &username,
            journal: &mut journal,
            journal_path: &journal_path,
            batch_size,
            assets_uploaded: &mut assets_uploaded,
            assets_skipped: &mut assets_skipped,
            log: &mut log,
        });
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(e) => {
                if !cfg.continue_on_error && (pending.is_some() || inflight.is_some()) {
                    let request_ok = flush_import_pipeline(FlushImportPipeline {
                        cfg,
                        http: &http,
                        url: &url,
                        username: &username,
                        pending: &mut pending,
                        inflight: &mut inflight,
                        first_import: &mut first_import,
                        trackers: &mut trackers,
                        journal: &mut journal,
                        journal_path: &journal_path,
                        log: &mut log,
                        progress: &mut progress,
                        results: &mut results,
                        total,
                        wait: true,
                    })?;
                    if !request_ok {
                        aborted = true;
                        break;
                    }
                }
                let err = e.to_string();
                record_file_failure(RecordFileFailure {
                    index: idx,
                    total,
                    name: &name,
                    error: &err,
                    source: "",
                    url: &url,
                    username: &username,
                    journal_path: &journal_path,
                    log: &mut log,
                    progress: &mut progress,
                    results: &mut results,
                });
                if !cfg.continue_on_error {
                    aborted = true;
                    break;
                }
                continue;
            }
        };

        if pending
            .as_ref()
            .is_some_and(|batch| batch.source != prepared.source)
        {
            let request_ok = flush_import_pipeline(FlushImportPipeline {
                cfg,
                http: &http,
                url: &url,
                username: &username,
                pending: &mut pending,
                inflight: &mut inflight,
                first_import: &mut first_import,
                trackers: &mut trackers,
                journal: &mut journal,
                journal_path: &journal_path,
                log: &mut log,
                progress: &mut progress,
                results: &mut results,
                total,
                wait: !cfg.continue_on_error,
            })?;
            if !request_ok && !cfg.continue_on_error {
                aborted = true;
                break;
            }
        }

        let message_count = prepared
            .chunks
            .iter()
            .map(|chunk| chunk.messages.len())
            .sum();
        trackers[idx] = Some(FileTracker {
            name: name.clone(),
            source: prepared.source.clone(),
            attachments: prepared.attachments,
            profile: prepared.profile,
            total_started: prepared.total_started,
            outstanding_messages: message_count,
            successful_messages: 0,
            queue_complete: false,
            failed: None,
            done: false,
        });

        for chunk in prepared.chunks {
            let must_flush = pending.as_ref().is_some_and(|batch| {
                should_flush_before_chunk(batch, &chunk, batch_size, MAX_IMPORT_BODY_BYTES)
            });
            if must_flush {
                let request_ok = flush_import_pipeline(FlushImportPipeline {
                    cfg,
                    http: &http,
                    url: &url,
                    username: &username,
                    pending: &mut pending,
                    inflight: &mut inflight,
                    first_import: &mut first_import,
                    trackers: &mut trackers,
                    journal: &mut journal,
                    journal_path: &journal_path,
                    log: &mut log,
                    progress: &mut progress,
                    results: &mut results,
                    total,
                    wait: !cfg.continue_on_error,
                })?;
                if !request_ok && !cfg.continue_on_error {
                    aborted = true;
                    break;
                }
                if trackers[idx]
                    .as_ref()
                    .is_some_and(|tracker| tracker.failed.is_some())
                {
                    break;
                }
            }

            let batch = pending.get_or_insert_with(|| ImportBatch::new(&prepared.source));
            batch.push(idx, chunk);
            if batch.messages.len() >= batch_size || batch.body.len() >= MAX_IMPORT_BODY_BYTES {
                let request_ok = flush_import_pipeline(FlushImportPipeline {
                    cfg,
                    http: &http,
                    url: &url,
                    username: &username,
                    pending: &mut pending,
                    inflight: &mut inflight,
                    first_import: &mut first_import,
                    trackers: &mut trackers,
                    journal: &mut journal,
                    journal_path: &journal_path,
                    log: &mut log,
                    progress: &mut progress,
                    results: &mut results,
                    total,
                    wait: !cfg.continue_on_error,
                })?;
                if !request_ok && !cfg.continue_on_error {
                    aborted = true;
                    break;
                }
                if trackers[idx]
                    .as_ref()
                    .is_some_and(|tracker| tracker.failed.is_some())
                {
                    break;
                }
            }
        }
        if aborted {
            break;
        }
        if let Some(tracker) = trackers[idx].as_mut() {
            tracker.queue_complete = true;
        }
        // File may still have outstanding messages in `pending` / `inflight`; finish runs
        // when those imports complete (overlapped with the next prepare_file).
        finish_file_if_ready(FinishFile {
            index: idx,
            total,
            trackers: &mut trackers,
            journal: &mut journal,
            journal_path: &journal_path,
            url: &url,
            username: &username,
            log: &mut log,
            progress: &mut progress,
            results: &mut results,
        })?;
    }

    if !aborted {
        let request_ok = flush_import_pipeline(FlushImportPipeline {
            cfg,
            http: &http,
            url: &url,
            username: &username,
            pending: &mut pending,
            inflight: &mut inflight,
            first_import: &mut first_import,
            trackers: &mut trackers,
            journal: &mut journal,
            journal_path: &journal_path,
            log: &mut log,
            progress: &mut progress,
            results: &mut results,
            total,
            wait: true,
        })?;
        if !request_ok && !cfg.continue_on_error {
            aborted = true;
        }
    } else {
        // Best-effort drain so journal/trackers stay consistent on abort.
        let _ = join_inflight_import(JoinInflightImport {
            inflight: &mut inflight,
            first_import: &mut first_import,
            trackers: &mut trackers,
            journal: &mut journal,
            journal_path: &journal_path,
            url: &url,
            username: &username,
            log: &mut log,
            progress: &mut progress,
            results: &mut results,
            total,
        });
    }

    finish_run(
        FinishRunArgs {
            cfg,
            run_started,
            started_at,
            report_path,
            auth,
            url,
            username,
            journal_path,
            journal,
            total,
            results,
            assets_uploaded,
            assets_skipped,
            aborted,
        },
        &mut progress,
        &mut log,
    )
}

struct PrepareFileArgs<'a> {
    input: &'a Path,
    path: &'a Path,
    name: &'a str,
    cfg: &'a VaultPushConfig,
    http: &'a HttpSession,
    url: &'a str,
    username: &'a str,
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    batch_size: usize,
    assets_uploaded: &'a mut u64,
    assets_skipped: &'a mut u64,
    log: &'a mut LogWriter,
}

struct PreparedFile {
    source: String,
    chunks: Vec<ImportChunk>,
    attachments: u64,
    profile: UploadProfile,
    total_started: Instant,
}

struct ImportChunk {
    body: Vec<u8>,
    messages: Vec<JournalMessage>,
}

fn prepare_file(args: PrepareFileArgs<'_>) -> Result<PreparedFile> {
    let total_started = Instant::now();
    let PrepareFileArgs {
        input,
        path,
        name,
        cfg,
        http,
        url,
        username,
        journal,
        journal_path,
        batch_size,
        assets_uploaded,
        assets_skipped,
        log,
    } = args;

    let read_started = Instant::now();
    let doc = read_conversation_jsonl(path)?;
    let read_ms = elapsed_ms(read_started);
    let header = ConversationHeader::from_document(&doc);
    let source = project::validate_header(&header)?;
    let messages = &doc.messages;

    let mut per_message_digests: Vec<Vec<(usize, String)>> = Vec::with_capacity(messages.len());
    let mut attachment_count = 0u64;
    let mut profile = UploadProfile {
        read_ms,
        ..UploadProfile::default()
    };
    let attachment_scan_hash_started = Instant::now();

    if cfg.skip_attachments {
        for msg in messages {
            let n = msg.attachments.len() as u64;
            attachment_count += n;
            *assets_skipped += n;
            per_message_digests.push(Vec::new());
        }
        profile.attachment_scan_hash_ms = elapsed_ms(attachment_scan_hash_started);
    } else {
        // digest -> (rel path, mime)
        let mut unique: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();

        for msg in messages {
            let mut digests = Vec::new();
            for (att_i, att) in msg.attachments.iter().enumerate() {
                attachment_count += 1;
                let Some(rel) = att.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                    bail!("{name}: attachment {att_i} has no path");
                };
                safe_rel(rel)?;
                let abs = resolve_attachment(input, rel)
                    .ok_or_else(|| anyhow::anyhow!("{name}: missing attachment {rel}"))?;
                let digest = match att
                    .digest_sha256
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    Some(d) => {
                        let claimed = normalize_digest_sha256(d).with_context(|| {
                            format!("{name}: invalid digest_sha256 for {rel}")
                        })?;
                        if cfg.verify_digests {
                            let actual = hash_file(&abs)?;
                            if actual != claimed {
                                bail!("{name}: sha256 mismatch for {rel}");
                            }
                        }
                        claimed
                    }
                    None => hash_file(&abs)?,
                };
                unique
                    .entry(digest.clone())
                    .or_insert_with(|| (rel.to_string(), att.mime_type.clone()));
                digests.push((att_i, digest));
            }
            per_message_digests.push(digests);
        }

        profile.attachment_scan_hash_ms = elapsed_ms(attachment_scan_hash_started);
        profile.unique_assets = u64::try_from(unique.len()).unwrap_or(u64::MAX);
        let asset_upload_started = Instant::now();
        let upload_stats = upload_assets(UploadAssets {
            input,
            name,
            cfg,
            http,
            url,
            username,
            source: &source,
            unique: &unique,
            journal,
            journal_path,
            assets_uploaded,
            assets_skipped,
            log,
        })?;
        profile.asset_upload_ms = elapsed_ms(asset_upload_started);
        profile.asset_bytes = upload_stats.bytes;
    }

    let header_line = project::document_header_line(&doc)?;
    let mut chunks = Vec::new();
    let mut chunk_body = header_line.clone();
    let mut chunk_messages: Vec<JournalMessage> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        let (line, guid) = if cfg.skip_attachments {
            project::message_line_without_attachments(msg)?
        } else {
            project::message_line(msg, &per_message_digests[i])?
        };
        if !cfg.force
            && journal
                .messages
                .contains(&JournalState::message_key(name, &guid))
        {
            continue;
        }
        if line.len() > MAX_IMPORT_BODY_BYTES {
            bail!(
                "{name}: message {guid} encodes to {} bytes alone, which exceeds the \
                 {} MiB import chunk limit — cannot upload through Cloudflare safely",
                line.len(),
                MAX_IMPORT_BODY_BYTES / (1024 * 1024)
            );
        }
        if !chunk_messages.is_empty()
            && (chunk_messages.len() >= batch_size
                || chunk_body.len() + line.len() > MAX_IMPORT_BODY_BYTES)
        {
            chunks.push(ImportChunk {
                body: std::mem::replace(&mut chunk_body, header_line.clone()),
                messages: std::mem::take(&mut chunk_messages),
            });
        }
        chunk_body.extend_from_slice(&line);
        chunk_messages.push(JournalMessage {
            file: name.to_string(),
            guid,
        });
    }
    if !chunk_messages.is_empty() {
        chunks.push(ImportChunk {
            body: chunk_body,
            messages: chunk_messages,
        });
    }

    Ok(PreparedFile {
        source,
        chunks,
        attachments: attachment_count,
        profile,
        total_started,
    })
}

struct AssetUploadJob {
    digest: String,
    path: PathBuf,
    mime: Option<String>,
}

struct AssetUploadResult {
    digest: String,
    response: http::AssetPutResponse,
}

#[derive(Default)]
struct AssetUploadStats {
    bytes: u64,
}

struct UploadAssets<'a> {
    input: &'a Path,
    name: &'a str,
    cfg: &'a VaultPushConfig,
    http: &'a HttpSession,
    url: &'a str,
    username: &'a str,
    source: &'a str,
    unique: &'a BTreeMap<String, (String, Option<String>)>,
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    assets_uploaded: &'a mut u64,
    assets_skipped: &'a mut u64,
    log: &'a mut LogWriter,
}

fn upload_assets(args: UploadAssets<'_>) -> Result<AssetUploadStats> {
    let UploadAssets {
        input,
        name,
        cfg,
        http,
        url,
        username,
        source,
        unique,
        journal,
        journal_path,
        assets_uploaded,
        assets_skipped,
        log,
    } = args;
    let mut jobs = Vec::with_capacity(unique.len());
    let mut stats = AssetUploadStats::default();
    for (digest, (rel, mime)) in unique {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        if !cfg.force && journal.assets.contains(digest) {
            *assets_skipped += 1;
            continue;
        }
        let path = resolve_attachment(input, rel)
            .ok_or_else(|| anyhow::anyhow!("{name}: missing attachment {rel}"))?;
        let file_len = fs::metadata(&path)
            .with_context(|| format!("stat {}", path.display()))?
            .len();
        if file_len > cfg.asset_max_bytes {
            bail!(
                "{name}: attachment {rel} is {} bytes ({} MiB), over the configured \
                 asset max of {} MiB. Raise vault [server] asset_max_bytes (and \
                 vault-push --asset-max-bytes) or omit the file.",
                file_len,
                file_len / (1024 * 1024),
                cfg.asset_max_bytes / (1024 * 1024)
            );
        }
        stats.bytes = stats.bytes.saturating_add(file_len);
        jobs.push(AssetUploadJob {
            digest: digest.clone(),
            path,
            mime: mime.clone(),
        });
    }
    if jobs.is_empty() {
        return Ok(stats);
    }

    let worker_count = cfg.asset_upload_workers.max(1).min(jobs.len());
    let next_job = AtomicUsize::new(0);
    let results = Mutex::new(
        std::iter::repeat_with(|| None)
            .take(jobs.len())
            .collect::<Vec<Option<Result<AssetUploadResult, String>>>>(),
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
                    let result = check_cancel(cfg.cancel.as_ref())
                        .map_err(|_| "cancelled".to_string())
                        .and_then(|_| {
                            http::with_retries(cfg.max_retries, || {
                                if let Some(existing) = http.head_asset(
                                    url,
                                    &cfg.key,
                                    username,
                                    source,
                                    &job.digest,
                                )? {
                                    return Ok(existing);
                                }
                                http.put_asset(AssetPutRequest {
                                    base_url: url,
                                    key: &cfg.key,
                                    username,
                                    source,
                                    sha256: &job.digest,
                                    file: &job.path,
                                    mime: job.mime.as_deref(),
                                    multipart_threshold: cfg.asset_multipart_threshold,
                                })
                            })
                            .map(|response| AssetUploadResult {
                                digest: job.digest.clone(),
                                response,
                            })
                            .map_err(|error| error.to_string())
                        });
                    results.lock().expect("asset result mutex poisoned")[index] = Some(result);
                }
            });
        }
    });

    let mut results = results.into_inner().expect("asset result mutex poisoned");
    for result in results.drain(..) {
        let result = result.expect("every asset job has a result");
        let uploaded = result.map_err(|error| anyhow::anyhow!("{name}: {error}"))?;
        journal.assets.insert(uploaded.digest.clone());
        journal::append(
            journal_path,
            &JournalEvent::AssetOk {
                url: url.to_string(),
                username: username.to_string(),
                source: source.to_string(),
                sha256: uploaded.digest.clone(),
            },
        )?;
        if uploaded.response.already_present {
            *assets_skipped += 1;
        } else {
            *assets_uploaded += 1;
        }
        log.line(&format!(
            "asset {} {}",
            if uploaded.response.already_present {
                "skip"
            } else {
                "ok"
            },
            uploaded.digest
        ));
    }
    Ok(stats)
}

struct FileTracker {
    name: String,
    source: String,
    attachments: u64,
    profile: UploadProfile,
    total_started: Instant,
    outstanding_messages: usize,
    successful_messages: u64,
    queue_complete: bool,
    failed: Option<String>,
    done: bool,
}

struct BatchMessage {
    file_index: usize,
    journal: JournalMessage,
}

struct ImportBatch {
    source: String,
    body: Vec<u8>,
    messages: Vec<BatchMessage>,
    conversations: usize,
}

impl ImportBatch {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            body: Vec::new(),
            messages: Vec::new(),
            conversations: 0,
        }
    }

    fn push(&mut self, file_index: usize, chunk: ImportChunk) {
        self.body.extend_from_slice(&chunk.body);
        self.messages
            .extend(chunk.messages.into_iter().map(|journal| BatchMessage {
                file_index,
                journal,
            }));
        self.conversations += 1;
    }
}

fn should_flush_before_chunk(
    batch: &ImportBatch,
    chunk: &ImportChunk,
    max_messages: usize,
    max_body_bytes: usize,
) -> bool {
    !batch.messages.is_empty()
        && (batch.messages.len() + chunk.messages.len() > max_messages
            || batch.body.len() + chunk.body.len() > max_body_bytes)
}

struct RecordFileFailure<'a, 'p, 'f> {
    index: usize,
    total: usize,
    name: &'a str,
    error: &'a str,
    source: &'a str,
    url: &'a str,
    username: &'a str,
    journal_path: &'a Path,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
}

fn record_file_failure(args: RecordFileFailure<'_, '_, '_>) {
    let msg = format!(
        "PROGRESS {}/{} fail {} {}",
        args.index + 1,
        args.total,
        args.name,
        args.error
    );
    args.log.line(&msg);
    let _ = journal::append(
        args.journal_path,
        &JournalEvent::Fail {
            url: args.url.to_string(),
            username: args.username.to_string(),
            source: args.source.to_string(),
            file: args.name.to_string(),
            guid: None,
            sha256: None,
            stage: "file".into(),
            error: args.error.to_string(),
        },
    );
    if let Some(cb) = args.progress.as_mut() {
        cb(ProgressEvent::Log(msg));
        cb(ProgressEvent::FileDone {
            file: args.name.to_string(),
            status: "failed".into(),
        });
    }
    args.results[args.index] = Some(FileResult {
        file: args.name.to_string(),
        status: "failed".into(),
        error: Some(args.error.to_string()),
        messages: 0,
        attachments: 0,
        profile: None,
    });
}

struct FinishFile<'a, 'p, 'f> {
    index: usize,
    total: usize,
    trackers: &'a mut [Option<FileTracker>],
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    url: &'a str,
    username: &'a str,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
}

fn finish_file_if_ready(args: FinishFile<'_, '_, '_>) -> Result<()> {
    let Some(tracker) = args.trackers[args.index].as_mut() else {
        return Ok(());
    };
    if tracker.done
        || (tracker.failed.is_none()
            && (!tracker.queue_complete || tracker.outstanding_messages != 0))
    {
        return Ok(());
    }

    tracker.done = true;
    tracker.profile.total_ms = elapsed_ms(tracker.total_started);
    let name = tracker.name.clone();
    let source = tracker.source.clone();
    let attachments = tracker.attachments;
    let messages = tracker.successful_messages;
    let profile = tracker.profile.clone();
    let error = tracker.failed.clone();

    let (status, result_messages) = if error.is_some() {
        ("failed", 0)
    } else {
        args.journal.files.insert(name.clone());
        journal::append(
            args.journal_path,
            &JournalEvent::FileOk {
                url: args.url.to_string(),
                username: args.username.to_string(),
                source,
                file: name.clone(),
            },
        )?;
        ("ok", messages)
    };
    let msg = if let Some(error) = error.as_ref() {
        format!(
            "PROGRESS {}/{} fail {name} {error}",
            args.index + 1,
            args.total
        )
    } else {
        format!(
            "PROGRESS {}/{} ok {name} msgs={messages} attachments={attachments}",
            args.index + 1,
            args.total
        )
    };
    args.log.line(&msg);
    let profile_msg = format!(
        "PROFILE {name} read_ms={} attachment_scan_hash_ms={} asset_upload_ms={} \
         message_import_ms={} total_ms={} unique_assets={} asset_bytes={}",
        profile.read_ms,
        profile.attachment_scan_hash_ms,
        profile.asset_upload_ms,
        profile.message_import_ms,
        profile.total_ms,
        profile.unique_assets,
        profile.asset_bytes
    );
    args.log.line(&profile_msg);
    if let Some(cb) = args.progress.as_mut() {
        cb(ProgressEvent::Log(msg));
        cb(ProgressEvent::Log(profile_msg));
        cb(ProgressEvent::FileDone {
            file: name.clone(),
            status: status.into(),
        });
    }
    args.results[args.index] = Some(FileResult {
        file: name,
        status: status.into(),
        error,
        messages: result_messages,
        attachments,
        profile: Some(profile),
    });
    Ok(())
}

struct InFlightImport {
    handle: JoinHandle<ImportHttpOutcome>,
}

struct ImportHttpOutcome {
    batch: ImportBatch,
    mode: String,
    request_ms: u64,
    messages_per_second: f64,
    mebibytes_per_second: f64,
    body_bytes: usize,
    message_count: usize,
    response: Result<http::ImportResponse, String>,
}

struct FlushImportPipeline<'a, 'p, 'f> {
    cfg: &'a VaultPushConfig,
    http: &'a HttpSession,
    url: &'a str,
    username: &'a str,
    pending: &'a mut Option<ImportBatch>,
    inflight: &'a mut Option<InFlightImport>,
    first_import: &'a mut bool,
    trackers: &'a mut [Option<FileTracker>],
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
    total: usize,
    /// When true, wait for the newly spawned import (also used at end-of-run).
    wait: bool,
}

struct JoinInflightImport<'a, 'p, 'f> {
    inflight: &'a mut Option<InFlightImport>,
    first_import: &'a mut bool,
    trackers: &'a mut [Option<FileTracker>],
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    url: &'a str,
    username: &'a str,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
    total: usize,
}

/// Join at most one in-flight import, then optionally spawn the current pending batch.
/// With `wait=false` and `continue_on_error`, prepare of later files overlaps the HTTP import.
fn flush_import_pipeline(args: FlushImportPipeline<'_, '_, '_>) -> Result<bool> {
    let mut ok = join_inflight_import(JoinInflightImport {
        inflight: args.inflight,
        first_import: args.first_import,
        trackers: args.trackers,
        journal: args.journal,
        journal_path: args.journal_path,
        url: args.url,
        username: args.username,
        log: args.log,
        progress: args.progress,
        results: args.results,
        total: args.total,
    })?;
    if !ok && !args.cfg.continue_on_error {
        *args.pending = None;
        return Ok(false);
    }
    if args.pending.is_none() {
        return Ok(ok);
    }
    check_cancel(args.cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
    let mode = if args.cfg.mode == "replace" && *args.first_import {
        "replace".to_string()
    } else {
        "append".to_string()
    };
    let batch = args.pending.take().expect("pending checked");
    *args.inflight = Some(spawn_import_http(SpawnImportHttp {
        http: args.http.clone(),
        url: args.url.to_string(),
        key: args.cfg.key.clone(),
        username: args.username.to_string(),
        max_retries: args.cfg.max_retries,
        mode,
        batch,
    }));
    if args.wait {
        ok = join_inflight_import(JoinInflightImport {
            inflight: args.inflight,
            first_import: args.first_import,
            trackers: args.trackers,
            journal: args.journal,
            journal_path: args.journal_path,
            url: args.url,
            username: args.username,
            log: args.log,
            progress: args.progress,
            results: args.results,
            total: args.total,
        })?;
    }
    Ok(ok)
}

struct SpawnImportHttp {
    http: HttpSession,
    url: String,
    key: String,
    username: String,
    max_retries: u32,
    mode: String,
    batch: ImportBatch,
}

fn spawn_import_http(args: SpawnImportHttp) -> InFlightImport {
    let handle = std::thread::spawn(move || {
        let SpawnImportHttp {
            http,
            url,
            key,
            username,
            max_retries,
            mode,
            batch,
        } = args;
        let request_started = Instant::now();
        let body_bytes = batch.body.len();
        let message_count = batch.messages.len();
        let response = http::with_retries(max_retries, || {
            http.post_import(
                &url,
                &key,
                &username,
                &batch.source,
                &mode,
                batch.body.clone(),
            )
        })
        .map_err(|error| error.to_string());
        let request_ms = elapsed_ms(request_started);
        let seconds = request_started.elapsed().as_secs_f64().max(0.001);
        ImportHttpOutcome {
            batch,
            mode,
            request_ms,
            messages_per_second: message_count as f64 / seconds,
            mebibytes_per_second: body_bytes as f64 / (1024.0 * 1024.0) / seconds,
            body_bytes,
            message_count,
            response,
        }
    });
    InFlightImport { handle }
}

fn join_inflight_import(args: JoinInflightImport<'_, '_, '_>) -> Result<bool> {
    let Some(job) = args.inflight.take() else {
        return Ok(true);
    };
    let outcome = job
        .handle
        .join()
        .map_err(|_| anyhow::anyhow!("import worker panicked"))?;
    apply_import_outcome(ApplyImportOutcome {
        outcome,
        first_import: args.first_import,
        trackers: args.trackers,
        journal: args.journal,
        journal_path: args.journal_path,
        url: args.url,
        username: args.username,
        log: args.log,
        progress: args.progress,
        results: args.results,
        total: args.total,
    })
}

struct ApplyImportOutcome<'a, 'p, 'f> {
    outcome: ImportHttpOutcome,
    first_import: &'a mut bool,
    trackers: &'a mut [Option<FileTracker>],
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    url: &'a str,
    username: &'a str,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
    total: usize,
}

fn apply_import_outcome(args: ApplyImportOutcome<'_, '_, '_>) -> Result<bool> {
    let ImportHttpOutcome {
        batch,
        mode,
        request_ms,
        messages_per_second,
        mebibytes_per_second,
        body_bytes,
        message_count,
        response,
    } = args.outcome;
    let represented: BTreeSet<usize> = batch
        .messages
        .iter()
        .map(|message| message.file_index)
        .collect();

    match response {
        Ok(response) => {
            *args.first_import = false;
            let journal_messages: Vec<JournalMessage> = batch
                .messages
                .iter()
                .map(|message| message.journal.clone())
                .collect();
            journal::append(
                args.journal_path,
                &JournalEvent::MessageBatchOk {
                    url: args.url.to_string(),
                    username: args.username.to_string(),
                    source: batch.source.clone(),
                    messages: journal_messages.clone(),
                },
            )?;
            for message in &batch.messages {
                args.journal.messages.insert(JournalState::message_key(
                    &message.journal.file,
                    &message.journal.guid,
                ));
                if let Some(tracker) = args.trackers[message.file_index].as_mut() {
                    tracker.outstanding_messages = tracker.outstanding_messages.saturating_sub(1);
                    tracker.successful_messages = tracker.successful_messages.saturating_add(1);
                }
            }
            let request_line = format!(
                "IMPORT_REQUEST ok source={} mode={mode} conversations={} messages={} \
                 server_messages={} bytes={body_bytes} elapsed_ms={request_ms} \
                 messages_per_second={messages_per_second:.1} mib_per_second={mebibytes_per_second:.2}",
                batch.source,
                batch.conversations,
                message_count,
                response.messages.max(response.messages_appended),
            );
            args.log.line(&request_line);
            if let Some(cb) = args.progress.as_mut() {
                cb(ProgressEvent::Log(request_line));
            }
            for index in represented {
                if let Some(tracker) = args.trackers[index].as_mut() {
                    tracker.profile.message_import_ms =
                        tracker.profile.message_import_ms.saturating_add(request_ms);
                }
                finish_file_if_ready(FinishFile {
                    index,
                    total: args.total,
                    trackers: args.trackers,
                    journal: args.journal,
                    journal_path: args.journal_path,
                    url: args.url,
                    username: args.username,
                    log: args.log,
                    progress: args.progress,
                    results: args.results,
                })?;
            }
            Ok(true)
        }
        Err(error) => {
            let request_line = format!(
                "IMPORT_REQUEST fail source={} mode={mode} conversations={} messages={} \
                 bytes={body_bytes} elapsed_ms={request_ms} \
                 messages_per_second={messages_per_second:.1} mib_per_second={mebibytes_per_second:.2} \
                 error={error}",
                batch.source, batch.conversations, message_count,
            );
            args.log.line(&request_line);
            if let Some(cb) = args.progress.as_mut() {
                cb(ProgressEvent::Log(request_line));
            }
            for index in represented {
                let Some(tracker) = args.trackers[index].as_mut() else {
                    continue;
                };
                tracker.profile.message_import_ms =
                    tracker.profile.message_import_ms.saturating_add(request_ms);
                if tracker.failed.is_none() {
                    tracker.failed = Some(error.clone());
                    let _ = journal::append(
                        args.journal_path,
                        &JournalEvent::Fail {
                            url: args.url.to_string(),
                            username: args.username.to_string(),
                            source: batch.source.clone(),
                            file: tracker.name.clone(),
                            guid: None,
                            sha256: None,
                            stage: "import".into(),
                            error: error.clone(),
                        },
                    );
                }
                finish_file_if_ready(FinishFile {
                    index,
                    total: args.total,
                    trackers: args.trackers,
                    journal: args.journal,
                    journal_path: args.journal_path,
                    url: args.url,
                    username: args.username,
                    log: args.log,
                    progress: args.progress,
                    results: args.results,
                })?;
            }
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(body_bytes: usize, messages: usize) -> ImportChunk {
        ImportChunk {
            body: vec![b'x'; body_bytes],
            messages: (0..messages)
                .map(|index| JournalMessage {
                    file: "conversation.jsonl".into(),
                    guid: format!("guid-{index}"),
                })
                .collect(),
        }
    }

    #[test]
    fn import_batch_flushes_for_message_or_byte_limit() {
        let mut batch = ImportBatch::new("imessage");
        batch.push(0, chunk(40, 2));

        assert!(should_flush_before_chunk(&batch, &chunk(10, 2), 3, 100));
        assert!(should_flush_before_chunk(&batch, &chunk(70, 1), 10, 100));
        assert!(!should_flush_before_chunk(&batch, &chunk(10, 1), 3, 100));
    }

    #[test]
    fn format_duration_ms_humanizes() {
        assert_eq!(format_duration_ms(0), "0s");
        assert_eq!(format_duration_ms(999), "999ms");
        assert_eq!(format_duration_ms(12_000), "12s");
        assert_eq!(format_duration_ms(2_052_000), "34m12s");
        assert_eq!(format_duration_ms(3_723_000), "1h02m03s");
    }

    #[test]
    fn normalize_digest_sha256_accepts_hex() {
        let d = "A".repeat(64);
        assert_eq!(normalize_digest_sha256(&d).unwrap(), "a".repeat(64));
        assert!(normalize_digest_sha256("not-a-digest").is_err());
    }
}
