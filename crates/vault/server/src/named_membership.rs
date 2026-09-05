//! Shared storage for named sets (Message Tags and Contact Groups).
//!
//! Both domains store a named set (rows in a names table) whose members are
//! conversation or contact ids. The HTTP layer addresses a set by id through
//! `list_sets`, `get_set`, `create_set`, `rename_set`, `delete_set`,
//! `list_member_ids_of`, and `patch_members`. The import path still fills a
//! group by name through `set_membership`, which creates the name on demand.
//! The operations are identical apart from table and column names, reserved
//! names, and one post-change hook, so this module implements them once
//! behind [`MembershipSpec`].

use std::future::Future;
use std::pin::Pin;

use anyhow::Result as AnyResult;
use sqlx::AnyConnection;

use crate::db::dialect::{engine_of, name_eq_ci, order_by_name_ci};

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

impl From<sqlx::Error> for MembershipError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<MembershipError> for crate::server::ApiError {
    fn from(e: MembershipError) -> Self {
        match e {
            MembershipError::BadRequest(m) => Self::BadRequest(m),
            MembershipError::NotFound(m) => Self::NotFound(m),
            MembershipError::Conflict(m) => Self::Conflict(m),
            MembershipError::Internal(m) => Self::Internal(m),
        }
    }
}

/// Extra work after a membership change, async over the connection borrow.
type ChangeHook = for<'a> fn(
    &'a mut AnyConnection,
    &'a str,
    i64,
) -> Pin<Box<dyn Future<Output = AnyResult<()>> + Send + 'a>>;

/// Table names, labels, reserved names, and messages for one named set.
///
/// `name_column` and `member_column` live on the membership table;
/// `member_table` is the table members must exist in. All values are compile
/// time constants, so the SQL built from them is fixed at build time.
pub struct MembershipSpec {
    /// Names table (`message_tags` / `contact_groups`).
    pub table: &'static str,
    /// Membership table (`message_tag_members` / `contact_group_members`).
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
    pub on_change: Option<ChangeHook>,
}

/// Message tags on conversations.
pub fn tag_spec() -> &'static MembershipSpec {
    static SPEC: MembershipSpec = MembershipSpec {
        table: "message_tags",
        members_table: "message_tag_members",
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

/// Bump the member contact's updated-at, boxed so the spec table can hold it as a plain function pointer.
fn touch_member_owner<'a>(
    conn: &'a mut AnyConnection,
    account_id: &'a str,
    member_id: i64,
) -> Pin<Box<dyn Future<Output = AnyResult<()>> + Send + 'a>> {
    Box::pin(crate::db::contacts::touch_contact(
        conn, account_id, member_id,
    ))
}

