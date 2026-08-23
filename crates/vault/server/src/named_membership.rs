//! Shared CRUD for named membership sets (thread tags and contact groups).
//!
//! Both domains store a named set (rows in a names table) whose members are
//! conversation or contact ids. The operations are identical apart from table
//! and column names, reserved names, and one post-change hook, so this module
//! implements them once behind [`MembershipSpec`].

use anyhow::Result as AnyResult;
use rusqlite::{Connection, OptionalExtension, params};

/// Longest allowed name for either kind of set (characters).
pub const MAX_NAME_LEN: usize = 80;

/// Create / rename / delete / membership failures for a named set.
#[derive(Debug)]
pub enum MembershipError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl From<rusqlite::Error> for MembershipError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Table names, labels, reserved names, and messages for one named set.
///
/// `name_column` and `member_column` live on the membership table;
/// `member_table` is the table members must exist in. All values are compile
/// time constants, so the SQL built from them is fixed at build time.
pub struct MembershipSpec {
    /// Names table (`conversation_tags` / `contact_groups`).
    pub table: &'static str,
    /// Membership table (`conversation_tag_members` / `contact_group_members`).
    pub members_table: &'static str,
    /// Column on the membership table that references the names table.
    pub name_column: &'static str,
    /// Column on the membership table that holds the member id.
    pub member_column: &'static str,
    /// Table members must exist in (`conversations` / `contacts`).
    pub member_table: &'static str,
    /// Singular label used in error messages (`"tag"` / `"group"`).
    pub label: &'static str,
    /// Member label used in error messages (`"conversation"` / `"contact"`).
    pub member_label: &'static str,
    /// Longest allowed name (characters).
    pub max_name_len: usize,
    /// Names that must not be created.
    pub reserved: &'static [&'static str],
    /// Reserved names with dedicated error messages (lowercase name, message).
    pub special_reserved: &'static [(&'static str, &'static str)],
    /// Extra work after a membership change (groups touch the contact row).
    #[allow(clippy::type_complexity)]
    pub on_change: Option<fn(&Connection, &str, i64) -> AnyResult<()>>,
}

/// Thread tags on conversations.
pub fn tag_spec() -> &'static MembershipSpec {
    static SPEC: MembershipSpec = MembershipSpec {
        table: "conversation_tags",
        members_table: "conversation_tag_members",
        name_column: "tag_id",
        member_column: "conversation_id",
        member_table: "conversations",
        label: "tag",
        member_label: "conversation",
        max_name_len: MAX_NAME_LEN,
        reserved: &[
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
        ],
        special_reserved: &[],
        on_change: None,
    };
    &SPEC
}

/// Contact groups on contacts.
pub fn group_spec() -> &'static MembershipSpec {
    static SPEC: MembershipSpec = MembershipSpec {
        table: "contact_groups",
        members_table: "contact_group_members",
        name_column: "group_id",
        member_column: "contact_id",
        member_table: "contacts",
        label: "group",
        member_label: "contact",
        max_name_len: MAX_NAME_LEN,
        reserved: &[
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
        ],
        special_reserved: &[
            ("contacts", "Contacts is a reserved group"),
            ("all", "All is a reserved group"),
            ("excluded", "Excluded is a reserved group"),
            ("unassigned", "Unassigned is a reserved group"),
            ("trash", "Trash is a reserved group"),
            ("no messages", "No messages is a reserved group"),
            ("no-messages", "No messages is a reserved group"),
            ("groups", "Group Messages is a reserved name"),
            ("group", "Group Messages is a reserved name"),
            ("group chats", "Group Messages is a reserved name"),
            ("group-chats", "Group Messages is a reserved name"),
            ("group chats 2", "Group Messages is a reserved name"),
            ("group-chats-2", "Group Messages is a reserved name"),
            ("group messages", "Group Messages is a reserved name"),
            ("group-messages", "Group Messages is a reserved name"),
            ("group messages 2", "Group Messages is a reserved name"),
            ("group-messages-2", "Group Messages is a reserved name"),
        ],
        on_change: Some(touch_member_owner),
    };
    &SPEC
}

fn touch_member_owner(conn: &Connection, account_id: &str, member_id: i64) -> AnyResult<()> {
    crate::db::contacts::touch_contact(conn, account_id, member_id)
}

