//! Per-account vault import session records (one row per vault-push / CLI import run).

use std::error::Error;
use std::fmt;

use anyhow::{Result, bail};
use chrono::Utc;
use serde::Serialize;
use sqlx::any::AnyRow;
use sqlx::{AnyConnection, Connection, Row};

use crate::db::dialect;

#[derive(Debug, Clone, Serialize)]
/// One row of `vault_imports`: a per-account import session record.
#[allow(dead_code)]
pub struct VaultImportRow {
    /// Import session id.
    pub id: i64,
    /// Vault account that owns the session.
    pub account_id: String,
    /// Source id the session imports.
    pub source: String,
    /// Importing tool, e.g. `vault-push`.
    pub tool: Option<String>,
    /// Import mode (`replace` or `append`).
    pub mode: String,
    /// Lifecycle status (`running`, `completed`, or `failed`).
    pub status: String,
    /// UTC time the session started.
    pub started_at: String,
    /// UTC time the session finished, when it has.
    pub finished_at: Option<String>,
    /// Messages counted for the session.
    pub message_count: i64,
    /// Attachments counted for the session.
    pub attachment_count: i64,
    /// Bytes uploaded so far.
    pub bytes_uploaded: i64,
    /// Total wall-clock duration, when finished.
    pub duration_ms: Option<i64>,
    /// Time spent parsing JSONL, when finished.
    pub parse_ms: Option<i64>,
    /// Time spent copying, converting, or skipping attachments, when finished.
    pub attachments_ms: Option<i64>,
    /// Time spent preparing conversation files, when finished.
    pub prepare_ms: Option<i64>,
    /// Time spent uploading assets, when finished.
    pub upload_ms: Option<i64>,
    /// Client-provided summary payload.
    pub summary_json: Option<String>,
}

/// Outcome fields written when a session completes.
#[derive(Debug, Clone, Default)]
pub struct CompleteImportArgs {
    /// True when the import finished successfully.
    pub ok: bool,
    /// Explicit outcome status; falls back to `ok` when `None`.
    pub status: Option<String>,
    /// Messages imported; counted from the database when omitted.
    pub message_count: Option<i64>,
    /// Attachments imported; counted from the database when omitted.
    pub attachment_count: Option<i64>,
    /// Bytes uploaded.
    pub bytes_uploaded: Option<i64>,
    /// Total wall-clock duration.
    pub duration_ms: Option<i64>,
    /// Time spent parsing JSONL.
    pub parse_ms: Option<i64>,
    /// Time spent copying, converting, or skipping attachments.
    pub attachments_ms: Option<i64>,
    /// Time spent preparing conversation files.
    pub prepare_ms: Option<i64>,
    /// Time spent uploading assets.
    pub upload_ms: Option<i64>,
    /// Client-provided summary payload.
    pub summary_json: Option<String>,
    /// Per-file issues to record against the session.
    pub issues: Vec<ImportIssueInput>,
}

impl CompleteImportArgs {
    /// Build a success outcome from message and attachment counts.
    pub fn succeeded(messages: u64, attachments: u64) -> Self {
        Self {
            ok: true,
            message_count: Some(messages as i64),
            attachment_count: Some(attachments as i64),
            ..Default::default()
        }
    }

    /// Build a failure outcome; nothing else is recorded.
    pub fn failed() -> Self {
        Self {
            ok: false,
            ..Default::default()
        }
    }
}

/// Finish an import session, logging a warning if the row update fails.
pub async fn complete_import_or_warn(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
    args: &CompleteImportArgs,
) {
    if let Err(e) = complete_import(conn, account_id, import_id, args).await {
        eprintln!("warning: complete_import({import_id}) failed: {e}");
    }
}

/// One problem to record against an import session.
#[derive(Debug, Clone)]
pub struct ImportIssueInput {
    /// Issue category, e.g. `file_missing`.
    pub kind: String,
    /// Pipeline stage that reported it.
    pub step: String,
    /// The file or message the issue is about.
    pub item: String,
    /// Human-readable explanation.
    pub reason: String,
}

/// One stored `vault_import_issues` row.
#[derive(Debug, Clone, Serialize)]
pub struct ImportIssueRow {
    /// Issue row id.
    pub id: i64,
    /// Session the issue belongs to.
    pub import_id: i64,
    /// Issue category, e.g. `file_missing`.
    pub kind: String,
    /// Pipeline stage that reported it.
    pub step: String,
    /// The file or message the issue is about.
    pub item: String,
    /// Human-readable explanation.
    pub reason: String,
    /// UTC time the issue was recorded.
    pub created_at: String,
}

