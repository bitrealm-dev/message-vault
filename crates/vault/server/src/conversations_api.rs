//! Read-only conversation list used by `GET /v1/export/conversations`.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::HeaderMap;
use serde::Serialize;
use sqlx::AnyConnection;
use sqlx::Row;

use crate::db::dialect::{engine_of, like_ci_numbered};
use crate::db::engine::DbEngine;
use crate::db::sql::{fold_in_id_chunks, group_rows_by_id, in_placeholders};
use crate::export_api::ExportQueryError;
use crate::search_query::{CountComparison, parse_count_comparison};
use crate::server::{ApiError, AppState, require_full_access, resolve_auth};

pub use crate::page_limits::{
    DEFAULT_LIST_LIMIT, MAX_CONVERSATION_LIST_LIMIT as MAX_LIST_LIMIT, MAX_LIST_OFFSET,
};

/// One bound value in the dynamic list query. sqlx Any has no
/// user-constructible dynamic value, so binds ride this enum and are chained
/// onto the statement in order at execution time.
enum Bind {
    Text(String),
    Int(i64),
}

/// One page of the conversation list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConversationListPage {
    /// Conversations on this page.
    pub conversations: Vec<ConversationSummary>,
    /// Total conversations matching the query.
    pub total: u64,
    /// Page size used.
    pub limit: usize,
    /// Page offset used.
    pub offset: usize,
}

/// One participant with display name and handle.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConversationParticipant {
    /// Display name from the import or the vault contact.
    pub name: Option<String>,
    /// Per service+identity alias from `contact_handles` when linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_alias: Option<String>,
    /// Raw handle value (phone, email, or username).
    pub handle: String,
    /// Platform service, e.g. `imessage`.
    pub service: String,
    /// Linked vault contact id, when the handle is linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<String>,
}

/// Conversation row for the list: participants, counts, tags.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConversationSummary {
    /// Numeric `conversations.id`, serialized as a string for `in:<id>` queries.
    pub id: String,
    /// Participants with names and handles.
    pub participants: Vec<ConversationParticipant>,
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
    /// Thread tags on this conversation.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationTypeFilter {
    Direct,
    Group,
}

#[derive(Debug, Default)]
struct ConversationListQuery {
    trash_only: bool,
    handle: Option<String>,
    /// Platform identity on `handles.service` (`phone` | `whatsapp`). Applied only with `handle:`.
    service: Option<String>,
    contact_id: Option<i64>,
    type_filter: Option<ConversationTypeFilter>,
    /// Filter by number of rows in `participants` (`participants:=5`, `:>3`, `:<10`).
    participants: Option<CountComparison>,
    /// Filter to conversations with at least one message from this import session.
    import_id: Option<i64>,
    /// Contact-group name (`people:` / `within:` / `label:`).
    people: Option<String>,
    /// Hide threads that involve that contact group (`-people:`).
    exclude_people: Option<String>,
    /// Thread tag name (`tag:`).
    tag: Option<String>,
    /// Hide threads that have that tag (`-tag:`).
    exclude_tag: Option<String>,
    /// Threads with no thread tags (`tag:none`).
    no_tag: bool,
    text: Option<String>,
}

/// Parse `participants:` values: `=5`, `>3`, `<10`, `>=2`, `<=8`, or bare `5` (=).
fn parse_participants_comparison(raw: &str) -> Option<CountComparison> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return parse_count_comparison(&format!("={t}"));
    }
    parse_count_comparison(t)
}

/// Pull `people:` / `tag:` / `within:` / `label:` (optional leading `-`) and quoted values.
fn pull_named_ops(q: &str) -> (String, Vec<(String, String, bool)>) {
    const KEYS: &[&str] = &["people", "tag", "within", "label"];
    let mut rest = String::new();
    let mut found = Vec::new();
    let bytes = q.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        let negated = bytes[i] == b'-';
        let key_start = if negated { i + 1 } else { i };
        let mut j = key_start;
        while j < bytes.len() && bytes[j] != b':' && !bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b':' {
            let key = q[key_start..j].to_ascii_lowercase();
            if KEYS.contains(&key.as_str()) {
                j += 1;
                let value = if j < bytes.len() && bytes[j] == b'"' {
                    j += 1;
                    let v0 = j;
                    while j < bytes.len() && bytes[j] != b'"' {
                        j += 1;
                    }
                    let v = q[v0..j].to_string();
                    if j < bytes.len() {
                        j += 1;
                    }
                    v
                } else {
                    let v0 = j;
                    while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    q[v0..j].to_string()
                };
                if !value.is_empty() {
                    found.push((key, value, negated));
                }
                i = j;
                continue;
            }
        }
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if !rest.is_empty() {
            rest.push(' ');
        }
        rest.push_str(&q[start..i]);
    }
    (rest, found)
}

