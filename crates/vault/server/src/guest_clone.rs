//! Clone a template vault account into a new guest (SQL rows + copied files).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sqlx::any::AnyRow;
use sqlx::{AnyConnection, Connection, Row};

use crate::config::Config;
use crate::db::{account_profile, session_tokens};

/// Copy the template account's rows and files into a new guest account.
///
/// Integer primary keys are remapped. `account_emails`, session tokens, and API
/// tokens are not copied. Attachment files are copied after the SQL transaction
/// commits so a later write on the guest path cannot change the template. If
/// file copy fails, the guest account is deleted so a half-created copy is not
/// left behind.
///
/// # Errors
///
/// Returns an error when the template is missing, a SQL statement fails, or
/// files cannot be copied.
pub async fn clone_template_to_guest(
    conn: &mut AnyConnection,
    cfg: &Config,
    template_account_id: &str,
) -> Result<String> {
    let guest_id = {
        let mut tx = conn.begin().await?;
        let guest_id = clone_sql(&mut *tx, template_account_id).await?;
        tx.commit().await?;
        guest_id
    };
    finish_file_clone(conn, cfg, template_account_id, &guest_id).await?;
    Ok(guest_id)
}

/// Clone the template and mark that guest assigned in the same SQL transaction.
///
/// Used by on-demand Try it so another request cannot take the new `ready` row
/// before this request issues a session. A plain transaction is enough: the
/// vault operation lock already serializes clone work against other writers.
///
/// # Errors
///
/// Returns an error when the template is missing, a SQL statement fails, or
/// files cannot be copied.
pub async fn clone_and_assign_guest(
    conn: &mut AnyConnection,
    cfg: &Config,
    template_account_id: &str,
    session_secs: u64,
) -> Result<(String, String, String)> {
    let (guest_id, username, token) = {
        let mut tx = conn.begin().await?;
        let guest_id = clone_sql(&mut *tx, template_account_id).await?;
        account_profile::set_guest_status(&mut *tx, &guest_id, "assigned").await?;
        let username = account_profile::username_for_account(&mut *tx, &guest_id)
            .await?
            .context("guest username missing after clone")?;
        let token = session_tokens::insert_account_session_token_with_ttl(
            &mut *tx,
            &guest_id,
            session_secs,
        )
        .await?;
        tx.commit().await?;
        (guest_id, username, token)
    };
    finish_file_clone(conn, cfg, template_account_id, &guest_id).await?;
    Ok((guest_id, username, token))
}

async fn finish_file_clone(
    conn: &mut AnyConnection,
    cfg: &Config,
    template_account_id: &str,
    guest_id: &str,
) -> Result<()> {
    let src_root = cfg.paths.data_dir.join(template_account_id);
    let dest_root = cfg.paths.data_dir.join(guest_id);
    if let Err(err) = copy_tree(&src_root, &dest_root) {
        let cleanup = account_profile::delete_account(conn, guest_id).await;
        let _ = fs::remove_dir_all(&dest_root);
        if let Err(cleanup_err) = cleanup {
            return Err(err.context(format!(
                "file clone failed; also failed to delete guest {guest_id}: {cleanup_err}"
            )));
        }
        return Err(err);
    }
    Ok(())
}

