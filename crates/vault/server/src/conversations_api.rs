//! Read-only conversation list used by `GET /v1/conversations`.

use std::collections::{HashMap, HashSet};

use crate::extract::{Json, Path as AxumPath, Query};
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;

use crate::db::conversation_messages::{Message, load_messages};
use crate::db::dialect::engine_of;
use crate::db::participant_names::{Participant, load_for_chat_handle, load_for_conversations};
use crate::db::sql::{
    SqlParam, bind_args, fold_in_id_chunks, in_placeholders, renumber_placeholders,
};
use crate::db::trash::{restore_conversation, trash_conversation};
use crate::paging::{DEFAULT_LIST_LIMIT, MAX_LIST_OFFSET, Page, page_params};
use crate::server::{ApiError, AppState, FullAccess};

/// Column the conversation list is ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversationSort {
    /// Timestamp of the most recent non-duplicate message in the thread.
    #[default]
    Date,
    /// Number of non-duplicate messages in the thread.
    Messages,
}

impl ConversationSort {
    /// Read a `sort=` value, falling back to the default.
    ///
    /// Deliberately lenient: before this parameter existed an unrecognised
    /// query parameter was ignored, and a stale bookmark or a third-party
    /// client sending `sort=` or `sort=oldest` should still get a conversation
    /// list rather than a 400 for the whole request.
    fn from_param(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "messages" => Self::Messages,
            _ => Self::Date,
        }
    }
}

/// Ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

impl SortOrder {
    /// Read an `order=` value, falling back to the default. Lenient for the
    /// same reason as [`ConversationSort::from_param`].
    fn from_param(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "asc" => Self::Asc,
            _ => Self::Desc,
        }
    }
}

/// How to order a conversation page. Defaults to newest activity first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConversationOrder {
    pub sort: ConversationSort,
    pub order: SortOrder,
}

impl ConversationOrder {
    /// The `ORDER BY` body for this ordering.
    ///
    /// Every arm is a fixed literal chosen by matching on an enum, so no part
    /// of the request reaches the SQL text. Both columns are output aliases of
    /// the page query, which SQLite and Postgres each allow in `ORDER BY`.
    /// `c.id` breaks ties so paging cannot repeat or skip a row.
    ///
    /// `last_message_at` is NULL for a thread whose every message is a
    /// duplicate, and the two engines disagree about where NULLs belong:
    /// SQLite sorts them lowest, while Postgres defaults to NULLS LAST when
    /// ascending and NULLS FIRST when descending. Leading with
    /// `(last_message_at IS NULL)` — false before true on both — pins those
    /// threads to the end in either direction and keeps the two engines
    /// agreeing.
    fn order_by_sql(self) -> &'static str {
        match (self.sort, self.order) {
            (ConversationSort::Date, SortOrder::Desc) => {
                "(last_message_at IS NULL) ASC, last_message_at DESC, c.id DESC"
            }
            (ConversationSort::Date, SortOrder::Asc) => {
                "(last_message_at IS NULL) ASC, last_message_at ASC, c.id ASC"
            }
            (ConversationSort::Messages, SortOrder::Desc) => "message_count DESC, c.id DESC",
            (ConversationSort::Messages, SortOrder::Asc) => "message_count ASC, c.id ASC",
        }
    }
}

/// Conversation row for the list: participants, counts, tags.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConversationSummary {
    /// The conversation's id; search for it as `in:#<id>`.
    pub id: i64,
    /// Participants with names and handles.
    pub participants: Vec<Participant>,
    /// Messages in the conversation (excluding hidden duplicates).
    pub message_count: u64,
    /// Timestamp of the last message.
    pub last_message_at: String,
    /// Timestamp of the conversation's first message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_start: Option<String>,
    /// Timestamp of the conversation's last message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_end: Option<String>,
    /// Platform service of the conversation, e.g. `imessage`.
    pub service: String,
    /// True for group conversations.
    pub is_group: bool,
    /// Group label from the export, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Message tags on this conversation.
    pub tags: Vec<String>,
}

struct RawConversation {
    id: i64,
    conversation_type: String,
    group_title: Option<String>,
    message_count: i64,
    last_message_at: Option<String>,
    date_range_start: Option<String>,
    date_range_end: Option<String>,
}