/// An import session row plus its recorded issues.
#[derive(Debug, Clone, Serialize)]
pub struct ImportDetail {
    /// The session.
    pub row: VaultImportRow,
    /// Issues recorded for it.
    pub issues: Vec<ImportIssueRow>,
}

/// Failure looking up or reusing an import session.
#[derive(Debug)]
pub enum ImportLookupError {
    /// No session with this id for this account.
    NotFound {
        /// The session id that was looked up.
        import_id: i64,
    },
    /// Session exists but cannot be reused (wrong status/source/mode).
    InvalidSession {
        /// Why the session cannot be reused.
        message: String,
    },
    /// Database failure.
    Db(anyhow::Error),
}

impl fmt::Display for ImportLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { import_id } => {
                write!(f, "import {import_id} not found for this account")
            }
            Self::InvalidSession { message } => f.write_str(message),
            Self::Db(err) => err.fmt(f),
        }
    }
}

impl Error for ImportLookupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NotFound { .. } | Self::InvalidSession { .. } => None,
            Self::Db(err) => err.source(),
        }
    }
}

impl From<sqlx::Error> for ImportLookupError {
    fn from(value: sqlx::Error) -> Self {
        Self::Db(value.into())
    }
}

impl From<anyhow::Error> for ImportLookupError {
    fn from(value: anyhow::Error) -> Self {
        Self::Db(value)
    }
}

/// Start a running import session for `account_id`.
pub async fn start_import(
    conn: &mut AnyConnection,
    account_id: &str,
    source: &str,
    mode: &str,
    tool: Option<&str>,
) -> Result<i64> {
    let started_at = Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO vault_imports (
            account_id, source, tool, mode, status, started_at,
            message_count, attachment_count, bytes_uploaded
        ) VALUES ($1, $2, $3, $4, 'running', $5, 0, 0, 0)
        RETURNING id
        "#,
    )
    .bind(account_id)
    .bind(source)
    .bind(tool)
    .bind(mode)
    .bind(started_at)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Column list for `vault_imports`, in the order reads map to a row.
const VAULT_IMPORT_COLUMNS: &str = "id, account_id, source, tool, mode, status, started_at, \
     finished_at, message_count, attachment_count, bytes_uploaded, duration_ms, parse_ms, \
     attachments_ms, prepare_ms, upload_ms, summary_json";

fn vault_import_from_row(row: &AnyRow) -> Result<VaultImportRow, sqlx::Error> {
    Ok(VaultImportRow {
        id: row.try_get(0)?,
        account_id: row.try_get(1)?,
        source: row.try_get(2)?,
        tool: row.try_get(3)?,
        mode: row.try_get(4)?,
        status: row.try_get(5)?,
        started_at: row.try_get(6)?,
        finished_at: row.try_get(7)?,
        message_count: row.try_get(8)?,
        attachment_count: row.try_get(9)?,
        bytes_uploaded: row.try_get(10)?,
        duration_ms: row.try_get(11)?,
        parse_ms: row.try_get(12)?,
        attachments_ms: row.try_get(13)?,
        prepare_ms: row.try_get(14)?,
        upload_ms: row.try_get(15)?,
        summary_json: row.try_get(16)?,
    })
}

/// Load an import row owned by `account_id`, or error.
pub async fn get_owned_import(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
) -> std::result::Result<VaultImportRow, ImportLookupError> {
    let row = sqlx::query(&format!(
        "SELECT {VAULT_IMPORT_COLUMNS}
         FROM vault_imports
         WHERE id = $1 AND account_id = $2"
    ))
    .bind(import_id)
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some(data) => Ok(vault_import_from_row(&data)?),
        None => Err(ImportLookupError::NotFound { import_id }),
    }
}

/// Like [`get_owned_import`], but the session must still be `running` and match
/// the source/mode the client is about to import with.
pub async fn require_reusable_import(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
    source: &str,
    mode: &str,
) -> std::result::Result<VaultImportRow, ImportLookupError> {
    let row = get_owned_import(conn, account_id, import_id).await?;
    if row.status != "running" {
        return Err(ImportLookupError::InvalidSession {
            message: format!("import {import_id} is not running (status={})", row.status),
        });
    }
    if row.source != source {
        return Err(ImportLookupError::InvalidSession {
            message: format!(
                "import {import_id} source mismatch (session={}, request={})",
                row.source, source
            ),
        });
    }
    if row.mode != mode {
        return Err(ImportLookupError::InvalidSession {
            message: format!(
                "import {import_id} mode mismatch (session={}, request={})",
                row.mode, mode
            ),
        });
    }
    Ok(row)
}

