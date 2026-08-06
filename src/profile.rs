//! Account profile read + update handlers.

use anyhow::Result;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
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
        let username = account_profile::username_for_account(&conn, &account_id)?
            .unwrap_or_else(|| account_id.clone());
        let preferred_name = account_profile::load_preferred_name(&conn, &account_id)?;
        let profile = account_profile::load_account_profile(&conn, &account_id)?;
        Ok(AccountProfileResponse {
            account_id,
            username,
            preferred_name,
            phones: profile.phones,
            emails: profile.emails,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(format!("profile load task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct AccountProfileUpdateRequest {
    #[serde(default)]
    pub preferred_name: Option<String>,
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

        if let Some(ref name) = req.preferred_name {
            let name = name.trim();
            conn.execute(
                "UPDATE accounts SET preferred_name = ?1 WHERE id = ?2",
                rusqlite::params![
                    if name.is_empty() { None::<&str> } else { Some(name) },
                    account_id
                ],
            )?;
        }

        let username = account_profile::username_for_account(&conn, &account_id)?
            .unwrap_or_else(|| account_id.clone());
        let preferred_name = account_profile::load_preferred_name(&conn, &account_id)?;
        let profile = account_profile::load_account_profile(&conn, &account_id)?;
        Ok(AccountProfileResponse {
            account_id,
            username,
            preferred_name,
            phones: profile.phones,
            emails: profile.emails,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(format!("profile update task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(result))
}
