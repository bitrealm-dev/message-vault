//! Thread tags stored in `conversation_tags` / `conversation_tag_members`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result as AnyResult;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
#[cfg(test)]
use rusqlite::params;
use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::db::sql::{fold_in_id_chunks, in_placeholders};
use crate::named_membership::{self, MembershipError, tag_spec};
use crate::server::{
    ApiError, AppState, JoinBlocking, MembershipChangedResponse, lock_conn, require_full_access,
    resolve_auth,
};

/// Create / rename / delete / membership failures.
pub type TagError = MembershipError;

/// Longest allowed tag name (characters).
pub use crate::named_membership::MAX_NAME_LEN as MAX_TAG_NAME_LEN;

/// True when `name` is reserved and must not be created.
pub fn is_reserved_tag_name(name: &str) -> bool {
    named_membership::is_reserved(tag_spec(), name)
}

/// Tag names for this account, A–Z, excluding reserved leftovers.
pub fn list_tags(conn: &Connection, account_id: &str) -> Result<Vec<String>, TagError> {
    named_membership::list_names(tag_spec(), conn, account_id)
}

/// Create a tag. Fails when the name is taken (ignoring case).
pub fn create_tag(conn: &Connection, account_id: &str, name: &str) -> Result<String, TagError> {
    named_membership::create_name(tag_spec(), conn, account_id, name)
}

/// Rename a tag. Allows a case-only change of the same name.
pub fn rename_tag(
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, TagError> {
    named_membership::rename_name(tag_spec(), conn, account_id, from, to)
}

/// Delete a tag and its memberships.
pub fn delete_tag(conn: &Connection, account_id: &str, name: &str) -> Result<(), TagError> {
    named_membership::delete_name(tag_spec(), conn, account_id, name)
}

/// Conversation ids that currently have a named tag (case-insensitive).
pub fn list_tag_member_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, TagError> {
    named_membership::list_member_ids(tag_spec(), conn, account_id, name)
}

/// Add or remove one tag for many conversations. Creates the tag when enabling.
pub fn set_conversations_tag_membership(
    conn: &Connection,
    account_id: &str,
    conversation_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, TagError> {
    named_membership::set_membership(tag_spec(), conn, account_id, conversation_ids, name, enable)
}

/// Tags on one conversation, A–Z.
#[cfg(test)]
pub(crate) fn tags_for_conversation(
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

fn map_tag_error(err: TagError) -> ApiError {
    match err {
        TagError::BadRequest(m) => ApiError::BadRequest(m),
        TagError::NotFound(m) => ApiError::NotFound(m),
        TagError::Conflict(m) => ApiError::Conflict(m),
        TagError::Internal(m) => ApiError::Internal(m),
    }
}

async fn with_tag_conn<T, F>(
    db: Arc<StdMutex<Connection>>,
    task: &'static str,
    f: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> Result<T, TagError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || -> Result<T, ApiError> {
        let conn = lock_conn(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
        f(&conn).map_err(map_tag_error)
    })
    .await
    .join_map(task, |e| e)
}

/// A tag name.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagNameBody {
    name: String,
}

/// Old and new tag names.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagRenameBody {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ThreadTagMembersQuery {
    name: String,
}

/// Conversation ids, tag name, and enable flag.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagMembershipBody {
    ids: Vec<i64>,
    name: String,
    enable: bool,
}

/// The account's tag names.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagsListResponse {
    tags: Vec<String>,
}

/// The affected tag plus the updated list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagNamedListResponse {
    name: String,
    tags: Vec<String>,
}

/// The updated list after deletion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagDeleteResponse {
    ok: bool,
    tags: Vec<String>,
}

/// Conversation ids carrying the named tag.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ThreadTagMembersResponse {
    name: String,
    #[serde(rename = "memberConversationIds")]
    member_conversation_ids: Vec<i64>,
}

