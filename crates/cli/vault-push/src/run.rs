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
//!   the import would fail. Media is uploaded before message text is sent.
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
//!   huge single uploads. Message batches are split, and large attachments use
//!   multipart, so a big chat or video does not hit that wall.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use message_ir::ConversationHeader;
use message_ir_format::read_conversation_jsonl;
use message_vault_io_core::{CancelFlag, check_cancel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AuthInfo;
use crate::http::{self, AssetPutRequest, CompleteImportArgs, HttpSession, PostImportArgs};
use crate::journal::{self, JournalEvent, JournalMessage, JournalState};
use crate::project;

/// How many messages to pack into one import HTTP request when size is not the limit.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;
/// Soft max size of one import request body (about 64 MiB).
///
/// Kept under Cloudflare's ~100 MiB upload cap so a large group chat is
/// split into several requests instead of one giant one that gets rejected.
pub const MAX_IMPORT_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Sentinel for "do not flush import batches on message count; size only".
///
/// Desktop import uses this so SMS-style short messages pack until
/// [`MAX_IMPORT_BODY_BYTES`] instead of stopping at a small count.
pub const NO_MESSAGE_COUNT_LIMIT: usize = usize::MAX;
/// Max size for uploading an attachment in a single HTTP PUT.
///
/// Bigger files use multipart upload (many smaller pieces), which proxies
/// accept more reliably than one huge body.
pub const MAX_PROXY_BODY_BYTES: usize = 90 * 1024 * 1024;
/// Refuse attachments larger than this (must match the vault server setting).
pub const DEFAULT_ASSET_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// How many attachment uploads may run at the same time.
pub const DEFAULT_ASSET_UPLOAD_WORKERS: usize = 8;
/// How many conversations may be prepared (read + upload media) ahead of the
/// import loop. Higher uses more memory/disk bandwidth; lower leaves the CPU
/// idle while waiting on the network.
pub const DEFAULT_PREPARE_AHEAD: usize = 3;
/// Worker threads that run [`prepare_file`] for that prepare-ahead queue.
pub const DEFAULT_PREPARE_WORKERS: usize = 2;

/// Shared map: absolute file path → sha256 hex string.
///
/// The same attachment file can appear in many chats. Caching the hash means
/// that file is read and hashed only once per push run.
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
    /// If false (default), trust a SHA-256 fingerprint already written in the
    /// JSON Lines file when it is present. That skips a slow full-file hash for
    /// every attachment. Files with an empty digest are still hashed. A path
    /// cache avoids hashing the same file twice when several chats share it.
    pub verify_digests: bool,
    /// If true, skip re-hashing attachments when the JSON Lines `size_bytes` matches
    /// the file size on disk. Default remains full verification of every file.
    pub trust_export: bool,
    pub max_retries: u32,
    pub batch_size: usize,
    /// Max parallel attachment uploads. Message imports stay one-at-a-time.
    pub asset_upload_workers: usize,
    /// Conversations to prepare (read + upload media) ahead of the import loop.
    pub prepare_ahead: usize,
    /// Worker threads that run [`prepare_file`] for that prepare-ahead queue.
    pub prepare_workers: usize,
    /// Files larger than this use multipart upload instead of one PUT.
    pub asset_multipart_threshold: usize,
    /// Hard max attachment size this run will attempt to upload.
    pub asset_max_bytes: u64,
    pub report_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub journal_path: Option<PathBuf>,
    pub cancel: Option<CancelFlag>,
    /// How the vault applies account contacts to import display names
    /// (`fill_missing` or `overwrite`).
    pub contact_name_mode: String,
    /// Existing import session to reuse when the caller already created one.
    pub import_id: Option<i64>,
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
/// These numbers help answer "why was this chat slow?" — reading JSON Lines,
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
    /// Messages placed in HTTP import request bodies.
    #[serde(default)]
    pub messages_attempted: u64,
    /// Messages the server inserted as new rows.
    #[serde(default)]
    pub messages_inserted: u64,
    /// Attempted messages the server reported as already present.
    #[serde(default)]
    pub messages_deduped: u64,
    /// Messages in HTTP requests that failed after all retries.
    #[serde(default)]
    pub messages_failed: u64,
    /// Legacy successful-request count. Equal to attempted minus failed.
    pub messages: u64,
    pub assets_uploaded: u64,
    pub assets_skipped: u64,
    pub assets_bytes: u64,
    pub results: Vec<FileResult>,
}

/// Running totals of messages attempted, inserted, already present, and failed.
#[derive(Debug, Clone, Copy, Default)]
struct MessageAccounting {
    attempted: u64,
    inserted: u64,
    deduped: u64,
    failed: u64,
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
    /// Structured skip/error for Import Errors (e.g. oversized attachment).
    Issue {
        kind: String,
        step: String,
        item: String,
        reason: String,
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

/// Three-way session status for `/v1/imports/{id}/complete` (import-session
/// spec, decisions 21–22). `failed` has a zero floor: aborted, or nothing
/// landed at all. A skip-only re-push is a no-op, not a failure. Item-level
/// failures beside successes are `completed_with_issues`.
pub fn outcome_status(report: &PushReport, aborted: bool) -> &'static str {
    let nothing_landed = report.conversations_total > 0
        && report.conversations_ok == 0
        && report.conversations_skipped == 0;
    if aborted || nothing_landed {
        return "failed";
    }
    if report.conversations_failed > 0 || report.messages_failed > 0 {
        return "completed_with_issues";
    }
    "completed"
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
Message accounting: {} attempted = {} new + {} deduped + {} failed\n\
Assets: {} uploaded, {} skipped\n\
Elapsed: {} ({} ms)",
        report.conversations_ok,
        report.conversations_failed,
        report.conversations_skipped,
        report.conversations_total,
        report.messages,
        report.messages_attempted,
        report.messages_inserted,
        report.messages_deduped,
        report.messages_failed,
        report.assets_uploaded,
        report.assets_skipped,
        format_duration_ms(report.elapsed_ms),
        report.elapsed_ms,
    )
}

