use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Context;
use axum::extract::{FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::ServeDir;

use rusqlite::Connection;

use crate::asset_uploads;
use crate::assets;
use crate::config::{Config, validate_source_id};
use crate::db::account_profile;
use crate::db::api_tokens;
use crate::db::schema;
use crate::db::session_tokens;
use crate::dedupe;
use crate::export_api::{
    self, DEFAULT_EXPORT_LIMIT, ExportCountOpts, ExportPageOpts, ExportQueryError,
};
use crate::import::{self, ImportMode, ImportOptions, ImportStats};

/// What a Bearer credential is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCapability {
    /// GUI session token — full API access.
    Full,
    /// Named API token with import and/or export rights.
    ApiToken(crate::db::api_tokens::ApiTokenScopes),
}

/// Authenticated vault account from a session token or named API token.
#[derive(Debug, Clone)]
pub struct AuthIdentity {
    pub account_id: String,
    pub capability: AuthCapability,
}

/// Reject API tokens on routes that require a GUI session.
pub fn require_full_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    match auth.capability {
        AuthCapability::Full => Ok(()),
        AuthCapability::ApiToken(_) => Err(ApiError::Forbidden(
            "this endpoint requires a signed-in session; use an API token only for import/export"
                .into(),
        )),
    }
}

/// Allow session or an API token that includes import.
pub fn require_import_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    match auth.capability {
        AuthCapability::Full => Ok(()),
        AuthCapability::ApiToken(scopes) if scopes.allows_import() => Ok(()),
        AuthCapability::ApiToken(_) => Err(ApiError::Forbidden(
            "this API token does not allow import".into(),
        )),
    }
}

/// Allow session or an API token that includes export.
pub fn require_export_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    match auth.capability {
        AuthCapability::Full => Ok(()),
        AuthCapability::ApiToken(scopes) if scopes.allows_export() => Ok(()),
        AuthCapability::ApiToken(_) => Err(ApiError::Forbidden(
            "this API token does not allow export".into(),
        )),
    }
}

/// Allow session or any API token (import, export, or both) for asset probes.
pub fn require_import_or_export_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    match auth.capability {
        AuthCapability::Full => Ok(()),
        AuthCapability::ApiToken(scopes) if scopes.allows_import() || scopes.allows_export() => {
            Ok(())
        }
        AuthCapability::ApiToken(_) => Err(ApiError::Forbidden(
            "this API token cannot access assets".into(),
        )),
    }
}

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    /// Warm connection for short import-session SQL only (`POST /v1/imports`,
    /// complete, import-id verify / one-shot start). Bulk `POST /v1/import` and
    /// export open their own connections so they do not hold this mutex.
    db: Arc<StdMutex<Connection>>,
    /// Per-account import mutex: same-account imports stay serialized so staging
    /// rows for that tenant are not wiped mid-run. Different accounts may overlap
    /// at the lock layer; SQLite WAL + busy_timeout serialize writers.
    account_import_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Serialize multipart complete per (account, sha256) so two clients cannot
    /// race `store_verified` on the same digest.
    asset_complete_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Multipart / asset size limits from `[server]` (env may override part size).
    upload_limits: asset_uploads::UploadLimits,
    /// Axum request body cap (single PUT or one part); equals `asset_max_bytes`.
    max_body_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct ImportQuery {
    source: String,
    /// Username or UUID. Optional; when set must match the Bearer token's account.
    #[serde(default)]
    account: Option<String>,
    #[serde(default = "default_import_mode")]
    mode: String,
    /// Run cross-source soft-dedupe after import.
    #[serde(default)]
    dedupe: bool,
    /// Optional vault import session id from POST /v1/imports.
    #[serde(default)]
    import_id: Option<i64>,
    /// How vault contacts supply participant names (`fill_missing`, `overwrite`, or `as_is`).
    #[serde(default = "default_contact_name_mode")]
    contact_name_mode: String,
}

fn default_contact_name_mode() -> String {
    "fill_missing".to_string()
}

fn default_import_mode() -> String {
    "append".to_string()
}

#[derive(Debug, Serialize)]
struct ImportResponse {
    ok: bool,
    source: String,
    account: String,
    #[serde(flatten)]
    stats: ImportStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    dedupe: Option<DedupeResponse>,
}

#[derive(Debug, Serialize)]
struct DedupeResponse {
    keys_filled: u64,
    exact_groups: u64,
    exact_flagged: u64,
    near_flagged: u64,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    ok: bool,
    error: String,
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    BadRequest(String),
    NotFound(String),
    TooManyRequests(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, m),
            Self::Internal(m) => {
                // Keep diagnostics server-side; clients only see a stable message.
                eprintln!("internal error: {m}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".into(),
                )
            }
        };
        (
            status,
            Json(ErrorBody {
                ok: false,
                error: message,
            }),
        )
            .into_response()
    }
}

impl From<ExportQueryError> for ApiError {
    fn from(e: ExportQueryError) -> Self {
        match e {
            ExportQueryError::BadRequest(m) => Self::BadRequest(m),
            ExportQueryError::Internal(m) => Self::Internal(m),
        }
    }
}

impl From<crate::db::vault_imports::ImportLookupError> for ApiError {
    fn from(e: crate::db::vault_imports::ImportLookupError) -> Self {
        match e {
            crate::db::vault_imports::ImportLookupError::NotFound { import_id } => {
                Self::NotFound(format!("import {import_id} not found for this account"))
            }
            crate::db::vault_imports::ImportLookupError::InvalidSession { message } => {
                Self::BadRequest(message)
            }
            crate::db::vault_imports::ImportLookupError::Db(err) => Self::Internal(err.to_string()),
        }
    }
}

/// Map a `spawn_blocking` join + inner error onto `ApiError`.
pub(crate) trait JoinBlocking<T, E>: Sized {
    fn join_blocking(self, task: &str) -> Result<T, ApiError>
    where
        E: ToString,
    {
        self.join_map(task, |e| ApiError::Internal(e.to_string()))
    }

    fn join_map(self, task: &str, map: impl FnOnce(E) -> ApiError) -> Result<T, ApiError>;
}

impl<T, E> JoinBlocking<T, E> for Result<Result<T, E>, tokio::task::JoinError> {
    fn join_map(self, task: &str, map: impl FnOnce(E) -> ApiError) -> Result<T, ApiError> {
        self.map_err(|e| ApiError::Internal(format!("{task}: {e}")))?
            .map_err(map)
    }
}

fn lock_conn(db: &StdMutex<Connection>) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
    lock_named(db, "database")
}

fn lock_import_conn(
    db: &StdMutex<Connection>,
) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
    lock_named(db, "import database")
}

fn lock_named<'a>(
    db: &'a StdMutex<Connection>,
    what: &str,
) -> anyhow::Result<std::sync::MutexGuard<'a, Connection>> {
    db.lock()
        .map_err(|_| anyhow::anyhow!("{what} mutex poisoned"))
}