/// List the account's thread tags (A–Z, reserved names hidden).
#[utoipa::path(
    get,
    path = "/v1/thread-tags",
    tag = "Thread tags",
    security(("bearer" = [])),
    responses(
        (status = 200, body = ThreadTagsListResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn thread_tags_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ThreadTagsListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let tags = with_tag_conn(db, "thread tags list", move |conn| {
        list_tags(conn, &auth.account_id)
    })
    .await?;
    Ok(Json(ThreadTagsListResponse { tags }))
}

/// Create a thread tag and return the updated list.
#[utoipa::path(
    post,
    path = "/v1/thread-tags",
    tag = "Thread tags",
    security(("bearer" = [])),
    request_body = ThreadTagNameBody,
    responses(
        (status = 200, body = ThreadTagNamedListResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn thread_tags_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ThreadTagNameBody>,
) -> Result<Json<ThreadTagNamedListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let name = body.name;
    let (created, tags) = with_tag_conn(db, "thread tags create", move |conn| {
        let created = create_tag(conn, &auth.account_id, &name)?;
        let tags = list_tags(conn, &auth.account_id)?;
        Ok((created, tags))
    })
    .await?;
    Ok(Json(ThreadTagNamedListResponse {
        name: created,
        tags,
    }))
}

/// Rename a thread tag and return the updated list.
#[utoipa::path(
    patch,
    path = "/v1/thread-tags",
    tag = "Thread tags",
    security(("bearer" = [])),
    request_body = ThreadTagRenameBody,
    responses(
        (status = 200, body = ThreadTagNamedListResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn thread_tags_rename_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ThreadTagRenameBody>,
) -> Result<Json<ThreadTagNamedListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let (name, tags) = with_tag_conn(db, "thread tags rename", move |conn| {
        let name = rename_tag(conn, &auth.account_id, &body.from, &body.to)?;
        let tags = list_tags(conn, &auth.account_id)?;
        Ok((name, tags))
    })
    .await?;
    Ok(Json(ThreadTagNamedListResponse { name, tags }))
}

/// Delete a thread tag and return the updated list.
#[utoipa::path(
    delete,
    path = "/v1/thread-tags",
    tag = "Thread tags",
    security(("bearer" = [])),
    request_body = ThreadTagNameBody,
    responses(
        (status = 200, body = ThreadTagDeleteResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn thread_tags_delete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ThreadTagNameBody>,
) -> Result<Json<ThreadTagDeleteResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let tags = with_tag_conn(db, "thread tags delete", move |conn| {
        delete_tag(conn, &auth.account_id, &body.name)?;
        list_tags(conn, &auth.account_id)
    })
    .await?;
    Ok(Json(ThreadTagDeleteResponse { ok: true, tags }))
}

/// Conversation ids that carry a named tag.
#[utoipa::path(
    get,
    path = "/v1/thread-tags/members",
    tag = "Thread tags",
    security(("bearer" = [])),
    params(("name" = String, Query, description = "Tag name")),
    responses(
        (status = 200, body = ThreadTagMembersResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn thread_tags_members_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ThreadTagMembersQuery>,
) -> Result<Json<ThreadTagMembersResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let name = query.name.clone();
    let member_conversation_ids = with_tag_conn(db, "thread tags members", move |conn| {
        list_tag_member_ids(conn, &auth.account_id, &name)
    })
    .await?;
    Ok(Json(ThreadTagMembersResponse {
        name: query.name,
        member_conversation_ids,
    }))
}

/// Add or remove a tag on conversations.
#[utoipa::path(
    post,
    path = "/v1/conversations/tags",
    tag = "Thread tags",
    security(("bearer" = [])),
    request_body = ThreadTagMembershipBody,
    responses(
        (status = 200, body = MembershipChangedResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn thread_tags_membership_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ThreadTagMembershipBody>,
) -> Result<Json<MembershipChangedResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let changed = with_tag_conn(db, "thread tags membership", move |conn| {
        set_conversations_tag_membership(conn, &auth.account_id, &body.ids, &body.name, body.enable)
    })
    .await?;
    Ok(Json(MembershipChangedResponse { changed }))
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