/// Case-insensitive name equality for one engine (`COLLATE NOCASE` is
/// invalid Postgres SQL; Postgres uses `lower()`).
fn name_eq_sql(engine: DbEngine, placeholder: usize) -> String {
    match engine {
        DbEngine::Sqlite => format!("name = ${placeholder} COLLATE NOCASE"),
        DbEngine::Postgres => format!("lower(name) = lower(${placeholder})"),
    }
}

fn involves_people_group_sql(engine: DbEngine, exclude: bool, placeholder: usize) -> String {
    let exists = if exclude { "NOT EXISTS" } else { "EXISTS" };
    format!(
        "{exists} (
           SELECT 1 FROM contact_group_members cgm
           JOIN contact_groups cg ON cg.id = cgm.group_id
           JOIN contact_handles ch ON ch.contact_id = cgm.contact_id
             AND ch.account_id = c.account_id
           WHERE cg.account_id = c.account_id
             AND {name_eq}
             AND (
               ch.handle_id = c.chat_handle_id
               OR EXISTS (
                 SELECT 1 FROM participants p
                 WHERE p.conversation_id = c.id AND p.handle_id = ch.handle_id
               )
             )
         )",
        name_eq = name_eq_sql(engine, placeholder)
    )
}

fn has_thread_tag_sql(engine: DbEngine, exclude: bool, placeholder: usize) -> String {
    let exists = if exclude { "NOT EXISTS" } else { "EXISTS" };
    format!(
        "{exists} (
           SELECT 1 FROM conversation_tag_members ctm
           JOIN conversation_tags ct ON ct.id = ctm.tag_id
           WHERE ctm.conversation_id = c.id
             AND ct.account_id = c.account_id
             AND {name_eq}
         )",
        name_eq = name_eq_sql(engine, placeholder)
    )
}

/// Parse space-separated tokens from `q`.
///
/// Recognized tokens: `is:trash`, `is:direct`, `is:group`, `handle:<raw>`,
/// `service:phone` / `service:whatsapp` (only combined with `handle:`),
/// `contact:<id>`, `import:<id>`, `participants:=N` / `:>N` / `:<N`,
/// `people:` / `-people:`, `tag:` / `-tag:`. Remaining tokens become
/// a free-text filter.
fn parse_conversation_list_query(q: &str) -> ConversationListQuery {
    let mut out = ConversationListQuery::default();
    let mut text_parts: Vec<&str> = Vec::new();
    let (remainder, named) = pull_named_ops(q);
    for (key, value, negated) in named {
        match key.as_str() {
            "tag" => {
                if value.eq_ignore_ascii_case("none") {
                    out.no_tag = true;
                } else if negated {
                    out.exclude_tag = Some(value);
                } else {
                    out.tag = Some(value);
                }
            }
            "people" | "within" | "label" => {
                if negated {
                    out.exclude_people = Some(value);
                } else {
                    out.people = Some(value);
                }
            }
            _ => {}
        }
    }

    for token in remainder.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if lower == "is:trash" {
            out.trash_only = true;
        } else if lower == "is:direct" {
            out.type_filter = Some(ConversationTypeFilter::Direct);
        } else if lower == "is:group" {
            out.type_filter = Some(ConversationTypeFilter::Group);
        } else if let Some(rest) = token
            .strip_prefix("handle:")
            .or_else(|| token.strip_prefix("HANDLE:"))
        {
            let rest = rest.trim().trim_matches('"');
            if !rest.is_empty() {
                out.handle = Some(rest.to_string());
            }
        } else if let Some(rest) = lower.strip_prefix("service:") {
            let rest = rest.trim().trim_matches('"');
            if rest == "phone" || rest == "whatsapp" {
                out.service = Some(rest.to_string());
            }
        } else if lower.starts_with("participants:") {
            if let Some((_, value)) = token.split_once(':')
                && let Some(cmp) = parse_participants_comparison(value)
            {
                out.participants = Some(cmp);
            }
        } else if let Some(rest) = lower.strip_prefix("import:") {
            if let Ok(id) = rest.trim().parse::<i64>()
                && id > 0
            {
                out.import_id = Some(id);
            }
        } else if let Some((_, id_part)) = token.split_once(':') {
            if lower.starts_with("contact:") {
                if let Ok(id) = id_part.trim().parse::<i64>() {
                    out.contact_id = Some(id);
                }
            } else {
                text_parts.push(token);
            }
        } else {
            text_parts.push(token);
        }
    }

    let text = text_parts.join(" ");
    if !text.is_empty() {
        out.text = Some(text);
    }
    out
}

