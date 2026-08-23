//! CRUD for named CLI API tokens.

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::api_tokens::{self, ApiTokenScopes};
use crate::db::schema;
use crate::server::{
    ApiError, AppState, JoinBlocking, reject_if_guest_account, require_full_access, resolve_auth,
};

/// One named API token as shown in Settings: label, scopes, and masked secret.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiTokenItem {
    /// Token id (the secret itself is stored hashed).
    pub id: String,
    /// User-chosen label shown in Settings.
    pub label: String,
    /// Scope string (`import`, `export`, or `both`).
    pub scopes: String,
    /// Masked secret for Settings (e.g. `mv-api-Sd..mE`).
    pub token_hint: String,
    /// Creation time as a Unix-seconds string.
    pub created_at: String,
    /// Unix-seconds string of last use; absent when never used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    /// Unix-seconds expiry; absent means no expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// True when the token is disabled and rejects requests.
    pub disabled: bool,
}

impl From<api_tokens::ApiTokenRow> for ApiTokenItem {
    fn from(row: api_tokens::ApiTokenRow) -> Self {
        Self {
            id: row.id,
            label: row.label,
            scopes: row.scopes.as_str().to_string(),
            token_hint: row.token_hint,
            created_at: row.created_at,
            last_accessed_at: row.last_accessed_at,
            expires_at: row.expires_at,
            disabled: row.disabled,
        }
    }
}

/// Label validation rejections are the caller's fault; anything else is a server error.
fn map_label_error(e: crate::db::api_tokens::ApiTokenMutationError) -> ApiError {
    use crate::db::api_tokens::ApiTokenMutationError;
    match e {
        ApiTokenMutationError::InvalidLabel(err) => ApiError::BadRequest(err.to_string()),
        ApiTokenMutationError::Other(err) => ApiError::Internal(err.to_string()),
    }
}

/// The account's named API tokens.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListApiTokensResponse {
    /// The account's tokens.
    pub items: Vec<ApiTokenItem>,
}

/// Body for creating a token: label, scopes, optional expiry.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateApiTokenRequest {
    /// User-chosen label shown in Settings.
    pub label: String,
    /// `import`, `export`, or `both` (default `both`).
    #[serde(default = "default_scopes")]
    pub scopes: String,
    /// Days until expiry. Omit for the default (365 days). Pass `0` for no expiry.
    #[serde(default)]
    pub expires_in_days: Option<u64>,
}

fn default_scopes() -> String {
    "both".into()
}

fn open_accounts_conn(db: &std::path::Path) -> anyhow::Result<Connection> {
    let conn = schema::open_configured(db)?;
    schema::ensure_accounts_schema(&conn)?;
    Ok(conn)
}

/// The created token, including its plaintext secret (returned once).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateApiTokenResponse {
    /// Token id.
    pub id: String,
    /// User-chosen label.
    pub label: String,
    /// Scope string (`import`, `export`, or `both`).
    pub scopes: String,
    /// Creation time as a Unix-seconds string.
    pub created_at: String,
    /// Unix-seconds expiry; absent means no expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Plaintext secret — returned once at creation.
    pub token: String,
    /// Masked form for the Settings list (also persisted).
    pub token_hint: String,
}

/// Deletion acknowledgement.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteApiTokenResponse {
    /// Always true when a response is returned.
    pub ok: bool,
}

/// Body for renaming a token.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RenameApiTokenRequest {
    /// Replacement label.
    pub label: String,
}

/// The renamed token's id and stored label.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameApiTokenResponse {
    /// Always true when a response is returned.
    pub ok: bool,
    /// Token id that was renamed.
    pub id: String,
    /// Stored label after the rename.
    pub label: String,
}

/// List the account's named API tokens with their scopes and masked secrets.
#[utoipa::path(
    get,
    path = "/v1/account/api-tokens",
    tag = "Account",
    security(("bearer" = [])),
    responses(
        (status = 200, body = ListApiTokensResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
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
        Ok(rows.into_iter().map(ApiTokenItem::from).collect())
    })
    .await
    .join_blocking("list API tokens task")?;

    Ok(Json(ListApiTokensResponse { items }))
}

