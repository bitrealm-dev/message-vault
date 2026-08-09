//! Read-only contact query used by `GET /v1/export/contacts`
//! and `GET /v1/export/contacts/{id}`.

use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

use crate::export_api::ExportQueryError;

pub const DEFAULT_LIST_LIMIT: usize = 40;
pub const MAX_LIST_LIMIT: usize = 500;

#[derive(Debug, Serialize)]
pub struct ContactListPage {
    pub contacts: Vec<ContactSummary>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct ContactSummary {
    pub id: i64,
    pub name: String,
    pub handle_count: u64,
    /// Normalized (and raw when distinct) handle values for client-side filter.
    #[serde(default)]
    pub handles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ContactHandleInfo {
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    pub message_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ContactDetail {
    pub id: i64,
    pub name: String,
    pub handles: Vec<ContactHandleInfo>,
    pub direct_conversations: u64,
    pub group_conversations: u64,
    pub total_messages: u64,
}

/// A contact is linked to a conversation when one of its handles is either
/// the conversation's chat handle or a participant handle in it.
///
/// Expects one bind parameter: `contact_id` (i64). Alias `c` = conversations.
pub(crate) fn involves_contact_sql() -> &'static str {
    "EXISTS (
       SELECT 1 FROM contact_handles ch
       WHERE ch.account_id = c.account_id
         AND ch.contact_id = ?
         AND (
           ch.handle_id = c.chat_handle_id
           OR EXISTS (
             SELECT 1 FROM participants p
             WHERE p.conversation_id = c.id AND p.handle_id = ch.handle_id
           )
         )
     )"
}

/// Flat list of contacts: id, display name, handle count, and handle values (paged).
///
/// `q` matches preferred name or any linked handle (raw/normalized), case-insensitive.
/// `handle:<raw>` restricts to contacts that have that handle substring.
/// Message-date stats belong on contact detail, not this list (keeps list SQL cheap).
pub fn list_contacts(
    conn: &Connection,
    account_id: &str,
    q: &str,
    limit: usize,
    offset: usize,
) -> Result<ContactListPage, ExportQueryError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    let offset = offset;

    let (handle_filter, mut text) = parse_contact_list_query(q);
    // Strip advanced tokens the UI may still emit.
    text = text
        .split_whitespace()
        .filter(|t| {
            let lower = t.to_ascii_lowercase();
            lower != "search:contacts"
                && !lower.starts_with("first-contact:")
                && !lower.starts_with("last-contact:")
                && !lower.starts_with("message-count:")
                && !lower.starts_with("group-count:")
                && !lower.starts_with("handle:")
        })
        .collect::<Vec<_>>()
        .join(" ");

    let mut where_parts = vec!["ct.account_id = ?1".to_string()];
    let mut params: Vec<rusqlite::types::Value> = vec![account_id.to_string().into()];

