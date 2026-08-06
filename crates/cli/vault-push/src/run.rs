//! Upload a folder of conversation files into Message Vault.
//!
//! # What this module does
//!
//! An export folder has one `.jsonl` file per conversation, plus an
//! `attachments/` folder of media files. This code:
//!
//! 1. Logs in to the vault with the API key.
//! 2. For each conversation file, finds attachments, uploads any the vault
//!    does not already have, then sends the messages in batches.
//! 3. Remembers progress in a journal file so a later run can skip work that
//!    already succeeded.
//!
//! # Why it is built this way (upload performance)
//!
//! - **Attachments first, then messages.** Messages point at attachments by a
//!   content fingerprint (sha256). The vault must already have that file, or
//!   the import would fail. So we upload media before we send message text.
//! - **Fingerprint = sha256.** Same bytes always produce the same hex string.
//!   The vault stores one copy per fingerprint, so the same photo shared in
//!   many chats is uploaded once.
//! - **Prepare ahead.** Reading a chat and uploading its media can take a long
//!   time. While the main loop waits on a message-import HTTP request, other
//!   threads can already prepare the next few conversations. That hides disk
//!   and upload work behind network wait time.
//! - **Several attachment uploads at once.** Small files are slow if sent one
//!   after another (network round trips dominate). Workers upload several at
//!   the same time.
//! - **One message-import request at a time.** Imports update shared vault
//!   state; running many imports in parallel is harder to reason about and
//!   can confuse the journal. Attachments stay parallel; message batches do not.
//! - **Size limits on each request.** Cloudflare (and similar proxies) reject
//!   huge single uploads. We split message batches and use multipart for large
//!   attachments so a big chat or video does not hit that wall.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
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

/// How many messages to pack into one import HTTP request when size is not the limit.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;
/// Soft max size of one import request body (about 8 MiB).
///
/// Kept far under Cloudflare's ~100 MiB upload cap so a large group chat is
/// split into several requests instead of one giant one that gets rejected.
pub const MAX_IMPORT_BODY_BYTES: usize = 8 * 1024 * 1024;
/// Max size for uploading an attachment in a single HTTP PUT.
///
/// Bigger files use multipart upload (many smaller pieces), which proxies
/// accept more reliably than one huge body.
pub const MAX_PROXY_BODY_BYTES: usize = 90 * 1024 * 1024;
/// Refuse attachments larger than this (must match the vault server setting).
pub const DEFAULT_ASSET_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// How many attachment uploads may run at the same time.
pub const DEFAULT_ASSET_UPLOAD_WORKERS: usize = 8;
/// How many conversations we may prepare (read + upload media) ahead of the
/// import loop. Higher uses more memory/disk bandwidth; lower leaves the CPU
/// idle while waiting on the network.
pub const DEFAULT_PREPARE_AHEAD: usize = 3;
/// Worker threads that run [`prepare_file`] for that prepare-ahead queue.
pub const DEFAULT_PREPARE_WORKERS: usize = 2;

/// Shared map: absolute file path → sha256 hex string.
///
/// The same attachment file can appear in many chats. Caching the hash means
/// we only read and hash that file once per push run.
type DigestCache = Mutex<HashMap<PathBuf, String>>;

/// Settings for one full push run (paths, URL, flags, limits).
#[derive(Debug, Clone)]
pub struct VaultPushConfig {
    pub input: PathBuf,
    pub base_url: String,
    pub username: String,
    pub key: String,
    /// `"append"` adds to existing data; `"replace"` clears then imports (with force).
    pub mode: String,
    /// If true, keep going after one conversation fails. If false, stop early.
    pub continue_on_error: bool,
    /// If true, ignore the journal and upload/import everything again.
    pub force: bool,
    /// Text-only import: do not upload or attach media.
    pub skip_attachments: bool,
    /// If true, always re-hash files and fail when the export's claimed sha256
    /// does not match the bytes on disk.
    ///
    /// If false (default), trust a sha256 already written in the JSONL when it
    /// is present. That skips a slow full-file hash for every attachment. We
    /// still hash when the export left the digest empty. A path cache avoids
    /// hashing the same file twice when several chats share it.
    pub verify_digests: bool,
    /// If true, skip re-hashing attachments when the JSONL `size_bytes` matches
    /// the file size on disk. Default remains full verification of every file.
    pub trust_export: bool,
    pub max_retries: u32,
    pub batch_size: usize,
    /// Max parallel attachment uploads. Message imports stay one-at-a-time.
    pub asset_upload_workers: usize,
    /// Files larger than this use multipart upload instead of one PUT.
    pub asset_multipart_threshold: usize,
    /// Hard max attachment size we will attempt to upload.
    pub asset_max_bytes: u64,
    pub report_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub journal_path: Option<PathBuf>,
    pub cancel: Option<CancelFlag>,
}

/// Per-conversation outcome written into the final report JSON.
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

/// Timing and size stats for one conversation (used for PROFILE log lines).
///
/// These numbers help answer "why was this chat slow?" — reading JSONL,
/// hashing/scanning attachments, uploading media, or importing messages.
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

/// Final summary of a whole push (also written to disk as the report file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushReport {
    pub ok: bool,
    pub account: String,
    pub username: String,
    pub mode: String,
    pub started_at: String,
    pub finished_at: String,
    /// Wall-clock time from start of auth through the last import.
    pub elapsed_ms: u64,
    pub conversations_total: u64,
    pub conversations_ok: u64,
    pub conversations_failed: u64,
    pub conversations_skipped: u64,
    pub messages: u64,
    pub assets_uploaded: u64,
    pub assets_skipped: u64,
    pub assets_bytes: u64,
    pub results: Vec<FileResult>,
}

/// Events the GUI/CLI can show while a push is running.
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

/// Callback type for live progress (GUI log panel, CLI stderr, tests).
pub type ProgressFn<'a> = dyn FnMut(ProgressEvent) + Send + 'a;

/// Append-only log file next to the export (also mirrored to progress callbacks).
struct LogWriter {
    file: File,
}

impl LogWriter {
    /// Create or open the log file, making parent folders if needed.
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

    /// Write one line and flush so a crash still leaves the last message on disk.
    fn line(&mut self, msg: &str) {
        let _ = writeln!(self.file, "{msg}");
        let _ = self.file.flush();
    }
}

/// Unix time in seconds as a string (for report timestamps).
fn now_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

/// Milliseconds since `started` (for PROFILE timing fields).
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Turn a millisecond count into a short human string like `34m12s` or `1h02m03s`.
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

