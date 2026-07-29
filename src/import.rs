use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};
use serde::Serialize;

use crate::assets::{self, AssetStats, StoredAsset};
use crate::contacts;
use crate::exclude::ExcludeSet;
use crate::jsonl;
use crate::models::{AttachmentRecord, ExportRecord, MessageRecord, clean_body};
use crate::schema;

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
    pub db_path: &'a Path,
    /// Content-addressed asset store (SHA-named files under account/source).
    pub assets_dir: &'a Path,
    /// Root for resolving relative attachment paths in JSONL.
    pub asset_root: &'a Path,
    pub contacts_csv: &'a Path,
    pub exclude_csv: &'a Path,
    pub overwrite_contacts: bool,
    pub mode: ImportMode,
    pub source: &'a str,
    pub account_id: &'a str,
}

#[derive(Debug, Default, Serialize)]
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
    pub conversations_excluded: u64,
    pub messages_excluded: u64,
    pub participants_excluded: u64,
    pub messages_deduped: u64,
    pub messages_appended: u64,
    pub unknown_contacts: u64,
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
        self.conversations_excluded += other.conversations_excluded;
        self.messages_excluded += other.messages_excluded;
        self.participants_excluded += other.participants_excluded;
    }
}

struct PreparedAttachment {
    record: AttachmentRecord,
    stored: Option<StoredAsset>,
}

/// Import every `*.jsonl` file under `export_dir` (CLI staging path).
pub fn import_export(
    export_dir: &Path,
    db_path: &Path,
    assets_dir: &Path,
    contacts_csv: &Path,
    exclude_csv: &Path,
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

    import_jsonl_files(
        &paths,
        &ImportOptions {
            db_path,
            assets_dir,
            asset_root: export_dir,
            contacts_csv,
            exclude_csv,
            overwrite_contacts,
            mode,
            source,
            account_id,
        },
    )
}