/// Build CORS from `[server].cors_origins`.
///
/// - empty → no cross-origin allow list (same-origin UI / API is fine)
/// - `["*"]` → fully permissive (local debugging only)
/// - otherwise → exact origin allow list
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    if origins.iter().any(|o| o.trim() == "*") {
        return CorsLayer::permissive();
    }
    if origins.is_empty() {
        return CorsLayer::new();
    }
    let allowed: Vec<header::HeaderValue> = origins
        .iter()
        .filter_map(|o| {
            let t = o.trim();
            if t.is_empty() { None } else { t.parse().ok() }
        })
        .collect();
    if allowed.is_empty() {
        return CorsLayer::new();
    }
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
}

pub(crate) async fn with_configured_db<T, F>(
    db_path: &Path,
    task: &str,
    f: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
{
    let db = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let conn = schema::open_configured(&db)?;
        f(&conn)
    })
    .await
    .join_blocking(task)
}

async fn with_configured_db_map<T, E, F>(db_path: &Path, task: &str, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    E: From<anyhow::Error> + Send + 'static,
    F: FnOnce(&Connection) -> Result<T, E> + Send + 'static,
    ApiError: From<E>,
{
    let db = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let conn = schema::open_configured(&db)?;
        f(&conn)
    })
    .await
    .join_map(task, ApiError::from)
}

async fn with_locked_conn<T, E, F>(
    db: Arc<StdMutex<Connection>>,
    task: &str,
    f: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    E: From<anyhow::Error> + ToString + Send + 'static,
    F: FnOnce(&Connection) -> Result<T, E> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let conn = lock_conn(&db)?;
        f(&conn)
    })
    .await
    .join_blocking(task)
}

pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let server = cfg.require_server()?.clone();
    let bind = server.bind.clone();
    let upload_limits = asset_uploads::UploadLimits::resolve(
        server.asset_part_size,
        server.asset_max_bytes,
        server.asset_hash_threshold_bytes,
    );
    let max_body_bytes = upload_limits.max_bytes as usize;

    // Open a warm writer, recover hot journals, and ensure schema once before serving.
    let db_conn = schema::open_configured(&cfg.paths.db)?;
    let _: i64 = db_conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))?;
    schema::ensure_vault_schema(&db_conn)?;
    let mode: String = db_conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".into());
    eprintln!("  db:   {} (journal_mode={mode})", cfg.paths.db.display());
    eprintln!(
        "  assets: max={} MiB  part_size={} MiB",
        upload_limits.max_bytes / (1024 * 1024),
        upload_limits.part_size / (1024 * 1024)
    );
    let db = Arc::new(StdMutex::new(db_conn));

    let state = AppState {
        cfg: Arc::new(cfg),
        db,
        account_import_locks: Arc::new(Mutex::new(HashMap::new())),
        asset_complete_locks: Arc::new(Mutex::new(HashMap::new())),
        upload_limits,
        max_body_bytes,
    };

    let auth_public = Router::new()
        .route("/v1/auth/register", post(crate::auth::register_handler))
        .route("/v1/auth/login", post(crate::auth::login_handler))
        .route(
            "/v1/auth/hanko/session",
            post(crate::auth::hanko_session_handler),
        )
        // Auth JSON is tiny; keep a tight limit so Argon2/JWKS abuse cannot ship 512 MiB bodies.
        .layer(RequestBodyLimitLayer::new(32 * 1024));

    let app = Router::new()
        .merge(auth_public)
        .route("/health", get(health))
        .route("/v1/auth/mode", get(auth_mode_handler))
        .route("/v1/auth/check", get(auth_check))
        .route("/v1/auth/logout", post(crate::auth::logout_handler))
        .route(
            "/v1/auth/change-password",
            post(crate::auth::change_password_handler),
        )
        .route(
            "/v1/auth/delete-account",
            post(crate::auth::delete_account_handler),
        )
        .route(
            "/v1/account/profile",
            get(crate::profile::account_profile_handler)
                .post(crate::profile::account_profile_update_handler),
        )
        .route(
            "/v1/account/delete-messages",
            post(crate::profile::delete_messages_handler),
        )
        .route("/v1/account/storage", get(account_storage_handler))
        .route(
            "/v1/account/api-tokens",
            get(crate::api_tokens_api::list_api_tokens_handler)
                .post(crate::api_tokens_api::create_api_token_handler),
        )
        .route(
            "/v1/account/api-tokens/{id}",
            delete(crate::api_tokens_api::delete_api_token_handler)
                .patch(crate::api_tokens_api::rename_api_token_handler),
        )
        .route(
            "/v1/export/messages/count",
            get(export_messages_count_handler),
        )
        .route("/v1/export/messages", get(export_messages_handler))
        .route("/v1/export/contacts", get(contacts_list_handler))
        .route(
            "/v1/export/contacts/{id}",
            get(contact_detail_handler).post(contact_mutate_handler),
        )
        .route("/v1/export/conversations", get(conversations_list_handler))
        .route(
            "/v1/export/conversations/{id}/sources",
            get(conversation_sources_handler),
        )
        .route("/v1/imports", get(imports_list_handler))
        .route("/v1/imports", post(imports_create_handler))
        .route("/v1/imports/{id}", get(imports_get_handler))
        .route("/v1/imports/{id}/complete", post(imports_complete_handler))
        .route("/v1/import", post(import_handler))
        .route(
            "/v1/assets/{sha256}",
            get(asset_get_handler)
                .put(asset_put_handler)
                .head(asset_head_handler),
        )
        .route(
            "/v1/assets/{sha256}/uploads",
            post(asset_upload_start_handler),
        )
        .route(
            "/v1/assets/{sha256}/uploads/{upload_id}/parts/{part}",
            put(asset_upload_part_handler),
        )
        .route(
            "/v1/assets/{sha256}/uploads/{upload_id}/complete",
            post(asset_upload_complete_handler),
        )
        .route(
            "/v1/assets/{sha256}/uploads/{upload_id}",
            delete(asset_upload_abort_handler),
        )
        .fallback_service(ServeDir::new("static"))
        .layer(build_cors_layer(&server.cors_origins))
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("message-vault-server serve listening on http://{bind}");
    eprintln!("  GET  /health");
    eprintln!("  GET  /v1/auth/mode     (unauthenticated — returns hanko or local)");
    eprintln!("  GET  /v1/auth/check   (Bearer session token or API token)");
    eprintln!("  GET  /v1/export/messages?q=&limit=&cursor=&account=  (read-only export)");
    eprintln!("  GET  /v1/export/messages/count?q=&account=&source=  (export match counts)");
    eprintln!("  GET  /v1/assets/{{sha256}}?source=&account=  (download content-addressed media)");
    eprintln!("  GET  /v1/imports       (list past import sessions with stats)");
    eprintln!("  GET  /v1/account/storage  (usage + top attachments)");
    eprintln!("  POST /v1/imports  (start import session; returns id)");
    eprintln!("  GET  /                  (static files — Vite SPA)");
    eprintln!("  POST /v1/imports/{{id}}/complete");
    eprintln!("  HEAD /v1/assets/{{sha256}}?source=&account=  (probe before PUT)");
    eprintln!("  PUT  /v1/assets/{{sha256}}?source=&account=  (raw body; content-addressed media)");
    eprintln!("  POST /v1/assets/{{sha256}}/uploads?source=&account=  (start multipart)");
    eprintln!("  PUT  /v1/assets/{{sha256}}/uploads/{{id}}/parts/{{n}}  (part body)");
    eprintln!("  POST /v1/assets/{{sha256}}/uploads/{{id}}/complete");
    eprintln!("  DELETE /v1/assets/{{sha256}}/uploads/{{id}}  (abort)");
    eprintln!("  POST /v1/import?source=&account=&mode=append|replace&dedupe=false&import_id=");
    eprintln!("       account= optional (must match token); derived from Bearer when omitted");
    eprintln!("       Content-Type: application/jsonl  (body only; assets by sha256)");
    eprintln!("       Content-Type: multipart/form-data   (field jsonl + file parts; remote push)");
    eprintln!("       Export routes are read-only (no delete); same Bearer token as import");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    eprintln!("shutting down");
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

