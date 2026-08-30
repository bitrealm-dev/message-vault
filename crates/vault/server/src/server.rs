//! Router assembly, shared state, auth resolution, and HTTP plumbing.
//!
//! Domain handlers live in their own modules: `auth` (login and session),
//! `profile` (account settings), `contacts_api`, `conversations_api`,
//! `export_api` (messages and counts), `import` (JSONL ingest and import
//! sessions), and `assets` (asset bytes and multipart uploads). This module
//! keeps the pieces they share: [`AppState`], [`ApiError`], Bearer token
//! resolution, body-streaming helpers, and `http_app`, which assembles the
//! router.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::ServeDir;

use crate::asset_uploads;
use crate::config::Config;
use crate::db::account_profile;
use crate::db::api_tokens;
use crate::db::engine::{self, DbEngine};
use crate::db::permissions::Permissions;
use crate::db::schema;
use crate::db::session_tokens;
use crate::export_api::ExportQueryError;

/// What a Bearer credential is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCapability {
    /// Signed-in session. Carries the account's own permissions.
    Session {
        /// The account may manage users.
        is_admin: bool,
        /// What the account may do.
        permissions: Permissions,
    },
    /// Named API token. Already intersected with its owner's permissions.
    ApiToken(Permissions),
}

/// Authenticated vault account from a session token or named API token.
#[derive(Debug, Clone)]
pub struct AuthIdentity {
    /// The authenticated vault account.
    pub account_id: String,
    /// What this credential is allowed to do.
    pub capability: AuthCapability,
}

impl AuthIdentity {
    /// What this credential may do, account and token already intersected.
    pub fn permissions(&self) -> Permissions {
        match self.capability {
            AuthCapability::Session { permissions, .. } => permissions,
            AuthCapability::ApiToken(permissions) => permissions,
        }
    }

    /// True only for a signed-in administrator, never for an API token.
    pub fn is_admin(&self) -> bool {
        matches!(
            self.capability,
            AuthCapability::Session { is_admin: true, .. }
        )
    }

    /// True when the credential is a signed-in session rather than a token.
    pub fn is_session(&self) -> bool {
        matches!(self.capability, AuthCapability::Session { .. })
    }
}

/// Reject API tokens on routes that require a GUI session.
///
/// # Errors
///
/// Returns forbidden when the credential is a named API token.
pub fn require_full_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.is_session() {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "this endpoint requires a signed-in session; use an API token only for import/export"
            .into(),
    ))
}

/// Reject anything that is not a signed-in administrator.
///
/// # Errors
///
/// Returns forbidden for ordinary sessions and for every API token.
pub fn require_admin(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.is_admin() {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "this endpoint requires an administrator session".into(),
    ))
}

/// Allow a credential that may import.
///
/// # Errors
///
/// Returns forbidden when import is not permitted.
pub fn require_import_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.permissions().import {
        return Ok(());
    }
    Err(ApiError::Forbidden("import is not permitted".into()))
}

/// Allow a credential that may export.
///
/// # Errors
///
/// Returns forbidden when export is not permitted.
pub fn require_export_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.permissions().export {
        return Ok(());
    }
    Err(ApiError::Forbidden("export is not permitted".into()))
}

/// Allow a credential that may import or export, for asset probes.
///
/// # Errors
///
/// Returns forbidden when neither is permitted.
pub fn require_import_or_export_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    let p = auth.permissions();
    if p.import || p.export {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "this credential cannot access assets".into(),
    ))
}

/// Allow a credential that may destroy message data.
///
/// # Errors
///
/// Returns forbidden when deletion is not permitted.
pub fn require_delete_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.permissions().delete {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "deleting messages is not permitted for this account".into(),
    ))
}

/// Shared server state passed to every HTTP handler.
#[derive(Clone)]
pub struct AppState {
    /// Loaded configuration.
    pub cfg: Arc<Config>,
    /// Connection pool (SQLite file or `[database] url`). Handlers acquire
    /// short-lived connections from here.
    pub db: sqlx::AnyPool,
    /// Engine the pool was opened for (SQLite by default, Postgres via URL).
    pub db_engine: DbEngine,
    /// Per-account import mutex: same-account imports stay serialized so staging
    /// rows (the temporary import area) for that tenant are not wiped mid-run.
    /// Different accounts may overlap at the lock layer; SQLite write-ahead
    /// logging plus busy_timeout serialize writers.
    pub(crate) account_import_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Serialize multipart complete per (account, sha256) so two clients cannot
    /// race `store_verified` on the same SHA-256 fingerprint.
    pub(crate) asset_complete_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Sliding-window hit counts for the unauthenticated auth endpoints. Held
    /// here, not in a static, so tests in one binary cannot rate-limit each
    /// other; a served vault has a single state, so the limit still spans it.
    pub(crate) auth_rate_limits: crate::auth::AuthRateLimits,
    /// Multipart / asset size limits from `[server]` (env may override part size).
    pub(crate) upload_limits: asset_uploads::UploadLimits,
    /// Axum request body cap (single PUT or one part); equals `asset_max_bytes`.
    pub(crate) max_body_bytes: usize,
}

