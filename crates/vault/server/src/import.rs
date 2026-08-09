use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use rusqlite::{
    Connection, OptionalExtension, Statement, Transaction, params, params_from_iter,
};
use serde::Serialize;
use tempfile::TempDir;

use crate::assets::{self, AssetStats, StoredAsset};
use crate::config::{PathsConfig, validate_source_id};
use crate::db::contacts;
use crate::db::schema;
use crate::import_media::{self, MediaMode};
use crate::jsonl;
use crate::models::{AttachmentRecord, ExportRecord, MessageRecord, clean_body};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Replace,
    Append,
}

/// How account contacts supply participant display names during import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContactNameMode {
    /// Use the vault contact name only when the import name is empty.
    #[default]
    FillMissing,
    /// Prefer the vault contact name whenever one exists for the handle.
    Overwrite,
}

impl ContactNameMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "fill_missing" | "fill-missing" => Ok(Self::FillMissing),
            "overwrite" => Ok(Self::Overwrite),
            other => bail!(
                "invalid contact_name_mode '{other}' (expected fill_missing or overwrite)"
            ),
        }
    }

    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FillMissing => "fill_missing",
            Self::Overwrite => "overwrite",
        }
    }
}

impl ImportMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "append" => Ok(Self::Append),
            other => bail!("invalid import mode '{other}' (expected replace or append)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Append => "append",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportOptions<'a> {
    /// Used by [`import_jsonl_files`] (CLI/tests). Warm HTTP path opens its own connection.
    #[allow(dead_code)]
    pub db_path: &'a Path,
    /// Content-addressed asset store when [`Self::source_from_jsonl`] is false.
    pub assets_dir: &'a Path,
    /// Root for resolving relative attachment paths in JSONL.
    pub asset_root: &'a Path,
    /// Optional address book to load: VCF or vCard CSV export.
    pub contacts: Option<&'a Path>,
    pub overwrite_contacts: bool,
    pub mode: ImportMode,
    /// Fixed source id (HTTP / `--source` override). Ignored when `source_from_jsonl`.
    pub source: &'a str,
    pub account_id: &'a str,
    /// Fill missing `content_key` values during promote (needed before cross-source dedupe).
    pub fill_content_keys: bool,
    /// Optional vault import session id (messages stamped on promote).
    pub import_id: Option<i64>,
    /// When true, stamp `messages.source` from each conversation's IR `export.source`.
    pub source_from_jsonl: bool,
    /// Required when `source_from_jsonl` to resolve per-source asset dirs.
    pub paths: Option<&'a PathsConfig>,
    pub media: MediaMode,
    /// When `source_from_jsonl` + Replace: wipe these sources before import.
    pub wipe_sources: Option<Vec<String>>,
    /// Apply vault contact preferred names to import `name_hint` values.
    pub contact_name_mode: ContactNameMode,
}

