use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use axum::extract::{FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tower_http::limit::RequestBodyLimitLayer;

use rusqlite::Connection;

use crate::asset_uploads;
use crate::assets;
use crate::config::{Config, validate_source_id};
use crate::db::account_profile;
use crate::db::api_tokens;
use crate::dedupe;
use crate::export_api::{self, DEFAULT_EXPORT_LIMIT, ExportPageOpts, ExportQueryError};
use crate::import::{self, ImportMode, ImportOptions, ImportStats};
use crate::db::schema;

/// Authenticated vault account from a per-account Import API token.
#[derive(Debug, Clone)]
struct AuthIdentity {
    account_id: String,
}

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
    /// Warm SQLite writer used by HTTP import (schema ensured at serve startup).
    db: Arc<StdMutex<Connection>>,
    /// Per-account import mutex: same-account imports stay serialized so staging
    /// rows for that tenant are not wiped mid-run. Different accounts may overlap
    /// at the lock layer; the shared `db` mutex still serializes writers.
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

enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
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

pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let server = cfg.require_server()?.clone();
    let bind = server.bind.clone();
    let upload_limits =
        asset_uploads::UploadLimits::resolve(server.asset_part_size, server.asset_max_bytes);
    let max_body_bytes = upload_limits.max_bytes as usize;

    // Open a warm writer, recover hot journals, and ensure schema once before serving.
    let db_conn = Connection::open(&cfg.paths.db)?;
    schema::configure_connection(&db_conn)?;
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

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/auth/check", get(auth_check))
        .route("/v1/export/messages", get(export_messages_handler))
        .route("/v1/imports", post(imports_create_handler))
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
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("message-vault-rs serve listening on http://{bind}");
    eprintln!("  GET  /health");
    eprintln!("  GET  /v1/auth/check   (Bearer per-account Import API token)");
    eprintln!("  GET  /v1/export/messages?q=&limit=&cursor=&account=  (read-only export)");
    eprintln!("  GET  /v1/assets/{{sha256}}?source=&account=  (download content-addressed media)");
    eprintln!("  POST /v1/imports  (start import session; returns id)");
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
    let db = db_path.to_path_buf();
    let account_id = account_id.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        // Read-only: do not run ensure_messages_schema (avoids write locks on auth).
        dedupe::source_priority_from_db(&conn, &account_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("sources list task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))
}

async fn lookup_or_resolve_query(
    db_path: &Path,
    account_ref: &str,
) -> Result<Option<String>, ApiError> {
    let db = db_path.to_path_buf();
    let account_ref = account_ref.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        account_profile::lookup_account_ref(&conn, &account_ref)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("account lookup task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))
}

async fn load_username(db_path: &Path, account_id: &str) -> Result<Option<String>, ApiError> {
    let db = db_path.to_path_buf();
    let account_id = account_id.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        account_profile::username_for_account(&conn, &account_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("username lookup task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))
}

async fn resolve_account_ref_async(db_path: &Path, account_ref: &str) -> Result<String, ApiError> {
    let db = db_path.to_path_buf();
    let account_ref = account_ref.to_string();
    tokio::task::spawn_blocking(move || account_profile::resolve_account_ref_at(&db, &account_ref))
        .await
        .map_err(|e| ApiError::Internal(format!("account resolve task: {e}")))?
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

fn bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
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

async fn resolve_auth(headers: &HeaderMap, state: &AppState) -> Result<AuthIdentity, ApiError> {
    let token = bearer_token(headers)?;
    // Always look up against SQLite so rotate/delete in Settings takes effect
    // without restarting serve (no process-local token cache).
    let db = state.cfg.paths.db.clone();
    let token_owned = token.clone();
    let account_id = tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        schema::ensure_accounts_schema(&conn)?;
        api_tokens::lookup_account_for_token(&conn, &token_owned)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("auth lookup task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    match account_id {
        Some(account_id) => Ok(AuthIdentity { account_id }),
        None => Err(ApiError::Unauthorized("invalid API token".into())),
    }
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
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("empty attachment path".into()));
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return Err(ApiError::BadRequest(format!(
            "attachment path must be relative: {name}"
        )));
    }
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ApiError::BadRequest(format!(
                    "unsafe attachment path: {name}"
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(ApiError::BadRequest(format!(
            "empty attachment path after normalize: {name}"
        )));
    }
    Ok(out)
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
}

fn default_true() -> bool {
    true
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

async fn imports_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateImportBody>,
) -> Result<Json<CreateImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    if body.source.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "body field source is required".into(),
        ));
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
        let conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("import database mutex poisoned"))?;
        crate::db::account_profile::ensure_account_row(&conn, &account)?;
        crate::db::vault_imports::start_import(
            &conn,
            &account,
            &source,
            &mode,
            tool.as_deref(),
        )
    })
    .await
    .map_err(|e| ApiError::Internal(format!("create import task failed: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(CreateImportResponse { ok: true, id }))
}

async fn imports_complete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
    Json(body): Json<CompleteImportBody>,
) -> Result<Json<CompleteImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account = resolve_import_account(&auth, None, &state.cfg.paths.db).await?;
    let db = Arc::clone(&state.db);
    let args = crate::db::vault_imports::CompleteImportArgs {
        ok: body.ok,
        message_count: body.message_count,
        attachment_count: body.attachment_count,
        bytes_uploaded: body.bytes_uploaded,
    };
    let row = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("import database mutex poisoned"))?;
        crate::db::vault_imports::complete_import(&conn, &account, import_id, &args)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("complete import task failed: {e}")))?
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") {
            ApiError::NotFound(msg)
        } else {
            ApiError::Internal(msg)
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

async fn import_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut query): Query<ImportQuery>,
    request: Request,
) -> Result<Json<ImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;

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