async fn clone_sql(tx: &mut AnyConnection, template: &str) -> Result<String> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM accounts WHERE id = $1")
        .bind(template)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        bail!("template account {template} not found");
    }

    let guest_id = uuid::Uuid::new_v4().to_string();
    let hex = guest_id.replace('-', "");
    let username = format!("guest-{}", &hex[..8]);
    let preferred = account_profile::load_preferred_name(tx, template).await?;
    account_profile::insert_guest_account(tx, &guest_id, &username, preferred.as_deref()).await?;

    let handle_map = copy_handles(tx, template, &guest_id).await?;
    let contact_map = copy_contacts(tx, template, &guest_id).await?;
    copy_contact_handles(tx, template, &guest_id, &handle_map, &contact_map).await?;
    copy_account_handles(tx, template, &guest_id, &handle_map).await?;
    let group_map = copy_contact_groups(tx, template, &guest_id).await?;
    copy_contact_group_members(tx, template, &contact_map, &group_map).await?;
    copy_trashed_handles(tx, template, &guest_id, &handle_map).await?;
    copy_trashed_contacts(tx, template, &guest_id, &contact_map).await?;

    let import_map = copy_vault_imports(tx, template, &guest_id).await?;
    copy_vault_import_issues(tx, &import_map).await?;

    let conversation_map = copy_conversations(tx, template, &guest_id, &handle_map).await?;
    let tag_map = copy_conversation_tags(tx, template, &guest_id).await?;
    copy_conversation_tag_members(tx, template, &conversation_map, &tag_map).await?;
    copy_trashed_conversations(tx, template, &guest_id, &conversation_map).await?;
    copy_participants(tx, template, &conversation_map, &handle_map, &contact_map).await?;

    let message_map = copy_messages(
        tx,
        template,
        &guest_id,
        &conversation_map,
        &handle_map,
        &import_map,
    )
    .await?;
    copy_attachments(tx, template, &message_map).await?;
    copy_tapbacks(tx, template, &message_map, &handle_map).await?;

    let staging_conv_map = copy_staging_conversations(tx, template, &guest_id, &handle_map).await?;
    copy_staging_participants(tx, template, &staging_conv_map, &handle_map, &contact_map).await?;
    let staging_msg_map = copy_staging_messages(
        tx,
        template,
        &guest_id,
        &staging_conv_map,
        &handle_map,
        &import_map,
    )
    .await?;
    copy_staging_attachments(tx, template, &staging_msg_map).await?;
    copy_staging_tapbacks(tx, template, &staging_msg_map, &handle_map).await?;

    copy_account_prefs(tx, template, &guest_id).await?;
    Ok(guest_id)
}

async fn collect_rows<T>(
    tx: &mut AnyConnection,
    sql: &str,
    account_id: &str,
    f: impl Fn(&AnyRow) -> Result<T>,
) -> Result<Vec<T>> {
    let rows = sqlx::query(sql)
        .bind(account_id)
        .fetch_all(&mut *tx)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(f(row)?);
    }
    Ok(out)
}

fn mapped(map: &HashMap<i64, i64>, old: i64) -> Option<i64> {
    map.get(&old).copied()
}

fn mapped_opt(map: &HashMap<i64, i64>, old: Option<i64>) -> Option<i64> {
    old.and_then(|id| map.get(&id).copied())
}