/// API error envelope returned for non-200 responses.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Whether the request succeeded; always `false` for error responses.
    pub ok: bool,
    /// Human-readable description of the failure.
    pub error: String,
}

/// API error returned as a JSON envelope with a matching HTTP status.
#[derive(Debug)]
pub enum ApiError {
    /// `401` — no valid session or API token.
    Unauthorized(String),
    /// `403` — the credential lacks permission for this route.
    Forbidden(String),
    /// `400` — malformed request or invalid parameter.
    BadRequest(String),
    /// `409` — the request conflicts with current state.
    Conflict(String),
    /// `404` — the requested resource does not exist.
    NotFound(String),
    /// `429` — rate limit hit.
    TooManyRequests(String),
    /// `503` — a dependency is temporarily unavailable.
    ServiceUnavailable(String),
    /// `500` — unexpected failure.
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::Conflict(m) => (StatusCode::CONFLICT, m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, m),
            Self::ServiceUnavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
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

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Origins the packaged desktop app runs from. A Tauri window is not a page on
/// the web, so its origin is fixed by the platform rather than chosen by
/// anyone: `tauri://localhost` on Linux and macOS, and `http(s)://tauri.localhost`
/// on Windows.
///
/// These are allowed whatever the config says. A vault built from source starts
/// with `cors_origins` commented out, and the desktop app pointed at it then
/// fails in a way that reads as a network problem — the browser refuses the
/// response before any code can see it, so the app reports the server as
/// unreachable while `curl` to the same port succeeds. That sends people to
/// their firewall for a missing line of TOML.
///
/// Allowing them by default gives away nothing a listed origin does not. The
/// browser sets `Origin` itself and a page on the web cannot claim to be one of
/// these, so this widens what the desktop app can reach, not what a website can.
pub(crate) const PACKAGED_DESKTOP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// Build the Cross-Origin Resource Sharing (CORS) layer from
/// `[server].cors_origins`. CORS is the browser rule that decides which other
/// websites may call this API.
///
/// - `["*"]` → fully permissive (local debugging only)
/// - otherwise → exact origin allow list, always including
///   [`PACKAGED_DESKTOP_ORIGINS`]
///
/// An empty list is therefore not "no CORS" but "the desktop app and nothing
/// else", which is what an unconfigured vault wants: the browser UI it serves
/// itself is same-origin and needs no header at all.
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    if origins.iter().any(|o| o.trim() == "*") {
        return CorsLayer::permissive();
    }
    let mut allowed: Vec<HeaderValue> = Vec::new();
    for origin in origins
        .iter()
        .map(String::as_str)
        .chain(PACKAGED_DESKTOP_ORIGINS.iter().copied())
    {
        let trimmed = origin.trim();
        if trimmed.is_empty() {
            continue;
        }
        // A config that lists a packaged origin by hand is the common case, and
        // naming the same origin twice in the allow list helps no one.
        if let Ok(value) = trimmed.parse::<HeaderValue>()
            && !allowed.contains(&value)
        {
            allowed.push(value);
        }
    }
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
}

fn limited_auth_router() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, spec) = crate::openapi::auth_public_openapi().split_for_parts();
    (
        // Auth JSON is tiny; keep a tight limit so Argon2 abuse cannot ship 512 MiB bodies.
        router.layer(RequestBodyLimitLayer::new(32 * 1024)),
        spec,
    )
}

pub(crate) fn http_app(state: AppState) -> Router {
    let openapi_ui = state
        .cfg
        .server
        .as_ref()
        .map(|s| s.openapi_ui)
        .unwrap_or(false);
    let cors_origins = state
        .cfg
        .server
        .as_ref()
        .map(|s| s.cors_origins.clone())
        .unwrap_or_default();
    let (auth_small, mut spec) = limited_auth_router();
    let (doc_router, rest) = crate::openapi::api_openapi().split_for_parts();
    spec.merge(rest);

    let mut api = Router::new()
        .merge(doc_router)
        .merge(auth_small)
        .fallback_service(ServeDir::new("static"))
        .layer(build_cors_layer(&cors_origins))
        .layer(RequestBodyLimitLayer::new(state.max_body_bytes));

    if openapi_ui {
        api = api.merge(utoipa_swagger_ui::SwaggerUi::new("/docs").url("/openapi.json", spec));
    }

    api.with_state(state)
}