/// Build the multi-line "Import success / completed with errors" blurb for the log.
pub fn format_push_summary(report: &PushReport) -> String {
    let status = if report.ok {
        "success"
    } else {
        "completed with errors"
    };
    format!(
        "==== Summary ====\n\
Import {status}\n\
Conversations: {} ok, {} failed, {} skipped ({} total)\n\
Messages: {}\n\
Assets: {} uploaded, {} skipped\n\
Elapsed: {} ({} ms)",
        report.conversations_ok,
        report.conversations_failed,
        report.conversations_skipped,
        report.conversations_total,
        report.messages,
        report.assets_uploaded,
        report.assets_skipped,
        format_duration_ms(report.elapsed_ms),
        report.elapsed_ms,
    )
}

/// How many finished conversations we group into one "files N/M …" log line.
/// Printing every single chat would flood the log on a big import.
const PROGRESS_BATCH_SIZE: usize = 10;
/// If the pending message batch is at least this many messages, start its HTTP
/// import now instead of waiting until we finish preparing the next chat.
///
/// Why: preparing the next chat may upload many attachments. If we hold a large
/// ready batch until that finishes, the UI looks stuck and we waste time when
/// the network could already be importing.
const OVERLAP_FLUSH_MIN_MESSAGES: usize = 100;
/// Same idea as [`OVERLAP_FLUSH_MIN_MESSAGES`], but for batch body size in bytes.
const OVERLAP_FLUSH_MIN_BODY_BYTES: usize = 512 * 1024;

/// Collects successes and emits one progress line every [`PROGRESS_BATCH_SIZE`] files.
struct ProgressBatcher {
    total: usize,
    done: usize,
    chunk_conversations: u64,
    chunk_messages: u64,
    chunk_bytes: u64,
    chunk_import_ms: u64,
    /// Wall clock for the current progress chunk (first note → emit).
    chunk_started: Option<Instant>,
    chunk_count: usize,
}

impl ProgressBatcher {
    fn new(total: usize) -> Self {
        Self {
            total,
            done: 0,
            chunk_conversations: 0,
            chunk_messages: 0,
            chunk_bytes: 0,
            chunk_import_ms: 0,
            chunk_started: None,
            chunk_count: 0,
        }
    }

    fn begin_chunk_if_needed(&mut self) {
        if self.chunk_started.is_none() {
            self.chunk_started = Some(Instant::now());
        }
    }

    /// Record one successful conversation. Returns a log line when the batch is full.
    fn note_ok(&mut self, messages: u64, profile: &UploadProfile) -> Option<String> {
        self.begin_chunk_if_needed();
        self.done = self.done.saturating_add(1);
        self.chunk_count = self.chunk_count.saturating_add(1);
        self.chunk_conversations = self.chunk_conversations.saturating_add(1);
        self.chunk_messages = self.chunk_messages.saturating_add(messages);
        self.chunk_bytes = self.chunk_bytes.saturating_add(profile.asset_bytes);
        self.chunk_import_ms = self
            .chunk_import_ms
            .saturating_add(profile.message_import_ms);
        if self.chunk_count >= PROGRESS_BATCH_SIZE || self.done >= self.total {
            Some(self.take_chunk_line())
        } else {
            None
        }
    }

    /// Record a conversation skipped because the journal says it already imported.
    fn note_skipped(&mut self) -> Option<String> {
        self.begin_chunk_if_needed();
        self.done = self.done.saturating_add(1);
        self.chunk_count = self.chunk_count.saturating_add(1);
        self.chunk_conversations = self.chunk_conversations.saturating_add(1);
        if self.chunk_count >= PROGRESS_BATCH_SIZE || self.done >= self.total {
            Some(self.take_chunk_line())
        } else {
            None
        }
    }

    /// Count a failure toward "done" without adding it to the success chunk totals.
    fn note_failed(&mut self) {
        self.done = self.done.saturating_add(1);
    }

    /// Emit any leftover partial batch at the end of the run.
    fn flush_remainder(&mut self) -> Option<String> {
        if self.chunk_count == 0 {
            None
        } else {
            Some(self.take_chunk_line())
        }
    }

    /// Format the current chunk line, then zero the counters for the next chunk.
    fn take_chunk_line(&mut self) -> String {
        // Wall time for this progress window — not the sum of per-file clocks
        // (those overlap when prepares run ahead of imports).
        let wall_ms = self.chunk_started.map(elapsed_ms).unwrap_or(0);
        let line = format!(
            "files {}/{} - conversations={} messages={} transfer size={}, import time={}, total time={}",
            self.done,
            self.total,
            self.chunk_conversations,
            self.chunk_messages,
            format_bytes_mb(self.chunk_bytes),
            format_ms_seconds(self.chunk_import_ms),
            format_ms_seconds(wall_ms),
        );
        self.chunk_conversations = 0;
        self.chunk_messages = 0;
        self.chunk_bytes = 0;
        self.chunk_import_ms = 0;
        self.chunk_started = None;
        self.chunk_count = 0;
        line
    }
}

fn format_bytes_mb(bytes: u64) -> String {
    format!("{:.1}MB", bytes as f64 / 1_000_000.0)
}

fn format_ms_seconds(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

/// Write a line to the log file and, if present, to the live progress callback.
fn emit_progress_line(
    log: &mut LogWriter,
    progress: &mut Option<&mut ProgressFn<'_>>,
    line: String,
) {
    log.line(&line);
    if let Some(cb) = progress.as_mut() {
        cb(ProgressEvent::Log(line));
    }
}

/// Log a conversation failure, plus optional PROFILE timing so slow fails are diagnosable.
fn emit_file_failure_lines(
    log: &mut LogWriter,
    progress: &mut Option<&mut ProgressFn<'_>>,
    name: &str,
    error: &str,
    profile: Option<&UploadProfile>,
) {
    let fail_line = format!("fail {name}: {error}");
    emit_progress_line(log, progress, fail_line);
    if let Some(profile) = profile {
        emit_progress_line(log, progress, format_profile_line(name, profile));
    }
}

/// One PROFILE line with per-phase timings for a conversation.
fn format_profile_line(name: &str, profile: &UploadProfile) -> String {
    format!(
        "PROFILE {name} read_ms={} attachment_scan_hash_ms={} asset_upload_ms={} \
         message_import_ms={} total_ms={} unique_assets={} asset_bytes={}",
        profile.read_ms,
        profile.attachment_scan_hash_ms,
        profile.asset_upload_ms,
        profile.message_import_ms,
        profile.total_ms,
        profile.unique_assets,
        profile.asset_bytes
    )
}

/// True for files vault-push itself writes (journal/report/log), not conversations.
fn is_push_artifact(name: &str) -> bool {
    name.eq_ignore_ascii_case(journal::JOURNAL_NAME)
        || name.eq_ignore_ascii_case(journal::REPORT_NAME)
        || name.eq_ignore_ascii_case(journal::LOG_NAME)
        || name.ends_with(".jsonl.tmp")
        || name.starts_with('.')
}

/// List conversation `.jsonl` files in `dir`, sorted, skipping journal/report/log.
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
    // Stable order so progress "3/681" is repeatable across runs.
    paths.sort();
    Ok(paths)
}

