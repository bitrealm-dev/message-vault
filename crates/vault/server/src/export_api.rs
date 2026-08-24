//! Read-only message export query used by `GET /v1/export/messages`
//! and `GET /v1/export/messages/count`.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use sqlx::{Arguments, Executor, Row};

use crate::db::dialect::{engine_of, like_ci};
use crate::db::engine::DbEngine;
use crate::db::sql::group_rows_by_id;
// Required so the moved handlers' unqualified `export_api::…` paths resolve.
use crate::export_api::{self};
#[cfg(test)]
use crate::search_query::MAX_SEARCH_QUERY_BYTES;
use crate::search_query::{FtsNode, ParsedSearchQuery, SearchMode, validate_search_query};
use crate::server::{
    ApiError, AppState, require_export_access, resolve_auth, resolve_import_account,
};

pub use crate::page_limits::{DEFAULT_EXPORT_LIMIT, MAX_EXPORT_LIMIT, MAX_EXPORT_OFFSET};

/// Options for one exported page of messages.
#[derive(Debug, Clone)]
pub struct ExportPageOpts<'a> {
    /// Vault account to export from.
    pub account_id: &'a str,
    /// Search query string.
    pub query: &'a str,
    /// Max messages on the page.
    pub limit: usize,
    /// Row offset; not combined with `cursor`.
    pub offset: Option<usize>,
    /// Opaque cursor from a previous page.
    pub cursor: Option<&'a str>,
    /// Force a single source (used by the web layer).
    pub source_override: Option<&'a str>,
}

/// Options for one export count query.
#[derive(Debug, Clone)]
pub struct ExportCountOpts<'a> {
    /// Vault account to count from.
    pub account_id: &'a str,
    /// Search query string.
    pub query: &'a str,
    /// Force a single source (used by the web layer).
    pub source_override: Option<&'a str>,
}

/// One page of exported messages.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportMessagesResponse {
    /// Always true when a response is returned.
    pub ok: bool,
    /// Query echoed back.
    pub query: String,
    /// Messages on this page.
    pub messages: Vec<ExportMessage>,
    /// Cursor for the next page; absent on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// True when more rows matched than the page limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

/// Match counts for an export query.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportCountResponse {
    /// Always true when a response is returned.
    pub ok: bool,
    /// Query echoed back.
    pub query: String,
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

/// Export query failure: caller error or server error.
#[derive(Debug)]
pub enum ExportQueryError {
    /// Invalid or unsupported query.
    BadRequest(String),
    /// Query execution failed.
    Internal(String),
}

impl std::fmt::Display for ExportQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "bad request: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for ExportQueryError {}

impl From<anyhow::Error> for ExportQueryError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<sqlx::Error> for ExportQueryError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl ExportQueryError {
    /// Build a [`ExportQueryError::BadRequest`] from a message.
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
}

/// One bound parameter in a dynamic export query. sqlx's Any driver exposes
/// no user-constructible dynamic value, so heterogeneous binds ride this enum.
///
/// `Bool`/`Null` are part of the swap plan's verbatim enum contract and bound
/// by [`bind_all`]; no current filter binds them, so silence the dead-code
/// warning rather than drop plan-mandated variants.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub(crate) enum SqlParam {
    Text(String),
    Int(i64),
    Bool(bool),
    Null,
}

/// Build a query from `sql` with all params bound, in order. Placeholders in
/// the SQL must match this order after `renumber_placeholders`.
///
/// sqlx 0.8.6 does not re-export `Query` at the crate root (the root
/// `sqlx::Query` re-export is 0.9-only), so the plan's literal signature is
/// unnameable; this builds the arguments through the public `Arguments` API
/// instead. `String`/`i64`/`bool`/`None` cannot fail to encode on the Any
/// driver; an encode failure is unreachable and panics like sqlx's own
/// `Query::bind`.
pub(crate) fn bind_all<'q>(
    sql: &'q str,
    params: &[SqlParam],
) -> impl sqlx::Execute<'q, sqlx::Any> + 'q {
    let mut args = sqlx::any::AnyArguments::default();
    for p in params {
        match p {
            SqlParam::Text(v) => args.add(v.clone()),
            SqlParam::Int(v) => args.add(*v),
            SqlParam::Bool(v) => args.add(*v),
            SqlParam::Null => args.add(Option::<String>::None),
        }
        .expect("error encoding argument");
    }
    sqlx::query_with(sql, args)
}

/// Rewrite `?` placeholders to `$1..$N` in order. The Any driver performs no
/// placeholder rewriting and `?` is invalid on Postgres; SQLite accepts `$N`.
/// Valid because no SQL fragment in this crate contains `?` inside a string
/// literal — keep it that way, and unit-test this against the committed
/// fragment set.
pub(crate) fn renumber_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut n = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug, Clone)]
struct PageCursor {
    timestamp: String,
    sort_order: i64,
    id: i64,
}

impl PageCursor {
    fn encode(&self) -> String {
        format!("{}|{}|{}", self.timestamp, self.sort_order, self.id)
    }

    fn decode(raw: &str) -> Option<Self> {
        let mut parts = raw.splitn(3, '|');
        let timestamp = parts.next()?.to_string();
        let sort_order: i64 = parts.next()?.parse().ok()?;
        let id: i64 = parts.next()?.parse().ok()?;
        if timestamp.is_empty() {
            return None;
        }
        Some(Self {
            timestamp,
            sort_order,
            id,
        })
    }
}