/// Finish an import: prefer client counts, else derive from linked messages.
pub async fn complete_import(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
    args: &CompleteImportArgs,
) -> Result<VaultImportRow> {
    let existing = get_owned_import(&mut *conn, account_id, import_id).await?;
    let finished_at = Utc::now().to_rfc3339();
    let status = args
        .status
        .as_deref()
        .unwrap_or(if args.ok { "completed" } else { "failed" });

    for issue in &args.issues {
        validate_issue_kind(&issue.kind)?;
    }

    let message_count = if let Some(n) = args.message_count {
        n
    } else {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE import_id = $1 AND account_id = $2",
        )
        .bind(import_id)
        .bind(account_id)
        .fetch_one(&mut *conn)
        .await?;
        n
    };
    let attachment_count = if let Some(n) = args.attachment_count {
        n
    } else {
        let n: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM attachments a
            JOIN messages m ON m.id = a.message_id
            WHERE m.import_id = $1 AND m.account_id = $2
            "#,
        )
        .bind(import_id)
        .bind(account_id)
        .fetch_one(&mut *conn)
        .await?;
        n
    };
    let bytes_uploaded = args.bytes_uploaded.unwrap_or(existing.bytes_uploaded);

    // `BEGIN IMMEDIATE` on SQLite matches today's write lock; Postgres uses a
    // plain BEGIN (no statement-level equivalent). Either way the update and
    // the issue inserts land as one unit, and a failed commit rolls back
    // (sqlx drops the transaction).
    let mut tx = conn
        .begin_with(dialect::begin_immediate_sql(dialect::engine_of(conn)))
        .await?;
    sqlx::query(
        r#"
        UPDATE vault_imports
        SET status = $1,
            finished_at = $2,
            message_count = $3,
            attachment_count = $4,
            bytes_uploaded = $5,
            duration_ms = $6,
            parse_ms = $7,
            attachments_ms = $8,
            prepare_ms = $9,
            upload_ms = $10,
            summary_json = $11
        WHERE id = $12 AND account_id = $13
        "#,
    )
    .bind(status)
    .bind(finished_at)
    .bind(message_count)
    .bind(attachment_count)
    .bind(bytes_uploaded)
    .bind(args.duration_ms)
    .bind(args.parse_ms)
    .bind(args.attachments_ms)
    .bind(args.prepare_ms)
    .bind(args.upload_ms)
    .bind(args.summary_json.as_deref())
    .bind(import_id)
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    insert_issues(&mut tx, import_id, &args.issues).await?;
    tx.commit().await?;

    Ok(get_owned_import(&mut *conn, account_id, import_id).await?)
}