/// List conversations for the account, newest first (paged).
///
/// Supported `q` tokens (combinable except free text with structured filters):
/// - empty / whitespace: all non-trashed conversations with at least one message
/// - `is:trash`: only trashed conversations
/// - `handle:<raw>`: conversations involving that handle (chat or participant)
/// - `service:phone` / `service:whatsapp`: with `handle:`, restrict to that platform
/// - `contact:<id>`: conversations involving any handle of that contact
/// - `import:<id>`: conversations with at least one message from that import session
/// - `people:<name>` / `-people:<name>`: involve (or hide) a contact group
/// - `tag:<name>` / `-tag:<name>`: have (or hide) a thread tag
/// - `is:direct` / `is:group`: restrict by conversation type
/// - other text: case-insensitive match on group title or participant handle/name
///
/// # Errors
///
/// Returns a bad-request error for an invalid query, or an internal error when
/// a database statement fails.
pub async fn list_conversations(
    conn: &mut AnyConnection,
    account_id: &str,
    q: &str,
    limit: usize,
    offset: usize,
) -> Result<ConversationListPage, ExportQueryError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    if offset > MAX_LIST_OFFSET {
        return Err(ExportQueryError::bad(format!(
            "offset exceeds maximum of {MAX_LIST_OFFSET}"
        )));
    }

    crate::search_query::validate_list_search_query(q)?;
    let parsed = parse_conversation_list_query(q.trim());
    let engine = engine_of(conn);

    let mut where_parts = vec!["c.account_id = $1".to_string()];
    let mut params: Vec<Bind> = vec![Bind::Text(account_id.to_string())];

    if parsed.trash_only {
        // Match normal-list exclusion: conversation trash OR chat-handle trash.
        where_parts.push(
            "(EXISTS (
               SELECT 1 FROM trashed_conversations tc
               WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
             )
             OR EXISTS (
               SELECT 1 FROM trashed_handles th
               WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id
             ))"
            .into(),
        );
    } else {
        where_parts.push(crate::contacts_api::NOT_TRASHED_CONVERSATION_SQL.into());
        where_parts.push(crate::contacts_api::NOT_TRASHED_CHAT_HANDLE_SQL.into());
    }

    // Only show threads that have at least one non-duplicate message, except when
    // filtering by import session (duplicate-only threads may still belong to that import).
    if parsed.import_id.is_none() {
        where_parts.push(
            "EXISTS (
               SELECT 1 FROM messages m0
               WHERE m0.conversation_id = c.id AND m0.duplicate_of IS NULL
             )"
            .into(),
        );
    }

    if let Some(ref handle) = parsed.handle {
        if let Some(ref service) = parsed.service {
            let n = params.len() + 1;
            where_parts.push(format!(
                "(
                    (hc.raw = ${n} AND lower(hc.service) = lower(${n1}))
                    OR EXISTS (
                        SELECT 1 FROM participants p
                        JOIN handles ph ON ph.id = p.handle_id
                        WHERE p.conversation_id = c.id
                          AND ph.raw = ${n2}
                          AND lower(ph.service) = lower(${n3})
                    )
                  )",
                n = n,
                n1 = n + 1,
                n2 = n + 2,
                n3 = n + 3
            ));
            params.push(Bind::Text(handle.clone()));
            params.push(Bind::Text(service.clone()));
            params.push(Bind::Text(handle.clone()));
            params.push(Bind::Text(service.clone()));
        } else {
            let n = params.len() + 1;
            where_parts.push(format!(
                "(hc.raw = ${n} OR EXISTS (
                    SELECT 1 FROM participants p
                    JOIN handles ph ON ph.id = p.handle_id
                    WHERE p.conversation_id = c.id AND ph.raw = ${n1}
                  ))",
                n = n,
                n1 = n + 1
            ));
            params.push(Bind::Text(handle.clone()));
            params.push(Bind::Text(handle.clone()));
        }
    }

    if let Some(contact_id) = parsed.contact_id {
        let n = params.len() + 1;
        where_parts.push(crate::contacts_api::involves_contact_expr(&format!("${n}")));
        params.push(Bind::Int(contact_id));
    }

    match parsed.type_filter {
        Some(ConversationTypeFilter::Direct) => {
            where_parts.push("c.conversation_type = 'individual'".into());
        }
        Some(ConversationTypeFilter::Group) => {
            where_parts.push("c.conversation_type = 'group'".into());
        }
        None => {}
    }

    if let Some(ref cmp) = parsed.participants {
        let n = params.len() + 1;
        where_parts.push(format!(
            "(SELECT COUNT(*) FROM participants pcnt WHERE pcnt.conversation_id = c.id) {} ${n}",
            cmp.comparator.as_str()
        ));
        params.push(Bind::Int(cmp.value as i64));
    }

    if let Some(ref people) = parsed.people {
        let n = params.len() + 1;
        where_parts.push(involves_people_group_sql(engine, false, n));
        params.push(Bind::Text(people.clone()));
    }
    if let Some(ref people) = parsed.exclude_people {
        let n = params.len() + 1;
        where_parts.push(involves_people_group_sql(engine, true, n));
        params.push(Bind::Text(people.clone()));
    }
    if let Some(ref tag) = parsed.tag {
        let n = params.len() + 1;
        where_parts.push(has_thread_tag_sql(engine, false, n));
        params.push(Bind::Text(tag.clone()));
    }
    if let Some(ref tag) = parsed.exclude_tag {
        let n = params.len() + 1;
        where_parts.push(has_thread_tag_sql(engine, true, n));
        params.push(Bind::Text(tag.clone()));
    }
    if parsed.no_tag {
        where_parts.push(
            "NOT EXISTS (
               SELECT 1 FROM conversation_tag_members ctm
               JOIN conversation_tags ct ON ct.id = ctm.tag_id
               WHERE ctm.conversation_id = c.id AND ct.account_id = c.account_id
             )"
            .into(),
        );
    }

    if let Some(import_id) = parsed.import_id {
        let n = params.len() + 1;
        where_parts.push(format!(
            "EXISTS (
               SELECT 1 FROM messages m
               WHERE m.conversation_id = c.id
                 AND m.account_id = c.account_id
                 AND m.import_id = ${n}
             )"
        ));
        params.push(Bind::Int(import_id));
    }

    if let Some(ref text) = parsed.text {
        let n = params.len() + 1;
        where_parts.push(format!(
            "(c.group_title {} OR hc.raw {} OR EXISTS (
                SELECT 1 FROM participants p
                JOIN handles ph ON ph.id = p.handle_id
                LEFT JOIN contacts ct ON ct.id = p.contact_id
                WHERE p.conversation_id = c.id
                  AND (
                    ph.raw {}
                    OR coalesce(p.name_alias, '') {}
                    OR coalesce(ct.preferred_name, '') {}
                  )
              ))",
            like_ci_numbered(engine, n),
            like_ci_numbered(engine, n + 1),
            like_ci_numbered(engine, n + 2),
            like_ci_numbered(engine, n + 3),
            like_ci_numbered(engine, n + 4),
        ));
        let like = format!("%{text}%");
        for _ in 0..5 {
            params.push(Bind::Text(like.clone()));
        }
    }

    let where_sql = where_parts.join(" AND ");

    let count_sql = format!(
        "SELECT COUNT(*)
         FROM conversations c
         JOIN handles hc ON hc.id = c.chat_handle_id
         WHERE {where_sql}"
    );
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    for p in &params {
        match p {
            Bind::Text(v) => count_q = count_q.bind(v.clone()),
            Bind::Int(v) => count_q = count_q.bind(*v),
        }
    }
    let total: i64 = count_q.fetch_one(&mut *conn).await?;
    let total = total.max(0) as u64;

    let page_n = params.len() + 1;
    let sql = format!(
        "SELECT c.id,
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
         FROM conversations c
         JOIN handles hc ON hc.id = c.chat_handle_id
         WHERE {where_sql}
         ORDER BY last_message_at DESC, c.id DESC
         LIMIT ${page_n} OFFSET ${page_n_plus}",
        page_n = page_n,
        page_n_plus = page_n + 1
    );
    let mut page_q = sqlx::query_as::<_, RawConversationRow>(&sql);
    for p in &params {
        match p {
            Bind::Text(v) => page_q = page_q.bind(v.clone()),
            Bind::Int(v) => page_q = page_q.bind(*v),
        }
    }
    let rows: Vec<RawConversationRow> = page_q
        .bind(limit as i64)
        .bind(offset as i64)
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
    let mut participants = load_participants(conn, &ids).await?;
    let source_sets = load_conversation_sources(conn, &ids).await?;
    let mut tag_sets = crate::thread_tags_api::tags_for_conversations(conn, account_id, &ids)
        .await
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

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
            chat_handle_as_participant(conn, row.id).await?
        } else {
            parts
        };
        out.push(ConversationSummary {
            id: row.id.to_string(),
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
    Ok(ConversationListPage {
        conversations: out,
        total,
        limit,
        offset,
    })
}

async fn chat_handle_as_participant(
    conn: &mut AnyConnection,
    conversation_id: i64,
) -> Result<Vec<ConversationParticipant>, ExportQueryError> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT h.raw,
                h.service,
                h.handle_type
         FROM conversations c
         JOIN handles h ON h.id = c.chat_handle_id
         WHERE c.id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(match row {
        Some((handle, service, handle_type)) => vec![ConversationParticipant {
            name: None,
            name_alias: None,
            handle,
            service: if service.trim().is_empty() {
                handle_type
            } else {
                service
            },
            contact_id: None,
        }],
        None => Vec::new(),
    })
}

