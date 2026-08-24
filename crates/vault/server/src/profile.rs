//! Account profile read + update handlers.

use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use message_ir::HandleType;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use sqlx::Connection;

use crate::db::account_profile;
use crate::server::{ApiError, AppState, require_full_access, resolve_auth};

/// The signed-in account's profile.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AccountProfileResponse {
    /// The signed-in account id.
    pub account_id: String,
    /// Account username (falls back to the account id).
    pub username: String,
    /// Display name, when set.
    pub preferred_name: Option<String>,
    /// Phone handles linked to the account.
    pub phones: Vec<String>,
    /// Email addresses linked to the account.
    pub emails: Vec<String>,
    /// True for the seeded demo account (cannot be deleted).
    pub is_demo: bool,
    /// True when `accounts.guest_status` is set (ready or assigned sample copy).
    pub is_guest: bool,
    /// True when the account is marked read-only.
    pub read_only: bool,
}

/// Load the profile JSON for `account_id`.
async fn load_response(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<AccountProfileResponse> {
    let username = account_profile::username_for_account(conn, account_id)
        .await?
        .unwrap_or_else(|| account_id.to_string());
    let preferred_name = account_profile::load_preferred_name(conn, account_id).await?;
    let profile = account_profile::load_account_profile(conn, account_id).await?;
    let read_only = account_profile::account_is_read_only(conn, account_id).await?;
    Ok(AccountProfileResponse {
        account_id: account_id.to_string(),
        username,
        preferred_name,
        phones: profile.phones,
        emails: profile.emails,
        is_demo: account_profile::is_demo_account(account_id),
        is_guest: account_profile::is_guest_account(conn, account_id).await?,
        read_only,
    })
}

/// Load the signed-in account's profile: username, display name, linked
/// handles, and demo/guest flags.
#[utoipa::path(
    get,
    path = "/v1/account/profile",
    tag = "Account",
    security(("bearer" = [])),
    responses(
        (status = 200, body = AccountProfileResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn account_profile_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountProfileResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let result = load_response(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(result))
}

/// One handle to link or unlink, with its platform service.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProfileHandleInput {
    /// Raw handle value, e.g. `+15555550100` or `alex@example.com`.
    pub handle: String,
    /// Platform the handle belongs to: `phone`, `email`, or `whatsapp`.
    pub service: String,
}

/// Display name and handle changes.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AccountProfileUpdateRequest {
    /// Display name to set; `None` (or empty) leaves the current name unchanged.
    #[serde(default)]
    pub preferred_name: Option<String>,
    /// Handles to add/link onto the account profile.
    #[serde(default)]
    pub handles: Vec<ProfileHandleInput>,
    /// Handles to unlink from the account profile.
    #[serde(default)]
    pub remove_handles: Vec<ProfileHandleInput>,
}