impl<'a> ImportOptions<'a> {
    /// HTTP / tests / reset-demo: fixed source + assets dir, copy media.
    #[allow(clippy::too_many_arguments)]
    pub fn fixed(
        db_path: &'a Path,
        assets_dir: &'a Path,
        asset_root: &'a Path,
        contacts: Option<&'a Path>,
        overwrite_contacts: bool,
        mode: ImportMode,
        source: &'a str,
        account_id: &'a str,
        fill_content_keys: bool,
        import_id: Option<i64>,
    ) -> Self {
        Self {
            db_path,
            assets_dir,
            asset_root,
            contacts,
            overwrite_contacts,
            mode,
            source,
            account_id,
            fill_content_keys,
            import_id,
            source_from_jsonl: false,
            paths: None,
            media: MediaMode::Copy,
            wipe_sources: None,
            contact_name_mode: ContactNameMode::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ImportStats {
    pub conversations: u64,
    pub participants: u64,
    pub messages: u64,
    pub attachments: u64,
    pub tapbacks: u64,
    pub files: u64,
    pub assets_copied: u64,
    pub assets_deduped: u64,
    pub assets_missing: u64,
    pub contacts: u64,
    pub contact_handles: u64,
    pub contact_label_links: u64,
    pub contacts_skipped: bool,
    pub messages_deduped: u64,
    pub messages_appended: u64,
    pub mode: String,
    /// Flagged phone handles (ambiguous; review note set) inserted by this import.
    pub phones_needing_review: u64,
}

impl ImportStats {
    fn merge_file(&mut self, other: &ImportStats) {
        self.conversations += other.conversations;
        self.participants += other.participants;
        self.messages += other.messages;
        self.attachments += other.attachments;
        self.tapbacks += other.tapbacks;
        self.messages_deduped += other.messages_deduped;
        self.phones_needing_review += other.phones_needing_review;
    }
}

struct PreparedAttachment {
    record: AttachmentRecord,
    stored: Option<StoredAsset>,
}

/// Import every `*.jsonl` file under `export_dir` (CLI staging path).
#[allow(clippy::too_many_arguments)]
pub fn import_export(
    export_dir: &Path,
    db_path: &Path,
    assets_dir: &Path,
    contacts: Option<&Path>,
    overwrite_contacts: bool,
    mode: ImportMode,
    source: &str,
    account_id: &str,
) -> Result<ImportStats> {
    if !export_dir.is_dir() {
        bail!("export directory does not exist: {}", export_dir.display());
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(export_dir)
        .with_context(|| format!("failed to read {}", export_dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        })
        .collect();
    paths.sort();

    let mut conn = Connection::open(db_path)
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    schema::configure_connection(&conn)?;
    schema::ensure_vault_schema(&conn)?;
    crate::db::account_profile::ensure_account_row(&conn, account_id)?;

    let import_id = crate::db::vault_imports::start_import(
        &conn,
        account_id,
        source,
        mode.as_str(),
        Some("message-vault-server"),
    )?;

    let result = import_jsonl_files_on_conn(
        &mut conn,
        &paths,
        &ImportOptions::fixed(
            db_path,
            assets_dir,
            export_dir,
            contacts,
            overwrite_contacts,
            mode,
            source,
            account_id,
            true,
            Some(import_id),
        ),
        ImportSchemaMode::AssumeReady,
    );

    match &result {
        Ok(stats) => {
            if let Err(e) = crate::db::vault_imports::complete_import(
                &conn,
                account_id,
                import_id,
                &crate::db::vault_imports::CompleteImportArgs {
                    ok: true,
                    message_count: Some(stats.messages as i64),
                    attachment_count: Some(stats.attachments as i64),
                    bytes_uploaded: None,
                },
            ) {
                eprintln!("warning: complete_import({import_id}) failed: {e}");
            }
        }
        Err(_) => {
            if let Err(e) = crate::db::vault_imports::complete_import(
                &conn,
                account_id,
                import_id,
                &crate::db::vault_imports::CompleteImportArgs {
                    ok: false,
                    message_count: None,
                    attachment_count: None,
                    bytes_uploaded: None,
                },
            ) {
                eprintln!("warning: complete_import({import_id}) failed: {e}");
            }
        }
    }

    result
}

/// Whether import should run DDL/schema ensure on the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSchemaMode {
    /// CLI / one-shot: ensure vault + messages schema.
    Ensure,
    /// HTTP serve hot path: schema already ensured on the warm connection.
    AssumeReady,
}

/// Import one or more JSONL files. Attachment relative paths resolve against `opts.asset_root`.
#[allow(dead_code)] // CLI/tests; HTTP serve uses [`import_jsonl_files_on_conn`]
pub fn import_jsonl_files(paths: &[PathBuf], opts: &ImportOptions<'_>) -> Result<ImportStats> {
    validate_import_options(opts)?;

    if let Some(parent) = opts.db_path.parent()
        && !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

    let mut conn = Connection::open(opts.db_path)
        .with_context(|| format!("failed to open database {}", opts.db_path.display()))?;
    schema::configure_connection(&conn)?;
    println!("  sql:      opened {}", opts.db_path.display());
    let _ = io::stdout().flush();
    import_jsonl_files_on_conn(&mut conn, paths, opts, ImportSchemaMode::Ensure)
}

fn validate_import_options(opts: &ImportOptions<'_>) -> Result<()> {
    if opts.source_from_jsonl {
        if opts.paths.is_none() {
            bail!("source_from_jsonl requires config paths for per-source assets");
        }
    } else if opts.source.trim().is_empty() {
        bail!("import source id must not be empty");
    }
    Ok(())
}

/// Import onto an existing connection (warm serve path or tests).
pub fn import_jsonl_files_on_conn(
    conn: &mut Connection,
    paths: &[PathBuf],
    opts: &ImportOptions<'_>,
    schema_mode: ImportSchemaMode,
) -> Result<ImportStats> {
    validate_import_options(opts)?;
    if !opts.source_from_jsonl {
        fs::create_dir_all(opts.assets_dir)
            .with_context(|| format!("failed to create {}", opts.assets_dir.display()))?;
    }

    if schema_mode == ImportSchemaMode::Ensure {
        schema::ensure_vault_schema(conn)?;
    }
    crate::db::account_profile::ensure_account_row(conn, opts.account_id)?;

    if let Some(path) = opts.contacts {
        println!("  sql:      loading contacts from {}…", path.display());
    } else {
        println!("  sql:      contacts load skipped (no --contacts address book)");
    }
    let _ = io::stdout().flush();
    let contact_stats = contacts::load_contacts_if_needed(
        conn,
        opts.contacts,
        opts.overwrite_contacts,
        opts.account_id,
    )?;
    if contact_stats.skipped {
        println!("  sql:      contacts skipped (already loaded or no address book)");
    } else {
        println!(
            "  sql:      contacts={} phones={} labels={}",
            contact_stats.contacts, contact_stats.phones, contact_stats.labels
        );
    }
    if schema_mode == ImportSchemaMode::Ensure {
        println!("  sql:      ensuring schema + resetting staging for account…");
        let _ = io::stdout().flush();
        schema::ensure_messages_schema(conn)?;
    } else {
        println!("  sql:      resetting staging for account…");
        let _ = io::stdout().flush();
    }
    schema::reset_staging_for_account(conn, opts.account_id)?;
    if opts.mode == ImportMode::Replace {
        let wipe: Vec<String> = if opts.source_from_jsonl {
            opts.wipe_sources.clone().unwrap_or_default()
        } else {
            vec![opts.source.to_string()]
        };
        for source in &wipe {
            validate_source_id(source)?;
            println!("  sql:      deleting existing messages for source '{source}'…");
            let _ = io::stdout().flush();
            schema::delete_messages_for_source(conn, opts.account_id, source)?;
        }
        println!("  sql:      wipe complete");
    }
    let _ = io::stdout().flush();

    let total_files = paths.len();
    println!(
        "  import:   {} JSONL file{}",
        total_files,
        if total_files == 1 { "" } else { "s" }
    );
    if opts.mode == ImportMode::Replace {
        if opts.source_from_jsonl {
            let wiped = opts
                .wipe_sources
                .as_ref()
                .map(|s| s.join(", "))
                .unwrap_or_default();
            println!("  import:   wiped existing rows for source(s) '{wiped}'");
        } else {
            println!(
                "  import:   wiped existing rows for source '{}'",
                opts.source
            );
        }
    }
    let _ = io::stdout().flush();

    let media_work = TempDir::new().context("temp dir for import-time media rewrite")?;

    let mut stats = ImportStats {
        contacts: contact_stats.contacts,
        contact_handles: contact_stats.phones,
        contact_label_links: contact_stats.labels,
        contacts_skipped: contact_stats.skipped,
        phones_needing_review: contact_stats.phones_needing_review,
        mode: opts.mode.as_str().to_string(),
        ..Default::default()
    };
    let mut asset_stats = AssetStats::default();
    let started = Instant::now();
    let progress_every = if total_files <= 20 {
        1usize
    } else {
        (total_files / 40).max(10)
    };
    const STAGING_COMMIT_EVERY: usize = 50;

    let mut tx = conn.transaction()?;
    let mut stmts = StagingInserts::prepare(&tx, opts.account_id, opts.import_id)?;

    for (idx, path) in paths.iter().enumerate() {
        let file_stats = import_file_to_staging(
            &tx,
            &mut stmts,
            opts,
            path,
            &mut asset_stats,
            media_work.path(),
        )?;
        stats.merge_file(&file_stats);
        stats.files += 1;

        let n = idx + 1;
        if n == 1 || n == total_files || n % progress_every == 0 {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            println!(
                "  import:   [{n}/{total_files}] {name}  msgs={} attachments={} assets_copied={} missing={}  ({:.0}s)",
                stats.messages,
                stats.attachments,
                asset_stats.copied,
                asset_stats.missing,
                started.elapsed().as_secs_f64()
            );
            let _ = io::stdout().flush();
        }

        if n % STAGING_COMMIT_EVERY == 0 && n < total_files {
            drop(stmts);
            tx.commit()?;
            tx = conn.transaction()?;
            stmts = StagingInserts::prepare(&tx, opts.account_id, opts.import_id)?;
        }
    }
    drop(stmts);
    tx.commit()?;

    println!(
        "  import:   promoting staging → production ({:.0}s so far)…",
        started.elapsed().as_secs_f64()
    );
    let _ = io::stdout().flush();
    let promote_stats = promote_append(
        conn,
        opts.mode,
        opts.account_id,
        opts.fill_content_keys,
    )?;
    stats.messages_deduped += promote_stats.messages_deduped;
    stats.messages_appended = promote_stats.messages_appended;
    if opts.mode == ImportMode::Append {
        stats.conversations = promote_stats.conversations;
        stats.participants = promote_stats.participants;
        stats.messages = promote_stats.messages;
        stats.attachments = promote_stats.attachments;
        stats.tapbacks = promote_stats.tapbacks;
    }

    schema::clear_staging_for_account(conn, opts.account_id)?;

    stats.assets_copied = asset_stats.copied;
    stats.assets_deduped = asset_stats.deduped;
    stats.assets_missing = asset_stats.missing;

    println!(
        "  import:   finished in {:.1}s  files={} msgs={} attachments={} assets_copied={}",
        started.elapsed().as_secs_f64(),
        stats.files,
        stats.messages,
        stats.attachments,
        stats.assets_copied
    );

    Ok(stats)
}

fn prepare_attachments(
    export_dir: &Path,
    assets_dir: &Path,
    attachments: Vec<AttachmentRecord>,
    asset_stats: &mut AssetStats,
    media: MediaMode,
    media_work: &Path,
) -> Result<Vec<PreparedAttachment>> {
    if media == MediaMode::None {
        return Ok(Vec::new());
    }

    let mut prepared = Vec::with_capacity(attachments.len());
    for mut att in attachments {
        // Import-time convert/compress: rewrite file before hash/store so digests match bytes.
        if matches!(media, MediaMode::Convert | MediaMode::Compress)
            && let Some(rel) = att.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                let source = export_dir.join(rel);
                if source.is_file()
                    && let Some(resolved) = import_media::resolve_for_store(
                        &source,
                        att.mime_type.as_deref(),
                        media,
                        media_work,
                    )? {
                        // Store rewritten bytes; drop claimed sha (bytes may have changed).
                        att.sha256 = None;
                        att.mime_type = resolved.mime_type.or(att.mime_type.take());
                        let stored = assets::hash_and_store(
                            &resolved.path,
                            assets_dir,
                            att.mime_type.as_deref(),
                            asset_stats,
                        )?;
                        prepared.push(PreparedAttachment {
                            record: att,
                            stored,
                        });
                        continue;
                    }
            }

        let stored = if let Some(sha) = att
            .sha256
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(found) = assets::lookup_by_sha256(assets_dir, sha) {
                asset_stats.deduped += 1;
                Some(StoredAsset {
                    mime_type: att.mime_type.clone().or(found.mime_type),
                    ..found
                })
            } else if let Some(rel) = att.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                // Digest claimed but not pre-uploaded: fall back to path under asset_root.
                let source = export_dir.join(rel);
                match assets::store_verified(
                    &source,
                    sha,
                    assets_dir,
                    att.mime_type.as_deref(),
                    false,
                    false,
                ) {
                    Ok((stored, already)) => {
                        if already {
                            asset_stats.deduped += 1;
                        } else {
                            asset_stats.copied += 1;
                        }
                        Some(stored)
                    }
                    Err(_) if !source.is_file() => {
                        asset_stats.missing += 1;
                        None
                    }
                    Err(e) => return Err(e),
                }
            } else {
                asset_stats.missing += 1;
                None
            }
        } else if let Some(rel) = att.path.as_deref() {
            assets::hash_and_store(
                &export_dir.join(rel),
                assets_dir,
                att.mime_type.as_deref(),
                asset_stats,
            )?
        } else {
            asset_stats.missing += 1;
            None
        };
        prepared.push(PreparedAttachment {
            record: att,
            stored,
        });
    }
    Ok(prepared)
}

