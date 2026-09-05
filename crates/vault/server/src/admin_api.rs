//! Administrator user management. Every route requires an administrator
//! session. Responses carry account metadata, counts, and storage sizes —
//! never message content. Multitenancy stays inviolable here: an
//! administrator manages accounts, not the contents of other people's
//! vaults, so nothing in this module reads `messages.body`,
//! `attachments.transcription`, or any other content column.

use crate::extract::{Json, Path};
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, Connection};

use crate::db::account_profile;
use crate::server::{Admin, ApiError, AppState};

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
    let storage_bytes =
        crate::db::vault_imports::account_attachment_bytes(conn, account_id).await?;
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

/// Return `404 Not Found` unless an account with this id exists.
async fn require_account_exists(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<(), ApiError> {
    if account_profile::username_for_account(conn, account_id)
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound(format!(
            "account {account_id} not found"
        )));
    }
    Ok(())
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
    Admin(_auth): Admin,
) -> Result<Json<ListUsersResponse>, ApiError> {
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
    Admin(_auth): Admin,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let username = crate::auth::normalize_username(&req.username);
    if !crate::auth::is_valid_username(&username) {
        return Err(ApiError::BadRequest(
            "username must be 1–128 chars (alphanumeric, _, -, .)".into(),
        ));
    }
    crate::auth::validate_password_policy(&req.password)?;
    let password_hash = crate::auth::hash_password(&req.password)?;

    let mut conn = state.db.acquire().await?;
    // insert_account and the optional set_admin must land together: a failure
    // between them must not leave an account that exists without the admin
    // flag the caller asked for (mirrors auth::register_handler).
    let mut tx = conn.begin().await?;
    if account_profile::lookup_account_ref(&mut tx, &username)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "username already taken: {username}"
        )));
    }

    let account_id = uuid::Uuid::new_v4().to_string();
    account_profile::insert_account(&mut tx, &account_id, &username, Some(&password_hash), None)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if req.is_admin {
        account_profile::set_admin(&mut tx, &account_id, true).await?;
    }
    tx.commit().await?;

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
    Admin(_auth): Admin,
    Json(req): Json<PatchUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let mut conn = state.db.acquire().await?;
    require_account_exists(&mut conn, &target).await?;

    if (req.is_admin == Some(false) || req.disabled == Some(true))
        && account_profile::is_last_admin(&mut conn, &target).await?
    {
        return Err(ApiError::BadRequest(
            "this is the only administrator; promote another account first".into(),
        ));
    }

    if let Some(is_admin) = req.is_admin {
        account_profile::set_admin(&mut conn, &target, is_admin).await?;
    }
    // Column names come from this compile-time array, never from the
    // request, so formatting them into the SQL is safe; values stay bound.
    let flags = [
        ("disabled", req.disabled),
        ("can_import", req.can_import),
        ("can_export", req.can_export),
        ("can_delete", req.can_delete),
    ];
    for (column, value) in flags {
        let Some(value) = value else { continue };
        sqlx::query(&format!("UPDATE accounts SET {column} = $1 WHERE id = $2"))
            .bind(value as i32)
            .bind(&target)
            .execute(&mut *conn)
            .await?;
    }

    let user = load_admin_user(&mut conn, &target)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("account {target} not found")))?;
    Ok(Json(user))
}

/// Set an account's password as an administrator. Invalidates that
/// account's existing session (unlike a self-service password change, which
/// leaves other sessions alone) — after this call the target must sign in
/// again with the new password.
#[utoipa::path(
    put,
    path = "/v1/admin/users/{id}/password",
    tag = "Admin",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "Account id whose password is set")),
    request_body = SetPasswordRequest,
    responses(
        (status = 204, description = "Password set"),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn set_user_password_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    Admin(_auth): Admin,
    Json(req): Json<SetPasswordRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    crate::auth::validate_password_policy(&req.password)?;
    let hash = crate::auth::hash_password(&req.password)?;

    let mut conn = state.db.acquire().await?;
    require_account_exists(&mut conn, &target).await?;
    account_profile::update_password_hash(&mut conn, &target, &hash).await?;
    crate::db::session_tokens::revoke_account_sessions(&mut conn, &target).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
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
    Admin(_auth): Admin,
) -> Result<Json<crate::profile::DeleteMessagesResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    require_account_exists(&mut conn, &target).await?;

    let stats = account_profile::delete_all_messages_for_account(&mut conn, &target).await?;
    crate::profile::remove_account_asset_trees(
        &state.cfg.paths.data_dir,
        &target,
        &state.cfg.paths.assets_dir,
        &state.cfg.paths.assets_converted_dir,
    )?;

    Ok(Json(crate::profile::DeleteMessagesResponse {
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
        (status = 204, description = "Account deleted"),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn delete_user_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    Admin(_auth): Admin,
) -> Result<axum::http::StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    require_account_exists(&mut conn, &target).await?;
    if account_profile::is_last_admin(&mut conn, &target).await? {
        return Err(ApiError::BadRequest(
            "this is the only administrator; promote another account first".into(),
        ));
    }

    account_profile::delete_account(&mut conn, &target).await?;
    let account_root = state.cfg.paths.data_dir.join(&target);
    if account_root.exists() {
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&account_root))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