/// How many finished conversations are grouped into one "files N/M …" log line.
/// Printing every single chat would flood the log on a big import.
const PROGRESS_BATCH_SIZE: usize = 10;
/// If the pending message batch is at least this many messages, start its HTTP
/// import now instead of waiting until the next chat is prepared.
///
/// Preparing the next chat may upload many attachments. Holding a large ready
/// batch until that finishes makes the UI look stuck and wastes time when the
/// network could already be importing.
const OVERLAP_FLUSH_MIN_MESSAGES: usize = 100;
/// Same idea as [`OVERLAP_FLUSH_MIN_MESSAGES`], but for batch body size in bytes.
const OVERLAP_FLUSH_MIN_BODY_BYTES: usize = 512 * 1024;

/// Collects successes and writes one progress line every [`PROGRESS_BATCH_SIZE`] files.
struct ProgressBatcher {
    total: usize,
    done: usize,
    chunk_conversations: u64,
    chunk_messages: u64,
    chunk_bytes: u64,
    chunk_import_ms: u64,
    /// Wall clock for the current progress chunk (first note until the line is written).
    chunk_started: Option<Instant>,
    chunk_count: usize,
}

impl ProgressBatcher {
    /// Start a batcher that writes a line when a chunk of successes is full.
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

    /// Start the chunk wall clock on the first success or skip in this window.
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

