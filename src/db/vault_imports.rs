//! Per-account vault import session records (one row per vault-push / CLI import run).

use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VaultImportRow {
    pub id: i64,
    pub account_id: String,
    pub source: String,
    pub tool: Option<String>,
    pub mode: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub message_count: i64,
    pub attachment_count: i64,
    pub bytes_uploaded: i64,
}

#[derive(Debug, Clone)]
pub struct CompleteImportArgs {
    pub ok: bool,
    pub message_count: Option<i64>,
    pub attachment_count: Option<i64>,
    pub bytes_uploaded: Option<i64>,
}

/// Start a running import session for `account_id`.
pub fn start_import(
    conn: &Connection,
    account_id: &str,
    source: &str,
    mode: &str,
    tool: Option<&str>,
) -> Result<i64> {
    let started_at = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO vault_imports (
            account_id, source, tool, mode, status, started_at,
            message_count, attachment_count, bytes_uploaded
        ) VALUES (?1, ?2, ?3, ?4, 'running', ?5, 0, 0, 0)
        "#,
        params![account_id, source, tool, mode, started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Load an import row owned by `account_id`, or error.
pub fn get_owned_import(
    conn: &Connection,
    account_id: &str,
    import_id: i64,
) -> Result<VaultImportRow> {
    match conn
        .query_row(
            r#"
            SELECT id, account_id, source, tool, mode, status, started_at, finished_at,
                   message_count, attachment_count, bytes_uploaded
            FROM vault_imports
            WHERE id = ?1 AND account_id = ?2
            "#,
            params![import_id, account_id],
            |row| {
                Ok(VaultImportRow {
                    id: row.get(0)?,
                    account_id: row.get(1)?,
                    source: row.get(2)?,
                    tool: row.get(3)?,
                    mode: row.get(4)?,
                    status: row.get(5)?,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                    message_count: row.get(8)?,
                    attachment_count: row.get(9)?,
                    bytes_uploaded: row.get(10)?,
                })
            },
        )
        .optional()?
    {
        Some(row) => Ok(row),
        None => bail!("import {import_id} not found for this account"),
    }
}

/// Finish an import: prefer client counts, else derive from linked messages.
pub fn complete_import(
    conn: &Connection,
    account_id: &str,
    import_id: i64,
    args: &CompleteImportArgs,
) -> Result<VaultImportRow> {
    let existing = get_owned_import(conn, account_id, import_id)?;
    let finished_at = Utc::now().to_rfc3339();
    let status = if args.ok { "completed" } else { "failed" };

    let message_count = if let Some(n) = args.message_count {
        n
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE import_id = ?1 AND account_id = ?2",
            params![import_id, account_id],
            |r| r.get(0),
        )?
    };
    let attachment_count = if let Some(n) = args.attachment_count {
        n
    } else {
        conn.query_row(
            r#"
            SELECT COUNT(*) FROM attachments a
            JOIN messages m ON m.id = a.message_id
            WHERE m.import_id = ?1 AND m.account_id = ?2
            "#,
            params![import_id, account_id],
            |r| r.get(0),
        )?
    };
    let bytes_uploaded = args.bytes_uploaded.unwrap_or(existing.bytes_uploaded);

    conn.execute(
        r#"
        UPDATE vault_imports
        SET status = ?1,
            finished_at = ?2,
            message_count = ?3,
            attachment_count = ?4,
            bytes_uploaded = ?5
        WHERE id = ?6 AND account_id = ?7
        "#,
        params![
            status,
            finished_at,
            message_count,
            attachment_count,
            bytes_uploaded,
            import_id,
            account_id
        ],
    )?;

    get_owned_import(conn, account_id, import_id)
}

/// List imports for an account, newest first.
#[allow(dead_code)] // used by unit tests; storage UI queries SQLite from Next.js
pub fn list_imports_for_account(
    conn: &Connection,
    account_id: &str,
    limit: i64,
) -> Result<Vec<VaultImportRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, account_id, source, tool, mode, status, started_at, finished_at,
               message_count, attachment_count, bytes_uploaded
        FROM vault_imports
        WHERE account_id = ?1
        ORDER BY started_at DESC, id DESC
        LIMIT ?2
        "#,
    )?;
    let rows = stmt
        .query_map(params![account_id, limit], |row| {
            Ok(VaultImportRow {
                id: row.get(0)?,
                account_id: row.get(1)?,
                source: row.get(2)?,
                tool: row.get(3)?,
                mode: row.get(4)?,
                status: row.get(5)?,
                started_at: row.get(6)?,
                finished_at: row.get(7)?,
                message_count: row.get(8)?,
                attachment_count: row.get(9)?,
                bytes_uploaded: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Total attachment bytes for an account (original size_bytes).
#[allow(dead_code)] // used by unit tests; storage UI queries SQLite from Next.js
pub fn account_attachment_bytes(conn: &Connection, account_id: &str) -> Result<i64> {
    let n: i64 = conn.query_row(
        r#"
        SELECT COALESCE(SUM(a.size_bytes), 0)
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        WHERE m.account_id = ?1
        "#,
        params![account_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TopAttachment {
    pub id: i64,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub conversation_id: i64,
    pub conversation_title: Option<String>,
    pub chat_identifier: String,
}

/// Largest attachments for an account.
#[allow(dead_code)] // used by unit tests; storage UI queries SQLite from Next.js
pub fn top_attachments_by_size(
    conn: &Connection,
    account_id: &str,
    limit: i64,
) -> Result<Vec<TopAttachment>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT a.id,
               a.original_name,
               a.mime_type,
               COALESCE(a.size_bytes, 0),
               c.id,
               c.group_title,
               c.chat_identifier
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        JOIN conversations c ON c.id = m.conversation_id
        WHERE m.account_id = ?1
          AND COALESCE(a.size_bytes, 0) > 0
        ORDER BY a.size_bytes DESC, a.id DESC
        LIMIT ?2
        "#,
    )?;
    let rows = stmt
        .query_map(params![account_id, limit], |row| {
            Ok(TopAttachment {
                id: row.get(0)?,
                original_name: row.get(1)?,
                mime_type: row.get(2)?,
                size_bytes: row.get(3)?,
                conversation_id: row.get(4)?,
                conversation_title: row.get(5)?,
                chat_identifier: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