/// Canonical form of a handle for identity matching, per type, plus a
/// human-readable note when the canonical form is ambiguous (guarded policy).
///
/// Phone: E.164 when the raw is unambiguous (`+`-prefixed, or a US national
/// number); otherwise digits-as-is with a review note — a trunk-zero
/// `020 7946 0000` becomes `02079460000` flagged, never `+02079460000`.
/// Email: lowercased. Username/Other: verbatim (trimmed).
fn normalize_handle(raw: &str, handle_type: HandleType) -> (String, Option<String>) {
    match handle_type {
        HandleType::Phone => {
            let guarded = phone::normalize_guarded(raw, phone::PhoneRegion::for_raw(raw));
            if guarded.normalized.is_empty() {
                // No usable digits: fall back to the raw, unflagged.
                (raw.trim().to_string(), None)
            } else {
                (guarded.normalized, guarded.note)
            }
        }
        HandleType::Email => (raw.trim().to_lowercase(), None),
        HandleType::Username | HandleType::Other => (raw.trim().to_string(), None),
    }
}

/// Infer a handle type from the handle's shape when the source does not say.
///
/// Mirrors the shared rule in message-ir-format: `@` → Email; digit-heavy
/// phone-shaped strings → Phone (covers SMS/iMessage/WhatsApp numbers);
/// anything else (Discord usernames, group chat ids) → Other.
fn infer_handle_type(handle: &str) -> HandleType {
    let h = handle.trim();
    if h.contains('@') {
        return HandleType::Email;
    }
    let has_digit = h.bytes().any(|b| b.is_ascii_digit());
    let all_phone_chars = h.bytes().all(|b| {
        b.is_ascii_digit() || matches!(b, b'+' | b'-' | b' ' | b'(' | b')' | b'.' | b'#' | b'*')
    });
    if !h.is_empty() && has_digit && all_phone_chars {
        return HandleType::Phone;
    }
    HandleType::Other
}

/// Resolve or create a handle row. Returns the handle id and whether this call
/// newly inserted a flagged (review-note) row.
fn resolve_handle(
    conn: &Connection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
    service: Option<&str>,
) -> Result<(i64, bool)> {
    let (normalized, note) = normalize_handle(raw, handle_type);
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO handles (account_id, raw, normalized, normalized_note, handle_type, service)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![account_id, raw, normalized, note, handle_type.as_str(), service],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM handles WHERE account_id = ?1 AND normalized = ?2 AND handle_type = ?3",
        params![account_id, normalized, handle_type.as_str()],
        |row| row.get(0),
    )?;
    Ok((id, inserted > 0 && note.is_some()))
}

/// Contact linked to a handle via `contact_handles`, if any.
fn contact_id_for_handle(
    conn: &Connection,
    account_id: &str,
    handle_id: i64,
) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT contact_id FROM contact_handles WHERE account_id = ?1 AND handle_id = ?2",
            params![account_id, handle_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn contact_preferred_name(
    conn: &Connection,
    account_id: &str,
    contact_id: i64,
) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT preferred_name FROM contacts WHERE account_id = ?1 AND id = ?2",
            params![account_id, contact_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty()))
}

/// Merge an import display name with a vault contact name per [`ContactNameMode`].
pub fn apply_contact_name_mode(
    mode: ContactNameMode,
    import_name: Option<String>,
    vault_name: Option<String>,
) -> Option<String> {
    let import_empty = import_name
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true);
    match mode {
        ContactNameMode::FillMissing => {
            if import_empty {
                vault_name.or(import_name)
            } else {
                import_name
            }
        }
        ContactNameMode::Overwrite => vault_name.or(import_name),
    }
}

struct StagingInserts<'conn> {
    account_id: String,
    import_id: Option<i64>,
    conv: Statement<'conn>,
    part: Statement<'conn>,
    msg: Statement<'conn>,
    att: Statement<'conn>,
    tap: Statement<'conn>,
}