/// Check that a sha256 string is exactly 64 hex digits; return lowercase form.
fn normalize_digest_sha256(digest: &str) -> Result<String> {
    let s = digest.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid sha256 digest (expected 64 hex digits)");
    }
    Ok(s)
}

/// Read a whole file and return its sha256 as a lowercase hex string.
///
/// "Hashing" here means feeding every byte into the SHA-256 algorithm. The
/// result is a fingerprint: same file bytes → same hex string. We stream the
/// file in 64 KiB chunks so a large video does not have to sit entirely in RAM.
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

/// Resolve the sha256 for an attachment file. The default behavior is to hash
/// every file from disk, compare against any JSONL claim, and warn on mismatch
/// (using the actual disk hash). Two flags alter this:
///
/// * `trust_export` — skip the hash when the JSONL `size_bytes` matches the
///   file size on disk (a cheap proxy for "file unchanged since export").
/// * `verify_digests` — hash from disk and **fail** on mismatch (no correction).
///
/// The vault server is the final verifier on upload; a stale sha256 is
/// self-correcting (the server rejects mismatches).
fn resolve_attachment_digest(
    abs: &Path,
    claimed_raw: Option<&str>,
    claimed_size: Option<u64>,
    verify_digests: bool,
    trust_export: bool,
    cache: &DigestCache,
    name: &str,
    rel: &str,
    warn: &mut dyn FnMut(String),
) -> Result<String> {
    // Fast path: another conversation already hashed this absolute path
    // during this run. Always trust the cache — it was computed from disk.
    {
        let guard = cache.lock().expect("digest cache mutex poisoned");
        if let Some(digest) = guard.get(abs) {
            return Ok(digest.clone());
        }
    }

    // Normalize the claimed sha256 from JSONL (may be absent or malformed).
    let claimed = match claimed_raw {
        Some(raw) => match normalize_digest_sha256(raw) {
            Ok(d) => Some(d),
            Err(e) => {
                warn(format!("{name}: bad digest_sha256 for {rel}: {e}"));
                None
            }
        },
        None => None,
    };

    let disk_size = std::fs::metadata(abs)
        .with_context(|| format!("{name}: stat {rel}"))?
        .len();

    // trust_export fast path: skip hash when JSONL size matches disk.
    if trust_export && !verify_digests {
        if let (Some(ref dig), Some(cl_size)) = (claimed.as_ref(), claimed_size) {
            if cl_size == disk_size {
                let digest = dig.to_string();
                cache
                    .lock()
                    .expect("digest cache mutex poisoned")
                    .insert(abs.to_path_buf(), digest.clone());
                return Ok(digest);
            }
        }
    }

    // Hash from disk — the default path.
    let disk_digest =
        hash_file(abs).with_context(|| format!("{name}: hash {rel}"))?;

    // Compare against JSONL claim.
    if let Some(ref claimed_digest) = claimed {
        if claimed_digest != &disk_digest {
            let size_note = match claimed_size {
                Some(cs) if cs != disk_size => {
                    format!(", size changed from {cs} to {disk_size} bytes")
                }
                _ => String::new(),
            };
            let msg = format!(
                "{name}: sha256 mismatch for {rel}: \
                 claimed {claimed_digest}, got {disk_digest}{size_note}"
            );
            if verify_digests {
                bail!("{msg}");
            }
            warn(msg);
        }
    }

    cache
        .lock()
        .expect("digest cache mutex poisoned")
        .insert(abs.to_path_buf(), disk_digest.clone());
    Ok(disk_digest)
}

/// Turn an attachment path from JSONL into a real file path under the export folder.
fn resolve_attachment(export_root: &Path, rel: &str) -> Option<PathBuf> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let under = export_root.join(candidate);
    under.is_file().then_some(under)
}

/// Reject paths that could escape the export folder (absolute paths or `..`).
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

/// Check the API key against the vault without importing any messages.
pub fn authenticate(
    base_url: &str,
    key: &str,
    username: &str,
) -> std::result::Result<AuthInfo, crate::AuthError> {
    http::auth_check(base_url, key, username)
}

/// Read the first conversation file's header and return its `export.source` string.
///
/// The GUI uses this to label the import session (for example `imessage`).
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

/// Everything `run` needs after login and scanning the export folder.
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
    /// Id from `/v1/imports` when the server supports import sessions; else `None`.
    import_id: Option<i64>,
}

/// Log in, open log/journal paths, list conversation files, start an import session.
///
/// This is the "setup" half of a push. The heavy upload loop lives in [`run`].
fn prepare_run_setup(
    cfg: &VaultPushConfig,
    progress: &mut Option<&mut ProgressFn<'_>>,
) -> Result<RunSetup> {
    // Accept either a folder or a file path; we always work from the folder.
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
    // The API key decides which account we are. Prefer the username the server
    // returns; fall back to the account id string if username is empty.
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

    // Fresh empty journal when forcing a full re-upload or replace mode.
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

    // Best-effort: tell the vault "a new import run is starting" so Storage UI
    // can show sessions. Older servers may not support this; we continue anyway.
    let source = detect_source(&input)?
        .unwrap_or_else(|| "unknown".to_string());
    let import_id = match http.start_import(
        &url,
        &cfg.key,
        &username,
        &source,
        &cfg.mode,
        Some("vault-push"),
    ) {
        Ok(id) => {
            if let Some(id) = id {
                log.line(&format!("vault import session id={id} source={source}"));
                if let Some(cb) = progress.as_mut() {
                    cb(ProgressEvent::Log(format!(
                        "Recording import session {id} ({source})"
                    )));
                }
            } else {
                log.line(
                    "vault import sessions not supported by this server; continuing without import_id",
                );
            }
            id
        }
        Err(error) => {
            log.line(&format!(
                "warning: could not start vault import session: {error}"
            ));
            if let Some(cb) = progress.as_mut() {
                cb(ProgressEvent::Log(format!(
                    "Warning: could not start vault import session: {error}"
                )));
            }
            None
        }
    };

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
        import_id,
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
    assets_bytes: u64,
    aborted: bool,
    http: &'a HttpSession,
    import_id: Option<i64>,
}

