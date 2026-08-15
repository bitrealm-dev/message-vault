//! Contact groups stored in `contact_groups` / `contact_group_members`.

use anyhow::Result as AnyResult;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::contacts::touch_contact;

/// Longest allowed group name (characters).
pub const MAX_GROUP_NAME_LEN: usize = 80;

/// Names that must not be created as user groups.
const RESERVED_GROUP_NAMES: &[&str] = &[
    "home",
    "contacts",
    "all",
    "excluded",
    "no-messages",
    "no messages",
    "unassigned",
    "trash",
    "groups",
    "group",
    "group-chats",
    "group chats",
    "group-chats-2",
    "group chats 2",
    "group-messages",
    "group messages",
    "group-messages-2",
    "group messages 2",
    "no-label",
    "no-group",
    "no group",
    "labels",
    "label",
    "no label",
];

/// Create / rename / delete / membership failures.
#[derive(Debug)]
pub enum GroupError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl From<rusqlite::Error> for GroupError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// True when `name` is reserved and must not be created.
pub fn is_reserved_group_name(name: &str) -> bool {
    let key = name.trim().to_ascii_lowercase();
    RESERVED_GROUP_NAMES.contains(&key.as_str())
}

fn reserved_group_error(name: &str) -> String {
    let key = name.trim().to_ascii_lowercase();
    match key.as_str() {
        "contacts" => "Contacts is a reserved group".into(),
        "all" => "All is a reserved group".into(),
        "excluded" => "Excluded is a reserved group".into(),
        "unassigned" => "Unassigned is a reserved group".into(),
        "trash" => "Trash is a reserved group".into(),
        "no messages" | "no-messages" => "No messages is a reserved group".into(),
        "groups" | "group" | "group chats" | "group-chats" | "group chats 2" | "group-chats-2"
        | "group messages" | "group-messages" | "group messages 2" | "group-messages-2" => {
            "Group Messages is a reserved name".into()
        }
        _ => format!("\"{}\" is a reserved group", name.trim()),
    }
}

fn normalize_new_name(name: &str) -> Result<String, GroupError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(GroupError::BadRequest("name required".into()));
    }
    if trimmed.chars().count() > MAX_GROUP_NAME_LEN {
        return Err(GroupError::BadRequest(format!(
            "name must be at most {MAX_GROUP_NAME_LEN} characters"
        )));
    }
    if is_reserved_group_name(trimmed) {
        return Err(GroupError::BadRequest(reserved_group_error(trimmed)));
    }
    Ok(trimmed.to_string())
}

