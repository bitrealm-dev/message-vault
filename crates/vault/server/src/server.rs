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
use std::sync::{Arc, Mutex as StdMutex};

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
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
use crate::config::{AuthMode, Config, GuestDemoSettings};
use crate::db::account_profile;
use crate::db::api_tokens;
use crate::db::schema;
use crate::db::session_tokens;
use crate::export_api::ExportQueryError;
use crate::guest_pool::{self, GuestPoolState};

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
    /// The authenticated vault account.
    pub account_id: String,
    /// What this credential is allowed to do.
    pub capability: AuthCapability,
}

/// Reject API tokens on routes that require a GUI session.
///
/// # Errors
///
/// Returns forbidden when the credential is a named API token.
pub fn require_full_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    match auth.capability {
        AuthCapability::Full => Ok(()),
        AuthCapability::ApiToken(_) => Err(ApiError::Forbidden(
            "this endpoint requires a signed-in session; use an API token only for import/export"
                .into(),
        )),
    }
}

/// Reject sample (guest) accounts on import, asset upload, and API-token mutations.
///
/// # Errors
///
/// Returns forbidden when `account_id` has a `guest_status`, or internal when
/// the lookup fails.
pub fn reject_if_guest(conn: &Connection, account_id: &str) -> Result<(), ApiError> {
    if account_profile::is_guest_account(conn, account_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::Forbidden(
            "sample accounts cannot import, export backups, or create API tokens".into(),
        ));
    }
    Ok(())
}

/// Open the configured database and reject the account when it is a guest.
pub(crate) async fn reject_if_guest_account(db: &Path, account_id: &str) -> Result<(), ApiError> {
    let db = db.to_path_buf();
    let account_id = account_id.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = schema::open_configured(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
        reject_if_guest(&conn, &account_id)
    })
    .await
    .join_map("guest check task", |e| e)
}

/// Allow session or an API token that includes import.
///
/// # Errors
///
/// Returns forbidden when the API token does not include import.
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
///
/// # Errors
///
/// Returns forbidden when the API token does not include export.
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
///
/// # Errors
///
/// Returns forbidden when the API token cannot access assets.
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

/// Shared server state passed to every HTTP handler.
#[derive(Clone)]
pub struct AppState {
    /// Loaded configuration.
    pub cfg: Arc<Config>,
    /// Warm connection for short import-session SQL only (`POST /v1/imports`,
    /// complete, import-id verify / one-shot start). Bulk `POST /v1/import` and
    /// export open their own connections so they do not hold this mutex.
    pub(crate) db: Arc<StdMutex<Connection>>,
    /// Per-account import mutex: same-account imports stay serialized so staging
    /// rows (the temporary import area) for that tenant are not wiped mid-run.
    /// Different accounts may overlap at the lock layer; SQLite write-ahead
    /// logging plus busy_timeout serialize writers.
    pub(crate) account_import_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Serialize multipart complete per (account, sha256) so two clients cannot
    /// race `store_verified` on the same SHA-256 fingerprint.
    pub(crate) asset_complete_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Multipart / asset size limits from `[server]` (env may override part size).
    pub(crate) upload_limits: asset_uploads::UploadLimits,
    /// Axum request body cap (single PUT or one part); equals `asset_max_bytes`.
    pub(crate) max_body_bytes: usize,
    /// Hosted guest-demo pool. Off on self-hosted (`GuestDemoSettings::disabled`).
    pub guest: GuestDemoSettings,
    /// One on-demand template clone at a time (empty-pool Try it).
    pub guest_clone_lock: Arc<Mutex<()>>,
    /// Hosted Try it assignments in the last 15 minutes (refill demand).
    pub guest_demand: Arc<StdMutex<GuestPoolState>>,
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

pub(crate) fn lock_conn(
    db: &StdMutex<Connection>,
) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
    lock_named(db, "database")
}