/// Apply name and handle changes on an open connection.
async fn apply_profile_update(
    conn: &mut AnyConnection,
    account_id: &str,
    preferred_name: Option<&str>,
    handles: &[ProfileHandleInput],
    remove_handles: &[ProfileHandleInput],
) -> Result<()> {
    if let Some(name) = preferred_name {
        let name = name.trim();
        let stored_name = if name.is_empty() {
            None::<&str>
        } else {
            Some(name)
        };
        sqlx::query("UPDATE accounts SET preferred_name = $1 WHERE id = $2")
            .bind(stored_name)
            .bind(account_id)
            .execute(&mut *conn)
            .await?;
    }

    for entry in remove_handles {
        let raw = entry.handle.trim();
        if raw.is_empty() {
            continue;
        }
        match parse_profile_service(&entry.service)? {
            ProfileHandleKind::Phone | ProfileHandleKind::Whatsapp => {
                account_profile::unlink_account_handle(conn, account_id, raw, HandleType::Phone)
                    .await?;
            }
            ProfileHandleKind::Email => {
                account_profile::unlink_account_handle(conn, account_id, raw, HandleType::Email)
                    .await?;
            }
        }
    }

    for entry in handles {
        let raw = entry.handle.trim();
        if raw.is_empty() {
            continue;
        }
        match parse_profile_service(&entry.service)? {
            ProfileHandleKind::Phone => {
                account_profile::link_account_handle(conn, account_id, raw, HandleType::Phone)
                    .await?;
            }
            ProfileHandleKind::Email => {
                account_profile::link_account_handle(conn, account_id, raw, HandleType::Email)
                    .await?;
                account_profile::upsert_account_email(
                    conn,
                    account_id,
                    &raw.to_ascii_lowercase(),
                    false,
                )
                .await?;
            }
            ProfileHandleKind::Whatsapp => {
                account_profile::link_account_handle_with_service(
                    conn,
                    account_id,
                    raw,
                    HandleType::Phone,
                    Some("whatsapp"),
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Apply a profile update in one transaction, then reload the response.
async fn update_profile_on_conn(
    conn: &mut AnyConnection,
    account_id: &str,
    req: &AccountProfileUpdateRequest,
) -> Result<AccountProfileResponse> {
    let mut tx = conn.begin().await?;
    apply_profile_update(
        &mut tx,
        account_id,
        req.preferred_name.as_deref(),
        &req.handles,
        &req.remove_handles,
    )
    .await?;
    tx.commit().await?;
    load_response(conn, account_id).await
}

enum ProfileHandleKind {
    Phone,
    Email,
    Whatsapp,
}

/// Map a client `service` string to a handle kind.
fn parse_profile_service(service: &str) -> Result<ProfileHandleKind> {
    match service.trim().to_ascii_lowercase().as_str() {
        "phone" => Ok(ProfileHandleKind::Phone),
        "email" => Ok(ProfileHandleKind::Email),
        "whatsapp" => Ok(ProfileHandleKind::Whatsapp),
        other => bail!("unsupported handle service: {other}"),
    }
}

/// Update the account's display name and linked handles, then return the
/// reloaded profile.
#[utoipa::path(
    post,
    path = "/v1/account/profile",
    tag = "Account",
    security(("bearer" = [])),
    request_body = AccountProfileUpdateRequest,
    responses(
        (status = 200, body = AccountProfileResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn account_profile_update_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AccountProfileUpdateRequest>,
) -> Result<Json<AccountProfileResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let result = update_profile_on_conn(&mut conn, &account_id, &req)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.starts_with("unsupported handle service:") {
                ApiError::BadRequest(msg)
            } else {
                ApiError::Internal(msg)
            }
        })?;

    Ok(Json(result))
}

/// Confirmation flag for deleting all messages.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeleteMessagesRequest {
    /// Must be `true`; anything else is rejected with a 400.
    pub confirm: bool,
}

/// Counts of deleted conversations and attachment rows.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteMessagesResponse {
    /// Always true when a response is returned.
    pub ok: bool,
    /// Conversations deleted.
    pub conversations: u64,
    /// Attachment rows deleted (on-disk files are removed too).
    pub attachments: u64,
}

/// Delete on-disk attachment trees for every source under this account.
fn remove_account_asset_trees(
    data_dir: &std::path::Path,
    account_id: &str,
    assets_name: &str,
    converted_name: &str,
) -> Result<()> {
    let account_root = data_dir.join(account_id);
    if !account_root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&account_root)
        .with_context(|| format!("read {}", account_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let source_root = entry.path();
        for name in [assets_name, converted_name] {
            let dir = source_root.join(name);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .with_context(|| format!("remove {}", dir.display()))?;
            }
        }
    }
    Ok(())
}

/// Delete every conversation, message, and attachment for the account.
/// Contacts and the account login survive.
#[utoipa::path(
    post,
    path = "/v1/account/delete-messages",
    tag = "Account",
    security(("bearer" = [])),
    request_body = DeleteMessagesRequest,
    responses(
        (status = 200, body = DeleteMessagesResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn delete_messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteMessagesRequest>,
) -> Result<Json<DeleteMessagesResponse>, ApiError> {
    if !req.confirm {
        return Err(ApiError::BadRequest(
            "confirmation flag must be true".into(),
        ));
    }
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let data_dir = state.cfg.paths.data_dir.clone();
    let assets_name = state.cfg.paths.assets_dir.clone();
    let converted_name = state.cfg.paths.assets_converted_dir.clone();

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let stats = account_profile::delete_all_messages_for_account(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    remove_account_asset_trees(&data_dir, &account_id, &assets_name, &converted_name)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(DeleteMessagesResponse {
        ok: true,
        conversations: stats.conversations,
        attachments: stats.attachments,
    }))
}

/// Attachment usage and the largest files.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AccountStorageResponse {
    pub total_bytes: i64,
    pub attachment_count: i64,
    pub top_attachments: Vec<crate::db::vault_imports::TopAttachment>,
}

