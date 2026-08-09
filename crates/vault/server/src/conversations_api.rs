//! Read-only conversation list used by `GET /v1/export/conversations`.

use std::collections::{HashMap, HashSet};

use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

use crate::export_api::ExportQueryError;
use crate::search_query::{CountComparator, CountComparison};

pub const DEFAULT_LIST_LIMIT: usize = 40;
pub const MAX_LIST_LIMIT: usize = 100;

#[derive(Debug, Serialize)]
pub struct ConversationListPage {
    pub conversations: Vec<ConversationSummary>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct ConversationParticipant {
    pub name: Option<String>,
    pub handle: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    /// Numeric `conversations.id`, serialized as a string for `in:<id>` queries.
    pub id: String,
    pub participants: Vec<ConversationParticipant>,
    pub message_count: u64,
    pub last_message_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_end: Option<String>,
    pub service: String,
    pub is_group: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

struct RawConversation {
    id: i64,
    service: Option<String>,
    conversation_type: String,
    group_title: Option<String>,
    message_count: i64,
    last_message_at: Option<String>,
    date_range_start: Option<String>,
    date_range_end: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationTypeFilter {
    Direct,
    Group,
}

#[derive(Debug, Default)]
struct ConversationListQuery {
    trash_only: bool,
    handle: Option<String>,
    contact_id: Option<i64>,
    type_filter: Option<ConversationTypeFilter>,
    /// Filter by number of rows in `participants` (`participants:=5`, `:>3`, `:<10`).
    participants: Option<CountComparison>,
    text: Option<String>,
}

/// Parse `participants:` values: `=5`, `>3`, `<10`, `>=2`, `<=8`, or bare `5` (=).
fn parse_participants_comparison(raw: &str) -> Option<CountComparison> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let (comparator, digits) = if let Some(rest) = t.strip_prefix(">=") {
        (CountComparator::Gte, rest)
    } else if let Some(rest) = t.strip_prefix("<=") {
        (CountComparator::Lte, rest)
    } else if let Some(rest) = t.strip_prefix('>') {
        (CountComparator::Gt, rest)
    } else if let Some(rest) = t.strip_prefix('<') {
        (CountComparator::Lt, rest)
    } else if let Some(rest) = t.strip_prefix('=') {
        (CountComparator::Eq, rest)
    } else if t.bytes().all(|b| b.is_ascii_digit()) {
        (CountComparator::Eq, t)
    } else {
        return None;
    };
    let digits = digits.trim();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(CountComparison {
        comparator,
        value: digits.parse().ok()?,
    })
}

/// Parse space-separated tokens from `q`.
///
/// Recognized tokens: `is:trash`, `is:direct`, `is:group`, `handle:<raw>`,
/// `contact:<id>`, `participants:=N` / `:>N` / `:<N`. Remaining tokens become
/// a free-text filter.
fn parse_conversation_list_query(q: &str) -> ConversationListQuery {
    let mut out = ConversationListQuery::default();
    let mut text_parts: Vec<&str> = Vec::new();

    for token in q.split_whitespace() {
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
        } else if lower.starts_with("participants:") {
            if let Some((_, value)) = token.split_once(':') {
                if let Some(cmp) = parse_participants_comparison(value) {
                    out.participants = Some(cmp);
                }
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
/// - `contact:<id>`: conversations involving any handle of that contact
/// - `is:direct` / `is:group`: restrict by conversation type
/// - other text: case-insensitive match on group title or participant handle/name
pub fn list_conversations(
    conn: &Connection,
    account_id: &str,
    q: &str,
    limit: usize,
    offset: usize,
) -> Result<ConversationListPage, ExportQueryError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    let offset = offset;

    let parsed = parse_conversation_list_query(q.trim());

    let mut where_parts = vec!["c.account_id = ?1".to_string()];
    let mut params: Vec<rusqlite::types::Value> = vec![account_id.to_string().into()];

    if parsed.trash_only {
        where_parts.push(
            "EXISTS (
               SELECT 1 FROM trashed_conversations tc
               WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
             )"
            .into(),
        );
    } else {
        where_parts.push(
            "NOT EXISTS (
               SELECT 1 FROM trashed_conversations tc
               WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
             )"
            .into(),
        );
        where_parts.push(
            "NOT EXISTS (
               SELECT 1 FROM trashed_handles th
               WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id
             )"
            .into(),
        );
    }

    // Only show threads that have at least one message.
    where_parts.push(
        "EXISTS (
           SELECT 1 FROM messages m0
           WHERE m0.conversation_id = c.id AND m0.duplicate_of IS NULL
         )"
        .into(),
    );

