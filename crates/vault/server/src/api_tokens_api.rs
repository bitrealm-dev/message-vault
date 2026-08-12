//! CRUD for named CLI API tokens.

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::api_tokens::{self, ApiTokenScopes};
use crate::db::schema;
use crate::server::{ApiError, AppState, JoinBlocking, require_full_access, resolve_auth};

#[derive(Debug, Serialize)]
pub struct ApiTokenItem {
    pub id: String,
    pub label: String,
    pub scopes: String,
    /// Masked secret for Settings (e.g. `mv-api-Sd..mE`).
    pub token_hint: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListApiTokensResponse {
    pub items: Vec<ApiTokenItem>,
}

#[derive(Debug, Deserialize)]
pub struct CreateApiTokenRequest {
    pub label: String,
    /// `import`, `export`, or `both` (default `both`).
    #[serde(default = "default_scopes")]
    pub scopes: String,
}

fn default_scopes() -> String {
    "both".into()
}

fn open_accounts_conn(db: &std::path::Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(db)?;
    schema::configure_connection(&conn)?;
    schema::ensure_accounts_schema(&conn)?;
    Ok(conn)
}

#[derive(Debug, Serialize)]
pub struct CreateApiTokenResponse {
    pub id: String,
    pub label: String,
    pub scopes: String,
    pub created_at: String,
    /// Plaintext secret — returned once at creation.
    pub token: String,
    /// Masked form for the Settings list (also persisted).
    pub token_hint: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteApiTokenResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize)]
pub struct RenameApiTokenRequest {
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct RenameApiTokenResponse {
    pub ok: bool,
    pub id: String,
    pub label: String,
}

/// `GET /v1/account/api-tokens`
pub async fn list_api_tokens_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ListApiTokensResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let db = state.cfg.paths.db.clone();

    let items = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<ApiTokenItem>> {
        let conn = open_accounts_conn(&db)?;
        let rows = api_tokens::list_api_tokens(&conn, &account_id)?;
        Ok(rows
            .into_iter()
            .map(|r| ApiTokenItem {
                id: r.id,
                label: r.label,
                scopes: r.scopes.as_str().to_string(),
                token_hint: r.token_hint,
                created_at: r.created_at,
                last_accessed_at: r.last_accessed_at,
            })
            .collect())
    })
    .await
    .join_blocking("list API tokens task")?;

    Ok(Json(ListApiTokensResponse { items }))
}

/// `POST /v1/account/api-tokens`
pub async fn create_api_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<Json<CreateApiTokenResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let label = req.label;
    let scopes =
        ApiTokenScopes::parse(&req.scopes).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let db = state.cfg.paths.db.clone();

    let created = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(String, String, ApiTokenScopes, String, String)> {
            let conn = open_accounts_conn(&db)?;
            api_tokens::create_api_token(&conn, &account_id, &label, scopes)
        },
    )
    .await
    .map_err(|e| ApiError::Internal(format!("create API token task: {e}")))?
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

    Ok(Json(CreateApiTokenResponse {
        id: created.0,
        label: created.1,
        scopes: created.2.as_str().to_string(),
        created_at: created.3,
        token_hint: api_tokens::mask_api_token(&created.4),
        token: created.4,
    }))
}

/// `DELETE /v1/account/api-tokens/{id}`
pub async fn delete_api_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<DeleteApiTokenResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let db = state.cfg.paths.db.clone();

    let deleted = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let conn = open_accounts_conn(&db)?;
        api_tokens::delete_api_token(&conn, &account_id, &id)
    })
    .await
    .join_blocking("delete API token task")?;

    if !deleted {
        return Err(ApiError::NotFound("API token not found".into()));
    }
    Ok(Json(DeleteApiTokenResponse { ok: true }))
}

/// `PATCH /v1/account/api-tokens/{id}`
pub async fn rename_api_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<RenameApiTokenRequest>,
) -> Result<Json<RenameApiTokenResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let label = req.label;
    let db = state.cfg.paths.db.clone();
    let id_for_resp = id.clone();

    let updated = tokio::task::spawn_blocking(move || -> anyhow::Result<(bool, String)> {
        let conn = open_accounts_conn(&db)?;
        let trimmed = label.trim().to_string();
        let ok = api_tokens::update_api_token_label(&conn, &account_id, &id, &trimmed)?;
        Ok((ok, trimmed))
    })
    .await
    .map_err(|e| ApiError::Internal(format!("rename API token task: {e}")))?
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("label is required") || msg.contains("at most 120") {
            ApiError::BadRequest(msg)
        } else {
            ApiError::Internal(msg)
        }
    })?;

    if !updated.0 {
        return Err(ApiError::NotFound("API token not found".into()));
    }
    Ok(Json(RenameApiTokenResponse {
        ok: true,
        id: id_for_resp,
        label: updated.1,
    }))
}