async fn copy_handles(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, raw, normalized, normalized_note, handle_type, service
        FROM handles WHERE account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<String, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<String, _>(4)?,
                row.try_get::<String, _>(5)?,
            ))
        },
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, raw, normalized, note, handle_type, service) in rows {
        let new_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO handles (
                account_id, raw, normalized, normalized_note, handle_type, service
            ) VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(guest)
        .bind(raw)
        .bind(normalized)
        .bind(note)
        .bind(handle_type)
        .bind(service)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_contacts(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        "SELECT id, preferred_name, last_modified FROM contacts WHERE account_id = $1",
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<String, _>(1)?,
                row.try_get::<String, _>(2)?,
            ))
        },
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, preferred_name, last_modified) in rows {
        let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name, last_modified) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(guest)
        .bind(preferred_name)
        .bind(last_modified)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_contact_handles(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
    contacts: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT handle_id, contact_id, name_alias FROM contact_handles WHERE account_id = $1",
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<Option<String>, _>(2)?,
            ))
        },
    )
    .await?;
    for (handle_id, contact_id, name_alias) in rows {
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        let Some(new_contact) = mapped(contacts, contact_id) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO contact_handles (account_id, handle_id, contact_id, name_alias)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(guest)
        .bind(new_handle)
        .bind(new_contact)
        .bind(name_alias)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_account_handles(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT handle_id FROM account_handles WHERE account_id = $1",
        template,
        |row| Ok(row.try_get::<i64, _>(0)?),
    )
    .await?;
    for handle_id in rows {
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        sqlx::query("INSERT INTO account_handles (account_id, handle_id) VALUES ($1, $2)")
            .bind(guest)
            .bind(new_handle)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

async fn copy_contact_groups(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        "SELECT id, name FROM contact_groups WHERE account_id = $1",
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<String, _>(1)?)),
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, name) in rows {
        let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO contact_groups (account_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(guest)
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_contact_group_members(
    tx: &mut AnyConnection,
    template: &str,
    contacts: &HashMap<i64, i64>,
    groups: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT cgm.contact_id, cgm.group_id
        FROM contact_group_members cgm
        JOIN contacts c ON c.id = cgm.contact_id
        WHERE c.account_id = $1
        "#,
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<i64, _>(1)?)),
    )
    .await?;
    for (contact_id, group_id) in rows {
        let Some(new_contact) = mapped(contacts, contact_id) else {
            continue;
        };
        let Some(new_group) = mapped(groups, group_id) else {
            continue;
        };
        sqlx::query("INSERT INTO contact_group_members (contact_id, group_id) VALUES ($1, $2)")
            .bind(new_contact)
            .bind(new_group)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

async fn copy_conversation_tags(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        "SELECT id, name FROM conversation_tags WHERE account_id = $1",
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<String, _>(1)?)),
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, name) in rows {
        let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversation_tags (account_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(guest)
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_conversation_tag_members(
    tx: &mut AnyConnection,
    template: &str,
    conversations: &HashMap<i64, i64>,
    tags: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT ctm.conversation_id, ctm.tag_id
        FROM conversation_tag_members ctm
        JOIN conversations c ON c.id = ctm.conversation_id
        WHERE c.account_id = $1
        "#,
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<i64, _>(1)?)),
    )
    .await?;
    for (conversation_id, tag_id) in rows {
        let Some(new_conversation) = mapped(conversations, conversation_id) else {
            continue;
        };
        let Some(new_tag) = mapped(tags, tag_id) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO conversation_tag_members (conversation_id, tag_id) VALUES ($1, $2)",
        )
        .bind(new_conversation)
        .bind(new_tag)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_trashed_handles(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT handle_id, trashed_at FROM trashed_handles WHERE account_id = $1",
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<String, _>(1)?)),
    )
    .await?;
    for (handle_id, trashed_at) in rows {
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO trashed_handles (account_id, handle_id, trashed_at) VALUES ($1, $2, $3)",
        )
        .bind(guest)
        .bind(new_handle)
        .bind(trashed_at)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_trashed_contacts(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    contacts: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT contact_id, trashed_at FROM trashed_contacts WHERE account_id = $1",
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<String, _>(1)?)),
    )
    .await?;
    for (contact_id, trashed_at) in rows {
        let Some(new_contact) = mapped(contacts, contact_id) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO trashed_contacts (account_id, contact_id, trashed_at) VALUES ($1, $2, $3)",
        )
        .bind(guest)
        .bind(new_contact)
        .bind(trashed_at)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_vault_imports(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, source, tool, mode, status, started_at, finished_at,
               message_count, attachment_count, bytes_uploaded,
               duration_ms, parse_ms, convert_ms, upload_ms, summary_json
        FROM vault_imports WHERE account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<String, _>(1)?,
                row.try_get::<Option<String>, _>(2)?,
                row.try_get::<String, _>(3)?,
                row.try_get::<String, _>(4)?,
                row.try_get::<String, _>(5)?,
                row.try_get::<Option<String>, _>(6)?,
                row.try_get::<i64, _>(7)?,
                row.try_get::<i64, _>(8)?,
                row.try_get::<i64, _>(9)?,
                row.try_get::<Option<i64>, _>(10)?,
                row.try_get::<Option<i64>, _>(11)?,
                row.try_get::<Option<i64>, _>(12)?,
                row.try_get::<Option<i64>, _>(13)?,
                row.try_get::<Option<String>, _>(14)?,
            ))
        },
    )
    .await?;
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
        let new_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO vault_imports (
                account_id, source, tool, mode, status, started_at, finished_at,
                message_count, attachment_count, bytes_uploaded,
                duration_ms, parse_ms, convert_ms, upload_ms, summary_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id
            "#,
        )
        .bind(guest)
        .bind(source)
        .bind(tool)
        .bind(mode)
        .bind(status)
        .bind(started_at)
        .bind(finished_at)
        .bind(message_count)
        .bind(attachment_count)
        .bind(bytes_uploaded)
        .bind(duration_ms)
        .bind(parse_ms)
        .bind(convert_ms)
        .bind(upload_ms)
        .bind(summary_json)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_vault_import_issues(
    tx: &mut AnyConnection,
    imports: &HashMap<i64, i64>,
) -> Result<()> {
    if imports.is_empty() {
        return Ok(());
    }
    let mut pending = Vec::new();
    for &old_import in imports.keys() {
        let rows = sqlx::query(
            r#"
            SELECT import_id, kind, step, item, reason, created_at
            FROM vault_import_issues
            WHERE import_id = $1
            "#,
        )
        .bind(old_import)
        .fetch_all(&mut *tx)
        .await?;
        for row in &rows {
            pending.push((
                row.try_get::<i64, _>(0)?,
                row.try_get::<String, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<String, _>(3)?,
                row.try_get::<String, _>(4)?,
                row.try_get::<String, _>(5)?,
            ));
        }
    }
    for (old_import, kind, step, item, reason, created_at) in pending {
        let Some(new_import) = mapped(imports, old_import) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO vault_import_issues (import_id, kind, step, item, reason, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(new_import)
        .bind(kind)
        .bind(step)
        .bind(item)
        .bind(reason)
        .bind(created_at)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_conversations(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, chat_handle_id, conversation_type, group_title, exported_at, source_file
        FROM conversations WHERE account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<Option<String>, _>(4)?,
                row.try_get::<String, _>(5)?,
            ))
        },
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, chat_handle_id, conversation_type, group_title, exported_at, source_file) in rows {
        let Some(new_handle) = mapped(handles, chat_handle_id) else {
            continue;
        };
        let new_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
            ) VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(guest)
        .bind(new_handle)
        .bind(conversation_type)
        .bind(group_title)
        .bind(exported_at)
        .bind(source_file)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_trashed_conversations(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    conversations: &HashMap<i64, i64>,
) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT conversation_id, trashed_at FROM trashed_conversations WHERE account_id = $1",
        template,
        |row| Ok((row.try_get::<i64, _>(0)?, row.try_get::<String, _>(1)?)),
    )
    .await?;
    for (conversation_id, trashed_at) in rows {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO trashed_conversations (account_id, conversation_id, trashed_at)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(guest)
        .bind(new_conv)
        .bind(trashed_at)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_participants(
    tx: &mut AnyConnection,
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
        WHERE c.account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<Option<i64>, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
            ))
        },
    )
    .await?;
    for (conversation_id, handle_id, contact_id, name_alias) in rows {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        let new_contact = mapped_opt(contacts, contact_id);
        sqlx::query(
            r#"
            INSERT INTO participants (conversation_id, handle_id, contact_id, name_alias)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(new_conv)
        .bind(new_handle)
        .bind(new_contact)
        .bind(name_alias)
        .execute(&mut *tx)
        .await?;
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

async fn copy_messages(
    tx: &mut AnyConnection,
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
        FROM messages WHERE account_id = $1
        "#,
        template,
        |row| {
            Ok(MessageRow {
                id: row.try_get(0)?,
                conversation_id: row.try_get(1)?,
                source: row.try_get(2)?,
                guid: row.try_get(3)?,
                timestamp: row.try_get(4)?,
                timestamp_utc: row.try_get(5)?,
                is_from_me: row.try_get(6)?,
                sender_handle_id: row.try_get(7)?,
                service: row.try_get(8)?,
                subject: row.try_get(9)?,
                body: row.try_get(10)?,
                is_announcement: row.try_get(11)?,
                is_reply: row.try_get(12)?,
                thread_originator_guid: row.try_get(13)?,
                thread_originator_part: row.try_get(14)?,
                num_replies: row.try_get(15)?,
                sort_order: row.try_get(16)?,
                content_key: row.try_get(17)?,
                duplicate_of: row.try_get(18)?,
                import_id: row.try_get(19)?,
            })
        },
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    let mut pending_fks = Vec::new();
    for row in rows {
        let Some(new_conv) = mapped(conversations, row.conversation_id) else {
            continue;
        };
        let sender = mapped_opt(handles, row.sender_handle_id);
        let new_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc,
                is_from_me, sender_handle_id, service, subject, body,
                is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, content_key,
                duplicate_of, import_id
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18, NULL, NULL
            )
            RETURNING id
            "#,
        )
        .bind(new_conv)
        .bind(guest)
        .bind(row.source)
        .bind(row.guid)
        .bind(row.timestamp)
        .bind(row.timestamp_utc)
        .bind(row.is_from_me)
        .bind(sender)
        .bind(row.service)
        .bind(row.subject)
        .bind(row.body)
        .bind(row.is_announcement)
        .bind(row.is_reply)
        .bind(row.thread_originator_guid)
        .bind(row.thread_originator_part)
        .bind(row.num_replies)
        .bind(row.sort_order)
        .bind(row.content_key)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(row.id, new_id);
        pending_fks.push((new_id, row.duplicate_of, row.import_id));
    }
    for (new_id, old_dup, old_import) in pending_fks {
        let dup = mapped_opt(&map, old_dup);
        let import = mapped_opt(imports, old_import);
        if dup.is_none() && import.is_none() {
            continue;
        }
        sqlx::query("UPDATE messages SET duplicate_of = $1, import_id = $2 WHERE id = $3")
            .bind(dup)
            .bind(import)
            .bind(new_id)
            .execute(&mut *tx)
            .await?;
    }
    Ok(map)
}