/// Count successes/failures, write the report JSON, compact the journal, notify progress.
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
        assets_bytes,
        aborted,
        http,
        import_id,
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
    let attachments: u64 = results
        .iter()
        .filter(|result| result.status == "ok")
        .map(|result| result.attachments)
        .sum();
    // Only shrink/rewrite the journal after a clean run so a failed run can retry.
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
        assets_bytes,
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
    if let Some(import_id) = import_id {
        match http.complete_import(
            &url,
            &cfg.key,
            import_id,
            report.ok,
            report.messages,
            attachments,
            assets_bytes,
        ) {
            Ok(()) => log.line(&format!("vault import session {import_id} completed")),
            Err(error) => log.line(&format!(
                "warning: could not complete vault import session {import_id}: {error}"
            )),
        }
    }
    log.line("");
    log.line(&format_push_summary(&report));
    if let Some(cb) = progress.as_mut() {
        cb(ProgressEvent::Log(String::new()));
        cb(ProgressEvent::Finished(report.clone()));
    }
    Ok(report)
}

/// Push every `.jsonl` conversation under `cfg.input`.
/// Run a full folder push: prepare conversations (with media upload), then import messages.
///
/// High-level flow:
/// 1. Setup (login, list files) via [`prepare_run_setup`].
/// 2. Start a few **prepare workers**. Each worker reads one chat file, uploads
///    its attachments, and builds message chunks. That work is slow (disk + network).
/// 3. The main thread **consumes prepare results in file order**, packs message
///    chunks into import batches, and sends those batches over HTTP.
/// 4. Message imports are mostly one-at-a-time, but we can start an import while
///    prepare workers keep working on later chats (overlap for speed).
/// 5. [`finish_run`] writes the report and cleans up.
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
        journal,
        files,
        total,
        batch_size,
        import_id,
    } = prepare_run_setup(cfg, &mut progress)?;

    // One slot per conversation file; filled as we finish or skip each one.
    let mut results: Vec<Option<FileResult>> = vec![None; total];
    let mut assets_uploaded = 0u64;
    let mut assets_skipped = 0u64;
    let mut assets_bytes = 0u64;
    // First import in replace mode may use mode=replace; later ones use append.
    let mut first_import = true;
    let mut aborted = false;
    let mut trackers: Vec<Option<FileTracker>> =
        std::iter::repeat_with(|| None).take(total).collect();
    // Messages waiting to be sent, and the HTTP import currently in flight (if any).
    let mut pending: Option<ImportBatch> = None;
    let mut inflight: Option<InFlightImport> = None;
    let mut batcher = ProgressBatcher::new(total);

    // Shared across prepare workers: hash cache + journal of already-uploaded assets.
    let digest_cache: Arc<DigestCache> = Arc::new(Mutex::new(HashMap::new()));
    let shared_journal = Arc::new(Mutex::new(SharedJournal {
        state: journal,
        assets_in_flight: HashSet::new(),
    }));

    // Bounded queue: at most `prepare_ahead` jobs waiting/running so we do not
    // prepare hundreds of chats (and hold their data) before the import loop catches up.
    let prepare_ahead = DEFAULT_PREPARE_AHEAD.max(1);
    let prepare_workers = DEFAULT_PREPARE_WORKERS.max(1).min(prepare_ahead);
    let (job_tx, job_rx) = mpsc::sync_channel::<Option<PrepareJob>>(prepare_ahead);
    let (result_tx, result_rx) = mpsc::channel::<PrepareJobResult>();
    let job_rx = Arc::new(Mutex::new(job_rx));

    std::thread::scope(|scope| -> Result<()> {
        // Prepare workers: pull jobs until they receive `None` (shutdown signal).
        for _ in 0..prepare_workers {
            let job_rx = Arc::clone(&job_rx);
            let result_tx = result_tx.clone();
            let digest_cache = Arc::clone(&digest_cache);
            let shared_journal = Arc::clone(&shared_journal);
            let http = http.clone();
            let input = input.clone();
            let url = url.clone();
            let username = username.clone();
            let journal_path = journal_path.clone();
            scope.spawn(move || {
                loop {
                    let job = {
                        let rx = job_rx.lock().expect("prepare job mutex poisoned");
                        rx.recv().unwrap_or(None)
                    };
                    let Some(job) = job else {
                        break;
                    };
                    let outcome = prepare_file(PrepareFileArgs {
                        input: &input,
                        path: &job.path,
                        name: &job.name,
                        cfg,
                        http: &http,
                        url: &url,
                        username: &username,
                        journal: &shared_journal,
                        journal_path: &journal_path,
                        batch_size,
                        digest_cache: &digest_cache,
                    });
                    let _ = result_tx.send(PrepareJobResult {
                        idx: job.idx,
                        name: job.name,
                        outcome,
                    });
                }
            });
        }
        // Drop our clone so workers' sends finish cleanly when they exit.
        drop(result_tx);

        // Submit conversations for prepare, and consume finished prepares in order.
        // Workers may finish out of order; `prepared_buf` holds early results until
        // we are ready for that index (keeps import order stable for the journal).
        let mut next_submit = 0usize;
        let mut next_consume = 0usize;
        let mut inflight_prepares = 0usize;
        let mut prepared_buf: BTreeMap<usize, PrepareJobResult> = BTreeMap::new();
        let mut stop_submitting = false;

        while next_consume < total {
            check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;

            // If we already have a large pending import batch, start its HTTP
            // request now (without waiting) so prepare workers keep the pipeline full.
            if pending.as_ref().is_some_and(|batch| {
                batch.messages.len() >= OVERLAP_FLUSH_MIN_MESSAGES
                    || batch.body.len() >= OVERLAP_FLUSH_MIN_BODY_BYTES
            }) {
                let request_ok = {
                    let mut guard = shared_journal.lock().expect("journal mutex poisoned");
                    flush_import_pipeline(FlushImportPipeline {
                        cfg,
                        http: &http,
                        url: &url,
                        username: &username,
                        pending: &mut pending,
                        inflight: &mut inflight,
                        first_import: &mut first_import,
                        trackers: &mut trackers,
                        journal: &mut guard.state,
                        journal_path: &journal_path,
                        log: &mut log,
                        progress: &mut progress,
                        results: &mut results,
                        batcher: &mut batcher,
                        import_id,
                        wait: false,
                    })?
                };
                if !request_ok && !cfg.continue_on_error {
                    aborted = true;
                    stop_submitting = true;
                }
            }

            // Fill the prepare queue up to `prepare_ahead` outstanding jobs.
            while !stop_submitting
                && !aborted
                && next_submit < total
                && inflight_prepares < prepare_ahead
            {
                let path = files[next_submit].clone();
                let idx = next_submit;
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

                // Journal says this whole file already imported successfully — skip work.
                let skip = cfg.mode == "append"
                    && !cfg.force
                    && shared_journal
                        .lock()
                        .expect("journal mutex poisoned")
                        .state
                        .files
                        .contains(&name);
                if skip {
                    results[idx] = Some(FileResult {
                        file: name.clone(),
                        status: "skipped".into(),
                        error: None,
                        messages: 0,
                        attachments: 0,
                        profile: None,
                    });
                    if let Some(cb) = progress.as_mut() {
                        cb(ProgressEvent::FileDone {
                            file: name,
                            status: "skipped".into(),
                        });
                    }
                    if let Some(line) = batcher.note_skipped() {
                        emit_progress_line(&mut log, &mut progress, line);
                    }
                    // Placeholder so the consume side still advances in index order.
                    // Empty `name` is the signal "already handled as skipped".
                    prepared_buf.insert(
                        idx,
                        PrepareJobResult {
                            idx,
                            name: String::new(),
                            outcome: Ok(PreparedFile {
                                source: String::new(),
                                chunks: Vec::new(),
                                attachments: 0,
                                profile: UploadProfile::default(),
                                total_started: Instant::now(),
                                assets_uploaded: 0,
                                assets_skipped: 0,
                                assets_bytes: 0,
                                log_lines: Vec::new(),
                            }),
                        },
                    );
                    next_submit += 1;
                    continue;
                }

                job_tx
                    .send(Some(PrepareJob { idx, path, name }))
                    .expect("prepare workers alive");
                inflight_prepares += 1;
                next_submit += 1;
            }

            if aborted {
                break;
            }

            // Wait for the next prepare result if we do not already have index `next_consume`.
            if !prepared_buf.contains_key(&next_consume) {
                if inflight_prepares == 0 && next_submit >= total {
                    break;
                }
                let job = result_rx
                    .recv()
                    .context("prepare worker disconnected")?;
                inflight_prepares = inflight_prepares.saturating_sub(1);
                prepared_buf.insert(job.idx, job);
            }

            // Process every consecutive ready index starting at `next_consume`.
            while let Some(job) = prepared_buf.remove(&next_consume) {
                let idx = job.idx;
                // Skipped files already recorded above (empty name sentinel).
                if job.name.is_empty() {
                    next_consume += 1;
                    continue;
                }
                let name = job.name;
                let prepared = match job.outcome {
                    Ok(prepared) => prepared,
                    Err(e) => {
                        if !cfg.continue_on_error && (pending.is_some() || inflight.is_some()) {
                            let request_ok = {
                                let mut guard =
                                    shared_journal.lock().expect("journal mutex poisoned");
                                flush_import_pipeline(FlushImportPipeline {
                                    cfg,
                                    http: &http,
                                    url: &url,
                                    username: &username,
                                    pending: &mut pending,
                                    inflight: &mut inflight,
                                    first_import: &mut first_import,
                                    trackers: &mut trackers,
                                    journal: &mut guard.state,
                                    journal_path: &journal_path,
                                    log: &mut log,
                                    progress: &mut progress,
                                    results: &mut results,
                                    batcher: &mut batcher,
                                    import_id,
                                    wait: true,
                                })?
                            };
                            if !request_ok {
                                aborted = true;
                                stop_submitting = true;
                                break;
                            }
                        }
                        let err = e.to_string();
                        record_file_failure(RecordFileFailure {
                            index: idx,
                            name: &name,
                            error: &err,
                            source: "",
                            url: &url,
                            username: &username,
                            journal_path: &journal_path,
                            log: &mut log,
                            progress: &mut progress,
                            results: &mut results,
                            batcher: &mut batcher,
                        });
                        if !cfg.continue_on_error {
                            aborted = true;
                            stop_submitting = true;
                            break;
                        }
                        next_consume += 1;
                        continue;
                    }
                };

                assets_uploaded += prepared.assets_uploaded;
                assets_skipped += prepared.assets_skipped;
                assets_bytes += prepared.assets_bytes;
                for line in &prepared.log_lines {
                    log.line(line);
                }

                if pending
                    .as_ref()
                    .is_some_and(|batch| batch.source != prepared.source)
                {
                    let request_ok = {
                        let mut guard = shared_journal.lock().expect("journal mutex poisoned");
                        flush_import_pipeline(FlushImportPipeline {
                            cfg,
                            http: &http,
                            url: &url,
                            username: &username,
                            pending: &mut pending,
                            inflight: &mut inflight,
                            first_import: &mut first_import,
                            trackers: &mut trackers,
                            journal: &mut guard.state,
                            journal_path: &journal_path,
                            log: &mut log,
                            progress: &mut progress,
                            results: &mut results,
                            batcher: &mut batcher,
                            import_id,
                            wait: !cfg.continue_on_error,
                        })?
                    };
                    if !request_ok && !cfg.continue_on_error {
                        aborted = true;
                        stop_submitting = true;
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
                        let request_ok = {
                            let mut guard =
                                shared_journal.lock().expect("journal mutex poisoned");
                            flush_import_pipeline(FlushImportPipeline {
                                cfg,
                                http: &http,
                                url: &url,
                                username: &username,
                                pending: &mut pending,
                                inflight: &mut inflight,
                                first_import: &mut first_import,
                                trackers: &mut trackers,
                                journal: &mut guard.state,
                                journal_path: &journal_path,
                                log: &mut log,
                                progress: &mut progress,
                                results: &mut results,
                                batcher: &mut batcher,
                                import_id,
                                wait: !cfg.continue_on_error,
                            })?
                        };
                        if !request_ok && !cfg.continue_on_error {
                            aborted = true;
                            stop_submitting = true;
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
                    if batch.messages.len() >= batch_size || batch.body.len() >= MAX_IMPORT_BODY_BYTES
                    {
                        let request_ok = {
                            let mut guard =
                                shared_journal.lock().expect("journal mutex poisoned");
                            flush_import_pipeline(FlushImportPipeline {
                                cfg,
                                http: &http,
                                url: &url,
                                username: &username,
                                pending: &mut pending,
                                inflight: &mut inflight,
                                first_import: &mut first_import,
                                trackers: &mut trackers,
                                journal: &mut guard.state,
                                journal_path: &journal_path,
                                log: &mut log,
                                progress: &mut progress,
                                results: &mut results,
                                batcher: &mut batcher,
                                import_id,
                                wait: !cfg.continue_on_error,
                            })?
                        };
                        if !request_ok && !cfg.continue_on_error {
                            aborted = true;
                            stop_submitting = true;
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
                {
                    let mut guard = shared_journal.lock().expect("journal mutex poisoned");
                    finish_file_if_ready(FinishFile {
                        index: idx,
                        trackers: &mut trackers,
                        journal: &mut guard.state,
                        journal_path: &journal_path,
                        url: &url,
                        username: &username,
                        log: &mut log,
                        progress: &mut progress,
                        results: &mut results,
                        batcher: &mut batcher,
                    })?;
                }
                next_consume += 1;
                if stop_submitting {
                    break;
                }
            }
            if aborted || stop_submitting {
                break;
            }
        }

        // Tell every prepare worker to exit (`None` is the stop signal).
        for _ in 0..prepare_workers {
            let _ = job_tx.send(None);
        }
        // Pull any leftover prepare results so workers are not stuck sending.
        // We still count their asset stats even if we aborted the import loop.
        while let Ok(job) = result_rx.recv() {
            inflight_prepares = inflight_prepares.saturating_sub(1);
            if let Ok(prepared) = job.outcome {
                assets_uploaded += prepared.assets_uploaded;
                assets_skipped += prepared.assets_skipped;
                assets_bytes += prepared.assets_bytes;
                for line in &prepared.log_lines {
                    log.line(line);
                }
            }
        }
        let _ = inflight_prepares;
        Ok(())
    })?;

    if !aborted {
        // End of run: send any leftover pending batch and wait for the last import.
        let request_ok = {
            let mut guard = shared_journal.lock().expect("journal mutex poisoned");
            flush_import_pipeline(FlushImportPipeline {
                cfg,
                http: &http,
                url: &url,
                username: &username,
                pending: &mut pending,
                inflight: &mut inflight,
                first_import: &mut first_import,
                trackers: &mut trackers,
                journal: &mut guard.state,
                journal_path: &journal_path,
                log: &mut log,
                progress: &mut progress,
                results: &mut results,
                batcher: &mut batcher,
                import_id,
                wait: true,
            })?
        };
        if !request_ok && !cfg.continue_on_error {
            aborted = true;
        }
    } else {
        // Aborted: still wait for the in-flight import so the journal stays consistent.
        let mut guard = shared_journal.lock().expect("journal mutex poisoned");
        let _ = join_inflight_import(JoinInflightImport {
            inflight: &mut inflight,
            first_import: &mut first_import,
            trackers: &mut trackers,
            journal: &mut guard.state,
            journal_path: &journal_path,
            url: &url,
            username: &username,
            log: &mut log,
            progress: &mut progress,
            results: &mut results,
            batcher: &mut batcher,
        });
    }

    if let Some(line) = batcher.flush_remainder() {
        emit_progress_line(&mut log, &mut progress, line);
    }

    let journal = Arc::try_unwrap(shared_journal)
        .expect("prepare workers released journal")
        .into_inner()
        .expect("journal mutex poisoned")
        .state;

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
            assets_bytes,
            aborted,
            http: &http,
            import_id,
        },
        &mut progress,
        &mut log,
    )
}

/// One conversation handed to a prepare worker.
struct PrepareJob {
    idx: usize,
    path: PathBuf,
    name: String,
}

/// Result coming back from a prepare worker (may finish out of order).
struct PrepareJobResult {
    idx: usize,
    name: String,
    outcome: Result<PreparedFile>,
}

struct PrepareFileArgs<'a> {
    input: &'a Path,
    path: &'a Path,
    name: &'a str,
    cfg: &'a VaultPushConfig,
    http: &'a HttpSession,
    url: &'a str,
    username: &'a str,
    journal: &'a Mutex<SharedJournal>,
    journal_path: &'a Path,
    batch_size: usize,
    digest_cache: &'a DigestCache,
}

/// Output of preparing one conversation: uploaded media + message chunks ready to import.
struct PreparedFile {
    source: String,
    chunks: Vec<ImportChunk>,
    attachments: u64,
    profile: UploadProfile,
    total_started: Instant,
    assets_uploaded: u64,
    assets_skipped: u64,
    assets_bytes: u64,
    log_lines: Vec<String>,
}

/// One piece of an import request: NDJSON body bytes plus the message ids in it.
struct ImportChunk {
    body: Vec<u8>,
    messages: Vec<JournalMessage>,
}

/// Shared journal state for parallel prepare workers.
///
/// `assets_in_flight` stops two workers from uploading the same sha256 at once
/// when two chats share a file that is not in the journal yet.
#[derive(Debug)]
struct SharedJournal {
    state: JournalState,
    assets_in_flight: HashSet<String>,
}

/// Read one conversation JSONL, upload its attachments, split messages into import chunks.
///
/// This is the expensive per-chat step. Design choices:
/// - Collect **unique** attachment digests first, then upload each digest once
///   (a photo sent twice in the same chat should not be uploaded twice).
/// - Upload media **before** building message lines that reference those digests.
/// - Split messages into chunks sized for Cloudflare-safe import requests.
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
        digest_cache,
    } = args;

    let read_started = Instant::now();
    let doc = read_conversation_jsonl(path)?;
    let read_ms = elapsed_ms(read_started);
    let header = ConversationHeader::from_document(&doc);
    let source = project::validate_header(&header)?;
    let messages = &doc.messages;

    // For each message: list of (attachment index, sha256, file size).
    let mut per_message_digests: Vec<Vec<(usize, String, u64)>> =
        Vec::with_capacity(messages.len());
    let mut attachment_count = 0u64;
    let mut assets_uploaded = 0u64;
    let mut assets_skipped = 0u64;
    let mut assets_bytes = 0u64;
    let mut log_lines = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut profile = UploadProfile {
        read_ms,
        ..UploadProfile::default()
    };
    let attachment_scan_hash_started = Instant::now();

    if cfg.skip_attachments {
        // Count attachments as skipped but do not upload or reference them.
        for msg in messages {
            let n = msg.attachments.len() as u64;
            attachment_count += n;
            assets_skipped += n;
            per_message_digests.push(Vec::new());
        }
        profile.attachment_scan_hash_ms = elapsed_ms(attachment_scan_hash_started);
    } else {
        // Map: sha256 → (relative path, mime). BTreeMap keeps a stable upload order.
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
                let file_len = std::fs::metadata(&abs)
                    .with_context(|| format!("{name}: stat attachment {rel}"))?
                    .len();
                let claimed = att
                    .digest_sha256
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let digest = resolve_attachment_digest(
                    &abs,
                    claimed,
                    att.size_bytes,
                    cfg.verify_digests,
                    cfg.trust_export,
                    digest_cache,
                    name,
                    rel,
                    &mut |msg| warnings.push(msg),
                )?;
                unique
                    .entry(digest.clone())
                    .or_insert_with(|| (rel.to_string(), att.mime_type.clone()));
                digests.push((att_i, digest, file_len));
            }
            per_message_digests.push(digests);
        }

        profile.attachment_scan_hash_ms = elapsed_ms(attachment_scan_hash_started);
        profile.unique_assets = u64::try_from(unique.len()).unwrap_or(u64::MAX);

        // Emit any warnings collected during verification.
        for warning in &warnings {
            log_lines.push(format!("WARN {warning}"));
        }

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
        })?;
        profile.asset_upload_ms = elapsed_ms(asset_upload_started);
        profile.asset_bytes = upload_stats.bytes;
        assets_uploaded = upload_stats.uploaded;
        assets_skipped = upload_stats.skipped;
        assets_bytes = upload_stats.bytes;
        log_lines = upload_stats.log_lines;
    }

    // Build import chunks: each chunk is "header line + many message lines" as NDJSON bytes.
    let header_line = project::document_header_line(&doc)?;
    let mut chunks = Vec::new();
    let mut chunk_body = header_line.clone();
    let mut chunk_messages: Vec<JournalMessage> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        let (line, guid) = if cfg.skip_attachments {
            project::message_line_without_attachments(msg)?
        } else {
            // Rewrite attachment fields to the digests we just uploaded.
            project::message_line(msg, &per_message_digests[i])?
        };
        if !cfg.force {
            let key = JournalState::message_key(name, &guid);
            let seen = journal
                .lock()
                .expect("journal mutex poisoned")
                .state
                .messages
                .contains(&key);
            if seen {
                // Already imported this message id on a previous successful push.
                continue;
            }
        }
        // A single message larger than the chunk limit cannot be split further.
        if line.len() > MAX_IMPORT_BODY_BYTES {
            bail!(
                "{name}: message {guid} encodes to {} bytes alone, which exceeds the \
                 {} MiB import chunk limit — cannot upload through Cloudflare safely",
                line.len(),
                MAX_IMPORT_BODY_BYTES / (1024 * 1024)
            );
        }
        // Start a new chunk when this message would blow the count or byte budget.
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
        assets_uploaded,
        assets_skipped,
        assets_bytes,
        log_lines,
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
    uploaded: u64,
    skipped: u64,
    log_lines: Vec<String>,
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
    journal: &'a Mutex<SharedJournal>,
    journal_path: &'a Path,
}

