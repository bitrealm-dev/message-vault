//! Read-only message export query used by `GET /v1/export/messages`
//! and `GET /v1/export/messages/count`.

use crate::extract::{Json, Query};
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use sqlx::{Executor, Row};

use crate::db::dialect::engine_of;
use crate::db::engine::DbEngine;
use crate::db::sql::{SqlParam, bind_all, group_rows_by_id, renumber_placeholders};
// Required so the moved handlers' unqualified `export_api::…` paths resolve.
use crate::export_api::{self};
use crate::server::{ApiError, AppState, ExportAccess, resolve_import_account};

use crate::paging::{DEFAULT_EXPORT_LIMIT, Page, page_params};

/// Options for one exported page of messages.
#[derive(Debug, Clone)]
pub struct ExportPageOpts<'a> {
    /// Vault account to export from.
    pub account_id: &'a str,
    /// Search query string, in the search language.
    pub query: &'a str,
    /// Max messages on the page. Already validated by the handler: 1..=MAX_LIST_LIMIT.
    pub limit: usize,
    /// Row offset.
    pub offset: usize,
    /// The day relative dates in `query` resolve against.
    pub today: chrono::NaiveDate,
}

/// Options for one export count query.
#[derive(Debug, Clone)]
pub struct ExportCountOpts<'a> {
    /// Vault account to count from.
    pub account_id: &'a str,
    /// Search query string, in the search language.
    pub query: &'a str,
    /// The day relative dates in `query` resolve against.
    pub today: chrono::NaiveDate,
}

/// Match counts for an export query.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportCountResponse {
    /// Matching messages.
    pub messages: u64,
    /// Distinct conversations with at least one matching message.
    pub conversations: u64,
    /// Unique attachment fingerprints among matching messages.
    pub attachments: u64,
    /// Sum of known `size_bytes` for those unique fingerprints (unknown sizes omitted).
    pub total_bytes: u64,
}

/// One exported message.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportMessage {
    /// Message row id.
    pub id: i64,
    /// Import source id.
    pub source: String,
    /// Platform service, e.g. `imessage`, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Export GUID for replies and grouping.
    pub guid: Option<String>,
    /// Message timestamp (local).
    pub timestamp: String,
    /// UTC timestamp, when known.
    pub timestamp_utc: Option<String>,
    /// Ordering key within the conversation.
    pub sort_order: i64,
    /// True for messages sent by the account owner.
    pub is_from_me: bool,
    /// Sender handle for incoming messages.
    pub sender: Option<String>,
    /// Subject line, when set.
    pub subject: Option<String>,
    /// Body text, when present.
    pub text: Option<String>,
    /// True for group announcements.
    pub is_announcement: bool,
    /// True when part of a reply thread.
    pub is_reply: bool,
    /// GUID of the message this replies to.
    pub thread_originator_guid: Option<String>,
    /// Part index of the originator (for tapbacks).
    pub thread_originator_part: Option<i64>,
    /// Replies in this thread.
    pub num_replies: i64,
    /// The conversation this message belongs to.
    pub conversation: ExportConversation,
    /// Attachments on this message.
    pub attachments: Vec<ExportAttachment>,
    /// Reactions on this message.
    pub tapbacks: Vec<ExportTapback>,
}

/// The conversation a message belongs to.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportConversation {
    /// Conversation row id.
    pub id: i64,
    /// Original chat id from the export.
    pub chat_identifier: String,
    /// `individual` or `group`.
    pub conversation_type: String,
    /// Group label, when set.
    pub group_title: Option<String>,
    /// Participants of the conversation.
    pub participants: Vec<ExportParticipant>,
}

/// One participant of an exported conversation.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ExportParticipant {
    /// Raw handle value.
    pub handle: String,
    /// Per-service alias, when linked to a contact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_alias: Option<String>,
    /// Vault contact display name, when linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_name: Option<String>,
    /// Linked contact id, when the handle is linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<i64>,
    /// Handle type (`phone`, `email`, or username).
    pub handle_type: Option<String>,
}