async fn copy_attachments(
    tx: &mut AnyConnection,
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
        WHERE m.account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<Option<String>, _>(1)?,
                row.try_get::<Option<String>, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<i64, _>(4)?,
                row.try_get::<Option<String>, _>(5)?,
                row.try_get::<Option<String>, _>(6)?,
                row.try_get::<Option<String>, _>(7)?,
                row.try_get::<Option<i64>, _>(8)?,
                row.try_get::<Option<String>, _>(9)?,
                row.try_get::<Option<String>, _>(10)?,
                row.try_get::<Option<String>, _>(11)?,
                row.try_get::<Option<String>, _>(12)?,
            ))
        },
    )
    .await?;
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
        sqlx::query(
            r#"
            INSERT INTO attachments (
                message_id, path, original_name, mime_type, is_sticker, transcription,
                sha256, assets_path, size_bytes, missing_reason,
                derived_sha256, derived_assets_path, derived_mime_type
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(new_msg)
        .bind(path)
        .bind(original_name)
        .bind(mime_type)
        .bind(is_sticker)
        .bind(transcription)
        .bind(sha256)
        .bind(assets_path)
        .bind(size_bytes)
        .bind(missing_reason)
        .bind(derived_sha256)
        .bind(derived_assets_path)
        .bind(derived_mime_type)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_tapbacks(
    tx: &mut AnyConnection,
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
        WHERE m.account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<i64, _>(4)?,
                row.try_get::<Option<i64>, _>(5)?,
            ))
        },
    )
    .await?;
    for (message_id, part_index, kind, emoji, is_from_me, sender_handle_id) in rows {
        let Some(new_msg) = mapped(messages, message_id) else {
            continue;
        };
        let sender = mapped_opt(handles, sender_handle_id);
        sqlx::query(
            r#"
            INSERT INTO tapbacks (
                message_id, part_index, kind, emoji, is_from_me, sender_handle_id
            ) VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(new_msg)
        .bind(part_index)
        .bind(kind)
        .bind(emoji)
        .bind(is_from_me)
        .bind(sender)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_staging_conversations(
    tx: &mut AnyConnection,
    template: &str,
    guest: &str,
    handles: &HashMap<i64, i64>,
) -> Result<HashMap<i64, i64>> {
    let rows = collect_rows(
        tx,
        r#"
        SELECT id, chat_handle_id, conversation_type, group_title, exported_at, source_file
        FROM staging_conversations WHERE account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<Option<String>, _>(4)?,
                row.try_get::<String, _>(5)?,
            ))
        },
    )
    .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (old_id, chat_handle_id, conversation_type, group_title, exported_at, source_file) in rows {
        let Some(new_handle) = mapped(handles, chat_handle_id) else {
            continue;
        };
        let new_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO staging_conversations (
                account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
            ) VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(guest)
        .bind(new_handle)
        .bind(conversation_type)
        .bind(group_title)
        .bind(exported_at)
        .bind(source_file)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_staging_participants(
    tx: &mut AnyConnection,
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
        WHERE c.account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<Option<i64>, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
            ))
        },
    )
    .await?;
    for (conversation_id, handle_id, contact_id, name_alias) in rows {
        let Some(new_conv) = mapped(conversations, conversation_id) else {
            continue;
        };
        let Some(new_handle) = mapped(handles, handle_id) else {
            continue;
        };
        let new_contact = mapped_opt(contacts, contact_id);
        sqlx::query(
            r#"
            INSERT INTO staging_participants (conversation_id, handle_id, contact_id, name_alias)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(new_conv)
        .bind(new_handle)
        .bind(new_contact)
        .bind(name_alias)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_staging_messages(
    tx: &mut AnyConnection,
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
        FROM staging_messages WHERE account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<String, _>(4)?,
                row.try_get::<Option<String>, _>(5)?,
                row.try_get::<i64, _>(6)?,
                row.try_get::<Option<i64>, _>(7)?,
                row.try_get::<Option<String>, _>(8)?,
                row.try_get::<Option<String>, _>(9)?,
                row.try_get::<Option<String>, _>(10)?,
                row.try_get::<i64, _>(11)?,
                row.try_get::<i64, _>(12)?,
                row.try_get::<Option<String>, _>(13)?,
                row.try_get::<Option<i64>, _>(14)?,
                row.try_get::<i64, _>(15)?,
                row.try_get::<i64, _>(16)?,
                row.try_get::<Option<i64>, _>(17)?,
            ))
        },
    )
    .await?;
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
        let new_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc,
                is_from_me, sender_handle_id, service, subject, body,
                is_announcement, is_reply, thread_originator_guid,
                thread_originator_part, num_replies, sort_order, import_id
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18
            )
            RETURNING id
            "#,
        )
        .bind(new_conv)
        .bind(guest)
        .bind(source)
        .bind(guid)
        .bind(timestamp)
        .bind(timestamp_utc)
        .bind(is_from_me)
        .bind(sender)
        .bind(service)
        .bind(subject)
        .bind(body)
        .bind(is_announcement)
        .bind(is_reply)
        .bind(thread_originator_guid)
        .bind(thread_originator_part)
        .bind(num_replies)
        .bind(sort_order)
        .bind(import)
        .fetch_one(&mut *tx)
        .await?;
        map.insert(old_id, new_id);
    }
    Ok(map)
}