pub(crate) fn lock_import_conn(
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

/// Build the Cross-Origin Resource Sharing (CORS) layer from
/// `[server].cors_origins`. CORS is the browser rule that decides which other
/// websites may call this API.
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
    let mut allowed = Vec::new();
    for origin in origins {
        let trimmed = origin.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = trimmed.parse() {
            allowed.push(value);
        }
    }
    if allowed.is_empty() {
        return CorsLayer::new();
    }
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
}

fn limited_auth_router(mode: AuthMode) -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, spec) =
        crate::openapi::auth_public_openapi(crate::openapi::SpecAuth::Live(mode)).split_for_parts();
    (
        router
            // Auth JSON is tiny; keep a tight limit so Argon2/JWKS abuse cannot ship 512 MiB bodies.
            .layer(RequestBodyLimitLayer::new(32 * 1024)),
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
    let mode = AuthMode::from_env();
    let (auth_small, mut spec) = limited_auth_router(mode);
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

/// Open a configured connection, run `f`, and map join/task errors.
///
/// # Errors
///
/// Returns an API error when the blocking task fails or `f` returns an error.
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

pub(crate) async fn with_configured_db_map<T, E, F>(
    db_path: &Path,
    task: &str,
    f: F,
) -> Result<T, ApiError>
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

pub(crate) async fn with_locked_conn<T, E, F>(
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

/// Start the HTTP server.
///
/// # Errors
///
/// Returns an error when the database cannot be opened, the operation lock
/// cannot be taken, or the listener cannot bind.
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let server = cfg.require_server()?.clone();
    let bind = server.bind.clone();
    let _operation_lock = crate::operation_lock::acquire_for_serve(&cfg.paths.db)?;
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
    crate::operation_lock::mark_ready(&cfg.paths.db)?;
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
        guest: GuestDemoSettings::from_env(),
        guest_clone_lock: Arc::new(Mutex::new(())),
        guest_demand: Arc::new(StdMutex::new(GuestPoolState::new())),
    };

    if state.guest.enabled {
        spawn_guest_pool_worker(state.clone());
    }

    let app = http_app(state);
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

/// Sweep expired guests and refill unused ready copies every 60 seconds.
///
/// The first tick runs immediately so the pool is not empty on the first Try it.
/// Shrink-over-ceiling runs without the clone lock. Each clone takes
/// `guest_clone_lock` only for that one copy so on-demand Try it can assign a
/// ready guest (or clone one) between refills. The worker does not take the
/// vault operation lock; `serve` already holds it.
fn spawn_guest_pool_worker(worker_state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let db = worker_state.cfg.paths.db.clone();
            let cfg = worker_state.cfg.clone();
            let guest = worker_state.guest;
            let demand = match worker_state.guest_demand.lock() {
                Ok(mut guard) => guard.count_last_15m(),
                Err(_) => {
                    eprintln!("guest demand lock poisoned; refill uses the floor");
                    0
                }
            };
            let clone_lock = worker_state.guest_clone_lock.clone();
            let data_dir = cfg.paths.data_dir.clone();

            let sweep_db = db.clone();
            log_guest_pool_task(
                "sweep",
                tokio::task::spawn_blocking(move || -> anyhow::Result<u32> {
                    let conn = schema::open_configured(&sweep_db)?;
                    guest_pool::sweep_expired_guests(&conn, &data_dir)
                })
                .await,
            );

            let shrink_db = db.clone();
            let shrink_cfg = cfg.clone();
            log_guest_pool_task(
                "shrink",
                tokio::task::spawn_blocking(move || -> anyhow::Result<u32> {
                    let conn = schema::open_configured(&shrink_db)?;
                    guest_pool::shrink_over_ceiling(&conn, &shrink_cfg, guest)
                })
                .await,
            );

            loop {
                let one_db = db.clone();
                let one_cfg = cfg.clone();
                let result = {
                    let _guard = clone_lock.lock().await;
                    tokio::task::spawn_blocking(move || -> anyhow::Result<u32> {
                        let mut conn = schema::open_configured(&one_db)?;
                        guest_pool::refill_one(&mut conn, &one_cfg, guest, demand)
                    })
                    .await
                };
                match result {
                    Ok(Ok(0)) => break,
                    Ok(Ok(_)) => {}
                    other => {
                        log_guest_pool_task("refill", other);
                        break;
                    }
                }
            }
        }
    });
}