/// Start the HTTP server.
///
/// # Errors
///
/// Returns an error when the database cannot be opened, the operation lock
/// cannot be taken, or the listener cannot bind.
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let server = cfg.require_server()?.clone();
    let bind = server.bind.clone();
    // Production entry points must install the Any drivers once before any pool
    // connect (idempotent; `engine::test_pool` does the same for tests).
    sqlx::any::install_default_drivers();
    let db_url = cfg.database.url.clone();
    let engine = match &db_url {
        Some(url) => engine::detect_engine(url)?,
        None => DbEngine::Sqlite,
    };
    let lock_path = if engine == DbEngine::Sqlite {
        cfg.paths.db.clone()
    } else {
        cfg.paths.data_dir.join(".operation.lock")
    };
    let _operation_lock = crate::operation_lock::acquire_for_serve(&lock_path)?;
    let upload_limits = asset_uploads::UploadLimits::resolve(
        server.asset_part_size,
        server.asset_max_bytes,
        server.asset_hash_threshold_bytes,
    );
    let max_body_bytes = upload_limits.max_bytes as usize;

    // Open the pool, warm it, and ensure schema once before serving.
    let pool = match &db_url {
        Some(url) => engine::open_pool_from_url(url).await?,
        None => engine::open_pool_for_path(&cfg.paths.db).await?,
    };
    {
        let mut conn = pool.acquire().await?;
        let _: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut *conn).await?; // warmup (i32: INT4 on Postgres, INTEGER on SQLite)
        schema::ensure_vault_schema(&mut conn).await?;
    }
    if engine == DbEngine::Sqlite {
        crate::operation_lock::mark_ready(&cfg.paths.db)?;
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| "unknown".into());
        eprintln!("  db:   {} (journal_mode={mode})", cfg.paths.db.display());
    }
    eprintln!(
        "  assets: max={} MiB  part_size={} MiB",
        upload_limits.max_bytes / (1024 * 1024),
        upload_limits.part_size / (1024 * 1024)
    );

    let state = AppState {
        cfg: Arc::new(cfg),
        db: pool,
        db_engine: engine,
        account_import_locks: Arc::new(Mutex::new(HashMap::new())),
        asset_complete_locks: Arc::new(Mutex::new(HashMap::new())),
        auth_rate_limits: Arc::new(std::sync::Mutex::new(HashMap::new())),
        upload_limits,
        max_body_bytes,
    };

    let app = http_app(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("message-vault-server serve listening on http://{bind}");
    eprintln!("  GET  /health");
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

/// Report process liveness.
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses((status = 200, description = "Process is up", body = String))
)]
pub(crate) async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok\n")
}

async fn resolve_account_ref_async(
    pool: &sqlx::AnyPool,
    account_ref: &str,
) -> Result<String, ApiError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    account_profile::resolve_account_ref(&mut conn, account_ref)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// Read the Bearer token from `Authorization`.
///
/// # Errors
///
/// Returns unauthorized when the header is missing or not a Bearer value.
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

/// Resolve a session token or named API token to an account.
///
/// # Errors
///
/// Returns unauthorized when the token is missing or invalid.
pub async fn resolve_auth(headers: &HeaderMap, state: &AppState) -> Result<AuthIdentity, ApiError> {
    let token = bearer_token(headers)?;
    // Always look up against SQLite so rotate/delete in Settings takes effect
    // without restarting serve (no process-local token cache).
    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    resolve_auth_on_conn(&mut conn, &token).await
}

/// Resolve a Bearer credential on an existing connection.
///
/// # Errors
///
/// Unauthorized when the token matches nothing; forbidden when the account is
/// disabled.
pub async fn resolve_auth_on_conn(
    conn: &mut AnyConnection,
    token: &str,
) -> Result<AuthIdentity, ApiError> {
    schema::ensure_accounts_schema(conn)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Credential-specific bit not yet folded into `AuthCapability`: a session
    // carries no extra state, an API token carries its own (pre-intersection)
    // permissions. Both branches load `AccountAuth` the same way so the
    // disabled check below runs exactly once, regardless of credential kind.
    enum Credential {
        Session,
        ApiToken(Permissions),
    }

    let resolved = if let Some(account_id) =
        session_tokens::lookup_account_for_token(&mut *conn, token)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some((account_id, Credential::Session))
    } else {
        api_tokens::lookup_account_for_api_token(&mut *conn, token)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map(|tok| (tok.account_id, Credential::ApiToken(tok.permissions)))
    };

    let Some((account_id, credential)) = resolved else {
        return Err(ApiError::Unauthorized("invalid API token".into()));
    };

    let auth = account_profile::load_account_auth(&mut *conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;
    if auth.disabled {
        return Err(ApiError::Forbidden("this account is disabled".into()));
    }

    let capability = match credential {
        Credential::Session => AuthCapability::Session {
            is_admin: auth.is_admin,
            permissions: auth.permissions,
        },
        Credential::ApiToken(tok_permissions) => {
            AuthCapability::ApiToken(auth.permissions.intersect(tok_permissions))
        }
    };

    Ok(AuthIdentity {
        account_id,
        capability,
    })
}