async fn insert_issues(
    conn: &mut AnyConnection,
    import_id: i64,
    issues: &[ImportIssueInput],
) -> Result<()> {
    for issue in issues {
        sqlx::query(
            r#"
            INSERT INTO vault_import_issues (
                import_id, kind, step, item, reason, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(import_id)
        .bind(&issue.kind)
        .bind(&issue.step)
        .bind(&issue.item)
        .bind(&issue.reason)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *conn)
        .await?;
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
pub async fn get_import_detail(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
) -> std::result::Result<ImportDetail, ImportLookupError> {
    let row = get_owned_import(conn, account_id, import_id).await?;
    let issue_rows: Vec<(i64, i64, String, String, String, String, String)> = sqlx::query_as(
        r#"
        SELECT id, import_id, kind, step, item, reason, created_at
        FROM vault_import_issues
        WHERE import_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(import_id)
    .fetch_all(&mut *conn)
    .await?;
    let issues = issue_rows
        .into_iter()
        .map(
            |(id, import_id, kind, step, item, reason, created_at)| ImportIssueRow {
                id,
                import_id,
                kind,
                step,
                item,
                reason,
                created_at,
            },
        )
        .collect();
    Ok(ImportDetail { row, issues })
}

/// Serializable slice of a session used in list responses.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ImportSummary {
    /// Import session id.
    pub id: i64,
    /// Source id the session imports.
    pub source: String,
    /// Importing tool, e.g. `vault-push`.
    pub tool: Option<String>,
    /// Import mode (`replace` or `append`).
    pub mode: String,
    /// Lifecycle status (`running`, `completed`, or `failed`).
    pub status: String,
    /// UTC time the session started.
    pub started_at: String,
    /// UTC time the session finished, when it has.
    pub finished_at: Option<String>,
    /// Messages counted for the session.
    pub message_count: i64,
    /// Attachments counted for the session.
    pub attachment_count: i64,
    /// Bytes uploaded so far.
    pub bytes_uploaded: i64,
    /// Total wall-clock duration, when finished.
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
pub async fn list_imports(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<ImportSummary>> {
    list_imports_for_account(conn, account_id, 100)
        .await
        .map(|rows| rows.into_iter().map(Into::into).collect())
}

/// List imports for an account, newest first.
#[allow(dead_code)] // used by unit tests; storage UI queries SQLite from Next.js
pub async fn list_imports_for_account(
    conn: &mut AnyConnection,
    account_id: &str,
    limit: i64,
) -> Result<Vec<VaultImportRow>> {
    let rows = sqlx::query(&format!(
        "SELECT {VAULT_IMPORT_COLUMNS}
         FROM vault_imports
         WHERE account_id = $1
         ORDER BY started_at DESC, id DESC
         LIMIT $2"
    ))
    .bind(account_id)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    rows.iter()
        .map(vault_import_from_row)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

const ACCOUNT_ATTACHMENTS_FROM: &str = r#"
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        WHERE m.account_id = $1
        "#;

/// Total attachment bytes for an account (original size_bytes).
pub async fn account_attachment_bytes(conn: &mut AnyConnection, account_id: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(a.size_bytes), 0) {ACCOUNT_ATTACHMENTS_FROM}"
    ))
    .bind(account_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(n)
}

/// Attachment row count for an account.
pub async fn account_attachment_count(conn: &mut AnyConnection, account_id: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {ACCOUNT_ATTACHMENTS_FROM}"))
        .bind(account_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(n)
}

/// One of an account's largest attachments by byte size.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TopAttachment {
    /// Attachment id.
    pub id: i64,
    /// File name from the export.
    pub original_name: Option<String>,
    /// MIME type, when known.
    pub mime_type: Option<String>,
    /// Attachment byte size.
    pub size_bytes: i64,
    /// Conversation that holds the attachment.
    pub conversation_id: i64,
    /// Conversation label, when set.
    pub conversation_title: Option<String>,
    /// Raw text of the conversation's chat handle (via `handles`).
    pub chat_identifier: String,
}

/// Raw row for [`top_attachments_by_size`] before mapping to [`TopAttachment`].
type TopAttachmentRow = (
    i64,
    Option<String>,
    Option<String>,
    i64,
    i64,
    Option<String>,
    String,
);

/// Largest attachments for an account.
pub async fn top_attachments_by_size(
    conn: &mut AnyConnection,
    account_id: &str,
    limit: i64,
) -> Result<Vec<TopAttachment>> {
    let rows: Vec<TopAttachmentRow> = sqlx::query_as(
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
            WHERE m.account_id = $1
              AND COALESCE(a.size_bytes, 0) > 0
            ORDER BY a.size_bytes DESC, a.id DESC
            LIMIT $2
            "#,
    )
    .bind(account_id)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                original_name,
                mime_type,
                size_bytes,
                conversation_id,
                conversation_title,
                chat_identifier,
            )| TopAttachment {
                id,
                original_name,
                mime_type,
                size_bytes,
                conversation_id,
                conversation_title,
                chat_identifier,
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT_ID: &str = "11111111-1111-1111-1111-111111111111";

    async fn setup_accounts_only() -> (sqlx::AnyPool, tempfile::TempDir) {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        crate::db::schema::ensure_accounts_schema(&mut conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, $2)")
            .bind(ACCOUNT_ID)
            .bind("alice")
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn complete_import_persists_timings_and_issues() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let import_id = start_import(
            &mut conn,
            ACCOUNT_ID,
            "ios",
            "append",
            Some("message-vault-io"),
        )
        .await
        .unwrap();

        let row = complete_import(
            &mut conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: true,
                status: None,
                message_count: Some(10),
                attachment_count: Some(2),
                bytes_uploaded: Some(100),
                duration_ms: Some(48_000),
                parse_ms: Some(18_000),
                attachments_ms: Some(22_000),
                prepare_ms: Some(4_000),
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
        .await
        .unwrap();

        assert_eq!(row.duration_ms, Some(48_000));
        assert_eq!(row.parse_ms, Some(18_000));
        assert_eq!(row.attachments_ms, Some(22_000));
        assert_eq!(row.prepare_ms, Some(4_000));
        assert_eq!(row.upload_ms, Some(8_000));
        assert_eq!(
            row.summary_json.as_deref(),
            Some(r#"{"parse":{"messages":10}}"#)
        );

        let issue_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vault_import_issues WHERE import_id = $1")
                .bind(import_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(issue_count, 1);
    }

    #[tokio::test]
    async fn complete_import_rejects_invalid_issue_kind() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let import_id = start_import(
            &mut conn,
            ACCOUNT_ID,
            "ios",
            "append",
            Some("message-vault-io"),
        )
        .await
        .unwrap();

        let err = complete_import(
            &mut conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: false,
                status: None,
                message_count: None,
                attachment_count: None,
                bytes_uploaded: None,
                duration_ms: None,
                parse_ms: None,
                attachments_ms: None,
                prepare_ms: None,
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
        .await
        .unwrap_err()
        .to_string();

        assert!(err.contains("invalid import issue kind"));

        let issue_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vault_import_issues WHERE import_id = $1")
                .bind(import_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(issue_count, 0);

        let status: String = sqlx::query_scalar("SELECT status FROM vault_imports WHERE id = $1")
            .bind(import_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[tokio::test]
    async fn require_reusable_import_rejects_completed_and_mismatched() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let import_id = start_import(
            &mut conn,
            ACCOUNT_ID,
            "ios",
            "append",
            Some("message-vault-io"),
        )
        .await
        .unwrap();
        complete_import(
            &mut conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs::succeeded(1, 0),
        )
        .await
        .unwrap();

        let err = require_reusable_import(&mut conn, ACCOUNT_ID, import_id, "ios", "append")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not running"), "{err}");

        let running = start_import(
            &mut conn,
            ACCOUNT_ID,
            "ios",
            "append",
            Some("message-vault-io"),
        )
        .await
        .unwrap();
        let src_err = require_reusable_import(&mut conn, ACCOUNT_ID, running, "android", "append")
            .await
            .unwrap_err()
            .to_string();
        assert!(src_err.contains("source mismatch"), "{src_err}");
        let mode_err = require_reusable_import(&mut conn, ACCOUNT_ID, running, "ios", "replace")
            .await
            .unwrap_err()
            .to_string();
        assert!(mode_err.contains("mode mismatch"), "{mode_err}");
        assert!(
            require_reusable_import(&mut conn, ACCOUNT_ID, running, "ios", "append")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn get_import_detail_returns_issues() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let import_id = start_import(
            &mut conn,
            ACCOUNT_ID,
            "ios",
            "append",
            Some("message-vault-io"),
        )
        .await
        .unwrap();
        complete_import(
            &mut conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: true,
                status: None,
                message_count: Some(10),
                attachment_count: Some(2),
                bytes_uploaded: Some(100),
                duration_ms: Some(48_000),
                parse_ms: Some(18_000),
                attachments_ms: Some(22_000),
                prepare_ms: Some(4_000),
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
        .await
        .unwrap();

        let detail = get_import_detail(&mut conn, ACCOUNT_ID, import_id)
            .await
            .unwrap();
        assert_eq!(detail.row.duration_ms, Some(48_000));
        assert_eq!(detail.row.parse_ms, Some(18_000));
        assert_eq!(detail.issues.len(), 2);
        assert_eq!(detail.issues[0].kind, "skip");
        assert_eq!(detail.issues[0].step, "convert");
        assert_eq!(detail.issues[1].kind, "error");
        assert_eq!(detail.issues[1].step, "upload");
    }

    #[tokio::test]
    async fn list_imports_includes_duration_ms() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let import_id = start_import(
            &mut conn,
            ACCOUNT_ID,
            "ios",
            "append",
            Some("message-vault-io"),
        )
        .await
        .unwrap();
        complete_import(
            &mut conn,
            ACCOUNT_ID,
            import_id,
            &CompleteImportArgs {
                ok: true,
                status: None,
                message_count: Some(10),
                attachment_count: Some(2),
                bytes_uploaded: Some(100),
                duration_ms: Some(48_000),
                parse_ms: None,
                attachments_ms: None,
                prepare_ms: None,
                upload_ms: None,
                summary_json: None,
                issues: vec![],
            },
        )
        .await
        .unwrap();

        let imports = list_imports(&mut conn, ACCOUNT_ID).await.unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].duration_ms, Some(48_000));
    }
}
