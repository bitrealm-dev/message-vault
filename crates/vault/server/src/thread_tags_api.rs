//! Thread tags stored in `conversation_tags` / `conversation_tag_members`.

use std::collections::HashMap;

use anyhow::Result as AnyResult;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::db::sql::{fold_in_id_chunks, in_placeholders};

/// Longest allowed tag name (characters).
pub const MAX_TAG_NAME_LEN: usize = 80;

/// Names that must not be created as user tags.
const RESERVED_TAG_NAMES: &[&str] = &[
    "home",
    "contacts",
    "threads",
    "thread",
    "all",
    "excluded",
    "unassigned",
    "trash",
    "tags",
    "tag",
    "no-tag",
    "no tag",
    "groups",
    "group",
    "labels",
    "label",
];

/// Create / rename / delete / membership failures.
#[derive(Debug)]
pub enum TagError {
    /// Invalid tag name (empty, reserved, or too long).
    BadRequest(String),
    /// The tag does not exist.
    NotFound(String),
    /// A tag with this name already exists.
    Conflict(String),
    /// Database or unexpected failure.
    Internal(String),
}

impl From<rusqlite::Error> for TagError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// True when `name` is reserved and must not be created.
pub fn is_reserved_tag_name(name: &str) -> bool {
    let key = name.trim().to_ascii_lowercase();
    RESERVED_TAG_NAMES.contains(&key.as_str())
}

fn reserved_tag_error(name: &str) -> String {
    format!("\"{}\" is a reserved tag", name.trim())
}

fn normalize_new_name(name: &str) -> Result<String, TagError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(TagError::BadRequest("name required".into()));
    }
    if trimmed.chars().count() > MAX_TAG_NAME_LEN {
        return Err(TagError::BadRequest(format!(
            "name must be at most {MAX_TAG_NAME_LEN} characters"
        )));
    }
    if is_reserved_tag_name(trimmed) {
        return Err(TagError::BadRequest(reserved_tag_error(trimmed)));
    }
    Ok(trimmed.to_string())
}