/// Resolve the account id for an import or export: Bearer token binds the account.
/// Optional query may be username or UUID and must match the token.
pub(crate) async fn resolve_import_account(
    auth: &AuthIdentity,
    query_account: Option<&str>,
    pool: &sqlx::AnyPool,
) -> Result<String, ApiError> {
    let query = nonempty_query_account(query_account);
    if let Some(q) = query {
        let resolved = resolve_account_ref_async(pool, q).await?;
        if resolved != auth.account_id {
            return Err(ApiError::Forbidden(
                "account query does not match token's account".into(),
            ));
        }
    }
    Ok(auth.account_id.clone())
}

pub(crate) fn nonempty_query_account(value: Option<&str>) -> Option<&str> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(crate) fn content_type_base(headers: &HeaderMap) -> Option<&str> {
    let ct = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    Some(ct.split(';').next().unwrap_or(ct).trim())
}

pub(crate) fn upload_content_type(headers: &HeaderMap) -> Option<String> {
    let base = content_type_base(headers)?;
    if base.is_empty() || base.eq_ignore_ascii_case("application/octet-stream") {
        None
    } else {
        Some(base.to_string())
    }
}

/// True when the request body is JSON Lines (one JSON object per line).
pub(crate) fn is_jsonl_content_type(base: &str) -> bool {
    base.eq_ignore_ascii_case("application/jsonl")
        || base.eq_ignore_ascii_case("application/x-ndjson")
}

pub(crate) fn is_multipart_content_type(base: &str) -> bool {
    base.eq_ignore_ascii_case("multipart/form-data")
}