/// Create a named API token. Returns the plaintext secret once, at creation;
/// it is never returned again.
#[utoipa::path(
    post,
    path = "/v1/account/api-tokens",
    tag = "Account",
    security(("bearer" = [])),
    request_body = CreateApiTokenRequest,
    responses(
        (status = 200, body = CreateApiTokenResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn create_api_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<Json<CreateApiTokenResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    reject_if_guest_account(&state.cfg.paths.db, &auth.account_id).await?;
    let account_id = auth.account_id;
    let label = req.label;
    let scopes =
        ApiTokenScopes::parse(&req.scopes).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let expires_in_days = req.expires_in_days;
    let db = state.cfg.paths.db.clone();

    let created = tokio::task::spawn_blocking(
        #[allow(clippy::type_complexity)]
        move || -> Result<(
            String,
            String,
            ApiTokenScopes,
            String,
            Option<String>,
            String,
        ), crate::db::api_tokens::ApiTokenMutationError> {
            let conn = open_accounts_conn(&db)?;
            api_tokens::create_api_token(&conn, &account_id, &label, scopes, expires_in_days)
        },
    )
    .await
    .join_map("create API token task", map_label_error)?;

    Ok(Json(CreateApiTokenResponse {
        id: created.0,
        label: created.1,
        scopes: created.2.as_str().to_string(),
        created_at: created.3,
        expires_at: created.4,
        token_hint: api_tokens::mask_api_token(&created.5),
        token: created.5,
    }))
}

/// Delete one named API token. Requests using it start failing on the next call.
#[utoipa::path(
    delete,
    path = "/v1/account/api-tokens/{id}",
    tag = "Account",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "API token id")),
    responses(
        (status = 200, body = DeleteApiTokenResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
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

/// Rename one named API token. The label is trimmed before storing.
#[utoipa::path(
    patch,
    path = "/v1/account/api-tokens/{id}",
    tag = "Account",
    security(("bearer" = [])),
    params(("id" = String, Path, description = "API token id")),
    request_body = RenameApiTokenRequest,
    responses(
        (status = 200, body = RenameApiTokenResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub async fn rename_api_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<RenameApiTokenRequest>,
) -> Result<Json<RenameApiTokenResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    reject_if_guest_account(&state.cfg.paths.db, &auth.account_id).await?;
    let account_id = auth.account_id;
    let label = req.label;
    let db = state.cfg.paths.db.clone();
    let id_for_resp = id.clone();

    let updated = tokio::task::spawn_blocking(
        move || -> Result<(bool, String), crate::db::api_tokens::ApiTokenMutationError> {
            let conn = open_accounts_conn(&db)?;
            let trimmed = label.trim().to_string();
            let ok = api_tokens::update_api_token_label(&conn, &account_id, &id, &trimmed)?;
            Ok((ok, trimmed))
        },
    )
    .await
    .join_map("rename API token task", map_label_error)?;

    if !updated.0 {
        return Err(ApiError::NotFound("API token not found".into()));
    }
    Ok(Json(RenameApiTokenResponse {
        ok: true,
        id: id_for_resp,
        label: updated.1,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api_tokens::{ApiTokenLabelError, ApiTokenMutationError};

    #[test]
    fn label_errors_map_to_bad_request_with_the_same_message() {
        let err = map_label_error(ApiTokenMutationError::InvalidLabel(
            ApiTokenLabelError::Required,
        ));
        match err {
            ApiError::BadRequest(msg) => assert_eq!(msg, "label is required"),
            other => panic!("expected BadRequest, got {other:?}"),
        }

        let err = map_label_error(ApiTokenMutationError::InvalidLabel(
            ApiTokenLabelError::TooLong,
        ));
        match err {
            ApiError::BadRequest(msg) => assert_eq!(msg, "label must be at most 120 characters"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn other_errors_map_to_internal() {
        let err = map_label_error(ApiTokenMutationError::Other(anyhow::anyhow!("boom")));
        match err {
            ApiError::Internal(msg) => assert_eq!(msg, "boom"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