struct BuiltFilters {
    where_sql: String,
    params: Vec<SqlParam>,
    dedupe_sql: String,
}

fn unique_ids(ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn nonempty_source(source: Option<&str>) -> Option<&str> {
    let raw = source?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Build WHERE-clause SQL for a validated message-mode search.
async fn prepare_message_export(
    conn: &mut AnyConnection,
    account_id: &str,
    query: &str,
    source_override: Option<&str>,
) -> Result<BuiltFilters, ExportQueryError> {
    let parsed = validate_search_query(query)?;
    if parsed.mode == SearchMode::Contacts {
        return Err(ExportQueryError::bad(
            "contacts search mode is not supported on /v1/export/messages; omit search:contacts",
        ));
    }
    build_message_filters(conn, account_id, &parsed, source_override).await
}

/// Export messages matching a Fastmail-style query (message mode only).
///
/// Empty query (no criteria) returns all non-trashed, non-duplicate messages for the account.
///
/// # Errors
///
/// Returns a bad-request error for an invalid query or cursor, or an internal
/// error when a database statement fails.
pub async fn export_messages(
    conn: &mut AnyConnection,
    opts: ExportPageOpts<'_>,
) -> Result<ExportMessagesResponse, ExportQueryError> {
    let limit = opts.limit.clamp(1, MAX_EXPORT_LIMIT);
    let cursor = match opts.cursor {
        Some(raw) if !raw.trim().is_empty() => Some(
            PageCursor::decode(raw.trim())
                .ok_or_else(|| ExportQueryError::bad("invalid cursor"))?,
        ),
        _ => None,
    };

    let filters =
        prepare_message_export(conn, opts.account_id, opts.query, opts.source_override).await?;
    let fetch_limit = limit + 1;

    let mut sql = format!(
        "SELECT m.id, m.conversation_id, m.source, m.service, m.guid, m.timestamp, m.timestamp_utc,
                m.sort_order, m.is_from_me, hs.raw AS sender, m.subject, m.body,
                m.is_announcement, m.is_reply, m.thread_originator_guid,
                m.thread_originator_part, m.num_replies,
                hc.raw AS chat_identifier, c.conversation_type, c.group_title
         {messages_from_sql}
         WHERE {where_sql}{dedupe}",
        messages_from_sql = messages_from_sql(),
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );

    let mut params = filters.params;
    if let Some(cur) = &cursor {
        sql.push_str(
            " AND (
                m.timestamp > ?
                OR (m.timestamp = ? AND m.sort_order > ?)
                OR (m.timestamp = ? AND m.sort_order = ? AND m.id > ?)
              )",
        );
        params.push(SqlParam::Text(cur.timestamp.clone()));
        params.push(SqlParam::Text(cur.timestamp.clone()));
        params.push(SqlParam::Int(cur.sort_order));
        params.push(SqlParam::Text(cur.timestamp.clone()));
        params.push(SqlParam::Int(cur.sort_order));
        params.push(SqlParam::Int(cur.id));
    }
    sql.push_str(" ORDER BY m.timestamp ASC, m.sort_order ASC, m.id ASC");
    if let (Some(offset), None) = (opts.offset, &cursor) {
        if offset > MAX_EXPORT_OFFSET {
            return Err(ExportQueryError::bad(format!(
                "offset exceeds maximum of {MAX_EXPORT_OFFSET}; use cursor pagination instead"
            )));
        }
        sql.push_str(" LIMIT ? OFFSET ?");
        params.push(SqlParam::Int(fetch_limit as i64));
        params.push(SqlParam::Int(offset as i64));
    } else {
        sql.push_str(" LIMIT ?");
        params.push(SqlParam::Int(fetch_limit as i64));
    }

    let sql = renumber_placeholders(&sql);
    let rows = (&mut *conn).fetch_all(bind_all(&sql, &params)).await?;
    let rows: Vec<RawRow> = rows
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
        .collect::<Result<Vec<RawRow>, ExportQueryError>>()?;

    let truncated = rows.len() > limit;
    let page_rows: Vec<RawRow> = if truncated {
        rows.into_iter().take(limit).collect()
    } else {
        rows
    };

    let next_cursor = if truncated {
        page_rows.last().map(|r| {
            PageCursor {
                timestamp: r.timestamp.clone(),
                sort_order: r.sort_order,
                id: r.id,
            }
            .encode()
        })
    } else {
        None
    };

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

    Ok(ExportMessagesResponse {
        ok: true,
        query: opts.query.to_string(),
        messages,
        next_cursor,
        truncated: truncated.then_some(true),
    })
}

/// Aggregate counts for messages matching a Fastmail-style query (no paging).
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
) -> Result<ExportCountResponse, ExportQueryError> {
    let filters =
        prepare_message_export(conn, opts.account_id, opts.query, opts.source_override).await?;

    let msg_sql = format!(
        "SELECT COUNT(*)
         {messages_from_sql}
         WHERE {where_sql}{dedupe}",
        messages_from_sql = messages_from_sql(),
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );
    let messages: i64 = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&msg_sql), &filters.params))
        .await?
        .try_get(0)?;

    let conv_sql = format!(
        "SELECT COUNT(DISTINCT c.id)
         {messages_from_sql}
         WHERE {where_sql}{dedupe}",
        messages_from_sql = messages_from_sql(),
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );
    let conversations: i64 = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&conv_sql), &filters.params))
        .await?
        .try_get(0)?;

    let att_sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(sz), 0)
         FROM (
           SELECT MAX(a.size_bytes) AS sz
           FROM attachments a
           JOIN messages m ON m.id = a.message_id
           {conversation_join_sql}
           WHERE {where_sql}{dedupe}
             AND a.sha256 IS NOT NULL
             AND length(trim(a.sha256)) > 0
           GROUP BY lower(trim(a.sha256))
         )",
        conversation_join_sql = conversation_join_sql(),
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );
    let row = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&att_sql), &filters.params))
        .await?;
    let (attachments, total_bytes): (i64, i64) = (row.try_get(0)?, row.try_get(1)?);

    Ok(ExportCountResponse {
        ok: true,
        query: opts.query.to_string(),
        messages: messages.max(0) as u64,
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

/// FROM clause for message queries: wires the handles joins the filter SQL
/// references (`hc` = conversation chat handle, `hs` = message sender handle).
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

fn push_participant_handle_or_alias_like(
    where_parts: &mut Vec<String>,
    params: &mut Vec<SqlParam>,
    needle: &str,
) {
    where_parts.push(
        "EXISTS (
                 SELECT 1 FROM participants p
                 JOIN handles ph ON ph.id = p.handle_id
                 WHERE p.conversation_id = c.id
                   AND (ph.raw LIKE ? OR coalesce(p.name_alias, '') LIKE ?)
               )"
        .into(),
    );
    let like = format!("%{needle}%");
    params.push(SqlParam::Text(like.clone()));
    params.push(SqlParam::Text(like));
}

fn reject_unimplemented_message_filters(
    parsed: &ParsedSearchQuery,
) -> Result<(), ExportQueryError> {
    let mut unsupported = Vec::new();
    if parsed.text.is_some() {
        unsupported.push("text:");
    }
    if parsed.filename.is_some() {
        unsupported.push("filename:");
    }
    if parsed.filetype.is_some() {
        unsupported.push("filetype:");
    }
    if parsed.larger_bytes.is_some() {
        unsupported.push("larger:");
    }
    if parsed.smaller_bytes.is_some() {
        unsupported.push("smaller:");
    }
    if parsed.message_count.is_some() {
        unsupported.push("message-count:");
    }
    if parsed.group_count.is_some() {
        unsupported.push("group-count:");
    }
    if parsed.has_attachment == Some(false) {
        unsupported.push("has:noattachment");
    }
    if unsupported.is_empty() {
        return Ok(());
    }
    Err(ExportQueryError::BadRequest(format!(
        "unsupported search operators (not implemented in SQL yet): {}",
        unsupported.join(", ")
    )))
}

async fn build_message_filters(
    conn: &mut AnyConnection,
    account_id: &str,
    parsed: &ParsedSearchQuery,
    source_override: Option<&str>,
) -> Result<BuiltFilters, ExportQueryError> {
    reject_unimplemented_message_filters(parsed)?;
    let engine = engine_of(conn);

    let mut where_parts = vec!["c.account_id = ?".to_string()];
    let mut params: Vec<SqlParam> = vec![SqlParam::Text(account_id.to_string())];

    append_metadata_text_filters(parsed, engine, &mut where_parts, &mut params)?;

    if let Some(conv) = &parsed.in_conversation {
        match conv.parse::<i64>() {
            Ok(id) => {
                where_parts.push("c.id = ?".into());
                params.push(SqlParam::Int(id));
            }
            Err(_) => {
                where_parts.push("hc.raw = ?".into());
                params.push(SqlParam::Text(conv.clone()));
            }
        }
    }

    if let Some(from) = &parsed.from {
        where_parts.push(
            "(m.is_from_me = 0 AND (hs.raw LIKE ? OR EXISTS (
                 SELECT 1 FROM participants p
                 JOIN handles ph ON ph.id = p.handle_id
                 WHERE p.conversation_id = c.id
                   AND (ph.raw LIKE ? OR coalesce(p.name_alias, '') LIKE ?)
               )))"
            .into(),
        );
        let like = format!("%{from}%");
        params.push(SqlParam::Text(like.clone()));
        params.push(SqlParam::Text(like.clone()));
        params.push(SqlParam::Text(like));
    }

    if let Some(to) = &parsed.to {
        push_participant_handle_or_alias_like(&mut where_parts, &mut params, to);
    }
    if let Some(with_person) = &parsed.with_person {
        push_participant_handle_or_alias_like(&mut where_parts, &mut params, with_person);
    }

    if let Some(subject) = &parsed.subject {
        where_parts.push(format!("coalesce(m.subject, '') {}", like_ci(engine)));
        params.push(SqlParam::Text(format!("%{subject}%")));
    }

    if let Some(after) = &parsed.after {
        where_parts.push("m.timestamp >= ?".into());
        params.push(SqlParam::Text(after.clone()));
    }
    if let Some(before) = &parsed.before {
        where_parts.push("m.timestamp < ?".into());
        let before_val = if before.len() == 10 {
            format!("{before}T23:59:59.999Z")
        } else {
            before.clone()
        };
        params.push(SqlParam::Text(before_val));
    }

    let source_filter = nonempty_source(source_override).or(parsed.source.as_deref());
    if let Some(source) = source_filter {
        where_parts.push("m.source = ?".into());
        params.push(SqlParam::Text(source.to_string()));
    }

    if let Some(ct) = parsed.conversation_type {
        where_parts.push("c.conversation_type = ?".into());
        params.push(SqlParam::Text(ct.to_string()));
    }

    if parsed.has_attachment == Some(true) {
        where_parts.push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)".into());
    }

    if let Some(within) = &parsed.within {
        let ids = list_group_member_contact_ids(conn, account_id, within).await?;
        where_parts.push(involves_contacts_sql(&ids));
    }
    if let Some(people) = &parsed.exclude_people {
        let ids = list_group_member_contact_ids(conn, account_id, people).await?;
        where_parts.push(format!("NOT {}", involves_contacts_sql(&ids)));
    }
    if let Some(tag) = &parsed.tag {
        where_parts.push(has_thread_tag_sql(engine, false));
        params.push(SqlParam::Text(tag.clone()));
    }
    if let Some(tag) = &parsed.exclude_tag {
        where_parts.push(has_thread_tag_sql(engine, true));
        params.push(SqlParam::Text(tag.clone()));
    }

    if !parsed.first_contact.is_empty() {
        let ids =
            contact_ids_within_day_bounds(conn, account_id, "first", &parsed.first_contact).await?;
        where_parts.push(involves_contacts_sql(&ids));
    }
    if !parsed.last_contact.is_empty() {
        let ids =
            contact_ids_within_day_bounds(conn, account_id, "last", &parsed.last_contact).await?;
        where_parts.push(involves_contacts_sql(&ids));
    }

    where_parts.push(crate::contacts_api::NOT_TRASHED_CONVERSATION_SQL.into());
    where_parts.push(crate::contacts_api::NOT_TRASHED_CHAT_HANDLE_SQL.into());

    let dedupe_sql = if source_filter.is_some() {
        String::new()
    } else {
        " AND m.duplicate_of IS NULL".to_string()
    };

    Ok(BuiltFilters {
        where_sql: where_parts.join(" AND "),
        params,
        dedupe_sql,
    })
}