    /// Write any leftover partial batch at the end of the run.
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

/// Format a byte count as megabytes with one decimal place.
fn format_bytes_mb(bytes: u64) -> String {
    format!("{:.1}MB", bytes as f64 / 1_000_000.0)
}

/// Format a millisecond count as seconds with one decimal place.
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

/// Write Import Errors skip rows for attachments that were not uploaded.
fn emit_attachment_skips(
    log: &mut LogWriter,
    progress: &mut Option<&mut ProgressFn<'_>>,
    skips: &[AttachmentSkipIssue],
) {
    for skip in skips {
        let line = format!("skip {}: {}", skip.item, skip.reason);
        log.line(&line);
        if let Some(cb) = progress.as_mut() {
            cb(ProgressEvent::Log(line));
            cb(ProgressEvent::Issue {
                kind: "skip".into(),
                step: "upload".into(),
                item: skip.item.clone(),
                reason: skip.reason.clone(),
            });
        }
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

/// List conversation JSON Lines (`.jsonl`) files in `dir`, sorted, skipping journal/report/log.
///
/// # Errors
///
/// Returns an error when `dir` cannot be read.
fn list_jsonl_files(dir: &Path, exclude: &[&Path]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if is_conversation_jsonl(&path, exclude) {
            paths.push(path);
        }
    }
    // Stable order so progress "3/681" is repeatable across runs.
    paths.sort();
    Ok(paths)
}

/// True when `path` is a conversation JSON Lines file, not a push log or report.
fn is_conversation_jsonl(path: &Path, exclude: &[&Path]) -> bool {
    if exclude.contains(&path) {
        return false;
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if is_push_artifact(name) {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
}

/// Check that a SHA-256 fingerprint is exactly 64 hex digits; return lowercase form.
///
/// SHA-256 is a short fingerprint of the file bytes.
///
/// # Errors
///
/// Returns an error when the string is not 64 hexadecimal characters.
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
/// result is a fingerprint: same file bytes → same hex string. The file is
/// read in 64 KiB chunks so a large video does not have to sit entirely in RAM.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
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

/// Resolve the SHA-256 fingerprint for an attachment file.
///
/// SHA-256 is a short hex fingerprint of the file bytes. The default is to hash
/// every file from disk, compare against any JSON Lines claim, and warn on
/// mismatch (using the actual disk hash). Two flags alter this:
///
/// * `trust_export` — skip the hash when the JSON Lines `size_bytes` matches the
///   file size on disk (a cheap proxy for "file unchanged since export").
/// * `verify_digests` — hash from disk and **fail** on mismatch (no correction).
///
/// The vault server is the final verifier on upload; a stale fingerprint is
/// self-correcting (the server rejects mismatches).
///
/// # Errors
///
/// Returns an error when the file cannot be hashed, or when `verify_digests` is
/// on and the on-disk hash does not match the export claim.
struct ResolveAttachmentDigestArgs<'a> {
    abs: &'a Path,
    claimed_raw: Option<&'a str>,
    claimed_size: Option<u64>,
    verify_digests: bool,
    trust_export: bool,
    cache: &'a DigestCache,
    name: &'a str,
    rel: &'a str,
    warn: &'a mut dyn FnMut(String),
}

fn resolve_attachment_digest(args: ResolveAttachmentDigestArgs<'_>) -> Result<String> {
    let ResolveAttachmentDigestArgs {
        abs,
        claimed_raw,
        claimed_size,
        verify_digests,
        trust_export,
        cache,
        name,
        rel,
        warn,
    } = args;
    // Fast path: another conversation already hashed this absolute path
    // during this run. Always trust the cache — it was computed from disk.
    if let Some(digest) = cache
        .lock()
        .expect("digest cache mutex poisoned")
        .get(abs)
        .cloned()
    {
        return Ok(digest);
    }

    // Use the SHA-256 fingerprint claimed in the JSON Lines file, if it is valid.
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

    // When trust_export is on, skip hashing if the claimed size matches the file on disk.
    if trust_export
        && !verify_digests
        && let (Some(claimed_digest), Some(claimed_size)) = (claimed.as_deref(), claimed_size)
        && claimed_size == disk_size
    {
        remember_digest(cache, abs, claimed_digest);
        return Ok(claimed_digest.to_string());
    }

    // Hash from disk — the default path.
    let disk_digest = hash_file(abs).with_context(|| format!("{name}: hash {rel}"))?;

    // Compare the hash of the file on disk to the fingerprint in the JSON Lines file.
    if let Some(claimed_digest) = claimed.as_deref()
        && claimed_digest != disk_digest
    {
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

    remember_digest(cache, abs, &disk_digest);
    Ok(disk_digest)
}

/// Store one file's sha256 so other conversations sharing the file skip hashing.
fn remember_digest(cache: &DigestCache, abs: &Path, digest: &str) {
    cache
        .lock()
        .expect("digest cache mutex poisoned")
        .insert(abs.to_path_buf(), digest.to_string());
}

/// Turn an attachment path from a JSON Lines file into a real file path under the export folder.
fn resolve_attachment(export_root: &Path, rel: &str) -> Option<PathBuf> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let under = export_root.join(candidate);
    under.is_file().then_some(under)
}

/// Reject paths that could escape the export folder (absolute paths or `..`).
///
/// # Errors
///
/// Returns an error when the path is absolute or contains `..`.
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

/// Name an attachment that has no path, for an Import Errors row.
///
/// Falls back to the position in the message so two pathless attachments in one
/// conversation stay distinguishable.
fn attachment_label(att: &message_ir::IrAttachment, index: usize) -> String {
    att.original_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| format!("attachment {index}"), str::to_string)
}

/// Check the API key against the vault without importing any messages.
///
/// # Errors
///
/// Returns [`crate::AuthError`] when the URL is invalid, the host is unreachable,
/// or the key is rejected.
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
///
/// # Errors
///
/// Returns an error when the folder cannot be listed, the file cannot be read,
/// or the header is invalid.
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
///
/// # Errors
///
/// Returns an error when login fails, the folder cannot be listed, or an import
/// session cannot be started.
fn prepare_run_setup(
    cfg: &VaultPushConfig,
    progress: &mut Option<&mut ProgressFn<'_>>,
) -> Result<RunSetup> {
    // Accept either a folder or a file path; work always starts from the folder.
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
    // The API key decides which account this run uses. Prefer the username the server
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

    let source = detect_source(&input)?.unwrap_or_else(|| "unknown".to_string());
    // Best-effort: tell the vault "a new import run is starting" unless the
    // caller already created a session and wants this run to reuse it.
    let import_id = if let Some(import_id) = cfg.import_id {
        log.line(&format!(
            "using provided vault import session id={import_id}"
        ));
        if let Some(cb) = progress.as_mut() {
            cb(ProgressEvent::Log(format!(
                "Reusing import session {import_id} ({source})"
            )));
        }
        Some(import_id)
    } else {
        match http.start_import(
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
    message_accounting: MessageAccounting,
    aborted: bool,
    http: &'a HttpSession,
    import_id: Option<i64>,
}

/// Count successes/failures, write the report JSON, compact the journal, notify progress.
///
/// # Errors
///
/// Returns an error when the report cannot be written or the journal cannot be compacted.
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
        message_accounting,
        aborted,
        http,
        import_id,
    } = args;

    let results: Vec<FileResult> = results.into_iter().flatten().collect();
    let counted = count_file_results(&results);
    let ok_n = counted.ok;
    let fail_n = counted.failed;
    let skip_n = counted.skipped;
    let messages = counted.messages;
    let attachments = counted.attachments;
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
        messages_attempted: message_accounting.attempted,
        messages_inserted: message_accounting.inserted,
        messages_deduped: message_accounting.deduped,
        messages_failed: message_accounting.failed,
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
    if cfg.import_id.is_none()
        && let Some(import_id) = import_id
    {
        match http.complete_import(CompleteImportArgs {
            base_url: &url,
            key: &cfg.key,
            import_id,
            ok: report.ok,
            status: outcome_status(&report, aborted),
            message_count: report.messages,
            attachment_count: attachments,
            bytes_uploaded: assets_bytes,
        }) {
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

/// Totals derived from per-conversation [`FileResult`] rows.
struct FileResultCounts {
    ok: u64,
    failed: u64,
    skipped: u64,
    messages: u64,
    attachments: u64,
}

/// Count ok / failed / skipped conversations and sum messages and attachments.
fn count_file_results(results: &[FileResult]) -> FileResultCounts {
    let mut counted = FileResultCounts {
        ok: 0,
        failed: 0,
        skipped: 0,
        messages: 0,
        attachments: 0,
    };
    for result in results {
        match result.status.as_str() {
            "ok" => {
                counted.ok += 1;
                counted.messages += result.messages;
                counted.attachments += result.attachments;
            }
            "failed" => counted.failed += 1,
            "skipped" => counted.skipped += 1,
            _ => {}
        }
    }
    counted
}

/// Push every `.jsonl` conversation under `cfg.input`.
///
/// High-level flow:
/// 1. Setup (login, list files) via [`prepare_run_setup`].
/// 2. Start a few **prepare workers**. Each worker reads one chat file, uploads
///    its attachments, and builds message chunks. That work is slow (disk + network).
/// 3. The main thread **consumes prepare results in file order**, packs message
///    chunks into import batches, and sends those batches over HTTP.
/// 4. Message imports are mostly one-at-a-time, but an import can start while
///    prepare workers keep working on later chats (overlap for speed).
/// 5. [`finish_run`] writes the report and cleans up.
///
/// # Errors
///
/// Returns an error when setup fails, a worker disconnects, or the report cannot
/// be written. Per-conversation failures are recorded in the report when
/// `continue_on_error` is true.
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

    // One slot per conversation file; filled as each one finishes or is skipped.
    let mut results: Vec<Option<FileResult>> = vec![None; total];
    let mut assets_uploaded = 0u64;
    let mut assets_skipped = 0u64;
    let mut assets_bytes = 0u64;
    let mut message_accounting = MessageAccounting::default();
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

    // Send the queued message batch. Every call site passes the same borrows and
    // differs only in whether it waits for the HTTP request to finish, so this is
    // a macro rather than a closure: a closure would have to hold `trackers`,
    // `results`, and `log` borrowed for the whole loop, which the surrounding
    // code also needs to touch between flushes. Evaluates to `Result<bool>`,
    // where `false` means the request failed.
    macro_rules! flush_imports {
        (wait: $wait:expr) => {{
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
                message_accounting: &mut message_accounting,
                import_id,
                wait: $wait,
            })
        }};
    }

    // Bounded queue: at most `prepare_ahead` jobs waiting or running so hundreds
    // of chats are not prepared (and held in memory) before the import loop catches up.
    let prepare_ahead = cfg.prepare_ahead.max(1);
    let prepare_workers = cfg.prepare_workers.max(1).min(prepare_ahead);
    let probe_existing_assets = Arc::new(AtomicBool::new(false));
    let preflight_done = Arc::new(Mutex::new(false));
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
            let probe_existing_assets = Arc::clone(&probe_existing_assets);
            let preflight_done = Arc::clone(&preflight_done);
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
                        probe_existing: probe_existing_assets.as_ref(),
                        preflight_done: preflight_done.as_ref(),
                    });
                    let _ = result_tx.send(PrepareJobResult {
                        idx: job.idx,
                        name: job.name,
                        outcome,
                    });
                }
            });
        }
        // Drop this clone so workers' sends finish cleanly when they exit.
        drop(result_tx);

        // Submit conversations for prepare, and consume finished prepares in order.
        // Workers may finish out of order; `prepared_buf` holds early results until
        // that index is next (keeps import order stable for the journal).
        let mut next_submit = 0usize;
        let mut next_consume = 0usize;
        let mut inflight_prepares = 0usize;
        let mut prepared_buf: BTreeMap<usize, PrepareJobResult> = BTreeMap::new();
        let mut stop_submitting = false;

        while next_consume < total {
            // Cancel must still join in-flight import and write a report (abort path).
            if check_cancel(cfg.cancel.as_ref()).is_err() {
                aborted = true;
                break;
            }

            // If a large pending import batch is already ready, start its HTTP
            // request now (without waiting) so prepare workers keep the pipeline full.
            if pending.as_ref().is_some_and(|batch| {
                batch.messages.len() >= OVERLAP_FLUSH_MIN_MESSAGES
                    || batch.body.len() >= OVERLAP_FLUSH_MIN_BODY_BYTES
            }) {
                let request_ok = flush_imports!(wait: false)?;
                if !request_ok
                    && (check_cancel(cfg.cancel.as_ref()).is_err() || !cfg.continue_on_error)
                {
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
                                attachment_skips: Vec::new(),
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

            // Wait for the next prepare result if index `next_consume` is not buffered yet.
            if !prepared_buf.contains_key(&next_consume) {
                if inflight_prepares == 0 && next_submit >= total {
                    break;
                }
                let job = result_rx.recv().context("prepare worker disconnected")?;
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
                            let request_ok = flush_imports!(wait: true)?;
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
                emit_attachment_skips(&mut log, &mut progress, &prepared.attachment_skips);

                if pending
                    .as_ref()
                    .is_some_and(|batch| batch.source != prepared.source)
                {
                    let request_ok = flush_imports!(wait: !cfg.continue_on_error)?;
                    if !request_ok
                        && (check_cancel(cfg.cancel.as_ref()).is_err() || !cfg.continue_on_error)
                    {
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
                        let request_ok = flush_imports!(wait: !cfg.continue_on_error)?;
                        if !request_ok
                            && (check_cancel(cfg.cancel.as_ref()).is_err()
                                || !cfg.continue_on_error)
                        {
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
                    if batch.messages.len() >= batch_size
                        || batch.body.len() >= MAX_IMPORT_BODY_BYTES
                    {
                        let request_ok = flush_imports!(wait: !cfg.continue_on_error)?;
                        if !request_ok
                            && (check_cancel(cfg.cancel.as_ref()).is_err()
                                || !cfg.continue_on_error)
                        {
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
        // Their asset stats still count even if the import loop aborted.
        while let Ok(job) = result_rx.recv() {
            if let Ok(prepared) = job.outcome {
                assets_uploaded += prepared.assets_uploaded;
                assets_skipped += prepared.assets_skipped;
                assets_bytes += prepared.assets_bytes;
                for line in &prepared.log_lines {
                    log.line(line);
                }
                emit_attachment_skips(&mut log, &mut progress, &prepared.attachment_skips);
            }
        }
        Ok(())
    })?;

    if check_cancel(cfg.cancel.as_ref()).is_err() {
        aborted = true;
    }

    if !aborted {
        // End of run: send any leftover pending batch and wait for the last import.
        let request_ok = flush_imports!(wait: true)?;
        if !request_ok && (check_cancel(cfg.cancel.as_ref()).is_err() || !cfg.continue_on_error) {
            aborted = true;
        }
    }
    if aborted {
        // Aborted/cancelled: still wait for the in-flight import so the journal stays consistent.
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
            message_accounting: &mut message_accounting,
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
            message_accounting,
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
    probe_existing: &'a AtomicBool,
    preflight_done: &'a Mutex<bool>,
}

/// One attachment omitted from upload but kept as metadata on the message.
#[derive(Debug, Clone)]
struct AttachmentSkipIssue {
    item: String,
    reason: String,
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
    attachment_skips: Vec<AttachmentSkipIssue>,
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

/// Read one conversation JSON Lines file, upload its attachments, split messages into import chunks.
///
/// This is the expensive per-chat step. Design choices:
/// - Collect **unique** attachment digests first, then upload each digest once
///   (a photo sent twice in the same chat should not be uploaded twice).
/// - Upload media **before** building message lines that reference those digests.
/// - Split messages into chunks sized for Cloudflare-safe import requests.
///
/// # Errors
///
/// Returns an error when the file cannot be read, an attachment path is unsafe,
/// hashing fails, or an upload fails.
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
        probe_existing,
        preflight_done,
    } = args;

    let read_started = Instant::now();
    let doc = read_conversation_jsonl(path)?;
    let read_ms = elapsed_ms(read_started);
    let header = ConversationHeader::from_document(&doc);
    let source = project::validate_header(&header)?;
    let messages = &doc.messages;

    // For each message: how to map attachments onto the import JSON Lines line.
    let mut per_message_projections: Vec<Vec<project::AttachmentProjection>> =
        Vec::with_capacity(messages.len());
    let mut attachment_count = 0u64;
    let mut assets_uploaded = 0u64;
    let mut assets_skipped = 0u64;
    let mut assets_bytes = 0u64;
    let mut log_lines = Vec::new();
    let mut attachment_skips: Vec<AttachmentSkipIssue> = Vec::new();
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
            per_message_projections.push(Vec::new());
        }
        profile.attachment_scan_hash_ms = elapsed_ms(attachment_scan_hash_started);
    } else {
        // Map: sha256 → (relative path, mime). BTreeMap keeps a stable upload order.
        let mut unique: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
        let mut scan_skipped = 0u64;

        for msg in messages {
            let mut projections = Vec::new();
            for (att_i, att) in msg.attachments.iter().enumerate() {
                attachment_count += 1;
                let Some(rel) = att.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                    // No path means the bytes were never staged. "Do not copy"
                    // exports look like this, and the reason the exporter set
                    // ("skipped" / "embed_disabled") explains why. Keep the
                    // metadata so the thread still shows the file was there.
                    scan_skipped += 1;
                    let reason = att.missing_reason.as_deref().unwrap_or("no_path");
                    if att.missing_reason.is_none() {
                        // An exporter dropped the path without saying why. That
                        // is a defect, so it earns an Import Errors row; a
                        // deliberate skip does not.
                        attachment_skips.push(AttachmentSkipIssue {
                            item: format!("{name}:{}", attachment_label(att, att_i)),
                            reason: "attachment has no file path in the export".into(),
                        });
                    }
                    projections.push(project::AttachmentProjection::Missing {
                        index: att_i,
                        reason: reason.into(),
                        size: att.size_bytes,
                    });
                    continue;
                };
                safe_rel(rel)?;
                let Some(abs) = resolve_attachment(input, rel) else {
                    scan_skipped += 1;
                    attachment_skips.push(AttachmentSkipIssue {
                        item: format!("{name}:{rel}"),
                        reason: "attachment file not found on disk".into(),
                    });
                    projections.push(project::AttachmentProjection::Missing {
                        index: att_i,
                        reason: "file_missing".into(),
                        size: att.size_bytes,
                    });
                    continue;
                };
                let file_len = std::fs::metadata(&abs)
                    .with_context(|| format!("{name}: stat attachment {rel}"))?
                    .len();
                if file_len > cfg.asset_max_bytes {
                    scan_skipped += 1;
                    attachment_skips.push(AttachmentSkipIssue {
                        item: format!("{name}:{rel}"),
                        reason: format!(
                            "attachment is {} bytes ({} MiB), over the configured asset max of {} MiB",
                            file_len,
                            file_len / (1024 * 1024),
                            cfg.asset_max_bytes / (1024 * 1024)
                        ),
                    });
                    projections.push(project::AttachmentProjection::Missing {
                        index: att_i,
                        reason: "too_large".into(),
                        size: Some(file_len),
                    });
                    continue;
                }
                let claimed = att
                    .digest_sha256
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let digest = resolve_attachment_digest(ResolveAttachmentDigestArgs {
                    abs: &abs,
                    claimed_raw: claimed,
                    claimed_size: att.size_bytes,
                    verify_digests: cfg.verify_digests,
                    trust_export: cfg.trust_export,
                    cache: digest_cache,
                    name,
                    rel,
                    warn: &mut |msg| warnings.push(msg),
                })?;
                unique
                    .entry(digest.clone())
                    .or_insert_with(|| (rel.to_string(), att.mime_type.clone()));
                projections.push(project::AttachmentProjection::Digested {
                    index: att_i,
                    digest,
                    size: file_len,
                });
            }
            per_message_projections.push(projections);
        }

        profile.attachment_scan_hash_ms = elapsed_ms(attachment_scan_hash_started);
        profile.unique_assets = u64::try_from(unique.len()).unwrap_or(u64::MAX);

        // Write any warnings collected during verification.
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
            probe_existing,
            preflight_done,
        })?;
        profile.asset_upload_ms = elapsed_ms(asset_upload_started);
        profile.asset_bytes = upload_stats.bytes;
        assets_uploaded = upload_stats.uploaded;
        assets_skipped = upload_stats.skipped.saturating_add(scan_skipped);
        assets_bytes = upload_stats.bytes;
        log_lines.extend(upload_stats.log_lines);
    }

    // Build import chunks: each chunk is "header line + many message lines" as NDJSON bytes.
    let header_line = project::document_header_line(&doc)?;
    let mut chunks = Vec::new();
    let mut chunk_body = header_line.clone();
    let mut chunk_messages: Vec<JournalMessage> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        let (line, guid) = if cfg.skip_attachments {
            project::message_line_without_attachments(msg, i)?
        } else {
            // Rewrite attachment fields to uploaded digests or missing placeholders.
            project::message_line(msg, &per_message_projections[i], i)?
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
        attachment_skips,
    })
}

