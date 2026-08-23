//! Stage message-ir JSONL rows into the temporary import tables.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use message_ir::{HandleService, HandleType};
use rusqlite::{Statement, Transaction, params};

use crate::assets::{self, AssetStats, StoredAsset};
use crate::config::validate_source_id;
use crate::db::handles::{infer_handle_type_from_shape as infer_handle_type, upsert_handle_row};
use crate::import_media::{self, MediaMode};
use crate::jsonl;
use crate::models::{AttachmentRecord, ExportRecord, MessageRecord, clean_body};

use super::contact_name::{
    apply_contact_name_mode, contact_preferred_name, ensure_sibling_contact_link,
    resolve_incoming_sender_handle, seed_contact_handle_alias,
};
use super::{ImportOptions, ImportStats};

struct PreparedAttachment {
    record: AttachmentRecord,
    stored: Option<StoredAsset>,
}

fn nonempty_rel(path: &Option<String>) -> Option<&str> {
    let raw = path.as_deref()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn nonempty_str(value: Option<&str>) -> Option<&str> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn stored_size_bytes(assets_dir: &Path, assets_path: Option<&str>) -> Option<i64> {
    let rel = assets_path?;
    let meta = std::fs::metadata(assets_dir.join(rel)).ok()?;
    Some(meta.len() as i64)
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
    // Bytes may have changed; drop any claimed SHA-256 fingerprint from the export.
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
    if let Some(sha) = nonempty_rel(&att.sha256) {
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

pub(super) struct StagingInserts<'conn> {
    account_id: String,
    import_id: Option<i64>,
    conv: Statement<'conn>,
    part: Statement<'conn>,
    msg: Statement<'conn>,
    att: Statement<'conn>,
    tap: Statement<'conn>,
}

impl<'conn> StagingInserts<'conn> {
    pub(super) fn prepare(
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
        let Some(source) = nonempty_str(export_source) else {
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
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    stem.eq_ignore_ascii_case("orphaned")
}

pub(super) fn import_file_to_staging<'conn>(
    tx: &Transaction<'conn>,
    stmts: &mut StagingInserts<'conn>,
    opts: &ImportOptions<'_>,
    path: &Path,
    asset_stats: &mut AssetStats,
    media_work: &Path,
) -> Result<ImportStats> {
    let source_file = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name.to_string(),
        None => "unknown.jsonl".to_string(),
    };
    let is_orphaned = is_orphaned_export(path);

    let records = jsonl::read_records(path)?;
    let mut stats = ImportStats::default();
    let mut pending: Option<ConversationHeader> = None;
    let mut messages: Vec<MessageRecord> = Vec::new();

    for record in records {
        match record {
            ExportRecord::Conversation(c) => {
                if let Some(header) = pending.take() {
                    stats.merge_file(&import_conversation_to_staging(ImportConversationArgs {
                        tx,
                        stmts,
                        opts,
                        source_file: &source_file,
                        conversation: header,
                        messages: std::mem::take(&mut messages),
                        asset_stats,
                        media_work,
                    })?);
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
        stats.merge_file(&import_conversation_to_staging(ImportConversationArgs {
            tx,
            stmts,
            opts,
            source_file: &source_file,
            conversation: header,
            messages,
            asset_stats,
            media_work,
        })?);
    } else if is_orphaned {
        if opts.source_from_jsonl {
            bail!(
                "{}: orphaned.jsonl without a conversation header cannot supply export.source",
                path.display()
            );
        }
        stats.merge_file(&import_conversation_to_staging(ImportConversationArgs {
            tx,
            stmts,
            opts,
            source_file: &source_file,
            conversation: (
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
        })?);
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

struct ImportConversationArgs<'a, 'conn> {
    tx: &'a Transaction<'conn>,
    stmts: &'a mut StagingInserts<'conn>,
    opts: &'a ImportOptions<'a>,
    source_file: &'a str,
    conversation: ConversationHeader,
    messages: Vec<MessageRecord>,
    asset_stats: &'a mut AssetStats,
    media_work: &'a Path,
}

fn import_conversation_to_staging(args: ImportConversationArgs<'_, '_>) -> Result<ImportStats> {
    let ImportConversationArgs {
        tx,
        stmts,
        opts,
        source_file,
        conversation,
        messages,
        asset_stats,
        media_work,
    } = args;
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

            let size_bytes = stored_size_bytes(&assets_dir, assets_path.as_deref())
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