fn append_metadata_text_filters(
    parsed: &ParsedSearchQuery,
    engine: DbEngine,
    where_parts: &mut Vec<String>,
    params: &mut Vec<SqlParam>,
) -> Result<(), ExportQueryError> {
    if let Some(ast) = &parsed.fts_ast {
        let mut sql = String::new();
        compile_metadata_fts_expr(ast, engine, &mut sql, params)?;
        where_parts.push(sql);
    }
    Ok(())
}

fn compile_metadata_fts_expr(
    node: &FtsNode,
    engine: DbEngine,
    sql: &mut String,
    params: &mut Vec<SqlParam>,
) -> Result<(), ExportQueryError> {
    match node {
        FtsNode::Term { value, prefix } => {
            push_metadata_like_chain(sql, params, engine, value);
            // Full-text match on the message body index, per engine.
            match engine {
                DbEngine::Sqlite => {
                    // Prefix: `"term"*` (star inside the quoted literal, matching
                    // the current export_api.rs behavior); plain term: `"term"`.
                    let fts_query = if *prefix == Some(true) {
                        format!("{}*", fts5_literal_query(value))
                    } else {
                        fts5_literal_query(value)
                    };
                    sql.push_str(
                        " OR EXISTS (SELECT 1 FROM messages_fts fts WHERE fts.rowid = m.id AND messages_fts MATCH ?",
                    );
                    params.push(SqlParam::Text(fts_query));
                    sql.push_str(")");
                }
                DbEngine::Postgres => {
                    if *prefix == Some(true) {
                        sql.push_str(
                            " OR EXISTS (SELECT 1 FROM messages m_fts WHERE m_fts.id = m.id AND m_fts.search_tsv @@ to_tsquery('simple', ?",
                        );
                        params.push(SqlParam::Text(pg_prefix_tsquery(value)));
                    } else {
                        sql.push_str(
                            " OR EXISTS (SELECT 1 FROM messages m_fts WHERE m_fts.id = m.id AND m_fts.search_tsv @@ plainto_tsquery('simple', ?",
                        );
                        params.push(SqlParam::Text(value.clone()));
                    }
                    sql.push_str("))");
                }
            }
            sql.push_str(")");
            Ok(())
        }
        FtsNode::Phrase { value } => {
            push_metadata_like_chain(sql, params, engine, value);
            match engine {
                DbEngine::Sqlite => {
                    sql.push_str(
                        " OR EXISTS (SELECT 1 FROM messages_fts fts WHERE fts.rowid = m.id AND messages_fts MATCH ?",
                    );
                    params.push(SqlParam::Text(fts5_literal_query(value)));
                    sql.push_str(")");
                }
                DbEngine::Postgres => {
                    sql.push_str(
                        " OR EXISTS (SELECT 1 FROM messages m_fts WHERE m_fts.id = m.id AND m_fts.search_tsv @@ phraseto_tsquery('simple', ?",
                    );
                    params.push(SqlParam::Text(value.clone()));
                    sql.push_str("))");
                }
            }
            sql.push_str(")");
            Ok(())
        }
        FtsNode::And { children } => {
            compile_metadata_fts_children("AND", engine, children, sql, params)
        }
        FtsNode::Or { children } => {
            compile_metadata_fts_children("OR", engine, children, sql, params)
        }
        FtsNode::Not { child } => {
            sql.push_str("(NOT (");
            compile_metadata_fts_expr(child, engine, sql, params)?;
            sql.push_str("))");
            Ok(())
        }
    }
}