/// Returns the server's configured authentication mode so clients
/// can render the correct login form before authenticating.
async fn auth_mode_handler() -> Json<serde_json::Value> {
    let mode = crate::config::AuthMode::from_env();
    let hanko_api_url = std::env::var("HANKO_API_URL")
        .ok()
        .or_else(|| std::env::var("NEXT_PUBLIC_HANKO_API_URL").ok());
    Json(serde_json::json!({
        "mode": match mode {
            crate::config::AuthMode::Hanko => "hanko",
            crate::config::AuthMode::Local => "local",
        },
        "hanko_api_url": hanko_api_url,
    }))
}

#[derive(Debug, Deserialize)]
struct AuthCheckQuery {
    #[serde(default)]
    account: Option<String>,
}

#[derive(Debug, Serialize)]
struct AuthCheckResponse {
    ok: bool,
    sources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    admin: Option<bool>,
}

async fn auth_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuthCheckQuery>,
) -> Result<Json<AuthCheckResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account_id = auth.account_id;
    let username = load_username(&state.cfg.paths.db, &account_id).await?;

    if let Some(q) = query
        .account
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let resolved = lookup_or_resolve_query(&state.cfg.paths.db, q).await?;
        let matches = match resolved {
            Some(resolved) => resolved == account_id,
            None => q == account_id,
        };
        if !matches {
            let for_user = username.as_deref().unwrap_or(account_id.as_str());
            return Err(ApiError::Forbidden(format!(
                "account query does not match token's account (token is for {for_user})"
            )));
        }
    }
    let sources = list_account_sources(&state.cfg.paths.db, &account_id).await?;
    Ok(Json(AuthCheckResponse {
        ok: true,
        sources,
        account_id: Some(account_id),
        username,
        account_ok: Some(true),
        admin: None,
    }))
}

async fn list_account_sources(db_path: &Path, account_id: &str) -> Result<Vec<String>, ApiError> {
    let account_id = account_id.to_string();
    // Read-only: do not run ensure_vault_schema (avoids write locks on auth).
    with_configured_db(db_path, "sources list task", move |conn| {
        dedupe::source_priority_from_db(conn, &account_id)
    })
    .await
}

async fn lookup_or_resolve_query(
    db_path: &Path,
    account_ref: &str,
) -> Result<Option<String>, ApiError> {
    let account_ref = account_ref.to_string();
    with_configured_db(db_path, "account lookup task", move |conn| {
        account_profile::lookup_account_ref(conn, &account_ref)
    })
    .await
}

async fn load_username(db_path: &Path, account_id: &str) -> Result<Option<String>, ApiError> {
    let account_id = account_id.to_string();
    with_configured_db(db_path, "username lookup task", move |conn| {
        account_profile::username_for_account(conn, &account_id)
    })
    .await
}

async fn resolve_account_ref_async(db_path: &Path, account_ref: &str) -> Result<String, ApiError> {
    let db = db_path.to_path_buf();
    let account_ref = account_ref.to_string();
    tokio::task::spawn_blocking(move || account_profile::resolve_account_ref_at(&db, &account_ref))
        .await
        .join_map("account resolve task", |e| {
            ApiError::BadRequest(e.to_string())
        })
}

pub fn bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(ApiError::Unauthorized(
            "missing Authorization: Bearer <token>".into(),
        ));
    };
    let value = value
        .to_str()
        .map_err(|_| ApiError::Unauthorized("invalid Authorization header".into()))?;
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::Unauthorized(
            "Authorization must be Bearer <token>".into(),
        ));
    };
    let token = token.trim();
    if token.is_empty() {
        return Err(ApiError::Unauthorized("empty API token".into()));
    }
    Ok(token.to_string())
}

pub async fn resolve_auth(headers: &HeaderMap, state: &AppState) -> Result<AuthIdentity, ApiError> {
    let token = bearer_token(headers)?;
    // Always look up against SQLite so rotate/delete in Settings takes effect
    // without restarting serve (no process-local token cache).
    let token_owned = token.clone();
    let resolved = with_configured_db(&state.cfg.paths.db, "auth lookup task", move |conn| {
        schema::ensure_accounts_schema(conn)?;
        if let Some(account_id) = session_tokens::lookup_account_for_token(conn, &token_owned)? {
            return Ok(Some(AuthIdentity {
                account_id,
                capability: AuthCapability::Full,
            }));
        }
        if let Some(tok) = api_tokens::lookup_account_for_api_token(conn, &token_owned)? {
            return Ok(Some(AuthIdentity {
                account_id: tok.account_id,
                capability: AuthCapability::ApiToken(tok.scopes),
            }));
        }
        Ok(None)
    })
    .await?;

    resolved.ok_or_else(|| ApiError::Unauthorized("invalid API token".into()))
}

/// Resolve the account id for an import: Bearer token binds the account.
/// Optional query may be username or UUID and must match the token.
async fn resolve_import_account(
    auth: &AuthIdentity,
    query_account: Option<&str>,
    db_path: &Path,
) -> Result<String, ApiError> {
    let query = query_account.map(str::trim).filter(|s| !s.is_empty());
    if let Some(q) = query {
        let resolved = resolve_account_ref_async(db_path, q).await?;
        if resolved != auth.account_id {
            return Err(ApiError::Forbidden(
                "account query does not match token's account".into(),
            ));
        }
    }
    Ok(auth.account_id.clone())
}

fn content_type_base(headers: &HeaderMap) -> Option<&str> {
    let ct = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    Some(ct.split(';').next().unwrap_or(ct).trim())
}

fn is_jsonl_content_type(base: &str) -> bool {
    base.eq_ignore_ascii_case("application/jsonl")
}

fn is_multipart_content_type(base: &str) -> bool {
    base.eq_ignore_ascii_case("multipart/form-data")
}

/// Reject path traversal; allow only relative Normal/CurDir components.
fn safe_rel_path(name: &str) -> Result<PathBuf, ApiError> {
    crate::config::safe_rel_path(name).map_err(|e| ApiError::BadRequest(e.to_string()))
}

