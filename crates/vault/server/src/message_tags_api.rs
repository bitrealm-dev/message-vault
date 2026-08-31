//! Message tags stored in `message_tags` / `message_tag_members`.
//!
//! CRUD and membership live in [`crate::named_membership`] behind
//! [`tag_spec`]; this module owns the HTTP surface (routes, DTOs, OpenAPI).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::named_membership::{self, tag_spec};
use crate::server::{
    ApiError, AppState, MembershipChangedResponse, require_full_access, resolve_auth,
};

/// A tag name.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MessageTagNameBody {
    name: String,
}

/// Old and new tag names.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MessageTagRenameBody {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessageTagMembersQuery {
    name: String,
}

/// Conversation ids, tag name, and enable flag.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MessageTagMembershipBody {
    ids: Vec<i64>,
    name: String,
    enable: bool,
}

/// The account's tag names.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MessageTagsListResponse {
    tags: Vec<String>,
}

/// The affected tag plus the updated list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MessageTagNamedListResponse {
    name: String,
    tags: Vec<String>,
}

/// The updated list after deletion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MessageTagDeleteResponse {
    ok: bool,
    tags: Vec<String>,
}

/// Conversation ids carrying the named tag.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MessageTagMembersResponse {
    name: String,
    #[serde(rename = "memberConversationIds")]
    member_conversation_ids: Vec<i64>,
}

/// List the account's message tags (A–Z, reserved names hidden).
#[utoipa::path(
    get,
    path = "/v1/message-tags",
    tag = "Message tags",
    security(("bearer" = [])),
    responses(
        (status = 200, body = MessageTagsListResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn message_tags_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MessageTagsListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let tags = named_membership::list_names(tag_spec(), &mut conn, &auth.account_id).await?;
    Ok(Json(MessageTagsListResponse { tags }))
}

/// Create a message tag and return the updated list.
#[utoipa::path(
    post,
    path = "/v1/message-tags",
    tag = "Message tags",
    security(("bearer" = [])),
    request_body = MessageTagNameBody,
    responses(
        (status = 200, body = MessageTagNamedListResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn message_tags_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MessageTagNameBody>,
) -> Result<Json<MessageTagNamedListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let name = body.name;
    let created =
        named_membership::create_name(tag_spec(), &mut conn, &auth.account_id, &name).await?;
    let tags = named_membership::list_names(tag_spec(), &mut conn, &auth.account_id).await?;
    Ok(Json(MessageTagNamedListResponse {
        name: created,
        tags,
    }))
}

/// Rename a message tag and return the updated list.
#[utoipa::path(
    patch,
    path = "/v1/message-tags",
    tag = "Message tags",
    security(("bearer" = [])),
    request_body = MessageTagRenameBody,
    responses(
        (status = 200, body = MessageTagNamedListResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn message_tags_rename_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MessageTagRenameBody>,
) -> Result<Json<MessageTagNamedListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let name = named_membership::rename_name(
        tag_spec(),
        &mut conn,
        &auth.account_id,
        &body.from,
        &body.to,
    )
    .await?;
    let tags = named_membership::list_names(tag_spec(), &mut conn, &auth.account_id).await?;
    Ok(Json(MessageTagNamedListResponse { name, tags }))
}

/// Delete a message tag and return the updated list.
#[utoipa::path(
    delete,
    path = "/v1/message-tags",
    tag = "Message tags",
    security(("bearer" = [])),
    request_body = MessageTagNameBody,
    responses(
        (status = 200, body = MessageTagDeleteResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn message_tags_delete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MessageTagNameBody>,
) -> Result<Json<MessageTagDeleteResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    named_membership::delete_name(tag_spec(), &mut conn, &auth.account_id, &body.name).await?;
    let tags = named_membership::list_names(tag_spec(), &mut conn, &auth.account_id).await?;
    Ok(Json(MessageTagDeleteResponse { ok: true, tags }))
}

/// Conversation ids that carry a named tag.
#[utoipa::path(
    get,
    path = "/v1/message-tags/members",
    tag = "Message tags",
    security(("bearer" = [])),
    params(("name" = String, Query, description = "Tag name")),
    responses(
        (status = 200, body = MessageTagMembersResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn message_tags_members_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MessageTagMembersQuery>,
) -> Result<Json<MessageTagMembersResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let member_conversation_ids =
        named_membership::list_member_ids(tag_spec(), &mut conn, &auth.account_id, &query.name)
            .await?;
    Ok(Json(MessageTagMembersResponse {
        name: query.name,
        member_conversation_ids,
    }))
}

/// Add or remove a tag on conversations.
#[utoipa::path(
    post,
    path = "/v1/conversations/tags",
    tag = "Message tags",
    security(("bearer" = [])),
    request_body = MessageTagMembershipBody,
    responses(
        (status = 200, body = MembershipChangedResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn message_tags_membership_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MessageTagMembershipBody>,
) -> Result<Json<MembershipChangedResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let changed = named_membership::set_membership(
        tag_spec(),
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
    use crate::db::engine;
    use crate::db::schema;
    use crate::named_membership::{
        self, MembershipError, create_name, delete_name, list_member_ids, list_names, rename_name,
        set_membership, tag_spec,
    };

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String, i64, i64) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000d1".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        let h1: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let h2: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550200', '+15555550200', 'phone', 'phone') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let a: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, source_file
            ) VALUES ($1, $2, 'individual', NULL, 't.json') RETURNING id
            "#,
        )
        .bind(&account)
        .bind(h1)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let b: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, source_file
            ) VALUES ($1, $2, 'individual', NULL, 't.json') RETURNING id
            "#,
        )
        .bind(&account)
        .bind(h2)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        (pool, dir, account, a, b)
    }

    #[tokio::test]
    async fn create_list_rename_delete_tag() {
        let (pool, _dir, account, _, _) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            create_name(tag_spec(), &mut conn, &account, " Holiday ")
                .await
                .unwrap(),
            "Holiday"
        );
        assert_eq!(
            list_names(tag_spec(), &mut conn, &account).await.unwrap(),
            vec!["Holiday"]
        );

        let err = create_name(tag_spec(), &mut conn, &account, "holiday")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::Conflict(_)));

        let err = create_name(tag_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));

        assert_eq!(
            rename_name(tag_spec(), &mut conn, &account, "holiday", "Trip")
                .await
                .unwrap(),
            "Trip"
        );
        assert_eq!(
            list_names(tag_spec(), &mut conn, &account).await.unwrap(),
            vec!["Trip"]
        );

        delete_name(tag_spec(), &mut conn, &account, "trip")
            .await
            .unwrap();
        assert!(
            list_names(tag_spec(), &mut conn, &account)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn membership_add_and_remove() {
        let (pool, _dir, account, a, b) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            set_membership(tag_spec(), &mut conn, &account, &[a, b], "Holiday", true)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            list_member_ids(tag_spec(), &mut conn, &account, "holiday")
                .await
                .unwrap(),
            vec![a, b]
        );
        assert_eq!(
            named_membership::names_for_item(tag_spec(), &mut conn, &account, a)
                .await
                .unwrap(),
            vec!["Holiday"]
        );
        assert_eq!(
            set_membership(tag_spec(), &mut conn, &account, &[a], "Holiday", false)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            list_member_ids(tag_spec(), &mut conn, &account, "Holiday")
                .await
                .unwrap(),
            vec![b]
        );
    }
}