type RawConversationRow = (
    i64,
    String,
    Option<String>,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// One page of the conversation list for `q`, a query in the search language.
///
/// # Errors
///
/// `BadRequest` for a query the language refuses; `Internal` when a
/// statement fails.
pub async fn list_conversations_sorted(
    conn: &mut AnyConnection,
    account_id: &str,
    q: &str,
    order: ConversationOrder,
    limit: usize,
    offset: usize,
    today: chrono::NaiveDate,
) -> Result<Page<ConversationSummary>, ApiError> {
    let engine = engine_of(conn);
    let filter = crate::search::compile(crate::search::CompileRequest {
        list: crate::search::ListKind::Conversations,
        query: q,
        account_id,
        engine,
        today,
    })?;
    let where_sql = filter.where_sql();

    let count_sql = renumber_placeholders(&format!(
        "SELECT COUNT(*) FROM conversations c WHERE {where_sql}"
    ));
    let total: i64 = sqlx::query_scalar_with(&count_sql, bind_args(filter.params()))
        .fetch_one(&mut *conn)
        .await?;
    let total = total.max(0) as u64;

    let mut params = filter.params().to_vec();
    params.push(SqlParam::Int(limit as i64));
    params.push(SqlParam::Int(offset as i64));
    let sql = renumber_placeholders(&format!(
        "{select} WHERE {where_sql} ORDER BY {order_by} LIMIT ? OFFSET ?",
        select = CONVERSATION_ROW_SELECT,
        order_by = order.order_by_sql(),
    ));
    let out = load_conversation_rows(conn, account_id, &sql, &params).await?;
    Ok(Page {
        items: out,
        total,
        limit,
        offset,
    })
}

/// One conversation by id, scoped to `account_id`. `None` when the id does
/// not exist or belongs to another account — the two cases look identical to
/// the caller, which is what keeps this a 404 rather than a 403.
///
/// # Errors
///
/// `Internal` when a statement fails.
pub async fn get_conversation_summary(
    conn: &mut AnyConnection,
    account_id: &str,
    conversation_id: i64,
) -> Result<Option<ConversationSummary>, ApiError> {
    let sql = renumber_placeholders(&format!(
        "{CONVERSATION_ROW_SELECT} WHERE c.id = ? AND c.account_id = ?"
    ));
    let params = [
        SqlParam::Int(conversation_id),
        SqlParam::Text(account_id.to_string()),
    ];
    let out = load_conversation_rows(conn, account_id, &sql, &params).await?;
    Ok(out.into_iter().next())
}

/// The row shape shared by the conversation list and the single-conversation
/// read: id, type, title, and the counts/timestamps computed from `messages`.
/// Callers append their own `WHERE`, `ORDER BY`, and paging.
const CONVERSATION_ROW_SELECT: &str = "SELECT c.id,
                c.conversation_type,
                c.group_title,
                (SELECT COUNT(*) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS message_count,
                (SELECT MAX(m.timestamp) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS last_message_at,
                (SELECT MIN(m.timestamp) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS date_range_start,
                (SELECT MAX(m.timestamp) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS date_range_end
         FROM conversations c";

/// Run a `CONVERSATION_ROW_SELECT`-shaped query and assemble
/// [`ConversationSummary`] rows: participants, sources, and tags, exactly as
/// the list builds them. Shared so the list and the single-conversation read
/// cannot drift into two different notions of what a conversation summary is.
async fn load_conversation_rows(
    conn: &mut AnyConnection,
    account_id: &str,
    sql: &str,
    params: &[SqlParam],
) -> Result<Vec<ConversationSummary>, ApiError> {
    let rows: Vec<RawConversationRow> = sqlx::query_as_with(sql, bind_args(params))
        .fetch_all(&mut *conn)
        .await?;
    let rows: Vec<RawConversation> = rows
        .into_iter()
        .map(
            |(
                id,
                conversation_type,
                group_title,
                message_count,
                last_message_at,
                date_range_start,
                date_range_end,
            )| RawConversation {
                id,
                conversation_type,
                group_title,
                message_count,
                last_message_at,
                date_range_start,
                date_range_end,
            },
        )
        .collect();

    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut participants = load_for_conversations(conn, &ids).await?;
    let source_sets = load_conversation_sources(conn, &ids).await?;
    let mut tag_sets = crate::named_membership::names_for_items(
        crate::named_membership::tag_spec(),
        conn,
        account_id,
        &ids,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let last = row
            .last_message_at
            .clone()
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
        let is_group = row.conversation_type.eq_ignore_ascii_case("group");
        let service = display_service_label(
            source_sets
                .get(&row.id)
                .map(|s| s.as_slice())
                .unwrap_or(&[]),
        );
        let parts = participants.remove(&row.id).unwrap_or_default();
        let parts = if parts.is_empty() {
            load_for_chat_handle(conn, row.id).await?
        } else {
            parts
        };
        out.push(ConversationSummary {
            id: row.id,
            participants: parts,
            message_count: row.message_count.max(0) as u64,
            last_message_at: last,
            date_range_start: row.date_range_start,
            date_range_end: row.date_range_end,
            service,
            is_group,
            label: row
                .group_title
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            tags: tag_sets.remove(&row.id).unwrap_or_default(),
        });
    }
    Ok(out)
}

const IMESSAGE_SOURCE: &str = "imessage";
const SBR_SOURCE: &str = "sms-backup-restore";
const WHATSAPP_SOURCE: &str = "whatsapp";

/// Header label from distinct message sources in a conversation.
pub fn display_service_label(sources: &[String]) -> String {
    let set: HashSet<&str> = sources.iter().map(|s| s.as_str()).collect();
    if set.contains(SBR_SOURCE) {
        return "SMS/MMS".into();
    }
    if set.len() == 1 && set.contains(IMESSAGE_SOURCE) {
        return IMESSAGE_SOURCE.into();
    }
    if set.len() == 1 && set.contains(WHATSAPP_SOURCE) {
        return "WhatsApp".into();
    }
    if set.len() == 1 {
        return sources[0].trim().to_string();
    }
    "unknown".into()
}

async fn load_conversation_sources(
    conn: &mut AnyConnection,
    conversation_ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>, ApiError> {
    fold_in_id_chunks(conn, conversation_ids, |conn, chunk| {
        Box::pin(async move {
            let placeholders = in_placeholders(1, chunk.len());
            let sql = format!(
                "SELECT conversation_id, source
                 FROM messages
                 WHERE duplicate_of IS NULL
                   AND conversation_id IN ({placeholders})
                 GROUP BY conversation_id, source
                 ORDER BY conversation_id, source"
            );
            let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
            for id in chunk {
                q = q.bind(*id);
            }
            let rows = q.fetch_all(&mut *conn).await?;
            let mut out = Vec::new();
            for (cid, source) in rows {
                if source.trim().is_empty() {
                    continue;
                }
                out.push((cid, source));
            }
            Ok(out)
        })
    })
    .await
}

/// One backup source with message counts and share.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConversationSourceInfo {
    /// Backup source name.
    pub backup_name: String,
    /// Messages in this conversation from this source.
    pub message_count: u64,
    /// Messages only this source has (not hidden duplicates).
    pub unique_count: u64,
    /// Share of the conversation's unique messages, 0–100.
    pub percentage: f64,
}

/// Per-source counts for one conversation.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConversationSourcesPage {
    /// One entry per source that contributed messages.
    pub items: Vec<ConversationSourceInfo>,
}

/// Per-source message counts for the Sources panel.
///
/// # Errors
///
/// Returns an internal error when a database statement fails.
pub async fn list_conversation_source_stats(
    conn: &mut AnyConnection,
    account_id: &str,
    conversation_id: i64,
) -> Result<Option<ConversationSourcesPage>, ApiError> {
    let owned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = $1 AND account_id = $2")
            .bind(conversation_id)
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await?;
    if owned == 0 {
        return Ok(None);
    }

    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT source,
                    COUNT(*) AS message_count,
                    SUM(CASE WHEN duplicate_of IS NULL THEN 1 ELSE 0 END) AS unique_count
             FROM messages
             WHERE conversation_id = $1
             GROUP BY source
             ORDER BY source",
    )
    .bind(conversation_id)
    .fetch_all(&mut *conn)
    .await?;

    let total_unique: i64 = rows.iter().map(|(_, _, u)| *u).sum();
    let sources = rows
        .into_iter()
        .map(|(source, message_count, unique_count)| {
            let percentage = if total_unique > 0 {
                (unique_count as f64) * 100.0 / (total_unique as f64)
            } else {
                0.0
            };
            ConversationSourceInfo {
                backup_name: source,
                message_count: message_count.max(0) as u64,
                unique_count: unique_count.max(0) as u64,
                percentage: (percentage * 10.0).round() / 10.0,
            }
        })
        .collect();
    Ok(Some(ConversationSourcesPage { items: sources }))
}

/// The `WHERE` a conversation's message page and its `total` share: the
/// conversation itself, the account scope, Export's not-duplicate filter
/// (`ListKind::Messages`'s default in `search::emit::compile`), and — when
/// `year` is given — the same calendar-year bounds `date:YYYY` matches in
/// the search language, computed by the same
/// [`crate::search::value::parse_date_span`] the search compiler calls, so a
/// day cannot fall inside the year for one and outside it for the other.
/// Trash plays no part here: reading one conversation's messages is not
/// gated by trash, the same rule [`get_conversation_summary`] follows for
/// the conversation itself.
///
/// # Errors
///
/// `BadRequest` when `year` is not a four-digit year.
fn conversation_messages_where(
    conversation_id: i64,
    account_id: &str,
    year: Option<i32>,
) -> Result<(String, Vec<SqlParam>), ApiError> {
    let mut sql =
        "m.conversation_id = ? AND m.account_id = ? AND m.duplicate_of IS NULL".to_string();
    let mut params = vec![
        SqlParam::Int(conversation_id),
        SqlParam::Text(account_id.to_string()),
    ];
    if let Some(year) = year {
        // `today` only matters to the relative-span forms (`7d`, `1y`, …)
        // `parse_date_span` also understands; a bare `YYYY` ignores it.
        let today = chrono::Local::now().date_naive();
        let span = crate::search::value::parse_date_span(&year.to_string(), today)
            .ok_or_else(|| ApiError::BadRequest("year must be a four-digit year".into()))?;
        sql.push_str(" AND m.timestamp >= ? AND m.timestamp < ?");
        params.push(SqlParam::Text(crate::search::value::ymd(span.start)));
        params.push(SqlParam::Text(crate::search::value::ymd(span.end)));
    }
    Ok((sql, params))
}

/// One page of a conversation's messages, ascending by timestamp then
/// `sort_order`. `None` when the conversation does not exist or belongs to
/// another account — checked before the message query runs, so an unknown id
/// and another account's conversation id are indistinguishable from the
/// outside, the same guarantee [`get_conversation_summary`] gives.
///
/// # Errors
///
/// `BadRequest` when `year` is not a four-digit year; `Internal` when a
/// statement fails.
pub async fn get_conversation_messages(
    conn: &mut AnyConnection,
    account_id: &str,
    conversation_id: i64,
    year: Option<i32>,
    limit: usize,
    offset: usize,
) -> Result<Option<Page<Message>>, ApiError> {
    let owned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = $1 AND account_id = $2")
            .bind(conversation_id)
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await?;
    if owned == 0 {
        return Ok(None);
    }

    let (where_sql, params) = conversation_messages_where(conversation_id, account_id, year)?;

    let count_sql = renumber_placeholders(&format!(
        "SELECT COUNT(*) FROM messages m WHERE {where_sql}"
    ));
    let total: i64 = sqlx::query_scalar_with(&count_sql, bind_args(&params))
        .fetch_one(&mut *conn)
        .await?;
    let total = total.max(0) as u64;

    let items = load_messages(conn, &where_sql, &params, limit as u32, offset as u32).await?;

    Ok(Some(Page {
        items,
        total,
        limit,
        offset,
    }))
}

/// Query string for the conversation list.
///
/// Its own type rather than [`crate::paging::PageQuery`] because `sort` and
/// `order` are meaningful here and nowhere else.
#[derive(Debug, Deserialize)]
pub(crate) struct ConversationsPageQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    /// Raw so an unrecognised value falls back to the default instead of
    /// failing the request; parsed by [`ConversationSort::from_param`].
    #[serde(default)]
    sort: Option<String>,
    /// Raw for the same reason as `sort`.
    #[serde(default)]
    order: Option<String>,
}

/// Page through conversations with participants, message counts, and tags.
/// Ordered by most recent activity unless `sort` and `order` say otherwise.
#[utoipa::path(
    get,
    path = "/v1/conversations",
    tag = "Conversations",
    security(("bearer" = [])),
    params(
        ("q" = Option<String>, Query, description = "Conversation search; empty lists all non-trashed"),
        ("limit" = Option<usize>, Query, description = "Page size, default 40, max 500"),
        ("offset" = Option<usize>, Query, description = "Page offset, max 50000"),
        ("sort" = Option<String>, Query, description = "Order by `date` (last message, default) or `messages` (message count)"),
        ("order" = Option<String>, Query, description = "`asc` or `desc` (default)")
    ),
    responses(
        (status = 200, body = crate::paging::Page<crate::conversations_api::ConversationSummary>),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversations_list_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Query(query): Query<ConversationsPageQuery>,
) -> Result<Json<Page<ConversationSummary>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let q = query.q.unwrap_or_default();
    let page = page_params(
        query.limit,
        query.offset,
        DEFAULT_LIST_LIMIT,
        Some(MAX_LIST_OFFSET),
    )?;
    let order = ConversationOrder {
        sort: query
            .sort
            .as_deref()
            .map_or_else(ConversationSort::default, ConversationSort::from_param),
        order: query
            .order
            .as_deref()
            .map_or_else(SortOrder::default, SortOrder::from_param),
    };
    let result = list_conversations_sorted(
        &mut conn,
        &auth.account_id,
        &q,
        order,
        page.limit,
        page.offset,
        chrono::Local::now().date_naive(),
    )
    .await?;
    Ok(Json(result))
}

/// One conversation, in the same shape a list row already has — so a caller
/// that opens a thread from a list does not have to convert between two
/// shapes, and paging through the whole list to find one id is never
/// necessary. Trash is a property the list applies, not a gate on reading:
/// a trashed conversation still answers here.
#[utoipa::path(
    get,
    path = "/v1/conversations/{id}",
    tag = "Conversations",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Conversation id")),
    responses(
        (status = 200, body = crate::conversations_api::ConversationSummary),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversation_detail_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(conversation_id): AxumPath<i64>,
) -> Result<Json<ConversationSummary>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let conversation =
        get_conversation_summary(&mut conn, &auth.account_id, conversation_id).await?;
    conversation
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("conversation not found".into()))
}