#[derive(Debug, Deserialize)]
struct CreateImportBody {
    source: String,
    #[serde(default = "default_import_mode")]
    mode: String,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    account: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateImportResponse {
    ok: bool,
    id: i64,
}

#[derive(Debug, Deserialize)]
struct CompleteImportBody {
    #[serde(default = "default_true")]
    ok: bool,
    #[serde(default)]
    message_count: Option<i64>,
    #[serde(default)]
    attachment_count: Option<i64>,
    #[serde(default)]
    bytes_uploaded: Option<i64>,
    #[serde(default)]
    duration_ms: Option<i64>,
    #[serde(default)]
    parse_ms: Option<i64>,
    #[serde(default)]
    convert_ms: Option<i64>,
    #[serde(default)]
    upload_ms: Option<i64>,
    #[serde(default)]
    summary: Option<serde_json::Value>,
    #[serde(default)]
    issues: Vec<CompleteImportIssueBody>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct CompleteImportIssueBody {
    kind: String,
    step: String,
    item: String,
    reason: String,
}

fn validate_complete_import_issues(issues: &[CompleteImportIssueBody]) -> Result<(), ApiError> {
    for issue in issues {
        match issue.kind.as_str() {
            "error" | "skip" => {}
            other => {
                return Err(ApiError::BadRequest(format!(
                    "invalid import issue kind '{other}'; expected 'error' or 'skip'"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct CompleteImportResponse {
    ok: bool,
    id: i64,
    status: String,
    message_count: i64,
    attachment_count: i64,
    bytes_uploaded: i64,
}

#[derive(Debug, Deserialize)]
struct ListPageQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

async fn contacts_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListPageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let q = query.q.unwrap_or_default();
    let limit = query
        .limit
        .unwrap_or(crate::contacts_api::DEFAULT_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let page = with_locked_conn(db, "contacts list task", move |conn| {
        crate::contacts_api::list_contacts(conn, &auth.account_id, &q, limit, offset)
    })
    .await?;
    Ok(Json(serde_json::json!({
        "contacts": page.contacts,
        "total": page.total,
        "limit": page.limit,
        "offset": page.offset,
    })))
}

async fn conversations_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListPageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let q = query.q.unwrap_or_default();
    let limit = query
        .limit
        .unwrap_or(crate::conversations_api::DEFAULT_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let page = with_locked_conn(db, "conversations list task", move |conn| {
        crate::conversations_api::list_conversations(conn, &auth.account_id, &q, limit, offset)
    })
    .await?;
    Ok(Json(serde_json::json!({
        "conversations": page.conversations,
        "total": page.total,
        "limit": page.limit,
        "offset": page.offset,
    })))
}

async fn conversation_sources_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let page = with_locked_conn(db, "conversation sources task", move |conn| {
        crate::conversations_api::list_conversation_source_stats(
            conn,
            &auth.account_id,
            conversation_id,
        )
    })
    .await?;
    page.map(|p| Json(serde_json::json!({ "sources": p.sources })))
        .ok_or_else(|| ApiError::NotFound("conversation not found".into()))
}

async fn contact_detail_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(contact_id): AxumPath<i64>,
) -> Result<Json<crate::contacts_api::ContactDetail>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let detail = with_locked_conn(db, "contact detail task", move |conn| {
        crate::contacts_api::get_contact_detail(conn, &auth.account_id, contact_id)
    })
    .await?;
    detail
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("contact not found".into()))
}

async fn contact_mutate_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(contact_id): AxumPath<i64>,
    Json(body): Json<crate::contacts_api::ContactMutationBody>,
) -> Result<Json<crate::contacts_api::ContactDetail>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let db = Arc::clone(&state.db);
    let account_id = auth.account_id.clone();
    let detail = tokio::task::spawn_blocking(move || -> Result<_, ApiError> {
        let conn = lock_conn(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
        match crate::contacts_api::mutate_contact(&conn, &account_id, contact_id, &body) {
            Ok(false) => Err(ApiError::NotFound("contact not found".into())),
            Err(e) => Err(ApiError::BadRequest(e.to_string())),
            Ok(true) => crate::contacts_api::get_contact_detail(&conn, &account_id, contact_id)
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .ok_or_else(|| ApiError::Internal("contact missing after mutate".into())),
        }
    })
    .await
    .join_map("contact mutate task", |e| e)?;

    Ok(Json(detail))
}

#[derive(Debug, Deserialize)]
struct ListImportsQuery {
    #[serde(default)]
    account: Option<String>,
}

async fn imports_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListImportsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;

    let db = Arc::clone(&state.db);
    let imports = with_locked_conn(db, "list imports task", move |conn| {
        crate::db::vault_imports::list_imports(conn, &account)
    })
    .await?;

    Ok(Json(serde_json::json!({ "imports": imports })))
}

async fn imports_get_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let db = Arc::clone(&state.db);
    let detail = tokio::task::spawn_blocking(move || {
        let conn = lock_conn(&db)?;
        crate::db::vault_imports::get_import_detail(&conn, &auth.account_id, import_id)
    })
    .await
    .join_map("import detail task", ApiError::from)?;

    Ok(Json(import_detail_response(detail)))
}

/// `GET /v1/account/storage` — attachment usage + top 100 largest attachments.
async fn account_storage_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    let account_id = auth.account_id;
    let db = Arc::clone(&state.db);
    let result = with_locked_conn(db, "account storage task", move |conn| {
        let total_bytes = crate::db::vault_imports::account_attachment_bytes(conn, &account_id)?;
        let attachment_count =
            crate::db::vault_imports::account_attachment_count(conn, &account_id)?;
        let top_attachments =
            crate::db::vault_imports::top_attachments_by_size(conn, &account_id, 100)?;
        Ok::<_, anyhow::Error>(serde_json::json!({
            "total_bytes": total_bytes,
            "attachment_count": attachment_count,
            "top_attachments": top_attachments,
        }))
    })
    .await?;

    Ok(Json(result))
}

async fn imports_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateImportBody>,
) -> Result<Json<CreateImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    if body.source.trim().is_empty() {
        return Err(ApiError::BadRequest("body field source is required".into()));
    }
    validate_source_id(&body.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    ImportMode::parse(&body.mode).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let account =
        resolve_import_account(&auth, body.account.as_deref(), &state.cfg.paths.db).await?;

    let db = Arc::clone(&state.db);
    let source = body.source.clone();
    let mode = body.mode.clone();
    let tool = body.tool.clone();
    let id = tokio::task::spawn_blocking(move || {
        let conn = lock_import_conn(&db)?;
        crate::db::account_profile::ensure_account_row(&conn, &account)?;
        crate::db::vault_imports::start_import(&conn, &account, &source, &mode, tool.as_deref())
    })
    .await
    .join_blocking("create import task failed")?;

    Ok(Json(CreateImportResponse { ok: true, id }))
}

