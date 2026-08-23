//! Contact groups stored in `contact_groups` / `contact_group_members`.

use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result as AnyResult;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::named_membership::{self, MembershipError, group_spec};
use crate::server::{
    ApiError, AppState, JoinBlocking, MembershipChangedResponse, lock_conn, require_full_access,
    resolve_auth,
};

/// Create / rename / delete / membership failures.
pub type GroupError = MembershipError;

/// Group names for this account, A–Z, excluding reserved leftovers.
pub fn list_groups(conn: &Connection, account_id: &str) -> Result<Vec<String>, GroupError> {
    named_membership::list_names(group_spec(), conn, account_id)
}

/// Create a group. Fails when the name is taken (ignoring case).
pub fn create_group(conn: &Connection, account_id: &str, name: &str) -> Result<String, GroupError> {
    named_membership::create_name(group_spec(), conn, account_id, name)
}

/// Rename a group. Allows a case-only change of the same name.
pub fn rename_group(
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, GroupError> {
    named_membership::rename_name(group_spec(), conn, account_id, from, to)
}

/// Delete a group and its memberships.
pub fn delete_group(conn: &Connection, account_id: &str, name: &str) -> Result<(), GroupError> {
    named_membership::delete_name(group_spec(), conn, account_id, name)
}

/// Contact ids that currently belong to a named group (case-insensitive).
pub fn list_group_member_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, GroupError> {
    named_membership::list_member_ids(group_spec(), conn, account_id, name)
}

/// Add or remove one group for many contacts. Creates the group when enabling.
pub fn set_contacts_group_membership(
    conn: &Connection,
    account_id: &str,
    contact_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, GroupError> {
    named_membership::set_membership(group_spec(), conn, account_id, contact_ids, name, enable)
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

fn map_group_error(err: GroupError) -> ApiError {
    match err {
        GroupError::BadRequest(m) => ApiError::BadRequest(m),
        GroupError::NotFound(m) => ApiError::NotFound(m),
        GroupError::Conflict(m) => ApiError::Conflict(m),
        GroupError::Internal(m) => ApiError::Internal(m),
    }
}

async fn with_group_conn<T, F>(
    db: Arc<StdMutex<Connection>>,
    task: &'static str,
    f: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> Result<T, GroupError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || -> Result<T, ApiError> {
        let conn = lock_conn(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
        f(&conn).map_err(map_group_error)
    })
    .await
    .join_map(task, |e| e)
}

/// A group name.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupNameBody {
    name: String,
}

/// Old and new group names.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupRenameBody {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ContactGroupMembersQuery {
    name: String,
}

/// Contact ids, group name, and enable flag.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupMembershipBody {
    ids: Vec<i64>,
    name: String,
    enable: bool,
}

/// The account's group names.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupsListResponse {
    groups: Vec<String>,
}

/// The affected group plus the updated list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupNamedListResponse {
    name: String,
    groups: Vec<String>,
}

/// The updated list after deletion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupDeleteResponse {
    ok: bool,
    groups: Vec<String>,
}

/// Contact ids in the named group.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ContactGroupMembersResponse {
    name: String,
    #[serde(rename = "memberContactIds")]
    member_contact_ids: Vec<i64>,
}

/// List the account's contact groups (A–Z, reserved names hidden).
#[utoipa::path(
    get,
    path = "/v1/contact-groups",
    tag = "Contacts",
    security(("bearer" = [])),
    responses(
        (status = 200, body = ContactGroupsListResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_groups_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ContactGroupsListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let groups = with_group_conn(db, "contact groups list", move |conn| {
        list_groups(conn, &auth.account_id)
    })
    .await?;
    Ok(Json(ContactGroupsListResponse { groups }))
}

/// Create a contact group and return the updated list.
#[utoipa::path(
    post,
    path = "/v1/contact-groups",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = ContactGroupNameBody,
    responses(
        (status = 200, body = ContactGroupNamedListResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_groups_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ContactGroupNameBody>,
) -> Result<Json<ContactGroupNamedListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let name = body.name;
    let (created, groups) = with_group_conn(db, "contact groups create", move |conn| {
        let created = create_group(conn, &auth.account_id, &name)?;
        let groups = list_groups(conn, &auth.account_id)?;
        Ok((created, groups))
    })
    .await?;
    Ok(Json(ContactGroupNamedListResponse {
        name: created,
        groups,
    }))
}

/// Rename a contact group and return the updated list.
#[utoipa::path(
    patch,
    path = "/v1/contact-groups",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = ContactGroupRenameBody,
    responses(
        (status = 200, body = ContactGroupNamedListResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_groups_rename_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ContactGroupRenameBody>,
) -> Result<Json<ContactGroupNamedListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let (name, groups) = with_group_conn(db, "contact groups rename", move |conn| {
        let name = rename_group(conn, &auth.account_id, &body.from, &body.to)?;
        let groups = list_groups(conn, &auth.account_id)?;
        Ok((name, groups))
    })
    .await?;
    Ok(Json(ContactGroupNamedListResponse { name, groups }))
}

/// Delete a contact group and return the updated list.
#[utoipa::path(
    delete,
    path = "/v1/contact-groups",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = ContactGroupNameBody,
    responses(
        (status = 200, body = ContactGroupDeleteResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_groups_delete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ContactGroupNameBody>,
) -> Result<Json<ContactGroupDeleteResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let groups = with_group_conn(db, "contact groups delete", move |conn| {
        delete_group(conn, &auth.account_id, &body.name)?;
        list_groups(conn, &auth.account_id)
    })
    .await?;
    Ok(Json(ContactGroupDeleteResponse { ok: true, groups }))
}

/// Contact ids that belong to a named group.
#[utoipa::path(
    get,
    path = "/v1/contact-groups/members",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("name" = String, Query, description = "Group name")),
    responses(
        (status = 200, body = ContactGroupMembersResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_groups_members_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ContactGroupMembersQuery>,
) -> Result<Json<ContactGroupMembersResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let name = query.name.clone();
    let member_contact_ids = with_group_conn(db, "contact groups members", move |conn| {
        list_group_member_ids(conn, &auth.account_id, &name)
    })
    .await?;
    Ok(Json(ContactGroupMembersResponse {
        name: query.name,
        member_contact_ids,
    }))
}

/// Add or remove contacts in a group.
#[utoipa::path(
    post,
    path = "/v1/contacts/groups",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = ContactGroupMembershipBody,
    responses(
        (status = 200, body = MembershipChangedResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_groups_membership_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ContactGroupMembershipBody>,
) -> Result<Json<MembershipChangedResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let changed = with_group_conn(db, "contact groups membership", move |conn| {
        set_contacts_group_membership(conn, &auth.account_id, &body.ids, &body.name, body.enable)
    })
    .await?;
    Ok(Json(MembershipChangedResponse { changed }))
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
