//! Read-only conversation list used by `GET /v1/export/conversations`.

use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

use crate::export_api::ExportQueryError;

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

/// List conversations for the account, newest first.
///
/// Supported `q` values:
/// - empty / whitespace: all non-trashed conversations with at least one message
/// - `is:trash`: only trashed conversations
/// - `handle:<raw>`: conversations involving that handle (chat or participant)
/// - other text: case-insensitive match on group title or participant handle/name
pub fn list_conversations(
    conn: &Connection,
    account_id: &str,
    q: &str,
) -> Result<Vec<ConversationSummary>, ExportQueryError> {
    let q = q.trim();
    let trash_only = q.eq_ignore_ascii_case("is:trash");
    let handle_filter = q
        .strip_prefix("handle:")
        .or_else(|| q.strip_prefix("HANDLE:"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let text_filter = if trash_only || handle_filter.is_some() || q.is_empty() {
        None
    } else {
        Some(q.to_string())
    };

    let mut where_parts = vec!["c.account_id = ?1".to_string()];
    let mut params: Vec<rusqlite::types::Value> = vec![account_id.to_string().into()];

    if trash_only {
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

    if let Some(ref handle) = handle_filter {
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

    if let Some(ref text) = text_filter {
        where_parts.push(
            "(c.group_title LIKE ? OR hc.raw LIKE ? OR EXISTS (
                SELECT 1 FROM participants p
                JOIN handles ph ON ph.id = p.handle_id
                WHERE p.conversation_id = c.id
                  AND (ph.raw LIKE ? OR coalesce(p.name_hint, '') LIKE ?)
              ))"
            .into(),
        );
        let like = format!("%{text}%");
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.into());
    }

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
         WHERE {}
         ORDER BY last_message_at DESC, c.id DESC",
        where_parts.join(" AND ")
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter().cloned()), |row| {
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

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let last = row
            .last_message_at
            .clone()
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
        let is_group = row.conversation_type.eq_ignore_ascii_case("group");
        let service = row
            .service
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "unknown".into());
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
    Ok(out)
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
        let list = list_conversations(&conn, &account, "").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "1");
        assert_eq!(list[0].message_count, 1);
        assert!(!list[0].is_group);
        assert_eq!(list[0].participants.len(), 1);
        assert_eq!(list[0].participants[0].handle, "+15555550200");
    }

    #[test]
    fn list_conversations_filters_by_handle() {
        let (conn, account) = setup();
        let hit = list_conversations(&conn, &account, "handle:+15555550200").unwrap();
        assert_eq!(hit.len(), 1);
        let miss = list_conversations(&conn, &account, "handle:+19999999999").unwrap();
        assert!(miss.is_empty());
    }
}
