//! Read-only contact query used by `GET /v1/export/contacts`
//! and `GET /v1/export/contacts/{id}`.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::export_api::ExportQueryError;

#[derive(Debug, Serialize)]
pub struct ContactSummary {
    pub id: i64,
    pub name: String,
    pub handle_count: u64,
    /// Normalized (and raw when distinct) handle values for client-side filter.
    #[serde(default)]
    pub handles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
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
fn involves_contact_sql() -> &'static str {
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

/// Flat list of contacts: id, display name, handle count, last message date.
pub fn list_contacts(
    conn: &Connection,
    account_id: &str,
) -> Result<Vec<ContactSummary>, ExportQueryError> {
    let mut stmt = conn
        .prepare(
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
                     )) AS handles,
                    (SELECT MAX(m.timestamp)
                     FROM messages m
                     JOIN conversations c ON c.id = m.conversation_id
                     WHERE c.account_id = ct.account_id
                       AND m.duplicate_of IS NULL
                       AND EXISTS (
                         SELECT 1 FROM contact_handles ch2
                         WHERE ch2.account_id = c.account_id AND ch2.contact_id = ct.id
                           AND (
                             ch2.handle_id = c.chat_handle_id
                             OR EXISTS (
                               SELECT 1 FROM participants p
                               WHERE p.conversation_id = c.id AND p.handle_id = ch2.handle_id
                             )
                           )
                       )) AS last_message_at
             FROM contacts ct
             WHERE ct.account_id = ?1
             ORDER BY name COLLATE NOCASE",
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map([account_id], |row| {
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
                last_message_at: row.get(4)?,
            })
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    Ok(rows)
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

    // Conversation + message stats across all handles of this contact.
    let mut stats_stmt = conn
        .prepare(&format!(
            "SELECT COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN c.id END),
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'group' THEN c.id END),
                    COALESCE(SUM(mc.m_count), 0)
             FROM conversations c
             LEFT JOIN (
               SELECT conversation_id, COUNT(*) AS m_count
               FROM messages
               WHERE account_id = ?1 AND duplicate_of IS NULL
               GROUP BY conversation_id
             ) mc ON mc.conversation_id = c.id
             WHERE c.account_id = ?1
               AND {involves_contact_sql}
               AND NOT EXISTS (
                 SELECT 1 FROM trashed_conversations tc
                 WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
               )
               AND NOT EXISTS (
                 SELECT 1 FROM trashed_handles th
                 WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id
               )",
            involves_contact_sql = involves_contact_sql(),
        ))
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let (direct, groups, total): (Option<i64>, Option<i64>, Option<i64>) = stats_stmt
        .query_row(rusqlite::params![account_id, contact_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

    Ok(Some(ContactDetail {
        id: contact_id,
        name,
        handles,
        direct_conversations: direct.unwrap_or(0).max(0) as u64,
        group_conversations: groups.unwrap_or(0).max(0) as u64,
        total_messages: total.unwrap_or(0).max(0) as u64,
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

        let list = list_contacts(&conn, &account).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Pat");
        assert_eq!(list[0].handle_count, 1);
        assert!(
            list[0]
                .handles
                .iter()
                .any(|h| h.contains("5555550100") || h.contains("+15555550100")),
            "handles={:?}",
            list[0].handles
        );
    }
}
