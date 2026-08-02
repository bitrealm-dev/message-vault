use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use axum::extract::{FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tower_http::limit::RequestBodyLimitLayer;

use rusqlite::Connection;

use crate::api_tokens;
use crate::assets;
use crate::config::{Config, validate_source_id};
use crate::dedupe;
use crate::import::{self, ImportMode, ImportOptions, ImportStats};
use crate::schema;
use crate::vault_owner;

const MAX_BODY_BYTES: usize = 512 * 1024 * 1024; // 512 MiB (multipart uploads)

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
    /// Process-local Import API token → account_id cache (avoids DB open per asset PUT).
    token_cache: Arc<Mutex<HashMap<String, String>>>,
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

    // Open a warm writer, recover hot journals, and ensure schema once before serving.
    let db_conn = Connection::open(&cfg.paths.db)?;
    schema::configure_connection(&db_conn)?;
    let _: i64 = db_conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))?;
    schema::ensure_vault_schema(&db_conn)?;
    schema::ensure_messages_schema(&db_conn)?;
    let mode: String = db_conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".into());
    eprintln!("  db:   {} (journal_mode={mode})", cfg.paths.db.display());
    let db = Arc::new(StdMutex::new(db_conn));

    let state = AppState {
        cfg: Arc::new(cfg),
        db,
        account_import_locks: Arc::new(Mutex::new(HashMap::new())),
        token_cache: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/auth/check", get(auth_check))
        .route("/v1/import", post(import_handler))
        .route(
            "/v1/assets/{sha256}",
            put(asset_put_handler).head(asset_head_handler),
        )
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("message-vault-rs serve listening on http://{bind}");
    eprintln!("  GET  /health");
    eprintln!("  GET  /v1/auth/check   (Bearer per-account Import API token)");
    eprintln!("  HEAD /v1/assets/{{sha256}}?source=&account=  (probe before PUT)");
    eprintln!("  PUT  /v1/assets/{{sha256}}?source=&account=  (raw body; content-addressed media)");
    eprintln!("  POST /v1/import?source=&account=&mode=append|replace&dedupe=false");
    eprintln!("       account= optional (must match token); derived from Bearer when omitted");
    eprintln!("       Content-Type: application/jsonl  (body only; assets by sha256)");
    eprintln!("       Content-Type: multipart/form-data   (field jsonl + file parts; remote push)");
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

    if let Some(q) = query
        .account
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let resolved = lookup_or_resolve_query(&state.cfg.paths.db, q).await?;
        if let Some(resolved) = resolved {
            if resolved != account_id {
                return Err(ApiError::Forbidden(
                    "account query does not match token's account".into(),
                ));
            }
        } else if q != account_id {
            return Err(ApiError::Forbidden(
                "account query does not match token's account".into(),
            ));
        }
    }
    let username = load_username(&state.cfg.paths.db, &account_id).await?;
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
        vault_owner::lookup_account_ref(&conn, &account_ref)
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
        vault_owner::username_for_account(&conn, &account_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("username lookup task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))
}

async fn resolve_account_ref_async(db_path: &Path, account_ref: &str) -> Result<String, ApiError> {
    let db = db_path.to_path_buf();
    let account_ref = account_ref.to_string();
    tokio::task::spawn_blocking(move || vault_owner::resolve_account_ref_at(&db, &account_ref))
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
    {
        let cache = state.token_cache.lock().await;
        if let Some(account_id) = cache.get(&token) {
            return Ok(AuthIdentity {
                account_id: account_id.clone(),
            });
        }
    }

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
        Some(account_id) => {
            state
                .token_cache
                .lock()
                .await
                .insert(token, account_id.clone());
            Ok(AuthIdentity { account_id })
        }
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
        let n = stream_body_to_file(request.into_body(), &jsonl_path).await?;
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
        let assets_dir = cfg.paths.assets_dir_for_account(&account_lookup, &source_lookup);
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
        discard_body(request.into_body()).await?;
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
    let n = match stream_body_to_file(request.into_body(), &tmp_path).await {
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
        assets::store_verified(&tmp_for_store, &sha, &assets_dir_store, mime.as_deref(), true)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("asset upload task: {e}")))?;

    // Rename consumes the temp file; remove leftovers after errors / already_present races.
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let (stored, already_present) =
        result.map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(AssetPutResponse {
        ok: true,
        sha256: stored.sha256,
        assets_path: stored.assets_path,
        already_present,
    }))
}

/// Drain request body without retaining it (used when asset already exists).
async fn discard_body(body: axum::body::Body) -> Result<(), ApiError> {
    let mut stream = body.into_data_stream();
    let mut seen = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| ApiError::BadRequest(format!("failed to read body: {e}")))?;
        seen = seen.saturating_add(chunk.len());
        if seen > MAX_BODY_BYTES {
            return Err(ApiError::BadRequest("request body too large".into()));
        }
    }
    Ok(())
}

async fn stream_body_to_file(body: axum::body::Body, dest: &Path) -> Result<u64, ApiError> {
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
        let chunk =
            chunk.map_err(|e| ApiError::BadRequest(format!("failed to read body: {e}")))?;
        written = written.saturating_add(chunk.len() as u64);
        if written > MAX_BODY_BYTES as u64 {
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
        let (mirror_csv, exclude_csv) = cfg.paths.ensure_account_csvs(&account)?;
        let opts = ImportOptions {
            db_path: &cfg.paths.db,
            assets_dir: &assets_dir,
            asset_root: &asset_root_owned,
            // HTTP import does not reload the address book; use CLI import-contacts / web VCF.
            contacts: None,
            contacts_mirror_csv: &mirror_csv,
            exclude_csv: &exclude_csv,
            overwrite_contacts: false,
            mode,
            source: &source_id,
            account_id: &account,
            // Content keys are only required when the optional post-import dedupe pass runs.
            fill_content_keys: do_dedupe,
            backfill_contacts: false,
        };
        let mut conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("import database mutex poisoned"))?;
        let stats = import::import_jsonl_files_on_conn(
            &mut conn,
            &[jsonl_path],
            &opts,
            import::ImportSchemaMode::AssumeReady,
        )?;
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