async fn imports_complete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
    Json(body): Json<CompleteImportBody>,
) -> Result<Json<CompleteImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account = resolve_import_account(&auth, None, &state.cfg.paths.db).await?;
    validate_complete_import_issues(&body.issues)?;
    let db = Arc::clone(&state.db);
    let summary_json = match body.summary {
        Some(summary) => Some(
            serde_json::to_string(&summary)
                .map_err(|e| ApiError::Internal(format!("serialize import summary: {e}")))?,
        ),
        None => None,
    };
    let args = crate::db::vault_imports::CompleteImportArgs {
        ok: body.ok,
        message_count: body.message_count,
        attachment_count: body.attachment_count,
        bytes_uploaded: body.bytes_uploaded,
        duration_ms: body.duration_ms,
        parse_ms: body.parse_ms,
        convert_ms: body.convert_ms,
        upload_ms: body.upload_ms,
        summary_json,
        issues: body
            .issues
            .into_iter()
            .map(|issue| crate::db::vault_imports::ImportIssueInput {
                kind: issue.kind,
                step: issue.step,
                item: issue.item,
                reason: issue.reason,
            })
            .collect(),
    };
    let row = tokio::task::spawn_blocking(move || {
        let conn = lock_import_conn(&db)?;
        crate::db::vault_imports::complete_import(&conn, &account, import_id, &args)
    })
    .await
    .join_map("complete import task failed", |e| {
        match e.downcast::<crate::db::vault_imports::ImportLookupError>() {
            Ok(lookup) => ApiError::from(lookup),
            Err(other) => ApiError::Internal(other.to_string()),
        }
    })?;

    Ok(Json(CompleteImportResponse {
        ok: true,
        id: row.id,
        status: row.status,
        message_count: row.message_count,
        attachment_count: row.attachment_count,
        bytes_uploaded: row.bytes_uploaded,
    }))
}

fn parse_summary_json(summary_json: Option<String>) -> serde_json::Value {
    match summary_json {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::Value::String(raw)),
        None => serde_json::Value::Null,
    }
}

fn import_detail_response(detail: crate::db::vault_imports::ImportDetail) -> serde_json::Value {
    let row = detail.row;
    let issues = detail
        .issues
        .into_iter()
        .map(|issue| {
            serde_json::json!({
                "kind": issue.kind,
                "step": issue.step,
                "item": issue.item,
                "reason": issue.reason,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "id": row.id,
        "source": row.source,
        "tool": row.tool,
        "mode": row.mode,
        "status": row.status,
        "started_at": row.started_at,
        "finished_at": row.finished_at,
        "message_count": row.message_count,
        "attachment_count": row.attachment_count,
        "bytes_uploaded": row.bytes_uploaded,
        "duration_ms": row.duration_ms,
        "parse_ms": row.parse_ms,
        "convert_ms": row.convert_ms,
        "upload_ms": row.upload_ms,
        "summary": parse_summary_json(row.summary_json),
        "issues": issues,
    })
}

async fn import_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut query): Query<ImportQuery>,
    request: Request,
) -> Result<Json<ImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;

    let Some(ct) = content_type_base(&headers) else {
        return Err(ApiError::BadRequest(
            "Content-Type required (application/jsonl or multipart/form-data)".into(),
        ));
    };

    if query.source.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "query param source is required".into(),
        ));
    }
    validate_source_id(&query.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    query.account = Some(account);

    if is_multipart_content_type(ct) {
        let multipart = Multipart::from_request(request, &state)
            .await
            .map_err(|e| ApiError::BadRequest(format!("invalid multipart body: {e}")))?;
        return import_multipart(state, query, multipart).await;
    }

    if is_jsonl_content_type(ct) {
        let temp = tempfile::tempdir().map_err(|e| ApiError::Internal(format!("temp dir: {e}")))?;
        let jsonl_path = temp.path().join("_import.jsonl");
        let n = stream_body_to_file(request.into_body(), &jsonl_path, state.max_body_bytes).await?;
        if n == 0 {
            return Err(ApiError::BadRequest("request body is empty".into()));
        }
        let response = run_import_path(state, query, jsonl_path, None).await;
        drop(temp);
        return response;
    }

    Err(ApiError::BadRequest(
        "Content-Type must be application/jsonl or multipart/form-data".into(),
    ))
}

#[derive(Debug, Deserialize)]
struct AssetPutQuery {
    source: String,
    #[serde(default)]
    account: Option<String>,
}

#[derive(Debug, Serialize)]
struct AssetPutResponse {
    ok: bool,
    sha256: String,
    assets_path: String,
    already_present: bool,
}

impl AssetPutResponse {
    fn stored(asset: assets::StoredAsset, already_present: bool) -> Json<Self> {
        Json(Self {
            ok: true,
            sha256: asset.sha256,
            assets_path: asset.assets_path,
            already_present,
        })
    }
}

enum AssetAccess {
    /// GET asset bytes — needs export (or full session).
    Read,
    /// PUT / multipart upload — needs import (or full session).
    Write,
    /// HEAD probe — import or export.
    Probe,
}

async fn resolve_asset_lookup(
    state: &AppState,
    headers: &HeaderMap,
    sha256: &str,
    query: &AssetPutQuery,
    access: AssetAccess,
) -> Result<(String, String, Option<assets::StoredAsset>), ApiError> {
    let auth = resolve_auth(headers, state).await?;
    match access {
        AssetAccess::Read => require_export_access(&auth)?,
        AssetAccess::Write => require_import_access(&auth)?,
        AssetAccess::Probe => require_import_or_export_access(&auth)?,
    }
    if query.source.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "query param source is required".into(),
        ));
    }
    validate_source_id(&query.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    let source_id = query.source.clone();

    let cfg = Arc::clone(&state.cfg);
    let sha_lookup = sha256.to_string();
    let account_lookup = account.clone();
    let source_lookup = source_id.clone();
    let existing = tokio::task::spawn_blocking(move || {
        let assets_dir = cfg
            .paths
            .assets_dir_for_account(&account_lookup, &source_lookup);
        assets::lookup_by_sha256(&assets_dir, &sha_lookup)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("asset lookup task: {e}")))?;
    Ok((account, source_id, existing))
}

/// Probe whether a content-addressed asset is already stored (no body).
async fn asset_head_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
    Query(query): Query<AssetPutQuery>,
) -> Result<Json<AssetPutResponse>, ApiError> {
    let (_account, _source_id, existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Probe).await?;
    let Some(stored) = existing else {
        return Err(ApiError::NotFound("asset not found".into()));
    };
    Ok(AssetPutResponse::stored(stored, true))
}

/// Download a previously stored content-addressed asset (read-only).
async fn asset_get_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
    Query(query): Query<AssetPutQuery>,
) -> Result<Response, ApiError> {
    let (account, source_id, existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Read).await?;
    let Some(stored) = existing else {
        return Err(ApiError::NotFound("asset not found".into()));
    };

    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let path = assets_dir.join(&stored.assets_path);
    // Reject symlinks / missing files before streaming.
    let meta = tokio::fs::symlink_metadata(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ApiError::NotFound("asset file missing on disk".into())
        } else {
            ApiError::Internal(format!("stat {}: {e}", path.display()))
        }
    })?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(ApiError::NotFound("asset file missing on disk".into()));
    }

    let mime = stored
        .mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".into());
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| ApiError::Internal(format!("open {}: {e}", path.display())))?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    let headers_mut = response.headers_mut();
    if let Ok(value) = header::HeaderValue::from_str(&mime) {
        headers_mut.insert(header::CONTENT_TYPE, value);
    }
    headers_mut.insert(
        header::HeaderName::from_static("x-content-type-options"),
        header::HeaderValue::from_static("nosniff"),
    );
    // Force download-ish disposition with a fixed safe name (never echo client paths).
    headers_mut.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment; filename=\"asset\""),
    );
    if meta.len() > 0 {
        headers_mut.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from(meta.len()),
        );
    }
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ExportMessagesQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExportMessagesCountQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

