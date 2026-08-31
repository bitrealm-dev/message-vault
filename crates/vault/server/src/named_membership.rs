//! Shared CRUD for named membership sets (message tags and contact groups).
//!
//! Both domains store a named set (rows in a names table) whose members are
//! conversation or contact ids. The operations are identical apart from table
//! and column names, reserved names, and one post-change hook, so this module
//! implements them once behind [`MembershipSpec`].

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

fn touch_member_owner<'a>(
    conn: &'a mut AnyConnection,
    account_id: &'a str,
    member_id: i64,
) -> Pin<Box<dyn Future<Output = AnyResult<()>> + Send + 'a>> {
    Box::pin(crate::db::contacts::touch_contact(
        conn, account_id, member_id,
    ))
}

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
pub async fn list_names(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<String>, MembershipError> {
    let order = order_by_name_ci(engine_of(conn), "name");
    let sql = format!(
        "SELECT name FROM {table} WHERE account_id = $1 {order}",
        table = spec.table
    );
    let rows = sqlx::query_scalar::<_, String>(&sql)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut out = Vec::new();
    for name in rows {
        if !is_reserved(spec, &name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Create a name. Fails when the name is taken (ignoring case).
pub async fn create_name(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<String, MembershipError> {
    let name = normalize_name(spec, name)?;
    if find_id(spec, conn, account_id, &name).await?.is_some() {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "INSERT INTO {table} (account_id, name) VALUES ($1, $2)",
        table = spec.table
    );
    sqlx::query(&sql)
        .bind(account_id)
        .bind(&name)
        .execute(&mut *conn)
        .await?;
    Ok(name)
}

/// Rename a name. Allows a case-only change of the same name.
pub async fn rename_name(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, MembershipError> {
    let old_name = from.trim();
    if old_name.is_empty() {
        return Err(MembershipError::BadRequest("from and to required".into()));
    }
    let new_name = normalize_name(spec, to)?;
    let Some(id) = find_id(spec, conn, account_id, old_name).await? else {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    };
    if old_name.eq_ignore_ascii_case(&new_name) {
        if old_name == new_name {
            return Ok(new_name);
        }
    } else if let Some(other) = find_id(spec, conn, account_id, &new_name).await?
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

/// Delete a name and its memberships.
pub async fn delete_name(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<(), MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    let Some(id) = find_id(spec, conn, account_id, trimmed).await? else {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    };
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

/// Member ids that currently belong to a named set (case-insensitive).
pub async fn list_member_ids(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
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
         WHERE n.account_id = $1 AND {name_eq}
         ORDER BY m.{mc}",
        mc = spec.member_column,
        mt = spec.members_table,
        table = spec.table,
        nc = spec.name_column,
        name_eq = name_eq_ci(engine_of(conn), "name", "$2"),
    );
    let rows = sqlx::query_scalar::<_, i64>(&sql)
        .bind(account_id)
        .bind(trimmed)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

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

    use crate::db::engine;
    use crate::db::schema;

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000d9".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir, account)
    }

    #[tokio::test]
    async fn reserved_names_rejected_with_exact_messages() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let err = create_name(tag_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "\"Trash\" is a reserved tag"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_name(group_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "Trash is a reserved group"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_name(group_spec(), &mut conn, &account, "Group Chats")
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
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let long = "x".repeat(MAX_NAME_LEN + 1);
        let err = create_name(tag_spec(), &mut conn, &account, &long)
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
    async fn on_change_hook_runs_on_membership_change() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
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
}