/// Per-backup message counts for one conversation (the Sources panel).
#[utoipa::path(
    get,
    path = "/v1/conversations/{id}/sources",
    tag = "Conversations",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Conversation id")),
    responses(
        (status = 200, body = crate::conversations_api::ConversationSourcesPage),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversation_sources_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(conversation_id): AxumPath<i64>,
) -> Result<Json<ConversationSourcesPage>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let page = list_conversation_source_stats(&mut conn, &auth.account_id, conversation_id).await?;
    page.map(Json)
        .ok_or_else(|| ApiError::NotFound("conversation not found".into()))
}

/// Query string for a conversation's messages.
#[derive(Debug, Deserialize)]
pub(crate) struct ConversationMessagesQuery {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    /// Narrow to one calendar year in the vault's stored offset — the same
    /// year `date:YYYY` matches in the search language.
    #[serde(default)]
    year: Option<i32>,
}

/// A conversation's messages, ascending by timestamp then `sort_order`. The
/// read path a screen uses to open a thread: no search query to compose,
/// just the conversation id.
#[utoipa::path(
    get,
    path = "/v1/conversations/{id}/messages",
    tag = "Conversations",
    security(("bearer" = [])),
    params(
        ("id" = i64, Path, description = "Conversation id"),
        ("limit" = Option<usize>, Query, description = "Page size, default 40, max 500"),
        ("offset" = Option<usize>, Query, description = "Page offset, max 50000"),
        ("year" = Option<i32>, Query, description = "Narrow to one calendar year, in the vault's stored offset")
    ),
    responses(
        (status = 200, body = crate::paging::Page<crate::db::conversation_messages::Message>),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversation_messages_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(conversation_id): AxumPath<i64>,
    Query(query): Query<ConversationMessagesQuery>,
) -> Result<Json<Page<Message>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let page = page_params(
        query.limit,
        query.offset,
        DEFAULT_LIST_LIMIT,
        Some(MAX_LIST_OFFSET),
    )?;
    let result = get_conversation_messages(
        &mut conn,
        &auth.account_id,
        conversation_id,
        query.year,
        page.limit,
        page.offset,
    )
    .await?;
    result
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("conversation not found".into()))
}

/// Put a conversation in the trash. Idempotent: trashing an
/// already-trashed conversation still answers 204.
#[utoipa::path(
    post,
    path = "/v1/conversations/{id}/trash",
    tag = "Conversations",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Conversation id")),
    responses(
        (status = 204, description = "Trashed"),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversation_trash_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(conversation_id): AxumPath<i64>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    if trash_conversation(&mut conn, &auth.account_id, conversation_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("conversation not found".into()))
    }
}

