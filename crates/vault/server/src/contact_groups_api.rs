//! Contact groups stored in `contact_groups` / `contact_group_members`.

use anyhow::Result as AnyResult;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;

use crate::db::dialect::engine_of;
use crate::db::engine::DbEngine;
use crate::named_membership::{self, MembershipError, group_spec};
use crate::server::{
    ApiError, AppState, MembershipChangedResponse, require_full_access, resolve_auth,
};

/// Create / rename / delete / membership failures.
pub type GroupError = MembershipError;

/// Group names for this account, A–Z, excluding reserved leftovers.
pub async fn list_groups(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<String>, GroupError> {
    named_membership::list_names(group_spec(), conn, account_id).await
}

/// Create a group. Fails when the name is taken (ignoring case).
pub async fn create_group(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<String, GroupError> {
    named_membership::create_name(group_spec(), conn, account_id, name).await
}

/// Rename a group. Allows a case-only change of the same name.
pub async fn rename_group(
    conn: &mut AnyConnection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, GroupError> {
    named_membership::rename_name(group_spec(), conn, account_id, from, to).await
}

/// Delete a group and its memberships.
pub async fn delete_group(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<(), GroupError> {
    named_membership::delete_name(group_spec(), conn, account_id, name).await
}

/// Contact ids that currently belong to a named group (case-insensitive).
pub async fn list_group_member_ids(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, GroupError> {
    named_membership::list_member_ids(group_spec(), conn, account_id, name).await
}

/// Add or remove one group for many contacts. Creates the group when enabling.
pub async fn set_contacts_group_membership(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, GroupError> {
    named_membership::set_membership(group_spec(), conn, account_id, contact_ids, name, enable)
        .await
}

/// Groups attached to one contact, A–Z.
pub async fn groups_for_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
) -> AnyResult<Vec<String>> {
    let order = match engine_of(conn) {
        DbEngine::Sqlite => "ORDER BY cl.name COLLATE NOCASE",
        DbEngine::Postgres => "ORDER BY lower(cl.name)",
    };
    let sql = format!(
        "SELECT cl.name
         FROM contact_groups cl
         JOIN contact_group_members m ON m.group_id = cl.id
         WHERE cl.account_id = $1 AND m.contact_id = $2
         {order}"
    );
    let rows = sqlx::query_scalar::<_, String>(&sql)
        .bind(account_id)
        .bind(contact_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

fn map_group_error(err: GroupError) -> ApiError {
    match err {
        GroupError::BadRequest(m) => ApiError::BadRequest(m),
        GroupError::NotFound(m) => ApiError::NotFound(m),
        GroupError::Conflict(m) => ApiError::Conflict(m),
        GroupError::Internal(m) => ApiError::Internal(m),
    }
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
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let groups = list_groups(&mut conn, &auth.account_id)
        .await
        .map_err(map_group_error)?;
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
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let name = body.name;
    let created = create_group(&mut conn, &auth.account_id, &name)
        .await
        .map_err(map_group_error)?;
    let groups = list_groups(&mut conn, &auth.account_id)
        .await
        .map_err(map_group_error)?;
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
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let name = rename_group(&mut conn, &auth.account_id, &body.from, &body.to)
        .await
        .map_err(map_group_error)?;
    let groups = list_groups(&mut conn, &auth.account_id)
        .await
        .map_err(map_group_error)?;
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
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    delete_group(&mut conn, &auth.account_id, &body.name)
        .await
        .map_err(map_group_error)?;
    let groups = list_groups(&mut conn, &auth.account_id)
        .await
        .map_err(map_group_error)?;
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
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let member_contact_ids = list_group_member_ids(&mut conn, &auth.account_id, &query.name)
        .await
        .map_err(map_group_error)?;
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
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let changed = set_contacts_group_membership(
        &mut conn,
        &auth.account_id,
        &body.ids,
        &body.name,
        body.enable,
    )
    .await
    .map_err(map_group_error)?;
    Ok(Json(MembershipChangedResponse { changed }))
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
        let account = "00000000-0000-4000-8000-0000000000c9".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir, account)
    }

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
    async fn create_list_rename_delete_group() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            create_group(&mut conn, &account, " Family ").await.unwrap(),
            "Family"
        );
        assert_eq!(
            list_groups(&mut conn, &account).await.unwrap(),
            vec!["Family"]
        );

        let err = create_group(&mut conn, &account, "family")
            .await
            .unwrap_err();
        assert!(matches!(err, GroupError::Conflict(_)));

        let err = create_group(&mut conn, &account, "Trash")
            .await
            .unwrap_err();
        assert!(matches!(err, GroupError::BadRequest(_)));

        assert_eq!(
            rename_group(&mut conn, &account, "family", "Work")
                .await
                .unwrap(),
            "Work"
        );
        assert_eq!(
            list_groups(&mut conn, &account).await.unwrap(),
            vec!["Work"]
        );

        delete_group(&mut conn, &account, "work").await.unwrap();
        assert!(list_groups(&mut conn, &account).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn membership_add_and_remove() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let b = insert_contact(&mut conn, &account, "Ben").await;
        assert_eq!(
            set_contacts_group_membership(&mut conn, &account, &[a, b], "Family", true)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            list_group_member_ids(&mut conn, &account, "family")
                .await
                .unwrap(),
            vec![a, b]
        );
        assert_eq!(
            groups_for_contact(&mut conn, &account, a).await.unwrap(),
            vec!["Family"]
        );
        assert_eq!(
            set_contacts_group_membership(&mut conn, &account, &[a], "Family", false)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            list_group_member_ids(&mut conn, &account, "Family")
                .await
                .unwrap(),
            vec![b]
        );
    }
}