async fn export_messages_count_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportMessagesCountQuery>,
) -> Result<Json<export_api::ExportCountResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_export_access(&auth)?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    let q = query.q.clone();
    let source = query.source.clone();

    let body = with_configured_db_map(&state.cfg.paths.db, "export count task", move |conn| {
        export_api::export_message_count(
            conn,
            ExportCountOpts {
                account_id: &account,
                query: &q,
                source_override: source.as_deref(),
            },
        )
    })
    .await?;
    Ok(Json(body))
}

async fn export_messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportMessagesQuery>,
) -> Result<Json<export_api::ExportMessagesResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_export_access(&auth)?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    let limit = query.limit.unwrap_or(DEFAULT_EXPORT_LIMIT);
    let offset = query.offset;
    let q = query.q.clone();
    let cursor = query.cursor.clone();
    let source = query.source.clone();

    let body = with_configured_db_map(&state.cfg.paths.db, "export task", move |conn| {
        export_api::export_messages(
            conn,
            ExportPageOpts {
                account_id: &account,
                query: &q,
                limit,
                offset,
                cursor: cursor.as_deref(),
                source_override: source.as_deref(),
            },
        )
    })
    .await?;
    Ok(Json(body))
}

async fn asset_put_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
    Query(query): Query<AssetPutQuery>,
    request: Request,
) -> Result<Json<AssetPutResponse>, ApiError> {
    let (account, source_id, existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Write).await?;

    let mime = content_type_base(&headers)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("application/octet-stream"))
        .map(str::to_string);

    if let Some(stored) = existing {
        discard_body(request.into_body(), state.max_body_bytes).await?;
        return Ok(AssetPutResponse::stored(stored, true));
    }

    // Write the upload into the account assets tree so verify can rename into place
    // instead of copying across filesystems (tempfile often lives on another mount).
    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let incoming_dir = assets_dir.join(".incoming");
    tokio::fs::create_dir_all(&incoming_dir)
        .await
        .map_err(|e| ApiError::Internal(format!("mkdir {}: {e}", incoming_dir.display())))?;
    let tmp_path = incoming_dir.join(format!(
        "{sha256}-{}.part",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let n = match stream_body_to_file(request.into_body(), &tmp_path, state.max_body_bytes).await {
        Ok(n) => n,
        Err(err) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(err);
        }
    };
    if n == 0 {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(ApiError::BadRequest("request body is empty".into()));
    }

    let sha = sha256.clone();
    let tmp_for_store = tmp_path.clone();
    let assets_dir_store = assets_dir.clone();
    let (stored, already_present) = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&assets_dir_store)?;
        assets::store_verified(
            &tmp_for_store,
            &sha,
            &assets_dir_store,
            mime.as_deref(),
            true,
            false,
        )
    })
    .await
    .join_map("asset upload task", |e| ApiError::BadRequest(e.to_string()))?;

    // Rename consumes the temp file; remove leftovers after errors / already_present races.
    let _ = tokio::fs::remove_file(&tmp_path).await;
    Ok(AssetPutResponse::stored(stored, already_present))
}

#[derive(Debug, Deserialize)]
struct AssetUploadStartBody {
    bytes: u64,
    #[serde(default)]
    mime: Option<String>,
}

#[derive(Debug, Serialize)]
struct AssetUploadStartResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    upload_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    part_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets_path: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    already_present: bool,
}

#[derive(Debug, Serialize)]
struct AssetUploadPartResponse {
    ok: bool,
    part: u32,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct AssetUploadAbortResponse {
    ok: bool,
}

async fn asset_upload_start_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
    Query(query): Query<AssetPutQuery>,
    Json(body): Json<AssetUploadStartBody>,
) -> Result<Json<AssetUploadStartResponse>, ApiError> {
    let (account, source_id, _existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Write).await?;
    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let mime = body.mime.clone();
    let bytes = body.bytes;
    let sha = sha256.clone();
    let limits = state.upload_limits;
    let result = tokio::task::spawn_blocking(move || {
        asset_uploads::start_upload(&assets_dir, &sha, bytes, mime.as_deref(), limits)
    })
    .await
    .join_map("upload start task", |e| ApiError::BadRequest(e.to_string()))?;

    match result {
        (Some(stored), None) => Ok(Json(AssetUploadStartResponse {
            ok: true,
            upload_id: None,
            part_size: None,
            sha256: Some(stored.sha256),
            assets_path: Some(stored.assets_path),
            already_present: true,
        })),
        (None, Some(start)) => Ok(Json(AssetUploadStartResponse {
            ok: true,
            upload_id: Some(start.upload_id),
            part_size: Some(start.part_size),
            sha256: None,
            assets_path: None,
            already_present: false,
        })),
        _ => Err(ApiError::Internal(
            "upload start returned inconsistent state".into(),
        )),
    }
}

async fn asset_upload_part_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((sha256, upload_id, part)): AxumPath<(String, String, u32)>,
    Query(query): Query<AssetPutQuery>,
    request: Request,
) -> Result<Json<AssetUploadPartResponse>, ApiError> {
    let (account, source_id, _existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Write).await?;
    if part == 0 {
        return Err(ApiError::BadRequest("part number must be >= 1".into()));
    }
    let body = read_body_limited(request.into_body(), state.upload_limits.part_size).await?;
    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let sha = sha256.clone();
    let uid = upload_id.clone();
    let written = tokio::task::spawn_blocking(move || {
        asset_uploads::put_part(&assets_dir, &sha, &uid, part, &body)
    })
    .await
    .join_map("upload part task", |e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(AssetUploadPartResponse {
        ok: true,
        part,
        bytes: written,
    }))
}

async fn asset_upload_complete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((sha256, upload_id)): AxumPath<(String, String)>,
    Query(query): Query<AssetPutQuery>,
) -> Result<Json<AssetPutResponse>, ApiError> {
    let (account, source_id, existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Write).await?;
    if let Some(stored) = existing {
        // Drop staging if a concurrent single-PUT won the race.
        let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
        let sha = sha256.clone();
        let uid = upload_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            asset_uploads::abort_upload(&assets_dir, &sha, &uid)
        })
        .await;
        return Ok(AssetPutResponse::stored(stored, true));
    }

    let lock_key = format!("{account}:{sha256}");
    let complete_lock = {
        let mut map = state.asset_complete_locks.lock().await;
        map.entry(lock_key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = complete_lock.lock().await;

    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let sha = sha256.clone();
    let uid = upload_id.clone();
    let limits = state.upload_limits;
    let (stored, already_present) = tokio::task::spawn_blocking(move || {
        asset_uploads::complete_upload(&assets_dir, &sha, &uid, limits)
    })
    .await
    .join_map("upload complete task", |e| {
        ApiError::BadRequest(e.to_string())
    })?;

    Ok(AssetPutResponse::stored(stored, already_present))
}

async fn asset_upload_abort_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((sha256, upload_id)): AxumPath<(String, String)>,
    Query(query): Query<AssetPutQuery>,
) -> Result<Json<AssetUploadAbortResponse>, ApiError> {
    let (account, source_id, _existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query, AssetAccess::Write).await?;
    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let sha = sha256.clone();
    let uid = upload_id.clone();
    tokio::task::spawn_blocking(move || asset_uploads::abort_upload(&assets_dir, &sha, &uid))
        .await
        .join_map("upload abort task", |e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(AssetUploadAbortResponse { ok: true }))
}