    if let Some(ref handle) = handle_filter {
        where_parts.push(
            "EXISTS (
               SELECT 1 FROM contact_handles ch
               JOIN handles h ON h.id = ch.handle_id
               WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                 AND (h.raw LIKE ? OR coalesce(h.normalized, '') LIKE ?)
             )"
            .into(),
        );
        let like = format!("%{handle}%");
        params.push(like.clone().into());
        params.push(like.into());
    }

    if !text.is_empty() {
        where_parts.push(
            "(COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)') LIKE ?
              OR EXISTS (
                SELECT 1 FROM contact_handles ch
                JOIN handles h ON h.id = ch.handle_id
                WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                  AND (h.raw LIKE ? OR coalesce(h.normalized, '') LIKE ?)
              ))"
            .into(),
        );
        let like = format!("%{text}%");
        params.push(like.clone().into());
        params.push(like.clone().into());
        params.push(like.into());
    }

    let where_sql = where_parts.join(" AND ");

    let count_sql = format!("SELECT COUNT(*) FROM contacts ct WHERE {where_sql}");
    let total: i64 = conn
        .query_row(
            &count_sql,
            params_from_iter(params.iter().cloned()),
            |row| row.get(0),
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let total = total.max(0) as u64;

    let sql = format!(
        "SELECT ct.id,
                COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)') AS name,
                (SELECT COUNT(*)
                 FROM contact_handles ch
                 WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id) AS handle_count,
                (SELECT GROUP_CONCAT(val, char(31))
                 FROM (
                   SELECT DISTINCT h.normalized AS val
                   FROM contact_handles ch
                   JOIN handles h ON h.id = ch.handle_id
                   WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                     AND h.normalized IS NOT NULL AND trim(h.normalized) != ''
                   UNION
                   SELECT DISTINCT h.raw AS val
                   FROM contact_handles ch
                   JOIN handles h ON h.id = ch.handle_id
                   WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                     AND h.raw IS NOT NULL AND trim(h.raw) != ''
                 )) AS handles
         FROM contacts ct
         WHERE {where_sql}
         ORDER BY name COLLATE NOCASE, ct.id
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
            let handles_blob: Option<String> = row.get(3)?;
            let handles = handles_blob
                .map(|s| {
                    s.split('\u{1f}')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(ContactSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                handle_count: row.get::<_, i64>(2)?.max(0) as u64,
                handles,
            })
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

    Ok(ContactListPage {
        contacts: rows,
        total,
        limit,
        offset,
    })
}

/// Parse `q` into optional handle filter + free-text remainder.
fn parse_contact_list_query(q: &str) -> (Option<String>, String) {
    let q = q.trim();
    if q.is_empty() {
        return (None, String::new());
    }
    let lower = q.to_ascii_lowercase();
    if let Some(start) = lower.find("handle:\"") {
        let after = start + "handle:\"".len();
        if let Some(rel_end) = q[after..].find('"') {
            let end = after + rel_end;
            let handle = q[after..end].to_string();
            let mut rest = String::new();
            rest.push_str(&q[..start]);
            rest.push_str(&q[end + 1..]);
            return (
                Some(handle).filter(|s| !s.is_empty()),
                rest.split_whitespace().collect::<Vec<_>>().join(" "),
            );
        }
    }
    if let Some(start) = lower.find("handle:") {
        let after = start + "handle:".len();
        if q.get(after..after + 1) != Some("\"") {
            let end = q[after..]
                .find(char::is_whitespace)
                .map(|i| after + i)
                .unwrap_or(q.len());
            if end > after {
                let handle = q[after..end].trim_matches('"').to_string();
                let mut rest = String::new();
                rest.push_str(&q[..start]);
                rest.push_str(&q[end..]);
                return (
                    Some(handle).filter(|s| !s.is_empty()),
                    rest.split_whitespace().collect::<Vec<_>>().join(" "),
                );
            }
        }
    }
    (None, q.to_string())
}

/// Full contact view: per-handle service + date range + direct message count,
/// plus conversation and total-message stats across all the contact's handles.
pub fn get_contact_detail(
    conn: &Connection,
    account_id: &str,
    contact_id: i64,
) -> Result<Option<ContactDetail>, ExportQueryError> {
    let name: Option<String> = conn
        .query_row(
            "SELECT COALESCE(NULLIF(trim(preferred_name), ''), '(unknown)')
             FROM contacts WHERE id = ?1 AND account_id = ?2",
            rusqlite::params![contact_id, account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let Some(name) = name else {
        return Ok(None);
    };

    // One row per handle. Date range covers direct + group conversations;
    // message count is direct-messages only (group stats are not attributed).
    let mut stmt = conn
        .prepare(
            "SELECT h.raw,
                    (SELECT c2.service FROM conversations c2
                     WHERE c2.account_id = ch.account_id
                       AND (c2.chat_handle_id = ch.handle_id
                            OR EXISTS (
                              SELECT 1 FROM participants p2
                              WHERE p2.conversation_id = c2.id AND p2.handle_id = ch.handle_id
                            ))
                     ORDER BY c2.id DESC LIMIT 1) AS service,
                    MIN(m.timestamp) AS first_ts,
                    MAX(m.timestamp) AS last_ts,
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN m.id END)
             FROM contact_handles ch
             JOIN handles h ON h.id = ch.handle_id
             LEFT JOIN conversations c ON c.account_id = ch.account_id
               AND (c.chat_handle_id = ch.handle_id
                    OR EXISTS (
                      SELECT 1 FROM participants p
                      WHERE p.conversation_id = c.id AND p.handle_id = ch.handle_id
                    ))
             LEFT JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
             WHERE ch.account_id = ?1 AND ch.contact_id = ?2
             GROUP BY ch.handle_id, h.raw
             ORDER BY h.raw",
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let mut handles = Vec::new();
    let rows = stmt
        .query_map(rusqlite::params![account_id, contact_id], |row| {
            Ok(ContactHandleInfo {
                handle: row.get(0)?,
                service: row.get(1)?,
                start_date: row.get(2)?,
                end_date: row.get(3)?,
                message_count: row.get::<_, i64>(4)?.max(0) as u64,
            })
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    for row in rows {
        handles.push(row.map_err(|e| ExportQueryError::Internal(e.to_string()))?);
    }

    // Conversation + message stats across handles of this contact only.
    // Do not GROUP BY the entire account messages table — that dominated drawer latency.
    let mut stats_stmt = conn
        .prepare(&format!(
            "WITH involved AS (
               SELECT c.id, c.conversation_type
               FROM conversations c
               WHERE c.account_id = ?1
                 AND {involves_contact_sql}
                 AND NOT EXISTS (
                   SELECT 1 FROM trashed_conversations tc
                   WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
                 )
                 AND NOT EXISTS (
                   SELECT 1 FROM trashed_handles th
                   WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id
                 )
             )
             SELECT
               (SELECT COUNT(*) FROM involved WHERE conversation_type = 'individual'),
               (SELECT COUNT(*) FROM involved WHERE conversation_type = 'group'),
               (SELECT COUNT(*) FROM messages m
                WHERE m.duplicate_of IS NULL
                  AND m.conversation_id IN (SELECT id FROM involved))",
            involves_contact_sql = involves_contact_sql(),
        ))
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let (direct, groups, total): (i64, i64, i64) = stats_stmt
        .query_row(rusqlite::params![account_id, contact_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

    Ok(Some(ContactDetail {
        id: contact_id,
        name,
        handles,
        direct_conversations: direct.max(0) as u64,
        group_conversations: groups.max(0) as u64,
        total_messages: total.max(0) as u64,
    }))
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
        let account = "00000000-0000-4000-8000-0000000000c1".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![&account],
        )
        .unwrap();
        (conn, account)
    }

    #[test]
    fn list_contacts_uses_preferred_name_and_handle_ids() {
        let (conn, account) = setup();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Pat')",
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
        let handle_id =
            account_profile::link_account_handle(&conn, &account, "+15555550100", HandleType::Phone)
                .unwrap();
        // link_account_handle puts it on account_handles; also link as contact handle.
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES (?1, ?2, ?3)",
            params![&account, handle_id, contact_id],
        )
        .unwrap();

        let page = list_contacts(&conn, &account, "", DEFAULT_LIST_LIMIT, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.contacts.len(), 1);
        assert_eq!(page.contacts[0].name, "Pat");
        assert_eq!(page.contacts[0].handle_count, 1);
        assert!(
            page.contacts[0]
                .handles
                .iter()
                .any(|h| h.contains("5555550100") || h.contains("+15555550100")),
            "handles={:?}",
            page.contacts[0].handles
        );
    }

    #[test]
    fn list_contacts_filters_and_paginates() {
        let (conn, account) = setup();
        for (name, phone) in [("Pat", "+15555550100"), ("Sam", "+15555550200"), ("Alex", "+15555550300")]
        {
            conn.execute(
                "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, ?2)",
                params![&account, name],
            )
            .unwrap();
            let contact_id: i64 = conn
                .query_row(
                    "SELECT id FROM contacts WHERE account_id = ?1 AND preferred_name = ?2",
                    params![&account, name],
                    |r| r.get(0),
                )
                .unwrap();
            let handle_id =
                account_profile::link_account_handle(&conn, &account, phone, HandleType::Phone)
                    .unwrap();
            conn.execute(
                "INSERT INTO contact_handles (account_id, handle_id, contact_id)
                 VALUES (?1, ?2, ?3)",
                params![&account, handle_id, contact_id],
            )
            .unwrap();
        }

        let by_name = list_contacts(&conn, &account, "sam", DEFAULT_LIST_LIMIT, 0).unwrap();
        assert_eq!(by_name.total, 1);
        assert_eq!(by_name.contacts[0].name, "Sam");

        let by_handle = list_contacts(&conn, &account, "handle:5555550200", DEFAULT_LIST_LIMIT, 0)
            .unwrap();
        assert_eq!(by_handle.total, 1);
        assert_eq!(by_handle.contacts[0].name, "Sam");

        let page0 = list_contacts(&conn, &account, "", 2, 0).unwrap();
        assert_eq!(page0.total, 3);
        assert_eq!(page0.limit, 2);
        assert_eq!(page0.offset, 0);
        assert_eq!(page0.contacts.len(), 2);
        let page1 = list_contacts(&conn, &account, "", 2, 2).unwrap();
        assert_eq!(page1.total, 3);
        assert_eq!(page1.offset, 2);
        assert_eq!(page1.contacts.len(), 1);

        let clamped = list_contacts(&conn, &account, "", MAX_LIST_LIMIT + 50, 0).unwrap();
        assert_eq!(clamped.limit, MAX_LIST_LIMIT);
        assert_eq!(clamped.total, 3);
    }

    #[test]
    fn get_contact_detail_counts_direct_group_and_messages() {
        let (conn, account) = setup();
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
        let peer =
            account_profile::link_account_handle(&conn, &account, "+15555550200", HandleType::Phone)
                .unwrap();
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES (?1, ?2, ?3)",
            params![&account, peer, contact_id],
        )
        .unwrap();

        // Direct conversation with 2 messages.
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, source_file
             ) VALUES (1, ?1, ?2, 'iMessage', 'individual', 'd.jsonl')",
            params![&account, peer],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO participants (conversation_id, handle_id, name_hint)
             VALUES (1, ?1, 'Sam')",
            params![peer],
        )
        .unwrap();
        for (body, ts) in [
            ("hi", "2024-06-01T12:00:00Z"),
            ("there", "2024-06-01T13:00:00Z"),
        ] {
            conn.execute(
                "INSERT INTO messages (
                    conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
                 ) VALUES (1, ?1, 'imessage', ?2, 0, 0, ?3)",
                params![&account, ts, body],
            )
            .unwrap();
        }

        // Group conversation that includes Sam, with 1 message.
        let group_chat = account_profile::link_account_handle(
            &conn,
            &account,
            "chat-sam-group",
            HandleType::Other,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, group_title, source_file
             ) VALUES (2, ?1, ?2, 'iMessage', 'group', 'Sam Group', 'g.jsonl')",
            params![&account, group_chat],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO participants (conversation_id, handle_id, name_hint)
             VALUES (2, ?1, 'Sam')",
            params![peer],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (2, ?1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'group hi')",
            params![&account],
        )
        .unwrap();

        // Unrelated conversation should not be counted.
        let other =
            account_profile::link_account_handle(&conn, &account, "+15555550999", HandleType::Phone)
                .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, service, conversation_type, source_file
             ) VALUES (9, ?1, ?2, 'iMessage', 'individual', 'other.jsonl')",
            params![&account, other],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (9, ?1, 'imessage', '2024-08-01T12:00:00Z', 0, 0, 'nope')",
            params![&account],
        )
        .unwrap();

        let detail = get_contact_detail(&conn, &account, contact_id)
            .unwrap()
            .expect("contact exists");
        assert_eq!(detail.name, "Sam");
        assert_eq!(detail.direct_conversations, 1);
        assert_eq!(detail.group_conversations, 1);
        assert_eq!(detail.total_messages, 3);
        assert_eq!(detail.handles.len(), 1);
        assert!(
            detail.handles[0].handle.contains("5555550200")
                || detail.handles[0].handle.contains("+15555550200"),
            "handle={:?}",
            detail.handles[0].handle
        );
    }
}