/// Take a conversation out of the trash. Idempotent: restoring a
/// conversation that was not trashed still answers 204.
#[utoipa::path(
    post,
    path = "/v1/conversations/{id}/restore",
    tag = "Conversations",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Conversation id")),
    responses(
        (status = 204, description = "Restored"),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversation_restore_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(conversation_id): AxumPath<i64>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    if restore_conversation(&mut conn, &auth.account_id, conversation_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("conversation not found".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::HandleType;

    use crate::db::{account_profile, engine, schema, vault_imports};
    use crate::test_support::{
        RegisteredAccount, TestVault, register_via_api, seed_one_message, test_vault,
    };

    /// A newest-first page — the default ordering, which is what most of these
    /// tests care about. Ordering itself is covered by its own tests below.
    async fn list_conversations(
        conn: &mut AnyConnection,
        account_id: &str,
        q: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Page<ConversationSummary>, ApiError> {
        list_conversations_sorted(
            conn,
            account_id,
            q,
            ConversationOrder::default(),
            limit,
            offset,
            crate::search::tests::today(),
        )
        .await
    }

    /// A vault, a signed-in account, and one conversation holding one message.
    async fn conversations_fixture() -> (TestVault, String, RegisteredAccount) {
        let vault = test_vault().await;
        let account = register_via_api(&vault.state, "alice", "hunter2hunter2").await;
        seed_one_message(&vault.state, &account.account_id).await;
        let token = account.token.clone();
        (vault, token, account)
    }

    #[tokio::test]
    async fn conversation_list_takes_the_search_language() {
        let (vault, token, _account) = conversations_fixture().await;
        let page: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/conversations?q=kind:direct", &token)
                .await;
        assert!(page["total"].as_u64().unwrap() >= 1);
        let status = crate::test_support::get_status(
            &vault.state,
            "/v1/conversations?q=wibble:direct",
            &token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        let status = crate::test_support::get_status(
            &vault.state,
            "/v1/conversations?q=trashed:yes",
            &token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
    }

    /// A vault with account `00000000-0000-4000-8000-0000000000c2` and one
    /// conversation (id 1) on a handle linked through the account profile,
    /// with one participant and one message.
    ///
    /// The peer handle goes through `account_profile::link_account_handle`
    /// rather than `seed_conversation`, because the participant-naming query
    /// reads the `account_handles` link that call creates and
    /// `seed_conversation`'s bare `handles` insert does not make one; the
    /// `participants` row (`name_alias`) that query also reads has no
    /// counterpart in the seeder at all. So this stays as explicit SQL
    /// rather than using the shared seeder.
    async fn conversations_setup() -> (sqlx::AnyPool, TestVault, String) {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c2", "alice")
            .await;
        let mut conn = vault.conn().await;
        let peer = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550200",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (1, $1, $2, 'individual', 'c.jsonl')",
        )
        .bind(&account)
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (1, $1, 'Sam')",
        )
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (1, $1, 'imessage', '2024-06-01T12:00:00Z', 0, 0, 'hello')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();
        let pool = vault.state.db.clone();
        (pool, vault, account)
    }

    #[tokio::test]
    async fn list_conversations_returns_summary() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let page = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, 1);
        assert_eq!(page.items[0].message_count, 1);
        assert!(!page.items[0].is_group);
        assert_eq!(page.items[0].participants.len(), 1);
        assert_eq!(
            page.items[0].participants[0].handle,
            Some("+15555550200".to_string())
        );
    }

    #[tokio::test]
    async fn list_conversations_filters_by_handle() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let hit = list_conversations(
            &mut conn,
            &account,
            "handle:+15555550200",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(hit.total, 1);
        assert_eq!(hit.items.len(), 1);
        let miss = list_conversations(
            &mut conn,
            &account,
            "handle:+19999999999",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(miss.total, 0);
        assert!(miss.items.is_empty());
    }

    #[tokio::test]
    async fn list_conversations_finds_a_handle_across_platforms() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // setup() already has phone:+15555550200 as conversation 1.
        let wa = account_profile::link_account_handle_with_service(
            &mut conn,
            &account,
            "+15555550200",
            HandleType::Phone,
            Some("whatsapp"),
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (10, $1, $2, 'individual', 'wa.jsonl')",
        )
        .bind(&account)
        .bind(wa)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (10, $1, 'Sam WA')",
        )
        .bind(wa)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (10, $1, 'whatsapp', '2024-08-01T12:00:00Z', 0, 0, 'wa hello')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        // `handle:` matches the raw value on any platform; it does not
        // distinguish which platform a handle belongs to (there is no search
        // word for that in the current language — `service:` filters by a
        // message's own transport, imessage/sms/mms/rcs/whatsapp, which is a
        // different thing and is covered by the search module's own tests).
        let any_platform = list_conversations(
            &mut conn,
            &account,
            "handle:+15555550200",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(any_platform.total, 2);
    }

    #[tokio::test]
    async fn list_conversations_sorts_by_date_or_message_count() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();

        // Conversation 1 (from setup) gets two more *older* messages, so it is
        // the busiest thread but not the most recent one. Conversation 2 gets a
        // single *newer* message. Date order and count order then disagree,
        // which is what makes this test able to tell them apart.
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES
                (1, $1, 'imessage', '2024-05-01T12:00:00Z', 0, 1, 'older'),
                (1, $1, 'imessage', '2024-05-02T12:00:00Z', 0, 2, 'older still')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let peer2 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550300",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (2, $1, $2, 'individual', 'c2.jsonl')",
        )
        .bind(&account)
        .bind(peer2)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (2, $1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'newest')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        async fn ids_for(
            pool: &sqlx::AnyPool,
            account: &str,
            sort: ConversationSort,
            order: SortOrder,
        ) -> Vec<i64> {
            let mut conn = pool.acquire().await.unwrap();
            list_conversations_sorted(
                &mut conn,
                account,
                "",
                ConversationOrder { sort, order },
                DEFAULT_LIST_LIMIT,
                0,
                crate::search::tests::today(),
            )
            .await
            .unwrap()
            .items
            .iter()
            .map(|c| c.id)
            .collect()
        }

        // 3 messages ending 2024-06-01 (id 1) vs 1 message on 2024-07-01 (id 2).
        assert_eq!(
            ids_for(&pool, &account, ConversationSort::Date, SortOrder::Desc).await,
            [2, 1],
            "newest activity first"
        );
        assert_eq!(
            ids_for(&pool, &account, ConversationSort::Date, SortOrder::Asc).await,
            [1, 2],
            "oldest activity first"
        );
        assert_eq!(
            ids_for(&pool, &account, ConversationSort::Messages, SortOrder::Desc).await,
            [1, 2],
            "busiest thread first"
        );
        assert_eq!(
            ids_for(&pool, &account, ConversationSort::Messages, SortOrder::Asc).await,
            [2, 1],
            "quietest thread first"
        );
    }

    #[tokio::test]
    async fn list_conversations_paginates() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // Second conversation + message.
        let peer2 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550300",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (2, $1, $2, 'individual', 'c2.jsonl')",
        )
        .bind(&account)
        .bind(peer2)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (2, $1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'later')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page0 = list_conversations(&mut conn, &account, "", 1, 0)
            .await
            .unwrap();
        assert_eq!(page0.total, 2);
        assert_eq!(page0.limit, 1);
        assert_eq!(page0.offset, 0);
        assert_eq!(page0.items.len(), 1);
        assert_eq!(page0.items[0].id, 2); // newer first

        let page1 = list_conversations(&mut conn, &account, "", 1, 1)
            .await
            .unwrap();
        assert_eq!(page1.total, 2);
        assert_eq!(page1.offset, 1);
        assert_eq!(page1.items.len(), 1);
        assert_eq!(page1.items[0].id, 1);

        let by_text = list_conversations(&mut conn, &account, "5555550300", 10, 0)
            .await
            .unwrap();
        assert_eq!(by_text.total, 1);
        assert_eq!(by_text.items[0].id, 2);
    }

    #[tokio::test]
    async fn list_queries_enforce_search_limits() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let oversized = "x".repeat(2_049);
        let too_many_terms = (0..33)
            .map(|index| format!("term{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let too_many_nodes = "(".repeat(65);

        for query in [&oversized, &too_many_terms, &too_many_nodes] {
            let contact_error = crate::contacts_api::list_contacts(
                &mut conn,
                &account,
                query,
                DEFAULT_LIST_LIMIT,
                0,
                chrono::Local::now().date_naive(),
            )
            .await
            .unwrap_err();
            assert!(
                matches!(contact_error, ApiError::BadRequest(_)),
                "contact query should be rejected: {query}"
            );

            let conversation_error =
                list_conversations(&mut conn, &account, query, DEFAULT_LIST_LIMIT, 0)
                    .await
                    .unwrap_err();
            assert!(
                matches!(conversation_error, ApiError::BadRequest(_)),
                "conversation query should be rejected: {query}"
            );
        }
    }

    #[tokio::test]
    async fn malformed_boolean_queries_are_bad_requests_for_export() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();

        for query in ["foo OR", "(foo OR bar", "foo OR bar)"] {
            let export_error = crate::export_api::export_message_count(
                &mut conn,
                crate::export_api::ExportCountOpts {
                    account_id: &account,
                    query,
                    today: chrono::Local::now().date_naive(),
                },
            )
            .await
            .unwrap_err();
            assert!(matches!(export_error, ApiError::BadRequest(_)));
        }
    }

    #[tokio::test]
    async fn list_conversations_filters_by_contact_and_type() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // Link peer handle to a contact.
        sqlx::query("INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        let contact_id: i64 = sqlx::query_scalar("SELECT id FROM contacts WHERE account_id = $1")
            .bind(&account)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let peer_handle_id: i64 =
            sqlx::query_scalar("SELECT id FROM handles WHERE account_id = $1 AND raw = $2")
                .bind(&account)
                .bind("+15555550200")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(peer_handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        // Unrelated group conversation (no link to Sam).
        let other = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550999",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, group_title, source_file
             ) VALUES (9, $1, $2, 'group', 'Other', 'g.jsonl')",
        )
        .bind(&account)
        .bind(other)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (9, $1, 'imessage', '2024-08-01T12:00:00Z', 0, 0, 'group')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        // Group that includes Sam (distinct chat handle; Sam is a participant).
        let group_chat = account_profile::link_account_handle(
            &mut conn,
            &account,
            "chat123456",
            HandleType::Other,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, group_title, source_file
             ) VALUES (3, $1, $2, 'group', 'Sam Group', 'sg.jsonl')",
        )
        .bind(&account)
        .bind(group_chat)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (3, $1, 'Sam')",
        )
        .bind(peer_handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (3, $1, 'imessage', '2024-09-01T12:00:00Z', 0, 0, 'hi group')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let all = list_conversations(
            &mut conn,
            &account,
            &format!("with:#{contact_id}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(all.total, 2);
        let ids: Vec<i64> = all.items.iter().map(|c| c.id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&3));

        let direct = list_conversations(
            &mut conn,
            &account,
            &format!("with:#{contact_id} kind:direct"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(direct.total, 1);
        assert_eq!(direct.items[0].id, 1);
        assert!(!direct.items[0].is_group);

        let groups = list_conversations(
            &mut conn,
            &account,
            &format!("with:#{contact_id} kind:group"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(groups.total, 1);
        assert_eq!(groups.items[0].id, 3);
        assert!(groups.items[0].is_group);
    }

    /// A newest-first page for `setup()`'s account, with the default query and
    /// paging — what each of the three participant-naming tests below needs.
    async fn list_conversations_page(
        conn: &mut AnyConnection,
        account_id: &str,
    ) -> Page<ConversationSummary> {
        list_conversations(conn, account_id, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap()
    }

    fn find_participant<'a>(
        page: &'a crate::paging::Page<ConversationSummary>,
        handle: &str,
    ) -> &'a Participant {
        page.items
            .iter()
            .flat_map(|c| c.participants.iter())
            .find(|p| p.handle.as_deref() == Some(handle))
            .expect("participant is in the page")
    }

    #[tokio::test]
    async fn list_conversations_shows_the_contact_name() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam Preferred')
             RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "SELECT id FROM handles WHERE account_id = $1 AND raw = '+15555550200'",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = list_conversations_page(&mut conn, &account).await;
        let p = find_participant(&page, "+15555550200");
        assert_eq!(p.name, "Sam Preferred");
        assert_eq!(p.contact_id, Some(contact_id));
    }

    #[tokio::test]
    async fn list_conversations_falls_back_to_the_backup_name() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // setup() records the backup name 'Sam' on +15555550200 and links no
        // contact, so the backup's name is what there is to show.
        let page = list_conversations_page(&mut conn, &account).await;
        let p = find_participant(&page, "+15555550200");
        assert_eq!(p.name, "Sam");
        assert_eq!(p.contact_id, None);
    }

    #[tokio::test]
    async fn list_conversations_falls_back_to_the_handle() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE participants SET name_alias = NULL")
            .execute(&mut *conn)
            .await
            .unwrap();
        let page = list_conversations_page(&mut conn, &account).await;
        let p = find_participant(&page, "+15555550200");
        assert_eq!(p.name, "+15555550200");
    }

    #[tokio::test]
    async fn list_conversations_filters_by_participant_count() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // setup() has conversation 1 with 1 participant.

        let p2 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550301",
            HandleType::Phone,
        )
        .await
        .unwrap();
        let p3 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550302",
            HandleType::Phone,
        )
        .await
        .unwrap();
        let group_chat = account_profile::link_account_handle(
            &mut conn,
            &account,
            "chat-big",
            HandleType::Other,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, group_title, source_file
             ) VALUES (10, $1, $2, 'group', 'Trio', 't.jsonl')",
        )
        .bind(&account)
        .bind(group_chat)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias) VALUES
             (10, $1, 'A'), (10, $2, 'B')",
        )
        .bind(p2)
        .bind(p3)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (10, $1, 'imessage', '2024-10-01T12:00:00Z', 0, 0, 'hi')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let eq2 = list_conversations(&mut conn, &account, "participants:=2", 50, 0)
            .await
            .unwrap();
        assert_eq!(eq2.total, 1);
        assert_eq!(eq2.items[0].id, 10);

        let gt1 = list_conversations(&mut conn, &account, "participants:>1", 50, 0)
            .await
            .unwrap();
        assert_eq!(gt1.total, 1);
        assert_eq!(gt1.items[0].id, 10);

        let eq1 = list_conversations(&mut conn, &account, "participants:1", 50, 0)
            .await
            .unwrap();
        assert_eq!(eq1.total, 1);
        assert_eq!(eq1.items[0].id, 1);

        let lt2 = list_conversations(&mut conn, &account, "kind:group participants:<2", 50, 0)
            .await
            .unwrap();
        assert_eq!(lt2.total, 0);
    }

    #[tokio::test]
    async fn list_conversations_participants_eq_three_on_built_fixture() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // setup() already owns conversation 1 with 1 participant, which the
        // `=3` filter below must exclude.

        let p2 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550401",
            HandleType::Phone,
        )
        .await
        .unwrap();
        let p3 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550402",
            HandleType::Phone,
        )
        .await
        .unwrap();
        let p4 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550403",
            HandleType::Phone,
        )
        .await
        .unwrap();
        let group_chat = account_profile::link_account_handle(
            &mut conn,
            &account,
            "chat-trio",
            HandleType::Other,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, group_title, source_file
             ) VALUES (20, $1, $2, 'group', 'Trio', 't2.jsonl')",
        )
        .bind(&account)
        .bind(group_chat)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias) VALUES
             (20, $1, 'A'), (20, $2, 'B'), (20, $3, 'C')",
        )
        .bind(p2)
        .bind(p3)
        .bind(p4)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (20, $1, 'imessage', '2024-11-01T12:00:00Z', 0, 0, 'hi trio')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = list_conversations(&mut conn, &account, "participants:=3", 50, 0)
            .await
            .unwrap();
        assert_eq!(
            page.total, 1,
            "only the trio conversation has exactly 3 participants"
        );
        assert_eq!(page.items[0].id, 20);
        assert_eq!(page.items[0].participants.len(), 3);
    }

    #[tokio::test]
    async fn list_conversations_filters_by_import_id() {
        // Fresh db (setup() already owns conversation 1, which this test inserts itself).
        let (pool, _dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000c2".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();

        let import_a = vault_imports::start_import(
            &mut conn,
            &vault_imports::StartImportArgs {
                account_id: &account,
                source: "imessage-ios",
                mode: "append",
                tool: Some("test"),
                stage: vault_imports::ImportStage::Parse,
                staging_dir: None,
                device_id: None,
                form_json: None,
                source_fingerprint: None,
                source_identities: None,
            },
        )
        .await
        .unwrap();
        // Only one session may be `running` per account (the partial unique
        // index); finish `import_a` so `import_b` can start.
        vault_imports::complete_import(
            &mut conn,
            &account,
            import_a,
            &vault_imports::CompleteImportArgs::succeeded(1, 0),
        )
        .await
        .unwrap();
        let import_b = vault_imports::start_import(
            &mut conn,
            &vault_imports::StartImportArgs {
                account_id: &account,
                source: "imessage-ios",
                mode: "append",
                tool: Some("test"),
                stage: vault_imports::ImportStage::Parse,
                staging_dir: None,
                device_id: None,
                form_json: None,
                source_fingerprint: None,
                source_identities: None,
            },
        )
        .await
        .unwrap();

        let peer1 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550200",
            HandleType::Phone,
        )
        .await
        .unwrap();
        let peer2 = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550300",
            HandleType::Phone,
        )
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (1, $1, $2, 'individual', 'c1.jsonl')",
        )
        .bind(&account)
        .bind(peer1)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (1, $1, 'Sam')",
        )
        .bind(peer1)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (2, $1, $2, 'individual', 'c2.jsonl')",
        )
        .bind(&account)
        .bind(peer2)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (2, $1, 'Alex')",
        )
        .bind(peer2)
        .execute(&mut *conn)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body,
                import_id
             ) VALUES (1, $1, 'imessage', '2024-06-01T12:00:00Z', 0, 0, 'hello', $2)",
        )
        .bind(&account)
        .bind(import_a)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body,
                import_id
             ) VALUES (2, $1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'later', $2)",
        )
        .bind(&account)
        .bind(import_b)
        .execute(&mut *conn)
        .await
        .unwrap();

        let a = list_conversations(
            &mut conn,
            &account,
            &format!("import:#{import_a}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(a.total, 1);
        assert_eq!(a.items[0].id, 1);

        let b = list_conversations(
            &mut conn,
            &account,
            &format!("import:#{import_b}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(b.total, 1);
        assert_eq!(b.items[0].id, 2);

        let missing =
            list_conversations(&mut conn, &account, "import:#999999", DEFAULT_LIST_LIMIT, 0)
                .await
                .unwrap();
        assert_eq!(missing.total, 0);

        // The language refuses a value it cannot parse instead of ignoring it.
        let junk = list_conversations(
            &mut conn,
            &account,
            "import:not-a-number",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap_err();
        assert!(matches!(junk, ApiError::BadRequest(_)));
    }

    #[test]
    fn sort_params_fall_back_instead_of_failing() {
        // Before `sort` existed an unknown query parameter was ignored, so an
        // unrecognised value must still yield a list rather than a 400.
        assert_eq!(
            ConversationSort::from_param("messages"),
            ConversationSort::Messages
        );
        assert_eq!(
            ConversationSort::from_param("MESSAGES"),
            ConversationSort::Messages
        );
        assert_eq!(ConversationSort::from_param("date"), ConversationSort::Date);
        assert_eq!(ConversationSort::from_param(""), ConversationSort::Date);
        assert_eq!(
            ConversationSort::from_param("oldest"),
            ConversationSort::Date
        );

        assert_eq!(SortOrder::from_param("asc"), SortOrder::Asc);
        assert_eq!(SortOrder::from_param(" Asc "), SortOrder::Asc);
        assert_eq!(SortOrder::from_param("desc"), SortOrder::Desc);
        assert_eq!(SortOrder::from_param(""), SortOrder::Desc);
        assert_eq!(SortOrder::from_param("sideways"), SortOrder::Desc);
    }

    #[tokio::test]
    async fn duplicate_only_threads_sort_last_in_either_date_direction() {
        // `last_message_at` is NULL for a thread whose every message is a
        // duplicate. Those threads are only listed under an `import:` filter,
        // which is the one path where NULL ordering is observable — and the two
        // engines disagree about it unless the query says where NULLs go.
        let (pool, _dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000c2".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();

        let import_a = vault_imports::start_import(
            &mut conn,
            &vault_imports::StartImportArgs {
                account_id: &account,
                source: "imessage-ios",
                mode: "append",
                tool: Some("test"),
                stage: vault_imports::ImportStage::Parse,
                staging_dir: None,
                device_id: None,
                form_json: None,
                source_fingerprint: None,
                source_identities: None,
            },
        )
        .await
        .unwrap();

        for (id, raw) in [(3, "+15555550400"), (4, "+15555550401")] {
            let peer =
                account_profile::link_account_handle(&mut conn, &account, raw, HandleType::Phone)
                    .await
                    .unwrap();
            sqlx::query(
                "INSERT INTO conversations (
                    id, account_id, chat_handle_id, conversation_type, source_file
                 ) VALUES ($1, $2, $3, 'individual', 'c.jsonl')",
            )
            .bind(id)
            .bind(&account)
            .bind(peer)
            .execute(&mut *conn)
            .await
            .unwrap();
        }

        // Conversation 4 keeps a real message, and it belongs to the import.
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body,
                import_id
             ) VALUES (4, $1, 'imessage', '2024-05-01T12:00:00Z', 0, 0, 'canonical', $2)",
        )
        .bind(&account)
        .bind(import_a)
        .execute(&mut *conn)
        .await
        .unwrap();
        let winner_id: i64 =
            sqlx::query_scalar("SELECT id FROM messages WHERE conversation_id = 4")
                .fetch_one(&mut *conn)
                .await
                .unwrap();

        // Conversation 3's only message is a duplicate, so its last_message_at
        // is NULL even though its timestamp is the later of the two.
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body,
                import_id, duplicate_of
             ) VALUES (3, $1, 'imessage', '2024-06-01T12:00:00Z', 0, 0, 'dup', $2, $3)",
        )
        .bind(&account)
        .bind(import_a)
        .bind(winner_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        async fn ids_for(
            pool: &sqlx::AnyPool,
            account: &str,
            q: &str,
            order: SortOrder,
        ) -> Vec<i64> {
            let mut conn = pool.acquire().await.unwrap();
            list_conversations_sorted(
                &mut conn,
                account,
                q,
                ConversationOrder {
                    sort: ConversationSort::Date,
                    order,
                },
                DEFAULT_LIST_LIMIT,
                0,
                crate::search::tests::today(),
            )
            .await
            .unwrap()
            .items
            .iter()
            .map(|c| c.id)
            .collect()
        }

        let q = format!("import:#{import_a}");
        assert_eq!(
            ids_for(&pool, &account, &q, SortOrder::Desc).await,
            [4, 3],
            "a thread with no surviving message sorts last, not first"
        );
        assert_eq!(
            ids_for(&pool, &account, &q, SortOrder::Asc).await,
            [4, 3],
            "and stays last when the direction flips"
        );
    }

    #[tokio::test]
    async fn list_conversations_import_id_includes_duplicate_only_thread() {
        // Fresh db: setup() would add a second non-duplicate conversation,
        // which breaks the "all" total assertion below.
        let (pool, _dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000c2".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();

        let import_a = vault_imports::start_import(
            &mut conn,
            &vault_imports::StartImportArgs {
                account_id: &account,
                source: "imessage-ios",
                mode: "append",
                tool: Some("test"),
                stage: vault_imports::ImportStage::Parse,
                staging_dir: None,
                device_id: None,
                form_json: None,
                source_fingerprint: None,
                source_identities: None,
            },
        )
        .await
        .unwrap();

        let peer = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550400",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (3, $1, $2, 'individual', 'dup-only.jsonl')",
        )
        .bind(&account)
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (3, $1, 'Pat')",
        )
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();

        // Canonical message in another conversation (winner for dedupe).
        let peer_other = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550401",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (4, $1, $2, 'individual', 'winner.jsonl')",
        )
        .bind(&account)
        .bind(peer_other)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (4, $1, 'imessage', '2024-05-01T12:00:00Z', 0, 0, 'canonical')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();
        let winner_id: i64 =
            sqlx::query_scalar("SELECT id FROM messages WHERE conversation_id = 4")
                .fetch_one(&mut *conn)
                .await
                .unwrap();

        // Only message in conversation 3 from import A is a duplicate.
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body,
                import_id, duplicate_of
             ) VALUES (3, $1, 'imessage', '2024-06-01T12:00:00Z', 0, 0, 'dup', $2, $3)",
        )
        .bind(&account)
        .bind(import_a)
        .bind(winner_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let by_import = list_conversations(
            &mut conn,
            &account,
            &format!("import:#{import_a}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(
            by_import.total, 1,
            "import filter should match duplicate-only thread"
        );
        assert_eq!(by_import.items[0].id, 3);

        let all = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(
            all.total, 1,
            "default list still requires a non-duplicate message"
        );
        assert_eq!(all.items[0].id, 4);
    }

    #[test]
    fn display_service_label_from_sources() {
        assert_eq!(display_service_label(&["imessage".into()]), "imessage");
        assert_eq!(
            display_service_label(&["sms-backup-restore".into()]),
            "SMS/MMS"
        );
        assert_eq!(
            display_service_label(&["imessage".into(), "sms-backup-restore".into()]),
            "SMS/MMS"
        );
        assert_eq!(display_service_label(&[]), "unknown");
        assert_eq!(display_service_label(&["whatsapp".into()]), "WhatsApp");
    }

    #[tokio::test]
    async fn list_conversations_filters_by_tag_and_people() {
        let (pool, _vault, account) = conversations_setup().await;
        let mut conn = pool.acquire().await.unwrap();
        crate::named_membership::set_membership(
            crate::named_membership::tag_spec(),
            &mut conn,
            &account,
            &[1],
            "Holiday",
            true,
        )
        .await
        .unwrap();
        let tagged = list_conversations(&mut conn, &account, "tag:Holiday", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(tagged.total, 1);
        assert_eq!(tagged.items[0].tags, vec!["Holiday".to_string()]);
        let hidden = list_conversations(&mut conn, &account, "-tag:Holiday", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(hidden.total, 0);
        let untagged = list_conversations(&mut conn, &account, "tag:none", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(untagged.total, 0);

        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 =
            sqlx::query_scalar("SELECT chat_handle_id FROM conversations WHERE id = 1")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id) VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        crate::named_membership::set_membership(
            crate::named_membership::group_spec(),
            &mut conn,
            &account,
            &[contact_id],
            "Family",
            true,
        )
        .await
        .unwrap();
        let family = list_conversations(&mut conn, &account, "group:Family", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(family.total, 1);
        let not_family =
            list_conversations(&mut conn, &account, "-group:Family", DEFAULT_LIST_LIMIT, 0)
                .await
                .unwrap();
        assert_eq!(not_family.total, 0);
    }

    #[tokio::test]
    async fn the_conversation_list_is_a_page_with_integer_ids() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;

        let page: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations?limit=10", &user.token).await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["limit"], 10);
        assert_eq!(page["offset"], 0);
        assert!(
            page["items"][0]["id"].is_i64(),
            "id must be an integer: {page}"
        );
        assert!(page.get("conversations").is_none());
        assert!(page.get("ok").is_none());
    }

    #[tokio::test]
    async fn a_limit_past_the_cap_or_an_offset_past_the_cap_is_a_400() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        for path in [
            "/v1/conversations?limit=501",
            "/v1/conversations?limit=0",
            "/v1/conversations?offset=50001",
        ] {
            let status = crate::test_support::get_status(&state, path, &user.token).await;
            assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{path}");
        }
    }

    #[tokio::test]
    async fn conversation_detail_returns_the_owned_conversation() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let body: serde_json::Value =
            crate::test_support::get_json(&state, &format!("/v1/conversations/{id}"), &user.token)
                .await;
        assert_eq!(body["id"], id);
        let participants = body["participants"].as_array().unwrap();
        assert!(!participants.is_empty());
        assert!(
            participants[0]["name"]
                .as_str()
                .is_some_and(|n| !n.is_empty()),
            "participant should carry a name: {body}"
        );
    }

    #[tokio::test]
    async fn conversation_detail_404s_for_an_id_this_account_does_not_own() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        let status =
            crate::test_support::get_status(&state, "/v1/conversations/999999", &user.token).await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn conversation_detail_404s_for_another_accounts_conversation() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let alice = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &alice.account_id).await;
        let alice_list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &alice.token).await;
        let alice_conversation_id = alice_list["items"][0]["id"].as_i64().unwrap();

        let bob = crate::test_support::register_via_api(&state, "bob", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &bob.account_id).await;

        // Bob asking for Alice's conversation id must 404, not 403 — a 403
        // would confirm the id exists in someone else's vault, and it must
        // not come back as Bob's own conversation either.
        let status = crate::test_support::get_status(
            &state,
            &format!("/v1/conversations/{alice_conversation_id}"),
            &bob.token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn conversation_detail_reads_a_trashed_conversation() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let mut conn = state.db.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO trashed_conversations (account_id, conversation_id) VALUES ($1, $2)",
        )
        .bind(&user.account_id)
        .bind(id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        // Trashed for the list, which no longer applies here — trash is a
        // property the list applies, not a gate on reading.
        let list_after: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        assert_eq!(
            list_after["total"], 0,
            "trashed conversation leaves the inbox list"
        );

        let status = crate::test_support::get_status(
            &state,
            &format!("/v1/conversations/{id}"),
            &user.token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
    }

    async fn trashed_conversation_row_count(
        conn: &mut AnyConnection,
        account_id: &str,
        id: i64,
    ) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM trashed_conversations
             WHERE account_id = $1 AND conversation_id = $2",
        )
        .bind(account_id)
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn conversation_trash_drops_it_from_the_list() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let status = crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{id}/trash"),
            &user.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);

        let list_after: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        assert_eq!(
            list_after["total"], 0,
            "a trashed conversation must leave the conversations list"
        );
    }

    #[tokio::test]
    async fn conversation_trash_twice_is_204_with_no_second_marker() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();
        let path = format!("/v1/conversations/{id}/trash");

        for _ in 0..2 {
            let status =
                crate::test_support::post_status(&state, &path, &user.token, serde_json::json!({}))
                    .await;
            assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
        }

        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_conversation_row_count(&mut conn, &user.account_id, id).await,
            1,
            "trashing twice must not create a second marker row"
        );
    }

    #[tokio::test]
    async fn conversation_restore_brings_it_back_to_the_list() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();
        crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{id}/trash"),
            &user.token,
            serde_json::json!({}),
        )
        .await;

        let status = crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{id}/restore"),
            &user.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);

        let list_after: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        assert_eq!(
            list_after["total"], 1,
            "a restored conversation must come back to the conversations list"
        );
    }

    #[tokio::test]
    async fn conversation_restore_twice_is_204_with_marker_gone() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();
        crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{id}/trash"),
            &user.token,
            serde_json::json!({}),
        )
        .await;
        let path = format!("/v1/conversations/{id}/restore");

        for _ in 0..2 {
            let status =
                crate::test_support::post_status(&state, &path, &user.token, serde_json::json!({}))
                    .await;
            assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
        }

        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_conversation_row_count(&mut conn, &user.account_id, id).await,
            0,
            "restoring twice must leave no marker row"
        );
    }

    #[tokio::test]
    async fn conversation_trash_404s_for_an_unknown_id() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        let status = crate::test_support::post_status(
            &state,
            "/v1/conversations/999999/trash",
            &user.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn conversation_restore_404s_for_an_unknown_id() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        let status = crate::test_support::post_status(
            &state,
            "/v1/conversations/999999/restore",
            &user.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn conversation_trash_404s_for_another_accounts_conversation() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let alice = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &alice.account_id).await;
        let alice_list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &alice.token).await;
        let alice_conversation_id = alice_list["items"][0]["id"].as_i64().unwrap();

        let bob = crate::test_support::register_via_api(&state, "bob", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &bob.account_id).await;

        // Bob trashing Alice's conversation id must 404, not 403 — a 403
        // would confirm the id exists in someone else's vault.
        let status = crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{alice_conversation_id}/trash"),
            &bob.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_conversation_row_count(&mut conn, &alice.account_id, alice_conversation_id)
                .await,
            0,
            "Bob's request must not trash Alice's conversation"
        );
    }

    #[tokio::test]
    async fn conversation_restore_404s_for_another_accounts_conversation() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let alice = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &alice.account_id).await;
        let alice_list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &alice.token).await;
        let alice_conversation_id = alice_list["items"][0]["id"].as_i64().unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO trashed_conversations (account_id, conversation_id) VALUES ($1, $2)",
        )
        .bind(&alice.account_id)
        .bind(alice_conversation_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let bob = crate::test_support::register_via_api(&state, "bob", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &bob.account_id).await;

        let status = crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{alice_conversation_id}/restore"),
            &bob.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_conversation_row_count(&mut conn, &alice.account_id, alice_conversation_id)
                .await,
            1,
            "Bob's request must not restore Alice's conversation"
        );
    }

    #[tokio::test]
    async fn conversation_trash_requires_auth() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let status = crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{id}/trash"),
            "not-a-token",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn conversation_restore_requires_auth() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations", &user.token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let status = crate::test_support::post_status(
            &state,
            &format!("/v1/conversations/{id}/restore"),
            "not-a-token",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    }

    /// A signed-in account and one conversation with no messages yet, for
    /// tests that seed their own message rows with specific timestamps and
    /// `sort_order`.
    async fn conversation_messages_fixture() -> (TestVault, RegisteredAccount, i64) {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        let mut conn = state.db.acquire().await.unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, $2, $2, 'phone', 'phone') RETURNING id",
        )
        .bind(&user.account_id)
        .bind(format!("+1555{}", user.account_id))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let conversation_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES ($1, $2, 'individual', 'seed.jsonl') RETURNING id",
        )
        .bind(&user.account_id)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        (vault, user, conversation_id)
    }

    /// Insert one message row with an explicit `timestamp` and `sort_order`,
    /// the control the JSON-import path does not give.
    async fn insert_message(
        conn: &mut AnyConnection,
        conversation_id: i64,
        account_id: &str,
        timestamp: &str,
        sort_order: i64,
        body: &str,
    ) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES ($1, $2, 'imessage', $3, 1, $4, $5) RETURNING id",
        )
        .bind(conversation_id)
        .bind(account_id)
        .bind(timestamp)
        .bind(sort_order)
        .bind(body)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn conversation_messages_are_ascending_by_timestamp_then_sort_order() {
        let (vault, user, conversation_id) = conversation_messages_fixture().await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        // Deliberately inserted out of order: the third-in-time message
        // first, and two same-timestamp messages ordered only by sort_order.
        insert_message(
            &mut conn,
            conversation_id,
            &user.account_id,
            "2024-01-03T00:00:00Z",
            0,
            "third",
        )
        .await;
        insert_message(
            &mut conn,
            conversation_id,
            &user.account_id,
            "2024-01-01T00:00:00Z",
            5,
            "second",
        )
        .await;
        insert_message(
            &mut conn,
            conversation_id,
            &user.account_id,
            "2024-01-01T00:00:00Z",
            1,
            "first",
        )
        .await;
        drop(conn);

        let page: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages"),
            &user.token,
        )
        .await;
        let texts: Vec<&str> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, vec!["first", "second", "third"]);
    }

    #[tokio::test]
    async fn conversation_messages_page_and_total_is_the_whole_count() {
        let (vault, user, conversation_id) = conversation_messages_fixture().await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        for day in 1..=5 {
            insert_message(
                &mut conn,
                conversation_id,
                &user.account_id,
                &format!("2024-01-0{day}T00:00:00Z"),
                0,
                &format!("msg{day}"),
            )
            .await;
        }
        drop(conn);

        let page: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages?limit=2&offset=1"),
            &user.token,
        )
        .await;
        assert_eq!(
            page["total"], 5,
            "total is the whole count, not the page's length: {page}"
        );
        assert_eq!(page["items"].as_array().unwrap().len(), 2);
        assert_eq!(page["limit"], 2);
        assert_eq!(page["offset"], 1);
        let texts: Vec<&str> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, vec!["msg2", "msg3"]);
    }

    #[tokio::test]
    async fn conversation_messages_year_narrows_and_total_is_the_years_count() {
        let (vault, user, conversation_id) = conversation_messages_fixture().await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        for day in 1..=2 {
            insert_message(
                &mut conn,
                conversation_id,
                &user.account_id,
                &format!("2023-06-0{day}T00:00:00Z"),
                0,
                "in 2023",
            )
            .await;
        }
        for day in 1..=3 {
            insert_message(
                &mut conn,
                conversation_id,
                &user.account_id,
                &format!("2024-06-0{day}T00:00:00Z"),
                0,
                "in 2024",
            )
            .await;
        }
        drop(conn);

        let page: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages?year=2024"),
            &user.token,
        )
        .await;
        assert_eq!(page["total"], 3, "total is the year's count: {page}");
        assert!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["text"] == "in 2024"),
            "only 2024 messages: {page}"
        );

        let whole: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages"),
            &user.token,
        )
        .await;
        assert_eq!(whole["total"], 5, "no year= is the whole conversation");
    }

    #[tokio::test]
    async fn a_message_at_31_december_2359_local_is_in_that_year_not_the_next() {
        let (vault, user, conversation_id) = conversation_messages_fixture().await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        // Local offset -05:00: 2024-12-31 23:59 local is 2025-01-01 04:59
        // UTC (`timestamp_utc`, set explicitly here). A boundary computed
        // against UTC would place this message in 2025; the search
        // language's date:2024 compares the local `timestamp` text as a
        // prefix, so it must stay in 2024. That is the boundary this route
        // must also use.
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, timestamp_utc,
                is_from_me, sort_order, body
             ) VALUES ($1, $2, 'imessage', $3, $4, 1, 0, 'new year''s eve')",
        )
        .bind(conversation_id)
        .bind(&user.account_id)
        .bind("2024-12-31T23:59:00-05:00")
        .bind("2025-01-01T04:59:00Z")
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let this_year: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages?year=2024"),
            &user.token,
        )
        .await;
        assert_eq!(
            this_year["total"], 1,
            "31 Dec 23:59 local belongs to its own year: {this_year}"
        );

        let next_year: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages?year=2025"),
            &user.token,
        )
        .await;
        assert_eq!(
            next_year["total"], 0,
            "31 Dec 23:59 local does not leak into the next year: {next_year}"
        );
    }

    #[tokio::test]
    async fn conversation_messages_404s_for_an_unknown_id() {
        let (vault, user, _conversation_id) = conversation_messages_fixture().await;
        let status = crate::test_support::get_status(
            &vault.state,
            "/v1/conversations/999999/messages",
            &user.token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn conversation_messages_404s_for_another_accounts_conversation() {
        let (vault, _alice, alice_conversation_id) = conversation_messages_fixture().await;
        let bob =
            crate::test_support::register_via_api(&vault.state, "bob", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&vault.state, &bob.account_id).await;

        let status = crate::test_support::get_status(
            &vault.state,
            &format!("/v1/conversations/{alice_conversation_id}/messages"),
            &bob.token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn conversation_messages_reads_a_trashed_conversations_messages() {
        let (vault, user, conversation_id) = conversation_messages_fixture().await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        insert_message(
            &mut conn,
            conversation_id,
            &user.account_id,
            "2024-01-01T00:00:00Z",
            0,
            "still here",
        )
        .await;
        sqlx::query(
            "INSERT INTO trashed_conversations (account_id, conversation_id) VALUES ($1, $2)",
        )
        .bind(&user.account_id)
        .bind(conversation_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let page: serde_json::Value = crate::test_support::get_json(
            &vault.state,
            &format!("/v1/conversations/{conversation_id}/messages"),
            &user.token,
        )
        .await;
        assert_eq!(
            page["total"], 1,
            "a trashed conversation's messages are readable: {page}"
        );
    }

    #[tokio::test]
    async fn conversation_messages_bad_limit_is_refused_like_other_paged_routes() {
        let (vault, user, conversation_id) = conversation_messages_fixture().await;

        for path in [
            format!("/v1/conversations/{conversation_id}/messages?limit=501"),
            format!("/v1/conversations/{conversation_id}/messages?limit=0"),
            format!("/v1/conversations/{conversation_id}/messages?offset=50001"),
        ] {
            let status = crate::test_support::get_status(&vault.state, &path, &user.token).await;
            assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{path}");
        }
    }
}