impl<'conn> StagingInserts<'conn> {
    fn prepare(
        tx: &'conn Transaction<'_>,
        account_id: &str,
        import_id: Option<i64>,
    ) -> Result<Self> {
        Ok(Self {
            account_id: account_id.to_string(),
            import_id,
            conv: tx.prepare(
                r#"
                INSERT INTO staging_conversations (
                    account_id, chat_handle_id, service, conversation_type, group_title, exported_at, source_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )?,
            part: tx.prepare(
                r#"
                INSERT INTO staging_participants (conversation_id, handle_id, contact_id, name_hint)
                VALUES (?1, ?2, ?3, ?4)
                "#,
            )?,
            msg: tx.prepare(
                r#"
                INSERT OR IGNORE INTO staging_messages (
                    conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                    sender_handle_id, subject, body, is_announcement, is_reply, thread_originator_guid,
                    thread_originator_part, num_replies, sort_order, import_id
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
                )
                "#,
            )?,
            att: tx.prepare(
                r#"
                INSERT INTO staging_attachments (
                    message_id, path, original_name, mime_type, is_sticker, transcription,
                    sha256, assets_path, size_bytes
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )?,
            tap: tx.prepare(
                r#"
                INSERT INTO staging_tapbacks (
                    message_id, part_index, kind, emoji, is_from_me, sender_handle_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
            )?,
        })
    }
}

/// chat_identifier, service, conversation_type, group_title, exported_at, participants, source
/// Participants are (handle, name_hint, handle_type).
type ConversationHeader = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Vec<(String, Option<String>, Option<HandleType>)>,
    String,
);

fn resolve_conversation_source(
    opts: &ImportOptions<'_>,
    path: &Path,
    chat_identifier: &str,
    export_source: Option<&str>,
) -> Result<String> {
    if opts.source_from_jsonl {
        let Some(source) = export_source.map(str::trim).filter(|s| !s.is_empty()) else {
            bail!(
                "{}: conversation '{}' is missing export.source \
                 (required for CLI directory import)",
                path.display(),
                chat_identifier
            );
        };
        validate_source_id(source)?;
        Ok(source.to_string())
    } else {
        Ok(opts.source.to_string())
    }
}

fn assets_dir_for_source(opts: &ImportOptions<'_>, source: &str) -> Result<PathBuf> {
    if opts.source_from_jsonl {
        let paths = opts
            .paths
            .ok_or_else(|| anyhow::anyhow!("source_from_jsonl requires config paths"))?;
        let dir = paths.assets_dir_for_account(opts.account_id, source);
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        Ok(dir)
    } else {
        Ok(opts.assets_dir.to_path_buf())
    }
}

fn import_file_to_staging(
    tx: &Transaction<'_>,
    stmts: &mut StagingInserts<'_>,
    opts: &ImportOptions<'_>,
    path: &Path,
    asset_stats: &mut AssetStats,
    media_work: &Path,
) -> Result<ImportStats> {
    let source_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.jsonl")
        .to_string();
    // Demo (and docs) use orphaned.jsonl; older bundles used orphaned.json.
    let is_orphaned = path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("orphaned"));

    let records = jsonl::read_records(path)?;
    let mut stats = ImportStats::default();
    let mut pending: Option<ConversationHeader> = None;
    let mut messages: Vec<MessageRecord> = Vec::new();

    for record in records {
        match record {
            ExportRecord::Conversation(c) => {
                if let Some(header) = pending.take() {
                    stats.merge_file(&import_conversation_to_staging(
                        tx,
                        stmts,
                        opts,
                        &source_file,
                        header,
                        std::mem::take(&mut messages),
                        asset_stats,
                        media_work,
                    )?);
                }
                let source = resolve_conversation_source(
                    opts,
                    path,
                    &c.chat_identifier,
                    c.export_source.as_deref(),
                )?;
                pending = Some((
                    c.chat_identifier,
                    c.service,
                    c.conversation_type,
                    c.group_title,
                    c.exported_at,
                    c.participants
                        .into_iter()
                        .map(|p| (p.handle, p.name_hint, p.handle_type))
                        .collect(),
                    source,
                ));
            }
            ExportRecord::Message(m) => {
                if pending.is_none() && !is_orphaned {
                    bail!(
                        "{} is missing a conversation header (expected before messages)",
                        path.display()
                    );
                }
                messages.push(m);
            }
        }
    }

    if let Some(header) = pending.take() {
        stats.merge_file(&import_conversation_to_staging(
            tx,
            stmts,
            opts,
            &source_file,
            header,
            messages,
            asset_stats,
            media_work,
        )?);
    } else if is_orphaned {
        if opts.source_from_jsonl {
            bail!(
                "{}: orphaned.jsonl without a conversation header cannot supply export.source",
                path.display()
            );
        }
        stats.merge_file(&import_conversation_to_staging(
            tx,
            stmts,
            opts,
            &source_file,
            (
                "orphaned".to_string(),
                None,
                "orphaned".to_string(),
                None,
                None,
                Vec::new(),
                opts.source.to_string(),
            ),
            messages,
            asset_stats,
            media_work,
        )?);
    } else if messages.is_empty() {
        bail!(
            "{} has no conversation header and no messages",
            path.display()
        );
    } else {
        bail!(
            "{} is missing a conversation header (expected first record)",
            path.display()
        );
    }

    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
fn import_conversation_to_staging(
    tx: &Transaction<'_>,
    stmts: &mut StagingInserts<'_>,
    opts: &ImportOptions<'_>,
    source_file: &str,
    conversation: ConversationHeader,
    messages: Vec<MessageRecord>,
    asset_stats: &mut AssetStats,
    media_work: &Path,
) -> Result<ImportStats> {
    let (
        chat_identifier,
        service,
        conversation_type,
        group_title,
        exported_at,
        participants,
        source,
    ) = conversation;

    let assets_dir = assets_dir_for_source(opts, &source)?;
    let mut stats = ImportStats::default();
    let kept_participants = participants;

    let mut prepared_messages = Vec::with_capacity(messages.len());
    for mut msg in messages {
        let attachments = prepare_attachments(
            opts.asset_root,
            &assets_dir,
            std::mem::take(&mut msg.attachments),
            asset_stats,
            opts.media,
            media_work,
        )?;
        prepared_messages.push((msg, attachments));
    }

    // Conversation identity: the chat handle, typed from its shape (Phone for
    // SMS/iMessage/WhatsApp numbers, Email for `@`, Other for group ids).
    let (chat_handle_id, flagged) = resolve_handle(
        tx,
        &stmts.account_id,
        &chat_identifier,
        infer_handle_type(&chat_identifier),
        service.as_deref(),
    )?;
    if flagged {
        stats.phones_needing_review += 1;
    }

    stmts.conv.execute(params![
        stmts.account_id,
        chat_handle_id,
        service,
        conversation_type,
        group_title,
        exported_at,
        source_file,
    ])?;
    let conversation_id = tx.last_insert_rowid();
    stats.conversations = 1;

    for (handle, name_hint, handle_type) in kept_participants {
        // Prefer the source-provided type; fall back to shape inference.
        let handle_type = handle_type.unwrap_or_else(|| infer_handle_type(&handle));
        let (handle_id, flagged) = resolve_handle(
            tx,
            &stmts.account_id,
            &handle,
            handle_type,
            service.as_deref(),
        )?;
        if flagged {
            stats.phones_needing_review += 1;
        }
        let contact_id = contact_id_for_handle(tx, &stmts.account_id, handle_id)?;
        let vault_name = match contact_id {
            Some(id) => contact_preferred_name(tx, &stmts.account_id, id)?,
            None => None,
        };
        let name_hint = apply_contact_name_mode(opts.contact_name_mode, name_hint, vault_name);
        stmts
            .part
            .execute(params![conversation_id, handle_id, contact_id, name_hint])?;
        stats.participants += 1;
    }

    for (sort_order, (msg, attachments)) in prepared_messages.into_iter().enumerate() {
        let body = if msg.is_announcement {
            clean_body(msg.announcement.as_deref()).or_else(|| clean_body(msg.text.as_deref()))
        } else {
            clean_body(msg.text.as_deref())
        };

        // Sender identity: resolved to a handle row (NULL for own messages).
        let sender_handle_id = if !msg.is_from_me
            && let Some(sender) = msg
                .sender
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            let handle_type = msg
                .sender_handle_type
                .unwrap_or_else(|| infer_handle_type(sender));
            let (handle_id, flagged) = resolve_handle(
                tx,
                &stmts.account_id,
                sender,
                handle_type,
                msg.service.as_deref(),
            )?;
            if flagged {
                stats.phones_needing_review += 1;
            }
            Some(handle_id)
        } else {
            None
        };

        let inserted = stmts.msg.execute(params![
            conversation_id,
            &stmts.account_id,
            source,
            msg.guid,
            msg.timestamp,
            msg.timestamp_utc,
            msg.is_from_me as i64,
            sender_handle_id,
            msg.subject,
            body,
            msg.is_announcement as i64,
            msg.is_reply as i64,
            msg.thread_originator_guid,
            msg.thread_originator_part,
            msg.num_replies,
            sort_order as i64,
            stmts.import_id,
        ])?;

        if inserted == 0 {
            stats.messages_deduped += 1;
            continue;
        }

        let message_id = tx.last_insert_rowid();
        stats.messages += 1;

        for prepared in attachments {
            let att = prepared.record;
            let (sha256, assets_path, mime_type) = match prepared.stored {
                Some(stored) => (
                    Some(stored.sha256),
                    Some(stored.assets_path),
                    stored.mime_type.or(att.mime_type),
                ),
                None => (None, None, att.mime_type),
            };

            let size_bytes = assets_path
                .as_deref()
                .and_then(|rel| std::fs::metadata(assets_dir.join(rel)).ok())
                .map(|meta| meta.len() as i64);

            stmts.att.execute(params![
                message_id,
                att.path,
                att.original_name,
                mime_type,
                att.is_sticker as i64,
                att.transcription,
                sha256,
                assets_path,
                size_bytes,
            ])?;
            stats.attachments += 1;
        }

        for tap in msg.tapbacks {
            // Tapback sender: resolved to a handle row (NULL for own tapbacks,
            // matching the message `sender_handle_id` convention).
            let sender_handle_id = if !tap.is_from_me
                && let Some(sender) = tap
                    .sender
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            {
                let (handle_id, flagged) = resolve_handle(
                    tx,
                    &stmts.account_id,
                    sender,
                    infer_handle_type(sender),
                    msg.service.as_deref(),
                )?;
                if flagged {
                    stats.phones_needing_review += 1;
                }
                Some(handle_id)
            } else {
                None
            };
            stmts.tap.execute(params![
                message_id,
                tap.part_index,
                tap.kind,
                tap.emoji,
                tap.is_from_me as i64,
                sender_handle_id,
            ])?;
            stats.tapbacks += 1;
        }
    }

    Ok(stats)
}

#[derive(Debug, Default)]
struct PromoteStats {
    conversations: u64,
    participants: u64,
    messages: u64,
    attachments: u64,
    tapbacks: u64,
    messages_deduped: u64,
    messages_appended: u64,
}

fn promote_append(
    conn: &mut Connection,
    mode: ImportMode,
    account_id: &str,
    fill_content_keys: bool,
) -> Result<PromoteStats> {
    let mut stats = PromoteStats::default();
    let started = Instant::now();

    let tx = conn.transaction()?;

    // Staging→prod conversation id map for set-based inserts.
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_conv_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_conv_map;
        "#,
    )?;

