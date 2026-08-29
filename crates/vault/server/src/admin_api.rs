//! Administrator user management. Every route requires an administrator
//! session. Responses carry account metadata, counts, and storage sizes —
//! never message content. Multitenancy stays inviolable here: an
//! administrator manages accounts, not the contents of other people's
//! vaults, so nothing in this module reads `messages.body`,
//! `attachments.transcription`, or any other content column.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;

use crate::db::account_profile;
use crate::server::{ApiError, AppState, require_admin, resolve_auth};

/// One account as an administrator sees it.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AdminUser {
    /// Account id.
    pub account_id: String,
    /// Login username.
    pub username: String,
    /// May manage users.
    pub is_admin: bool,
    /// May not sign in.
    pub disabled: bool,
    /// May call the import endpoints.
    pub can_import: bool,
    /// May call the export endpoints.
    pub can_export: bool,
    /// May destroy message data.
    pub can_delete: bool,
    /// Messages this account owns.
    pub message_count: i64,
    /// Attachment bytes this account owns.
    pub storage_bytes: i64,
}

/// Every account in the vault.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListUsersResponse {
    /// One row per account.
    pub items: Vec<AdminUser>,
}

/// Body for creating an account as an administrator.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateUserRequest {
    /// Login username.
    pub username: String,
    /// Initial password. Must satisfy the vault's password policy.
    pub password: String,
    /// Grant the administrative flag. Default false.
    #[serde(default)]
    pub is_admin: bool,
}

/// Body for changing an account's flags. Omitted fields are left alone.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PatchUserRequest {
    /// Grant or revoke administration.
    #[serde(default)]
    pub is_admin: Option<bool>,
    /// Disable or re-enable sign-in.
    #[serde(default)]
    pub disabled: Option<bool>,
    /// Allow or forbid import.
    #[serde(default)]
    pub can_import: Option<bool>,
    /// Allow or forbid export.
    #[serde(default)]
    pub can_export: Option<bool>,
    /// Allow or forbid deleting message data.
    #[serde(default)]
    pub can_delete: Option<bool>,
}

/// Body for an administrator setting someone's password.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetPasswordRequest {
    /// The new password. Must satisfy the vault's password policy.
    pub password: String,
}

/// Number of messages an account owns. Never touches message content.
async fn account_message_count(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<i64, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// Load one account's admin-facing row: flags, message count, storage bytes.
/// `None` when the account no longer exists.
async fn load_admin_user(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Option<AdminUser>, ApiError> {
    let row: Option<(String, i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT username, is_admin, disabled, can_import, can_export, can_delete
         FROM accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((username, is_admin, disabled, import, export, delete)) = row else {
        return Ok(None);
    };
    let message_count = account_message_count(conn, account_id).await?;
    let storage_bytes = crate::db::vault_imports::account_attachment_bytes(conn, account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Some(AdminUser {
        account_id: account_id.to_string(),
        username,
        is_admin: is_admin != 0,
        disabled: disabled != 0,
        can_import: import != 0,
        can_export: export != 0,
        can_delete: delete != 0,
        message_count,
        storage_bytes,
    }))
}

/// List every account with its flags, message count, and storage use.
#[utoipa::path(
    get,
    path = "/v1/admin/users",
    tag = "Admin",
    security(("bearer" = [])),
    responses(
        (status = 200, body = ListUsersResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn list_users_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ListUsersResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let mut conn = state.db.acquire().await?;
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM accounts ORDER BY username")
        .fetch_all(&mut *conn)
        .await?;

    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(user) = load_admin_user(&mut conn, &id).await? {
            items.push(user);
        }
    }
    Ok(Json(ListUsersResponse { items }))
}

/// Create an account as an administrator. Never grants the first-account
/// administrator flag automatically — this endpoint could not have been
/// called unless an administrator already exists.
#[utoipa::path(
    post,
    path = "/v1/admin/users",
    tag = "Admin",
    security(("bearer" = [])),
    request_body = CreateUserRequest,
    responses(
        (status = 200, body = AdminUser),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn create_user_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let username = crate::auth::normalize_username(&req.username);
    if !crate::auth::is_valid_username(&username) {
        return Err(ApiError::BadRequest(
            "username must be 1–128 chars (alphanumeric, _, -, .)".into(),
        ));
    }
    crate::auth::validate_password_policy(&req.password)?;
    let password_hash =
        crate::auth::hash_password(&req.password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut conn = state.db.acquire().await?;
    if account_profile::lookup_account_ref(&mut conn, &username)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "username already taken: {username}"
        )));
    }

    let account_id = uuid::Uuid::new_v4().to_string();
    account_profile::insert_account(
        &mut conn,
        &account_id,
        &username,
        Some(&password_hash),
        None,
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if req.is_admin {
        account_profile::set_admin(&mut conn, &account_id, true)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    let user = load_admin_user(&mut conn, &account_id)
        .await?
        .ok_or_else(|| ApiError::Internal("account vanished immediately after insert".into()))?;
    Ok(Json(user))
}

/// Change an account's administrative, disabled, or permission flags.
///
/// Refuses (`400`) a request that would demote, disable, or otherwise leave
/// the vault without an administrator.
#[utoipa::path(
    patch,
    path = "/v1/admin/users/{id}",
    tag = "Admin",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Account id to modify")),
    request_body = PatchUserRequest,
    responses(
        (status = 200, body = AdminUser),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn patch_user_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
    Json(req): Json<PatchUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let mut conn = state.db.acquire().await?;
    if account_profile::username_for_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::NotFound(format!("account {target} not found")));
    }

    if (req.is_admin == Some(false) || req.disabled == Some(true))
        && account_profile::is_last_admin(&mut conn, &target)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::BadRequest(
            "this is the only administrator; promote another account first".into(),
        ));
    }

    if let Some(is_admin) = req.is_admin {
        account_profile::set_admin(&mut conn, &target, is_admin)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(disabled) = req.disabled {
        sqlx::query("UPDATE accounts SET disabled = $1 WHERE id = $2")
            .bind(disabled as i32)
            .bind(&target)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(can_import) = req.can_import {
        sqlx::query("UPDATE accounts SET can_import = $1 WHERE id = $2")
            .bind(can_import as i32)
            .bind(&target)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(can_export) = req.can_export {
        sqlx::query("UPDATE accounts SET can_export = $1 WHERE id = $2")
            .bind(can_export as i32)
            .bind(&target)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(can_delete) = req.can_delete {
        sqlx::query("UPDATE accounts SET can_delete = $1 WHERE id = $2")
            .bind(can_delete as i32)
            .bind(&target)
            .execute(&mut *conn)
            .await?;
    }

    let user = load_admin_user(&mut conn, &target)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("account {target} not found")))?;
    Ok(Json(user))
}

/// Set an account's password as an administrator. Does not invalidate that
/// account's existing session.
#[utoipa::path(
    post,
    path = "/v1/admin/users/{id}/password",
    tag = "Admin",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Account id whose password is set")),
    request_body = SetPasswordRequest,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn set_user_password_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SetPasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;
    crate::auth::validate_password_policy(&req.password)?;
    let hash =
        crate::auth::hash_password(&req.password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut conn = state.db.acquire().await?;
    if account_profile::username_for_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::NotFound(format!("account {target} not found")));
    }
    account_profile::update_password_hash(&mut conn, &target, &hash)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Destroy one account's conversations, messages, and attachments. The
