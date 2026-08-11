//! Per-account vault import session records (one row per vault-push / CLI import run).

use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
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
    pub duration_ms: Option<i64>,
    pub parse_ms: Option<i64>,
    pub convert_ms: Option<i64>,
    pub upload_ms: Option<i64>,
    pub summary_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompleteImportArgs {
    pub ok: bool,
    pub message_count: Option<i64>,
    pub attachment_count: Option<i64>,
    pub bytes_uploaded: Option<i64>,
    pub duration_ms: Option<i64>,
    pub parse_ms: Option<i64>,
    pub convert_ms: Option<i64>,
    pub upload_ms: Option<i64>,
    pub summary_json: Option<String>,
    pub issues: Vec<ImportIssueInput>,
}

struct ImportTransaction<'conn> {
    conn: &'conn Connection,
    committed: bool,
}

impl<'conn> ImportTransaction<'conn> {
    fn begin(conn: &'conn Connection) -> Result<Self> {
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
        Ok(Self {
            conn,
            committed: false,
        })
    }

    fn commit(mut self) -> Result<()> {
        self.conn.execute_batch("COMMIT;")?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for ImportTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.conn.execute_batch("ROLLBACK;");
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportIssueInput {
    pub kind: String,
    pub step: String,
    pub item: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportIssueRow {
    pub id: i64,
    pub import_id: i64,
    pub kind: String,
    pub step: String,
    pub item: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportDetail {
    pub row: VaultImportRow,
    pub issues: Vec<ImportIssueRow>,
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

fn load_import_row(conn: &Connection, account_id: &str, import_id: i64) -> Result<VaultImportRow> {
    match conn
        .query_row(
            r#"
            SELECT id, account_id, source, tool, mode, status, started_at, finished_at,
                   message_count, attachment_count, bytes_uploaded, duration_ms, parse_ms,
                   convert_ms, upload_ms, summary_json
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
                    duration_ms: row.get(11)?,
                    parse_ms: row.get(12)?,
                    convert_ms: row.get(13)?,
                    upload_ms: row.get(14)?,
                    summary_json: row.get(15)?,
                })
            },
        )
        .optional()?
    {
        Some(row) => Ok(row),
        None => bail!("import {import_id} not found for this account"),
    }
}

/// Load an import row owned by `account_id`, or error.
pub fn get_owned_import(
    conn: &Connection,
    account_id: &str,
    import_id: i64,
) -> Result<VaultImportRow> {
    load_import_row(conn, account_id, import_id)
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

    for issue in &args.issues {
        validate_issue_kind(&issue.kind)?;
    }

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

    let tx = ImportTransaction::begin(conn)?;
    conn.execute(
        r#"
        UPDATE vault_imports
        SET status = ?1,
            finished_at = ?2,
            message_count = ?3,
            attachment_count = ?4,
            bytes_uploaded = ?5,
            duration_ms = ?6,
            parse_ms = ?7,
            convert_ms = ?8,
            upload_ms = ?9,
            summary_json = ?10
        WHERE id = ?11 AND account_id = ?12
        "#,
        params![
            status,
            finished_at,
            message_count,
            attachment_count,
            bytes_uploaded,
            args.duration_ms,
            args.parse_ms,
            args.convert_ms,
            args.upload_ms,
            args.summary_json.as_deref(),
            import_id,
            account_id
        ],
    )?;
    insert_issues(conn, import_id, &args.issues)?;
    tx.commit()?;

    get_owned_import(conn, account_id, import_id)
}

fn insert_issues(conn: &Connection, import_id: i64, issues: &[ImportIssueInput]) -> Result<()> {
    for issue in issues {
        validate_issue_kind(&issue.kind)?;
        conn.execute(
            r#"
            INSERT INTO vault_import_issues (
                import_id, kind, step, item, reason, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                import_id,
                &issue.kind,
                &issue.step,
                &issue.item,
                &issue.reason,
                Utc::now().to_rfc3339()
            ],
        )?;
    }
    Ok(())
}

fn validate_issue_kind(kind: &str) -> Result<()> {
    match kind {
        "error" | "skip" => Ok(()),
        other => bail!("invalid import issue kind '{other}'; expected 'error' or 'skip'"),
    }
}

/// Load one import row and its issue list.
pub fn get_import_detail(
    conn: &Connection,
    account_id: &str,
    import_id: i64,
) -> Result<ImportDetail> {
    let row = get_owned_import(conn, account_id, import_id)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT id, import_id, kind, step, item, reason, created_at
        FROM vault_import_issues
        WHERE import_id = ?1
        ORDER BY id ASC
        "#,
    )?;
    let issues = stmt
        .query_map(params![import_id], |row| {
            Ok(ImportIssueRow {
                id: row.get(0)?,
                import_id: row.get(1)?,
                kind: row.get(2)?,
                step: row.get(3)?,
                item: row.get(4)?,
                reason: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ImportDetail { row, issues })
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub id: i64,
    pub source: String,
    pub tool: Option<String>,
    pub mode: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub message_count: i64,
    pub attachment_count: i64,
    pub bytes_uploaded: i64,
    pub duration_ms: Option<i64>,
}

impl From<VaultImportRow> for ImportSummary {
    fn from(r: VaultImportRow) -> Self {
        ImportSummary {
            id: r.id,
            source: r.source,
            tool: r.tool,
            mode: r.mode,
            status: r.status,
            started_at: r.started_at,
            finished_at: r.finished_at,
            message_count: r.message_count,
            attachment_count: r.attachment_count,
            bytes_uploaded: r.bytes_uploaded,
            duration_ms: r.duration_ms,
        }
    }
}

/// List imports for an account, newest first. Returns serializable summaries.
pub fn list_imports(conn: &Connection, account_id: &str) -> Result<Vec<ImportSummary>> {
    list_imports_for_account(conn, account_id, 100).map(|rows| rows.into_iter().map(Into::into).collect())
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
               message_count, attachment_count, bytes_uploaded, duration_ms, parse_ms,
               convert_ms, upload_ms, summary_json
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
                duration_ms: row.get(11)?,
                parse_ms: row.get(12)?,
                convert_ms: row.get(13)?,
                upload_ms: row.get(14)?,
                summary_json: row.get(15)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Total attachment bytes for an account (original size_bytes).
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

/// Attachment row count for an account.
pub fn account_attachment_count(conn: &Connection, account_id: &str) -> Result<i64> {
    let n: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        WHERE m.account_id = ?1
        "#,
        params![account_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TopAttachment {
    pub id: i64,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub conversation_id: i64,
    pub conversation_title: Option<String>,
    /// Raw text of the conversation's chat handle (via `handles`).
    pub chat_identifier: String,
}

/// Largest attachments for an account.
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
               h.raw
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        JOIN conversations c ON c.id = m.conversation_id
        JOIN handles h ON h.id = c.chat_handle_id
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

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT_ID: &str = "11111111-1111-1111-1111-111111111111";

    fn setup_accounts_only() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::schema::ensure_accounts_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES (?1, ?2)",
            params![ACCOUNT_ID, "alice"],
        )
        .unwrap();
        conn
    }

    #[test]
    fn complete_import_persists_timings_and_issues() {
        let conn = setup_accounts_only();
        let import_id =
            start_import(&conn, ACCOUNT_ID, "ios", "append", Some("message-vault-io")).unwrap();

        let row = complete_import(
            &conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: true,
                message_count: Some(10),
                attachment_count: Some(2),
                bytes_uploaded: Some(100),
                duration_ms: Some(48_000),
                parse_ms: Some(18_000),
                convert_ms: Some(22_000),
                upload_ms: Some(8_000),
                summary_json: Some(r#"{"parse":{"messages":10}}"#.into()),
                issues: vec![ImportIssueInput {
                    kind: "skip".into(),
                    step: "convert".into(),
                    item: "photo.heic".into(),
                    reason: "convert failed".into(),
                }],
            },
        )
        .unwrap();

        assert_eq!(row.duration_ms, Some(48_000));
        assert_eq!(row.parse_ms, Some(18_000));
        assert_eq!(row.convert_ms, Some(22_000));
        assert_eq!(row.upload_ms, Some(8_000));
        assert_eq!(row.summary_json.as_deref(), Some(r#"{"parse":{"messages":10}}"#));

        let issue_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vault_import_issues WHERE import_id = ?1",
                params![import_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(issue_count, 1);
    }

    #[test]
    fn complete_import_rejects_invalid_issue_kind() {
        let conn = setup_accounts_only();
        let import_id =
            start_import(&conn, ACCOUNT_ID, "ios", "append", Some("message-vault-io")).unwrap();

        let err = complete_import(
            &conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: false,
                message_count: None,
                attachment_count: None,
                bytes_uploaded: None,
                duration_ms: None,
                parse_ms: None,
                convert_ms: None,
                upload_ms: None,
                summary_json: None,
                issues: vec![ImportIssueInput {
                    kind: "warning".into(),
                    step: "upload".into(),
                    item: "archive.zip".into(),
                    reason: "not allowed".into(),
                }],
            },
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("invalid import issue kind"));

        let issue_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vault_import_issues WHERE import_id = ?1",
                params![import_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(issue_count, 0);

        let status: String = conn
            .query_row(
                "SELECT status FROM vault_imports WHERE id = ?1",
                params![import_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "running");
    }

    #[test]
    fn get_import_detail_returns_issues() {
        let conn = setup_accounts_only();
        let import_id =
            start_import(&conn, ACCOUNT_ID, "ios", "append", Some("message-vault-io")).unwrap();
        complete_import(
            &conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: true,
                message_count: Some(10),
                attachment_count: Some(2),
                bytes_uploaded: Some(100),
                duration_ms: Some(48_000),
                parse_ms: Some(18_000),
                convert_ms: Some(22_000),
                upload_ms: Some(8_000),
                summary_json: Some(r#"{"parse":{"messages":10}}"#.into()),
                issues: vec![
                    ImportIssueInput {
                        kind: "skip".into(),
                        step: "convert".into(),
                        item: "photo.heic".into(),
                        reason: "convert failed".into(),
                    },
                    ImportIssueInput {
                        kind: "error".into(),
                        step: "upload".into(),
                        item: "archive.zip".into(),
                        reason: "upload failed".into(),
                    },
                ],
            },
        )
        .unwrap();

        let detail = get_import_detail(&conn, ACCOUNT_ID, import_id).unwrap();
        assert_eq!(detail.row.duration_ms, Some(48_000));
        assert_eq!(detail.row.parse_ms, Some(18_000));
        assert_eq!(detail.issues.len(), 2);
        assert_eq!(detail.issues[0].kind, "skip");
        assert_eq!(detail.issues[0].step, "convert");
        assert_eq!(detail.issues[1].kind, "error");
        assert_eq!(detail.issues[1].step, "upload");
    }

    #[test]
    fn list_imports_includes_duration_ms() {
        let conn = setup_accounts_only();
        let import_id =
            start_import(&conn, ACCOUNT_ID, "ios", "append", Some("message-vault-io")).unwrap();
        complete_import(
            &conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: true,
                message_count: Some(10),
                attachment_count: Some(2),
                bytes_uploaded: Some(100),
                duration_ms: Some(48_000),
                parse_ms: None,
                convert_ms: None,
                upload_ms: None,
                summary_json: None,
                issues: vec![],
            },
        )
        .unwrap();

        let imports = list_imports(&conn, ACCOUNT_ID).unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].duration_ms, Some(48_000));
    }
}