    let staging_conv_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM staging_conversations WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?;
    println!("  sql:      promote: {staging_conv_count} staging conversations → production…");
    let _ = io::stdout().flush();

    let max_conv_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM conversations", [], |r| {
            r.get(0)
        })?;
    tx.execute(
        r#"
        INSERT INTO conversations (
            account_id, chat_handle_id, service, conversation_type,
            group_title, exported_at, source_file
        )
        SELECT
            account_id, chat_handle_id, service, conversation_type,
            group_title, exported_at, source_file
        FROM staging_conversations
        WHERE account_id = ?1
        ON CONFLICT(account_id, chat_handle_id) DO UPDATE SET
            service = COALESCE(excluded.service, conversations.service),
            conversation_type = excluded.conversation_type,
            group_title = COALESCE(excluded.group_title, conversations.group_title),
            exported_at = COALESCE(excluded.exported_at, conversations.exported_at),
            source_file = excluded.source_file
        "#,
        params![account_id],
    )?;
    tx.execute(
        r#"
        INSERT INTO _promote_conv_map (staging_id, prod_id)
        SELECT sc.id, c.id
        FROM staging_conversations sc
        JOIN conversations c
          ON c.account_id = sc.account_id
         AND c.chat_handle_id = sc.chat_handle_id
        WHERE sc.account_id = ?1
        "#,
        params![account_id],
    )?;
    let new_conversations: i64 = tx.query_row(
        "SELECT COUNT(*) FROM _promote_conv_map WHERE prod_id > ?1",
        params![max_conv_before],
        |r| r.get(0),
    )?;
    stats.conversations = u64::try_from(new_conversations).unwrap_or(0);
    println!(
        "  sql:      promote: conversations done (new={})  ({:.1}s)",
        stats.conversations,
        started.elapsed().as_secs_f64()
    );

    let staging_part_count: i64 = tx.query_row(
        r#"
        SELECT COUNT(*) FROM staging_participants
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = ?1
        )
        "#,
        params![account_id],
        |r| r.get(0),
    )?;
    println!("  sql:      promote: {staging_part_count} staging participants → production…");
    let _ = io::stdout().flush();
    stats.participants = u64::try_from(tx.execute(
        r#"
        INSERT OR IGNORE INTO participants (conversation_id, handle_id, contact_id, name_hint)
        SELECT cm.prod_id, sp.handle_id, sp.contact_id, sp.name_hint
        FROM staging_participants sp
        JOIN _promote_conv_map cm ON cm.staging_id = sp.conversation_id
        "#,
        [],
    )?)
    .unwrap_or(0);
    println!(
        "  sql:      promote: participants done (new={})  ({:.1}s)",
        stats.participants,
        started.elapsed().as_secs_f64()
    );

    let total_msgs: i64 = tx.query_row(
        r#"
        SELECT COUNT(*) FROM staging_messages
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = ?1
        )
        "#,
        params![account_id],
        |r| r.get(0),
    )?;
    println!(
        "  sql:      promote: {total_msgs} staging messages → production ({})…",
        mode.as_str()
    );
    let _ = io::stdout().flush();

    // Skip per-row FTS trigger work during bulk message/attachment inserts; index once after.
    let phase = Instant::now();
    println!("  sql:      promote: pausing FTS triggers…");
    let _ = io::stdout().flush();
    schema::drop_messages_fts_triggers(&tx)?;
    promote_phase_done(started, phase, "FTS triggers paused");

    let existing_msgs: i64 =
        tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let drop_secondary = should_drop_messages_secondary_indexes(total_msgs, existing_msgs);
    if drop_secondary {
        let phase = Instant::now();
        println!(
            "  sql:      promote: dropping secondary message indexes (staging={total_msgs} existing={existing_msgs})…"
        );
        let _ = io::stdout().flush();
        schema::drop_messages_secondary_indexes(&tx)?;
        promote_phase_done(started, phase, "secondary indexes dropped");
    } else {
        println!(
            "  sql:      promote: keeping secondary message indexes (staging={total_msgs} existing={existing_msgs})"
        );
        let _ = io::stdout().flush();
    }

    let msg_map = promote_messages_chunked(&tx, mode, account_id, total_msgs, &mut stats, started)?;

    if drop_secondary {
        let phase = Instant::now();
        println!("  sql:      promote: rebuilding secondary message indexes…");
        let _ = io::stdout().flush();
        schema::create_messages_secondary_indexes(&tx)?;
        promote_phase_done(
            started,
            phase,
            format!(
                "secondary indexes rebuilt (inserted={} skipped={})",
                stats.messages, stats.messages_deduped
            ),
        );
    } else {
        println!(
            "  sql:      promote: messages done (inserted={} skipped={})  (total {:.1}s)",
            stats.messages,
            stats.messages_deduped,
            started.elapsed().as_secs_f64()
        );
        let _ = io::stdout().flush();
    }

    let phase = Instant::now();
    println!(
        "  sql:      promote: writing message id map ({} pairs)…",
        msg_map.len()
    );
    let _ = io::stdout().flush();
    fill_promote_msg_map(&tx, &msg_map)?;
    promote_phase_done(started, phase, "message id map written");

    let phase = Instant::now();
    println!("  sql:      promote: bulk-inserting attachments…");
    let _ = io::stdout().flush();
    let att_inserted = tx.execute(
        r#"
        INSERT INTO attachments (
            message_id, path, original_name, mime_type, is_sticker, transcription,
            sha256, assets_path, size_bytes
        )
        SELECT
            mm.prod_id, sa.path, sa.original_name, sa.mime_type, sa.is_sticker, sa.transcription,
            sa.sha256, sa.assets_path, sa.size_bytes
        FROM staging_attachments sa
        JOIN _promote_msg_map mm ON mm.staging_id = sa.message_id
        "#,
        [],
    )?;
    stats.attachments = att_inserted as u64;
    promote_phase_done(
        started,
        phase,
        format!("attachments done (inserted={})", stats.attachments),
    );

    let phase = Instant::now();
    println!("  sql:      promote: bulk-inserting tapbacks…");
    let _ = io::stdout().flush();
    let tap_inserted = tx.execute(
        r#"
        INSERT INTO tapbacks (
            message_id, part_index, kind, emoji, is_from_me, sender_handle_id
        )
        SELECT
            mm.prod_id, st.part_index, st.kind, st.emoji, st.is_from_me, st.sender_handle_id
        FROM staging_tapbacks st
        JOIN _promote_msg_map mm ON mm.staging_id = st.message_id
        "#,
        [],
    )?;
    stats.tapbacks = tap_inserted as u64;
    promote_phase_done(
        started,
        phase,
        format!("tapbacks done (inserted={})", stats.tapbacks),
    );

    let phase = Instant::now();
    println!("  sql:      promote: bulk-indexing FTS for new messages…");
    let _ = io::stdout().flush();
    let fts_indexed = schema::index_messages_fts_from_promote_map(&tx)?;
    schema::install_messages_fts_triggers(&tx)?;
    promote_phase_done(
        started,
        phase,
        format!("FTS indexed={fts_indexed} (triggers restored)"),
    );

    if fill_content_keys {
        let phase = Instant::now();
        println!("  sql:      promote: filling content keys…");
        let _ = io::stdout().flush();
        let keys = crate::dedupe::fill_missing_content_keys(&tx, account_id)?;
        promote_phase_done(started, phase, format!("content keys filled={keys}"));
    }

    let phase = Instant::now();
    println!("  sql:      promote: committing transaction…");
    let _ = io::stdout().flush();
    tx.commit()?;
    promote_phase_done(
        started,
        phase,
        format!(
            "committed  convs={} parts={} msgs={} atts={} taps={}",
            stats.conversations,
            stats.participants,
            stats.messages,
            stats.attachments,
            stats.tapbacks
        ),
    );

    Ok(stats)
}