async fn load_participants(
    conn: &mut AnyConnection,
    conversation_ids: &[i64],
) -> Result<HashMap<i64, Vec<ConversationParticipant>>, ExportQueryError> {
    // Join contact preferred_name / name_alias here so the list path does not
    // issue one follow-up SELECT per participant. Contact fields apply only when
    // `p.contact_id` links the same handle; otherwise residue `p.name_alias` is
    // exposed as `name` and `name_alias` stays unset.
    group_rows_by_id(
        conn,
        conversation_ids,
        |placeholders| {
            format!(
                "SELECT p.conversation_id,
                    CASE
                      WHEN NULLIF(trim(c.preferred_name), '') IS NOT NULL
                        THEN NULLIF(trim(c.preferred_name), '')
                      ELSE NULLIF(trim(p.name_alias), '')
                    END AS name,
                    NULLIF(trim(ch.name_alias), '') AS name_alias,
                    h.raw,
                    coalesce(nullif(trim(h.service), ''), h.handle_type),
                    p.contact_id
             FROM participants p
             JOIN handles h ON h.id = p.handle_id
             JOIN conversations conv ON conv.id = p.conversation_id
             LEFT JOIN contact_handles ch
               ON ch.contact_id = p.contact_id
              AND ch.account_id = conv.account_id
              AND ch.handle_id = p.handle_id
             LEFT JOIN contacts c
               ON c.id = ch.contact_id AND c.account_id = conv.account_id
             WHERE p.conversation_id IN ({placeholders})
             ORDER BY p.conversation_id, p.id"
            )
        },
        |row| {
            let contact_id: Option<i64> = row.try_get(5)?;
            Ok((
                row.try_get::<i64, _>(0)?,
                ConversationParticipant {
                    name: row.try_get(1)?,
                    name_alias: row.try_get(2)?,
                    handle: row.try_get(3)?,
                    service: row
                        .try_get::<String, _>(4)
                        .unwrap_or_else(|_| "unknown".into()),
                    contact_id: contact_id.map(|id| id.to_string()),
                },
            ))
        },
    )
    .await
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
) -> Result<HashMap<i64, Vec<String>>, ExportQueryError> {
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
    pub sources: Vec<ConversationSourceInfo>,
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
) -> Result<Option<ConversationSourcesPage>, ExportQueryError> {
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
    Ok(Some(ConversationSourcesPage { sources }))
}

