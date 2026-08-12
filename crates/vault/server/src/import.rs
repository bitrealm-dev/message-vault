use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use message_ir::{HandleService, HandleType};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params, params_from_iter};
use serde::Serialize;
use tempfile::TempDir;

use crate::assets::{self, AssetStats, StoredAsset};
use crate::config::{PathsConfig, validate_source_id};
use crate::db::contacts;
use crate::db::handles::{infer_handle_type_from_shape as infer_handle_type, upsert_handle_row};
use crate::db::schema;
use crate::db::sql::{SQLITE_IN_CHUNK, pair_placeholders};
use crate::db::vault_imports::{self, CompleteImportArgs};
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
    /// Keep the import display name unchanged (including empty / unknown).
    AsIs,
}

impl ContactNameMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "fill_missing" | "fill-missing" => Ok(Self::FillMissing),
            "overwrite" => Ok(Self::Overwrite),
            "as_is" | "as-is" | "leave" | "keep_import" | "keep-import" => Ok(Self::AsIs),
            other => bail!(
                "invalid contact_name_mode '{other}' (expected fill_missing, overwrite, or as_is)"
            ),
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
    /// Apply vault contact preferred names to import `name_alias` values.
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

    let paths = crate::import_cli::list_jsonl_files(export_dir)?;

    let mut conn = schema::open_configured(db_path)
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    schema::ensure_vault_schema(&conn)?;
    crate::db::account_profile::ensure_account_row(&conn, account_id)?;

    let import_id = vault_imports::start_import(
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

    let complete_args = match &result {
        Ok(stats) => CompleteImportArgs::succeeded(stats.messages, stats.attachments),
        Err(_) => CompleteImportArgs::failed(),
    };
    vault_imports::complete_import_or_warn(&conn, account_id, import_id, &complete_args);

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
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut conn = schema::open_configured(opts.db_path)
        .with_context(|| format!("failed to open database {}", opts.db_path.display()))?;
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
    } else {
        println!("  sql:      resetting staging for account…");
        let _ = io::stdout().flush();
    }
    schema::reset_staging_for_account(conn, opts.account_id)?;
    let wipe_sources: Vec<String> = if opts.mode == ImportMode::Replace {
        if opts.source_from_jsonl {
            opts.wipe_sources.clone().unwrap_or_default()
        } else {
            vec![opts.source.to_string()]
        }
    } else {
        Vec::new()
    };
    for source in &wipe_sources {
        validate_source_id(source)?;
    }
    let _ = io::stdout().flush();

    let total_files = paths.len();
    println!(
        "  import:   {} JSONL file{}",
        total_files,
        if total_files == 1 { "" } else { "s" }
    );
    if opts.mode == ImportMode::Replace {
        let wiped = wipe_sources.join(", ");
        println!("  import:   will wipe source(s) '{wiped}' after staging succeeds");
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
        &wipe_sources,
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

    schema::reset_staging_for_account(conn, opts.account_id)?;

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

fn nonempty_rel(path: &Option<String>) -> Option<&str> {
    path.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Convert/compress when requested; `None` means fall through to claimed-sha / path store.
fn try_store_converted(
    att: &mut AttachmentRecord,
    export_dir: &Path,
    assets_dir: &Path,
    asset_stats: &mut AssetStats,
    media: MediaMode,
    media_work: &Path,
) -> Result<Option<StoredAsset>> {
    if !matches!(media, MediaMode::Convert | MediaMode::Compress) {
        return Ok(None);
    }
    let Some(rel) = nonempty_rel(&att.path) else {
        return Ok(None);
    };
    let source = crate::config::resolve_under_root(export_dir, rel)?;
    if !source.is_file() {
        return Ok(None);
    }
    let Some(resolved) =
        import_media::resolve_for_store(&source, att.mime_type.as_deref(), media, media_work)?
    else {
        return Ok(None);
    };
    // Bytes may have changed; drop any claimed digest from the export.
    att.sha256 = None;
    att.mime_type = resolved.mime_type.or(att.mime_type.take());
    assets::hash_and_store(
        &resolved.path,
        assets_dir,
        att.mime_type.as_deref(),
        asset_stats,
    )
}

fn store_claimed_or_path(
    att: &AttachmentRecord,
    export_dir: &Path,
    assets_dir: &Path,
    asset_stats: &mut AssetStats,
) -> Result<Option<StoredAsset>> {
    if let Some(sha) = att
        .sha256
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(found) = assets::lookup_by_sha256(assets_dir, sha) {
            asset_stats.deduped += 1;
            return Ok(Some(StoredAsset {
                mime_type: att.mime_type.clone().or(found.mime_type),
                ..found
            }));
        }
        if let Some(rel) = nonempty_rel(&att.path) {
            let source = crate::config::resolve_under_root(export_dir, rel)?;
            return match assets::store_verified(
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
                    Ok(Some(stored))
                }
                Err(_) if !source.is_file() => {
                    asset_stats.missing += 1;
                    Ok(None)
                }
                Err(e) => Err(e),
            };
        }
        asset_stats.missing += 1;
        return Ok(None);
    }

    if let Some(rel) = att.path.as_deref() {
        let source = crate::config::resolve_under_root(export_dir, rel)?;
        return assets::hash_and_store(&source, assets_dir, att.mime_type.as_deref(), asset_stats);
    }
    asset_stats.missing += 1;
    Ok(None)
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
        let stored = match try_store_converted(
            &mut att,
            export_dir,
            assets_dir,
            asset_stats,
            media,
            media_work,
        )? {
            Some(stored) => Some(stored),
            None => store_claimed_or_path(&att, export_dir, assets_dir, asset_stats)?,
        };
        prepared.push(PreparedAttachment {
            record: att,
            stored,
        });
    }
    Ok(prepared)
}