/// Staging rows per set-based insert window (progress + smaller WAL spikes).
const PROMOTE_MESSAGE_BATCH: i64 = 10_000;
/// Pairs per multi-row INSERT into `_promote_msg_map` (SQLite default max variables is 999).
const PROMOTE_MSG_MAP_VALUE_BATCH: usize = 400;
/// Drop secondary indexes only for large promotes relative to the existing table.
const PROMOTE_INDEX_DROP_MIN_STAGING: i64 = 5_000;

fn promote_phase_done(total: Instant, phase: Instant, msg: impl std::fmt::Display) {
    println!(
        "  sql:      promote: {msg}  (phase {:.1}s, total {:.1}s)",
        phase.elapsed().as_secs_f64(),
        total.elapsed().as_secs_f64()
    );
    let _ = io::stdout().flush();
}

fn should_drop_messages_secondary_indexes(staging_count: i64, existing_count: i64) -> bool {
    staging_count >= PROMOTE_INDEX_DROP_MIN_STAGING
        && staging_count.saturating_mul(5) >= existing_count.max(1)
}

fn promote_messages_chunked(
    tx: &Transaction<'_>,
    mode: ImportMode,
    account_id: &str,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    let bounds: (Option<i64>, Option<i64>) = tx.query_row(
        r#"
        SELECT MIN(sm.id), MAX(sm.id)
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = ?1
        "#,
        params![account_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (Some(min_id), Some(max_id)) = bounds else {
        stats.messages = 0;
        stats.messages_appended = 0;
        stats.messages_deduped = 0;
        return Ok(HashMap::new());
    };

    if mode == ImportMode::Replace {
        promote_messages_replace_chunked(tx, account_id, min_id, max_id, total_msgs, stats, started)
    } else {
        promote_messages_append_chunked(tx, account_id, min_id, max_id, total_msgs, stats, started)
    }
}

fn promote_messages_replace_chunked(
    tx: &Transaction<'_>,
    account_id: &str,
    min_id: i64,
    max_id: i64,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    let mut msg_map = HashMap::new();
    let mut max_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
    let mut inserted_total = 0u64;
    let mut lo = min_id - 1;
    let mut chunk_idx = 0u32;

    while lo < max_id {
        chunk_idx += 1;
        let hi = (lo + PROMOTE_MESSAGE_BATCH).min(max_id);
        let phase = Instant::now();
        println!(
            "  sql:      promote: inserting messages chunk {chunk_idx} (staging id {}..{}, replace)…",
            lo + 1,
            hi
        );
        let _ = io::stdout().flush();

        let inserted = tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, subject, body, is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender_handle_id, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
                sm.import_id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND sm.id > ?2
              AND sm.id <= ?3
            ORDER BY sm.id
            "#,
            params![account_id, lo, hi],
        )?;
        inserted_total += inserted as u64;

        let staging_ids: Vec<i64> = tx
            .prepare(
                r#"
                SELECT sm.id
                FROM staging_messages sm
                JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
                WHERE sm.account_id = ?1
                  AND sm.id > ?2
                  AND sm.id <= ?3
                ORDER BY sm.id
                "#,
            )?
            .query_map(params![account_id, lo, hi], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let prod_ids: Vec<i64> = tx
            .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id")?
            .query_map(params![max_before], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if staging_ids.len() != prod_ids.len() {
            bail!(
                "promote replace message id map mismatch: staging={} new_prod={} (chunk staging id {}..{})",
                staging_ids.len(),
                prod_ids.len(),
                lo + 1,
                hi
            );
        }
        for (staging_id, prod_id) in staging_ids.into_iter().zip(prod_ids) {
            msg_map.insert(staging_id, prod_id);
        }
        max_before = tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;

        promote_phase_done(
            started,
            phase,
            format!(
                "chunk {chunk_idx} inserted={inserted} running={inserted_total}/{total_msgs}"
            ),
        );
        lo = hi;
    }

    stats.messages = inserted_total;
    stats.messages_appended = inserted_total;
    Ok(msg_map)
}

fn promote_messages_append_chunked(
    tx: &Transaction<'_>,
    account_id: &str,
    min_id: i64,
    max_id: i64,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    // Append: rely on partial unique index ix_messages_account_source_guid via
    // INSERT OR IGNORE. Correlated NOT EXISTS / JOIN anti-joins mis-plan onto
    // ix_messages_source and scan the whole source (~10s+ at 50k+ rows).
    let mut msg_map = HashMap::new();
    let mut max_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
    let mut inserted_total = 0u64;
    let mut lo = min_id - 1;
    let mut chunk_idx = 0u32;

    while lo < max_id {
        chunk_idx += 1;
        let hi = (lo + PROMOTE_MESSAGE_BATCH).min(max_id);
        let phase = Instant::now();
        println!(
            "  sql:      promote: inserting messages chunk {chunk_idx} (staging id {}..{}, append)…",
            lo + 1,
            hi
        );
        let _ = io::stdout().flush();

        let inserted = tx.execute(
            r#"
            INSERT OR IGNORE INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, subject, body, is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender_handle_id, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
                sm.import_id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND sm.guid IS NOT NULL
              AND sm.guid != ''
              AND sm.id > ?2
              AND sm.id <= ?3
            ORDER BY sm.id
            "#,
            params![account_id, lo, hi],
        )?;
        inserted_total += inserted as u64;

        let staging_ids: Vec<i64> = tx
            .prepare(
                r#"
                SELECT sm.id
                FROM messages m
                JOIN staging_messages sm
                  ON sm.account_id = m.account_id
                 AND sm.source = m.source
                 AND sm.guid = m.guid
                JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
                WHERE m.id > ?1
                  AND m.account_id = ?2
                  AND m.guid IS NOT NULL
                  AND m.guid != ''
                  AND sm.id > ?3
                  AND sm.id <= ?4
                ORDER BY m.id
                "#,
            )?
            .query_map(params![max_before, account_id, lo, hi], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let prod_ids: Vec<i64> = tx
            .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id")?
            .query_map(params![max_before], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if staging_ids.len() != prod_ids.len() {
            bail!(
                "promote append message id map mismatch: staging_new={} new_prod={} (chunk staging id {}..{})",
                staging_ids.len(),
                prod_ids.len(),
                lo + 1,
                hi
            );
        }
        for (staging_id, prod_id) in staging_ids.into_iter().zip(prod_ids) {
            msg_map.insert(staging_id, prod_id);
        }
        max_before = tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;

        promote_phase_done(
            started,
            phase,
            format!(
                "chunk {chunk_idx} inserted={inserted} running={inserted_total}/{total_msgs}"
            ),
        );
        lo = hi;
    }

    // Null/empty guids are outside the partial unique index — always insert.
    let phase = Instant::now();
    println!("  sql:      promote: inserting messages with empty guids…");
    let _ = io::stdout().flush();
    let empty_max_before = max_before;
    let inserted_empty = tx.execute(
        r#"
        INSERT INTO messages (
            conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
            sender_handle_id, subject, body, is_announcement, is_reply, thread_originator_guid,
            thread_originator_part, num_replies, sort_order, import_id
        )
        SELECT
            cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
            sm.sender_handle_id, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
            sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
            sm.import_id
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = ?1
          AND (sm.guid IS NULL OR sm.guid = '')
        ORDER BY sm.id
        "#,
        params![account_id],
    )?;
    inserted_total += inserted_empty as u64;

    let empty_staging_ids: Vec<i64> = tx
        .prepare(
            r#"
            SELECT sm.id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND (sm.guid IS NULL OR sm.guid = '')
            ORDER BY sm.id
            "#,
        )?
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let empty_prod_ids: Vec<i64> = tx
        .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id")?
        .query_map(params![empty_max_before], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if empty_staging_ids.len() != empty_prod_ids.len() {
        bail!(
            "promote append empty-guid id map mismatch: staging={} new_prod={}",
            empty_staging_ids.len(),
            empty_prod_ids.len()
        );
    }
    for (staging_id, prod_id) in empty_staging_ids.into_iter().zip(empty_prod_ids) {
        msg_map.insert(staging_id, prod_id);
    }
    promote_phase_done(
        started,
        phase,
        format!("empty-guid messages inserted={inserted_empty}"),
    );

    stats.messages = inserted_total;
    stats.messages_appended = inserted_total;
    stats.messages_deduped = (total_msgs as u64).saturating_sub(inserted_total);
    Ok(msg_map)
}

fn fill_promote_msg_map(tx: &Transaction<'_>, msg_map: &HashMap<i64, i64>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_msg_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_msg_map;
        "#,
    )?;
    if msg_map.is_empty() {
        return Ok(());
    }

    let pairs: Vec<(i64, i64)> = msg_map.iter().map(|(&s, &p)| (s, p)).collect();
    for chunk in pairs.chunks(PROMOTE_MSG_MAP_VALUE_BATCH) {
        let mut sql =
            String::from("INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES ");
        for (i, _) in chunk.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            let base = i * 2;
            sql.push_str(&format!("(?{}, ?{})", base + 1, base + 2));
        }
        let mut stmt = tx.prepare(&sql)?;
        let mut vals: Vec<rusqlite::types::Value> = Vec::with_capacity(chunk.len() * 2);
        for &(staging_id, prod_id) in chunk {
            vals.push(staging_id.into());
            vals.push(prod_id.into());
        }
        stmt.execute(params_from_iter(vals))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    fn write_jsonl(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn append_skips_existing_guids_and_keeps_id_map() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");

        let first = write_jsonl(
            tmp.path(),
            "a.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":2,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183522000}}}
{"guid":"g-keep","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"one","attachments":[],"imessage":null,"source":null}
{"guid":"g-dup","timestamp_unix_ms":1426183522000,"direction":"outgoing","service":"sms","message_kind":"sms","sender_handle":null,"sender_display_name":null,"subject":null,"text":"two","attachments":[],"imessage":null,"source":null}
"#,
        );
        let first_stats = import_jsonl_files(
            &[first],
            &ImportOptions::fixed(
                &db,
                &assets,
                tmp.path(),
                None,
                false,
                ImportMode::Replace,
                "sms-backup-restore",
                TEST_ACCOUNT,
                true,
                None,
            ),
        )
        .unwrap();
        assert_eq!(first_stats.messages, 2);

        let second = write_jsonl(
            tmp.path(),
            "b.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":3,"attachment_count":0,"first_timestamp_unix_ms":1426183522000,"last_timestamp_unix_ms":1426183642000}}}
{"guid":"g-dup","timestamp_unix_ms":1426183522000,"direction":"outgoing","service":"sms","message_kind":"sms","sender_handle":null,"sender_display_name":null,"subject":null,"text":"two again","attachments":[],"imessage":null,"source":null}
{"guid":"g-new","timestamp_unix_ms":1426183582000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"three","attachments":[],"imessage":null,"source":null}
{"guid":"","timestamp_unix_ms":1426183642000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"empty guid always inserts","attachments":[],"imessage":null,"source":null}
"#,
        );
        let second_stats = import_jsonl_files(
            &[second],
            &ImportOptions::fixed(
                &db,
                &assets,
                tmp.path(),
                None,
                false,
                ImportMode::Append,
                "sms-backup-restore",
                TEST_ACCOUNT,
                false,
                None,
            ),
        )
        .unwrap();
        assert_eq!(second_stats.messages_appended, 2);
        assert_eq!(second_stats.messages_deduped, 1);

        let conn = Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4);
        let dup_body: String = conn
            .query_row("SELECT body FROM messages WHERE guid = 'g-dup'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(dup_body, "two");

        // Deferred FTS during promote must still index new bodies and restore triggers.
        let fts_three: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'three'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts_three, 1);
        let fts_one: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'one'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts_one, 1);
        let triggers: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE '%_fts_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(triggers, 6);
    }

    #[test]
    fn deferred_fts_indexes_attachment_text_after_promote() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        let att_dir = tmp.path().join("attachments");
        fs::create_dir_all(&att_dir).unwrap();
        fs::write(att_dir.join("receipt.pdf"), b"%PDF-fixture").unwrap();

        let path = write_jsonl(
            tmp.path(),
            "att.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-att","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"see attached","attachments":[{"path":"attachments/receipt.pdf","original_name":"uniqueinvoice.pdf","mime_type":"application/pdf","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
"#,
        );
        import_jsonl_files(
            &[path],
            &ImportOptions::fixed(
                &db,
                &assets,
                tmp.path(),
                None,
                false,
                ImportMode::Append,
                "imessage",
                TEST_ACCOUNT,
                false,
                None,
            ),
        )
        .unwrap();

        let conn = Connection::open(&db).unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'uniqueinvoice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            hits, 1,
            "attachment original_name must be searchable after deferred FTS"
        );
    }

    #[test]
    fn promote_stamps_messages_with_import_id() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let path = write_jsonl(
            tmp.path(),
            "import-id.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-import","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"linked","attachments":[],"imessage":null,"source":null}
"#,
        );

        let mut conn = Connection::open(&db).unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        crate::db::account_profile::ensure_account_row(&conn, TEST_ACCOUNT).unwrap();
        let import_id = crate::db::vault_imports::start_import(
            &conn,
            TEST_ACCOUNT,
            "imessage",
            "append",
            Some("test"),
        )
        .unwrap();

        let stats = import_jsonl_files_on_conn(
            &mut conn,
            &[path],
            &ImportOptions::fixed(
                &db,
                &assets,
                tmp.path(),
                None,
                false,
                ImportMode::Append,
                "imessage",
                TEST_ACCOUNT,
                false,
                Some(import_id),
            ),
            ImportSchemaMode::AssumeReady,
        )
        .unwrap();
        assert_eq!(stats.messages, 1);

        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE import_id = ?1",
                params![import_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped, 1);

        let row = crate::db::vault_imports::complete_import(
            &conn,
            TEST_ACCOUNT,
            import_id,
            &crate::db::vault_imports::CompleteImportArgs {
                ok: true,
                message_count: Some(stats.messages as i64),
                attachment_count: Some(0),
                bytes_uploaded: Some(0),
            },
        )
        .unwrap();
        assert_eq!(row.status, "completed");
        assert_eq!(row.message_count, 1);

        let listed = crate::db::vault_imports::list_imports_for_account(&conn, TEST_ACCOUNT, 10)
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source, "imessage");
        assert!(!listed[0].started_at.is_empty());
        assert!(listed[0].finished_at.is_some());
        assert_eq!(
            crate::db::vault_imports::account_attachment_bytes(&conn, TEST_ACCOUNT).unwrap(),
            0
        );
        assert!(
            crate::db::vault_imports::top_attachments_by_size(&conn, TEST_ACCOUNT, 5)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn trunk_zero_phone_imports_digits_with_review_note() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let path = write_jsonl(
            tmp.path(),
            "trunk-zero.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"020 7946 0000","conversation_type":"individual","group_title":null,"participants":[{"handle":"020 7946 0000","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-trunk-zero","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"020 7946 0000","sender_display_name":null,"subject":null,"text":"hello","attachments":[],"imessage":null,"source":null}
"#,
        );

        let mut conn = Connection::open(&db).unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        crate::db::account_profile::ensure_account_row(&conn, TEST_ACCOUNT).unwrap();

        let stats = import_jsonl_files_on_conn(
            &mut conn,
            &[path],
            &ImportOptions::fixed(
                &db,
                &assets,
                tmp.path(),
                None,
                false,
                ImportMode::Append,
                "imessage",
                TEST_ACCOUNT,
                false,
                None,
            ),
            ImportSchemaMode::AssumeReady,
        )
        .unwrap();
        assert_eq!(stats.phones_needing_review, 1);

        // Guarded policy: normalized mirrors the digits (never +02079460000)
        // and the handles row carries a review note.
        let (normalized, note): (String, Option<String>) = conn
            .query_row(
                "SELECT normalized, normalized_note FROM handles
                 WHERE account_id = ?1 AND handle_type = 'phone'",
                params![TEST_ACCOUNT],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(normalized, "02079460000");
        assert!(
            note.as_deref().is_some(),
            "trunk-zero import must carry a review note"
        );
    }

    #[test]
    fn source_from_jsonl_stamps_export_source_and_assets() {
        use crate::config::PathsConfig;
        use crate::import_media::MediaMode;

        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let data_dir = tmp.path().join("data");
        let paths = PathsConfig {
            db: db.clone(),
            data_dir: data_dir.clone(),
            assets_dir: "assets".into(),
            assets_converted_dir: "assets_converted".into(),
        };
        let placeholder = tmp.path().join("unused-assets");
        fs::create_dir_all(tmp.path().join("media")).unwrap();
        fs::write(tmp.path().join("media/photo.jpg"), b"jpeg-bytes").unwrap();

        let path = write_jsonl(
            tmp.path(),
            "c.jsonl",
            r#"{"schema_version":3,"export":{"source":"go-sms-pro","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550100","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550100","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g1","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550100","sender_display_name":null,"subject":null,"text":"hi","attachments":[{"path":"media/photo.jpg","original_name":"photo.jpg","mime_type":"image/jpeg","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
"#,
        );
        let stats = import_jsonl_files(
            &[path],
            &ImportOptions {
                db_path: &db,
                assets_dir: &placeholder,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "",
                account_id: TEST_ACCOUNT,
                fill_content_keys: true,
                import_id: None,
                source_from_jsonl: true,
                paths: Some(&paths),
                media: MediaMode::Copy,
                wipe_sources: Some(vec!["go-sms-pro".into()]),
                contact_name_mode: ContactNameMode::default(),
            },
        )
        .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.assets_copied, 1);

        let conn = Connection::open(&db).unwrap();
        let source: String = conn
            .query_row("SELECT source FROM messages WHERE guid = 'g1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(source, "go-sms-pro");
        let assets_root = paths.assets_dir_for_account(TEST_ACCOUNT, "go-sms-pro");
        assert!(assets_root.is_dir());
    }

    #[test]
    fn media_none_skips_attachment_copy() {
        use crate::config::PathsConfig;
        use crate::import_media::MediaMode;

        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let data_dir = tmp.path().join("data");
        let paths = PathsConfig {
            db: db.clone(),
            data_dir,
            assets_dir: "assets".into(),
            assets_converted_dir: "assets_converted".into(),
        };
        let placeholder = tmp.path().join("unused-assets");
        fs::create_dir_all(tmp.path().join("media")).unwrap();
        fs::write(tmp.path().join("media/photo.jpg"), b"jpeg-bytes").unwrap();

        let path = write_jsonl(
            tmp.path(),
            "c.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550100","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550100","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g1","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550100","sender_display_name":null,"subject":null,"text":"hi","attachments":[{"path":"media/photo.jpg","original_name":"photo.jpg","mime_type":"image/jpeg","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
"#,
        );
        let stats = import_jsonl_files(
            &[path],
            &ImportOptions {
                db_path: &db,
                assets_dir: &placeholder,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
                source_from_jsonl: true,
                paths: Some(&paths),
                media: MediaMode::None,
                wipe_sources: Some(vec!["sms".into()]),
                contact_name_mode: ContactNameMode::default(),
            },
        )
        .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.attachments, 0);
        assert_eq!(stats.assets_copied, 0);
    }

    fn seed_contact(db: &Path, handle: &str, preferred_name: &str) {
        let conn = Connection::open(db).unwrap();
        schema::configure_connection(&conn).unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        crate::db::account_profile::ensure_account_row(&conn, TEST_ACCOUNT).unwrap();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, ?2)",
            params![TEST_ACCOUNT, preferred_name],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type)
             VALUES (?1, ?2, ?2, 'phone')",
            params![TEST_ACCOUNT, handle],
        )
        .unwrap();
        let handle_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES (?1, ?2, ?3)",
            params![TEST_ACCOUNT, handle_id, contact_id],
        )
        .unwrap();
    }

    fn participant_name_hint(db: &Path) -> Option<String> {
        let conn = Connection::open(db).unwrap();
        conn.query_row("SELECT name_hint FROM participants LIMIT 1", [], |r| {
            r.get(0)
        })
        .optional()
        .unwrap()
    }

    #[test]
    fn apply_contact_name_mode_unit() {
        assert_eq!(
            apply_contact_name_mode(ContactNameMode::FillMissing, None, Some("Vault".into())),
            Some("Vault".into())
        );
        assert_eq!(
            apply_contact_name_mode(
                ContactNameMode::FillMissing,
                Some("Import".into()),
                Some("Vault".into())
            ),
            Some("Import".into())
        );
        assert_eq!(
            apply_contact_name_mode(
                ContactNameMode::Overwrite,
                Some("Import".into()),
                Some("Vault".into())
            ),
            Some("Vault".into())
        );
        assert_eq!(
            apply_contact_name_mode(ContactNameMode::Overwrite, Some("Import".into()), None),
            Some("Import".into())
        );
    }

    #[test]
    fn contact_name_mode_fill_missing_keeps_import_name() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice");
        let path = write_jsonl(
            tmp.path(),
            "named.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Backup Bob"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-fill","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(
            &db,
            &assets,
            tmp.path(),
            None,
            false,
            ImportMode::Append,
            "imessage",
            TEST_ACCOUNT,
            false,
            None,
        );
        opts.contact_name_mode = ContactNameMode::FillMissing;
        import_jsonl_files(&[path], &opts).unwrap();
        assert_eq!(participant_name_hint(&db).as_deref(), Some("Backup Bob"));
    }

    #[test]
    fn contact_name_mode_fill_missing_uses_vault_when_empty() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice");
        let path = write_jsonl(
            tmp.path(),
            "missing.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-missing","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(
            &db,
            &assets,
            tmp.path(),
            None,
            false,
            ImportMode::Append,
            "imessage",
            TEST_ACCOUNT,
            false,
            None,
        );
        opts.contact_name_mode = ContactNameMode::FillMissing;
        import_jsonl_files(&[path], &opts).unwrap();
        assert_eq!(participant_name_hint(&db).as_deref(), Some("Vault Alice"));
    }

    #[test]
    fn contact_name_mode_overwrite_prefers_vault() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice");
        let path = write_jsonl(
            tmp.path(),
            "overwrite.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Backup Bob"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-over","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(
            &db,
            &assets,
            tmp.path(),
            None,
            false,
            ImportMode::Append,
            "imessage",
            TEST_ACCOUNT,
            false,
            None,
        );
        opts.contact_name_mode = ContactNameMode::Overwrite;
        import_jsonl_files(&[path], &opts).unwrap();
        assert_eq!(participant_name_hint(&db).as_deref(), Some("Vault Alice"));
    }
}
