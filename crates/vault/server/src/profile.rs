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

#[derive(Debug, Serialize)]
pub struct AccountProfileResponse {
    pub account_id: String,
    pub username: String,
    pub preferred_name: Option<String>,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    /// True for the seeded demo account (cannot be deleted).
    pub is_demo: bool,
    pub read_only: bool,
}

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
        read_only,
    })
}

/// `GET /v1/account/profile`
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

#[derive(Debug, Deserialize)]
pub struct ProfileHandleInput {
    pub handle: String,
    pub service: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountProfileUpdateRequest {
    #[serde(default)]
    pub preferred_name: Option<String>,
    /// Handles to add/link onto the account profile.
    #[serde(default)]
    pub handles: Vec<ProfileHandleInput>,
    /// Handles to unlink from the account profile.
    #[serde(default)]
    pub remove_handles: Vec<ProfileHandleInput>,
}

fn apply_profile_update(
    conn: &Connection,
    account_id: &str,
    preferred_name: Option<&str>,
    handles: &[ProfileHandleInput],
    remove_handles: &[ProfileHandleInput],
) -> Result<()> {
    if let Some(name) = preferred_name {
        let name = name.trim();
        conn.execute(
            "UPDATE accounts SET preferred_name = ?1 WHERE id = ?2",
            rusqlite::params![
                if name.is_empty() {
                    None::<&str>
                } else {
                    Some(name)
                },
                account_id
            ],
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

enum ProfileHandleKind {
    Phone,
    Email,
    Whatsapp,
}

fn parse_profile_service(service: &str) -> Result<ProfileHandleKind> {
    match service.trim().to_ascii_lowercase().as_str() {
        "phone" => Ok(ProfileHandleKind::Phone),
        "email" => Ok(ProfileHandleKind::Email),
        "whatsapp" => Ok(ProfileHandleKind::Whatsapp),
        other => bail!("unsupported handle service: {other}"),
    }
}

/// `POST /v1/account/profile`
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
        let conn = schema::open_configured(&db)?;
        apply_profile_update(
            &conn,
            &account_id,
            req.preferred_name.as_deref(),
            &req.handles,
            &req.remove_handles,
        )?;
        load_response(&conn, &account_id)
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

#[derive(Debug, Deserialize)]
pub struct DeleteMessagesRequest {
    pub confirm: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteMessagesResponse {
    pub ok: bool,
    pub conversations: u64,
    pub attachments: u64,
}

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

/// `POST /v1/account/delete-messages` — delete conversations/messages/attachments;
/// keep contacts and account login.
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
}