/// Try to reserve this sha256 for upload. Returns false if another worker
/// already uploaded it or is uploading it (unless `force`).
fn claim_asset_upload(journal: &Mutex<SharedJournal>, digest: &str, force: bool) -> bool {
    let mut guard = journal.lock().expect("journal mutex poisoned");
    if !force
        && (guard.state.assets.contains(digest) || guard.assets_in_flight.contains(digest))
    {
        return false;
    }
    guard.assets_in_flight.insert(digest.to_string());
    true
}

/// Clear the in-flight claim. On success, mark the digest as uploaded in the journal.
fn finish_asset_upload(
    journal: &Mutex<SharedJournal>,
    journal_path: &Path,
    url: &str,
    username: &str,
    source: &str,
    digest: &str,
    ok: bool,
) -> Result<()> {
    let mut guard = journal.lock().expect("journal mutex poisoned");
    guard.assets_in_flight.remove(digest);
    if !ok {
        return Ok(());
    }
    guard.state.assets.insert(digest.to_string());
    drop(guard);
    journal::append(
        journal_path,
        &JournalEvent::AssetOk {
            url: url.to_string(),
            username: username.to_string(),
            source: source.to_string(),
            sha256: digest.to_string(),
        },
    )?;
    Ok(())
}

/// Upload each unique attachment for one conversation (several workers in parallel).
///
/// For each digest we first ask the vault with HEAD "do you already have this?".
/// If yes, we skip the PUT. That makes re-runs and shared media much faster than
/// always re-sending file bytes.
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
    } = args;
    let mut jobs = Vec::with_capacity(unique.len());
    let mut stats = AssetUploadStats::default();
    // Build the work list: skip digests already in the journal / claimed by another worker.
    for (digest, (rel, mime)) in unique {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        if !claim_asset_upload(journal, digest, cfg.force) {
            stats.skipped += 1;
            continue;
        }
        let path = match resolve_attachment(input, rel) {
            Some(path) => path,
            None => {
                let _ = finish_asset_upload(journal, journal_path, url, username, source, digest, false);
                bail!("{name}: missing attachment {rel}");
            }
        };
        let file_len = match fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(error) => {
                let _ = finish_asset_upload(journal, journal_path, url, username, source, digest, false);
                return Err(error).with_context(|| format!("stat {}", path.display()));
            }
        };
        if file_len > cfg.asset_max_bytes {
            let _ = finish_asset_upload(journal, journal_path, url, username, source, digest, false);
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

    // Work-stealing style: workers pull the next job index from a shared counter.
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
                                // Cheap existence check before sending file bytes.
                                if let Some(existing) = http.head_asset(
                                    url,
                                    &cfg.key,
                                    username,
                                    source,
                                    &job.digest,
                                )? {
                                    return Ok(existing);
                                }
                                // PUT (or multipart for large files) sends the bytes.
                                // The URL includes the sha256; the server re-hashes and
                                // rejects the upload if the bytes do not match.
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

    // Apply journal updates in a stable order after all workers finish.
    let mut results = results.into_inner().expect("asset result mutex poisoned");
    for result in results.drain(..) {
        let result = result.expect("every asset job has a result");
        match result {
            Ok(uploaded) => {
                finish_asset_upload(
                    journal,
                    journal_path,
                    url,
                    username,
                    source,
                    &uploaded.digest,
                    true,
                )?;
                if uploaded.response.already_present {
                    stats.skipped += 1;
                } else {
                    stats.uploaded += 1;
                }
                stats.log_lines.push(format!(
                    "asset {} {}",
                    if uploaded.response.already_present {
                        "skip"
                    } else {
                        "ok"
                    },
                    uploaded.digest
                ));
            }
            Err(error) => {
                // Release every in-flight claim so a retry is not stuck forever.
                for job in &jobs {
                    let _ =
                        finish_asset_upload(journal, journal_path, url, username, source, &job.digest, false);
                }
                bail!("{name}: {error}");
            }
        }
    }
    Ok(stats)
}

