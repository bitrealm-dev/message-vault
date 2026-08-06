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
/// the conversation's `chat_identifier` or a participant handle in it.
fn involves_contact_sql() -> &'static str {
    "EXISTS (
       SELECT 1 FROM contact_handles ch
       WHERE ch.account_id = c.account_id
         AND ch.contact_id = ?
         AND (
           ch.handle = c.chat_identifier
           OR EXISTS (
             SELECT 1 FROM participants p
             WHERE p.conversation_id = c.id AND p.handle = ch.handle
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
                    COALESCE(NULLIF(trim(ct.preferred_name), ''), NULLIF(trim(ct.preferred_handle), ''), '(unknown)') AS name,
                    (SELECT COUNT(*) FROM contact_handles ch WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id) AS handle_count,
                    (SELECT MAX(m.timestamp)
                     FROM messages m
                     JOIN conversations c ON c.id = m.conversation_id
                     WHERE c.account_id = ct.account_id
                       AND m.duplicate_of IS NULL
                       AND EXISTS (
                         SELECT 1 FROM contact_handles ch2
                         WHERE ch2.account_id = c.account_id AND ch2.contact_id = ct.id
                           AND (
                             ch2.handle = c.chat_identifier
                             OR EXISTS (
                               SELECT 1 FROM participants p
                               WHERE p.conversation_id = c.id AND p.handle = ch2.handle
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
            Ok(ContactSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                handle_count: row.get::<_, i64>(2)?.max(0) as u64,
                last_message_at: row.get(3)?,
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
            "SELECT COALESCE(NULLIF(trim(preferred_name), ''), NULLIF(trim(preferred_handle), ''), '(unknown)')
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
    // COUNT(DISTINCT ...) guards against a conversation matching two handles
    // of the same contact (chat_identifier + participant).
    let mut stmt = conn
        .prepare(
            "SELECT ch.handle,
                    (SELECT c2.service FROM conversations c2
                     WHERE c2.account_id = ch.account_id
                       AND (c2.chat_identifier = ch.handle
                            OR EXISTS (
                              SELECT 1 FROM participants p2
                              WHERE p2.conversation_id = c2.id AND p2.handle = ch.handle
                            ))
                     ORDER BY c2.id DESC LIMIT 1) AS service,
                    MIN(m.timestamp) AS first_ts,
                    MAX(m.timestamp) AS last_ts,
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN m.id END)
             FROM contact_handles ch
             LEFT JOIN conversations c ON c.account_id = ch.account_id
               AND (c.chat_identifier = ch.handle
                    OR EXISTS (
                      SELECT 1 FROM participants p
                      WHERE p.conversation_id = c.id AND p.handle = ch.handle
                    ))
             LEFT JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
             WHERE ch.account_id = ?1 AND ch.contact_id = ?2
             GROUP BY ch.handle
             ORDER BY ch.handle",
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
                 WHERE th.account_id = c.account_id AND th.handle = c.chat_identifier
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