/// One attachment of an exported message.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ExportAttachment {
    /// Path inside the export.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// File name from the export.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// MIME type, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Content fingerprint of the stored bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// True for sticker files.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_sticker: bool,
    /// OCR/ASR transcription, when processed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
    /// Why the file is missing, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_reason: Option<String>,
}

/// One tapback reaction on an exported message.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ExportTapback {
    /// Attachment part the reaction applies to.
    pub part_index: i64,
    /// Reaction type, e.g. `love`.
    pub kind: String,
    /// Emoji form of the reaction, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// True when the account owner reacted.
    pub is_from_me: bool,
    /// Reactor handle for incoming reactions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
}

fn unique_ids(ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Compile `query` into a WHERE fragment over the messages alias `m`.
fn message_filter(
    engine: DbEngine,
    account_id: &str,
    query: &str,
    today: chrono::NaiveDate,
) -> Result<crate::search::Filter, ApiError> {
    Ok(crate::search::compile(crate::search::CompileRequest {
        list: crate::search::ListKind::Messages,
        query,
        account_id,
        engine,
        today,
    })?)
}

/// `COUNT(*)` of the messages a compiled filter matches.
async fn count_matching_messages(
    conn: &mut AnyConnection,
    filter: &crate::search::Filter,
) -> Result<u64, ApiError> {
    let sql = format!(
        "SELECT COUNT(*)
         {messages_from_sql}
         WHERE {where_sql}",
        messages_from_sql = messages_from_sql(),
        where_sql = filter.where_sql(),
    );
    let n: i64 = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&sql), filter.params()))
        .await?
        .try_get(0)?;
    Ok(n.max(0) as u64)
}