async fn resolve_asset_lookup(
    state: &AppState,
    headers: &HeaderMap,
    sha256: &str,
    query: &AssetPutQuery,
) -> Result<(String, String, Option<assets::StoredAsset>), ApiError> {
    let auth = resolve_auth(headers, state).await?;
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
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;
    let Some(stored) = existing else {
        return Err(ApiError::NotFound("asset not found".into()));
    };
    Ok(Json(AssetPutResponse {
        ok: true,
        sha256: stored.sha256,
        assets_path: stored.assets_path,
        already_present: true,
    }))
}

/// Download a previously stored content-addressed asset (read-only).
async fn asset_get_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
    Query(query): Query<AssetPutQuery>,
) -> Result<Response, ApiError> {
    let (account, source_id, existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;
    let Some(stored) = existing else {
        return Err(ApiError::NotFound("asset not found".into()));
    };

    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let path = assets_dir.join(&stored.assets_path);
    if !path.is_file() {
        return Err(ApiError::NotFound("asset file missing on disk".into()));
    }

    let mime = stored
        .mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".into());
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::Internal(format!("read {}: {e}", path.display())))?;
    let len = bytes.len();

    let mut response = bytes.into_response();
    let headers_mut = response.headers_mut();
    if let Ok(value) = header::HeaderValue::from_str(&mime) {
        headers_mut.insert(header::CONTENT_TYPE, value);
    }
    headers_mut.insert(header::CONTENT_LENGTH, header::HeaderValue::from(len));
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ExportMessagesQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

async fn export_messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportMessagesQuery>,
) -> Result<Json<export_api::ExportMessagesResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account =
        resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;
    let limit = query.limit.unwrap_or(DEFAULT_EXPORT_LIMIT);
    let q = query.q.clone();
    let cursor = query.cursor.clone();
    let source = query.source.clone();
    let db = Arc::clone(&state.db);

    let result = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| ExportQueryError::Internal("db lock poisoned".into()))?;
        export_api::export_messages(
            &conn,
            ExportPageOpts {
                account_id: &account,
                query: &q,
                limit,
                cursor: cursor.as_deref(),
                source_override: source.as_deref(),
            },
        )
    })
    .await
    .map_err(|e| ApiError::Internal(format!("export task: {e}")))?;

    match result {
        Ok(body) => Ok(Json(body)),
        Err(ExportQueryError::BadRequest(m)) => Err(ApiError::BadRequest(m)),
        Err(ExportQueryError::Internal(m)) => Err(ApiError::Internal(m)),
    }
}