    if let Some(ref handle) = parsed.handle {
        where_parts.push(
            "(hc.raw = ? OR EXISTS (
                SELECT 1 FROM participants p
                JOIN handles ph ON ph.id = p.handle_id
                WHERE p.conversation_id = c.id AND ph.raw = ?
              ))"
            .into(),
        );
        params.push(handle.clone().into());
        params.push(handle.clone().into());
    }

    if let Some(contact_id) = parsed.contact_id {
        where_parts.push(crate::contacts_api::involves_contact_sql().into());
        params.push(contact_id.into());
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
        where_parts.push(format!(
            "(SELECT COUNT(*) FROM participants pcnt WHERE pcnt.conversation_id = c.id) {} ?",
            cmp.comparator.as_str()
        ));
        params.push((cmp.value as i64).into());
    }

    if let Some(ref text) = parsed.text {
        where_parts.push(
            "(c.group_title LIKE ? OR hc.raw LIKE ? OR EXISTS (
                SELECT 1 FROM participants p
                JOIN handles ph ON ph.id = p.handle_id
                LEFT JOIN contacts ct ON ct.id = p.contact_id
                WHERE p.conversation_id = c.id
                  AND (
                    ph.raw LIKE ?
                    OR coalesce(p.name_hint, '') LIKE ?
                    OR coalesce(ct.preferred_name, '') LIKE ?
                  )
              ))"
            .into(),
        );
        let like = format!("%{text}%");
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.into());
    }

    let where_sql = where_parts.join(" AND ");

    let count_sql = format!(
        "SELECT COUNT(*)
         FROM conversations c
         JOIN handles hc ON hc.id = c.chat_handle_id
         WHERE {where_sql}"
    );
    let total: i64 = conn
        .query_row(
            &count_sql,
            params_from_iter(params.iter().cloned()),
            |row| row.get(0),
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let total = total.max(0) as u64;

    let sql = format!(
        "SELECT c.id,
                c.service,
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
         LIMIT ? OFFSET ?"
    );

    let mut page_params = params.clone();
    page_params.push((limit as i64).into());
    page_params.push((offset as i64).into());

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map(params_from_iter(page_params.iter().cloned()), |row| {
            Ok(RawConversation {
                id: row.get(0)?,
                service: row.get(1)?,
                conversation_type: row.get(2)?,
                group_title: row.get(3)?,
                message_count: row.get(4)?,
                last_message_at: row.get(5)?,
                date_range_start: row.get(6)?,
                date_range_end: row.get(7)?,
            })
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut participants = load_participants(conn, &ids)?;
    let source_sets = load_conversation_sources(conn, &ids)?;

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
            row.service.as_deref(),
        );
        let parts = participants.remove(&row.id).unwrap_or_default();
        let parts = if parts.is_empty() {
            chat_handle_as_participant(conn, row.id)?
        } else {
            parts
        };
        // Prefer contact preferred_name over name_hint when linked.
        let enriched = enrich_participant_names(conn, account_id, parts)?;
        out.push(ConversationSummary {
            id: row.id.to_string(),
            participants: enriched,
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
        });
    }
    Ok(ConversationListPage {
        conversations: out,
        total,
        limit,
        offset,
    })
}

