//! Clone a template vault account into a new guest (SQL rows + hard-linked files).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::config::Config;
use crate::db::account_profile;

/// Copy the template account's rows and files into a new guest account.
///
/// Integer primary keys are remapped. `account_emails`, session tokens, and API
/// tokens are not copied. Attachment files are hard-linked (or copied) after
/// the SQL transaction commits. If file linking fails, the guest account is
/// deleted so a half-created copy is not left behind.
///
/// # Errors
///
/// Returns an error when the template is missing, a SQL statement fails, or
/// files cannot be linked or copied.
pub fn clone_template_to_guest(
    conn: &mut Connection,
    cfg: &Config,
    template_account_id: &str,
) -> Result<String> {
    let guest_id = {
        let tx = conn.transaction()?;
        let guest_id = clone_sql(&tx, template_account_id)?;
        tx.commit()?;
        guest_id
    };

    let src_root = cfg.paths.data_dir.join(template_account_id);
    let dest_root = cfg.paths.data_dir.join(&guest_id);
    if let Err(err) = link_tree(&src_root, &dest_root) {
        let cleanup = account_profile::delete_account(conn, &guest_id);
        let _ = fs::remove_dir_all(&dest_root);
        if let Err(cleanup_err) = cleanup {
            return Err(err.context(format!(
                "file clone failed; also failed to delete guest {guest_id}: {cleanup_err}"
            )));
        }
        return Err(err);
    }
    Ok(guest_id)
}

fn clone_sql(tx: &Transaction<'_>, template: &str) -> Result<String> {
    let exists: Option<String> = tx
        .query_row(
            "SELECT id FROM accounts WHERE id = ?1",
            params![template],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        bail!("template account {template} not found");
    }

    let guest_id = uuid::Uuid::new_v4().to_string();
    let hex = guest_id.replace('-', "");
    let username = format!("guest-{}", &hex[..8]);
    let preferred = account_profile::load_preferred_name(tx, template)?;
    account_profile::insert_guest_account(tx, &guest_id, &username, preferred.as_deref())?;

    let handle_map = copy_handles(tx, template, &guest_id)?;
    let contact_map = copy_contacts(tx, template, &guest_id)?;
    copy_contact_handles(tx, template, &guest_id, &handle_map, &contact_map)?;
    copy_account_handles(tx, template, &guest_id, &handle_map)?;
    let label_map = copy_contact_labels(tx, template, &guest_id)?;
    copy_contact_label_members(tx, template, &contact_map, &label_map)?;
    copy_trashed_handles(tx, template, &guest_id, &handle_map)?;
    copy_trashed_contacts(tx, template, &guest_id, &contact_map)?;

    let import_map = copy_vault_imports(tx, template, &guest_id)?;
    copy_vault_import_issues(tx, &import_map)?;

    let conversation_map = copy_conversations(tx, template, &guest_id, &handle_map)?;
    copy_trashed_conversations(tx, template, &guest_id, &conversation_map)?;
    copy_participants(tx, template, &conversation_map, &handle_map, &contact_map)?;

    let message_map = copy_messages(
        tx,
        template,
        &guest_id,
        &conversation_map,
        &handle_map,
        &import_map,
    )?;
    copy_attachments(tx, template, &message_map)?;
    copy_tapbacks(tx, template, &message_map, &handle_map)?;

    let staging_conv_map = copy_staging_conversations(tx, template, &guest_id, &handle_map)?;
    copy_staging_participants(tx, template, &staging_conv_map, &handle_map, &contact_map)?;
    let staging_msg_map = copy_staging_messages(
        tx,
        template,
        &guest_id,
        &staging_conv_map,
        &handle_map,
        &import_map,
    )?;
    copy_staging_attachments(tx, template, &staging_msg_map)?;
    copy_staging_tapbacks(tx, template, &staging_msg_map, &handle_map)?;

    copy_account_prefs(tx, template, &guest_id)?;
    Ok(guest_id)
}