/// Id of the named set called `name`, if it exists.
async fn find_id(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<Option<i64>, MembershipError> {
    let sql = format!(
        "SELECT id FROM {table} WHERE account_id = $1 AND {name_eq}",
        table = spec.table,
        name_eq = name_eq_ci(engine_of(conn), "name", "$2"),
    );
    let id = sqlx::query_scalar::<_, i64>(&sql)
        .bind(account_id)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(id)
}

/// Id of the named set called `name`, creating it if needed.
async fn ensure_id(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<i64, MembershipError> {
    let name = normalize_name(spec, name)?;
    let sql = format!(
        "INSERT INTO {table} (account_id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        table = spec.table
    );
    sqlx::query(&sql)
        .bind(account_id)
        .bind(&name)
        .execute(&mut *conn)
        .await?;
    find_id(spec, conn, account_id, &name)
        .await?
        .ok_or_else(|| MembershipError::Internal(format!("failed to ensure {} {name}", spec.label)))
}

/// True when `name` is reserved and must not be created.
pub fn is_reserved(spec: &MembershipSpec, name: &str) -> bool {
    let key = name.trim().to_ascii_lowercase();
    spec.reserved.contains(&key.as_str())
}

/// The message for a reserved name: the spec's specific one, or the generic one.
fn reserved_error(spec: &MembershipSpec, name: &str) -> String {
    let key = name.trim().to_ascii_lowercase();
    for (reserved, message) in spec.special_reserved {
        if key == *reserved {
            return (*message).to_string();
        }
    }
    format!("\"{}\" is a reserved {}", name.trim(), spec.label)
}

/// Trim and validate a set name against the spec's length and reserved-name rules.
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

/// True when the member row belongs to this account.
async fn member_exists(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    member_id: i64,
) -> Result<bool, MembershipError> {
    let sql = format!(
        "SELECT id FROM {mt} WHERE id = $1 AND account_id = $2",
        mt = spec.member_table
    );
    let found: Option<i64> = sqlx::query_scalar::<_, i64>(&sql)
        .bind(member_id)
        .bind(account_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(found.is_some())
}

/// Add or remove one name for many members. Creates the name when enabling.
pub async fn set_membership(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    member_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, MembershipError> {
    let ids = clean_ids(member_ids);
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
        if !member_exists(spec, conn, account_id, *id).await? {
            return Err(MembershipError::NotFound(format!(
                "{} {id} not found",
                spec.member_label
            )));
        }
    }

    let name_row_id = if enable {
        ensure_id(spec, conn, account_id, name_trimmed).await?
    } else {
        match find_id(spec, conn, account_id, name_trimmed).await? {
            Some(id) => id,
            None => return Ok(0),
        }
    };

    let mut changed = 0u64;
    for id in ids {
        let n = if enable {
            let sql = format!(
                "INSERT INTO {mt} ({mc}, {nc})
                 SELECT id, $1 FROM {member_table} WHERE id = $2 AND account_id = $3
                 ON CONFLICT DO NOTHING",
                mt = spec.members_table,
                mc = spec.member_column,
                nc = spec.name_column,
                member_table = spec.member_table,
            );
            sqlx::query(&sql)
                .bind(name_row_id)
                .bind(id)
                .bind(account_id)
                .execute(&mut *conn)
                .await?
                .rows_affected()
        } else {
            let sql = format!(
                "DELETE FROM {mt}
                 WHERE {mc} = $1 AND {nc} = $2
                   AND EXISTS (
                     SELECT 1 FROM {member_table}
                     WHERE {member_table}.id = {mt}.{mc}
                       AND {member_table}.account_id = $3
                   )",
                mt = spec.members_table,
                mc = spec.member_column,
                nc = spec.name_column,
                member_table = spec.member_table,
            );
            sqlx::query(&sql)
                .bind(id)
                .bind(name_row_id)
                .bind(account_id)
                .execute(&mut *conn)
                .await?
                .rows_affected()
        };
        if n > 0 {
            changed += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, id)
                    .await
                    .map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
        }
    }
    Ok(changed)
}

/// Drop non-positive ids, sort, and dedupe a caller's member id list.
fn clean_ids(ids: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = ids.iter().copied().filter(|id| *id > 0).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Sets for this account with their ids, A–Z, excluding reserved leftovers.
pub async fn list_sets(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<(i64, String)>, MembershipError> {
    let order = order_by_name_ci(engine_of(conn), "name");
    let sql = format!(
        "SELECT id, name FROM {table} WHERE account_id = $1 {order}",
        table = spec.table
    );
    let rows = sqlx::query_as::<_, (i64, String)>(&sql)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .filter(|(_, name)| !is_reserved(spec, name))
        .collect())
}

/// One set by id, or `NotFound` when it is not this account's.
pub async fn get_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<(i64, String), MembershipError> {
    let sql = format!(
        "SELECT id, name FROM {table} WHERE id = $1 AND account_id = $2",
        table = spec.table
    );
    let row = sqlx::query_as::<_, (i64, String)>(&sql)
        .bind(id)
        .bind(account_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| MembershipError::NotFound(format!("{} not found", spec.label)))?;
    // A reserved-name row can only be a leftover (create_set and rename_set
    // both refuse reserved names): list_sets never shows it, so its id must
    // not work either.
    if is_reserved(spec, &row.1) {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    }
    Ok(row)
}

/// Create a set and answer its id and trimmed name. Fails when the name is
/// taken (ignoring case) or reserved.
pub async fn create_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<(i64, String), MembershipError> {
    let name = normalize_name(spec, name)?;
    if find_id(spec, conn, account_id, &name).await?.is_some() {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "INSERT INTO {table} (account_id, name) VALUES ($1, $2) RETURNING id",
        table = spec.table
    );
    let id: i64 = sqlx::query_scalar(&sql)
        .bind(account_id)
        .bind(&name)
        .fetch_one(&mut *conn)
        .await?;
    Ok((id, name))
}

/// Rename a set by id. A case-only change of its own name is allowed; another
/// set's name (ignoring case) is a conflict.
pub async fn rename_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
    name: &str,
) -> Result<String, MembershipError> {
    let (_, old_name) = get_set(spec, conn, account_id, id).await?;
    let new_name = normalize_name(spec, name)?;
    if old_name == new_name {
        return Ok(new_name);
    }
    if let Some(other) = find_id(spec, conn, account_id, &new_name).await?
        && other != id
    {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "UPDATE {table} SET name = $1 WHERE id = $2 AND account_id = $3",
        table = spec.table
    );
    sqlx::query(&sql)
        .bind(&new_name)
        .bind(id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(new_name)
}

/// Delete a set by id, and its memberships.
pub async fn delete_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<(), MembershipError> {
    get_set(spec, conn, account_id, id).await?;
    let members_sql = format!(
        "DELETE FROM {mt} WHERE {nc} = $1",
        mt = spec.members_table,
        nc = spec.name_column
    );
    sqlx::query(&members_sql)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let sql = format!(
        "DELETE FROM {table} WHERE id = $1 AND account_id = $2",
        table = spec.table
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Member ids of one set, ascending.
pub async fn list_member_ids_of(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<Vec<i64>, MembershipError> {
    get_set(spec, conn, account_id, id).await?;
    let sql = format!(
        "SELECT {mc} FROM {mt} WHERE {nc} = $1 ORDER BY {mc}",
        mc = spec.member_column,
        mt = spec.members_table,
        nc = spec.name_column,
    );
    let rows = sqlx::query_scalar::<_, i64>(&sql)
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Add and remove members of one set in one call, answering
/// `(added, removed)`. Every id is checked before anything is written, so a
/// foreign or unknown member id leaves the set as it was. An id present in
/// both `add` and `remove` nets to "removed": it is dropped from `add` so it
/// is deleted, not inserted then deleted, and the `on_change` hook fires
/// once for it rather than twice.
pub async fn patch_members(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
    add: &[i64],
    remove: &[i64],
) -> Result<(u64, u64), MembershipError> {
    get_set(spec, conn, account_id, id).await?;
    let remove = clean_ids(remove);
    let add: Vec<i64> = clean_ids(add)
        .into_iter()
        .filter(|id| !remove.contains(id))
        .collect();
    if add.is_empty() && remove.is_empty() {
        return Err(MembershipError::BadRequest(format!(
            "{} ids required",
            spec.member_label
        )));
    }
    for member in add.iter().chain(remove.iter()) {
        if !member_exists(spec, conn, account_id, *member).await? {
            return Err(MembershipError::NotFound(format!(
                "{} {member} not found",
                spec.member_label
            )));
        }
    }

    let insert_sql = format!(
        "INSERT INTO {mt} ({mc}, {nc})
         SELECT id, $1 FROM {member_table} WHERE id = $2 AND account_id = $3
         ON CONFLICT DO NOTHING",
        mt = spec.members_table,
        mc = spec.member_column,
        nc = spec.name_column,
        member_table = spec.member_table,
    );
    let delete_sql = format!(
        "DELETE FROM {mt}
         WHERE {mc} = $1 AND {nc} = $2
           AND EXISTS (
             SELECT 1 FROM {member_table}
             WHERE {member_table}.id = {mt}.{mc}
               AND {member_table}.account_id = $3
           )",
        mt = spec.members_table,
        mc = spec.member_column,
        nc = spec.name_column,
        member_table = spec.member_table,
    );

    let mut added = 0u64;
    for member in add {
        let n = sqlx::query(&insert_sql)
            .bind(id)
            .bind(member)
            .bind(account_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if n > 0 {
            added += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, member)
                    .await
                    .map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
        }
    }
    let mut removed = 0u64;
    for member in remove {
        let n = sqlx::query(&delete_sql)
            .bind(member)
            .bind(id)
            .bind(account_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if n > 0 {
            removed += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, member)
                    .await
                    .map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
        }
    }
    Ok((added, removed))
}

/// Names attached to one member, A–Z.
pub async fn names_for_item(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    item_id: i64,
) -> AnyResult<Vec<String>> {
    let order = order_by_name_ci(engine_of(conn), "n.name");
    let sql = format!(
        "SELECT n.name
         FROM {table} n
         JOIN {members} m ON m.{name_col} = n.id
         WHERE n.account_id = $1 AND m.{member_col} = $2
         {order}",
        table = spec.table,
        members = spec.members_table,
        name_col = spec.name_column,
        member_col = spec.member_column,
    );
    let rows = sqlx::query_scalar::<_, String>(&sql)
        .bind(account_id)
        .bind(item_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Names attached to each member id, A–Z within each list.
pub async fn names_for_items(
    spec: &'static MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    item_ids: &[i64],
) -> AnyResult<std::collections::HashMap<i64, Vec<String>>> {
    use crate::db::sql::{fold_in_id_chunks, in_placeholders};
    let account_id = account_id.to_string();
    fold_in_id_chunks(conn, item_ids, |conn, chunk| {
        let account_id = account_id.clone();
        Box::pin(async move {
            let placeholders = in_placeholders(2, chunk.len());
            let order = order_by_name_ci(engine_of(conn), "n.name");
            let sql = format!(
                "SELECT m.{member_col}, n.name
                 FROM {members} m
                 JOIN {table} n ON n.id = m.{name_col}
                 WHERE n.account_id = $1 AND m.{member_col} IN ({placeholders})
                 {order}",
                table = spec.table,
                members = spec.members_table,
                name_col = spec.name_column,
                member_col = spec.member_column,
            );
            let mut q = sqlx::query_as::<_, (i64, String)>(&sql).bind(&account_id);
            for id in chunk {
                q = q.bind(*id);
            }
            let rows = q.fetch_all(&mut *conn).await?;
            Ok(rows)
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_contact(conn: &mut AnyConnection, account: &str, name: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(account)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn reserved_names_rejected_with_exact_messages() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let err = create_set(tag_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "\"Trash\" is a reserved tag"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_set(group_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "Trash is a reserved group"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_set(group_spec(), &mut conn, &account, "Group Chats")
            .await
            .unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => {
                assert_eq!(msg, "Group Messages is a reserved name")
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn names_over_max_len_rejected() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let long = "x".repeat(MAX_NAME_LEN + 1);
        let err = create_set(tag_spec(), &mut conn, &account, &long)
            .await
            .unwrap_err();
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

    #[tokio::test]
    async fn create_set_refuses_an_empty_name() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let err = create_set(group_spec(), &mut conn, &account, "   ")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));
    }

    #[tokio::test]
    async fn rename_set_refuses_an_empty_or_over_long_name() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();

        let err = rename_set(group_spec(), &mut conn, &account, id, "   ")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));

        let long = "x".repeat(MAX_NAME_LEN + 1);
        let err = rename_set(group_spec(), &mut conn, &account, id, &long)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));
    }

    #[tokio::test]
    async fn on_change_hook_runs_on_membership_change() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        sqlx::query("INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Ada')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Ada') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("UPDATE contacts SET last_modified = '2000-01-01 00:00:00' WHERE id = $1")
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        assert_eq!(
            set_membership(
                group_spec(),
                &mut conn,
                &account,
                &[contact_id],
                "Family",
                true
            )
            .await
            .unwrap(),
            1
        );
        let after: String = sqlx::query_scalar("SELECT last_modified FROM contacts WHERE id = $1")
            .bind(contact_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_ne!(
            after, "2000-01-01 00:00:00",
            "group change must touch the contact"
        );
    }

    #[tokio::test]
    async fn create_and_list_sets_answer_ids_and_names_a_to_z() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let (work_id, work) = create_set(group_spec(), &mut conn, &account, " Work ")
            .await
            .unwrap();
        assert_eq!(work, "Work");
        let (family_id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        assert_ne!(work_id, family_id);
        assert_eq!(
            list_sets(group_spec(), &mut conn, &account).await.unwrap(),
            vec![
                (family_id, "Family".to_string()),
                (work_id, "Work".to_string())
            ]
        );
        assert_eq!(
            get_set(group_spec(), &mut conn, &account, work_id)
                .await
                .unwrap(),
            (work_id, "Work".to_string())
        );
    }

    #[tokio::test]
    async fn create_set_refuses_duplicates_and_reserved_names() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        let err = create_set(group_spec(), &mut conn, &account, "family")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::Conflict(_)));
        let err = create_set(group_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));
    }

    #[tokio::test]
    async fn rename_set_allows_a_case_change_and_refuses_another_sets_name() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let (family_id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        let (work_id, _) = create_set(group_spec(), &mut conn, &account, "Work")
            .await
            .unwrap();
        assert_eq!(
            rename_set(group_spec(), &mut conn, &account, family_id, "FAMILY")
                .await
                .unwrap(),
            "FAMILY"
        );
        let err = rename_set(group_spec(), &mut conn, &account, work_id, "family")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::Conflict(_)));
        let err = rename_set(group_spec(), &mut conn, &account, 999_999, "Anything")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_set_drops_its_memberships_and_refuses_an_unknown_id() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        patch_members(group_spec(), &mut conn, &account, id, &[a], &[])
            .await
            .unwrap();
        delete_set(group_spec(), &mut conn, &account, id)
            .await
            .unwrap();
        assert!(
            list_sets(group_spec(), &mut conn, &account)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            names_for_item(group_spec(), &mut conn, &account, a)
                .await
                .unwrap()
                .is_empty()
        );
        let err = delete_set(group_spec(), &mut conn, &account, id)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
    }

    #[tokio::test]
    async fn patch_members_adds_and_removes_in_one_call() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let b = insert_contact(&mut conn, &account, "Ben").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        assert_eq!(
            patch_members(group_spec(), &mut conn, &account, id, &[a, b, b], &[])
                .await
                .unwrap(),
            (2, 0)
        );
        assert_eq!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap(),
            vec![a, b]
        );
        assert_eq!(
            patch_members(group_spec(), &mut conn, &account, id, &[a], &[b])
                .await
                .unwrap(),
            (0, 1)
        );
        assert_eq!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap(),
            vec![a]
        );
        let err = patch_members(group_spec(), &mut conn, &account, id, &[], &[])
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));
    }

    #[tokio::test]
    async fn patch_members_with_a_foreign_member_writes_nothing() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        let err = patch_members(group_spec(), &mut conn, &account, id, &[a, 999_999], &[])
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
        assert!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn another_accounts_set_is_not_found() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let other = "00000000-0000-4000-8000-0000000000ca";
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'bob')")
            .bind(other)
            .execute(&mut *conn)
            .await
            .unwrap();
        let (id, _) = create_set(tag_spec(), &mut conn, other, "Holiday")
            .await
            .unwrap();
        let err = get_set(tag_spec(), &mut conn, &account, id)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
        assert!(
            list_sets(tag_spec(), &mut conn, &account)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn get_set_does_not_find_a_reserved_name_leftover() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        // create_set and rename_set both refuse reserved names, so the only
        // way a reserved-name row exists is a leftover from before that
        // check existed (or a direct insert, as here).
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO contact_groups (account_id, name) VALUES ($1, 'Trash') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let err = get_set(group_spec(), &mut conn, &account, id)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
    }

    #[tokio::test]
    async fn patch_members_an_id_in_both_add_and_remove_nets_to_removed() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();

        // Already a member: add and remove the same id nets to "removed",
        // and the change hook fires once, not twice.
        patch_members(group_spec(), &mut conn, &account, id, &[a], &[])
            .await
            .unwrap();
        assert_eq!(
            patch_members(group_spec(), &mut conn, &account, id, &[a], &[a])
                .await
                .unwrap(),
            (0, 1)
        );
        assert!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap()
                .is_empty()
        );

        // Never a member: add and remove the same id changes nothing.
        let b = insert_contact(&mut conn, &account, "Ben").await;
        assert_eq!(
            patch_members(group_spec(), &mut conn, &account, id, &[b], &[b])
                .await
                .unwrap(),
            (0, 0)
        );
        assert!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// The import path still fills groups by name through `set_membership`.
    #[tokio::test]
    async fn set_membership_by_name_still_creates_and_fills_a_group() {
        let vault = crate::test_support::test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000d9", "alice")
            .await;
        let mut conn = vault.conn().await;
        let a = insert_contact(&mut conn, &account, "Ada").await;
        assert_eq!(
            set_membership(group_spec(), &mut conn, &account, &[a], "Family", true)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            names_for_item(group_spec(), &mut conn, &account, a)
                .await
                .unwrap(),
            vec!["Family"]
        );
    }
}