fn find_id(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Option<i64>, MembershipError> {
    let sql = format!(
        "SELECT id FROM {table} WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE",
        table = spec.table
    );
    let id = conn
        .query_row(&sql, params![account_id, name], |row| row.get(0))
        .optional()?;
    Ok(id)
}

fn ensure_id(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<i64, MembershipError> {
    let name = normalize_name(spec, name)?;
    let sql = format!(
        "INSERT OR IGNORE INTO {table} (account_id, name) VALUES (?1, ?2)",
        table = spec.table
    );
    conn.execute(&sql, params![account_id, name])?;
    find_id(spec, conn, account_id, &name)?
        .ok_or_else(|| MembershipError::Internal(format!("failed to ensure {} {name}", spec.label)))
}

/// True when `name` is reserved and must not be created.
pub fn is_reserved(spec: &MembershipSpec, name: &str) -> bool {
    let key = name.trim().to_ascii_lowercase();
    spec.reserved.contains(&key.as_str())
}

fn reserved_error(spec: &MembershipSpec, name: &str) -> String {
    let key = name.trim().to_ascii_lowercase();
    for (reserved, message) in spec.special_reserved {
        if key == *reserved {
            return (*message).to_string();
        }
    }
    format!("\"{}\" is a reserved {}", name.trim(), spec.label)
}

fn normalize_name(spec: &MembershipSpec, name: &str) -> Result<String, MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    if trimmed.chars().count() > spec.max_name_len {
        return Err(MembershipError::BadRequest(format!(
            "name must be at most {} characters",
            spec.max_name_len
        )));
    }
    if is_reserved(spec, trimmed) {
        return Err(MembershipError::BadRequest(reserved_error(spec, trimmed)));
    }
    Ok(trimmed.to_string())
}

/// Names for this account, A–Z, excluding reserved leftovers.
pub fn list_names(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
) -> Result<Vec<String>, MembershipError> {
    let sql = format!(
        "SELECT name FROM {table} WHERE account_id = ?1 ORDER BY name COLLATE NOCASE",
        table = spec.table
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let name = row?;
        if !is_reserved(spec, &name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Create a name. Fails when the name is taken (ignoring case).
pub fn create_name(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<String, MembershipError> {
    let name = normalize_name(spec, name)?;
    if find_id(spec, conn, account_id, &name)?.is_some() {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "INSERT INTO {table} (account_id, name) VALUES (?1, ?2)",
        table = spec.table
    );
    conn.execute(&sql, params![account_id, name])?;
    Ok(name)
}

/// Rename a name. Allows a case-only change of the same name.
pub fn rename_name(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, MembershipError> {
    let old_name = from.trim();
    if old_name.is_empty() {
        return Err(MembershipError::BadRequest("from and to required".into()));
    }
    let new_name = normalize_name(spec, to)?;
    let Some(id) = find_id(spec, conn, account_id, old_name)? else {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    };
    if old_name.eq_ignore_ascii_case(&new_name) {
        if old_name == new_name {
            return Ok(new_name);
        }
    } else if let Some(other) = find_id(spec, conn, account_id, &new_name)?
        && other != id
    {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "UPDATE {table} SET name = ?1 WHERE id = ?2 AND account_id = ?3",
        table = spec.table
    );
    conn.execute(&sql, params![new_name, id, account_id])?;
    Ok(new_name)
}

/// Delete a name and its memberships.
pub fn delete_name(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<(), MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    let Some(id) = find_id(spec, conn, account_id, trimmed)? else {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    };
    let members_sql = format!(
        "DELETE FROM {mt} WHERE {nc} = ?1",
        mt = spec.members_table,
        nc = spec.name_column
    );
    conn.execute(&members_sql, params![id])?;
    let sql = format!(
        "DELETE FROM {table} WHERE id = ?1 AND account_id = ?2",
        table = spec.table
    );
    conn.execute(&sql, params![id, account_id])?;
    Ok(())
}

/// Member ids that currently belong to a named set (case-insensitive).
pub fn list_member_ids(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    let sql = format!(
        "SELECT m.{mc}
         FROM {mt} m
         JOIN {table} n ON n.id = m.{nc}
         WHERE n.account_id = ?1 AND n.name = ?2 COLLATE NOCASE
         ORDER BY m.{mc}",
        mc = spec.member_column,
        mt = spec.members_table,
        table = spec.table,
        nc = spec.name_column,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id, trimmed], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn member_exists(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    member_id: i64,
) -> Result<bool, MembershipError> {
    let sql = format!(
        "SELECT id FROM {mt} WHERE id = ?1 AND account_id = ?2",
        mt = spec.member_table
    );
    let found: Option<i64> = conn
        .query_row(&sql, params![member_id, account_id], |row| row.get(0))
        .optional()?;
    Ok(found.is_some())
}

/// Add or remove one name for many members. Creates the name when enabling.
pub fn set_membership(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    member_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, MembershipError> {
    let mut ids: Vec<i64> = member_ids.iter().copied().filter(|id| *id > 0).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(MembershipError::BadRequest(format!(
            "{} ids required",
            spec.member_label
        )));
    }
    let name_trimmed = name.trim();
    if name_trimmed.is_empty() {
        return Err(MembershipError::BadRequest(format!(
            "{} name required",
            spec.label
        )));
    }
    if is_reserved(spec, name_trimmed) {
        return Err(MembershipError::BadRequest(reserved_error(
            spec,
            name_trimmed,
        )));
    }

    for id in &ids {
        if !member_exists(spec, conn, account_id, *id)? {
            return Err(MembershipError::NotFound(format!(
                "{} {id} not found",
                spec.member_label
            )));
        }
    }

    let name_row_id = if enable {
        ensure_id(spec, conn, account_id, name_trimmed)?
    } else {
        match find_id(spec, conn, account_id, name_trimmed)? {
            Some(id) => id,
            None => return Ok(0),
        }
    };

    let mut changed = 0u64;
    for id in ids {
        let n = if enable {
            let sql = format!(
                "INSERT OR IGNORE INTO {mt} ({mc}, {nc})
                 SELECT id, ?1 FROM {member_table} WHERE id = ?2 AND account_id = ?3",
                mt = spec.members_table,
                mc = spec.member_column,
                nc = spec.name_column,
                member_table = spec.member_table,
            );
            conn.execute(&sql, params![name_row_id, id, account_id])?
        } else {
            let sql = format!(
                "DELETE FROM {mt}
                 WHERE {mc} = ?1 AND {nc} = ?2
                   AND EXISTS (
                     SELECT 1 FROM {member_table}
                     WHERE {member_table}.id = {mt}.{mc}
                       AND {member_table}.account_id = ?3
                   )",
                mt = spec.members_table,
                mc = spec.member_column,
                nc = spec.name_column,
                member_table = spec.member_table,
            );
            conn.execute(&sql, params![id, name_row_id, account_id])?
        };
        if n > 0 {
            changed += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, id).map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
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
        let account = "00000000-0000-4000-8000-0000000000d9".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![&account],
        )
        .unwrap();
        (conn, account)
    }

    #[test]
    fn reserved_names_rejected_with_exact_messages() {
        let (conn, account) = setup();
        let err = create_name(tag_spec(), &conn, &account, "Trash").unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "\"Trash\" is a reserved tag"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_name(group_spec(), &conn, &account, "Trash").unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "Trash is a reserved group"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_name(group_spec(), &conn, &account, "Group Chats").unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => {
                assert_eq!(msg, "Group Messages is a reserved name")
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn names_over_max_len_rejected() {
        let (conn, account) = setup();
        let long = "x".repeat(MAX_NAME_LEN + 1);
        let err = create_name(tag_spec(), &conn, &account, &long).unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => {
                assert_eq!(
                    msg,
                    format!("name must be at most {MAX_NAME_LEN} characters")
                )
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn on_change_hook_runs_on_membership_change() {
        let (conn, account) = setup();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Ada')",
            params![&account],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE contacts SET last_modified = '2000-01-01 00:00:00' WHERE id = ?1",
            params![contact_id],
        )
        .unwrap();

        assert_eq!(
            set_membership(group_spec(), &conn, &account, &[contact_id], "Family", true).unwrap(),
            1
        );
        let after: String = conn
            .query_row(
                "SELECT last_modified FROM contacts WHERE id = ?1",
                params![contact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(
            after, "2000-01-01 00:00:00",
            "group change must touch the contact"
        );
    }
}
