//! Read-only message export query used by `GET /v1/export/messages`
//! and `GET /v1/export/messages/count`.

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::db::sql::group_rows_by_id;
#[cfg(test)]
use crate::search_query::MAX_SEARCH_QUERY_BYTES;
use crate::search_query::{FtsNode, ParsedSearchQuery, SearchMode, validate_search_query};

pub const DEFAULT_EXPORT_LIMIT: usize = 100;
pub const MAX_EXPORT_LIMIT: usize = 500;
/// Cap expensive OFFSET skips (prefer cursor pagination for deep pages).
pub const MAX_EXPORT_OFFSET: usize = 50_000;
#[derive(Debug, Clone)]
pub struct ExportPageOpts<'a> {
    pub account_id: &'a str,
    pub query: &'a str,
    pub limit: usize,
    pub offset: Option<usize>,
    pub cursor: Option<&'a str>,
    pub source_override: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ExportCountOpts<'a> {
    pub account_id: &'a str,
    pub query: &'a str,
    pub source_override: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct ExportMessagesResponse {
    pub ok: bool,
    pub query: String,
    pub messages: Vec<ExportMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ExportCountResponse {
    pub ok: bool,
    pub query: String,
    pub messages: u64,
    /// Distinct conversations with at least one matching message.
    pub conversations: u64,
    /// Unique attachment fingerprints among matching messages.
    pub attachments: u64,
    /// Sum of known `size_bytes` for those unique fingerprints (unknown sizes omitted).
    pub total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct ExportMessage {
    pub id: i64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub guid: Option<String>,
    pub timestamp: String,
    pub timestamp_utc: Option<String>,
    pub sort_order: i64,
    pub is_from_me: bool,
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub is_announcement: bool,
    pub is_reply: bool,
    pub thread_originator_guid: Option<String>,
    pub thread_originator_part: Option<i64>,
    pub num_replies: i64,
    pub conversation: ExportConversation,
    pub attachments: Vec<ExportAttachment>,
    pub tapbacks: Vec<ExportTapback>,
}

#[derive(Debug, Serialize)]
pub struct ExportConversation {
    pub id: i64,
    pub chat_identifier: String,
    pub conversation_type: String,
    pub group_title: Option<String>,
    pub participants: Vec<ExportParticipant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportParticipant {
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<i64>,
    pub handle_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportAttachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_sticker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportTapback {
    pub part_index: i64,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    pub is_from_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
}

#[derive(Debug)]
pub enum ExportQueryError {
    BadRequest(String),
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

impl From<rusqlite::Error> for ExportQueryError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl ExportQueryError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
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
    params: Vec<rusqlite::types::Value>,
    dedupe_sql: String,
}

fn unique_ids(ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn nonempty_source(source: Option<&str>) -> Option<&str> {
    let Some(raw) = source else {
        return None;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Build WHERE-clause SQL for a validated message-mode search.
fn prepare_message_export(
    conn: &Connection,
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
    build_message_filters(conn, account_id, &parsed, source_override)
}

/// Export messages matching a Fastmail-style query (message mode only).
///
/// Empty query (no criteria) returns all non-trashed, non-duplicate messages for the account.
///
/// # Errors
///
/// Returns a bad-request error for an invalid query or cursor, or an internal
/// error when a database statement fails.
pub fn export_messages(
    conn: &Connection,
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

    let filters = prepare_message_export(conn, opts.account_id, opts.query, opts.source_override)?;
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
        params.push(cur.timestamp.clone().into());
        params.push(cur.timestamp.clone().into());
        params.push(cur.sort_order.into());
        params.push(cur.timestamp.clone().into());
        params.push(cur.sort_order.into());
        params.push(cur.id.into());
    }
    sql.push_str(" ORDER BY m.timestamp ASC, m.sort_order ASC, m.id ASC");
    if let (Some(offset), None) = (opts.offset, &cursor) {
        if offset > MAX_EXPORT_OFFSET {
            return Err(ExportQueryError::bad(format!(
                "offset exceeds maximum of {MAX_EXPORT_OFFSET}; use cursor pagination instead"
            )));
        }
        sql.push_str(" LIMIT ? OFFSET ?");
        params.push((fetch_limit as i64).into());
        params.push((offset as i64).into());
    } else {
        sql.push_str(" LIMIT ?");
        params.push((fetch_limit as i64).into());
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(params.iter().cloned()), |row| {
            Ok(RawRow {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                source: row.get(2)?,
                service: row.get(3)?,
                guid: row.get(4)?,
                timestamp: row.get(5)?,
                timestamp_utc: row.get(6)?,
                sort_order: row.get(7)?,
                is_from_me: row.get::<_, i64>(8)? != 0,
                sender: row.get(9)?,
                subject: row.get(10)?,
                body: row.get(11)?,
                is_announcement: row.get::<_, i64>(12)? != 0,
                is_reply: row.get::<_, i64>(13)? != 0,
                thread_originator_guid: row.get(14)?,
                thread_originator_part: row.get(15)?,
                num_replies: row.get(16)?,
                chat_identifier: row.get(17)?,
                conversation_type: row.get(18)?,
                group_title: row.get(19)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

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
    let participants = load_participants(conn, &conv_ids)?;
    let msg_ids: Vec<i64> = page_rows.iter().map(|r| r.id).collect();
    let attachments = load_attachments(conn, &msg_ids)?;
    let tapbacks = load_tapbacks(conn, &msg_ids)?;

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
pub fn export_message_count(
    conn: &Connection,
    opts: ExportCountOpts<'_>,
) -> Result<ExportCountResponse, ExportQueryError> {
    let filters = prepare_message_export(conn, opts.account_id, opts.query, opts.source_override)?;

    let msg_sql = format!(
        "SELECT COUNT(*)
         {messages_from_sql}
         WHERE {where_sql}{dedupe}",
        messages_from_sql = messages_from_sql(),
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );
    let messages: i64 = conn.query_row(
        &msg_sql,
        params_from_iter(filters.params.iter().cloned()),
        |row| row.get(0),
    )?;

    let conv_sql = format!(
        "SELECT COUNT(DISTINCT c.id)
         {messages_from_sql}
         WHERE {where_sql}{dedupe}",
        messages_from_sql = messages_from_sql(),
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );
    let conversations: i64 = conn.query_row(
        &conv_sql,
        params_from_iter(filters.params.iter().cloned()),
        |row| row.get(0),
    )?;

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
    let (attachments, total_bytes): (i64, i64) = conn.query_row(
        &att_sql,
        params_from_iter(filters.params.iter().cloned()),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

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
    params: &mut Vec<rusqlite::types::Value>,
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
    params.push(like.clone().into());
    params.push(like.into());
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

fn build_message_filters(
    conn: &Connection,
    account_id: &str,
    parsed: &ParsedSearchQuery,
    source_override: Option<&str>,
) -> Result<BuiltFilters, ExportQueryError> {
    reject_unimplemented_message_filters(parsed)?;

    let mut where_parts = vec!["c.account_id = ?".to_string()];
    let mut params: Vec<rusqlite::types::Value> = vec![account_id.to_string().into()];

    append_metadata_text_filters(parsed, &mut where_parts, &mut params)?;

    if let Some(conv) = &parsed.in_conversation {
        match conv.parse::<i64>() {
            Ok(id) => {
                where_parts.push("c.id = ?".into());
                params.push(id.into());
            }
            Err(_) => {
                where_parts.push("hc.raw = ?".into());
                params.push(conv.clone().into());
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
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.into());
    }

    if let Some(to) = &parsed.to {
        push_participant_handle_or_alias_like(&mut where_parts, &mut params, to);
    }
    if let Some(with_person) = &parsed.with_person {
        push_participant_handle_or_alias_like(&mut where_parts, &mut params, with_person);
    }

    if let Some(subject) = &parsed.subject {
        where_parts.push("coalesce(m.subject, '') LIKE ? COLLATE NOCASE".into());
        params.push(format!("%{subject}%").into());
    }

    if let Some(after) = &parsed.after {
        where_parts.push("m.timestamp >= ?".into());
        params.push(after.clone().into());
    }
    if let Some(before) = &parsed.before {
        where_parts.push("m.timestamp < ?".into());
        let before_val = if before.len() == 10 {
            format!("{before}T23:59:59.999Z")
        } else {
            before.clone()
        };
        params.push(before_val.into());
    }

    let source_filter = nonempty_source(source_override).or(parsed.source.as_deref());
    if let Some(source) = source_filter {
        where_parts.push("m.source = ?".into());
        params.push(source.to_string().into());
    }

    if let Some(ct) = parsed.conversation_type {
        where_parts.push("c.conversation_type = ?".into());
        params.push(ct.to_string().into());
    }

    if parsed.has_attachment == Some(true) {
        where_parts.push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)".into());
    }

    if let Some(within) = &parsed.within {
        let ids = list_group_member_contact_ids(conn, account_id, within)?;
        where_parts.push(involves_contacts_sql(&ids));
    }
    if let Some(people) = &parsed.exclude_people {
        let ids = list_group_member_contact_ids(conn, account_id, people)?;
        where_parts.push(format!("NOT {}", involves_contacts_sql(&ids)));
    }
    if let Some(tag) = &parsed.tag {
        where_parts.push(has_thread_tag_sql(false));
        params.push(tag.clone().into());
    }
    if let Some(tag) = &parsed.exclude_tag {
        where_parts.push(has_thread_tag_sql(true));
        params.push(tag.clone().into());
    }

    if !parsed.first_contact.is_empty() {
        let ids = contact_ids_within_day_bounds(conn, account_id, "first", &parsed.first_contact)?;
        where_parts.push(involves_contacts_sql(&ids));
    }
    if !parsed.last_contact.is_empty() {
        let ids = contact_ids_within_day_bounds(conn, account_id, "last", &parsed.last_contact)?;
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
    where_parts: &mut Vec<String>,
    params: &mut Vec<rusqlite::types::Value>,
) -> Result<(), ExportQueryError> {
    if let Some(ast) = &parsed.fts_ast {
        where_parts.push(compile_metadata_fts_expr(ast, params)?);
    }
    Ok(())
}

fn compile_metadata_fts_expr(
    node: &FtsNode,
    params: &mut Vec<rusqlite::types::Value>,
) -> Result<String, ExportQueryError> {
    match node {
        FtsNode::Term { value, prefix } => {
            let fts_query = if prefix == &Some(true) {
                format!("{}*", fts5_literal_query(value))
            } else {
                fts5_literal_query(value)
            };
            Ok(metadata_term_matches_sql(params, value, &fts_query))
        }
        FtsNode::Phrase { value } => Ok(metadata_term_matches_sql(
            params,
            value,
            &fts5_literal_query(value),
        )),
        FtsNode::And { children } => compile_metadata_fts_children("AND", children, params),
        FtsNode::Or { children } => compile_metadata_fts_children("OR", children, params),
        FtsNode::Not { child } => Ok(format!(
            "(NOT ({}))",
            compile_metadata_fts_expr(child, params)?
        )),
    }
}

fn compile_metadata_fts_children(
    operator: &str,
    children: &[FtsNode],
    params: &mut Vec<rusqlite::types::Value>,
) -> Result<String, ExportQueryError> {
    if children.is_empty() {
        return Err(ExportQueryError::bad(format!(
            "{operator} search expression has no operands"
        )));
    }
    let compiled = children
        .iter()
        .map(|child| compile_metadata_fts_expr(child, params))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("({})", compiled.join(&format!(" {operator} "))))
}

fn metadata_term_matches_sql(
    params: &mut Vec<rusqlite::types::Value>,
    term: &str,
    fts_query: &str,
) -> String {
    let like = format!("%{term}%");
    for _ in 0..8 {
        params.push(like.clone().into());
    }
    params.push(fts_query.to_string().into());
    "(
    coalesce(hs.raw, '') LIKE ? COLLATE NOCASE
    OR EXISTS (
      SELECT 1 FROM participants p_md
      JOIN handles hp ON hp.id = p_md.handle_id
      WHERE p_md.conversation_id = c.id
        AND (
          hp.raw LIKE ? COLLATE NOCASE
          OR coalesce(p_md.name_alias, '') LIKE ? COLLATE NOCASE
        )
    )
    OR EXISTS (
      SELECT 1 FROM contact_handles ch_md
      JOIN contacts ct_md ON ct_md.id = ch_md.contact_id
      JOIN handles hm ON hm.id = ch_md.handle_id
      WHERE ch_md.account_id = c.account_id
        AND (
          hm.raw LIKE ? COLLATE NOCASE
          OR coalesce(ct_md.preferred_name, '') LIKE ? COLLATE NOCASE
        )
        AND (
          (c.conversation_type = 'individual' AND hm.id = c.chat_handle_id)
          OR EXISTS (
            SELECT 1 FROM participants p_md2
            WHERE p_md2.conversation_id = c.id AND p_md2.handle_id = ch_md.handle_id
          )
        )
    )
    OR EXISTS (
      SELECT 1 FROM attachments a_md
      WHERE a_md.message_id = m.id
        AND (
          coalesce(a_md.original_name, '') LIKE ? COLLATE NOCASE
          OR coalesce(a_md.mime_type, '') LIKE ? COLLATE NOCASE
          OR coalesce(a_md.derived_mime_type, '') LIKE ? COLLATE NOCASE
        )
    )
    OR EXISTS (
      SELECT 1 FROM messages_fts fts
      WHERE fts.rowid = m.id AND messages_fts MATCH ?
    )
  )"
    .into()
}

/// Quote a free-text token for full-text search so operators and punctuation are treated as literal text.
fn fts5_literal_query(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn has_thread_tag_sql(exclude: bool) -> String {
    let exists = if exclude { "NOT EXISTS" } else { "EXISTS" };
    format!(
        "{exists} (
           SELECT 1 FROM conversation_tag_members ctm
           JOIN conversation_tags ct ON ct.id = ctm.tag_id
           WHERE ctm.conversation_id = c.id
             AND ct.account_id = c.account_id
             AND ct.name = ? COLLATE NOCASE
         )"
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

fn list_group_member_contact_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, ExportQueryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT cgm.contact_id
             FROM contact_group_members cgm
             JOIN contact_groups cg ON cg.id = cgm.group_id
             WHERE cg.name = ? COLLATE NOCASE AND cg.account_id = ?
             ORDER BY cgm.contact_id",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![trimmed, account_id], |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(rows)
}

fn contact_ids_within_day_bounds(
    conn: &Connection,
    account_id: &str,
    bound: &str,
    bounds: &crate::search_query::DateBounds,
) -> Result<Vec<i64>, ExportQueryError> {
    let day = if bound == "first" { "MIN" } else { "MAX" };
    let mut having = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = vec![account_id.to_string().into()];
    if let Some(from) = &bounds.from {
        having.push(format!("{day}(substr(m.timestamp, 1, 10)) >= ?"));
        params.push(from.clone().into());
    }
    if let Some(to) = &bounds.to {
        // Exclusive upper bound: the string compare on YYYY-MM-DD already excludes the day.
        having.push(format!("{day}(substr(m.timestamp, 1, 10)) < ?"));
        params.push(to.clone().into());
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
         WHERE ch.account_id = ? AND m.duplicate_of IS NULL
         GROUP BY ch.contact_id
         HAVING {having_sql}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(params.iter().cloned()), |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(rows)
}

fn load_participants(
    conn: &Connection,
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
                row.get::<_, i64>(0)?,
                ExportParticipant {
                    handle: row.get(1)?,
                    name_alias: row.get(2)?,
                    preferred_name: row.get(3)?,
                    handle_type: row.get(4)?,
                    contact_id: row.get(5)?,
                },
            ))
        },
    )
}

fn load_attachments(
    conn: &Connection,
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
                row.get::<_, i64>(0)?,
                ExportAttachment {
                    path: row.get(1)?,
                    original_name: row.get(2)?,
                    mime_type: row.get(3)?,
                    sha256: row.get(4)?,
                    is_sticker: row.get::<_, i64>(5)? != 0,
                    transcription: row.get(6)?,
                    missing_reason: row.get(7)?,
                },
            ))
        },
    )
}

fn load_tapbacks(
    conn: &Connection,
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
                row.get::<_, i64>(0)?,
                ExportTapback {
                    part_index: row.get(1)?,
                    kind: row.get(2)?,
                    emoji: row.get(3)?,
                    is_from_me: row.get::<_, i64>(4)? != 0,
                    sender: row.get(5)?,
                },
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, params};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::schema::ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES ('a1', 'alice', 0)",
            [],
        )
        .unwrap();
        // Create handles and conversations using chat_handle_id (FK to handles).
        for (cid, phone) in [(1, "+1555"), (2, "+1666")] {
            conn.execute(
                "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                 VALUES ('a1', ?1, ?1, 'phone', 'phone')",
                params![phone],
            )
            .unwrap();
            let handle_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO conversations (id, account_id, chat_handle_id, conversation_type, source_file)
                 VALUES (?1, 'a1', ?2, 'individual', 'backup-a.jsonl')",
                params![cid, handle_id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO messages (id, conversation_id, account_id, source, service, timestamp, is_from_me, sort_order, body)
             VALUES (1, 1, 'a1', 'sms', 'sms', '2020-01-01T00:00:00Z', 0, 0, 'hello one'),
                    (2, 2, 'a1', 'sms', 'sms', '2020-01-02T00:00:00Z', 0, 0, 'hello two')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn export_includes_attachment_missing_reason() {
        let conn = setup();
        conn.execute(
            "INSERT INTO attachments (
                message_id, path, original_name, mime_type, sha256, is_sticker,
                size_bytes, missing_reason
             ) VALUES (1, 'attachments/gone.bin', 'gone.bin', 'image/png', NULL, 0, 2048, 'file_missing')",
            [],
        )
        .unwrap();

        let res = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "in:1",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].attachments.len(), 1);
        let att = &res.messages[0].attachments[0];
        assert!(att.sha256.is_none());
        assert_eq!(att.missing_reason.as_deref(), Some("file_missing"));
        assert_eq!(att.original_name.as_deref(), Some("gone.bin"));
        assert_eq!(att.mime_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn conversation_filter_scopes_messages() {
        let conn = setup();

        let res = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "in:1",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 1);
        assert_eq!(res.messages[0].service.as_deref(), Some("sms"));

        let res = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "conversation:2",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 2);

        let res = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
    }

    #[test]
    fn export_message_count_supports_handle_filters() {
        let conn = setup();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('a1', 'alice', 'alice', 'other', 'other')",
            [],
        )
        .unwrap();
        let sender_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE messages SET sender_handle_id = ?1 WHERE id = 1",
            [sender_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE handles SET raw = 'alice-chat', normalized = 'alice-chat'
             WHERE id = (SELECT chat_handle_id FROM conversations WHERE id = 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO attachments (
                message_id, path, original_name, mime_type, sha256, is_sticker, size_bytes
             ) VALUES (1, 'attachments/a.txt', 'a.txt', 'text/plain', 'abc123', 0, 12)",
            [],
        )
        .unwrap();

        for query in ["from:alice", "in:alice-chat"] {
            let counts = export_message_count(
                &conn,
                ExportCountOpts {
                    account_id: "a1",
                    query,
                    source_override: None,
                },
            )
            .unwrap();
            assert_eq!(counts.messages, 1, "query={query}");
            assert_eq!(counts.conversations, 1, "query={query}");
            assert_eq!(counts.attachments, 1, "query={query}");
        }
    }

    #[test]
    fn free_text_matches_message_body_via_fts() {
        let conn = setup();
        let res = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "one",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
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

    #[test]
    fn export_boolean_query_preserves_or() {
        let conn = setup();
        conn.execute("UPDATE messages SET body = 'foo' WHERE id = 1", [])
            .unwrap();
        conn.execute("UPDATE messages SET body = 'bar' WHERE id = 2", [])
            .unwrap();
        conn.execute(
            "INSERT INTO messages (
                id, conversation_id, account_id, source, service, timestamp,
                is_from_me, sort_order, body
             ) VALUES (
                3, 1, 'a1', 'sms', 'sms', '2020-01-03T00:00:00Z',
                0, 0, 'foo bar'
             )",
            [],
        )
        .unwrap();

        let result = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "foo OR bar",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        let ids: Vec<i64> = result.messages.iter().map(|message| message.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn export_boolean_query_preserves_and_and_not() {
        let conn = setup();
        conn.execute("UPDATE messages SET body = 'foo' WHERE id = 1", [])
            .unwrap();
        conn.execute("UPDATE messages SET body = 'bar' WHERE id = 2", [])
            .unwrap();
        conn.execute(
            "INSERT INTO messages (
                id, conversation_id, account_id, source, service, timestamp,
                is_from_me, sort_order, body
             ) VALUES (
                3, 1, 'a1', 'sms', 'sms', '2020-01-03T00:00:00Z',
                0, 0, 'foo bar'
             )",
            [],
        )
        .unwrap();

        let matching_ids = |query| {
            export_messages(
                &conn,
                ExportPageOpts {
                    account_id: "a1",
                    query,
                    limit: 100,
                    offset: None,
                    cursor: None,
                    source_override: None,
                },
            )
            .unwrap()
            .messages
            .into_iter()
            .map(|message| message.id)
            .collect::<Vec<_>>()
        };
        assert_eq!(matching_ids("foo AND bar"), vec![3]);
        assert_eq!(matching_ids("foo AND NOT bar"), vec![1]);
    }

    #[test]
    fn export_boolean_query_combines_metadata_body_phrases_prefixes_and_nesting() {
        let conn = setup();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('a1', 'blocked', 'blocked', 'other', 'other')",
            [],
        )
        .unwrap();
        let blocked_sender = conn.last_insert_rowid();
        conn.execute(
            "UPDATE messages SET body = 'alpha phrase', sender_handle_id = ?1 WHERE id = 1",
            [blocked_sender],
        )
        .unwrap();
        conn.execute("UPDATE messages SET body = 'unrelated' WHERE id = 2", [])
            .unwrap();
        conn.execute(
            "INSERT INTO attachments (
                message_id, path, original_name, mime_type, sha256, is_sticker
             ) VALUES (
                2, 'attachments/report-final.pdf', 'report-final.pdf',
                'application/pdf', 'report-digest', 0
             )",
            [],
        )
        .unwrap();

        let matching_ids = |query| {
            export_messages(
                &conn,
                ExportPageOpts {
                    account_id: "a1",
                    query,
                    limit: 100,
                    offset: None,
                    cursor: None,
                    source_override: None,
                },
            )
            .unwrap()
            .messages
            .into_iter()
            .map(|message| message.id)
            .collect::<Vec<_>>()
        };

        assert_eq!(matching_ids(r#""alpha phrase" OR report*"#), vec![1, 2]);
        assert_eq!(
            matching_ids(r#"blocked AND ("alpha phrase" OR report*)"#),
            vec![1]
        );
        assert_eq!(matching_ids("NOT NOT report*"), vec![2]);

        conn.execute(
            "INSERT INTO trashed_conversations (account_id, conversation_id)
             VALUES ('a1', 2)",
            [],
        )
        .unwrap();
        assert_eq!(matching_ids(r#""alpha phrase" OR report*"#), vec![1]);
    }

    #[test]
    fn rejects_oversized_search_query_and_offset() {
        let conn = setup();
        let huge = "x".repeat(MAX_SEARCH_QUERY_BYTES + 1);
        let err = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: &huge,
                limit: 10,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("exceeds"), "{err}");

        let err = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 10,
                offset: Some(MAX_EXPORT_OFFSET + 1),
                cursor: None,
                source_override: None,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("offset"), "{err}");
    }

    #[test]
    fn export_does_not_leak_other_account_messages() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES ('a2', 'bob', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('a2', '+1777', '+1777', 'phone', 'phone')",
            [],
        )
        .unwrap();
        let bob_handle = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO conversations (id, account_id, chat_handle_id, conversation_type, source_file)
             VALUES (99, 'a2', ?1, 'individual', 'bob.jsonl')",
            params![bob_handle],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, account_id, source, service, timestamp, is_from_me, sort_order, body)
             VALUES (99, 99, 'a2', 'sms', 'sms', '2020-02-01T00:00:00Z', 0, 0, 'bob secret')",
            [],
        )
        .unwrap();

        let alice = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "secret",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        assert!(
            alice.messages.is_empty(),
            "alice must not see bob's FTS hits"
        );

        let alice_all = export_messages(
            &conn,
            ExportPageOpts {
                account_id: "a1",
                query: "",
                limit: 100,
                offset: None,
                cursor: None,
                source_override: None,
            },
        )
        .unwrap();
        assert!(alice_all.messages.iter().all(|m| m.id != 99));
    }
}