fn collect_rows<T>(
    tx: &Transaction<'_>,
    sql: &str,
    account_id: &str,
    f: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt
        .query_map(params![account_id], f)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn mapped(map: &HashMap<i64, i64>, old: i64) -> Option<i64> {
    map.get(&old).copied()
}

fn mapped_opt(map: &HashMap<i64, i64>, old: Option<i64>) -> Option<i64> {
    old.and_then(|id| map.get(&id).copied())
}

fn copy_handles(tx: &Transaction<'_>, template: &str, guest: &str) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, raw, normalized, normalized_note, handle_type, service
        FROM handles WHERE account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, raw, normalized, note, handle_type, service) in rows {
        tx.execute(
            r#"
            INSERT INTO handles (
                account_id, raw, normalized, normalized_note, handle_type, service
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![guest, raw, normalized, note, handle_type, service],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_contacts(tx: &Transaction<'_>, template: &str, guest: &str) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        "SELECT id, preferred_name, last_modified FROM contacts WHERE account_id = ?1",
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, preferred_name, last_modified) in rows {
        tx.execute(
            "INSERT INTO contacts (account_id, preferred_name, last_modified) VALUES (?1, ?2, ?3)",
            params![guest, preferred_name, last_modified],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_contact_handles(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
    contacts: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT handle_id, contact_id, name_alias FROM contact_handles WHERE account_id = ?1",
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )?;
    for (handle_id, contact_id, name_alias) in rows {
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        let Some(new_contact) = mapped(contacts, contact_id) else {
            continue;
        };
        tx.execute(
            r#"
            INSERT INTO contact_handles (account_id, handle_id, contact_id, name_alias)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![guest, new_handle, new_contact, name_alias],
        )?;
    }
    Ok(())
}

fn copy_account_handles(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT handle_id FROM account_handles WHERE account_id = ?1",
        template,
        |row| row.get::<_, i64>(0),
    )?;
    for handle_id in rows {
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        tx.execute(
            "INSERT INTO account_handles (account_id, handle_id) VALUES (?1, ?2)",
            params![guest, new_handle],
        )?;
    }
    Ok(())
}

fn copy_contact_labels(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        "SELECT id, name FROM contact_labels WHERE account_id = ?1",
        template,
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, name) in rows {
        tx.execute(
            "INSERT INTO contact_labels (account_id, name) VALUES (?1, ?2)",
            params![guest, name],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_contact_label_members(
    tx: &Transaction<'_>,
    template: &str,
    contacts: &HashMap<i64, i64>,
    labels: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT clm.contact_id, clm.label_id
        FROM contact_label_members clm
        JOIN contacts c ON c.id = clm.contact_id
        WHERE c.account_id = ?1
        "#,
        template,
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    for (contact_id, label_id) in rows {
        let Some(new_contact) = mapped(contacts, contact_id) else {
            continue;
        };
        let Some(new_label) = mapped(labels, label_id) else {
            continue;
        };
        tx.execute(
            "INSERT INTO contact_label_members (contact_id, label_id) VALUES (?1, ?2)",
            params![new_contact, new_label],
        )?;
    }
    Ok(())
}

fn copy_trashed_handles(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT handle_id, trashed_at FROM trashed_handles WHERE account_id = ?1",
        template,
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    for (handle_id, trashed_at) in rows {
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        tx.execute(
            "INSERT INTO trashed_handles (account_id, handle_id, trashed_at) VALUES (?1, ?2, ?3)",
            params![guest, new_handle, trashed_at],
        )?;
    }
    Ok(())
}

fn copy_trashed_contacts(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    contacts: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT contact_id, trashed_at FROM trashed_contacts WHERE account_id = ?1",
        template,
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    for (contact_id, trashed_at) in rows {
        let Some(new_contact) = mapped(contacts, contact_id) else {
            continue;
        };
        tx.execute(
            "INSERT INTO trashed_contacts (account_id, contact_id, trashed_at) VALUES (?1, ?2, ?3)",
            params![guest, new_contact, trashed_at],
        )?;
    }
    Ok(())
}

fn copy_vault_imports(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, source, tool, mode, status, started_at, finished_at,
               message_count, attachment_count, bytes_uploaded,
               duration_ms, parse_ms, convert_ms, upload_ms, summary_json
        FROM vault_imports WHERE account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<i64>>(11)?,
                row.get::<_, Option<i64>>(12)?,
                row.get::<_, Option<i64>>(13)?,
                row.get::<_, Option<String>>(14)?,
            ))
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (
        old_id,
        source,
        tool,
        mode,
        status,
        started_at,
        finished_at,
        message_count,
        attachment_count,
        bytes_uploaded,
        duration_ms,
        parse_ms,
        convert_ms,
        upload_ms,
        summary_json,
    ) in rows
    {
        tx.execute(
            r#"
            INSERT INTO vault_imports (
                account_id, source, tool, mode, status, started_at, finished_at,
                message_count, attachment_count, bytes_uploaded,
                duration_ms, parse_ms, convert_ms, upload_ms, summary_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                guest,
                source,
                tool,
                mode,
                status,
                started_at,
                finished_at,
                message_count,
                attachment_count,
                bytes_uploaded,
                duration_ms,
                parse_ms,
                convert_ms,
                upload_ms,
                summary_json
            ],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_vault_import_issues(tx: &Transaction<'_>, imports: &HashMap<i64, i64>) -> Result<()> {
    if imports.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare(
        r#"
        SELECT import_id, kind, step, item, reason, created_at
        FROM vault_import_issues
        WHERE import_id = ?1
        "#,
    )?;
    let mut pending = Vec::new();
    for &old_import in imports.keys() {
        let rows = stmt
            .query_map(params![old_import], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        pending.extend(rows);
    }
    drop(stmt);
    for (old_import, kind, step, item, reason, created_at) in pending {
        let Some(new_import) = mapped(imports, old_import) else {
            continue;
        };
        tx.execute(
            r#"
            INSERT INTO vault_import_issues (import_id, kind, step, item, reason, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![new_import, kind, step, item, reason, created_at],
        )?;
    }
    Ok(())
}

fn copy_conversations(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, chat_handle_id, conversation_type, group_title, exported_at, source_file
        FROM conversations WHERE account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, chat_handle_id, conversation_type, group_title, exported_at, source_file) in rows {
        let Some(new_handle) = mapped(handles, chat_handle_id) else {
            continue;
        };
        tx.execute(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                guest,
                new_handle,
                conversation_type,
                group_title,
                exported_at,
                source_file
            ],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_trashed_conversations(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    conversations: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT conversation_id, trashed_at FROM trashed_conversations WHERE account_id = ?1",
        template,
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    for (conversation_id, trashed_at) in rows {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        tx.execute(
            r#"
            INSERT INTO trashed_conversations (account_id, conversation_id, trashed_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![guest, new_conv, trashed_at],
        )?;
    }
    Ok(())
}

fn copy_participants(
    tx: &Transaction<'_>,
    template: &str,
    conversations: &HashMap<i64, i64>,
    handles: &HashMap<i64, i64>,
    contacts: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT p.conversation_id, p.handle_id, p.contact_id, p.name_alias
        FROM participants p
        JOIN conversations c ON c.id = p.conversation_id
        WHERE c.account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    for (conversation_id, handle_id, contact_id, name_alias) in rows {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        let new_contact = mapped_opt(contacts, contact_id);
        tx.execute(
            r#"
            INSERT INTO participants (conversation_id, handle_id, contact_id, name_alias)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![new_conv, new_handle, new_contact, name_alias],
        )?;
    }
    Ok(())
}

struct MessageRow {
    id: i64,
    conversation_id: i64,
    source: String,
    guid: Option<String>,
    timestamp: String,
    timestamp_utc: Option<String>,
    is_from_me: i64,
    sender_handle_id: Option<i64>,
    service: Option<String>,
    subject: Option<String>,
    body: Option<String>,
    is_announcement: i64,
    is_reply: i64,
    thread_originator_guid: Option<String>,
    thread_originator_part: Option<i64>,
    num_replies: i64,
    sort_order: i64,
    content_key: Option<String>,
    duplicate_of: Option<i64>,
    import_id: Option<i64>,
}

fn copy_messages(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    conversations: &HashMap<i64, i64>,
    handles: &HashMap<i64, i64>,
    imports: &HashMap<i64, i64>,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, conversation_id, source, guid, timestamp, timestamp_utc,
               is_from_me, sender_handle_id, service, subject, body,
               is_announcement, is_reply, thread_originator_guid,
               thread_originator_part, num_replies, sort_order, content_key,
               duplicate_of, import_id
        FROM messages WHERE account_id = ?1
        "#,
        template,
        |row| {
            Ok(MessageRow {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                source: row.get(2)?,
                guid: row.get(3)?,
                timestamp: row.get(4)?,
                timestamp_utc: row.get(5)?,
                is_from_me: row.get(6)?,
                sender_handle_id: row.get(7)?,
                service: row.get(8)?,
                subject: row.get(9)?,
                body: row.get(10)?,
                is_announcement: row.get(11)?,
                is_reply: row.get(12)?,
                thread_originator_guid: row.get(13)?,
                thread_originator_part: row.get(14)?,
                num_replies: row.get(15)?,
                sort_order: row.get(16)?,
                content_key: row.get(17)?,
                duplicate_of: row.get(18)?,
                import_id: row.get(19)?,
            })
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    let mut pending_fks = Vec::new();
    for row in rows {
        let Some(new_conv) = mapped(conversations, row.conversation_id) else {
            continue;
        };
        let sender = mapped_opt(handles, row.sender_handle_id);
        tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc,
                is_from_me, sender_handle_id, service, subject, body,
                is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, content_key,
                duplicate_of, import_id
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, NULL, NULL
            )
            "#,
            params![
                new_conv,
                guest,
                row.source,
                row.guid,
                row.timestamp,
                row.timestamp_utc,
                row.is_from_me,
                sender,
                row.service,
                row.subject,
                row.body,
                row.is_announcement,
                row.is_reply,
                row.thread_originator_guid,
                row.thread_originator_part,
                row.num_replies,
                row.sort_order,
                row.content_key
            ],
        )?;
        let new_id = tx.last_insert_rowid();
        map.insert(row.id, new_id);
        pending_fks.push((new_id, row.duplicate_of, row.import_id));
    }
    for (new_id, old_dup, old_import) in pending_fks {
        let dup = mapped_opt(&map, old_dup);
        let import = mapped_opt(imports, old_import);
        if dup.is_none() && import.is_none() {
            continue;
        }
        tx.execute(
            "UPDATE messages SET duplicate_of = ?1, import_id = ?2 WHERE id = ?3",
            params![dup, import, new_id],
        )?;
    }
    Ok(map)
}

fn copy_attachments(
    tx: &Transaction<'_>,
    template: &str,
    messages: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT a.message_id, a.path, a.original_name, a.mime_type, a.is_sticker,
               a.transcription, a.sha256, a.assets_path, a.size_bytes, a.missing_reason,
               a.derived_sha256, a.derived_assets_path, a.derived_mime_type
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        WHERE m.account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        },
    )?;
    for (
        message_id,
        path,
        original_name,
        mime_type,
        is_sticker,
        transcription,
        sha256,
        assets_path,
        size_bytes,
        missing_reason,
        derived_sha256,
        derived_assets_path,
        derived_mime_type,
    ) in rows
    {
        let Some(new_msg) = mapped(messages, message_id) else {
            continue;
        };
        tx.execute(
            r#"
            INSERT INTO attachments (
                message_id, path, original_name, mime_type, is_sticker, transcription,
                sha256, assets_path, size_bytes, missing_reason,
                derived_sha256, derived_assets_path, derived_mime_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                new_msg,
                path,
                original_name,
                mime_type,
                is_sticker,
                transcription,
                sha256,
                assets_path,
                size_bytes,
                missing_reason,
                derived_sha256,
                derived_assets_path,
                derived_mime_type
            ],
        )?;
    }
    Ok(())
}

fn copy_tapbacks(
    tx: &Transaction<'_>,
    template: &str,
    messages: &HashMap<i64, i64>,
    handles: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT t.message_id, t.part_index, t.kind, t.emoji, t.is_from_me, t.sender_handle_id
        FROM tapbacks t
        JOIN messages m ON m.id = t.message_id
        WHERE m.account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        },
    )?;
    for (message_id, part_index, kind, emoji, is_from_me, sender_handle_id) in rows {
        let Some(new_msg) = mapped(messages, message_id) else {
            continue;
        };
        let sender = mapped_opt(handles, sender_handle_id);
        tx.execute(
            r#"
            INSERT INTO tapbacks (
                message_id, part_index, kind, emoji, is_from_me, sender_handle_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![new_msg, part_index, kind, emoji, is_from_me, sender],
        )?;
    }
    Ok(())
}

fn copy_staging_conversations(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, chat_handle_id, conversation_type, group_title, exported_at, source_file
        FROM staging_conversations WHERE account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, chat_handle_id, conversation_type, group_title, exported_at, source_file) in rows {
        let new_handle = mapped(handles, chat_handle_id).unwrap_or(chat_handle_id);
        tx.execute(
            r#"
            INSERT INTO staging_conversations (
                account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                guest,
                new_handle,
                conversation_type,
                group_title,
                exported_at,
                source_file
            ],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_staging_participants(
    tx: &Transaction<'_>,
    template: &str,
    conversations: &HashMap<i64, i64>,
    handles: &HashMap<i64, i64>,
    contacts: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT p.conversation_id, p.handle_id, p.contact_id, p.name_alias
        FROM staging_participants p
        JOIN staging_conversations c ON c.id = p.conversation_id
        WHERE c.account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    for (conversation_id, handle_id, contact_id, name_alias) in rows {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        let new_handle = mapped(handles, handle_id).unwrap_or(handle_id);
        let new_contact = mapped_opt(contacts, contact_id);
        tx.execute(
            r#"
            INSERT INTO staging_participants (conversation_id, handle_id, contact_id, name_alias)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![new_conv, new_handle, new_contact, name_alias],
        )?;
    }
    Ok(())
}

fn copy_staging_messages(
    tx: &Transaction<'_>,
    template: &str,
    guest: &str,
    conversations: &HashMap<i64, i64>,
    handles: &HashMap<i64, i64>,
    imports: &HashMap<i64, i64>,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, conversation_id, source, guid, timestamp, timestamp_utc,
               is_from_me, sender_handle_id, service, subject, body,
               is_announcement, is_reply, thread_originator_guid,
               thread_originator_part, num_replies, sort_order, import_id
        FROM staging_messages WHERE account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<i64>>(14)?,
                row.get::<_, i64>(15)?,
                row.get::<_, i64>(16)?,
                row.get::<_, Option<i64>>(17)?,
            ))
        },
    )?;
    let mut map = HashMap::with_capacity(rows.len());
    for (
        old_id,
        conversation_id,
        source,
        guid,
        timestamp,
        timestamp_utc,
        is_from_me,
        sender_handle_id,
        service,
        subject,
        body,
        is_announcement,
        is_reply,
        thread_originator_guid,
        thread_originator_part,
        num_replies,
        sort_order,
        import_id,
    ) in rows
    {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        let sender = mapped_opt(handles, sender_handle_id);
        let import = mapped_opt(imports, import_id);
        tx.execute(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc,
                is_from_me, sender_handle_id, service, subject, body,
                is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18
            )
            "#,
            params![
                new_conv,
                guest,
                source,
                guid,
                timestamp,
                timestamp_utc,
                is_from_me,
                sender,
                service,
                subject,
                body,
                is_announcement,
                is_reply,
                thread_originator_guid,
                thread_originator_part,
                num_replies,
                sort_order,
                import
            ],
        )?;
        map.insert(old_id, tx.last_insert_rowid());
    }
    Ok(map)
}

fn copy_staging_attachments(
    tx: &Transaction<'_>,
    template: &str,
    messages: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT a.message_id, a.path, a.original_name, a.mime_type, a.is_sticker,
               a.transcription, a.sha256, a.assets_path, a.size_bytes, a.missing_reason,
               a.derived_sha256, a.derived_assets_path, a.derived_mime_type
        FROM staging_attachments a
        JOIN staging_messages m ON m.id = a.message_id
        WHERE m.account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        },
    )?;
    for (
        message_id,
        path,
        original_name,
        mime_type,
        is_sticker,
        transcription,
        sha256,
        assets_path,
        size_bytes,
        missing_reason,
        derived_sha256,
        derived_assets_path,
        derived_mime_type,
    ) in rows
    {
        let Some(new_msg) = mapped(messages, message_id) else {
            continue;
        };
        tx.execute(
            r#"
            INSERT INTO staging_attachments (
                message_id, path, original_name, mime_type, is_sticker, transcription,
                sha256, assets_path, size_bytes, missing_reason,
                derived_sha256, derived_assets_path, derived_mime_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                new_msg,
                path,
                original_name,
                mime_type,
                is_sticker,
                transcription,
                sha256,
                assets_path,
                size_bytes,
                missing_reason,
                derived_sha256,
                derived_assets_path,
                derived_mime_type
            ],
        )?;
    }
    Ok(())
}

fn copy_staging_tapbacks(
    tx: &Transaction<'_>,
    template: &str,
    messages: &HashMap<i64, i64>,
    handles: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT t.message_id, t.part_index, t.kind, t.emoji, t.is_from_me, t.sender_handle_id
        FROM staging_tapbacks t
        JOIN staging_messages m ON m.id = t.message_id
        WHERE m.account_id = ?1
        "#,
        template,
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        },
    )?;
    for (message_id, part_index, kind, emoji, is_from_me, sender_handle_id) in rows {
        let Some(new_msg) = mapped(messages, message_id) else {
            continue;
        };
        let sender = mapped_opt(handles, sender_handle_id);
        tx.execute(
            r#"
            INSERT INTO staging_tapbacks (
                message_id, part_index, kind, emoji, is_from_me, sender_handle_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![new_msg, part_index, kind, emoji, is_from_me, sender],
        )?;
    }
    Ok(())
}

fn copy_account_prefs(tx: &Transaction<'_>, template: &str, guest: &str) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT key, value FROM account_prefs WHERE account_id = ?1",
        template,
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    for (key, value) in rows {
        tx.execute(
            "INSERT INTO account_prefs (account_id, key, value) VALUES (?1, ?2, ?3)",
            params![guest, key, value],
        )?;
    }
    Ok(())
}

fn link_or_copy(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::hard_link(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(src, dest)?;
            Ok(())
        }
    }
}

