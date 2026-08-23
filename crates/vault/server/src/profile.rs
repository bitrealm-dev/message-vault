//! Account profile read + update handlers.

use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use message_ir::HandleType;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{account_profile, schema};
use crate::server::{ApiError, AppState, JoinBlocking, require_full_access, resolve_auth};

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
fn load_response(conn: &Connection, account_id: &str) -> Result<AccountProfileResponse> {
    let username = account_profile::username_for_account(conn, account_id)?
        .unwrap_or_else(|| account_id.to_string());
    let preferred_name = account_profile::load_preferred_name(conn, account_id)?;
    let profile = account_profile::load_account_profile(conn, account_id)?;
    let read_only = account_profile::account_is_read_only(conn, account_id)?;
    Ok(AccountProfileResponse {
        account_id: account_id.to_string(),
        username,
        preferred_name,
        phones: profile.phones,
        emails: profile.emails,
        is_demo: account_profile::is_demo_account(account_id),
        is_guest: account_profile::is_guest_account(conn, account_id)?,
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

    let db = state.cfg.paths.db.clone();
    let result = crate::server::with_configured_db(&db, "profile load task", move |conn| {
        load_response(conn, &account_id)
    })
    .await?;

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
fn apply_profile_update(
    conn: &Connection,
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
        conn.execute(
            "UPDATE accounts SET preferred_name = ?1 WHERE id = ?2",
            rusqlite::params![stored_name, account_id],
        )?;
    }

    for entry in remove_handles {
        let raw = entry.handle.trim();
        if raw.is_empty() {
            continue;
        }
        match parse_profile_service(&entry.service)? {
            ProfileHandleKind::Phone | ProfileHandleKind::Whatsapp => {
                account_profile::unlink_account_handle(conn, account_id, raw, HandleType::Phone)?;
            }
            ProfileHandleKind::Email => {
                account_profile::unlink_account_handle(conn, account_id, raw, HandleType::Email)?;
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
                account_profile::link_account_handle(conn, account_id, raw, HandleType::Phone)?;
            }
            ProfileHandleKind::Email => {
                account_profile::link_account_handle(conn, account_id, raw, HandleType::Email)?;
                account_profile::upsert_account_email(
                    conn,
                    account_id,
                    &raw.to_ascii_lowercase(),
                    false,
                )?;
            }
            ProfileHandleKind::Whatsapp => {
                account_profile::link_account_handle_with_service(
                    conn,
                    account_id,
                    raw,
                    HandleType::Phone,
                    Some("whatsapp"),
                )?;
            }
        }
    }

    Ok(())
}

/// Apply a profile update in one transaction, then reload the response.
fn update_profile_on_conn(
    conn: &mut Connection,
    account_id: &str,
    req: &AccountProfileUpdateRequest,
) -> Result<AccountProfileResponse> {
    let tx = conn.transaction()?;
    apply_profile_update(
        &tx,
        account_id,
        req.preferred_name.as_deref(),
        &req.handles,
        &req.remove_handles,
    )?;
    tx.commit()?;
    load_response(conn, account_id)
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

    let db = state.cfg.paths.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<AccountProfileResponse> {
        let mut conn = schema::open_configured(&db)?;
        update_profile_on_conn(&mut conn, &account_id, &req)
    })
    .await
    .join_map("profile update task", |e| {
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
    let db = state.cfg.paths.db.clone();
    let data_dir = state.cfg.paths.data_dir.clone();
    let assets_name = state.cfg.paths.assets_dir.clone();
    let converted_name = state.cfg.paths.assets_converted_dir.clone();

    let stats =
        tokio::task::spawn_blocking(move || -> Result<account_profile::DeletedMessagesStats> {
            let conn = schema::open_configured(&db)?;
            let stats = account_profile::delete_all_messages_for_account(&conn, &account_id)?;
            remove_account_asset_trees(&data_dir, &account_id, &assets_name, &converted_name)?;
            Ok(stats)
        })
        .await
        .join_blocking("delete messages task")?;

    Ok(Json(DeleteMessagesResponse {
        ok: true,
        conversations: stats.conversations,
        attachments: stats.attachments,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        let account_id = "00000000-0000-4000-8000-000000000001".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, ?2, 0)",
            params![&account_id, "alice"],
        )
        .unwrap();
        (conn, account_id)
    }

    #[test]
    fn apply_profile_update_sets_name_and_handles() {
        let (conn, account_id) = setup();
        apply_profile_update(
            &conn,
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
        .unwrap();

        let loaded = load_response(&conn, &account_id).unwrap();
        assert_eq!(loaded.preferred_name.as_deref(), Some("Alex"));
        assert!(loaded.phones.iter().any(|p| p == "+15555550100"));
        assert!(loaded.phones.iter().any(|p| p == "+15555550199"));
        assert!(loaded.emails.iter().any(|e| e == "alex@example.com"));

        let wa_service: String = conn
            .query_row(
                "SELECT service FROM handles WHERE account_id = ?1 AND normalized = ?2",
                params![&account_id, "+15555550199"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(wa_service, "whatsapp");
    }

    #[test]
    fn apply_profile_update_removes_handles() {
        let (conn, account_id) = setup();
        apply_profile_update(
            &conn,
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
        .unwrap();

        apply_profile_update(
            &conn,
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
        .unwrap();

        let loaded = load_response(&conn, &account_id).unwrap();
        assert!(loaded.phones.is_empty());
        assert!(loaded.emails.is_empty());
    }

    #[test]
    fn load_response_sets_is_guest_true_when_guest_status_assigned() {
        let (conn, account_id) = setup();
        let guest_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        account_profile::insert_guest_account(&conn, guest_id, "guest-bbbb", None).unwrap();
        account_profile::set_guest_status(&conn, guest_id, "assigned").unwrap();

        let guest = load_response(&conn, guest_id).unwrap();
        assert!(guest.is_guest);

        let regular = load_response(&conn, &account_id).unwrap();
        assert!(!regular.is_guest);
    }

    #[test]
    fn profile_update_rolls_back_when_a_handle_service_is_unsupported() {
        let (mut conn, account_id) = setup();

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
        );

        assert!(result.is_err());
        assert_eq!(
            account_profile::load_preferred_name(&conn, &account_id).unwrap(),
            None
        );
    }
}