/// Page through conversations (newest first) with participants, message
/// counts, and tags.
#[utoipa::path(
    get,
    path = "/v1/export/conversations",
    tag = "Conversations",
    security(("bearer" = [])),
    params(
        ("q" = Option<String>, Query, description = "Conversation search; empty lists all non-trashed"),
        ("limit" = Option<usize>, Query, description = "Page size"),
        ("offset" = Option<usize>, Query, description = "Page offset")
    ),
    responses(
        (status = 200, body = crate::conversations_api::ConversationListPage),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn conversations_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<crate::server::ListPageQuery>,
) -> Result<Json<ConversationListPage>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let q = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let page = list_conversations(&mut conn, &auth.account_id, &q, limit, offset).await?;
    Ok(Json(page))
}

/// Per-backup message counts for one conversation (the Sources panel).
#[utoipa::path(
    get,
    path = "/v1/export/conversations/{id}/sources",
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
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<i64>,
) -> Result<Json<ConversationSourcesPage>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let page = list_conversation_source_stats(&mut conn, &auth.account_id, conversation_id).await?;
    page.map(Json)
        .ok_or_else(|| ApiError::NotFound("conversation not found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::HandleType;

    use crate::db::{account_profile, engine, schema, vault_imports};
    use crate::search_query::CountComparator;

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String) {
        let (pool, dir) = engine::test_pool().await;
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
        (pool, dir, account)
    }

    #[tokio::test]
    async fn list_conversations_returns_summary() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let page = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.conversations.len(), 1);
        assert_eq!(page.conversations[0].id, "1");
        assert_eq!(page.conversations[0].message_count, 1);
        assert!(!page.conversations[0].is_group);
        assert_eq!(page.conversations[0].participants.len(), 1);
        assert_eq!(page.conversations[0].participants[0].handle, "+15555550200");
    }

    #[tokio::test]
    async fn is_trash_includes_handle_trashed_conversations() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let handle_id: i64 =
            sqlx::query_scalar("SELECT chat_handle_id FROM conversations WHERE id = 1")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        sqlx::query("INSERT INTO trashed_handles (account_id, handle_id) VALUES ($1, $2)")
            .bind(&account)
            .bind(handle_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let normal = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(normal.total, 0, "handle-trashed threads leave the inbox");

        let trash = list_conversations(&mut conn, &account, "is:trash", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(
            trash.total, 1,
            "is:trash should include handle-trashed threads"
        );
        assert_eq!(trash.conversations[0].id, "1");
    }

    #[tokio::test]
    async fn list_conversations_filters_by_handle() {
        let (pool, _dir, account) = setup().await;
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
        assert_eq!(hit.conversations.len(), 1);
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
        assert!(miss.conversations.is_empty());
    }

    #[tokio::test]
    async fn list_conversations_filters_by_handle_and_service() {
        let (pool, _dir, account) = setup().await;
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

        let phone_only = list_conversations(
            &mut conn,
            &account,
            "handle:+15555550200 service:phone",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(phone_only.total, 1);
        assert_eq!(phone_only.conversations[0].id, "1");

        let wa_only = list_conversations(
            &mut conn,
            &account,
            "handle:+15555550200 service:whatsapp",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(wa_only.total, 1);
        assert_eq!(wa_only.conversations[0].id, "10");

        let lone_service = list_conversations(
            &mut conn,
            &account,
            "service:whatsapp",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        let all = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(lone_service.total, all.total);
    }

    #[tokio::test]
    async fn list_conversations_paginates() {
        let (pool, _dir, account) = setup().await;
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
        assert_eq!(page0.conversations.len(), 1);
        assert_eq!(page0.conversations[0].id, "2"); // newer first

        let page1 = list_conversations(&mut conn, &account, "", 1, 1)
            .await
            .unwrap();
        assert_eq!(page1.total, 2);
        assert_eq!(page1.offset, 1);
        assert_eq!(page1.conversations.len(), 1);
        assert_eq!(page1.conversations[0].id, "1");

        let by_text = list_conversations(&mut conn, &account, "5555550300", 10, 0)
            .await
            .unwrap();
        assert_eq!(by_text.total, 1);
        assert_eq!(by_text.conversations[0].id, "2");

        let clamped = list_conversations(&mut conn, &account, "", MAX_LIST_LIMIT + 50, 0)
            .await
            .unwrap();
        assert_eq!(clamped.limit, MAX_LIST_LIMIT);
        assert_eq!(clamped.total, 2);
    }

    #[tokio::test]
    async fn list_queries_enforce_search_limits() {
        let (pool, _dir, account) = setup().await;
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
                crate::contacts_api::DEFAULT_LIST_LIMIT,
                0,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(contact_error, ExportQueryError::BadRequest(_)),
                "contact query should be rejected: {query}"
            );

            let conversation_error =
                list_conversations(&mut conn, &account, query, DEFAULT_LIST_LIMIT, 0)
                    .await
                    .unwrap_err();
            assert!(
                matches!(conversation_error, ExportQueryError::BadRequest(_)),
                "conversation query should be rejected: {query}"
            );
        }
    }

    #[tokio::test]
    async fn list_queries_accept_literal_boolean_words_and_parentheses() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        for query in [
            "OR", "AND", "NOT", "foo OR", "foo AND", "foo NOT", "(", ")", "(foo", "foo)",
        ] {
            crate::contacts_api::list_contacts(
                &mut conn,
                &account,
                query,
                crate::contacts_api::DEFAULT_LIST_LIMIT,
                0,
            )
            .await
            .unwrap();

            list_conversations(&mut conn, &account, query, DEFAULT_LIST_LIMIT, 0)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn malformed_boolean_queries_are_bad_requests_for_export() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        for query in ["foo OR", "(foo OR bar", "foo OR bar)"] {
            let export_error = crate::export_api::export_message_count(
                &mut conn,
                crate::export_api::ExportCountOpts {
                    account_id: &account,
                    query,
                    source_override: None,
                },
            )
            .await
            .unwrap_err();
            assert!(matches!(export_error, ExportQueryError::BadRequest(_)));
        }
    }

    #[tokio::test]
    async fn list_conversations_filters_by_contact_and_type() {
        let (pool, _dir, account) = setup().await;
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
            &format!("contact:{contact_id}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(all.total, 2);
        let ids: Vec<&str> = all.conversations.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"1"));
        assert!(ids.contains(&"3"));

        let direct = list_conversations(
            &mut conn,
            &account,
            &format!("contact:{contact_id} is:direct"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(direct.total, 1);
        assert_eq!(direct.conversations[0].id, "1");
        assert!(!direct.conversations[0].is_group);

        let groups = list_conversations(
            &mut conn,
            &account,
            &format!("contact:{contact_id} is:group"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(groups.total, 1);
        assert_eq!(groups.conversations[0].id, "3");
        assert!(groups.conversations[0].is_group);
    }

    #[test]
    fn parse_conversation_list_query_tokens() {
        let q = parse_conversation_list_query(
            "contact:42 is:direct handle:+15555550100 service:whatsapp",
        );
        assert_eq!(q.contact_id, Some(42));
        assert_eq!(q.type_filter, Some(ConversationTypeFilter::Direct));
        assert_eq!(q.handle.as_deref(), Some("+15555550100"));
        assert_eq!(q.service.as_deref(), Some("whatsapp"));
        assert!(q.text.is_none());
        assert!(!q.trash_only);

        let trash = parse_conversation_list_query("is:trash");
        assert!(trash.trash_only);

        let parts = parse_conversation_list_query("is:group participants:>3");
        assert_eq!(parts.type_filter, Some(ConversationTypeFilter::Group));
        assert_eq!(
            parts.participants,
            Some(CountComparison {
                comparator: CountComparator::Gt,
                value: 3,
            })
        );

        let eq_bare = parse_conversation_list_query("participants:5");
        assert_eq!(
            eq_bare.participants,
            Some(CountComparison {
                comparator: CountComparator::Eq,
                value: 5,
            })
        );

        let eq_prefix = parse_conversation_list_query("participants:=8");
        assert_eq!(
            eq_prefix.participants,
            Some(CountComparison {
                comparator: CountComparator::Eq,
                value: 8,
            })
        );

        let lt = parse_conversation_list_query("participants:<10");
        assert_eq!(
            lt.participants,
            Some(CountComparison {
                comparator: CountComparator::Lt,
                value: 10,
            })
        );

        let quoted_handle = parse_conversation_list_query(r#"handle:"+15555550100""#);
        assert_eq!(quoted_handle.handle.as_deref(), Some("+15555550100"));
    }

    #[tokio::test]
    async fn list_conversations_enriches_participant_names_from_contact() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // setup() participant residue is name_alias 'Sam' on +15555550200.
        sqlx::query(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam Preferred')",
        )
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
            "INSERT INTO contact_handles (account_id, handle_id, contact_id, name_alias)
             VALUES ($1, $2, $3, 'Sammy')",
        )
        .bind(&account)
        .bind(peer_handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE participants SET contact_id = $1 WHERE conversation_id = 1 AND handle_id = $2",
        )
        .bind(contact_id)
        .bind(peer_handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = list_conversations(&mut conn, &account, "", 10, 0)
            .await
            .unwrap();
        assert_eq!(page.conversations.len(), 1);
        let p = &page.conversations[0].participants[0];
        assert_eq!(p.handle, "+15555550200");
        assert_eq!(p.name.as_deref(), Some("Sam Preferred"));
        assert_eq!(p.name_alias.as_deref(), Some("Sammy"));
        assert_eq!(p.contact_id, Some(contact_id.to_string()));
    }

    #[tokio::test]
    async fn list_conversations_keeps_participant_residue_name_without_contact() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let page = list_conversations(&mut conn, &account, "", 10, 0)
            .await
            .unwrap();
        let p = &page.conversations[0].participants[0];
        // No contact_id → residue `participants.name_alias` is exposed as `name`.
        assert_eq!(p.name.as_deref(), Some("Sam"));
        assert_eq!(p.name_alias, None);
        assert_eq!(p.contact_id, None);
    }

    #[tokio::test]
    async fn list_conversations_keeps_residue_when_linked_contact_has_empty_preferred_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO contacts (account_id, preferred_name) VALUES ($1, '')")
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
            "INSERT INTO contact_handles (account_id, handle_id, contact_id, name_alias)
             VALUES ($1, $2, $3, 'Sammy')",
        )
        .bind(&account)
        .bind(peer_handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE participants SET contact_id = $1 WHERE conversation_id = 1 AND handle_id = $2",
        )
        .bind(contact_id)
        .bind(peer_handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = list_conversations(&mut conn, &account, "", 10, 0)
            .await
            .unwrap();
        let p = &page.conversations[0].participants[0];
        assert_eq!(p.name.as_deref(), Some("Sam"));
        assert_eq!(p.name_alias.as_deref(), Some("Sammy"));
    }

    #[tokio::test]
    async fn list_conversations_matches_contact_preferred_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam Preferred')",
        )
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
            "UPDATE participants SET contact_id = $1, name_alias = NULL
             WHERE conversation_id = '1' AND handle_id = $2",
        )
        .bind(contact_id)
        .bind(peer_handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let by_name = list_conversations(&mut conn, &account, "Sam Preferred", 10, 0)
            .await
            .unwrap();
        assert_eq!(by_name.total, 1);
        assert_eq!(by_name.conversations[0].id, "1");
    }

    #[test]
    fn parse_participants_comparison_values() {
        assert_eq!(
            parse_participants_comparison(">=2").unwrap(),
            CountComparison {
                comparator: CountComparator::Gte,
                value: 2,
            }
        );
        assert!(parse_participants_comparison("").is_none());
        assert!(parse_participants_comparison("abc").is_none());
        assert!(parse_participants_comparison(">").is_none());
    }

    #[tokio::test]
    async fn list_conversations_filters_by_participant_count() {
        let (pool, _dir, account) = setup().await;
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
        assert_eq!(eq2.conversations[0].id, "10");

        let gt1 = list_conversations(&mut conn, &account, "participants:>1", 50, 0)
            .await
            .unwrap();
        assert_eq!(gt1.total, 1);
        assert_eq!(gt1.conversations[0].id, "10");

        let eq1 = list_conversations(&mut conn, &account, "participants:1", 50, 0)
            .await
            .unwrap();
        assert_eq!(eq1.total, 1);
        assert_eq!(eq1.conversations[0].id, "1");

        let lt2 = list_conversations(&mut conn, &account, "is:group participants:<2", 50, 0)
            .await
            .unwrap();
        assert_eq!(lt2.total, 0);
    }

    #[tokio::test]
    async fn list_conversations_participants_eq_on_demo_fixture_db() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../data/vault.db");
        if !path.is_file() {
            eprintln!("skip — missing {}", path.display());
            return;
        }
        let pool = engine::open_pool_for_path(&path).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let account = "00000000-0000-0000-0000-00000000d001";
        let page = list_conversations(&mut conn, account, "participants:=3", 50, 0)
            .await
            .unwrap();
        assert!(
            page.total >= 1,
            "demo db should have conversations with 3 participants; total={}",
            page.total
        );
        assert!(
            page.conversations.iter().all(|c| c.participants.len() == 3),
            "every returned conversation should have 3 participants"
        );
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
            &account,
            "imessage-ios",
            "append",
            Some("test"),
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
            &account,
            "imessage-ios",
            "append",
            Some("test"),
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
            &format!("import:{import_a}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(a.total, 1);
        assert_eq!(a.conversations[0].id, "1");

        let b = list_conversations(
            &mut conn,
            &account,
            &format!("import:{import_b}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(b.total, 1);
        assert_eq!(b.conversations[0].id, "2");

        let missing =
            list_conversations(&mut conn, &account, "import:999999", DEFAULT_LIST_LIMIT, 0)
                .await
                .unwrap();
        assert_eq!(missing.total, 0);

        let junk = list_conversations(
            &mut conn,
            &account,
            "import:not-a-number",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        let all = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(junk.total, all.total);
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
            &account,
            "imessage-ios",
            "append",
            Some("test"),
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
            &format!("import:{import_a}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(
            by_import.total, 1,
            "import filter should match duplicate-only thread"
        );
        assert_eq!(by_import.conversations[0].id, "3");

        let all = list_conversations(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(
            all.total, 1,
            "default list still requires a non-duplicate message"
        );
        assert_eq!(all.conversations[0].id, "4");
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
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        crate::thread_tags_api::set_conversations_tag_membership(
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
        assert_eq!(tagged.conversations[0].tags, vec!["Holiday".to_string()]);
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
        crate::contact_groups_api::set_contacts_group_membership(
            &mut conn,
            &account,
            &[contact_id],
            "Family",
            true,
        )
        .await
        .unwrap();
        let family =
            list_conversations(&mut conn, &account, "people:Family", DEFAULT_LIST_LIMIT, 0)
                .await
                .unwrap();
        assert_eq!(family.total, 1);
        let not_family =
            list_conversations(&mut conn, &account, "-people:Family", DEFAULT_LIST_LIMIT, 0)
                .await
                .unwrap();
        assert_eq!(not_family.total, 0);
    }
}