async fn copy_staging_attachments(
    tx: &mut AnyConnection,
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
        WHERE m.account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<Option<String>, _>(1)?,
                row.try_get::<Option<String>, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<i64, _>(4)?,
                row.try_get::<Option<String>, _>(5)?,
                row.try_get::<Option<String>, _>(6)?,
                row.try_get::<Option<String>, _>(7)?,
                row.try_get::<Option<i64>, _>(8)?,
                row.try_get::<Option<String>, _>(9)?,
                row.try_get::<Option<String>, _>(10)?,
                row.try_get::<Option<String>, _>(11)?,
                row.try_get::<Option<String>, _>(12)?,
            ))
        },
    )
    .await?;
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
        sqlx::query(
            r#"
            INSERT INTO staging_attachments (
                message_id, path, original_name, mime_type, is_sticker, transcription,
                sha256, assets_path, size_bytes, missing_reason,
                derived_sha256, derived_assets_path, derived_mime_type
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(new_msg)
        .bind(path)
        .bind(original_name)
        .bind(mime_type)
        .bind(is_sticker)
        .bind(transcription)
        .bind(sha256)
        .bind(assets_path)
        .bind(size_bytes)
        .bind(missing_reason)
        .bind(derived_sha256)
        .bind(derived_assets_path)
        .bind(derived_mime_type)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_staging_tapbacks(
    tx: &mut AnyConnection,
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
        WHERE m.account_id = $1
        "#,
        template,
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                row.try_get::<i64, _>(1)?,
                row.try_get::<String, _>(2)?,
                row.try_get::<Option<String>, _>(3)?,
                row.try_get::<i64, _>(4)?,
                row.try_get::<Option<i64>, _>(5)?,
            ))
        },
    )
    .await?;
    for (message_id, part_index, kind, emoji, is_from_me, sender_handle_id) in rows {
        let Some(new_msg) = mapped(messages, message_id) else {
            continue;
        };
        let sender = mapped_opt(handles, sender_handle_id);
        sqlx::query(
            r#"
            INSERT INTO staging_tapbacks (
                message_id, part_index, kind, emoji, is_from_me, sender_handle_id
            ) VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(new_msg)
        .bind(part_index)
        .bind(kind)
        .bind(emoji)
        .bind(is_from_me)
        .bind(sender)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn copy_account_prefs(tx: &mut AnyConnection, template: &str, guest: &str) -> Result<()> {
    let rows = collect_rows(
        tx,
        "SELECT key, value FROM account_prefs WHERE account_id = $1",
        template,
        |row| Ok((row.try_get::<String, _>(0)?, row.try_get::<String, _>(1)?)),
    )
    .await?;
    for (key, value) in rows {
        sqlx::query("INSERT INTO account_prefs (account_id, key, value) VALUES ($1, $2, $3)")
            .bind(guest)
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

fn copy_file(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest)?;
    Ok(())
}

fn copy_tree(src_root: &Path, dest_root: &Path) -> Result<()> {
    if !src_root.exists() {
        return Ok(());
    }
    copy_tree_inner(src_root, dest_root)
}

fn copy_tree_inner(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree_inner(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            copy_file(&entry.path(), &dest_path).with_context(|| {
                format!("copy {} -> {}", entry.path().display(), dest_path.display())
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::engine;
    use crate::db::schema;
    use sqlx::AnyPool;

    const T: &str = "00000000-0000-0000-0000-00000000d001";

    async fn tiny_template(pool: &AnyPool) {
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES ($1, 'demo', 1, 'Alex Demo')",
        )
        .bind(T)
        .execute(&mut *conn)
        .await
        .unwrap();
        let hid: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')
             RETURNING id",
        )
        .bind(T)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("INSERT INTO account_handles (account_id, handle_id) VALUES ($1, $2)")
            .bind(T)
            .bind(hid)
            .execute(&mut *conn)
            .await
            .unwrap();
        let cid: i64 = sqlx::query_scalar(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES ($1, $2, 'individual', 'a.jsonl')
             RETURNING id",
        )
        .bind(T)
        .bind(hid)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
            ) VALUES ($1, $2, 'imessage', 'g1', '2020-01-01T00:00:00Z', 1, 0, 'hello')"#,
        )
        .bind(cid)
        .bind(T)
        .execute(&mut *conn)
        .await
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
                database: crate::config::DatabaseConfig::default(),
            },
            _tmp: tmp,
        }
    }

    #[tokio::test]
    async fn clone_copies_rows_and_leaves_template() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let cfg = test_config(); // temp data_dir
        let mut conn = pool.acquire().await.unwrap();
        let guest = clone_template_to_guest(&mut conn, &cfg, T).await.unwrap();
        assert_ne!(guest, T);
        let t_msgs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = $1")
            .bind(T)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let g_msgs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = $1")
            .bind(&guest)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(t_msgs, 1);
        assert_eq!(g_msgs, 1);
        let emails: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM account_emails WHERE account_id = $1")
                .bind(&guest)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(emails, 0);
        let status: String = sqlx::query_scalar("SELECT guest_status FROM accounts WHERE id = $1")
            .bind(&guest)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(status, "ready");
    }

    #[tokio::test]
    async fn second_clone_does_not_collide() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let cfg = test_config();
        let mut conn = pool.acquire().await.unwrap();
        let a = clone_template_to_guest(&mut conn, &cfg, T).await.unwrap();
        let b = clone_template_to_guest(&mut conn, &cfg, T).await.unwrap();
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn clone_copies_asset_files_without_sharing_inode() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let cfg = test_config();
        let src = cfg
            .paths
            .assets_dir_for_account(T, "imessage")
            .join("photo.jpg");
        std::fs::create_dir_all(src.parent().expect("parent")).unwrap();
        std::fs::write(&src, b"asset-bytes").unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let guest = clone_template_to_guest(&mut conn, &cfg, T).await.unwrap();
        let dest = cfg
            .paths
            .assets_dir_for_account(&guest, "imessage")
            .join("photo.jpg");
        assert!(dest.is_file(), "guest asset missing at {}", dest.display());
        assert_eq!(std::fs::read(&src).unwrap(), b"asset-bytes");
        assert_eq!(std::fs::read(&dest).unwrap(), b"asset-bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let src_meta = std::fs::metadata(&src).unwrap();
            let dest_meta = std::fs::metadata(&dest).unwrap();
            assert!(
                src_meta.ino() != dest_meta.ino() || src_meta.dev() != dest_meta.dev(),
                "guest asset still shares an inode with the template"
            );
        }
        std::fs::write(&dest, b"changed").unwrap();
        assert_eq!(
            std::fs::read(&src).unwrap(),
            b"asset-bytes",
            "writing the guest file must not change the template"
        );
    }

    #[tokio::test]
    async fn clone_and_assign_leaves_no_ready_row_to_steal() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let cfg = test_config();
        let mut conn = pool.acquire().await.unwrap();
        let (guest_id, username, token) = clone_and_assign_guest(&mut conn, &cfg, T, 120)
            .await
            .unwrap();
        assert!(username.starts_with("guest-"));
        assert!(token.starts_with("mv-user-"));
        assert_eq!(
            account_profile::guest_status(&mut conn, &guest_id)
                .await
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        let ready: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE guest_status = 'ready'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(
            ready, 0,
            "on-demand clone left a ready row another Try it could take"
        );
    }

    #[tokio::test]
    async fn clone_skips_staging_rows_with_unmapped_handles() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let template_handle: i64 =
            sqlx::query_scalar("SELECT id FROM handles WHERE account_id = $1")
                .bind(T)
                .fetch_one(&mut *conn)
                .await
                .unwrap();

        const DANGLING_CONV_HANDLE: i64 = 9_000_001;
        const DANGLING_PART_HANDLE: i64 = 9_000_002;

        sqlx::query(
            "INSERT INTO staging_conversations (
                account_id, chat_handle_id, conversation_type, source_file
             ) VALUES ($1, $2, 'individual', 'orphan.jsonl')",
        )
        .bind(T)
        .bind(DANGLING_CONV_HANDLE)
        .execute(&mut *conn)
        .await
        .unwrap();
        let ok_staging_cid: i64 = sqlx::query_scalar(
            "INSERT INTO staging_conversations (
                account_id, chat_handle_id, conversation_type, source_file
             ) VALUES ($1, $2, 'individual', 'ok.jsonl')
             RETURNING id",
        )
        .bind(T)
        .bind(template_handle)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO staging_participants (conversation_id, handle_id, name_alias)
             VALUES ($1, $2, 'orphan-part')",
        )
        .bind(ok_staging_cid)
        .bind(DANGLING_PART_HANDLE)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO staging_participants (conversation_id, handle_id, name_alias)
             VALUES ($1, $2, 'mapped-part')",
        )
        .bind(ok_staging_cid)
        .bind(template_handle)
        .execute(&mut *conn)
        .await
        .unwrap();

        let cfg = test_config();
        let guest = clone_template_to_guest(&mut conn, &cfg, T).await.unwrap();

        let guest_handle: i64 = sqlx::query_scalar("SELECT id FROM handles WHERE account_id = $1")
            .bind(&guest)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_ne!(guest_handle, template_handle);

        let guest_staging_handles: Vec<i64> = sqlx::query_scalar(
            "SELECT chat_handle_id FROM staging_conversations WHERE account_id = $1",
        )
        .bind(&guest)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert!(
            !guest_staging_handles.contains(&DANGLING_CONV_HANDLE),
            "guest staging conversation kept the template handle id {DANGLING_CONV_HANDLE}"
        );
        assert!(
            !guest_staging_handles.contains(&template_handle),
            "guest staging conversation reused the template handle id {template_handle}"
        );
        assert_eq!(guest_staging_handles, vec![guest_handle]);

        let guest_part_handles: Vec<i64> = sqlx::query_scalar(
            "SELECT p.handle_id FROM staging_participants p
             JOIN staging_conversations c ON c.id = p.conversation_id
             WHERE c.account_id = $1",
        )
        .bind(&guest)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert!(
            !guest_part_handles.contains(&DANGLING_PART_HANDLE),
            "guest staging participant kept the template handle id {DANGLING_PART_HANDLE}"
        );
        assert!(
            !guest_part_handles.contains(&template_handle),
            "guest staging participant reused the template handle id {template_handle}"
        );
        assert_eq!(guest_part_handles, vec![guest_handle]);
    }
}