/// account itself, its contacts, and its login survive. Refuses (`400`)
/// nothing here — deleting messages never affects the last-admin rule.
#[utoipa::path(
    delete,
    path = "/v1/admin/users/{id}/messages",
    tag = "Admin",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Account whose messages are destroyed")),
    responses(
        (status = 200, body = crate::profile::DeleteMessagesResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn delete_user_messages_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Result<Json<crate::profile::DeleteMessagesResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let mut conn = state.db.acquire().await?;
    if account_profile::username_for_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::NotFound(format!("account {target} not found")));
    }

    let stats = account_profile::delete_all_messages_for_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    crate::profile::remove_account_asset_trees(
        &state.cfg.paths.data_dir,
        &target,
        &state.cfg.paths.assets_dir,
        &state.cfg.paths.assets_converted_dir,
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(crate::profile::DeleteMessagesResponse {
        ok: true,
        conversations: stats.conversations,
        attachments: stats.attachments,
    }))
}

/// Permanently delete an account: login, profile, contacts, and every
/// message it owns. Refuses (`400`) deleting the vault's last administrator.
#[utoipa::path(
    delete,
    path = "/v1/admin/users/{id}",
    tag = "Admin",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Account id to delete")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn delete_user_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let mut conn = state.db.acquire().await?;
    if account_profile::username_for_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::NotFound(format!("account {target} not found")));
    }
    if account_profile::is_last_admin(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::BadRequest(
            "this is the only administrator; promote another account first".into(),
        ));
    }

    account_profile::delete_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let account_root = state.cfg.paths.data_dir.join(&target);
    if account_root.exists() {
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&account_root))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    use crate::test_support::{
        delete_status, get_json, get_status, login_status, patch_status, post_json, post_status,
        register_via_api, seed_one_message, test_vault,
    };

    #[tokio::test]
    async fn non_admins_are_refused() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let _admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let ordinary = register_via_api(&state, "bob", "hunter2hunter2").await;

        let status = get_status(&state, "/v1/admin/users", &ordinary.token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn api_tokens_are_refused_even_for_the_admins_own_token() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

        // No token-creation helper in this module's test surface; assert the
        // guard directly instead of round-tripping through the token API.
        let mut conn = state.db.acquire().await.unwrap();
        let auth = crate::server::resolve_auth_on_conn(&mut conn, &admin.token)
            .await
            .unwrap();
        assert!(auth.is_admin());
        let token_auth = crate::server::AuthIdentity {
            account_id: auth.account_id.clone(),
            capability: crate::server::AuthCapability::ApiToken(auth.permissions()),
        };
        assert!(!token_auth.is_admin());
        assert!(require_admin(&token_auth).is_err());
    }

    #[tokio::test]
    async fn the_admin_sees_every_account_but_no_messages() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let _other = register_via_api(&state, "bob", "hunter2hunter2").await;

        let body: ListUsersResponse = get_json(&state, "/v1/admin/users", &admin.token).await;

        assert_eq!(body.items.len(), 2);
        let bob = body.items.iter().find(|u| u.username == "bob").unwrap();
        assert_eq!(bob.message_count, 0);
        assert!(!bob.is_admin);
        assert!(!bob.disabled);
    }

    #[tokio::test]
    async fn list_response_has_no_message_content_fields() {
        // Structural proof, not just behavioral: serialize a row and check the
        // JSON object's keys are exactly the metadata fields, nothing else.
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        seed_one_message(&state, &admin.account_id).await;

        let body: serde_json::Value = {
            let raw: ListUsersResponse = get_json(&state, "/v1/admin/users", &admin.token).await;
            serde_json::to_value(raw).unwrap()
        };
        let item = &body["items"][0];
        let mut keys: Vec<&str> = item
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "account_id",
                "can_delete",
                "can_export",
                "can_import",
                "disabled",
                "is_admin",
                "message_count",
                "storage_bytes",
                "username",
            ],
            "admin user rows must carry only metadata, never message content"
        );
    }

    #[tokio::test]
    async fn the_last_admin_cannot_be_demoted_disabled_or_deleted() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let path = format!("/v1/admin/users/{}", admin.account_id);

        let demoted = patch_status(
            &state,
            &path,
            &admin.token,
            serde_json::json!({ "is_admin": false }),
        )
        .await;
        assert_eq!(demoted, StatusCode::BAD_REQUEST);

        let disabled = patch_status(
            &state,
            &path,
            &admin.token,
            serde_json::json!({ "disabled": true }),
        )
        .await;
        assert_eq!(disabled, StatusCode::BAD_REQUEST);

        let deleted = delete_status(&state, &path, &admin.token).await;
        assert_eq!(deleted, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn once_a_second_admin_exists_the_first_can_be_demoted() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

        let promoted = patch_status(
            &state,
            &format!("/v1/admin/users/{}", bob.account_id),
            &admin.token,
            serde_json::json!({ "is_admin": true }),
        )
        .await;
        assert_eq!(promoted, StatusCode::OK);

        let demoted = patch_status(
            &state,
            &format!("/v1/admin/users/{}", admin.account_id),
            &admin.token,
            serde_json::json!({ "is_admin": false }),
        )
        .await;
        assert_eq!(demoted, StatusCode::OK);
    }

    #[tokio::test]
    async fn create_user_never_grants_admin_by_default() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

        let created: AdminUser = post_json(
            &state,
            "/v1/admin/users",
            &admin.token,
            serde_json::json!({ "username": "carol", "password": "hunter2hunter2" }),
        )
        .await;
        assert!(!created.is_admin);
        assert_eq!(created.username, "carol");
        assert_eq!(created.message_count, 0);

        // The created account can sign in with the password it was given.
        let login = login_status(&state, "carol", "hunter2hunter2").await;
        assert_eq!(login, StatusCode::OK);
    }

    #[tokio::test]
    async fn create_user_can_grant_admin_explicitly() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

        let created: AdminUser = post_json(
            &state,
            "/v1/admin/users",
            &admin.token,
            serde_json::json!({ "username": "carol", "password": "hunter2hunter2", "is_admin": true }),
        )
        .await;
        assert!(created.is_admin);
    }

    #[tokio::test]
    async fn setting_a_password_lets_the_new_password_sign_in() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

        let status = post_status(
            &state,
            &format!("/v1/admin/users/{}/password", bob.account_id),
            &admin.token,
            serde_json::json!({ "password": "newpassword123" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let login = login_status(&state, "bob", "newpassword123").await;
        assert_eq!(login, StatusCode::OK);
    }

    #[tokio::test]
    async fn deleting_one_users_messages_leaves_the_others_alone() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let victim = register_via_api(&state, "bob", "hunter2hunter2").await;
        seed_one_message(&state, &victim.account_id).await;
        seed_one_message(&state, &admin.account_id).await;

        let status = delete_status(
            &state,
            &format!("/v1/admin/users/{}/messages", victim.account_id),
            &admin.token,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let body: ListUsersResponse = get_json(&state, "/v1/admin/users", &admin.token).await;
        let bob = body.items.iter().find(|u| u.username == "bob").unwrap();
        let alice = body.items.iter().find(|u| u.username == "alice").unwrap();
        assert_eq!(bob.message_count, 0);
        assert_eq!(alice.message_count, 1, "the other tenant is untouched");
    }
}