fn compile_metadata_fts_children(
    operator: &str,
    engine: DbEngine,
    children: &[FtsNode],
    sql: &mut String,
    params: &mut Vec<SqlParam>,
) -> Result<(), ExportQueryError> {
    if children.is_empty() {
        return Err(ExportQueryError::bad(format!(
            "{operator} search expression has no operands"
        )));
    }
    sql.push_str("(");
    for (i, child) in children.iter().enumerate() {
        if i > 0 {
            sql.push_str(&format!(" {operator} "));
        }
        compile_metadata_fts_expr(child, engine, sql, params)?;
    }
    sql.push_str(")");
    Ok(())
}

/// The LIKE-based metadata chain shared by Term and Phrase leaves: handles,
/// participant aliases, contacts, and attachment names, all `%term%`
/// case-insensitive. Pushes 8 binds (one per LIKE clause); the caller then
/// pushes the full-text match and closes the outer parenthesis.
fn push_metadata_like_chain(
    sql: &mut String,
    params: &mut Vec<SqlParam>,
    engine: DbEngine,
    term: &str,
) {
    let pattern = format!("%{term}%");
    sql.push_str("(coalesce(hs.raw, '') ");
    sql.push_str(like_ci(engine));
    sql.push_str(
        " OR EXISTS (SELECT 1 FROM participants p_md JOIN handles hp ON hp.id = p_md.handle_id WHERE p_md.conversation_id = c.id AND (hp.raw ",
    );
    sql.push_str(like_ci(engine));
    sql.push_str(" OR coalesce(p_md.name_alias, '') ");
    sql.push_str(like_ci(engine));
    sql.push_str(
        ")) OR EXISTS (SELECT 1 FROM contact_handles ch_md JOIN contacts ct_md ON ct_md.id = ch_md.contact_id JOIN handles hm ON hm.id = ch_md.handle_id WHERE ch_md.account_id = c.account_id AND (hm.raw ",
    );
    sql.push_str(like_ci(engine));
    sql.push_str(" OR coalesce(ct_md.preferred_name, '') ");
    sql.push_str(like_ci(engine));
    sql.push_str(
        ") AND ((c.conversation_type = 'individual' AND hm.id = c.chat_handle_id) OR EXISTS (SELECT 1 FROM participants p_md2 WHERE p_md2.conversation_id = c.id AND p_md2.handle_id = ch_md.handle_id))) OR EXISTS (SELECT 1 FROM attachments a_md WHERE a_md.message_id = m.id AND (coalesce(a_md.original_name, '') ",
    );
    sql.push_str(like_ci(engine));
    sql.push_str(" OR coalesce(a_md.mime_type, '') ");
    sql.push_str(like_ci(engine));
    sql.push_str(" OR coalesce(a_md.derived_mime_type, '') ");
    sql.push_str(like_ci(engine));
    sql.push_str("))");
    for _ in 0..8 {
        params.push(SqlParam::Text(pattern.clone()));
    }
}