fn link_tree(src_root: &Path, dest_root: &Path) -> Result<()> {
    if !src_root.exists() {
        return Ok(());
    }
    link_tree_inner(src_root, dest_root)
}

fn link_tree_inner(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            link_tree_inner(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            link_or_copy(&entry.path(), &dest_path).with_context(|| {
                format!("link {} -> {}", entry.path().display(), dest_path.display())
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use rusqlite::{params, Connection};

    const T: &str = "00000000-0000-0000-0000-00000000d001";

    fn tiny_template(conn: &Connection) {
        schema::ensure_vault_schema(conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 'demo', 1, 'Alex Demo')",
            params![T],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![T],
        )
        .unwrap();
        let hid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO account_handles (account_id, handle_id) VALUES (?1, ?2)",
            params![T, hid],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES (?1, ?2, 'individual', 'a.jsonl')",
            params![T, hid],
        )
        .unwrap();
        let cid = conn.last_insert_rowid();
        conn.execute(
            r#"INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
            ) VALUES (?1, ?2, 'imessage', 'g1', '2020-01-01T00:00:00Z', 1, 0, 'hello')"#,
            params![cid, T],
        )
        .unwrap();
    }

    struct TestEnv {
        cfg: crate::config::Config,
        _tmp: tempfile::TempDir,
    }

    impl std::ops::Deref for TestEnv {
        type Target = crate::config::Config;
        fn deref(&self) -> &Self::Target {
            &self.cfg
        }
    }

    fn test_config() -> TestEnv {
        let tmp = tempfile::tempdir().expect("temp data_dir");
        let data_dir = tmp.path().to_path_buf();
        TestEnv {
            cfg: crate::config::Config {
                paths: crate::config::PathsConfig {
                    db: data_dir.join("vault.db"),
                    data_dir,
                    assets_dir: "assets".into(),
                    assets_converted_dir: "assets_converted".into(),
                },
                server: None,
            },
            _tmp: tmp,
        }
    }

    #[test]
    fn clone_copies_rows_and_leaves_template() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        tiny_template(&conn);
        let cfg = test_config(); // temp data_dir
        let guest = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        assert_ne!(guest, T);
        let t_msgs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account_id = ?1",
                params![T],
                |r| r.get(0),
            )
            .unwrap();
        let g_msgs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account_id = ?1",
                params![guest],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t_msgs, 1);
        assert_eq!(g_msgs, 1);
        let emails: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM account_emails WHERE account_id = ?1",
                params![guest],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(emails, 0);
        let status: String = conn
            .query_row(
                "SELECT guest_status FROM accounts WHERE id = ?1",
                params![guest],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "ready");
    }

    #[test]
    fn second_clone_does_not_collide() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        tiny_template(&conn);
        let cfg = test_config();
        let a = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        let b = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn clone_hard_links_or_copies_asset_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        tiny_template(&conn);
        let cfg = test_config();
        let src = cfg
            .paths
            .assets_dir_for_account(T, "imessage")
            .join("photo.jpg");
        std::fs::create_dir_all(src.parent().expect("parent")).unwrap();
        std::fs::write(&src, b"asset-bytes").unwrap();
        let guest = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        let dest = cfg
            .paths
            .assets_dir_for_account(&guest, "imessage")
            .join("photo.jpg");
        assert!(dest.is_file(), "guest asset missing at {}", dest.display());
        let src_bytes = std::fs::read(&src).unwrap();
        let dest_bytes = std::fs::read(&dest).unwrap();
        assert_eq!(src_bytes, dest_bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let src_meta = std::fs::metadata(&src).unwrap();
            let dest_meta = std::fs::metadata(&dest).unwrap();
            if src_meta.ino() != dest_meta.ino() || src_meta.dev() != dest_meta.dev() {
                // Hard-link unavailable; byte equality already asserted.
            }
        }
    }
}