/// Import one or more JSONL files. Attachment relative paths resolve against `opts.asset_root`.
pub fn import_jsonl_files(paths: &[PathBuf], opts: &ImportOptions<'_>) -> Result<ImportStats> {
    if opts.source.trim().is_empty() {
        bail!("import source id must not be empty");
    }

    if let Some(parent) = opts.db_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    fs::create_dir_all(opts.assets_dir)
        .with_context(|| format!("failed to create {}", opts.assets_dir.display()))?;

    let mut conn = Connection::open(opts.db_path)
        .with_context(|| format!("failed to open database {}", opts.db_path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    println!("  sql:      opened {}", opts.db_path.display());
    let _ = io::stdout().flush();

    schema::ensure_vault_schema(&conn)?;
    crate::vault_owner::ensure_account_row(&conn, opts.account_id)?;

    println!(
        "  sql:      loading contacts from {}…",
        opts.contacts_csv.display()
    );
    let _ = io::stdout().flush();
    let contact_stats = contacts::load_contacts_if_needed(
        &mut conn,
        opts.contacts_csv,
        opts.overwrite_contacts,
        opts.account_id,
    )?;
    if contact_stats.skipped {
        println!("  sql:      contacts skipped (already loaded)");
    } else {
        println!(
            "  sql:      contacts={} phones={} labels={}",
            contact_stats.contacts, contact_stats.phones, contact_stats.labels
        );
    }
    let exclude = ExcludeSet::load(opts.exclude_csv)?;
    println!(
        "  sql:      exclude entries from {}",
        opts.exclude_csv.display()
    );

    println!("  sql:      ensuring schema + resetting staging for account…");
    let _ = io::stdout().flush();
    schema::ensure_messages_schema(&conn)?;
    schema::reset_staging_for_account(&conn, opts.account_id)?;
    if opts.mode == ImportMode::Replace {
        println!(
            "  sql:      deleting existing messages for source '{}'…",
            opts.source
        );
        let _ = io::stdout().flush();
        schema::delete_messages_for_source(&conn, opts.account_id, opts.source)?;
        println!("  sql:      wipe complete");
    }
    let _ = io::stdout().flush();

    let total_files = paths.len();
    println!(
        "  import:   {} NDJSON file{}",
        total_files,
        if total_files == 1 { "" } else { "s" }
    );
    if opts.mode == ImportMode::Replace {
        println!(
            "  import:   wiped existing rows for source '{}'",
            opts.source
        );
    }
    let _ = io::stdout().flush();

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
    let mut stmts = StagingInserts::prepare(&tx, opts.account_id)?;

    for (idx, path) in paths.iter().enumerate() {
        let file_stats = import_file_to_staging(
            &tx,
            &mut stmts,
            opts.asset_root,
            opts.assets_dir,
            path,
            &exclude,
            &mut asset_stats,
            opts.source,
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
            stmts = StagingInserts::prepare(&tx, opts.account_id)?;
        }
    }
    drop(stmts);
    tx.commit()?;

    println!(
        "  import:   promoting staging → production ({:.0}s so far)…",
        started.elapsed().as_secs_f64()
    );
    let _ = io::stdout().flush();
    let promote_stats = promote_append(&mut conn, opts.mode, opts.account_id)?;
    stats.messages_deduped += promote_stats.messages_deduped;
    stats.messages_appended = promote_stats.messages_appended;
    if opts.mode == ImportMode::Append {
        stats.conversations = promote_stats.conversations;
        stats.participants = promote_stats.participants;
        stats.messages = promote_stats.messages;
        stats.attachments = promote_stats.attachments;
        stats.tapbacks = promote_stats.tapbacks;
    }

    schema::clear_staging_for_account(&conn, opts.account_id)?;

    let unknown = contacts::ensure_unknown_contacts(&mut conn, opts.account_id, opts.contacts_csv)?;
    stats.unknown_contacts = unknown;
    if unknown > 0 {
        println!("  sql:      created {unknown} contact(s) for previously unassigned handles");
    }

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

/// Write JSONL bytes to a temp file and import (HTTP body / single stream).
#[allow(dead_code)] // retained for callers/tests that pass an in-memory body
pub fn import_jsonl_bytes(bytes: &[u8], opts: &ImportOptions<'_>) -> Result<ImportStats> {
    let dir = tempfile::tempdir().context("failed to create temp dir for JSONL import")?;
    let path = dir.path().join("import.jsonl");
    fs::write(&path, bytes).context("failed to write temp JSONL")?;
    import_jsonl_files(&[path], opts)
}

fn prepare_attachments(
    export_dir: &Path,
    assets_dir: &Path,
    attachments: Vec<AttachmentRecord>,
    asset_stats: &mut AssetStats,
) -> Result<Vec<PreparedAttachment>> {
    let mut prepared = Vec::with_capacity(attachments.len());
    for att in attachments {
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
                match assets::store_verified(&source, sha, assets_dir, att.mime_type.as_deref()) {
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
    conv: Statement<'conn>,
    part: Statement<'conn>,
    msg: Statement<'conn>,
    att: Statement<'conn>,
    tap: Statement<'conn>,
}

impl<'conn> StagingInserts<'conn> {
    fn prepare(tx: &'conn Transaction<'_>, account_id: &str) -> Result<Self> {
        Ok(Self {
            account_id: account_id.to_string(),
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
                    thread_originator_part, num_replies, sort_order
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
                )
                "#,
            )?,
            att: tx.prepare(
                r#"
                INSERT INTO staging_attachments (
                    message_id, path, original_name, mime_type, is_sticker, transcription,
                    sha256, assets_path
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
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

type ConversationHeader = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Vec<(String, Option<String>)>,
);

fn import_file_to_staging(
    tx: &Transaction<'_>,
    stmts: &mut StagingInserts<'_>,
    asset_root: &Path,
    assets_dir: &Path,
    path: &Path,
    exclude: &ExcludeSet,
    asset_stats: &mut AssetStats,
    source: &str,
) -> Result<ImportStats> {
    let source_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.jsonl")
        .to_string();

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
                        asset_root,
                        assets_dir,
                        &source_file,
                        header,
                        std::mem::take(&mut messages),
                        exclude,
                        asset_stats,
                        source,
                    )?);
                }
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
                ));
            }
            ExportRecord::Message(m) => {
                if pending.is_none() && source_file != "orphaned.json" {
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
            asset_root,
            assets_dir,
            &source_file,
            header,
            messages,
            exclude,
            asset_stats,
            source,
        )?);
    } else if source_file == "orphaned.json" {
        stats.merge_file(&import_conversation_to_staging(
            tx,
            stmts,
            asset_root,
            assets_dir,
            &source_file,
            (
                "orphaned".to_string(),
                None,
                "orphaned".to_string(),
                None,
                None,
                Vec::new(),
            ),
            messages,
            exclude,
            asset_stats,
            source,
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

fn import_conversation_to_staging(
    tx: &Transaction<'_>,
    stmts: &mut StagingInserts<'_>,
    asset_root: &Path,
    assets_dir: &Path,
    source_file: &str,
    conversation: ConversationHeader,
    messages: Vec<MessageRecord>,
    exclude: &ExcludeSet,
    asset_stats: &mut AssetStats,
    source: &str,
) -> Result<ImportStats> {
    let (chat_identifier, service, conversation_type, group_title, exported_at, participants) =
        conversation;

    let mut stats = ImportStats::default();

    if exclude.contains_handle(&chat_identifier) {
        stats.conversations_excluded = 1;
        return Ok(stats);
    }
    if conversation_type == "individual" {
        let peer_excluded = participants
            .iter()
            .any(|(handle, _)| exclude.contains_handle(handle));
        if peer_excluded {
            stats.conversations_excluded = 1;
            return Ok(stats);
        }
    }

    let kept_participants: Vec<(String, Option<String>)> = participants
        .into_iter()
        .filter(|(handle, _)| {
            if exclude.contains_handle(handle) {
                stats.participants_excluded += 1;
                false
            } else {
                true
            }
        })
        .collect();

    let kept_messages: Vec<MessageRecord> = messages
        .into_iter()
        .filter(|msg| {
            let excluded = msg
                .sender
                .as_deref()
                .is_some_and(|s| exclude.contains_handle(s));
            if excluded {
                stats.messages_excluded += 1;
                false
            } else {
                true
            }
        })
        .collect();

    let mut prepared_messages = Vec::with_capacity(kept_messages.len());
    for mut msg in kept_messages {
        let attachments = prepare_attachments(
            asset_root,
            assets_dir,
            std::mem::take(&mut msg.attachments),
            asset_stats,
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

            stmts.att.execute(params![
                message_id,
                att.path,
                att.original_name,
                mime_type,
                att.is_sticker as i64,
                att.transcription,
                sha256,
                assets_path,
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
) -> Result<PromoteStats> {
    let mut stats = PromoteStats::default();
    let started = Instant::now();

    // Bulk promote is much faster with a larger page cache and memory temp tables.
    conn.execute_batch(
        r#"
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -200000;
        "#,
    )?;

    let tx = conn.transaction()?;

    println!("  sql:      promote: reading staging conversations…");
    let _ = io::stdout().flush();
    let mut staging_convs = tx.prepare(
        r#"
        SELECT id, chat_identifier, service, conversation_type, group_title, exported_at, source_file
        FROM staging_conversations
        WHERE account_id = ?1
        ORDER BY id
        "#,
    )?;

    let staging_conv_rows: Vec<(
        i64,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = staging_convs
        .query_map(params![account_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(staging_convs);
    println!(
        "  sql:      promote: {} staging conversations → production…",
        staging_conv_rows.len()
    );
    let _ = io::stdout().flush();

    let mut conv_map: HashMap<i64, i64> = HashMap::new();
    let mut find_conv =
        tx.prepare("SELECT id FROM conversations WHERE account_id = ?1 AND chat_identifier = ?2")?;
    let mut update_conv = tx.prepare(
        r#"
        UPDATE conversations SET
            service = COALESCE(?2, service),
            conversation_type = ?3,
            group_title = COALESCE(?4, group_title),
            exported_at = COALESCE(?5, exported_at),
            source_file = ?6
        WHERE id = ?1
        "#,
    )?;
    let mut insert_conv = tx.prepare(
        r#"
        INSERT INTO conversations (
            account_id, chat_identifier, service, conversation_type, group_title, exported_at, source_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )?;

    for (
        staging_id,
        chat_identifier,
        service,
        conversation_type,
        group_title,
        exported_at,
        source_file,
    ) in staging_conv_rows
    {
        let existing: Option<i64> = find_conv
            .query_row(params![account_id, chat_identifier], |row| row.get(0))
            .optional()?;

        let prod_id = if let Some(id) = existing {
            update_conv.execute(params![
                id,
                service,
                conversation_type,
                group_title,
                exported_at,
                source_file
            ])?;
            id
        } else {
            insert_conv.execute(params![
                account_id,
                chat_identifier,
                service,
                conversation_type,
                group_title,
                exported_at,
                source_file,
            ])?;
            stats.conversations += 1;
            tx.last_insert_rowid()
        };
        conv_map.insert(staging_id, prod_id);
    }
    drop(find_conv);
    drop(update_conv);
    drop(insert_conv);
    println!(
        "  sql:      promote: conversations done (new={})  ({:.1}s)",
        stats.conversations,
        started.elapsed().as_secs_f64()
    );

    println!("  sql:      promote: reading staging participants…");
    let _ = io::stdout().flush();
    let mut staging_parts = tx.prepare(
        r#"
        SELECT conversation_id, handle, name_hint
        FROM staging_participants
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = ?1
        )
        ORDER BY id
        "#,
    )?;
    let staging_part_rows: Vec<(i64, String, Option<String>)> = staging_parts
        .query_map(params![account_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(staging_parts);
    println!(
        "  sql:      promote: {} staging participants → production…",
        staging_part_rows.len()
    );
    let _ = io::stdout().flush();

    let mut insert_part = tx.prepare(
        r#"
        INSERT OR IGNORE INTO participants (conversation_id, handle, name_hint)
        VALUES (?1, ?2, ?3)
        "#,
    )?;
    for (staging_conv_id, handle, name_hint) in staging_part_rows {
        let Some(&prod_conv_id) = conv_map.get(&staging_conv_id) else {
            continue;
        };
        let inserted = insert_part.execute(params![prod_conv_id, handle, name_hint])?;
        if inserted > 0 {
            stats.participants += 1;
        }
    }
    drop(insert_part);
    println!(
        "  sql:      promote: participants done (new={})  ({:.1}s)",
        stats.participants,
        started.elapsed().as_secs_f64()
    );

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
    {
        let mut ins =
            tx.prepare("INSERT INTO _promote_conv_map (staging_id, prod_id) VALUES (?1, ?2)")?;
        for (staging_id, prod_id) in &conv_map {
            ins.execute(params![staging_id, prod_id])?;
        }
    }

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

    let msg_map = if mode == ImportMode::Replace {
        // Source rows were wiped already: one set-based INSERT, then zip new ids in order.
        let max_before: i64 =
            tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
        let inserted = tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me, sender,
                subject, body, is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order
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
        // Append: insert only staging rows whose (source, guid) is not already in production,
        // then zip new ids in order (same mapping trick as replace).
        let max_before: i64 =
            tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;

        let new_filter = r#"
            (sm.guid IS NULL OR sm.guid = '')
            OR NOT EXISTS (
                SELECT 1 FROM messages m
                WHERE m.account_id = sm.account_id
                  AND m.source = sm.source
                  AND m.guid = sm.guid
            )
            "#;

        let staging_ids: Vec<i64> = tx
            .prepare(&format!(
                r#"
                SELECT sm.id
                FROM staging_messages sm
                JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
                WHERE sm.account_id = ?1 AND ({new_filter})
                ORDER BY sm.id
                "#
            ))?
            .query_map(params![account_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let inserted = tx.execute(
            &format!(
                r#"
                INSERT INTO messages (
                    conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me, sender,
                    subject, body, is_announcement, is_reply, thread_originator_guid,
                    thread_originator_part, num_replies, sort_order
                )
                SELECT
                    cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                    sm.sender, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                    sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order
                FROM staging_messages sm
                JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
                WHERE sm.account_id = ?1 AND ({new_filter})
                ORDER BY sm.id
                "#
            ),
            params![account_id],
        )?;
        stats.messages = inserted as u64;
        stats.messages_appended = inserted as u64;
        stats.messages_deduped = (total_msgs as u64).saturating_sub(inserted as u64);

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
            sha256, assets_path
        )
        SELECT
            mm.prod_id, sa.path, sa.original_name, sa.mime_type, sa.is_sticker, sa.transcription,
            sa.sha256, sa.assets_path
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

    println!("  sql:      promote: filling content keys…");
    let _ = io::stdout().flush();
    let keys = crate::dedupe::fill_missing_content_keys(&tx, account_id)?;
    println!(
        "  sql:      promote: content keys filled={keys}  ({:.1}s)",
        started.elapsed().as_secs_f64()
    );

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