/// Export messages matching a query in the search language, a page at a time.
///
/// An empty query returns every non-trashed, non-duplicate message for the account.
/// An offset past the end returns an empty page carrying the true `total`.
///
/// # Errors
///
/// Returns a bad-request error for an invalid query, or an internal
/// error when a database statement fails.
pub async fn export_messages(
    conn: &mut AnyConnection,
    opts: ExportPageOpts<'_>,
) -> Result<Page<ExportMessage>, ApiError> {
    let filter = message_filter(engine_of(conn), opts.account_id, opts.query, opts.today)?;
    let total = count_matching_messages(conn, &filter).await?;

    let mut sql = format!(
        "SELECT m.id, m.conversation_id, m.source, m.service, m.guid, m.timestamp, m.timestamp_utc,
                m.sort_order, m.is_from_me, hs.raw AS sender, m.subject, m.body,
                m.is_announcement, m.is_reply, m.thread_originator_guid,
                m.thread_originator_part, m.num_replies,
                hc.raw AS chat_identifier, c.conversation_type, c.group_title
         {messages_from_sql}
         WHERE {where_sql}",
        messages_from_sql = messages_from_sql(),
        where_sql = filter.where_sql(),
    );
    let mut params = filter.params().to_vec();
    sql.push_str(" ORDER BY m.timestamp ASC, m.sort_order ASC, m.id ASC LIMIT ? OFFSET ?");
    params.push(SqlParam::Int(opts.limit as i64));
    params.push(SqlParam::Int(opts.offset as i64));

    let sql = renumber_placeholders(&sql);
    let rows = (&mut *conn).fetch_all(bind_all(&sql, &params)).await?;
    let page_rows: Vec<RawRow> = rows
        .iter()
        .map(|row| {
            Ok(RawRow {
                id: row.try_get::<i64, _>(0)?,
                conversation_id: row.try_get(1)?,
                source: row.try_get(2)?,
                service: row.try_get(3)?,
                guid: row.try_get(4)?,
                timestamp: row.try_get(5)?,
                timestamp_utc: row.try_get(6)?,
                sort_order: row.try_get(7)?,
                is_from_me: row.try_get::<i64, _>(8)? != 0,
                sender: row.try_get(9)?,
                subject: row.try_get(10)?,
                body: row.try_get(11)?,
                is_announcement: row.try_get::<i64, _>(12)? != 0,
                is_reply: row.try_get::<i64, _>(13)? != 0,
                thread_originator_guid: row.try_get(14)?,
                thread_originator_part: row.try_get(15)?,
                num_replies: row.try_get(16)?,
                chat_identifier: row.try_get(17)?,
                conversation_type: row.try_get(18)?,
                group_title: row.try_get(19)?,
            })
        })
        .collect::<Result<Vec<RawRow>, ApiError>>()?;

    let conv_ids = unique_ids(page_rows.iter().map(|r| r.conversation_id));
    let participants = load_participants(conn, &conv_ids).await?;
    let msg_ids: Vec<i64> = page_rows.iter().map(|r| r.id).collect();
    let attachments = load_attachments(conn, &msg_ids).await?;
    let tapbacks = load_tapbacks(conn, &msg_ids).await?;

    let messages = page_rows
        .into_iter()
        .map(|r| {
            let parts = participants
                .get(&r.conversation_id)
                .cloned()
                .unwrap_or_default();
            ExportMessage {
                id: r.id,
                source: r.source,
                service: r.service,
                guid: r.guid,
                timestamp: r.timestamp,
                timestamp_utc: r.timestamp_utc,
                sort_order: r.sort_order,
                is_from_me: r.is_from_me,
                sender: r.sender,
                subject: r.subject,
                text: r.body,
                is_announcement: r.is_announcement,
                is_reply: r.is_reply,
                thread_originator_guid: r.thread_originator_guid,
                thread_originator_part: r.thread_originator_part,
                num_replies: r.num_replies,
                conversation: ExportConversation {
                    id: r.conversation_id,
                    chat_identifier: r.chat_identifier,
                    conversation_type: r.conversation_type,
                    group_title: r.group_title,
                    participants: parts,
                },
                attachments: attachments.get(&r.id).cloned().unwrap_or_default(),
                tapbacks: tapbacks.get(&r.id).cloned().unwrap_or_default(),
            }
        })
        .collect();

    Ok(Page {
        items: messages,
        total,
        limit: opts.limit,
        offset: opts.offset,
    })
}

/// Aggregate counts for messages matching a query in the search language (no paging).
///
/// Attachment count is unique non-empty SHA-256 fingerprints (a short
/// fingerprint of the file contents) on matching messages.
/// `total_bytes` sums known `attachments.size_bytes` for those fingerprints.
///
/// # Errors
///
/// Returns a bad-request error for an invalid query, or an internal error when
/// a database statement fails.
pub async fn export_message_count(
    conn: &mut AnyConnection,
    opts: ExportCountOpts<'_>,
) -> Result<ExportCountResponse, ApiError> {
    let filter = message_filter(engine_of(conn), opts.account_id, opts.query, opts.today)?;
    let params = filter.params();

    let messages = count_matching_messages(conn, &filter).await?;

    let conv_sql = format!(
        "SELECT COUNT(DISTINCT c.id)
         {messages_from_sql}
         WHERE {where_sql}",
        messages_from_sql = messages_from_sql(),
        where_sql = filter.where_sql(),
    );
    let conversations: i64 = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&conv_sql), params))
        .await?
        .try_get(0)?;

    let att_sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(sz), 0)
         FROM (
           SELECT MAX(a.size_bytes) AS sz
           FROM attachments a
           JOIN messages m ON m.id = a.message_id
           {conversation_join_sql}
           WHERE {where_sql}
             AND a.sha256 IS NOT NULL
             AND length(trim(a.sha256)) > 0
           GROUP BY lower(trim(a.sha256))
         )",
        conversation_join_sql = conversation_join_sql(),
        where_sql = filter.where_sql(),
    );
    let row = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&att_sql), params))
        .await?;
    let (attachments, total_bytes): (i64, i64) = (row.try_get(0)?, row.try_get(1)?);

    Ok(ExportCountResponse {
        messages,
        conversations: conversations.max(0) as u64,
        attachments: attachments.max(0) as u64,
        total_bytes: total_bytes.max(0) as u64,
    })
}