async fn read_body_limited(body: axum::body::Body, max_bytes: usize) -> Result<Vec<u8>, ApiError> {
    let mut out = Vec::new();
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ApiError::BadRequest(format!("failed to read body: {e}")))?;
        if out.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ApiError::BadRequest("request body too large".into()));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Drain request body without retaining it (used when asset already exists).
async fn discard_body(body: axum::body::Body, max_body_bytes: usize) -> Result<(), ApiError> {
    let mut stream = body.into_data_stream();
    let mut seen = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ApiError::BadRequest(format!("failed to read body: {e}")))?;
        seen = seen.saturating_add(chunk.len());
        if seen > max_body_bytes {
            return Err(ApiError::BadRequest("request body too large".into()));
        }
    }
    Ok(())
}

async fn create_dest_file(dest: &Path) -> Result<tokio::fs::File, ApiError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("mkdir {}: {e}", parent.display())))?;
    }
    tokio::fs::File::create(dest)
        .await
        .map_err(|e| ApiError::Internal(format!("create {}: {e}", dest.display())))
}

async fn stream_body_to_file(
    body: axum::body::Body,
    dest: &Path,
    max_body_bytes: usize,
) -> Result<u64, ApiError> {
    let mut file = create_dest_file(dest).await?;
    let mut written = 0u64;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ApiError::BadRequest(format!("failed to read body: {e}")))?;
        written = written.saturating_add(chunk.len() as u64);
        if written > max_body_bytes as u64 {
            return Err(ApiError::BadRequest("request body too large".into()));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| ApiError::Internal(format!("write {}: {e}", dest.display())))?;
    }
    file.flush()
        .await
        .map_err(|e| ApiError::Internal(format!("flush {}: {e}", dest.display())))?;
    Ok(written)
}

async fn stream_field_to_file(
    mut field: axum::extract::multipart::Field<'_>,
    dest: &Path,
) -> Result<u64, ApiError> {
    let mut file = create_dest_file(dest).await?;
    let mut written = 0u64;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart chunk: {e}")))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| ApiError::Internal(format!("write {}: {e}", dest.display())))?;
        written += chunk.len() as u64;
    }
    file.flush()
        .await
        .map_err(|e| ApiError::Internal(format!("flush {}: {e}", dest.display())))?;
    Ok(written)
}

async fn import_multipart(
    state: AppState,
    query: ImportQuery,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, ApiError> {
    let temp = tempfile::tempdir().map_err(|e| ApiError::Internal(format!("temp dir: {e}")))?;
    let asset_root = temp.path().to_path_buf();
    let jsonl_path = asset_root.join("_import.jsonl");
    let mut have_jsonl = false;
    let mut file_count = 0u64;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart field error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "jsonl" => {
                let n = stream_field_to_file(field, &jsonl_path).await?;
                if n == 0 {
                    return Err(ApiError::BadRequest("jsonl part is empty".into()));
                }
                have_jsonl = true;
            }
            "file" => {
                let filename = field
                    .file_name()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ApiError::BadRequest(
                            "file part missing filename (use relative path e.g. attachments/a.jpg)"
                                .into(),
                        )
                    })?;
                let rel = safe_rel_path(&filename)?;
                let dest = asset_root.join(&rel);
                stream_field_to_file(field, &dest).await?;
                file_count += 1;
            }
            other => {
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("multipart chunk: {e}")))?
                {
                    let _ = chunk;
                }
                eprintln!("import: ignoring unknown multipart field {other:?}");
            }
        }
    }

    if !have_jsonl {
        return Err(ApiError::BadRequest(
            "multipart missing required field 'jsonl'".into(),
        ));
    }
    eprintln!("import: multipart jsonl + {file_count} file(s)");

    let response = run_import_path(state, query, jsonl_path, Some(asset_root)).await;
    drop(temp);
    response
}