fn resolve_incoming_sender_handle(
    tx: &Transaction<'_>,
    account_id: &str,
    is_from_me: bool,
    sender: Option<&str>,
    handle_type: Option<HandleType>,
    platform: &str,
    stats: &mut ImportStats,
) -> Result<Option<i64>> {
    if is_from_me {
        return Ok(None);
    }
    let Some(sender) = sender.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let handle_type = handle_type.unwrap_or_else(|| infer_handle_type(sender));
    let (handle_id, flagged) =
        upsert_handle_row(tx, account_id, sender, handle_type, Some(platform))?;
    if flagged {
        stats.phones_needing_review += 1;
    }
    let _ = ensure_sibling_contact_link(tx, account_id, handle_id)?;
    Ok(Some(handle_id))
}

/// If this handle has no contact but a sibling handle (same normalized + type,
/// different platform service) is already linked, attach this handle to that contact.
fn ensure_sibling_contact_link(
    conn: &Connection,
    account_id: &str,
    handle_id: i64,
) -> Result<Option<i64>> {
    if let Some(existing) = contacts::contact_id_for_handle(conn, account_id, handle_id)? {
        return Ok(Some(existing));
    }
    let sibling_contact: Option<i64> = conn
        .query_row(
            "SELECT ch.contact_id
             FROM handles h
             JOIN handles h2
               ON h2.account_id = h.account_id
              AND h2.normalized = h.normalized
              AND h2.handle_type = h.handle_type
              AND h2.id != h.id
             JOIN contact_handles ch
               ON ch.account_id = h.account_id AND ch.handle_id = h2.id
             WHERE h.id = ?1 AND h.account_id = ?2
             LIMIT 1",
            params![handle_id, account_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(contact_id) = sibling_contact else {
        return Ok(None);
    };
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO contact_handles (account_id, handle_id, contact_id)
         VALUES (?1, ?2, ?3)",
        params![account_id, handle_id, contact_id],
    )?;
    if inserted > 0 {
        crate::db::contacts::touch_contact(conn, account_id, contact_id)?;
    }
    Ok(Some(contact_id))
}

/// First-wins seed of `contact_handles.name_alias` from an import display name.
/// Only fills when the linked row exists and `name_alias` is empty.
fn seed_contact_handle_alias(
    conn: &Connection,
    account_id: &str,
    handle_id: i64,
    import_display: Option<&str>,
) -> Result<()> {
    let Some(alias) = import_display.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    conn.execute(
        "UPDATE contact_handles
         SET name_alias = ?1
         WHERE account_id = ?2
           AND handle_id = ?3
           AND (name_alias IS NULL OR trim(name_alias) = '')",
        params![alias, account_id, handle_id],
    )?;
    Ok(())
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
        ContactNameMode::AsIs => import_name,
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
                    account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
            )?,
            part: tx.prepare(
                r#"
                INSERT INTO staging_participants (conversation_id, handle_id, contact_id, name_alias)
                VALUES (?1, ?2, ?3, ?4)
                "#,
            )?,
            msg: tx.prepare(
                r#"
                INSERT OR IGNORE INTO staging_messages (
                    conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                    sender_handle_id, service, subject, body, is_announcement, is_reply,
                    thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
                )
                "#,
            )?,
            att: tx.prepare(
                r#"
                INSERT INTO staging_attachments (
                    message_id, path, original_name, mime_type, is_sticker, transcription,
                    sha256, assets_path, size_bytes, missing_reason
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
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

/// chat_identifier, platform_service, conversation_type, group_title, exported_at, participants, source
/// Participants are (handle, name_alias, handle_type).
/// `platform_service` is `phone` | `whatsapp` for handle rows.
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
        fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
        Ok(dir)
    } else {
        Ok(opts.assets_dir.to_path_buf())
    }
}

/// Messages with no conversation of their own live in `orphaned.jsonl`
/// (older bundles used `orphaned.json`), so they may omit a conversation header.
pub fn is_orphaned_export(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("orphaned"))
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
    let is_orphaned = is_orphaned_export(path);

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
                        .map(|p| (p.handle, p.name_alias, p.handle_type))
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
        platform_service,
        conversation_type,
        group_title,
        exported_at,
        participants,
        source,
    ) = conversation;

    let assets_dir = assets_dir_for_source(opts, &source)?;
    let mut stats = ImportStats::default();
    let kept_participants = participants;

    // Platform for chat/participant handles: conversation hint, else export source.
    let platform = platform_service
        .as_deref()
        .map(HandleService::parse)
        .unwrap_or_else(|| {
            if source.eq_ignore_ascii_case("whatsapp") {
                HandleService::Whatsapp
            } else {
                HandleService::Phone
            }
        });
    let platform_str = platform.as_str();

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
    let (chat_handle_id, flagged) = upsert_handle_row(
        tx,
        &stmts.account_id,
        &chat_identifier,
        infer_handle_type(&chat_identifier),
        Some(platform_str),
    )?;
    if flagged {
        stats.phones_needing_review += 1;
    }
    let _ = ensure_sibling_contact_link(tx, &stmts.account_id, chat_handle_id)?;

    stmts.conv.execute(params![
        stmts.account_id,
        chat_handle_id,
        conversation_type,
        group_title,
        exported_at,
        source_file,
    ])?;
    let conversation_id = tx.last_insert_rowid();
    stats.conversations = 1;

    for (handle, name_alias, handle_type) in kept_participants {
        // Prefer the source-provided type; fall back to shape inference.
        let handle_type = handle_type.unwrap_or_else(|| infer_handle_type(&handle));
        let (handle_id, flagged) = upsert_handle_row(
            tx,
            &stmts.account_id,
            &handle,
            handle_type,
            Some(platform_str),
        )?;
        if flagged {
            stats.phones_needing_review += 1;
        }
        let contact_id = ensure_sibling_contact_link(tx, &stmts.account_id, handle_id)?;
        // Seed contact identity alias from the import display name (first wins).
        seed_contact_handle_alias(tx, &stmts.account_id, handle_id, name_alias.as_deref())?;
        let vault_name = match contact_id {
            Some(id) => contact_preferred_name(tx, &stmts.account_id, id)?,
            None => None,
        };
        let name_alias = apply_contact_name_mode(opts.contact_name_mode, name_alias, vault_name);
        stmts
            .part
            .execute(params![conversation_id, handle_id, contact_id, name_alias])?;
        stats.participants += 1;
    }

    for (sort_order, (msg, attachments)) in prepared_messages.into_iter().enumerate() {
        let body = if msg.is_announcement {
            clean_body(msg.announcement.as_deref()).or_else(|| clean_body(msg.text.as_deref()))
        } else {
            clean_body(msg.text.as_deref())
        };

        // Sender identity: platform from message transport (whatsapp vs phone).
        let sender_platform = msg
            .service
            .as_deref()
            .map(HandleService::parse)
            .unwrap_or(platform);
        let sender_handle_id = resolve_incoming_sender_handle(
            tx,
            &stmts.account_id,
            msg.is_from_me,
            msg.sender.as_deref(),
            msg.sender_handle_type,
            sender_platform.as_str(),
            &mut stats,
        )?;

        let inserted = stmts.msg.execute(params![
            conversation_id,
            &stmts.account_id,
            source,
            msg.guid,
            msg.timestamp,
            msg.timestamp_utc,
            msg.is_from_me as i64,
            sender_handle_id,
            msg.service,
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
                .map(|meta| meta.len() as i64)
                .or_else(|| att.size_bytes.map(|n| n as i64));

            // Bytes absent and reason set: keep metadata-only placeholder rows.
            let missing_reason = if sha256.is_none() {
                att.missing_reason
            } else {
                None
            };

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
                missing_reason,
            ])?;
            stats.attachments += 1;
        }

        for tap in msg.tapbacks {
            // Tapback sender: resolved to a handle row (NULL for own tapbacks,
            // matching the message `sender_handle_id` convention).
            let sender_handle_id = resolve_incoming_sender_handle(
                tx,
                &stmts.account_id,
                tap.is_from_me,
                tap.sender.as_deref(),
                None,
                sender_platform.as_str(),
                &mut stats,
            )?;
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
    wipe_sources: &[String],
) -> Result<PromoteStats> {
    let mut stats = PromoteStats::default();
    let started = Instant::now();

    let tx = conn.transaction()?;

    if mode == ImportMode::Replace {
        for source in wipe_sources {
            println!("  sql:      deleting existing messages for source '{source}'…");
            let _ = io::stdout().flush();
            schema::delete_messages_for_source(&tx, account_id, source)?;
        }
        if !wipe_sources.is_empty() {
            println!("  sql:      wipe complete (inside promote transaction)");
            let _ = io::stdout().flush();
        }
    }

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
    promote_log(format_args!(
        "{staging_conv_count} staging conversations → production…"
    ));

    let max_conv_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM conversations", [], |r| {
            r.get(0)
        })?;
    tx.execute(
        r#"
        INSERT INTO conversations (
            account_id, chat_handle_id, conversation_type,
            group_title, exported_at, source_file
        )
        SELECT
            account_id, chat_handle_id, conversation_type,
            group_title, exported_at, source_file
        FROM staging_conversations
        WHERE account_id = ?1
        ON CONFLICT(account_id, chat_handle_id) DO UPDATE SET
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
    promote_log(format_args!(
        "conversations done (new={})  ({:.1}s)",
        stats.conversations,
        started.elapsed().as_secs_f64()
    ));

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
    promote_log(format_args!(
        "{staging_part_count} staging participants → production…"
    ));
    stats.participants = u64::try_from(tx.execute(
        r#"
        INSERT OR IGNORE INTO participants (conversation_id, handle_id, contact_id, name_alias)
        SELECT cm.prod_id, sp.handle_id, sp.contact_id, sp.name_alias
        FROM staging_participants sp
        JOIN _promote_conv_map cm ON cm.staging_id = sp.conversation_id
        "#,
        [],
    )?)
    .unwrap_or(0);
    promote_log(format_args!(
        "participants done (new={})  ({:.1}s)",
        stats.participants,
        started.elapsed().as_secs_f64()
    ));

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
    promote_log(format_args!(
        "{total_msgs} staging messages → production ({})…",
        mode.as_str()
    ));

    // Skip per-row FTS trigger work during bulk message/attachment inserts; index once after.
    let phase = Instant::now();
    promote_log("pausing FTS triggers…");
    schema::drop_messages_fts_triggers(&tx)?;
    promote_phase_done(started, phase, "FTS triggers paused");

    let existing_msgs: i64 = tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let drop_secondary = should_drop_messages_secondary_indexes(total_msgs, existing_msgs);
    if drop_secondary {
        let phase = Instant::now();
        promote_log(format_args!(
            "dropping secondary message indexes (staging={total_msgs} existing={existing_msgs})…"
        ));
        schema::drop_messages_secondary_indexes(&tx)?;
        promote_phase_done(started, phase, "secondary indexes dropped");
    } else {
        promote_log(format_args!(
            "keeping secondary message indexes (staging={total_msgs} existing={existing_msgs})"
        ));
    }

    let msg_map = promote_messages_chunked(&tx, mode, account_id, total_msgs, &mut stats, started)?;

    if drop_secondary {
        let phase = Instant::now();
        promote_log("rebuilding secondary message indexes…");
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
        promote_log(format_args!(
            "messages done (inserted={} skipped={})  (total {:.1}s)",
            stats.messages,
            stats.messages_deduped,
            started.elapsed().as_secs_f64()
        ));
    }

    let phase = Instant::now();
    promote_log(format_args!(
        "writing message id map ({} pairs)…",
        msg_map.len()
    ));
    fill_promote_msg_map(&tx, &msg_map)?;
    promote_phase_done(started, phase, "message id map written");

    let phase = Instant::now();
    promote_log("bulk-inserting attachments…");
    let att_inserted = tx.execute(
        r#"
        INSERT INTO attachments (
            message_id, path, original_name, mime_type, is_sticker, transcription,
            sha256, assets_path, size_bytes, missing_reason
        )
        SELECT
            mm.prod_id, sa.path, sa.original_name, sa.mime_type, sa.is_sticker, sa.transcription,
            sa.sha256, sa.assets_path, sa.size_bytes, sa.missing_reason
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
    promote_log("bulk-inserting tapbacks…");
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
    promote_log("bulk-indexing FTS for new messages…");
    let fts_indexed = schema::index_messages_fts_from_promote_map(&tx)?;
    schema::install_messages_fts_triggers(&tx)?;
    promote_phase_done(
        started,
        phase,
        format!("FTS indexed={fts_indexed} (triggers restored)"),
    );

    if fill_content_keys {
        let phase = Instant::now();
        promote_log("filling content keys…");
        let keys = crate::dedupe::fill_missing_content_keys(&tx, account_id)?;
        promote_phase_done(started, phase, format!("content keys filled={keys}"));
    }

    let phase = Instant::now();
    promote_log("committing transaction…");
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
/// Drop secondary indexes only for large promotes relative to the existing table.
const PROMOTE_INDEX_DROP_MIN_STAGING: i64 = 5_000;

/// Announce a promote phase. Flushed so piped output streams during long imports.
fn promote_log(msg: impl std::fmt::Display) {
    println!("  sql:      promote: {msg}");
    let _ = io::stdout().flush();
}

fn promote_phase_done(total: Instant, phase: Instant, msg: impl std::fmt::Display) {
    promote_log(format_args!(
        "{msg}  (phase {:.1}s, total {:.1}s)",
        phase.elapsed().as_secs_f64(),
        total.elapsed().as_secs_f64()
    ));
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
        promote_log(format_args!(
            "inserting messages chunk {chunk_idx} (staging id {}..{}, replace)…",
            lo + 1,
            hi
        ));

        let inserted = tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, service, subject, body, is_announcement, is_reply,
                thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender_handle_id, sm.service, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
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
        max_before = zip_new_message_ids(tx, &mut msg_map, staging_ids, max_before, |n, p| {
            format!(
                "promote replace message id map mismatch: staging={n} new_prod={p} (chunk staging id {}..{hi})",
                lo + 1
            )
        })?;

        promote_phase_done(
            started,
            phase,
            format!("chunk {chunk_idx} inserted={inserted} running={inserted_total}/{total_msgs}"),
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
        promote_log(format_args!(
            "inserting messages chunk {chunk_idx} (staging id {}..{}, append)…",
            lo + 1,
            hi
        ));

        let inserted = tx.execute(
            r#"
            INSERT OR IGNORE INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, service, subject, body, is_announcement, is_reply,
                thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender_handle_id, sm.service, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
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
        max_before = zip_new_message_ids(tx, &mut msg_map, staging_ids, max_before, |n, p| {
            format!(
                "promote append message id map mismatch: staging_new={n} new_prod={p} (chunk staging id {}..{hi})",
                lo + 1
            )
        })?;

        promote_phase_done(
            started,
            phase,
            format!("chunk {chunk_idx} inserted={inserted} running={inserted_total}/{total_msgs}"),
        );
        lo = hi;
    }

    // Null/empty guids are outside the partial unique index — always insert.
    let phase = Instant::now();
    promote_log("inserting messages with empty guids…");
    let empty_max_before = max_before;
    let inserted_empty = tx.execute(
        r#"
        INSERT INTO messages (
            conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
            sender_handle_id, service, subject, body, is_announcement, is_reply,
            thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
        )
        SELECT
            cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
            sm.sender_handle_id, sm.service, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
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
    zip_new_message_ids(
        tx,
        &mut msg_map,
        empty_staging_ids,
        empty_max_before,
        |n, p| format!("promote append empty-guid id map mismatch: staging={n} new_prod={p}"),
    )?;
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

fn zip_new_message_ids(
    tx: &Transaction<'_>,
    msg_map: &mut HashMap<i64, i64>,
    staging_ids: Vec<i64>,
    max_before: i64,
    mismatch: impl FnOnce(usize, usize) -> String,
) -> Result<i64> {
    let prod_ids: Vec<i64> = tx
        .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id")?
        .query_map(params![max_before], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if staging_ids.len() != prod_ids.len() {
        bail!("{}", mismatch(staging_ids.len(), prod_ids.len()));
    }
    for (staging_id, prod_id) in staging_ids.into_iter().zip(prod_ids) {
        msg_map.insert(staging_id, prod_id);
    }
    Ok(tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?)
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
    for chunk in pairs.chunks(SQLITE_IN_CHUNK) {
        let sql = format!(
            "INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES {}",
            pair_placeholders(chunk.len())
        );
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
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(row.status, "completed");
        assert_eq!(row.message_count, 1);

        let listed =
            crate::db::vault_imports::list_imports_for_account(&conn, TEST_ACCOUNT, 10).unwrap();
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
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, ?2, ?2, 'phone', 'phone')",
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

    fn participant_name_alias(db: &Path) -> Option<String> {
        let conn = Connection::open(db).unwrap();
        conn.query_row("SELECT name_alias FROM participants LIMIT 1", [], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()
        .unwrap()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    }

    fn contact_handle_name_alias(db: &Path) -> Option<String> {
        let conn = Connection::open(db).unwrap();
        conn.query_row("SELECT name_alias FROM contact_handles LIMIT 1", [], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()
        .unwrap()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
        assert_eq!(
            apply_contact_name_mode(ContactNameMode::AsIs, None, Some("Vault".into())),
            None
        );
        assert_eq!(
            apply_contact_name_mode(
                ContactNameMode::AsIs,
                Some("Import".into()),
                Some("Vault".into())
            ),
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
        assert_eq!(participant_name_alias(&db).as_deref(), Some("Backup Bob"));
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
        assert_eq!(participant_name_alias(&db).as_deref(), Some("Vault Alice"));
    }

    #[test]
    fn contact_name_mode_as_is_ignores_vault() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice");
        let path = write_jsonl(
            tmp.path(),
            "as-is.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-asis","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
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
        opts.contact_name_mode = ContactNameMode::AsIs;
        import_jsonl_files(&[path], &opts).unwrap();
        assert_eq!(participant_name_alias(&db), None);
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
        assert_eq!(participant_name_alias(&db).as_deref(), Some("Vault Alice"));
    }

    #[test]
    fn seed_contact_handle_alias_unit_first_wins() {
        let conn = Connection::open_in_memory().unwrap();
        schema::configure_connection(&conn).unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        crate::db::account_profile::ensure_account_row(&conn, TEST_ACCOUNT).unwrap();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Pat')",
            params![TEST_ACCOUNT],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550999', '+15555550999', 'phone', 'phone')",
            params![TEST_ACCOUNT],
        )
        .unwrap();
        let handle_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES (?1, ?2, ?3)",
            params![TEST_ACCOUNT, handle_id, contact_id],
        )
        .unwrap();

        seed_contact_handle_alias(&conn, TEST_ACCOUNT, handle_id, Some("First")).unwrap();
        seed_contact_handle_alias(&conn, TEST_ACCOUNT, handle_id, Some("Second")).unwrap();
        let alias: Option<String> = conn
            .query_row(
                "SELECT name_alias FROM contact_handles WHERE handle_id = ?1",
                [handle_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(alias.as_deref(), Some("First"));
    }

    #[test]
    fn contact_handle_alias_seeds_first_wins() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice");
        assert!(contact_handle_name_alias(&db).is_none());

        let path1 = write_jsonl(
            tmp.path(),
            "alias1.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Backup Bob"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-alias1","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
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
        import_jsonl_files(&[path1], &opts).unwrap();
        assert_eq!(
            contact_handle_name_alias(&db).as_deref(),
            Some("Backup Bob")
        );

        let path2 = write_jsonl(
            tmp.path(),
            "alias2.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Other Name"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183463000,"last_timestamp_unix_ms":1426183463000}}}
{"guid":"g-alias2","timestamp_unix_ms":1426183463000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"yo","attachments":[],"imessage":null,"source":null}
"#,
        );
        import_jsonl_files(&[path2], &opts).unwrap();
        assert_eq!(
            contact_handle_name_alias(&db).as_deref(),
            Some("Backup Bob")
        );
    }

    #[test]
    fn sibling_contact_link_bumps_last_modified_only_on_insert() {
        let conn = Connection::open_in_memory().unwrap();
        schema::configure_connection(&conn).unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        crate::db::account_profile::ensure_account_row(&conn, TEST_ACCOUNT).unwrap();

        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Pat')",
            params![TEST_ACCOUNT],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![TEST_ACCOUNT],
        )
        .unwrap();
        let phone_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES (?1, ?2, ?3)",
            params![TEST_ACCOUNT, phone_id, contact_id],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'whatsapp')",
            params![TEST_ACCOUNT],
        )
        .unwrap();
        let wa_id = conn.last_insert_rowid();

        const OLD: &str = "2000-01-01 00:00:00";
        conn.execute(
            "UPDATE contacts SET last_modified = ?1 WHERE id = ?2",
            params![OLD, contact_id],
        )
        .unwrap();

        let linked = ensure_sibling_contact_link(&conn, TEST_ACCOUNT, wa_id)
            .unwrap()
            .expect("sibling link");
        assert_eq!(linked, contact_id);
        let after_insert: String = conn
            .query_row(
                "SELECT last_modified FROM contacts WHERE id = ?1",
                params![contact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(after_insert, OLD);

        conn.execute(
            "UPDATE contacts SET last_modified = ?1 WHERE id = ?2",
            params![OLD, contact_id],
        )
        .unwrap();
        let again = ensure_sibling_contact_link(&conn, TEST_ACCOUNT, wa_id)
            .unwrap()
            .expect("already linked");
        assert_eq!(again, contact_id);
        let after_noop: String = conn
            .query_row(
                "SELECT last_modified FROM contacts WHERE id = ?1",
                params![contact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after_noop, OLD);
    }

    #[test]
    fn persists_missing_reason_with_null_sha256() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        fs::create_dir_all(&assets).unwrap();

        let path = write_jsonl(
            tmp.path(),
            "missing-att.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-missing","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"see attached","attachments":[{"path":"attachments/gone.bin","original_name":"gone.bin","mime_type":"application/octet-stream","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":999,"missing_reason":"too_large"}],"imessage":null,"source":null}
"#,
        );
        let stats = import_jsonl_files(
            &[path],
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
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.attachments, 1);

        let conn = Connection::open(&db).unwrap();
        let (sha256, missing_reason, size_bytes, original_name): (
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT sha256, missing_reason, size_bytes, original_name FROM attachments LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert!(sha256.is_none());
        assert_eq!(missing_reason.as_deref(), Some("too_large"));
        assert_eq!(size_bytes, Some(999));
        assert_eq!(original_name.as_deref(), Some("gone.bin"));
    }

    #[test]
    fn rejects_attachment_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let export_dir = tmp.path().join("export");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(tmp.path().join("secret.txt"), b"secret-bytes").unwrap();

        let path = write_jsonl(
            &export_dir,
            "traverse.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-trav","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"x","attachments":[{"path":"../secret.txt","original_name":"secret.txt","mime_type":"text/plain","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":12,"missing_reason":null}],"imessage":null,"source":null}
"#,
        );
        let err = import_jsonl_files(
            &[path],
            &ImportOptions::fixed(
                &db,
                &assets,
                &export_dir,
                None,
                false,
                ImportMode::Append,
                "sms-backup-restore",
                TEST_ACCOUNT,
                false,
                None,
            ),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unsafe attachment path"),
            "expected path rejection, got: {err}"
        );
    }

    #[test]
    fn failed_replace_keeps_existing_messages() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let export_dir = tmp.path().join("export");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&export_dir).unwrap();

        let first = write_jsonl(
            &export_dir,
            "ok.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-keep-replace","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"keep me","attachments":[],"imessage":null,"source":null}
"#,
        );
        import_jsonl_files(
            &[first],
            &ImportOptions::fixed(
                &db,
                &assets,
                &export_dir,
                None,
                false,
                ImportMode::Replace,
                "sms-backup-restore",
                TEST_ACCOUNT,
                false,
                None,
            ),
        )
        .unwrap();

        let bad = write_jsonl(
            &export_dir,
            "bad.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-bad","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"nope","attachments":[{"path":"../secret.txt","original_name":"secret.txt","mime_type":"text/plain","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":1,"missing_reason":null}],"imessage":null,"source":null}
"#,
        );
        let err = import_jsonl_files(
            &[bad],
            &ImportOptions::fixed(
                &db,
                &assets,
                &export_dir,
                None,
                false,
                ImportMode::Replace,
                "sms-backup-restore",
                TEST_ACCOUNT,
                false,
                None,
            ),
        )
        .unwrap_err();
        assert!(err.to_string().contains("unsafe attachment path"));

        let conn = Connection::open(&db).unwrap();
        let body: String = conn
            .query_row(
                "SELECT body FROM messages WHERE guid = 'g-keep-replace'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(body, "keep me");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