struct RawRow {
    id: i64,
    conversation_id: i64,
    source: String,
    service: Option<String>,
    guid: Option<String>,
    timestamp: String,
    timestamp_utc: Option<String>,
    sort_order: i64,
    is_from_me: bool,
    sender: Option<String>,
    subject: Option<String>,
    body: Option<String>,
    is_announcement: bool,
    is_reply: bool,
    thread_originator_guid: Option<String>,
    thread_originator_part: Option<i64>,
    num_replies: i64,
    chat_identifier: String,
    conversation_type: String,
    group_title: Option<String>,
}

/// FROM clause for message queries. The compiled filter mentions only `m`;
/// these joins are here for the SELECT list, which reports the conversation
/// and the two handles' raw text. The count statements carry the same joins
/// so they count exactly the rows the page can return.
fn messages_from_sql() -> String {
    format!("FROM messages m\n{}", conversation_join_sql())
}

/// Handles joins for a query already anchored on `messages m`.
/// `hc` supplies `c.chat_handle_id` raw text; `hs` supplies `m.sender_handle_id`
/// raw text (LEFT, since outgoing messages carry no sender handle).
fn conversation_join_sql() -> String {
    "JOIN conversations c ON c.id = m.conversation_id
     JOIN handles hc ON hc.id = c.chat_handle_id
     LEFT JOIN handles hs ON hs.id = m.sender_handle_id"
        .into()
}

async fn load_participants(
    conn: &mut AnyConnection,
    conversation_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<ExportParticipant>>, ApiError> {
    group_rows_by_id(
        conn,
        conversation_ids,
        |placeholders| {
            format!(
                "SELECT p.conversation_id,
                    h.raw AS handle,
                    CASE
                      WHEN ch.handle_id IS NOT NULL THEN NULLIF(trim(ch.name_alias), '')
                      ELSE NULLIF(trim(p.name_alias), '')
                    END AS name_alias,
                    CASE
                      WHEN ch.handle_id IS NOT NULL THEN NULLIF(trim(c.preferred_name), '')
                      ELSE NULL
                    END AS preferred_name,
                    h.handle_type,
                    p.contact_id
             FROM participants p
             JOIN handles h ON h.id = p.handle_id
             JOIN conversations conv ON conv.id = p.conversation_id
             LEFT JOIN contact_handles ch
               ON ch.handle_id = p.handle_id AND ch.account_id = conv.account_id
             LEFT JOIN contacts c
               ON c.id = ch.contact_id AND c.account_id = conv.account_id
             WHERE p.conversation_id IN ({placeholders})
             ORDER BY p.conversation_id, p.id"
            )
        },
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                ExportParticipant {
                    handle: row.try_get(1)?,
                    name_alias: row.try_get(2)?,
                    preferred_name: row.try_get(3)?,
                    handle_type: row.try_get(4)?,
                    contact_id: row.try_get(5)?,
                },
            ))
        },
    )
    .await
}

async fn load_attachments(
    conn: &mut AnyConnection,
    message_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<ExportAttachment>>, ApiError> {
    group_rows_by_id(
        conn,
        message_ids,
        |placeholders| {
            format!(
                "SELECT message_id, path, original_name, mime_type, sha256, is_sticker, transcription,
                    missing_reason
             FROM attachments
             WHERE message_id IN ({placeholders})
             ORDER BY message_id, id"
            )
        },
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                ExportAttachment {
                    path: row.try_get(1)?,
                    original_name: row.try_get(2)?,
                    mime_type: row.try_get(3)?,
                    sha256: row.try_get(4)?,
                    is_sticker: row.try_get::<i64, _>(5)? != 0,
                    transcription: row.try_get(6)?,
                    missing_reason: row.try_get(7)?,
                },
            ))
        },
    )
    .await
}