/// Tag names for this account, A–Z, excluding reserved leftovers.
pub fn list_tags(conn: &Connection, account_id: &str) -> Result<Vec<String>, TagError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM conversation_tags
         WHERE account_id = ?1
         ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let name = row?;
        if !is_reserved_tag_name(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Tags on one conversation, A–Z.
#[cfg(test)]
pub fn tags_for_conversation(
    conn: &Connection,
    account_id: &str,
    conversation_id: i64,
) -> AnyResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT ct.name
         FROM conversation_tags ct
         JOIN conversation_tag_members m ON m.tag_id = ct.id
         WHERE ct.account_id = ?1 AND m.conversation_id = ?2
         ORDER BY ct.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![account_id, conversation_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Tags on each conversation id, A–Z within each list.
pub fn tags_for_conversations(
    conn: &Connection,
    account_id: &str,
    conversation_ids: &[i64],
) -> AnyResult<HashMap<i64, Vec<String>>> {
    fold_in_id_chunks(conversation_ids, |chunk| {
        let placeholders = in_placeholders(chunk.len());
        let sql = format!(
            "SELECT m.conversation_id, ct.name
             FROM conversation_tag_members m
             JOIN conversation_tags ct ON ct.id = m.tag_id
             WHERE ct.account_id = ? AND m.conversation_id IN ({placeholders})
             ORDER BY ct.name COLLATE NOCASE"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut binds: Vec<rusqlite::types::Value> = Vec::with_capacity(chunk.len() + 1);
        binds.push(account_id.to_string().into());
        for id in chunk {
            binds.push((*id).into());
        }
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    })
}

fn find_tag_id(conn: &Connection, account_id: &str, name: &str) -> Result<Option<i64>, TagError> {
    let id = conn
        .query_row(
            "SELECT id FROM conversation_tags
             WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE",
            params![account_id, name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(id)
}

fn ensure_tag_id(conn: &Connection, account_id: &str, name: &str) -> Result<i64, TagError> {
    let name = normalize_new_name(name)?;
    conn.execute(
        "INSERT OR IGNORE INTO conversation_tags (account_id, name) VALUES (?1, ?2)",
        params![account_id, name],
    )?;
    find_tag_id(conn, account_id, &name)?
        .ok_or_else(|| TagError::Internal(format!("failed to ensure tag {name}")))
}

/// Create a tag. Fails when the name is taken (ignoring case).
pub fn create_tag(conn: &Connection, account_id: &str, name: &str) -> Result<String, TagError> {
    let name = normalize_new_name(name)?;
    if find_tag_id(conn, account_id, &name)?.is_some() {
        return Err(TagError::Conflict("tag already exists".into()));
    }
    conn.execute(
        "INSERT INTO conversation_tags (account_id, name) VALUES (?1, ?2)",
        params![account_id, name],
    )?;
    Ok(name)
}

/// Rename a tag. Allows a case-only change of the same name.
pub fn rename_tag(
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, TagError> {
    let old_name = from.trim();
    if old_name.is_empty() {
        return Err(TagError::BadRequest("from and to required".into()));
    }
    let new_name = normalize_new_name(to)?;
    let Some(id) = find_tag_id(conn, account_id, old_name)? else {
        return Err(TagError::NotFound("tag not found".into()));
    };
    if old_name.eq_ignore_ascii_case(&new_name) {
        if old_name == new_name {
            return Ok(new_name);
        }
    } else if let Some(other) = find_tag_id(conn, account_id, &new_name)?
        && other != id
    {
        return Err(TagError::Conflict("tag already exists".into()));
    }
    conn.execute(
        "UPDATE conversation_tags SET name = ?1 WHERE id = ?2 AND account_id = ?3",
        params![new_name, id, account_id],
    )?;
    Ok(new_name)
}

/// Delete a tag and its memberships.
pub fn delete_tag(conn: &Connection, account_id: &str, name: &str) -> Result<(), TagError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(TagError::BadRequest("name required".into()));
    }
    let Some(id) = find_tag_id(conn, account_id, trimmed)? else {
        return Err(TagError::NotFound("tag not found".into()));
    };
    conn.execute(
        "DELETE FROM conversation_tag_members WHERE tag_id = ?1",
        params![id],
    )?;
    conn.execute(
        "DELETE FROM conversation_tags WHERE id = ?1 AND account_id = ?2",
        params![id, account_id],
    )?;
    Ok(())
}

/// Conversation ids that currently have a named tag (case-insensitive).
pub fn list_tag_member_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, TagError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(TagError::BadRequest("name required".into()));
    }
    let mut stmt = conn.prepare(
        "SELECT ctm.conversation_id
         FROM conversation_tag_members ctm
         JOIN conversation_tags ct ON ct.id = ctm.tag_id
         WHERE ct.account_id = ?1 AND ct.name = ?2 COLLATE NOCASE
         ORDER BY ctm.conversation_id",
    )?;
    let rows = stmt.query_map(params![account_id, trimmed], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn conversation_exists(
    conn: &Connection,
    account_id: &str,
    conversation_id: i64,
) -> Result<bool, TagError> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM conversations WHERE id = ?1 AND account_id = ?2",
            params![conversation_id, account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Add or remove one tag for many conversations. Creates the tag when enabling.
pub fn set_conversations_tag_membership(
    conn: &Connection,
    account_id: &str,
    conversation_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, TagError> {
    let mut ids: Vec<i64> = conversation_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(TagError::BadRequest("conversation ids required".into()));
    }
    let tag_name = name.trim();
    if tag_name.is_empty() {
        return Err(TagError::BadRequest("tag name required".into()));
    }
    if is_reserved_tag_name(tag_name) {
        return Err(TagError::BadRequest(reserved_tag_error(tag_name)));
    }

    for id in &ids {
        if !conversation_exists(conn, account_id, *id)? {
            return Err(TagError::NotFound(format!("conversation {id} not found")));
        }
    }

    let tag_row_id = if enable {
        ensure_tag_id(conn, account_id, tag_name)?
    } else {
        match find_tag_id(conn, account_id, tag_name)? {
            Some(id) => id,
            None => return Ok(0),
        }
    };

    let mut changed = 0u64;
    for id in ids {
        let n = if enable {
            conn.execute(
                "INSERT OR IGNORE INTO conversation_tag_members (conversation_id, tag_id)
                 SELECT id, ?1 FROM conversations WHERE id = ?2 AND account_id = ?3",
                params![tag_row_id, id, account_id],
            )?
        } else {
            conn.execute(
                "DELETE FROM conversation_tag_members
                 WHERE conversation_id = ?1 AND tag_id = ?2
                   AND EXISTS (
                     SELECT 1 FROM conversations
                     WHERE conversations.id = conversation_tag_members.conversation_id
                       AND conversations.account_id = ?3
                   )",
                params![id, tag_row_id, account_id],
            )?
        };
        if n > 0 {
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    use crate::db::schema;

    fn setup() -> (Connection, String, i64, i64) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        let account = "00000000-0000-4000-8000-0000000000d1".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![&account],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![&account],
        )
        .unwrap();
        let h1 = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550200', '+15555550200', 'phone', 'phone')",
            params![&account],
        )
        .unwrap();
        let h2 = conn.last_insert_rowid();
        conn.execute(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, source_file
            ) VALUES (?1, ?2, 'individual', NULL, 't.json')
            "#,
            params![&account, h1],
        )
        .unwrap();
        let a = conn.last_insert_rowid();
        conn.execute(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, source_file
            ) VALUES (?1, ?2, 'individual', NULL, 't.json')
            "#,
            params![&account, h2],
        )
        .unwrap();
        let b = conn.last_insert_rowid();
        (conn, account, a, b)
    }

    #[test]
    fn create_list_rename_delete_tag() {
        let (conn, account, _, _) = setup();
        assert_eq!(create_tag(&conn, &account, " Holiday ").unwrap(), "Holiday");
        assert_eq!(list_tags(&conn, &account).unwrap(), vec!["Holiday"]);

        let err = create_tag(&conn, &account, "holiday").unwrap_err();
        assert!(matches!(err, TagError::Conflict(_)));

        let err = create_tag(&conn, &account, "Trash").unwrap_err();
        assert!(matches!(err, TagError::BadRequest(_)));

        assert_eq!(
            rename_tag(&conn, &account, "holiday", "Trip").unwrap(),
            "Trip"
        );
        assert_eq!(list_tags(&conn, &account).unwrap(), vec!["Trip"]);

        delete_tag(&conn, &account, "trip").unwrap();
        assert!(list_tags(&conn, &account).unwrap().is_empty());
    }

    #[test]
    fn membership_add_and_remove() {
        let (conn, account, a, b) = setup();
        assert_eq!(
            set_conversations_tag_membership(&conn, &account, &[a, b], "Holiday", true).unwrap(),
            2
        );
        assert_eq!(
            list_tag_member_ids(&conn, &account, "holiday").unwrap(),
            vec![a, b]
        );
        assert_eq!(
            tags_for_conversation(&conn, &account, a).unwrap(),
            vec!["Holiday"]
        );
        assert_eq!(
            set_conversations_tag_membership(&conn, &account, &[a], "Holiday", false).unwrap(),
            1
        );
        assert_eq!(
            list_tag_member_ids(&conn, &account, "Holiday").unwrap(),
            vec![b]
        );
    }
}