fn chat_handle_as_participant(
    conn: &Connection,
    conversation_id: i64,
) -> Result<Vec<ConversationParticipant>, ExportQueryError> {
    let row: Option<(String, Option<String>, String)> = conn
        .query_row(
            "SELECT h.raw,
                    nullif(trim(c.service), ''),
                    h.handle_type
             FROM conversations c
             JOIN handles h ON h.id = c.chat_handle_id
             WHERE c.id = ?1",
            [conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    Ok(match row {
        Some((handle, service, handle_type)) => vec![ConversationParticipant {
            name: None,
            handle,
            service: service.unwrap_or(handle_type),
            contact_id: None,
        }],
        None => Vec::new(),
    })
}

fn load_participants(
    conn: &Connection,
    conversation_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<ConversationParticipant>>, ExportQueryError> {
    let mut map = std::collections::HashMap::new();
    if conversation_ids.is_empty() {
        return Ok(map);
    }
    for chunk in conversation_ids.chunks(400) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT p.conversation_id,
                    p.name_hint,
                    h.raw,
                    coalesce(nullif(trim(h.service), ''), nullif(trim(c.service), ''), h.handle_type),
                    p.contact_id
             FROM participants p
             JOIN handles h ON h.id = p.handle_id
             JOIN conversations c ON c.id = p.conversation_id
             WHERE p.conversation_id IN ({placeholders})
             ORDER BY p.conversation_id, p.id"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter().copied()), |row| {
                let contact_id: Option<i64> = row.get(4)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    ConversationParticipant {
                        name: row.get::<_, Option<String>>(1)?,
                        handle: row.get(2)?,
                        service: row.get::<_, String>(3).unwrap_or_else(|_| "unknown".into()),
                        contact_id: contact_id.map(|id| id.to_string()),
                    },
                ))
            })
            .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
        for row in rows {
            let (cid, p) = row.map_err(|e| ExportQueryError::Internal(e.to_string()))?;
            map.entry(cid).or_insert_with(Vec::new).push(p);
        }
    }
    Ok(map)
}

