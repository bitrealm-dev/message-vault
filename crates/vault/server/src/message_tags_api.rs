//! Message tags stored in `message_tags` / `message_tag_members`.

use std::collections::HashMap;

use anyhow::Result as AnyResult;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;

use crate::db::dialect::engine_of;
use crate::db::engine::DbEngine;
use crate::db::sql::{fold_in_id_chunks, in_placeholders};
use crate::named_membership::{self, MembershipError, tag_spec};
use crate::server::{
    ApiError, AppState, MembershipChangedResponse, require_full_access, resolve_auth,
};

/// Create / rename / delete / membership failures.
pub type TagError = MembershipError;

/// Tag names for this account, A–Z, excluding reserved leftovers.
pub async fn list_tags(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<String>, TagError> {
    named_membership::list_names(tag_spec(), conn, account_id).await
}

/// Create a tag. Fails when the name is taken (ignoring case).
pub async fn create_tag(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<String, TagError> {
    named_membership::create_name(tag_spec(), conn, account_id, name).await
}

/// Rename a tag. Allows a case-only change of the same name.
pub async fn rename_tag(
    conn: &mut AnyConnection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, TagError> {
    named_membership::rename_name(tag_spec(), conn, account_id, from, to).await
}

/// Delete a tag and its memberships.
pub async fn delete_tag(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<(), TagError> {
    named_membership::delete_name(tag_spec(), conn, account_id, name).await
}

/// Conversation ids that currently have a named tag (case-insensitive).
pub async fn list_tag_member_ids(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, TagError> {
    named_membership::list_member_ids(tag_spec(), conn, account_id, name).await
}

/// Add or remove one tag for many conversations. Creates the tag when enabling.
pub async fn set_conversations_tag_membership(
    conn: &mut AnyConnection,
    account_id: &str,
    conversation_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, TagError> {
    named_membership::set_membership(tag_spec(), conn, account_id, conversation_ids, name, enable)
        .await
}

/// Tags on one conversation, A–Z.
#[cfg(test)]
pub(crate) async fn tags_for_conversation(
    conn: &mut AnyConnection,
    account_id: &str,
    conversation_id: i64,
) -> AnyResult<Vec<String>> {
    let order = match engine_of(conn) {
        DbEngine::Sqlite => "ORDER BY ct.name COLLATE NOCASE",
        DbEngine::Postgres => "ORDER BY lower(ct.name)",
    };
    let sql = format!(
        "SELECT ct.name
         FROM message_tags ct
         JOIN message_tag_members m ON m.tag_id = ct.id
         WHERE ct.account_id = $1 AND m.conversation_id = $2
         {order}"
    );
    let rows = sqlx::query_scalar::<_, String>(&sql)
        .bind(account_id)
        .bind(conversation_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Tags on each conversation id, A–Z within each list.
pub async fn tags_for_conversations(
    conn: &mut AnyConnection,
    account_id: &str,
    conversation_ids: &[i64],
) -> AnyResult<HashMap<i64, Vec<String>>> {
    let account_id = account_id.to_string();
    fold_in_id_chunks(conn, conversation_ids, |conn, chunk| {
        let account_id = account_id.clone();
        Box::pin(async move {
            let placeholders = in_placeholders(2, chunk.len());
            let order = match engine_of(conn) {
                DbEngine::Sqlite => "ORDER BY ct.name COLLATE NOCASE",
                DbEngine::Postgres => "ORDER BY lower(ct.name)",
            };
            let sql = format!(
                "SELECT m.conversation_id, ct.name
                 FROM message_tag_members m
                 JOIN message_tags ct ON ct.id = m.tag_id
                 WHERE ct.account_id = $1 AND m.conversation_id IN ({placeholders})
                 {order}"
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

fn map_tag_error(err: TagError) -> ApiError {
    match err {
        TagError::BadRequest(m) => ApiError::BadRequest(m),
        TagError::NotFound(m) => ApiError::NotFound(m),
        TagError::Conflict(m) => ApiError::Conflict(m),
        TagError::Internal(m) => ApiError::Internal(m),
    }
}

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
    let tags = list_tags(&mut conn, &auth.account_id)
        .await
        .map_err(map_tag_error)?;
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
    let created = create_tag(&mut conn, &auth.account_id, &name)
        .await
        .map_err(map_tag_error)?;
    let tags = list_tags(&mut conn, &auth.account_id)
        .await
        .map_err(map_tag_error)?;
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
    let name = rename_tag(&mut conn, &auth.account_id, &body.from, &body.to)
        .await
        .map_err(map_tag_error)?;
    let tags = list_tags(&mut conn, &auth.account_id)
        .await
        .map_err(map_tag_error)?;
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
    delete_tag(&mut conn, &auth.account_id, &body.name)
        .await
        .map_err(map_tag_error)?;
    let tags = list_tags(&mut conn, &auth.account_id)
        .await
        .map_err(map_tag_error)?;
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
    let member_conversation_ids = list_tag_member_ids(&mut conn, &auth.account_id, &query.name)
        .await
        .map_err(map_tag_error)?;
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
    let changed = set_conversations_tag_membership(
        &mut conn,
        &auth.account_id,
        &body.ids,
        &body.name,
        body.enable,
    )
    .await
    .map_err(map_tag_error)?;
    Ok(Json(MembershipChangedResponse { changed }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::engine;
    use crate::db::schema;

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
            create_tag(&mut conn, &account, " Holiday ").await.unwrap(),
            "Holiday"
        );
        assert_eq!(
            list_tags(&mut conn, &account).await.unwrap(),
            vec!["Holiday"]
        );

        let err = create_tag(&mut conn, &account, "holiday")
            .await
            .unwrap_err();
        assert!(matches!(err, TagError::Conflict(_)));

        let err = create_tag(&mut conn, &account, "Trash").await.unwrap_err();
        assert!(matches!(err, TagError::BadRequest(_)));

        assert_eq!(
            rename_tag(&mut conn, &account, "holiday", "Trip")
                .await
                .unwrap(),
            "Trip"
        );
        assert_eq!(list_tags(&mut conn, &account).await.unwrap(), vec!["Trip"]);

        delete_tag(&mut conn, &account, "trip").await.unwrap();
        assert!(list_tags(&mut conn, &account).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn membership_add_and_remove() {
        let (pool, _dir, account, a, b) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            set_conversations_tag_membership(&mut conn, &account, &[a, b], "Holiday", true)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            list_tag_member_ids(&mut conn, &account, "holiday")
                .await
                .unwrap(),
            vec![a, b]
        );
        assert_eq!(
            tags_for_conversation(&mut conn, &account, a).await.unwrap(),
            vec!["Holiday"]
        );
        assert_eq!(
            set_conversations_tag_membership(&mut conn, &account, &[a], "Holiday", false)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            list_tag_member_ids(&mut conn, &account, "Holiday")
                .await
                .unwrap(),
            vec![b]
        );
    }
}