async fn asset_put_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
    Query(query): Query<AssetPutQuery>,
    request: Request,
) -> Result<Json<AssetPutResponse>, ApiError> {
    let (account, source_id, existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;

    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("application/octet-stream"));

    if let Some(stored) = existing {
        discard_body(request.into_body(), state.max_body_bytes).await?;
        return Ok(Json(AssetPutResponse {
            ok: true,
            sha256: stored.sha256,
            assets_path: stored.assets_path,
            already_present: true,
        }));
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
    let result = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&assets_dir_store)?;
        assets::store_verified(
            &tmp_for_store,
            &sha,
            &assets_dir_store,
            mime.as_deref(),
            true,
        )
    })
    .await
    .map_err(|e| ApiError::Internal(format!("asset upload task: {e}")))?;

    // Rename consumes the temp file; remove leftovers after errors / already_present races.
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let (stored, already_present) = result.map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(AssetPutResponse {
        ok: true,
        sha256: stored.sha256,
        assets_path: stored.assets_path,
        already_present,
    }))
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
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;
    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let mime = body.mime.clone();
    let bytes = body.bytes;
    let sha = sha256.clone();
    let limits = state.upload_limits;
    let result = tokio::task::spawn_blocking(move || {
        asset_uploads::start_upload(&assets_dir, &sha, bytes, mime.as_deref(), limits)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("upload start task: {e}")))?
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

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
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;
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
    .map_err(|e| ApiError::Internal(format!("upload part task: {e}")))?
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
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
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;
    if let Some(stored) = existing {
        // Drop staging if a concurrent single-PUT won the race.
        let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
        let sha = sha256.clone();
        let uid = upload_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            asset_uploads::abort_upload(&assets_dir, &sha, &uid)
        })
        .await;
        return Ok(Json(AssetPutResponse {
            ok: true,
            sha256: stored.sha256,
            assets_path: stored.assets_path,
            already_present: true,
        }));
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
    let result = tokio::task::spawn_blocking(move || {
        asset_uploads::complete_upload(&assets_dir, &sha, &uid)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("upload complete task: {e}")))?
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let (stored, already_present) = result;
    Ok(Json(AssetPutResponse {
        ok: true,
        sha256: stored.sha256,
        assets_path: stored.assets_path,
        already_present,
    }))
}

async fn asset_upload_abort_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((sha256, upload_id)): AxumPath<(String, String)>,
    Query(query): Query<AssetPutQuery>,
) -> Result<Json<AssetUploadAbortResponse>, ApiError> {
    let (account, source_id, _existing) =
        resolve_asset_lookup(&state, &headers, &sha256, &query).await?;
    let assets_dir = state.cfg.paths.assets_dir_for_account(&account, &source_id);
    let sha = sha256.clone();
    let uid = upload_id.clone();
    tokio::task::spawn_blocking(move || asset_uploads::abort_upload(&assets_dir, &sha, &uid))
        .await
        .map_err(|e| ApiError::Internal(format!("upload abort task: {e}")))?
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(AssetUploadAbortResponse { ok: true }))
}

async fn read_body_limited(
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
async fn discard_body(
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

async fn stream_body_to_file(
    body: axum::body::Body,
    dest: &Path,
    max_body_bytes: usize,
) -> Result<u64, ApiError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("mkdir {}: {e}", parent.display())))?;
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| ApiError::Internal(format!("create {}: {e}", dest.display())))?;
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
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("mkdir {}: {e}", parent.display())))?;
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| ApiError::Internal(format!("create {}: {e}", dest.display())))?;
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

    let result = tokio::task::spawn_blocking(move || {
        let assets_dir = cfg.paths.assets_dir_for_account(&account, &source_id);
        // Raw body imports resolve attachment paths only via pre-uploaded sha256 assets.
        // Multipart supplies a temp asset_root for relative file parts.
        let asset_root_owned = asset_root_override.unwrap_or_else(|| assets_dir.clone());

        // Client session (vault-push): verify ownership. Otherwise start a one-shot
        // vault_imports row so Storage history works for curl / single POSTs.
        let (import_id, owns_session) = if let Some(id) = query_import_id {
            let conn = db
                .lock()
                .map_err(|_| anyhow::anyhow!("import database mutex poisoned"))?;
            crate::db::vault_imports::get_owned_import(&conn, &account, id)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            drop(conn);
            (Some(id), false)
        } else {
            let conn = db
                .lock()
                .map_err(|_| anyhow::anyhow!("import database mutex poisoned"))?;
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

        let opts = ImportOptions::fixed(
            &cfg.paths.db,
            &assets_dir,
            &asset_root_owned,
            None,
            false,
            mode,
            &source_id,
            &account,
            do_dedupe,
            false,
            import_id,
        );
        let mut conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("import database mutex poisoned"))?;
        let import_result = import::import_jsonl_files_on_conn(
            &mut conn,
            &[jsonl_path],
            &opts,
            import::ImportSchemaMode::AssumeReady,
        );

        if owns_session {
            if let Some(id) = import_id {
                let complete_args = match &import_result {
                    Ok(stats) => crate::db::vault_imports::CompleteImportArgs {
                        ok: true,
                        message_count: Some(stats.messages as i64),
                        attachment_count: Some(stats.attachments as i64),
                        bytes_uploaded: None,
                    },
                    Err(_) => crate::db::vault_imports::CompleteImportArgs {
                        ok: false,
                        message_count: None,
                        attachment_count: None,
                        bytes_uploaded: None,
                    },
                };
                if let Err(e) = crate::db::vault_imports::complete_import(
                    &conn,
                    &account,
                    id,
                    &complete_args,
                ) {
                    eprintln!("warning: complete_import({id}) failed: {e}");
                }
            }
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
    .map_err(|e| ApiError::Internal(format!("import task failed: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

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