fn log_guest_pool_task(what: &str, result: Result<anyhow::Result<u32>, tokio::task::JoinError>) {
    match result {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => eprintln!("guest pool {what} failed: {err:#}"),
        Err(join_err) => eprintln!("guest pool {what} failed: {join_err}"),
    }
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

async fn resolve_account_ref_async(db_path: &Path, account_ref: &str) -> Result<String, ApiError> {
    let db = db_path.to_path_buf();
    let account_ref = account_ref.to_string();
    tokio::task::spawn_blocking(move || account_profile::resolve_account_ref_at(&db, &account_ref))
        .await
        .join_map("account resolve task", |e| {
            ApiError::BadRequest(e.to_string())
        })
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

/// Resolve the account id for an import or export: Bearer token binds the account.
/// Optional query may be username or UUID and must match the token.
pub(crate) async fn resolve_import_account(
    auth: &AuthIdentity,
    query_account: Option<&str>,
    db_path: &Path,
) -> Result<String, ApiError> {
    let query = nonempty_query_account(query_account);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{
        CompleteImportBody, CompleteImportIssueBody, CreateImportBody, imports_complete_handler,
        imports_create_handler, imports_get_handler,
    };
    use axum::extract::{Path as AxumPath, Query, State};
    use rusqlite::params;
    use std::sync::{Arc, Mutex as StdMutex};
    use tempfile::TempDir;

    fn auth_public_router(mode: AuthMode) -> Router<AppState> {
        limited_auth_router(mode).0
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
                    openapi_ui: false,
                }),
            }),
            db: Arc::new(StdMutex::new(conn)),
            account_import_locks: Arc::new(Mutex::new(HashMap::new())),
            asset_complete_locks: Arc::new(Mutex::new(HashMap::new())),
            upload_limits: asset_uploads::UploadLimits::default(),
            max_body_bytes: asset_uploads::DEFAULT_MAX_BYTES as usize,
            guest: crate::config::GuestDemoSettings::disabled(),
            guest_clone_lock: Arc::new(Mutex::new(())),
            guest_demand: Arc::new(StdMutex::new(GuestPoolState::new())),
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

    #[tokio::test]
    async fn health_still_ok() {
        let (_tmp, state, _token, _import_id) = test_state();
        let response = get_path(state, "/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "ok\n");
    }

    #[tokio::test]
    async fn openapi_ui_off_does_not_serve_spec() {
        let (_tmp, state, _token, _import_id) = test_state();
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
        let (_tmp, mut state, _token, _import_id) = test_state();
        {
            let cfg = Arc::make_mut(&mut state.cfg);
            cfg.server.as_mut().unwrap().openapi_ui = true;
        }
        let response = get_path(state, "/openapi.json").await;
        assert_eq!(response.status(), StatusCode::OK);
        let v: serde_json::Value = response.json().await.unwrap();
        assert!(v["openapi"].as_str().unwrap().starts_with("3."));
    }

    async fn auth_route_status(mode: AuthMode, path: &str) -> StatusCode {
        let (_tmp, state, _token, _import_id) = test_state();
        let app = auth_public_router(mode).with_state(state);
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
    async fn auth_mode_includes_try_demo_flag() {
        let (_tmp, state, _token, _import_id) = test_state();
        let Json(value) = crate::auth::auth_mode_handler(State(state)).await;
        assert!(!value.try_demo);
        assert!(!value.mode.is_empty());
    }

    #[tokio::test]
    async fn try_demo_route_exists() {
        assert_ne!(
            auth_route_status(AuthMode::Local, "/v1/auth/try-demo").await,
            StatusCode::NOT_FOUND
        );
        assert_ne!(
            auth_route_status(AuthMode::Hanko, "/v1/auth/try-demo").await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn hanko_router_excludes_local_auth_routes() {
        for path in ["/v1/auth/register", "/v1/auth/login"] {
            assert_ne!(
                auth_route_status(AuthMode::Local, path).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                auth_route_status(AuthMode::Hanko, path).await,
                StatusCode::NOT_FOUND
            );
        }
        assert_ne!(
            auth_route_status(AuthMode::Hanko, "/v1/auth/hanko/session").await,
            StatusCode::NOT_FOUND
        );
    }

    fn guest_test_state() -> (TempDir, AppState, String) {
        let (tmp, state, _token, _import_id) = test_state();
        let guest_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let token = {
            let conn = state.db.lock().unwrap();
            account_profile::insert_guest_account(&conn, guest_id, "guest-bbbb", None).unwrap();
            account_profile::set_guest_status(&conn, guest_id, "assigned").unwrap();
            crate::db::session_tokens::insert_account_session_token(&conn, guest_id).unwrap()
        };
        (tmp, state, token)
    }

    #[tokio::test]
    async fn guest_cannot_create_imports_but_can_export_messages() {
        let (_tmp, state, token) = guest_test_state();

        let err = imports_create_handler(
            State(state.clone()),
            auth_headers(&token),
            Json(CreateImportBody {
                source: "ios".into(),
                mode: "append".into(),
                tool: None,
                account: None,
            }),
        )
        .await
        .unwrap_err();
        match err {
            ApiError::Forbidden(msg) => {
                assert!(
                    msg.contains("sample accounts"),
                    "expected sample-account message, got {msg}"
                );
            }
            other => panic!("expected forbidden on POST /v1/imports, got {other:?}"),
        }

        let export = crate::export_api::export_messages_handler(
            State(state),
            auth_headers(&token),
            Query(crate::export_api::ExportMessagesQuery {
                q: "hello".into(),
                limit: None,
                offset: None,
                cursor: None,
                account: None,
                source: None,
            }),
        )
        .await;
        match export {
            Ok(_) => {}
            Err(ApiError::BadRequest(_)) => {}
            Err(ApiError::Forbidden(msg)) => {
                panic!("GET /v1/export/messages must not be 403 for guests, got {msg}")
            }
            Err(other) => panic!("unexpected export error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn guest_cannot_complete_imports() {
        let (_tmp, state, token) = guest_test_state();
        let err = imports_complete_handler(
            State(state),
            auth_headers(&token),
            AxumPath(1),
            Json(CompleteImportBody {
                ok: true,
                message_count: Some(1),
                attachment_count: None,
                bytes_uploaded: None,
                duration_ms: None,
                parse_ms: None,
                convert_ms: None,
                upload_ms: None,
                summary: None,
                issues: vec![],
            }),
        )
        .await
        .unwrap_err();
        match err {
            ApiError::Forbidden(msg) => {
                assert!(
                    msg.contains("sample accounts"),
                    "expected sample-account message, got {msg}"
                );
            }
            other => {
                panic!("expected forbidden on POST /v1/imports/{{id}}/complete, got {other:?}")
            }
        }
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
        assert_eq!(value.id, import_id);
        assert_eq!(value.duration_ms, Some(48_000));
        assert_eq!(value.parse_ms, Some(18_000));
        assert_eq!(value.convert_ms, Some(22_000));
        assert_eq!(value.upload_ms, Some(8_000));
        assert_eq!(value.summary["parse"]["messages"], 10);
        assert_eq!(value.issues.len(), 2);
        assert_eq!(value.issues[0].kind, "skip");
        assert_eq!(value.issues[0].step, "convert");
        assert_eq!(value.issues[1].kind, "error");
        assert_eq!(value.issues[1].step, "upload");
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