async fn load_tapbacks(
    conn: &mut AnyConnection,
    message_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<ExportTapback>>, ApiError> {
    group_rows_by_id(
        conn,
        message_ids,
        |placeholders| {
            format!(
                "SELECT t.message_id, t.part_index, t.kind, t.emoji, t.is_from_me,
                    hs.raw AS sender
             FROM tapbacks t
             LEFT JOIN handles hs ON hs.id = t.sender_handle_id
             WHERE t.message_id IN ({placeholders})
             ORDER BY t.message_id, t.id"
            )
        },
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                ExportTapback {
                    part_index: row.try_get(1)?,
                    kind: row.try_get(2)?,
                    emoji: row.try_get(3)?,
                    is_from_me: row.try_get::<i64, _>(4)? != 0,
                    sender: row.try_get(5)?,
                },
            ))
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExportMessagesQuery {
    #[serde(default)]
    pub(crate) q: String,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) account: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExportMessagesCountQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    account: Option<String>,
}

/// Count messages, conversations, and attachment fingerprints matching a
/// query.
#[utoipa::path(
    get,
    path = "/v1/export/messages/count",
    tag = "Export",
    security(("bearer" = [])),
    params(
        ("q" = String, Query, description = "Query in the search language; empty is every non-trashed message"),
        ("account" = Option<String>, Query)
    ),
    responses(
        (status = 200, body = crate::export_api::ExportCountResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn export_messages_count_handler(
    State(state): State<AppState>,
    ExportAccess(auth): ExportAccess,
    Query(query): Query<ExportMessagesCountQuery>,
) -> Result<Json<export_api::ExportCountResponse>, ApiError> {
    let account = resolve_import_account(&auth, query.account.as_deref(), &state.db).await?;
    let q = query.q;
    let today = chrono::Local::now().date_naive();

    let mut conn = state.db.acquire().await?;
    let body = export_api::export_message_count(
        &mut conn,
        ExportCountOpts {
            account_id: &account,
            query: &q,
            today,
        },
    )
    .await?;
    Ok(Json(body))
}

/// Export messages matching a query in the search language, a page at a time.
#[utoipa::path(
    get,
    path = "/v1/export/messages",
    tag = "Export",
    security(("bearer" = [])),
    params(
        ("q" = String, Query, description = "Query in the search language; empty is every non-trashed message"),
        ("limit" = Option<usize>, Query, description = "Page size, default 100, max 500"),
        ("offset" = Option<usize>, Query, description = "Page offset; no cap, an offset past the end is an empty page"),
        ("account" = Option<String>, Query)
    ),
    responses(
        (status = 200, body = crate::paging::Page<crate::export_api::ExportMessage>),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn export_messages_handler(
    State(state): State<AppState>,
    ExportAccess(auth): ExportAccess,
    Query(query): Query<ExportMessagesQuery>,
) -> Result<Json<Page<export_api::ExportMessage>>, ApiError> {
    let account = resolve_import_account(&auth, query.account.as_deref(), &state.db).await?;
    let page = page_params(query.limit, query.offset, DEFAULT_EXPORT_LIMIT, None)?;
    let today = chrono::Local::now().date_naive();

    let mut conn = state.db.acquire().await?;
    let body = export_api::export_messages(
        &mut conn,
        ExportPageOpts {
            account_id: &account,
            query: &query.q,
            limit: page.limit,
            offset: page.offset,
            today,
        },
    )
    .await?;
    Ok(Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{engine, schema};

    #[tokio::test]
    async fn export_takes_the_search_language() {
        let (pool, _dir, f) = crate::search::tests::seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        let today = crate::search::tests::today();
        let page = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: crate::search::tests::ACCOUNT,
                query: "from:me avocado",
                limit: 50,
                offset: 0,
                today,
            },
        )
        .await
        .unwrap();
        let mut ids: Vec<i64> = page.items.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![f.jane_avocado_from_me, f.sam_avocado_from_me]);

        let count = export_message_count(
            &mut conn,
            ExportCountOpts {
                account_id: crate::search::tests::ACCOUNT,
                query: "source:whatsapp",
                today,
            },
        )
        .await
        .unwrap();
        assert_eq!(count.messages, 1);

        // A word the language does not have is a 400, not a text search.
        let err = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: crate::search::tests::ACCOUNT,
                query: "sparkle:yes",
                limit: 50,
                offset: 0,
                today,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind("a1")
            .execute(&mut *conn)
            .await
            .unwrap();
        // Create handles and conversations using chat_handle_id (FK to handles).
        for (cid, phone) in [(1, "+1555"), (2, "+1666")] {
            sqlx::query(
                "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                 VALUES ($1, $2, $2, 'phone', 'phone')",
            )
            .bind("a1")
            .bind(phone)
            .execute(&mut *conn)
            .await
            .unwrap();
            let handle_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO conversations (id, account_id, chat_handle_id, conversation_type, source_file)
                 VALUES ($1, 'a1', $2, 'individual', 'backup-a.jsonl')",
            )
            .bind(cid)
            .bind(handle_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, account_id, source, service, timestamp, is_from_me, sort_order, body)
             VALUES (1, 1, 'a1', 'sms', 'sms', '2020-01-01T00:00:00Z', 0, 0, 'hello one'),
                    (2, 2, 'a1', 'sms', 'sms', '2020-01-02T00:00:00Z', 0, 0, 'hello two')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn export_includes_attachment_missing_reason() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO attachments (
                message_id, path, original_name, mime_type, sha256, is_sticker,
                size_bytes, missing_reason
             ) VALUES (1, 'attachments/gone.bin', 'gone.bin', 'image/png', NULL, 0, 2048, 'file_missing')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "in:#1",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].attachments.len(), 1);
        let att = &res.items[0].attachments[0];
        assert!(att.sha256.is_none());
        assert_eq!(att.missing_reason.as_deref(), Some("file_missing"));
        assert_eq!(att.original_name.as_deref(), Some("gone.bin"));
        assert_eq!(att.mime_type.as_deref(), Some("image/png"));
    }

    #[tokio::test]
    async fn conversation_filter_scopes_messages() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "in:#1",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].id, 1);
        assert_eq!(res.items[0].service.as_deref(), Some("sms"));

        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "in:#2",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].id, 2);

        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert_eq!(res.items.len(), 2);
    }

    #[tokio::test]
    async fn export_message_count_supports_handle_filters() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('a1', 'alice', 'alice', 'other', 'other')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let sender_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query("UPDATE messages SET sender_handle_id = $1 WHERE id = 1")
            .bind(sender_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE handles SET raw = 'alice-chat', normalized = 'alice-chat'
             WHERE id = (SELECT chat_handle_id FROM conversations WHERE id = 1)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attachments (
                message_id, path, original_name, mime_type, sha256, is_sticker, size_bytes
             ) VALUES (1, 'attachments/a.txt', 'a.txt', 'text/plain', 'abc123', 0, 12)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        for query in ["from:alice", "in:alice-chat"] {
            let counts = export_message_count(
                &mut conn,
                ExportCountOpts {
                    account_id: "a1",
                    query,
                    today: crate::search::tests::today(),
                },
            )
            .await
            .unwrap();
            assert_eq!(counts.messages, 1, "query={query}");
            assert_eq!(counts.conversations, 1, "query={query}");
            assert_eq!(counts.attachments, 1, "query={query}");
        }
    }

    #[tokio::test]
    async fn free_text_matches_message_body_via_fts() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "one",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].id, 1);
        assert!(res.items[0].text.as_deref().unwrap_or("").contains("one"));
    }

    #[tokio::test]
    async fn export_boolean_query_preserves_or() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE messages SET body = 'foo' WHERE id = 1")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("UPDATE messages SET body = 'bar' WHERE id = 2")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                id, conversation_id, account_id, source, service, timestamp,
                is_from_me, sort_order, body
             ) VALUES (
                3, 1, 'a1', 'sms', 'sms', '2020-01-03T00:00:00Z',
                0, 0, 'foo bar'
             )",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let result = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "foo OR bar",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        let ids: Vec<i64> = result.items.iter().map(|message| message.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn export_boolean_query_preserves_and_and_not() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE messages SET body = 'foo' WHERE id = 1")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("UPDATE messages SET body = 'bar' WHERE id = 2")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                id, conversation_id, account_id, source, service, timestamp,
                is_from_me, sort_order, body
             ) VALUES (
                3, 1, 'a1', 'sms', 'sms', '2020-01-03T00:00:00Z',
                0, 0, 'foo bar'
             )",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        // All call sites pass string literals, so `'static` sidesteps the
        // closure-returning-future lifetime puzzle (the future would otherwise
        // borrow a caller-owned reference the closure cannot name).
        let matching_ids = |query: &'static str| {
            let pool = pool.clone();
            async move {
                let mut conn = pool.acquire().await.unwrap();
                export_messages(
                    &mut conn,
                    ExportPageOpts {
                        account_id: "a1",
                        query,
                        limit: 100,
                        offset: 0,
                        today: crate::search::tests::today(),
                    },
                )
                .await
                .unwrap()
                .items
                .into_iter()
                .map(|message| message.id)
                .collect::<Vec<_>>()
            }
        };
        assert_eq!(matching_ids("foo AND bar").await, vec![3]);
        assert_eq!(matching_ids("foo AND NOT bar").await, vec![1]);
    }

    #[tokio::test]
    async fn export_boolean_query_combines_body_phrases_prefixes_and_nesting() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE messages SET body = 'alpha phrase at sunrise' WHERE id = 1")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("UPDATE messages SET body = 'unrelated' WHERE id = 2")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO attachments (
                message_id, path, original_name, mime_type, sha256, is_sticker
             ) VALUES (
                2, 'attachments/report-final.pdf', 'report-final.pdf',
                'application/pdf', 'report-digest', 0
             )",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        // All call sites pass string literals, so `'static` sidesteps the
        // closure-returning-future lifetime puzzle (the future would otherwise
        // borrow a caller-owned reference the closure cannot name).
        let matching_ids = |query: &'static str| {
            let pool = pool.clone();
            async move {
                let mut conn = pool.acquire().await.unwrap();
                export_messages(
                    &mut conn,
                    ExportPageOpts {
                        account_id: "a1",
                        query,
                        limit: 100,
                        offset: 0,
                        today: crate::search::tests::today(),
                    },
                )
                .await
                .unwrap()
                .items
                .into_iter()
                .map(|message| message.id)
                .collect::<Vec<_>>()
            }
        };

        assert_eq!(
            matching_ids(r#""alpha phrase" OR report*"#).await,
            vec![1, 2]
        );
        assert_eq!(
            matching_ids(r#"sunrise AND ("alpha phrase" OR report*)"#).await,
            vec![1]
        );
        assert_eq!(matching_ids("NOT NOT report*").await, vec![2]);

        sqlx::query(
            "INSERT INTO trashed_conversations (account_id, conversation_id)
             VALUES ('a1', 2)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        assert_eq!(matching_ids(r#""alpha phrase" OR report*"#).await, vec![1]);
    }

    #[tokio::test]
    async fn rejects_an_oversized_query() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let huge = "x".repeat(crate::search::lex::MAX_QUERY_BYTES + 1);
        let err = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: &huge,
                limit: 10,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, ApiError::BadRequest(m) if m.contains("longer than")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn export_does_not_leak_other_account_messages() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ('a2', 'bob')")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('a2', '+1777', '+1777', 'phone', 'phone')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let bob_handle: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO conversations (id, account_id, chat_handle_id, conversation_type, source_file)
             VALUES (99, 'a2', $1, 'individual', 'bob.jsonl')",
        )
        .bind(bob_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, account_id, source, service, timestamp, is_from_me, sort_order, body)
             VALUES (99, 99, 'a2', 'sms', 'sms', '2020-02-01T00:00:00Z', 0, 0, 'bob secret')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let alice = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "secret",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert!(alice.items.is_empty(), "alice must not see bob's FTS hits");

        let alice_all = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 100,
                offset: 0,
                today: crate::search::tests::today(),
            },
        )
        .await
        .unwrap();
        assert!(alice_all.items.iter().all(|m| m.id != 99));
    }

    #[tokio::test]
    async fn export_pages_by_offset_and_reports_the_total() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO messages (
                id, conversation_id, account_id, source, service, timestamp,
                is_from_me, sort_order, body
             ) VALUES (
                3, 1, 'a1', 'sms', 'sms', '2020-01-03T00:00:00Z',
                0, 0, 'third'
             )",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = |limit: usize, offset: usize| {
            let pool = pool.clone();
            async move {
                let mut conn = pool.acquire().await.unwrap();
                export_messages(
                    &mut conn,
                    ExportPageOpts {
                        account_id: "a1",
                        query: "",
                        limit,
                        offset,
                        today: crate::search::tests::today(),
                    },
                )
                .await
                .unwrap()
            }
        };

        let first = page(2, 0).await;
        assert_eq!(first.items.len(), 2);
        assert_eq!((first.total, first.limit, first.offset), (3, 2, 0));

        let second = page(2, 2).await;
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].id, 3);
        assert_eq!(second.total, 3);

        // Past the end is an empty page with the true total, not an error.
        let past = page(2, 10).await;
        assert!(past.items.is_empty());
        assert_eq!(past.total, 3);
    }

    #[tokio::test]
    async fn the_export_route_answers_a_page_and_refuses_a_bad_limit() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;

        let page: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/export/messages?q=&limit=10", &user.token)
                .await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        for gone in ["ok", "query", "messages", "next_cursor", "truncated"] {
            assert!(page.get(gone).is_none(), "{gone} must be gone: {page}");
        }

        let status = crate::test_support::get_status(
            &state,
            "/v1/export/messages?q=&limit=501",
            &user.token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

        let count: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/export/messages/count?q=", &user.token)
                .await;
        assert_eq!(count["messages"], 1);
        assert!(count.get("ok").is_none());
        assert!(count.get("query").is_none());
    }

    /// End-to-end placeholder discipline: the assembled export query's `$N`
    /// placeholders must be exactly `1..=params.len()` in bind order.
    #[tokio::test]
    async fn export_sql_placeholders_match_params_order() {
        let (pool, _dir) = setup().await;
        let conn = pool.acquire().await.unwrap();
        let filter = message_filter(
            engine_of(&conn),
            "a1",
            r#"from:alice to:bo subject:hello tag:work ("alpha phrase" OR report*)"#,
            crate::search::tests::today(),
        )
        .unwrap();
        let sql = format!(
            "SELECT m.id {messages_from_sql} WHERE {where_sql}",
            messages_from_sql = messages_from_sql(),
            where_sql = filter.where_sql(),
        );
        let renumbered = renumber_placeholders(&sql);
        assert!(
            !renumbered.contains('?'),
            "no `?` may survive: {renumbered}"
        );
        assert_eq!(renumbered.matches('$').count(), filter.params().len());
        for n in 1..=filter.params().len() {
            assert!(
                renumbered.contains(&format!("${n}")),
                "missing ${n}: {renumbered}"
            );
        }
    }
}