/// One attachment a worker should HEAD/PUT.
struct AssetUploadJob {
    digest: String,
    path: PathBuf,
    mime: Option<String>,
}

/// Outcome of one attachment upload (digest plus vault response).
struct AssetUploadResult {
    digest: String,
    response: http::AssetPutResponse,
}

#[derive(Default)]
/// Totals from uploading one conversation's unique attachments.
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
    probe_existing: &'a AtomicBool,
    preflight_done: &'a Mutex<bool>,
}

/// Try to reserve this sha256 for upload. Returns false if another worker
/// already uploaded it or is uploading it (unless `force`).
fn claim_asset_upload(journal: &Mutex<SharedJournal>, digest: &str, force: bool) -> bool {
    let mut guard = journal.lock().expect("journal mutex poisoned");
    if !force && (guard.state.assets.contains(digest) || guard.assets_in_flight.contains(digest)) {
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

/// One HEAD of the first queued digest for this run. If the vault already has
/// it, enable HEAD-skip so later files do not send PUT bodies.
fn preflight_existing_assets(
    http: &HttpSession,
    url: &str,
    key: &str,
    username: &str,
    source: &str,
    jobs: &[AssetUploadJob],
    probe_existing: &AtomicBool,
    preflight_done: &Mutex<bool>,
    max_retries: u32,
) -> Result<()> {
    if probe_existing.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut done = preflight_done.lock().expect("preflight mutex poisoned");
    if probe_existing.load(Ordering::Relaxed) || *done {
        return Ok(());
    }
    *done = true;
    let Some(job) = jobs.first() else {
        return Ok(());
    };
    let present = vault_http::with_retries(max_retries, || {
        http.head_asset(url, key, username, source, &job.digest)
    })?;
    if present.is_some() {
        probe_existing.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Upload each unique attachment for one conversation (several workers in parallel).
///
/// PUT first after one cheap HEAD of the first queued digest in this run.
///
/// If that HEAD reports `already_present`, later files HEAD and skip the body
/// (re-import). If it misses, this run PUTs until a response sets the flag.
/// Holding the preflight lock during that HEAD keeps parallel conversations
/// from PUTting duplicate bodies before the answer is known.
///
/// # Errors
///
/// Returns an error when HEAD/PUT fails after retries, or a worker panics.
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
        probe_existing,
        preflight_done,
    } = args;
    let mut jobs = Vec::with_capacity(unique.len());
    let mut stats = AssetUploadStats::default();
    // Give up a claim without marking the digest uploaded, so a retry (or another
    // conversation sharing the file) is free to try again.
    let release_claim = |digest: &str| {
        let _ = finish_asset_upload(journal, journal_path, url, username, source, digest, false);
    };
    // Build the work list: skip digests already in the journal / claimed by another worker.
    for (digest, (rel, mime)) in unique {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        if !claim_asset_upload(journal, digest, cfg.force) {
            stats.skipped += 1;
            continue;
        }
        let Some(path) = resolve_attachment(input, rel) else {
            release_claim(digest);
            bail!("{name}: missing attachment {rel}");
        };
        let file_len = match fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(error) => {
                release_claim(digest);
                return Err(error).with_context(|| format!("stat {}", path.display()));
            }
        };
        if file_len > cfg.asset_max_bytes {
            release_claim(digest);
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

    preflight_existing_assets(
        http,
        url,
        &cfg.key,
        username,
        source,
        &jobs,
        probe_existing,
        preflight_done,
        cfg.max_retries,
    )?;

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
                            vault_http::with_retries(cfg.max_retries, || {
                                if probe_existing.load(Ordering::Relaxed) {
                                    if let Some(existing) = http.head_asset(
                                        url,
                                        &cfg.key,
                                        username,
                                        source,
                                        &job.digest,
                                    )? {
                                        return Ok(existing);
                                    }
                                }
                                let response = http.put_asset(AssetPutRequest {
                                    base_url: url,
                                    key: &cfg.key,
                                    username,
                                    source,
                                    sha256: &job.digest,
                                    file: &job.path,
                                    mime: job.mime.as_deref(),
                                    multipart_threshold: cfg.asset_multipart_threshold,
                                })?;
                                if response.already_present {
                                    probe_existing.store(true, Ordering::Relaxed);
                                }
                                Ok(response)
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
                let outcome = if uploaded.response.already_present {
                    stats.skipped += 1;
                    "skip"
                } else {
                    stats.uploaded += 1;
                    "ok"
                };
                stats
                    .log_lines
                    .push(format!("asset {outcome} {}", uploaded.digest));
            }
            Err(error) => {
                // Release every in-flight claim so a retry is not stuck forever.
                for job in &jobs {
                    release_claim(&job.digest);
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

/// One message id in an import batch, tied back to its conversation file index.
struct BatchMessage {
    file_index: usize,
    journal: JournalMessage,
}

/// Messages from one backup source packed into a single import HTTP body.
struct ImportBatch {
    source: String,
    body: Vec<u8>,
    messages: Vec<BatchMessage>,
    conversations: usize,
}

impl ImportBatch {
    /// Empty batch that will hold messages from one backup source.
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
/// some HTTP imports are still in flight. The file is marked done only when the last
/// outstanding message count hits zero, or when a hard failure was recorded.
///
/// # Errors
///
/// Returns an error when a remaining import cannot be recorded or the journal
/// cannot be updated.
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
        emit_file_failure_lines(args.log, args.progress, &name, error, Some(&profile));
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

/// Background thread running one import HTTP POST.
struct InFlightImport {
    handle: JoinHandle<ImportHttpOutcome>,
}

/// Result of one import HTTP request, including timing and the batch that was sent.
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
    message_accounting: &'a mut MessageAccounting,
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
    message_accounting: &'a mut MessageAccounting,
}

/// Finish the current in-flight import (if any), then start the pending batch (if any).
///
/// `wait = false` means: start the HTTP request on a background thread and return
/// so the caller can keep preparing more chats. That overlap is a major reason
/// large imports feel faster than "upload everything, then import everything".
///
/// `wait = true` means: block until this import finishes (used at end of run or
/// when continuing after an error is not allowed).
///
/// # Errors
///
/// Returns an error when an import HTTP request fails and `continue_on_error`
/// is false, or when the journal cannot be updated.
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
        message_accounting: args.message_accounting,
    })?;
    if !ok && !args.cfg.continue_on_error {
        *args.pending = None;
        return Ok(false);
    }
    if args.pending.is_none() {
        return Ok(ok);
    }
    // Do not start a new import after cancel; leave pending unsent.
    if check_cancel(args.cfg.cancel.as_ref()).is_err() {
        return Ok(false);
    }
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
        contact_name_mode: args.cfg.contact_name_mode.clone(),
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
            message_accounting: args.message_accounting,
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
    contact_name_mode: String,
}

/// Start one message-import HTTP request on a background thread and return immediately.
///
/// Running the POST off the main thread lets prepare workers keep hashing and
/// uploading attachments during the network wait. Only one import is in flight
/// at a time.
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
            contact_name_mode,
        } = args;
        let request_started = Instant::now();
        let body_bytes = batch.body.len();
        let message_count = batch.messages.len();
        let response = vault_http::with_retries(max_retries, || {
            http.post_import(PostImportArgs {
                base_url: &url,
                key: &key,
                username: &username,
                source: &batch.source,
                mode: &mode,
                import_id,
                contact_name_mode: &contact_name_mode,
                ndjson: batch.body.clone(),
            })
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
///
/// # Errors
///
/// Returns an error when the worker thread panics, or when [`apply_import_outcome`]
/// fails.
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
        message_accounting: args.message_accounting,
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
    message_accounting: &'a mut MessageAccounting,
}

/// Update journal + per-file trackers after one import HTTP request finishes.
///
/// On success: record each message id so a later push can skip them.
/// On failure: mark every conversation that contributed to this batch as failed.
///
/// # Errors
///
/// Returns an error when a hard failure must stop the run (`continue_on_error`
/// is false) or when the journal cannot be updated.
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
    args.message_accounting.attempted = args
        .message_accounting
        .attempted
        .saturating_add(message_count as u64);
    // One HTTP request may cover several conversations, so split the duration
    // instead of charging the full request time to each one. The first
    // conversation absorbs the remainder.
    let conversation_count = represented.len().max(1) as u64;
    let share_ms = request_ms / conversation_count;
    let remainder_ms = request_ms % conversation_count;

    match response {
        Ok(response) => {
            args.message_accounting.inserted = args
                .message_accounting
                .inserted
                .saturating_add(response.messages_appended);
            args.message_accounting.deduped = args
                .message_accounting
                .deduped
                .saturating_add(response.messages_deduped);
            *args.first_import = false;
            journal::append(
                args.journal_path,
                &JournalEvent::MessageBatchOk {
                    url: args.url.to_string(),
                    username: args.username.to_string(),
                    source: batch.source.clone(),
                    messages: batch
                        .messages
                        .iter()
                        .map(|message| message.journal.clone())
                        .collect(),
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
            for (position, index) in represented.into_iter().enumerate() {
                if let Some(tracker) = args.trackers[index].as_mut() {
                    let add = share_ms + if position == 0 { remainder_ms } else { 0 };
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
            args.message_accounting.failed = args
                .message_accounting
                .failed
                .saturating_add(message_count as u64);
            let request_line = format!(
                "IMPORT_REQUEST fail source={} mode={mode} conversations={} messages={} \
                 bytes={body_bytes} elapsed_ms={request_ms} \
                 messages_per_second={messages_per_second:.1} mib_per_second={mebibytes_per_second:.2} \
                 error={error}",
                batch.source, batch.conversations, message_count,
            );
            args.log.line(&request_line);
            for (position, index) in represented.into_iter().enumerate() {
                let Some(tracker) = args.trackers[index].as_mut() else {
                    continue;
                };
                let add = share_ms + if position == 0 { remainder_ms } else { 0 };
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
    fn import_body_limit_is_64_mib() {
        assert_eq!(MAX_IMPORT_BODY_BYTES, 64 * 1024 * 1024);
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
    fn import_batch_does_not_flush_on_count_when_unlimited() {
        let mut batch = ImportBatch::new("imessage");
        batch.push(0, chunk(40, 2));
        assert!(
            !should_flush_before_chunk(&batch, &chunk(10, 50), NO_MESSAGE_COUNT_LIMIT, 1000),
            "desktop size-only flush must not split on message count"
        );
        assert!(should_flush_before_chunk(
            &batch,
            &chunk(70, 1),
            NO_MESSAGE_COUNT_LIMIT,
            100
        ));
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
    fn trust_export_skips_hash_when_size_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pic.bin");
        std::fs::write(&path, b"hello").unwrap();
        let claimed = "a".repeat(64);
        let cache: DigestCache = Mutex::new(HashMap::new());
        let mut warnings = Vec::new();

        let trusted = resolve_attachment_digest(ResolveAttachmentDigestArgs {
            abs: &path,
            claimed_raw: Some(&claimed),
            claimed_size: Some(5),
            verify_digests: false,
            trust_export: true,
            cache: &cache,
            name: "chat.jsonl",
            rel: "attachments/pic.bin",
            warn: &mut |msg| warnings.push(msg),
        })
        .unwrap();
        assert_eq!(
            trusted, claimed,
            "matching size_bytes must skip hashing and keep the export digest"
        );
        assert!(warnings.is_empty());

        let cache2: DigestCache = Mutex::new(HashMap::new());
        let mut warnings2 = Vec::new();
        let disk = resolve_attachment_digest(ResolveAttachmentDigestArgs {
            abs: &path,
            claimed_raw: Some(&claimed),
            claimed_size: Some(5),
            verify_digests: false,
            trust_export: false,
            cache: &cache2,
            name: "chat.jsonl",
            rel: "attachments/pic.bin",
            warn: &mut |msg| warnings2.push(msg),
        })
        .unwrap();
        let expected_disk = hex::encode(Sha256::digest(b"hello"));
        assert_eq!(disk, expected_disk);
        assert_ne!(disk, claimed);
        assert_eq!(warnings2.len(), 1);

        let cache3: DigestCache = Mutex::new(HashMap::new());
        let mut warnings3 = Vec::new();
        let size_mismatch = resolve_attachment_digest(ResolveAttachmentDigestArgs {
            abs: &path,
            claimed_raw: Some(&claimed),
            claimed_size: Some(4),
            verify_digests: false,
            trust_export: true,
            cache: &cache3,
            name: "chat.jsonl",
            rel: "attachments/pic.bin",
            warn: &mut |msg| warnings3.push(msg),
        })
        .unwrap();
        assert_eq!(
            size_mismatch, expected_disk,
            "trust_export must still hash when size_bytes does not match the file"
        );
        assert_ne!(size_mismatch, claimed);
        assert_eq!(warnings3.len(), 1);
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
            messages_attempted: 100,
            messages_inserted: 90,
            messages_deduped: 10,
            messages_failed: 0,
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
        assert!(
            summary.contains("Message accounting: 100 attempted = 90 new + 10 deduped + 0 failed")
        );
        assert!(summary.contains("Assets: 4 uploaded, 2 skipped"));
        assert!(summary.contains("Elapsed: 12s (12000 ms)"));
        assert!(
            !summary.lines().any(|l| l.starts_with(' ')),
            "summary lines must not be indented"
        );
    }

    fn sample_report() -> PushReport {
        PushReport {
            ok: true,
            account: "acct".into(),
            username: "user".into(),
            mode: "append".into(),
            started_at: "2026-08-29T00:00:00Z".into(),
            finished_at: "2026-08-29T00:01:00Z".into(),
            elapsed_ms: 60_000,
            conversations_total: 10,
            conversations_ok: 10,
            conversations_failed: 0,
            conversations_skipped: 0,
            messages_attempted: 100,
            messages_inserted: 90,
            messages_deduped: 10,
            messages_failed: 0,
            messages: 100,
            assets_uploaded: 5,
            assets_skipped: 0,
            assets_bytes: 1_000,
            results: Vec::new(),
        }
    }

    #[test]
    fn outcome_status_matches_the_spec_verdicts() {
        // Clean run.
        assert_eq!(outcome_status(&sample_report(), false), "completed");

        // Aborted is failed regardless of counts.
        assert_eq!(outcome_status(&sample_report(), true), "failed");

        // Nothing landed at all: the zero floor.
        let mut nothing = sample_report();
        nothing.ok = false;
        nothing.conversations_ok = 0;
        nothing.conversations_failed = 10;
        assert_eq!(outcome_status(&nothing, false), "failed");

        // A skip-only re-push is a no-op, not a failure.
        let mut skips = sample_report();
        skips.conversations_ok = 0;
        skips.conversations_skipped = 10;
        assert_eq!(outcome_status(&skips, false), "completed");

        // Item-level failures beside successes.
        let mut partial = sample_report();
        partial.ok = false;
        partial.conversations_ok = 8;
        partial.conversations_failed = 2;
        assert_eq!(outcome_status(&partial, false), "completed_with_issues");

        // Message failures inside ok conversations.
        let mut msgs = sample_report();
        msgs.messages_failed = 3;
        assert_eq!(outcome_status(&msgs, false), "completed_with_issues");
    }
}