/// Quote a free-text token for full-text search so operators and punctuation are treated as literal text.
fn fts5_literal_query(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Quote a term for a Postgres prefix query under the 'simple' config:
/// `'term':*`. Single quotes are stripped (FTS5 treats them as literal text,
/// 'simple' cannot carry them either).
fn pg_prefix_tsquery(term: &str) -> String {
    format!("'{}':*", term.replace('\'', ""))
}

/// Case-insensitive equality fragment on the aliased `name` column (`?`
/// placeholder form; the renumber pass rewrites it). SQLite uses COLLATE
/// NOCASE; Postgres lower()s both sides with the alias INSIDE `lower()` —
/// `ct.lower(...)` would parse as a schema-qualified function call.
fn name_eq_ci(engine: DbEngine, alias: &str) -> String {
    match engine {
        DbEngine::Sqlite => format!("{alias}.name = ? COLLATE NOCASE"),
        DbEngine::Postgres => format!("lower({alias}.name) = lower(?)"),
    }
}

/// Case-insensitive equality on the aliased `name` column with a hand-numbered
/// placeholder.
fn name_eq_sql(engine: DbEngine, alias: &str, placeholder: usize) -> String {
    match engine {
        DbEngine::Sqlite => format!("{alias}.name = ${placeholder} COLLATE NOCASE"),
        DbEngine::Postgres => format!("lower({alias}.name) = lower(${placeholder})"),
    }
}

fn has_thread_tag_sql(engine: DbEngine, exclude: bool) -> String {
    let exists = if exclude { "NOT EXISTS" } else { "EXISTS" };
    format!(
        "{exists} (
           SELECT 1 FROM conversation_tag_members ctm
           JOIN conversation_tags ct ON ct.id = ctm.tag_id
           WHERE ctm.conversation_id = c.id
             AND ct.account_id = c.account_id
             AND {name_eq}
         )",
        name_eq = name_eq_ci(engine, "ct"),
    )
}

