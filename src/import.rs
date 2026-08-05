use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Statement, Transaction, params};
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
    /// Create unknown contacts / fill empty names after promote.
    pub backfill_contacts: bool,
    /// Optional vault import session id (messages stamped on promote).
    pub import_id: Option<i64>,
    /// When true, stamp `messages.source` from each conversation's IR `export.source`.
    pub source_from_jsonl: bool,
    /// Required when `source_from_jsonl` to resolve per-source asset dirs.
    pub paths: Option<&'a PathsConfig>,
    pub media: MediaMode,
    /// When `source_from_jsonl` + Replace: wipe these sources before import.
    pub wipe_sources: Option<Vec<String>>,
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
        backfill_contacts: bool,
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
            backfill_contacts,
            import_id,
            source_from_jsonl: false,
            paths: None,
            media: MediaMode::Copy,
            wipe_sources: None,
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
}

impl ImportStats {
    fn merge_file(&mut self, other: &ImportStats) {
        self.conversations += other.conversations;
        self.participants += other.participants;
        self.messages += other.messages;
        self.attachments += other.attachments;
        self.tapbacks += other.tapbacks;
        self.messages_deduped += other.messages_deduped;
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
        Some("message-vault-rs"),
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
                    account_id, chat_identifier, service, conversation_type, group_title, exported_at, source_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )?,
            part: tx.prepare(
                r#"
                INSERT INTO staging_participants (conversation_id, handle, name_hint)
                VALUES (?1, ?2, ?3)
                "#,
            )?,
            msg: tx.prepare(
                r#"
                INSERT OR IGNORE INTO staging_messages (
                    conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me, sender,
                    subject, body, is_announcement, is_reply, thread_originator_guid,
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
                    message_id, part_index, kind, emoji, is_from_me, sender
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
            )?,
        })
    }
}

