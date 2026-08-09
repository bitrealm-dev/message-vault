//! Account profile read + update handlers.

use anyhow::{bail, Result};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use message_ir::HandleType;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{account_profile, schema};
use crate::server::{ApiError, AppState, resolve_auth};

#[derive(Debug, Serialize)]
pub struct AccountProfileResponse {
    pub account_id: String,
    pub username: String,
    pub preferred_name: Option<String>,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
}

fn load_response(conn: &Connection, account_id: &str) -> Result<AccountProfileResponse> {
    let username = account_profile::username_for_account(conn, account_id)?
        .unwrap_or_else(|| account_id.to_string());
    let preferred_name = account_profile::load_preferred_name(conn, account_id)?;
    let profile = account_profile::load_account_profile(conn, account_id)?;
    Ok(AccountProfileResponse {
        account_id: account_id.to_string(),
        username,
        preferred_name,
        phones: profile.phones,
        emails: profile.emails,
    })
}

/// `GET /v1/account/profile`
pub async fn account_profile_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountProfileResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account_id = auth.account_id;

    let db = state.cfg.paths.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<AccountProfileResponse> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        load_response(&conn, &account_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("profile load task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

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
    #[serde(default)]
    pub handles: Vec<ProfileHandleInput>,
}

fn apply_profile_update(
    conn: &Connection,
    account_id: &str,
    preferred_name: Option<&str>,
    handles: &[ProfileHandleInput],
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

    for entry in handles {
        let raw = entry.handle.trim();
        if raw.is_empty() {
            continue;
        }
        let service = entry.service.trim().to_ascii_lowercase();
        match service.as_str() {
            "phone" => {
                account_profile::link_account_handle(
                    conn,
                    account_id,
                    raw,
                    HandleType::Phone,
                )?;
            }
            "email" => {
                account_profile::link_account_handle(
                    conn,
                    account_id,
                    raw,
                    HandleType::Email,
                )?;
                account_profile::upsert_account_email(conn, account_id, &raw.to_ascii_lowercase(), false)?;
            }
            "whatsapp" => {
                account_profile::link_account_handle_with_service(
                    conn,
                    account_id,
                    raw,
                    HandleType::Phone,
                    Some("whatsapp"),
                )?;
            }
            other => bail!("unsupported handle service: {other}"),
        }
    }

    Ok(())
}

/// `POST /v1/account/profile`
pub async fn account_profile_update_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AccountProfileUpdateRequest>,
) -> Result<Json<AccountProfileResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account_id = auth.account_id;

    let db = state.cfg.paths.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<AccountProfileResponse> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        apply_profile_update(
            &conn,
            &account_id,
            req.preferred_name.as_deref(),
            &req.handles,
        )?;
        load_response(&conn, &account_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("profile update task: {e}")))?
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
}