fn enrich_participant_names(
    conn: &Connection,
    account_id: &str,
    mut participants: Vec<ConversationParticipant>,
) -> Result<Vec<ConversationParticipant>, ExportQueryError> {
    for p in &mut participants {
        let Some(ref contact_id) = p.contact_id else {
            continue;
        };
        let Ok(cid) = contact_id.parse::<i64>() else {
            continue;
        };
        let name: Option<String> = conn
            .query_row(
                "SELECT NULLIF(trim(preferred_name), '')
                 FROM contacts WHERE id = ?1 AND account_id = ?2",
                rusqlite::params![cid, account_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
        if let Some(n) = name.filter(|s| !s.is_empty()) {
            p.name = Some(n);
        }
    }
    Ok(participants)
}

const IMESSAGE_SOURCE: &str = "imessage";
const SBR_SOURCE: &str = "sms-backup-restore";

/// Header label from distinct message sources in a conversation.
pub fn display_service_label(sources: &[String], stored_service: Option<&str>) -> String {
    let set: HashSet<&str> = sources.iter().map(|s| s.as_str()).collect();
    if set.contains(SBR_SOURCE) {
        return "SMS/MMS".into();
    }
    if set.len() == 1 && set.contains(IMESSAGE_SOURCE) {
        return IMESSAGE_SOURCE.into();
    }
    stored_service
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn load_conversation_sources(
    conn: &Connection,
    conversation_ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>, ExportQueryError> {
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    if conversation_ids.is_empty() {
        return Ok(map);
    }
    for chunk in conversation_ids.chunks(400) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT conversation_id, source
             FROM messages
             WHERE duplicate_of IS NULL
               AND conversation_id IN ({placeholders})
             GROUP BY conversation_id, source
             ORDER BY conversation_id, source"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter().copied()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
        for row in rows {
            let (cid, source) = row.map_err(|e| ExportQueryError::Internal(e.to_string()))?;
            if source.trim().is_empty() {
                continue;
            }
            map.entry(cid).or_default().push(source);
        }
    }
    Ok(map)
}

#[derive(Debug, Serialize)]
pub struct ConversationSourceInfo {
    pub backup_name: String,
    pub message_count: u64,
    pub unique_count: u64,
    pub percentage: f64,
}

#[derive(Debug, Serialize)]
pub struct ConversationSourcesPage {
    pub sources: Vec<ConversationSourceInfo>,
}

/// Per-source message counts for the Sources panel.
pub fn list_conversation_source_stats(
    conn: &Connection,
    account_id: &str,
    conversation_id: i64,
) -> Result<Option<ConversationSourcesPage>, ExportQueryError> {
    let owned: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversations WHERE id = ?1 AND account_id = ?2",
            rusqlite::params![conversation_id, account_id],
            |row| row.get(0),
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    if owned == 0 {
        return Ok(None);
    }

    let mut stmt = conn
        .prepare(
            "SELECT source,
                    COUNT(*) AS message_count,
                    SUM(CASE WHEN duplicate_of IS NULL THEN 1 ELSE 0 END) AS unique_count
             FROM messages
             WHERE conversation_id = ?1
             GROUP BY source
             ORDER BY source",
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let rows: Vec<(String, i64, i64)> = stmt
        .query_map(rusqlite::params![conversation_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::HandleType;
    use rusqlite::params;

    use crate::db::{account_profile, schema};

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        let account = "00000000-0000-4000-8000-0000000000c2".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![&account],
        )
        .unwrap();
        let peer =
            account_profile::link_account_handle(&conn, &account, "+15555550200", HandleType::Phone)
                .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, source_file
             ) VALUES (1, ?1, ?2, 'iMessage', 'individual', 'c.jsonl')",
            params![&account, peer],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO participants (conversation_id, handle_id, name_hint)
             VALUES (1, ?1, 'Sam')",
            params![peer],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (1, ?1, 'imessage', '2024-06-01T12:00:00Z', 0, 0, 'hello')",
            params![&account],
        )
        .unwrap();
        (conn, account)
    }

    #[test]
    fn list_conversations_returns_summary() {
        let (conn, account) = setup();
        let page = list_conversations(&conn, &account, "", DEFAULT_LIST_LIMIT, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.conversations.len(), 1);
        assert_eq!(page.conversations[0].id, "1");
        assert_eq!(page.conversations[0].message_count, 1);
        assert!(!page.conversations[0].is_group);
        assert_eq!(page.conversations[0].participants.len(), 1);
        assert_eq!(page.conversations[0].participants[0].handle, "+15555550200");
    }

    #[test]
    fn list_conversations_filters_by_handle() {
        let (conn, account) = setup();
        let hit =
            list_conversations(&conn, &account, "handle:+15555550200", DEFAULT_LIST_LIMIT, 0)
                .unwrap();
        assert_eq!(hit.total, 1);
        assert_eq!(hit.conversations.len(), 1);
        let miss =
            list_conversations(&conn, &account, "handle:+19999999999", DEFAULT_LIST_LIMIT, 0)
                .unwrap();
        assert_eq!(miss.total, 0);
        assert!(miss.conversations.is_empty());
    }

    #[test]
    fn list_conversations_paginates() {
        let (conn, account) = setup();
        // Second conversation + message.
        let peer2 =
            account_profile::link_account_handle(&conn, &account, "+15555550300", HandleType::Phone)
                .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, source_file
             ) VALUES (2, ?1, ?2, 'iMessage', 'individual', 'c2.jsonl')",
            params![&account, peer2],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (2, ?1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'later')",
            params![&account],
        )
        .unwrap();

        let page0 = list_conversations(&conn, &account, "", 1, 0).unwrap();
        assert_eq!(page0.total, 2);
        assert_eq!(page0.limit, 1);
        assert_eq!(page0.offset, 0);
        assert_eq!(page0.conversations.len(), 1);
        assert_eq!(page0.conversations[0].id, "2"); // newer first

        let page1 = list_conversations(&conn, &account, "", 1, 1).unwrap();
        assert_eq!(page1.total, 2);
        assert_eq!(page1.offset, 1);
        assert_eq!(page1.conversations.len(), 1);
        assert_eq!(page1.conversations[0].id, "1");

        let by_text = list_conversations(&conn, &account, "5555550300", 10, 0).unwrap();
        assert_eq!(by_text.total, 1);
        assert_eq!(by_text.conversations[0].id, "2");

        let clamped = list_conversations(&conn, &account, "", MAX_LIST_LIMIT + 50, 0).unwrap();
        assert_eq!(clamped.limit, MAX_LIST_LIMIT);
        assert_eq!(clamped.total, 2);
    }

    #[test]
    fn list_conversations_filters_by_contact_and_type() {
        let (conn, account) = setup();
        // Link peer handle to a contact.
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Sam')",
            params![&account],
        )
        .unwrap();
        let contact_id: i64 = conn
            .query_row(
                "SELECT id FROM contacts WHERE account_id = ?1",
                params![&account],
                |r| r.get(0),
            )
            .unwrap();
        let peer_handle_id: i64 = conn
            .query_row(
                "SELECT id FROM handles WHERE account_id = ?1 AND raw = ?2",
                params![&account, "+15555550200"],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES (?1, ?2, ?3)",
            params![&account, peer_handle_id, contact_id],
        )
        .unwrap();

        // Unrelated group conversation (no link to Sam).
        let other =
            account_profile::link_account_handle(&conn, &account, "+15555550999", HandleType::Phone)
                .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, group_title, source_file
             ) VALUES (9, ?1, ?2, 'iMessage', 'group', 'Other', 'g.jsonl')",
            params![&account, other],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (9, ?1, 'imessage', '2024-08-01T12:00:00Z', 0, 0, 'group')",
            params![&account],
        )
        .unwrap();

        // Group that includes Sam (distinct chat handle; Sam is a participant).
        let group_chat = account_profile::link_account_handle(
            &conn,
            &account,
            "chat123456",
            HandleType::Other,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, group_title, source_file
             ) VALUES (3, ?1, ?2, 'iMessage', 'group', 'Sam Group', 'sg.jsonl')",
            params![&account, group_chat],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO participants (conversation_id, handle_id, name_hint)
             VALUES (3, ?1, 'Sam')",
            params![peer_handle_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (3, ?1, 'imessage', '2024-09-01T12:00:00Z', 0, 0, 'hi group')",
            params![&account],
        )
        .unwrap();

        let all = list_conversations(
            &conn,
            &account,
            &format!("contact:{contact_id}"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .unwrap();
        assert_eq!(all.total, 2);
        let ids: Vec<&str> = all.conversations.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"1"));
        assert!(ids.contains(&"3"));

        let direct = list_conversations(
            &conn,
            &account,
            &format!("contact:{contact_id} is:direct"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .unwrap();
        assert_eq!(direct.total, 1);
        assert_eq!(direct.conversations[0].id, "1");
        assert!(!direct.conversations[0].is_group);

        let groups = list_conversations(
            &conn,
            &account,
            &format!("contact:{contact_id} is:group"),
            DEFAULT_LIST_LIMIT,
            0,
        )
        .unwrap();
        assert_eq!(groups.total, 1);
        assert_eq!(groups.conversations[0].id, "3");
        assert!(groups.conversations[0].is_group);
    }

    #[test]
    fn parse_conversation_list_query_tokens() {
        let q = parse_conversation_list_query("contact:42 is:direct handle:+15555550100");
        assert_eq!(q.contact_id, Some(42));
        assert_eq!(q.type_filter, Some(ConversationTypeFilter::Direct));
        assert_eq!(q.handle.as_deref(), Some("+15555550100"));
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

        let quoted_handle =
            parse_conversation_list_query(r#"handle:"+15555550100""#);
        assert_eq!(quoted_handle.handle.as_deref(), Some("+15555550100"));
    }

    #[test]
    fn list_conversations_matches_contact_preferred_name() {
        let (conn, account) = setup();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Sam Preferred')",
            params![&account],
        )
        .unwrap();
        let contact_id: i64 = conn
            .query_row(
                "SELECT id FROM contacts WHERE account_id = ?1",
                params![&account],
                |r| r.get(0),
            )
            .unwrap();
        let peer_handle_id: i64 = conn
            .query_row(
                "SELECT id FROM handles WHERE account_id = ?1 AND raw = ?2",
                params![&account, "+15555550200"],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "UPDATE participants SET contact_id = ?1, name_hint = NULL
             WHERE conversation_id = '1' AND handle_id = ?2",
            params![contact_id, peer_handle_id],
        )
        .unwrap();

        let by_name = list_conversations(&conn, &account, "Sam Preferred", 10, 0).unwrap();
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

    #[test]
    fn list_conversations_filters_by_participant_count() {
        let (conn, account) = setup();
        // setup() has conversation 1 with 1 participant.

        let p2 =
            account_profile::link_account_handle(&conn, &account, "+15555550301", HandleType::Phone)
                .unwrap();
        let p3 =
            account_profile::link_account_handle(&conn, &account, "+15555550302", HandleType::Phone)
                .unwrap();
        let group_chat = account_profile::link_account_handle(
            &conn,
            &account,
            "chat-big",
            HandleType::Other,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, group_title, source_file
             ) VALUES (10, ?1, ?2, 'iMessage', 'group', 'Trio', 't.jsonl')",
            params![&account, group_chat],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO participants (conversation_id, handle_id, name_hint) VALUES
             (10, ?1, 'A'), (10, ?2, 'B')",
            params![p2, p3],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (10, ?1, 'imessage', '2024-10-01T12:00:00Z', 0, 0, 'hi')",
            params![&account],
        )
        .unwrap();

        let eq2 = list_conversations(&conn, &account, "participants:=2", 50, 0).unwrap();
        assert_eq!(eq2.total, 1);
        assert_eq!(eq2.conversations[0].id, "10");

        let gt1 = list_conversations(&conn, &account, "participants:>1", 50, 0).unwrap();
        assert_eq!(gt1.total, 1);
        assert_eq!(gt1.conversations[0].id, "10");

        let eq1 = list_conversations(&conn, &account, "participants:1", 50, 0).unwrap();
        assert_eq!(eq1.total, 1);
        assert_eq!(eq1.conversations[0].id, "1");

        let lt2 = list_conversations(&conn, &account, "is:group participants:<2", 50, 0).unwrap();
        assert_eq!(lt2.total, 0);
    }

    #[test]
    fn list_conversations_participants_eq_on_demo_fixture_db() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data/vault.db");
        if !path.is_file() {
            eprintln!("skip — missing {}", path.display());
            return;
        }
        let conn = Connection::open(&path).unwrap();
        let account = "00000000-0000-0000-0000-00000000d001";
        let page = list_conversations(&conn, account, "participants:=3", 50, 0).unwrap();
        assert!(
            page.total >= 1,
            "demo db should have conversations with 3 participants; total={}",
            page.total
        );
        assert!(
            page.conversations
                .iter()
                .all(|c| c.participants.len() == 3),
            "every returned conversation should have 3 participants"
        );
    }

    #[test]
    fn display_service_label_from_sources() {
        assert_eq!(
            display_service_label(&["imessage".into()], None),
            "imessage"
        );
        assert_eq!(
            display_service_label(&["sms-backup-restore".into()], None),
            "SMS/MMS"
        );
        assert_eq!(
            display_service_label(
                &["imessage".into(), "sms-backup-restore".into()],
                Some("iMessage")
            ),
            "SMS/MMS"
        );
        assert_eq!(display_service_label(&[], None), "unknown");
        assert_eq!(
            display_service_label(&["whatsapp".into()], Some("WhatsApp")),
            "WhatsApp"
        );
    }
}
