//! Contact groups stored in `contact_groups` / `contact_group_members`.
//!
//! CRUD and membership live in [`crate::named_membership`] behind
//! [`group_spec`]; this module owns the HTTP surface (routes, DTOs, OpenAPI).

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use crate::named_membership::{self, group_spec};
use crate::server::{ApiError, AppState, FullAccess, MembershipChangedResponse};

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
    FullAccess(auth): FullAccess,
) -> Result<Json<ContactGroupsListResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let groups = named_membership::list_names(group_spec(), &mut conn, &auth.account_id).await?;
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
    FullAccess(auth): FullAccess,
    Json(body): Json<ContactGroupNameBody>,
) -> Result<Json<ContactGroupNamedListResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let name = body.name;
    let created =
        named_membership::create_name(group_spec(), &mut conn, &auth.account_id, &name).await?;
    let groups = named_membership::list_names(group_spec(), &mut conn, &auth.account_id).await?;
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
    FullAccess(auth): FullAccess,
    Json(body): Json<ContactGroupRenameBody>,
) -> Result<Json<ContactGroupNamedListResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let name = named_membership::rename_name(
        group_spec(),
        &mut conn,
        &auth.account_id,
        &body.from,
        &body.to,
    )
    .await?;
    let groups = named_membership::list_names(group_spec(), &mut conn, &auth.account_id).await?;
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
    FullAccess(auth): FullAccess,
    Json(body): Json<ContactGroupNameBody>,
) -> Result<Json<ContactGroupDeleteResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    named_membership::delete_name(group_spec(), &mut conn, &auth.account_id, &body.name).await?;
    let groups = named_membership::list_names(group_spec(), &mut conn, &auth.account_id).await?;
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
    FullAccess(auth): FullAccess,
    Query(query): Query<ContactGroupMembersQuery>,
) -> Result<Json<ContactGroupMembersResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let member_contact_ids =
        named_membership::list_member_ids(group_spec(), &mut conn, &auth.account_id, &query.name)
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
    FullAccess(auth): FullAccess,
    Json(body): Json<ContactGroupMembershipBody>,
) -> Result<Json<MembershipChangedResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let changed = named_membership::set_membership(
        group_spec(),
        &mut conn,
        &auth.account_id,
        &body.ids,
        &body.name,
        body.enable,
    )
    .await?;
    Ok(Json(MembershipChangedResponse { changed }))
}

#[cfg(test)]
mod tests {
    use sqlx::AnyConnection;

    use crate::db::engine;
    use crate::db::schema;
    use crate::named_membership::{
        self, MembershipError, create_name, delete_name, group_spec, list_member_ids, list_names,
        rename_name, set_membership,
    };

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
            create_name(group_spec(), &mut conn, &account, " Family ")
                .await
                .unwrap(),
            "Family"
        );
        assert_eq!(
            list_names(group_spec(), &mut conn, &account).await.unwrap(),
            vec!["Family"]
        );

        let err = create_name(group_spec(), &mut conn, &account, "family")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::Conflict(_)));

        let err = create_name(group_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));

        assert_eq!(
            rename_name(group_spec(), &mut conn, &account, "family", "Work")
                .await
                .unwrap(),
            "Work"
        );
        assert_eq!(
            list_names(group_spec(), &mut conn, &account).await.unwrap(),
            vec!["Work"]
        );

        delete_name(group_spec(), &mut conn, &account, "work")
            .await
            .unwrap();
        assert!(
            list_names(group_spec(), &mut conn, &account)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn membership_add_and_remove() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let b = insert_contact(&mut conn, &account, "Ben").await;
        assert_eq!(
            set_membership(group_spec(), &mut conn, &account, &[a, b], "Family", true)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            list_member_ids(group_spec(), &mut conn, &account, "family")
                .await
                .unwrap(),
            vec![a, b]
        );
        assert_eq!(
            named_membership::names_for_item(group_spec(), &mut conn, &account, a)
                .await
                .unwrap(),
            vec!["Family"]
        );
        assert_eq!(
            set_membership(group_spec(), &mut conn, &account, &[a], "Family", false)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            list_member_ids(group_spec(), &mut conn, &account, "Family")
                .await
                .unwrap(),
            vec![b]
        );
    }
}