/// Attachment storage usage for the account: total bytes, count, and the 100
/// largest files.
#[utoipa::path(
    get,
    path = "/v1/account/storage",
    tag = "Account",
    security(("bearer" = [])),
    responses(
        (status = 200, body = AccountStorageResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn account_storage_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountStorageResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let total_bytes = crate::db::vault_imports::account_attachment_bytes(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let attachment_count =
        crate::db::vault_imports::account_attachment_count(&mut conn, &account_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    let top_attachments =
        crate::db::vault_imports::top_attachments_by_size(&mut conn, &account_id, 100)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    let result = AccountStorageResponse {
        total_bytes,
        attachment_count,
        top_attachments,
    };

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::{engine, schema};

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account_id = "00000000-0000-4000-8000-000000000001".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username, read_only) VALUES ($1, $2, 0)")
            .bind(&account_id)
            .bind("alice")
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir, account_id)
    }

    #[tokio::test]
    async fn apply_profile_update_sets_name_and_handles() {
        let (pool, _dir, account_id) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        apply_profile_update(
            &mut conn,
            &account_id,
            Some("Alex"),
            &[
                ProfileHandleInput {
                    handle: "+1 (555) 555-0100".into(),
                    service: "phone".into(),
                },
                ProfileHandleInput {
                    handle: "Alex@Example.com".into(),
                    service: "email".into(),
                },
                ProfileHandleInput {
                    handle: "+15555550199".into(),
                    service: "whatsapp".into(),
                },
            ],
            &[],
        )
        .await
        .unwrap();

        let loaded = load_response(&mut conn, &account_id).await.unwrap();
        assert_eq!(loaded.preferred_name.as_deref(), Some("Alex"));
        assert!(loaded.phones.iter().any(|p| p == "+15555550100"));
        assert!(loaded.phones.iter().any(|p| p == "+15555550199"));
        assert!(loaded.emails.iter().any(|e| e == "alex@example.com"));

        let wa_service: String = sqlx::query_scalar(
            "SELECT service FROM handles WHERE account_id = $1 AND normalized = $2",
        )
        .bind(&account_id)
        .bind("+15555550199")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(wa_service, "whatsapp");
    }

    #[tokio::test]
    async fn apply_profile_update_removes_handles() {
        let (pool, _dir, account_id) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        apply_profile_update(
            &mut conn,
            &account_id,
            None,
            &[
                ProfileHandleInput {
                    handle: "+15555550100".into(),
                    service: "phone".into(),
                },
                ProfileHandleInput {
                    handle: "alex@example.com".into(),
                    service: "email".into(),
                },
            ],
            &[],
        )
        .await
        .unwrap();

        apply_profile_update(
            &mut conn,
            &account_id,
            None,
            &[],
            &[
                ProfileHandleInput {
                    handle: "+15555550100".into(),
                    service: "phone".into(),
                },
                ProfileHandleInput {
                    handle: "alex@example.com".into(),
                    service: "email".into(),
                },
            ],
        )
        .await
        .unwrap();

        let loaded = load_response(&mut conn, &account_id).await.unwrap();
        assert!(loaded.phones.is_empty());
        assert!(loaded.emails.is_empty());
    }

    #[tokio::test]
    async fn load_response_sets_is_guest_true_when_guest_status_assigned() {
        let (pool, _dir, account_id) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let guest_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        account_profile::insert_guest_account(&mut conn, guest_id, "guest-bbbb", None)
            .await
            .unwrap();
        account_profile::set_guest_status(&mut conn, guest_id, "assigned")
            .await
            .unwrap();

        let guest = load_response(&mut conn, guest_id).await.unwrap();
        assert!(guest.is_guest);

        let regular = load_response(&mut conn, &account_id).await.unwrap();
        assert!(!regular.is_guest);
    }

    #[tokio::test]
    async fn profile_update_rolls_back_when_a_handle_service_is_unsupported() {
        let (pool, _dir, account_id) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let result = update_profile_on_conn(
            &mut conn,
            &account_id,
            &AccountProfileUpdateRequest {
                preferred_name: Some("Changed Name".into()),
                handles: vec![ProfileHandleInput {
                    handle: "alice@example.com".into(),
                    service: "unsupported".into(),
                }],
                remove_handles: vec![],
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(
            account_profile::load_preferred_name(&mut conn, &account_id)
                .await
                .unwrap(),
            None
        );
    }
}