fn involves_contacts_sql(contact_ids: &[i64]) -> String {
    if contact_ids.is_empty() {
        return "1=0".into();
    }
    let mut id_list = String::new();
    for (i, id) in contact_ids.iter().enumerate() {
        if i > 0 {
            id_list.push(',');
        }
        id_list.push_str(&id.to_string());
    }
    format!(
        "EXISTS (
    SELECT 1 FROM contact_handles ch
    WHERE ch.account_id = c.account_id
      AND ch.contact_id IN ({id_list})
      AND (
        ch.handle_id = c.chat_handle_id
        OR EXISTS (
          SELECT 1 FROM participants p_link
          WHERE p_link.conversation_id = c.id AND p_link.handle_id = ch.handle_id
        )
      )
  )"
    )
}

async fn list_group_member_contact_ids(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, ExportQueryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let engine = engine_of(conn);
    let sql = format!(
        "SELECT cgm.contact_id
             FROM contact_group_members cgm
             JOIN contact_groups cg ON cg.id = cgm.group_id
             WHERE {name_eq} AND cg.account_id = $2
             ORDER BY cgm.contact_id",
        name_eq = name_eq_sql(engine, "cg", 1),
    );
    let rows = sqlx::query(&sql)
        .bind(trimmed)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter()
        .map(|r| r.try_get::<i64, _>(0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

async fn contact_ids_within_day_bounds(
    conn: &mut AnyConnection,
    account_id: &str,
    bound: &str,
    bounds: &crate::search_query::DateBounds,
) -> Result<Vec<i64>, ExportQueryError> {
    let day = if bound == "first" { "MIN" } else { "MAX" };
    let mut having = Vec::new();
    let mut params: Vec<SqlParam> = vec![SqlParam::Text(account_id.to_string())];
    let mut n = 1;
    if let Some(from) = &bounds.from {
        n += 1;
        having.push(format!("{day}(substr(m.timestamp, 1, 10)) >= ${n}"));
        params.push(SqlParam::Text(from.clone()));
    }
    if let Some(to) = &bounds.to {
        n += 1;
        having.push(format!("{day}(substr(m.timestamp, 1, 10)) < ${n}"));
        params.push(SqlParam::Text(to.clone()));
    }
    if having.is_empty() {
        return Ok(Vec::new());
    }
    let having_sql = having.join(" AND ");
    let sql = format!(
        "SELECT ch.contact_id
         FROM contact_handles ch
         JOIN conversations c
           ON c.account_id = ch.account_id
          AND c.conversation_type = 'individual'
          AND c.chat_handle_id = ch.handle_id
         JOIN messages m ON m.conversation_id = c.id
         WHERE ch.account_id = $1 AND m.duplicate_of IS NULL
         GROUP BY ch.contact_id
         HAVING {having_sql}"
    );
    let rows = (&mut *conn).fetch_all(bind_all(&sql, &params)).await?;
    rows.iter()
        .map(|r| r.try_get::<i64, _>(0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

async fn load_participants(
    conn: &mut AnyConnection,
    conversation_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<ExportParticipant>>, ExportQueryError> {
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
) -> Result<std::collections::HashMap<i64, Vec<ExportAttachment>>, ExportQueryError> {
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
) -> Result<std::collections::HashMap<i64, Vec<ExportTapback>>, ExportQueryError> {
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
    pub(crate) cursor: Option<String>,
    #[serde(default)]
    pub(crate) account: Option<String>,
    #[serde(default)]
    pub(crate) source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExportMessagesCountQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// Count messages, conversations, and attachment fingerprints matching a
/// query.
#[utoipa::path(
    get,
    path = "/v1/export/messages/count",
    tag = "Export",
    security(("bearer" = [])),
    params(
        ("q" = String, Query, description = "Metadata search subset; empty is all non-trashed"),
        ("account" = Option<String>, Query),
        ("source" = Option<String>, Query)
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
    headers: HeaderMap,
    Query(query): Query<ExportMessagesCountQuery>,
) -> Result<Json<export_api::ExportCountResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_export_access(&auth)?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    let q = query.q.clone();
    let source = query.source.clone();

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let body = export_api::export_message_count(
        &mut conn,
        ExportCountOpts {
            account_id: &account,
            query: &q,
            source_override: source.as_deref(),
        },
    )
    .await?;
    Ok(Json(body))
}

/// Export messages matching a search query (message mode; cursor paging).
#[utoipa::path(
    get,
    path = "/v1/export/messages",
    tag = "Export",
    security(("bearer" = [])),
    params(
        ("q" = String, Query, description = "Metadata search subset; empty is all non-trashed"),
        ("limit" = Option<usize>, Query, description = "Page size, default 100, max 500"),
        ("offset" = Option<usize>, Query, description = "Legacy offset; prefer cursor"),
        ("cursor" = Option<String>, Query, description = "Opaque next_cursor from a previous page"),
        ("account" = Option<String>, Query),
        ("source" = Option<String>, Query)
    ),
    responses(
        (status = 200, body = crate::export_api::ExportMessagesResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn export_messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportMessagesQuery>,
) -> Result<Json<export_api::ExportMessagesResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_export_access(&auth)?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    let limit = query.limit.unwrap_or(DEFAULT_EXPORT_LIMIT);
    let offset = query.offset;
    let q = query.q.clone();
    let cursor = query.cursor.clone();
    let source = query.source.clone();

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let body = export_api::export_messages(
        &mut conn,
        ExportPageOpts {
            account_id: &account,
            query: &q,
            limit,
            offset,
            cursor: cursor.as_deref(),
            source_override: source.as_deref(),
        },
    )
    .await?;
    Ok(Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{engine, schema};

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username, read_only) VALUES ($1, 'alice', 0)")
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
                query: "in:1",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].attachments.len(), 1);
        let att = &res.messages[0].attachments[0];
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
                query: "in:1",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 1);
        assert_eq!(res.messages[0].service.as_deref(), Some("sms"));

        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "conversation:2",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 2);

        let res = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(res.messages.len(), 2);
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
                    source_override: None,
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
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 1);
        assert!(
            res.messages[0]
                .text
                .as_deref()
                .unwrap_or("")
                .contains("one")
        );
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
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        let ids: Vec<i64> = result.messages.iter().map(|message| message.id).collect();
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
                        offset: None,
                        cursor: None,
                        source_override: None,
                    },
                )
                .await
                .unwrap()
                .messages
                .into_iter()
                .map(|message| message.id)
                .collect::<Vec<_>>()
            }
        };
        assert_eq!(matching_ids("foo AND bar").await, vec![3]);
        assert_eq!(matching_ids("foo AND NOT bar").await, vec![1]);
    }

    #[tokio::test]
    async fn export_boolean_query_combines_metadata_body_phrases_prefixes_and_nesting() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('a1', 'blocked', 'blocked', 'other', 'other')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let blocked_sender: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE messages SET body = 'alpha phrase', sender_handle_id = $1 WHERE id = 1",
        )
        .bind(blocked_sender)
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
                        offset: None,
                        cursor: None,
                        source_override: None,
                    },
                )
                .await
                .unwrap()
                .messages
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
            matching_ids(r#"blocked AND ("alpha phrase" OR report*)"#).await,
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
    async fn rejects_oversized_search_query_and_offset() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let huge = "x".repeat(MAX_SEARCH_QUERY_BYTES + 1);
        let err = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: &huge,
                limit: 10,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("exceeds"), "{err}");

        let err = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 10,
                offset: Some(MAX_EXPORT_OFFSET + 1),
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("offset"), "{err}");
    }

    #[tokio::test]
    async fn export_does_not_leak_other_account_messages() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username, read_only) VALUES ('a2', 'bob', 0)")
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
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert!(
            alice.messages.is_empty(),
            "alice must not see bob's FTS hits"
        );

        let alice_all = export_messages(
            &mut conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .await
        .unwrap();
        assert!(alice_all.messages.iter().all(|m| m.id != 99));
    }

    /// Cursor pagination end to end: the 6 cursor binds (`timestamp`, then the
    /// `timestamp|sort_order|id` triple twice) must align with the assembled
    /// SQL after renumbering, and `LIMIT ?` follows them.
    #[tokio::test]
    async fn export_pages_with_cursor_and_limit() {
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

        // Owned cursor so the async closure borrows nothing borrowed by the
        // caller (a `&str` capture would need a lifetime the closure cannot name).
        let page = |limit: usize, cursor: Option<String>| {
            let pool = pool.clone();
            async move {
                let mut conn = pool.acquire().await.unwrap();
                export_messages(
                    &mut conn,
                    ExportPageOpts {
                        account_id: "a1",
                        query: "",
                        limit,
                        offset: None,
                        cursor: cursor.as_deref(),
                        source_override: None,
                    },
                )
                .await
                .unwrap()
            }
        };

        let first = page(2, None).await;
        assert_eq!(first.messages.len(), 2);
        assert!(first.truncated == Some(true));
        let cursor = first.next_cursor.expect("first page must carry a cursor");

        let second = page(2, Some(cursor)).await;
        assert_eq!(second.messages.len(), 1);
        assert_eq!(second.messages[0].id, 3);
        assert_eq!(second.truncated, None);
        assert!(second.next_cursor.is_none());
    }

    #[test]
    fn renumber_placeholders_numbers_in_order() {
        let sql = "a = ? AND b = ? AND c IN (?, ?) AND d LIKE ?";
        assert_eq!(
            renumber_placeholders(sql),
            "a = $1 AND b = $2 AND c IN ($3, $4) AND d LIKE $5"
        );
        assert_eq!(renumber_placeholders("no placeholders"), "no placeholders");
    }

    /// The compiled metadata search for a mixed Term/Phrase/And/Or/Not query
    /// must place exactly one `?` per pushed bind, in push order: each leaf
    /// pushes 8 LIKE patterns (`%term%`) then its full-text bind.
    #[test]
    fn compiled_fts_placeholders_match_bind_order() {
        let parsed =
            validate_search_query(r#"blocked AND ("alpha phrase" OR report*) AND NOT spam"#)
                .unwrap();
        let ast = parsed.fts_ast.as_ref().unwrap();
        let mut sql = String::new();
        let mut params = Vec::new();
        compile_metadata_fts_expr(ast, DbEngine::Sqlite, &mut sql, &mut params).unwrap();

        assert_eq!(params.len(), 36, "4 leaves × 9 binds each");
        let renumbered = renumber_placeholders(&sql);
        assert!(
            !renumbered.contains('?'),
            "no `?` may survive: {renumbered}"
        );
        assert_eq!(renumbered.matches('$').count(), params.len());

        // Bind order: leaf 1 (blocked) 8 LIKE patterns, then its FTS bind;
        // leaf 2 (alpha phrase) 8 + FTS; leaf 3 (report*) 8 + FTS; leaf 4 (spam).
        for i in 0..8 {
            assert_eq!(params[i], SqlParam::Text("%blocked%".into()));
        }
        assert_eq!(params[8], SqlParam::Text("\"blocked\"".into()));
        for i in 9..17 {
            assert_eq!(params[i], SqlParam::Text("%alpha phrase%".into()));
        }
        assert_eq!(params[17], SqlParam::Text("\"alpha phrase\"".into()));
        for i in 18..26 {
            assert_eq!(params[i], SqlParam::Text("%report%".into()));
        }
        assert_eq!(params[26], SqlParam::Text("\"report\"*".into()));
        for i in 27..35 {
            assert_eq!(params[i], SqlParam::Text("%spam%".into()));
        }
        assert_eq!(params[35], SqlParam::Text("\"spam\"".into()));

        // The renumbered SQL numbers the same order: `$1..$9` for leaf 1, etc.
        for n in 1..=36 {
            assert!(
                renumbered.contains(&format!("${n}")),
                "missing ${n}: {renumbered}"
            );
        }
    }

    /// The Postgres branch emits the tsquery calls; prefix terms become
    /// `to_tsquery` with a `'term':*` operand, phrases `phraseto_tsquery`.
    #[test]
    fn fts_compiler_emits_postgres_branch() {
        let parsed = validate_search_query(r#"report* OR "alpha phrase""#).unwrap();
        let ast = parsed.fts_ast.as_ref().unwrap();
        let mut sql = String::new();
        let mut params = Vec::new();
        compile_metadata_fts_expr(ast, DbEngine::Postgres, &mut sql, &mut params).unwrap();

        assert!(
            sql.contains("search_tsv @@ to_tsquery('simple', ?)"),
            "{sql}"
        );
        assert!(
            sql.contains("search_tsv @@ phraseto_tsquery('simple', ?)"),
            "{sql}"
        );
        assert!(!sql.contains("messages_fts"), "{sql}");
        assert!(sql.contains("ILIKE ?"), "{sql}");
        assert!(!sql.contains("COLLATE NOCASE"), "{sql}");
        assert_eq!(params.len(), 18);
        assert_eq!(params[8], SqlParam::Text("'report':*".into()));
        assert_eq!(params[17], SqlParam::Text("alpha phrase".into()));
        let renumbered = renumber_placeholders(&sql);
        assert!(!renumbered.contains('?'));
        assert_eq!(renumbered.matches('$').count(), params.len());
    }

    /// The case-insensitive equality fragments must keep the table alias
    /// INSIDE `lower()` on Postgres — `ct.lower(...)` parses as a
    /// schema-qualified function call — at both call sites: the thread-tag
    /// subquery (alias `ct`) and the contact-group lookup (alias `cg`). The
    /// SQLite arms stay alias-outside COLLATE NOCASE, unchanged.
    #[test]
    fn pg_ci_eq_keeps_alias_inside_lower() {
        let tag_sql = has_thread_tag_sql(DbEngine::Postgres, false);
        assert!(tag_sql.contains("lower(ct.name) = lower(?)"), "{tag_sql}");
        assert!(!tag_sql.contains("ct.lower("), "{tag_sql}");

        let group_eq = name_eq_sql(DbEngine::Postgres, "cg", 1);
        assert_eq!(group_eq, "lower(cg.name) = lower($1)");
        assert!(!group_eq.contains("cg.lower("));

        // SQLite arms are byte-identical in behavior to the pre-port form.
        let tag_sqlite = has_thread_tag_sql(DbEngine::Sqlite, false);
        assert!(
            tag_sqlite.contains("ct.name = ? COLLATE NOCASE"),
            "{tag_sqlite}"
        );
        assert_eq!(
            name_eq_sql(DbEngine::Sqlite, "cg", 1),
            "cg.name = $1 COLLATE NOCASE"
        );
    }

    /// End-to-end placeholder discipline: the assembled export query's `$N`
    /// placeholders must be exactly `1..=params.len()` in bind order.
    #[tokio::test]
    async fn export_sql_placeholders_match_params_order() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let parsed = validate_search_query(
            r#"from:alice to:bob subject:hello tag:work "alpha phrase" OR report*"#,
        )
        .unwrap();
        let filters = build_message_filters(&mut conn, "a1", &parsed, None)
            .await
            .unwrap();
        let sql = format!(
            "SELECT m.id {messages_from_sql} WHERE {where_sql}{dedupe}",
            messages_from_sql = messages_from_sql(),
            where_sql = filters.where_sql,
            dedupe = filters.dedupe_sql,
        );
        let renumbered = renumber_placeholders(&sql);
        assert!(
            !renumbered.contains('?'),
            "no `?` may survive: {renumbered}"
        );
        assert_eq!(renumbered.matches('$').count(), filters.params.len());
        for n in 1..=filters.params.len() {
            assert!(
                renumbered.contains(&format!("${n}")),
                "missing ${n}: {renumbered}"
            );
        }
        // account 1, `from:alice` 3, `to:bob` 2, `subject:hello` 1,
        // `tag:work` 1, then the two FTS leaves at 9 binds each = 26.
        assert_eq!(filters.params.len(), 26);
    }
}