async fn run_import_path(
    state: AppState,
    query: ImportQuery,
    jsonl_path: PathBuf,
    asset_root_override: Option<PathBuf>,
) -> Result<Json<ImportResponse>, ApiError> {
    let mode = ImportMode::parse(&query.mode).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    validate_source_id(&query.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let contact_name_mode = import::ContactNameMode::parse(&query.contact_name_mode)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let cfg = Arc::clone(&state.cfg);
    let db = Arc::clone(&state.db);
    let account = query
        .account
        .clone()
        .ok_or_else(|| ApiError::BadRequest("account is required".into()))?;
    let source_id = query.source.clone();
    let do_dedupe = query.dedupe;
    let query_import_id = query.import_id;

    let account_lock = {
        let mut map = state.account_import_locks.lock().await;
        map.entry(account.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = account_lock.lock().await;

    // Validate client-owned sessions before staging work so bad ids return 400.
    if let Some(id) = query_import_id {
        let db = Arc::clone(&db);
        let account_check = account.clone();
        let source_check = source_id.clone();
        let mode_check = mode.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = lock_import_conn(&db)?;
            crate::db::vault_imports::require_reusable_import(
                &conn,
                &account_check,
                id,
                &source_check,
                &mode_check,
            )
            .map_err(anyhow::Error::new)?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .join_map("import session check", |e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                ApiError::NotFound(msg)
            } else if msg.contains("not running") || msg.contains("mismatch") {
                ApiError::BadRequest(msg)
            } else {
                ApiError::Internal(msg)
            }
        })?;
    }

    let result = tokio::task::spawn_blocking(move || {
        let assets_dir = cfg.paths.assets_dir_for_account(&account, &source_id);
        // Raw body imports resolve attachment paths only via pre-uploaded sha256 assets.
        // Multipart supplies a temp asset_root for relative file parts.
        let asset_root_owned = asset_root_override.unwrap_or_else(|| assets_dir.clone());

        // Client session (vault-push): ownership/status already checked above.
        // Otherwise start a one-shot vault_imports row so Storage history works for curl / single POSTs.
        let (import_id, owns_session) = if let Some(id) = query_import_id {
            (Some(id), false)
        } else {
            let conn = lock_import_conn(&db)?;
            crate::db::account_profile::ensure_account_row(&conn, &account)?;
            let id = crate::db::vault_imports::start_import(
                &conn,
                &account,
                &source_id,
                mode.as_str(),
                Some("http"),
            )?;
            drop(conn);
            (Some(id), true)
        };

        let mut opts = ImportOptions::fixed(
            &cfg.paths.db,
            &assets_dir,
            &asset_root_owned,
            None,
            false,
            mode,
            &source_id,
            &account,
            do_dedupe,
            import_id,
        );
        opts.contact_name_mode = contact_name_mode;
        // Dedicated connection for the long import so we do not hold `state.db`
        // across JSONL / asset IO / promote (export and session SQL stay free).
        let mut conn = schema::open_configured(&cfg.paths.db)
            .with_context(|| format!("open import database {}", cfg.paths.db.display()))?;
        let import_result = import::import_jsonl_files_on_conn(
            &mut conn,
            &[jsonl_path],
            &opts,
            import::ImportSchemaMode::AssumeReady,
        );

        if owns_session && let Some(id) = import_id {
            let complete_args = match &import_result {
                Ok(stats) => crate::db::vault_imports::CompleteImportArgs::succeeded(
                    stats.messages,
                    stats.attachments,
                ),
                Err(_) => crate::db::vault_imports::CompleteImportArgs::failed(),
            };
            crate::db::vault_imports::complete_import_or_warn(&conn, &account, id, &complete_args);
        }
        let stats = import_result?;
        drop(conn);
        let dedupe_stats = if do_dedupe {
            Some(dedupe::run_dedupe(&cfg.paths.db, &account, 2)?)
        } else {
            None
        };
        Ok::<_, anyhow::Error>((stats, dedupe_stats, source_id, account))
    })
    .await
    .join_blocking("import task failed")?;

    let (stats, dedupe_stats, source_id, account) = result;
    Ok(Json(ImportResponse {
        ok: true,
        source: source_id,
        account,
        stats,
        dedupe: dedupe_stats.map(|d| DedupeResponse {
            keys_filled: d.keys_filled,
            exact_groups: d.exact_groups,
            exact_flagged: d.exact_flagged,
            near_flagged: d.near_flagged,
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::sync::{Arc, Mutex as StdMutex};
    use tempfile::TempDir;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    fn test_state() -> (TempDir, AppState, String, i64) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("vault.db");
        let data_dir = tmp.path().join("data");
        let conn = Connection::open(&db_path).unwrap();
        schema::configure_connection(&conn).unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        schema::ensure_accounts_schema(&conn).unwrap();
        crate::db::account_profile::ensure_account_row(&conn, TEST_ACCOUNT).unwrap();
        let token =
            crate::db::session_tokens::insert_account_session_token(&conn, TEST_ACCOUNT).unwrap();
        let import_id = crate::db::vault_imports::start_import(
            &conn,
            TEST_ACCOUNT,
            "ios",
            "append",
            Some("message-vault-server"),
        )
        .unwrap();

        let state = AppState {
            cfg: Arc::new(crate::config::Config {
                paths: crate::config::PathsConfig {
                    db: db_path,
                    data_dir,
                    assets_dir: "assets".into(),
                    assets_converted_dir: "assets_converted".into(),
                },
                server: Some(crate::config::ServerConfig {
                    bind: "127.0.0.1:0".into(),
                    asset_max_bytes: 8 * 1024 * 1024,
                    asset_part_size: 1024 * 1024,
                    asset_hash_threshold_bytes: 1024 * 1024,
                    cors_origins: Vec::new(),
                }),
            }),
            db: Arc::new(StdMutex::new(conn)),
            account_import_locks: Arc::new(Mutex::new(HashMap::new())),
            asset_complete_locks: Arc::new(Mutex::new(HashMap::new())),
            upload_limits: asset_uploads::UploadLimits::default(),
            max_body_bytes: asset_uploads::DEFAULT_MAX_BYTES as usize,
        };

        (tmp, state, token, import_id)
    }

    fn auth_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        headers
    }

    #[tokio::test]
    async fn imports_complete_and_detail_surface_timings_and_issues() {
        let (_tmp, state, token, import_id) = test_state();
        let body = CompleteImportBody {
            ok: true,
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: Some(48_000),
            parse_ms: Some(18_000),
            convert_ms: Some(22_000),
            upload_ms: Some(8_000),
            summary: Some(serde_json::json!({
                "parse": { "messages": 10 },
                "convert": { "files": 2 }
            })),
            issues: vec![
                CompleteImportIssueBody {
                    kind: "skip".into(),
                    step: "convert".into(),
                    item: "photo.heic".into(),
                    reason: "convert failed".into(),
                },
                CompleteImportIssueBody {
                    kind: "error".into(),
                    step: "upload".into(),
                    item: "archive.zip".into(),
                    reason: "upload failed".into(),
                },
            ],
        };

        let response = imports_complete_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(body),
        )
        .await
        .unwrap();
        assert_eq!(response.0.status, "completed");
        assert_eq!(response.0.message_count, 10);
        assert_eq!(response.0.attachment_count, 2);
        assert_eq!(response.0.bytes_uploaded, 100);

        let detail = imports_get_handler(State(state), auth_headers(&token), AxumPath(import_id))
            .await
            .unwrap();
        let value = detail.0;
        assert_eq!(value["id"], import_id);
        assert_eq!(value["duration_ms"], 48_000);
        assert_eq!(value["parse_ms"], 18_000);
        assert_eq!(value["convert_ms"], 22_000);
        assert_eq!(value["upload_ms"], 8_000);
        assert_eq!(value["summary"]["parse"]["messages"], 10);
        assert_eq!(value["issues"].as_array().unwrap().len(), 2);
        assert_eq!(value["issues"][0]["kind"], "skip");
        assert_eq!(value["issues"][0]["step"], "convert");
        assert_eq!(value["issues"][1]["kind"], "error");
        assert_eq!(value["issues"][1]["step"], "upload");
    }

    #[tokio::test]
    async fn imports_complete_rejects_invalid_issue_kind_before_db_write() {
        let (_tmp, state, token, import_id) = test_state();
        let body = CompleteImportBody {
            ok: true,
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: Some(48_000),
            parse_ms: Some(18_000),
            convert_ms: Some(22_000),
            upload_ms: Some(8_000),
            summary: None,
            issues: vec![CompleteImportIssueBody {
                kind: "warning".into(),
                step: "upload".into(),
                item: "archive.zip".into(),
                reason: "not allowed".into(),
            }],
        };

        let err = imports_complete_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(body),
        )
        .await
        .unwrap_err();

        match err {
            ApiError::BadRequest(msg) => {
                assert!(msg.contains("invalid import issue kind"));
            }
            other => panic!("expected bad request, got {other:?}"),
        }

        let status: String = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM vault_imports WHERE id = ?1",
                params![import_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "running");
    }

    #[tokio::test]
    async fn imports_get_handler_returns_not_found_for_missing_import() {
        let (_tmp, state, token, import_id) = test_state();
        let err = imports_get_handler(State(state), auth_headers(&token), AxumPath(import_id + 1))
            .await
            .unwrap_err();

        match err {
            ApiError::NotFound(msg) => {
                assert!(msg.contains("import"));
                assert!(msg.contains("not found"));
            }
            other => panic!("expected not found, got {other:?}"),
        }
    }
}