/// chat_identifier, service, conversation_type, group_title, exported_at, participants, source
type ConversationHeader = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Vec<(String, Option<String>)>,
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
                        .map(|p| (p.handle, p.name_hint))
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

    stmts.conv.execute(params![
        stmts.account_id,
        chat_identifier,
        service,
        conversation_type,
        group_title,
        exported_at,
        source_file,
    ])?;
    let conversation_id = tx.last_insert_rowid();
    stats.conversations = 1;

    for (handle, name_hint) in kept_participants {
        stmts
            .part
            .execute(params![conversation_id, handle, name_hint])?;
        stats.participants += 1;
    }

    for (sort_order, (msg, attachments)) in prepared_messages.into_iter().enumerate() {
        let body = if msg.is_announcement {
            clean_body(msg.announcement.as_deref()).or_else(|| clean_body(msg.text.as_deref()))
        } else {
            clean_body(msg.text.as_deref())
        };

        let inserted = stmts.msg.execute(params![
            conversation_id,
            &stmts.account_id,
            source,
            msg.guid,
            msg.timestamp,
            msg.timestamp_utc,
            msg.is_from_me as i64,
            msg.sender,
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
            stmts.tap.execute(params![
                message_id,
                tap.part_index,
                tap.kind,
                tap.emoji,
                tap.is_from_me as i64,
                tap.sender,
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
            account_id, chat_identifier, service, conversation_type,
            group_title, exported_at, source_file
        )
        SELECT
            account_id, chat_identifier, service, conversation_type,
            group_title, exported_at, source_file
        FROM staging_conversations
        WHERE account_id = ?1
        ON CONFLICT(account_id, chat_identifier) DO UPDATE SET
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
         AND c.chat_identifier = sc.chat_identifier
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
        INSERT OR IGNORE INTO participants (conversation_id, handle, name_hint)
        SELECT cm.prod_id, sp.handle, sp.name_hint
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
    println!("  sql:      promote: pausing FTS triggers…");
    let _ = io::stdout().flush();
    schema::drop_messages_fts_triggers(&tx)?;

    let msg_map = if mode == ImportMode::Replace {
        // Source rows were wiped already: one set-based INSERT, then zip new ids in order.
        let max_before: i64 =
            tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
        let inserted = tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me, sender,
                subject, body, is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
                sm.import_id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            ORDER BY sm.id
            "#,
            [],
        )?;
        stats.messages = inserted as u64;
        stats.messages_appended = inserted as u64;

        let staging_ids: Vec<i64> = tx
            .prepare(
                r#"
                SELECT sm.id
                FROM staging_messages sm
                JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
                ORDER BY sm.id
                "#,
            )?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let prod_ids: Vec<i64> = tx
            .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id")?
            .query_map(params![max_before], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if staging_ids.len() != prod_ids.len() {
            bail!(
                "promote replace message id map mismatch: staging={} new_prod={}",
                staging_ids.len(),
                prod_ids.len()
            );
        }
        staging_ids
            .into_iter()
            .zip(prod_ids)
            .collect::<HashMap<_, _>>()
    } else {
        // Append: rely on partial unique index ix_messages_account_source_guid via
        // INSERT OR IGNORE. Correlated NOT EXISTS / JOIN anti-joins mis-plan onto
        // ix_messages_source and scan the whole source (~10s+ at 50k+ rows).
        let max_before: i64 =
            tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;

        let inserted_guided = tx.execute(
            r#"
            INSERT OR IGNORE INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me, sender,
                subject, body, is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
                sm.import_id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND sm.guid IS NOT NULL
              AND sm.guid != ''
            ORDER BY sm.id
            "#,
            params![account_id],
        )?;

        // Null/empty guids are outside the partial unique index — always insert.
        let inserted_empty = tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me, sender,
                subject, body, is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
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

        let inserted = inserted_guided + inserted_empty;
        stats.messages = inserted as u64;
        stats.messages_appended = inserted as u64;
        stats.messages_deduped = (total_msgs as u64).saturating_sub(inserted as u64);

        // New production rows were inserted in order: guided (by sm.id) then empty (by sm.id).
        let mut staging_ids: Vec<i64> = tx
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
                ORDER BY m.id
                "#,
            )?
            .query_map(params![max_before, account_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
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
        staging_ids.extend(empty_staging_ids);

        let prod_ids: Vec<i64> = tx
            .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id")?
            .query_map(params![max_before], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if staging_ids.len() != prod_ids.len() {
            bail!(
                "promote append message id map mismatch: staging_new={} new_prod={}",
                staging_ids.len(),
                prod_ids.len()
            );
        }
        staging_ids
            .into_iter()
            .zip(prod_ids)
            .collect::<HashMap<_, _>>()
    };

    println!(
        "  sql:      promote: messages done (inserted={} skipped={})  ({:.1}s)",
        stats.messages,
        stats.messages_deduped,
        started.elapsed().as_secs_f64()
    );

    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_msg_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_msg_map;
        "#,
    )?;
    {
        let mut ins =
            tx.prepare("INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES (?1, ?2)")?;
        for (staging_id, prod_id) in &msg_map {
            ins.execute(params![staging_id, prod_id])?;
        }
    }

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
    println!(
        "  sql:      promote: attachments done (inserted={})  ({:.1}s)",
        stats.attachments,
        started.elapsed().as_secs_f64()
    );

    println!("  sql:      promote: bulk-inserting tapbacks…");
    let _ = io::stdout().flush();
    let tap_inserted = tx.execute(
        r#"
        INSERT INTO tapbacks (
            message_id, part_index, kind, emoji, is_from_me, sender
        )
        SELECT
            mm.prod_id, st.part_index, st.kind, st.emoji, st.is_from_me, st.sender
        FROM staging_tapbacks st
        JOIN _promote_msg_map mm ON mm.staging_id = st.message_id
        "#,
        [],
    )?;
    stats.tapbacks = tap_inserted as u64;
    println!(
        "  sql:      promote: tapbacks done (inserted={})  ({:.1}s)",
        stats.tapbacks,
        started.elapsed().as_secs_f64()
    );

    println!("  sql:      promote: bulk-indexing FTS for new messages…");
    let _ = io::stdout().flush();
    let fts_indexed = schema::index_messages_fts_from_promote_map(&tx)?;
    schema::install_messages_fts_triggers(&tx)?;
    println!(
        "  sql:      promote: FTS indexed={fts_indexed} (triggers restored)  ({:.1}s)",
        started.elapsed().as_secs_f64()
    );

    if fill_content_keys {
        println!("  sql:      promote: filling content keys…");
        let _ = io::stdout().flush();
        let keys = crate::dedupe::fill_missing_content_keys(&tx, account_id)?;
        println!(
            "  sql:      promote: content keys filled={keys}  ({:.1}s)",
            started.elapsed().as_secs_f64()
        );
    }

    println!("  sql:      promote: committing transaction…");
    let _ = io::stdout().flush();
    tx.commit()?;
    println!(
        "  sql:      promote: committed  ({:.1}s)  convs={} parts={} msgs={} atts={} taps={}",
        started.elapsed().as_secs_f64(),
        stats.conversations,
        stats.participants,
        stats.messages,
        stats.attachments,
        stats.tapbacks
    );

    Ok(stats)
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
                false,
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
                backfill_contacts: false,
                import_id: None,
                source_from_jsonl: true,
                paths: Some(&paths),
                media: MediaMode::Copy,
                wipe_sources: Some(vec!["go-sms-pro".into()]),
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
                backfill_contacts: false,
                import_id: None,
                source_from_jsonl: true,
                paths: Some(&paths),
                media: MediaMode::None,
                wipe_sources: Some(vec!["sms".into()]),
            },
        )
        .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.attachments, 0);
        assert_eq!(stats.assets_copied, 0);
    }
}