/// Tracks one conversation from "prepared" until all its import chunks succeed or fail.
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

    /// Append one prepared chunk onto this batch (body bytes + message ids).
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

/// True if adding `chunk` would exceed the message count or byte size limit.
///
/// In that case the caller should send the current batch first, then start a new one.
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
    name: &'a str,
    error: &'a str,
    source: &'a str,
    url: &'a str,
    username: &'a str,
    journal_path: &'a Path,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
    batcher: &'a mut ProgressBatcher,
}

/// Mark one conversation failed during prepare (before any import chunks were queued).
fn record_file_failure(args: RecordFileFailure<'_, '_, '_>) {
    // Flush any pending "files N/M" success line so failure text is not mixed into it.
    if let Some(line) = args.batcher.flush_remainder() {
        emit_progress_line(args.log, args.progress, line);
    }
    args.batcher.note_failed();
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
    emit_file_failure_lines(args.log, args.progress, args.name, args.error, None);
    if let Some(cb) = args.progress.as_mut() {
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
    trackers: &'a mut [Option<FileTracker>],
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    url: &'a str,
    username: &'a str,
    log: &'a mut LogWriter,
    progress: &'a mut Option<&'p mut ProgressFn<'f>>,
    results: &'a mut [Option<FileResult>],
    batcher: &'a mut ProgressBatcher,
}

/// If this conversation has no remaining import chunks (or already failed), write its result.
///
/// A chat can be "queue complete" (all chunks handed to the import pipeline) while
/// some HTTP imports are still in flight. We only mark the file done when the last
/// outstanding message count hits zero, or when a hard failure was recorded.
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

    if let Some(error) = error.as_ref() {
        if let Some(line) = args.batcher.flush_remainder() {
            emit_progress_line(args.log, args.progress, line);
        }
        args.batcher.note_failed();
        emit_file_failure_lines(
            args.log,
            args.progress,
            &name,
            error,
            Some(&profile),
        );
    } else {
        // Keep quiet per-file detail in the on-disk log only.
        args.log.line(&format!(
            "ok {name} msgs={result_messages} attachments={attachments}"
        ));
        args.log.line(&format_profile_line(&name, &profile));
        if let Some(line) = args.batcher.note_ok(result_messages, &profile) {
            emit_progress_line(args.log, args.progress, line);
        }
    }

    if let Some(cb) = args.progress.as_mut() {
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
    batcher: &'a mut ProgressBatcher,
    /// When true, wait for the newly spawned import (also used at end-of-run).
    wait: bool,
    import_id: Option<i64>,
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
    batcher: &'a mut ProgressBatcher,
}

/// Finish the current in-flight import (if any), then start the pending batch (if any).
///
/// `wait = false` means: start the HTTP request on a background thread and return
/// so the caller can keep preparing more chats. That overlap is a major reason
/// large imports feel faster than "upload everything, then import everything".
///
/// `wait = true` means: block until this import finishes (used at end of run or
/// when we must not continue on error).
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
        batcher: args.batcher,
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
        import_id: args.import_id,
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
            batcher: args.batcher,
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
    import_id: Option<i64>,
}

