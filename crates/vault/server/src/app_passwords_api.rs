//! CRUD for named app passwords (CLI import/export credentials).

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::app_passwords::{self, AppPasswordScopes};
use crate::db::schema;
use crate::server::{ApiError, AppState, require_full_access, resolve_auth};

#[derive(Debug, Serialize)]
pub struct AppPasswordItem {
    pub id: String,
    pub label: String,
    pub scopes: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListAppPasswordsResponse {
    pub items: Vec<AppPasswordItem>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAppPasswordRequest {
    pub label: String,
    /// `import`, `export`, or `both` (default `both`).
    #[serde(default = "default_scopes")]
    pub scopes: String,
}

fn default_scopes() -> String {
    "both".into()
}

#[derive(Debug, Serialize)]
pub struct CreateAppPasswordResponse {
    pub id: String,
    pub label: String,
    pub scopes: String,
    pub created_at: String,
    /// Plaintext secret — returned once at creation.
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteAppPasswordResponse {
    pub ok: bool,
}

/// `GET /v1/account/app-passwords`
pub async fn list_app_passwords_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ListAppPasswordsResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let db = state.cfg.paths.db.clone();

    let items = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<AppPasswordItem>> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        schema::ensure_accounts_schema(&conn)?;
        let rows = app_passwords::list_app_passwords(&conn, &account_id)?;
        Ok(rows
            .into_iter()
            .map(|r| AppPasswordItem {
                id: r.id,
                label: r.label,
                scopes: r.scopes.as_str().to_string(),
                created_at: r.created_at,
            })
            .collect())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("list app passwords task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ListAppPasswordsResponse { items }))
}

/// `POST /v1/account/app-passwords`
pub async fn create_app_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateAppPasswordRequest>,
) -> Result<Json<CreateAppPasswordResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let label = req.label;
    let scopes = AppPasswordScopes::parse(&req.scopes).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let db = state.cfg.paths.db.clone();

    let created = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(String, String, AppPasswordScopes, String, String)> {
            let conn = Connection::open(&db)?;
            schema::configure_connection(&conn)?;
            schema::ensure_accounts_schema(&conn)?;
            app_passwords::create_app_password(&conn, &account_id, &label, scopes)
        },
    )
    .await
    .map_err(|e| ApiError::Internal(format!("create app password task: {e}")))?
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("label is required")
            || msg.contains("at most 120")
            || msg.contains("scopes must be")
        {
            ApiError::BadRequest(msg)
        } else {
            ApiError::Internal(msg)
        }
    })?;

    Ok(Json(CreateAppPasswordResponse {
        id: created.0,
        label: created.1,
        scopes: created.2.as_str().to_string(),
        created_at: created.3,
        token: created.4,
    }))
}

/// `DELETE /v1/account/app-passwords/{id}`
pub async fn delete_app_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<DeleteAppPasswordResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let db = state.cfg.paths.db.clone();

    let deleted = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        schema::ensure_accounts_schema(&conn)?;
        app_passwords::delete_app_password(&conn, &account_id, &id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("delete app password task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !deleted {
        return Err(ApiError::NotFound("app password not found".into()));
    }
    Ok(Json(DeleteAppPasswordResponse { ok: true }))
}