/// Group names for this account, A–Z, excluding reserved leftovers.
pub fn list_groups(conn: &Connection, account_id: &str) -> Result<Vec<String>, GroupError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM contact_groups
         WHERE account_id = ?1
         ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let name = row?;
        if !is_reserved_group_name(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Groups attached to one contact, A–Z.
pub fn groups_for_contact(
    conn: &Connection,
    account_id: &str,
    contact_id: i64,
) -> AnyResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT cl.name
         FROM contact_groups cl
         JOIN contact_group_members m ON m.group_id = cl.id
         WHERE cl.account_id = ?1 AND m.contact_id = ?2
         ORDER BY cl.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![account_id, contact_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn find_group_id(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Option<i64>, GroupError> {
    let id = conn
        .query_row(
            "SELECT id FROM contact_groups
             WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE",
            params![account_id, name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(id)
}

fn ensure_group_id(conn: &Connection, account_id: &str, name: &str) -> Result<i64, GroupError> {
    let name = normalize_new_name(name)?;
    conn.execute(
        "INSERT OR IGNORE INTO contact_groups (account_id, name) VALUES (?1, ?2)",
        params![account_id, name],
    )?;
    find_group_id(conn, account_id, &name)?
        .ok_or_else(|| GroupError::Internal(format!("failed to ensure group {name}")))
}

/// Create a group. Fails when the name is taken (ignoring case).
pub fn create_group(conn: &Connection, account_id: &str, name: &str) -> Result<String, GroupError> {
    let name = normalize_new_name(name)?;
    if find_group_id(conn, account_id, &name)?.is_some() {
        return Err(GroupError::Conflict("group already exists".into()));
    }
    conn.execute(
        "INSERT INTO contact_groups (account_id, name) VALUES (?1, ?2)",
        params![account_id, name],
    )?;
    Ok(name)
}

/// Rename a group. Allows a case-only change of the same name.
pub fn rename_group(
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, GroupError> {
    let old_name = from.trim();
    if old_name.is_empty() {
        return Err(GroupError::BadRequest("from and to required".into()));
    }
    let new_name = normalize_new_name(to)?;
    let Some(id) = find_group_id(conn, account_id, old_name)? else {
        return Err(GroupError::NotFound("group not found".into()));
    };
    if old_name.eq_ignore_ascii_case(&new_name) {
        if old_name == new_name {
            return Ok(new_name);
        }
    } else if let Some(other) = find_group_id(conn, account_id, &new_name)?
        && other != id
    {
        return Err(GroupError::Conflict("group already exists".into()));
    }
    conn.execute(
        "UPDATE contact_groups SET name = ?1 WHERE id = ?2 AND account_id = ?3",
        params![new_name, id, account_id],
    )?;
    Ok(new_name)
}

/// Delete a group and its memberships.
pub fn delete_group(conn: &Connection, account_id: &str, name: &str) -> Result<(), GroupError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(GroupError::BadRequest("name required".into()));
    }
    let Some(id) = find_group_id(conn, account_id, trimmed)? else {
        return Err(GroupError::NotFound("group not found".into()));
    };
    conn.execute(
        "DELETE FROM contact_group_members WHERE group_id = ?1",
        params![id],
    )?;
    conn.execute(
        "DELETE FROM contact_groups WHERE id = ?1 AND account_id = ?2",
        params![id, account_id],
    )?;
    Ok(())
}

/// Contact ids that currently belong to a named group (case-insensitive).
pub fn list_group_member_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, GroupError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(GroupError::BadRequest("name required".into()));
    }
    let mut stmt = conn.prepare(
        "SELECT clm.contact_id
         FROM contact_group_members clm
         JOIN contact_groups cl ON cl.id = clm.group_id
         WHERE cl.account_id = ?1 AND cl.name = ?2 COLLATE NOCASE
         ORDER BY clm.contact_id",
    )?;
    let rows = stmt.query_map(params![account_id, trimmed], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn contact_exists(
    conn: &Connection,
    account_id: &str,
    contact_id: i64,
) -> Result<bool, GroupError> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM contacts WHERE id = ?1 AND account_id = ?2",
            params![contact_id, account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Add or remove one group for many contacts. Creates the group when enabling.
pub fn set_contacts_group_membership(
    conn: &Connection,
    account_id: &str,
    contact_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, GroupError> {
    let mut ids: Vec<i64> = contact_ids.iter().copied().filter(|id| *id > 0).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(GroupError::BadRequest("contact ids required".into()));
    }
    let group_name = name.trim();
    if group_name.is_empty() {
        return Err(GroupError::BadRequest("group name required".into()));
    }
    if is_reserved_group_name(group_name) {
        return Err(GroupError::BadRequest(reserved_group_error(group_name)));
    }

    for id in &ids {
        if !contact_exists(conn, account_id, *id)? {
            return Err(GroupError::NotFound(format!("contact {id} not found")));
        }
    }

    let group_row_id = if enable {
        ensure_group_id(conn, account_id, group_name)?
    } else {
        match find_group_id(conn, account_id, group_name)? {
            Some(id) => id,
            None => return Ok(0),
        }
    };

    let mut changed = 0u64;
    for id in ids {
        let n = if enable {
            conn.execute(
                "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id)
                 SELECT id, ?1 FROM contacts WHERE id = ?2 AND account_id = ?3",
                params![group_row_id, id, account_id],
            )?
        } else {
            conn.execute(
                "DELETE FROM contact_group_members
                 WHERE contact_id = ?1 AND group_id = ?2
                   AND EXISTS (
                     SELECT 1 FROM contacts
                     WHERE contacts.id = contact_group_members.contact_id
                       AND contacts.account_id = ?3
                   )",
                params![id, group_row_id, account_id],
            )?
        };
        if n > 0 {
            changed += 1;
            touch_contact(conn, account_id, id).map_err(|e| GroupError::Internal(e.to_string()))?;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    use crate::db::schema;

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        let account = "00000000-0000-4000-8000-0000000000c9".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![&account],
        )
        .unwrap();
        (conn, account)
    }

    fn insert_contact(conn: &Connection, account: &str, name: &str) -> i64 {
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, ?2)",
            params![account, name],
        )
        .unwrap();
        conn.query_row(
            "SELECT id FROM contacts WHERE account_id = ?1 ORDER BY id DESC LIMIT 1",
            params![account],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn create_list_rename_delete_group() {
        let (conn, account) = setup();
        assert_eq!(create_group(&conn, &account, " Family ").unwrap(), "Family");
        assert_eq!(list_groups(&conn, &account).unwrap(), vec!["Family"]);

        let err = create_group(&conn, &account, "family").unwrap_err();
        assert!(matches!(err, GroupError::Conflict(_)));

        let err = create_group(&conn, &account, "Trash").unwrap_err();
        assert!(matches!(err, GroupError::BadRequest(_)));

        assert_eq!(
            rename_group(&conn, &account, "family", "Work").unwrap(),
            "Work"
        );
        assert_eq!(list_groups(&conn, &account).unwrap(), vec!["Work"]);

        delete_group(&conn, &account, "work").unwrap();
        assert!(list_groups(&conn, &account).unwrap().is_empty());
    }

    #[test]
    fn membership_add_and_remove() {
        let (conn, account) = setup();
        let a = insert_contact(&conn, &account, "Ada");
        let b = insert_contact(&conn, &account, "Ben");
        assert_eq!(
            set_contacts_group_membership(&conn, &account, &[a, b], "Family", true).unwrap(),
            2
        );
        assert_eq!(
            list_group_member_ids(&conn, &account, "family").unwrap(),
            vec![a, b]
        );
        assert_eq!(
            groups_for_contact(&conn, &account, a).unwrap(),
            vec!["Family"]
        );
        assert_eq!(
            set_contacts_group_membership(&conn, &account, &[a], "Family", false).unwrap(),
            1
        );
        assert_eq!(
            list_group_member_ids(&conn, &account, "Family").unwrap(),
            vec![b]
        );
    }
}