/// Start one message-import HTTP request on a background thread and return immediately.
///
/// Running the POST off the main thread lets prepare workers keep hashing/uploading
/// attachments while we wait on the network. Only one import is in flight at a time.
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
            import_id,
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
                import_id,
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

/// Wait for the background import thread (if any) and apply its success or failure.
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
        batcher: args.batcher,
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
    batcher: &'a mut ProgressBatcher,
}

/// Update journal + per-file trackers after one import HTTP request finishes.
///
/// On success: record each message id so a later push can skip them.
/// On failure: mark every conversation that contributed to this batch as failed.
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
            let n = represented.len().max(1) as u64;
            let share = request_ms / n;
            let rem = request_ms % n;
            for (i, index) in represented.into_iter().enumerate() {
                if let Some(tracker) = args.trackers[index].as_mut() {
                    // One HTTP request may cover several conversations; split the
                    // duration so progress "import time" is not multiplied by N.
                    let add = share + if i == 0 { rem } else { 0 };
                    tracker.profile.message_import_ms =
                        tracker.profile.message_import_ms.saturating_add(add);
                }
                finish_file_if_ready(FinishFile {
                    index,
                    trackers: args.trackers,
                    journal: args.journal,
                    journal_path: args.journal_path,
                    url: args.url,
                    username: args.username,
                    log: args.log,
                    progress: args.progress,
                    results: args.results,
                    batcher: args.batcher,
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
            let n = represented.len().max(1) as u64;
            let share = request_ms / n;
            let rem = request_ms % n;
            for (i, index) in represented.into_iter().enumerate() {
                let Some(tracker) = args.trackers[index].as_mut() else {
                    continue;
                };
                let add = share + if i == 0 { rem } else { 0 };
                tracker.profile.message_import_ms =
                    tracker.profile.message_import_ms.saturating_add(add);
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
                    trackers: args.trackers,
                    journal: args.journal,
                    journal_path: args.journal_path,
                    url: args.url,
                    username: args.username,
                    log: args.log,
                    progress: args.progress,
                    results: args.results,
                    batcher: args.batcher,
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

    #[test]
    fn progress_batcher_emits_every_ten_and_on_completion() {
        let mut batcher = ProgressBatcher::new(25);
        let profile = UploadProfile {
            message_import_ms: 3_300,
            total_ms: 5_500,
            asset_bytes: 700_000,
            ..UploadProfile::default()
        };
        let mut lines = Vec::new();
        for _ in 0..9 {
            assert!(batcher.note_ok(2, &profile).is_none());
        }
        let tenth = batcher.note_ok(2, &profile).unwrap();
        assert!(tenth.starts_with("files 10/25 - "));
        assert!(tenth.contains("conversations=10"));
        assert!(tenth.contains("messages=20"));
        assert!(tenth.contains("transfer size=7.0MB"));
        assert!(tenth.contains("import time=33.0s"));
        // total time is wall-clock for the progress window, not sum of profile.total_ms.
        assert!(tenth.contains("total time="));
        assert!(!tenth.contains("total time=55.0s"));
        assert!(!tenth.contains("bytes="));
        assert!(!tenth.contains("import_ms="));
        assert!(!tenth.contains("total_ms="));
        lines.push(tenth);
        for _ in 0..15 {
            if let Some(line) = batcher.note_ok(1, &profile) {
                lines.push(line);
            }
        }
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("files 20/25 - "));
        assert!(lines[1].contains("conversations=10"));
        assert!(lines[2].starts_with("files 25/25 - "));
        assert!(lines[2].contains("conversations=5"));
    }

    #[test]
    fn format_push_summary_is_multiline() {
        let report = PushReport {
            ok: true,
            account: "a".into(),
            username: "u".into(),
            mode: "append".into(),
            started_at: "t0".into(),
            finished_at: "t1".into(),
            elapsed_ms: 12_000,
            conversations_total: 10,
            conversations_ok: 8,
            conversations_failed: 1,
            conversations_skipped: 1,
            messages: 100,
            assets_uploaded: 4,
            assets_skipped: 2,
            assets_bytes: 1_048_576,
            results: vec![],
        };
        let summary = format_push_summary(&report);
        assert!(summary.contains("==== Summary ===="));
        assert!(summary.contains("Import success"));
        assert!(summary.contains("Conversations: 8 ok, 1 failed, 1 skipped (10 total)"));
        assert!(summary.contains("Messages: 100"));
        assert!(summary.contains("Assets: 4 uploaded, 2 skipped"));
        assert!(summary.contains("Elapsed: 12s (12000 ms)"));
        assert!(
            !summary.lines().any(|l| l.starts_with(' ')),
            "summary lines must not be indented"
        );
    }
}