/// Reject path traversal; allow only relative Normal/CurDir components.
pub(crate) fn safe_rel_path(name: &str) -> Result<PathBuf, ApiError> {
    crate::config::safe_rel_path(name).map_err(|e| ApiError::BadRequest(e.to_string()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListPageQuery {
    #[serde(default)]
    pub(crate) q: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
}

/// Number of memberships changed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MembershipChangedResponse {
    pub(crate) changed: u64,
}

pub(crate) async fn read_body_limited(
    body: axum::body::Body,
    max_bytes: usize,
) -> Result<Vec<u8>, ApiError> {
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
pub(crate) async fn discard_body(
    body: axum::body::Body,
    max_body_bytes: usize,
) -> Result<(), ApiError> {
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

pub(crate) async fn stream_body_to_file(
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

pub(crate) async fn stream_field_to_file(
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

/// Build the `AppState` every test in this crate drives: a real `Config`
/// rooted at `data_dir` (with a sibling `vault.db` path that nothing in the
/// test suite reads from disk — queries go through `pool`), the given pool,
/// and default upload limits. `#[cfg(test)]`-gated so it never ships in a
/// release build; `pub(crate)` so `test_support` and the other test modules
/// in this crate can reach it.
#[cfg(test)]
pub(crate) async fn test_app_state(pool: sqlx::AnyPool, data_dir: &Path) -> AppState {
    AppState {
        cfg: Arc::new(crate::config::Config {
            paths: crate::config::PathsConfig {
                db: data_dir.join("vault.db"),
                data_dir: data_dir.to_path_buf(),
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: Some(crate::config::ServerConfig {
                bind: "127.0.0.1:0".into(),
                asset_max_bytes: 8 * 1024 * 1024,
                asset_part_size: 1024 * 1024,
                asset_hash_threshold_bytes: 1024 * 1024,
                cors_origins: Vec::new(),
                openapi_ui: false,
            }),
            database: crate::config::DatabaseConfig::default(),
        }),
        db: pool,
        db_engine: DbEngine::Sqlite,
        account_import_locks: Arc::new(Mutex::new(HashMap::new())),
        asset_complete_locks: Arc::new(Mutex::new(HashMap::new())),
        auth_rate_limits: Arc::new(std::sync::Mutex::new(HashMap::new())),
        upload_limits: asset_uploads::UploadLimits::default(),
        max_body_bytes: asset_uploads::DEFAULT_MAX_BYTES as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{
        CompleteImportBody, CompleteImportIssueBody, CreateImportBody, SetImportStageBody,
        imports_active_handler, imports_complete_handler, imports_create_handler,
        imports_discard_handler, imports_get_handler, imports_stage_handler,
    };
    use axum::extract::{Path as AxumPath, State};
    use tempfile::TempDir;

    fn auth_public_router() -> Router<AppState> {
        limited_auth_router().0
    }

    #[test]
    fn jsonl_content_type_accepts_x_ndjson() {
        assert!(is_jsonl_content_type("application/x-ndjson"));
        assert!(is_jsonl_content_type("application/jsonl"));
        assert!(is_jsonl_content_type("Application/X-NDJSON"));
        assert!(!is_jsonl_content_type("multipart/form-data"));
        assert!(!is_jsonl_content_type("application/json"));
    }

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    /// Test database with the vault schema applied. The temp dir is returned
    /// too: dropping it deletes the database file out from under the checked-out
    /// connection, after which SQLite rejects writes with SQLITE_READONLY.
    async fn test_conn() -> (TempDir, sqlx::pool::PoolConnection<sqlx::Any>) {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        (dir, conn)
    }

    #[tokio::test]
    async fn api_token_cannot_exceed_its_owner() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None)
            .await
            .unwrap();
        sqlx::query("UPDATE accounts SET can_import = 0 WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();
        let created =
            api_tokens::create_api_token(&mut conn, TEST_ACCOUNT, "tool", Permissions::all(), None)
                .await
                .unwrap();

        let identity = resolve_auth_on_conn(&mut conn, &created.5).await.unwrap();

        assert!(
            !identity.permissions().import,
            "the account lost import, so its token must not have it"
        );
        assert!(identity.permissions().export);
    }

    #[tokio::test]
    async fn disabling_an_account_kills_its_live_session() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None)
            .await
            .unwrap();
        let token = session_tokens::insert_account_session_token(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();

        // The token works while the account is active.
        resolve_auth_on_conn(&mut conn, &token).await.unwrap();

        sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();

        let err = resolve_auth_on_conn(&mut conn, &token).await.unwrap_err();
        assert!(
            matches!(err, ApiError::Forbidden(_)),
            "a disabled account's existing token must stop working, got {err:?}"
        );
    }

    #[tokio::test]
    async fn disabling_an_account_kills_its_live_api_token() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None)
            .await
            .unwrap();
        let created =
            api_tokens::create_api_token(&mut conn, TEST_ACCOUNT, "tool", Permissions::all(), None)
                .await
                .unwrap();
        let token = created.5;

        // The API token works while the account is active.
        resolve_auth_on_conn(&mut conn, &token).await.unwrap();

        sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();

        let err = resolve_auth_on_conn(&mut conn, &token).await.unwrap_err();
        assert!(
            matches!(err, ApiError::Forbidden(_)),
            "a disabled account's existing API token must stop working, got {err:?}"
        );
    }

    async fn test_state() -> (TempDir, AppState, String, i64) {
        let (pool, tmp) = crate::db::engine::test_pool().await;
        let data_dir = tmp.path().join("data");
        {
            let mut conn = pool.acquire().await.unwrap();
            schema::ensure_vault_schema(&mut conn).await.unwrap();
            schema::ensure_accounts_schema(&mut conn).await.unwrap();
            crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
                .await
                .unwrap();
        }
        let token = crate::db::session_tokens::insert_account_session_token(
            &mut pool.acquire().await.unwrap(),
            TEST_ACCOUNT,
        )
        .await
        .unwrap();
        let import_id = crate::db::vault_imports::start_import(
            &mut pool.acquire().await.unwrap(),
            &crate::db::vault_imports::StartImportArgs {
                account_id: TEST_ACCOUNT,
                source: "ios",
                mode: "append",
                tool: Some("message-vault-server"),
                stage: crate::db::vault_imports::ImportStage::Parse,
                staging_dir: None,
                device_id: None,
                form_json: None,
                source_fingerprint: None,
            },
        )
        .await
        .unwrap();

        let state = test_app_state(pool, &data_dir).await;

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

    async fn get_path(state: AppState, path: &str) -> reqwest::Response {
        let app = http_app(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}{path}"))
            .send()
            .await
            .unwrap();
        server.abort();
        response
    }

    fn with_cors(mut state: AppState, origins: &[&str]) -> AppState {
        let mut cfg = (*state.cfg).clone();
        cfg.server.as_mut().unwrap().cors_origins =
            origins.iter().map(|s| (*s).to_string()).collect();
        state.cfg = Arc::new(cfg);
        state
    }

    async fn cors_preflight(state: AppState, origin: &str) -> reqwest::Response {
        let app = http_app(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let response = reqwest::Client::new()
            .request(reqwest::Method::OPTIONS, format!("http://{address}/health"))
            .header("Origin", origin)
            .header("Access-Control-Request-Method", "GET")
            .header("Access-Control-Request-Headers", "content-type")
            .send()
            .await
            .unwrap();
        server.abort();
        response
    }

    fn allow_origin(response: &reqwest::Response) -> Option<&str> {
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok())
    }

    #[tokio::test]
    async fn cors_preflight_allows_packaged_desktop_and_vite_origins() {
        let (_tmp, state, _token, _import_id) = test_state().await;
        let origins = [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "https://tauri.localhost",
            "http://tauri.localhost",
            "tauri://localhost",
        ];
        for origin in origins {
            let response = cors_preflight(with_cors(state.clone(), &origins), origin).await;
            assert_eq!(
                allow_origin(&response),
                Some(origin),
                "preflight Origin {origin}"
            );
        }
    }

    /// A vault built from source starts with `cors_origins` commented out. The
    /// desktop app still has to reach it, so the packaged origins do not wait
    /// to be configured.
    #[tokio::test]
    async fn cors_preflight_allows_packaged_desktop_without_configuration() {
        let (_tmp, state, _token, _import_id) = test_state().await;
        for origin in PACKAGED_DESKTOP_ORIGINS {
            let response = cors_preflight(with_cors(state.clone(), &[]), origin).await;
            assert_eq!(
                allow_origin(&response),
                Some(*origin),
                "unconfigured preflight Origin {origin}"
            );
        }
    }

    /// Built in does not mean open: everything else still has to be listed.
    #[tokio::test]
    async fn cors_preflight_rejects_unknown_origin_without_configuration() {
        let (_tmp, state, _token, _import_id) = test_state().await;
        let response = cors_preflight(with_cors(state, &[]), "https://evil.example").await;
        assert_eq!(allow_origin(&response), None);
    }

    #[tokio::test]
    async fn cors_preflight_rejects_unknown_origin() {
        let (_tmp, state, _token, _import_id) = test_state().await;
        let response = cors_preflight(
            with_cors(state, &["tauri://localhost"]),
            "https://evil.example",
        )
        .await;
        assert_eq!(allow_origin(&response), None);
    }

    #[tokio::test]
    async fn health_still_ok() {
        let (_tmp, state, _token, _import_id) = test_state().await;
        let response = get_path(state, "/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "ok\n");
    }

    #[tokio::test]
    async fn openapi_ui_off_does_not_serve_spec() {
        let (_tmp, state, _token, _import_id) = test_state().await;
        assert!(!state.cfg.require_server().unwrap().openapi_ui);
        let response = get_path(state, "/openapi.json").await;
        assert_ne!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            "application/json"
        );
    }

    #[tokio::test]
    async fn openapi_ui_on_serves_spec_without_token() {
        let (_tmp, mut state, _token, _import_id) = test_state().await;
        {
            let cfg = Arc::make_mut(&mut state.cfg);
            cfg.server.as_mut().unwrap().openapi_ui = true;
        }
        let response = get_path(state, "/openapi.json").await;
        assert_eq!(response.status(), StatusCode::OK);
        let v: serde_json::Value = response.json().await.unwrap();
        assert!(v["openapi"].as_str().unwrap().starts_with("3."));
    }

    async fn auth_route_status(path: &str) -> StatusCode {
        let (_tmp, state, _token, _import_id) = test_state().await;
        let app = auth_public_router().with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let response = reqwest::Client::new()
            .post(format!("http://{address}{path}"))
            .send()
            .await
            .unwrap();
        server.abort();
        response.status()
    }

    #[tokio::test]
    async fn try_demo_route_is_gone() {
        // server.rs's own helper returns (TempDir, AppState, token, import_id).
        // The shared harness in test_support.rs does not exist until Task 4.
        let (_tmp, state, _token, _import_id) = test_state().await;
        let response = get_path(state, "/v1/auth/try-demo").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn local_auth_routes_exist() {
        for path in ["/v1/auth/register", "/v1/auth/login"] {
            assert_ne!(auth_route_status(path).await, StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn imports_complete_and_detail_surface_timings_and_issues() {
        let (_tmp, state, token, import_id) = test_state().await;
        let body = CompleteImportBody {
            ok: true,
            status: None,
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: Some(48_000),
            parse_ms: Some(18_000),
            attachments_ms: Some(22_000),
            prepare_ms: Some(4_000),
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
        assert_eq!(value.id, import_id);
        assert_eq!(value.duration_ms, Some(48_000));
        assert_eq!(value.parse_ms, Some(18_000));
        assert_eq!(value.attachments_ms, Some(22_000));
        assert_eq!(value.prepare_ms, Some(4_000));
        assert_eq!(value.upload_ms, Some(8_000));
        assert_eq!(value.summary["parse"]["messages"], 10);
        assert_eq!(value.issues.len(), 2);
        assert_eq!(value.issues[0].kind, "skip");
        assert_eq!(value.issues[0].step, "convert");
        assert_eq!(value.issues[1].kind, "error");
        assert_eq!(value.issues[1].step, "upload");
    }

    #[tokio::test]
    async fn imports_complete_stores_completed_with_issues_status() {
        let (_tmp, state, token, import_id) = test_state().await;
        let body = CompleteImportBody {
            ok: true,
            status: Some("completed_with_issues".into()),
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: None,
            parse_ms: None,
            attachments_ms: None,
            prepare_ms: None,
            upload_ms: None,
            summary: None,
            issues: Vec::new(),
        };
        let response = imports_complete_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(body),
        )
        .await
        .unwrap();
        assert_eq!(response.0.status, "completed_with_issues");
    }

    #[tokio::test]
    async fn imports_complete_rejects_unknown_status() {
        let (_tmp, state, token, import_id) = test_state().await;
        let body = CompleteImportBody {
            ok: true,
            status: Some("victorious".into()),
            message_count: None,
            attachment_count: None,
            bytes_uploaded: None,
            duration_ms: None,
            parse_ms: None,
            attachments_ms: None,
            prepare_ms: None,
            upload_ms: None,
            summary: None,
            issues: Vec::new(),
        };
        let err = imports_complete_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(body),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));

        // The session is untouched.
        let mut conn = state.db.acquire().await.unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM vault_imports WHERE id = $1")
            .bind(import_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[tokio::test]
    async fn imports_complete_rejects_invalid_issue_kind_before_db_write() {
        let (_tmp, state, token, import_id) = test_state().await;
        let body = CompleteImportBody {
            ok: true,
            status: None,
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: Some(48_000),
            parse_ms: Some(18_000),
            attachments_ms: Some(22_000),
            prepare_ms: Some(4_000),
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

        let status: String = sqlx::query_scalar("SELECT status FROM vault_imports WHERE id = $1")
            .bind(import_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[tokio::test]
    async fn imports_get_handler_returns_not_found_for_missing_import() {
        let (_tmp, state, token, import_id) = test_state().await;
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

    #[tokio::test]
    async fn active_session_is_empty_then_reports_the_live_one() {
        let (_tmp, state, token, import_id) = test_state().await;

        let body = CreateImportBody {
            source: "imessage".into(),
            mode: "append".into(),
            tool: Some("message-vault-io".into()),
            account: None,
            stage: Some("write".into()),
            staging_dir: Some("/home/u/message-vault/staging-260830".into()),
            device_id: Some("device-a".into()),
            form: Some(serde_json::json!({ "source": "imessage-ios" })),
            source_fingerprint: Some(serde_json::json!({ "size_bytes": 42 })),
        };
        // `test_state` already opened a session; close it so this one can start.
        let _ = imports_discard_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
        )
        .await
        .unwrap();

        let created =
            imports_create_handler(State(state.clone()), auth_headers(&token), Json(body))
                .await
                .unwrap();

        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        let session = active.0.session.expect("a live session is reported");
        assert_eq!(session.id, created.0.id);
        assert_eq!(session.stage.as_deref(), Some("write"));
        assert_eq!(
            session.staging_dir.as_deref(),
            Some("/home/u/message-vault/staging-260830")
        );
        assert_eq!(session.device_id.as_deref(), Some("device-a"));
        assert_eq!(session.form["source"], "imessage-ios");
    }

    /// A stored form snapshot never carries credentials, whatever the
    /// client posts: the row outlives the run, and the secret must not.
    #[tokio::test]
    async fn a_stored_form_snapshot_drops_credentials() {
        let (_tmp, state, token, import_id) = test_state().await;
        let _ = imports_discard_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
        )
        .await
        .unwrap();

        let body = CreateImportBody {
            source: "imessage".into(),
            mode: "append".into(),
            tool: None,
            account: None,
            stage: None,
            staging_dir: None,
            device_id: None,
            // A client that has not learned the rule.
            form: Some(serde_json::json!({
                "source": "imessage-ios",
                "backupPassword": "hunter2",
                "whatsappKey": "0123456789abcdef",
            })),
            source_fingerprint: None,
        };
        let _ = imports_create_handler(State(state.clone()), auth_headers(&token), Json(body))
            .await
            .unwrap();

        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        let session = active.0.session.expect("a live session is reported");
        assert_eq!(
            session.form["source"], "imessage-ios",
            "the rest of the snapshot is kept"
        );
        assert!(
            session.form.get("backupPassword").is_none(),
            "backupPassword was stored: {}",
            session.form
        );
        assert!(
            session.form.get("whatsappKey").is_none(),
            "whatsappKey was stored: {}",
            session.form
        );
    }

    #[tokio::test]
    async fn a_second_session_is_refused_with_conflict() {
        let (_tmp, state, token, _import_id) = test_state().await;
        let body = CreateImportBody {
            source: "imessage".into(),
            mode: "append".into(),
            tool: None,
            account: None,
            stage: None,
            staging_dir: None,
            device_id: None,
            form: None,
            source_fingerprint: None,
        };
        let err = imports_create_handler(State(state.clone()), auth_headers(&token), Json(body))
            .await
            .unwrap_err();
        let ApiError::Conflict(message) = &err else {
            panic!("expected Conflict, got {err:?}");
        };
        // The 409 has to name the way out: the only place a stranded
        // session can be resumed or discarded is the desktop app's Import
        // screen.
        assert!(
            message.contains("Import in the desktop app"),
            "the conflict names how to clear the session: {message}"
        );
    }

    #[tokio::test]
    async fn stage_endpoint_advances_and_rejects_an_unknown_stage() {
        let (_tmp, state, token, import_id) = test_state().await;

        let _ = imports_stage_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(SetImportStageBody {
                stage: "pushing".into(),
                summary: None,
            }),
        )
        .await
        .unwrap();
        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        assert_eq!(active.0.session.unwrap().stage.as_deref(), Some("pushing"));

        let err = imports_stage_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(SetImportStageBody {
                stage: "halfway".into(),
                summary: None,
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[tokio::test]
    async fn discard_frees_the_slot() {
        let (_tmp, state, token, import_id) = test_state().await;
        let _ = imports_discard_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
        )
        .await
        .unwrap();
        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        assert!(active.0.session.is_none());
    }

    /// `/v1/imports/active` is a literal route registered alongside
    /// `/v1/imports/{id}`; if router registration order ever let the `{id}`
    /// extractor swallow it, `active` would fail to parse as an `i64` and
    /// this would come back 400 instead of 200.
    #[tokio::test]
    async fn active_route_is_not_captured_by_the_id_route() {
        let (_tmp, state, token, _import_id) = test_state().await;
        let status = crate::test_support::get_status(&state, "/v1/imports/active", &token).await;
        assert_eq!(status, StatusCode::OK);
    }

    /// `require_import_access` guards `GET /v1/imports`: with `can_import`
    /// off, the endpoint refuses; turned back on, it succeeds. Nothing else
    /// in the suite calls this route through the real HTTP stack, so
    /// deleting or inverting the guard inside the handler would ship green
    /// without this test.
    #[tokio::test]
    async fn import_endpoint_honors_can_import_flag() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let admin =
            crate::test_support::register_via_api(&state, "import-guard-admin", "hunter2hunter2")
                .await;
        let user =
            crate::test_support::register_via_api(&state, "import-guard-user", "hunter2hunter2")
                .await;

        assert_eq!(
            crate::test_support::patch_status(
                &state,
                &format!("/v1/admin/users/{}", user.account_id),
                &admin.token,
                serde_json::json!({ "can_import": false }),
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            crate::test_support::get_status(&state, "/v1/imports", &user.token).await,
            StatusCode::FORBIDDEN,
            "can_import=false must refuse GET /v1/imports"
        );

        assert_eq!(
            crate::test_support::patch_status(
                &state,
                &format!("/v1/admin/users/{}", user.account_id),
                &admin.token,
                serde_json::json!({ "can_import": true }),
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            crate::test_support::get_status(&state, "/v1/imports", &user.token).await,
            StatusCode::OK,
            "can_import=true must allow GET /v1/imports"
        );
    }

    /// `require_export_access` guards `GET /v1/export/messages/count`: with
    /// `can_export` off, the endpoint refuses; turned back on, it succeeds.
    #[tokio::test]
    async fn export_endpoint_honors_can_export_flag() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let admin =
            crate::test_support::register_via_api(&state, "export-guard-admin", "hunter2hunter2")
                .await;
        let user =
            crate::test_support::register_via_api(&state, "export-guard-user", "hunter2hunter2")
                .await;

        assert_eq!(
            crate::test_support::patch_status(
                &state,
                &format!("/v1/admin/users/{}", user.account_id),
                &admin.token,
                serde_json::json!({ "can_export": false }),
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            crate::test_support::get_status(&state, "/v1/export/messages/count", &user.token).await,
            StatusCode::FORBIDDEN,
            "can_export=false must refuse GET /v1/export/messages/count"
        );

        assert_eq!(
            crate::test_support::patch_status(
                &state,
                &format!("/v1/admin/users/{}", user.account_id),
                &admin.token,
                serde_json::json!({ "can_export": true }),
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            crate::test_support::get_status(&state, "/v1/export/messages/count", &user.token).await,
            StatusCode::OK,
            "can_export=true must allow GET /v1/export/messages/count"
        );
    }
}
