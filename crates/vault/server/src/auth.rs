//! Authentication handlers: register, login, and Hanko session exchange.
//!
//! All three return a Bearer API token the rest of the API already accepts.
//! There is no separate session layer — these are additional ways to get a
//! token. Hanko is an external sign-in service. A Hanko session is a signed
//! JSON Web Token (a signed claim of who the user is) that this server checks
//! and then exchanges for a vault token.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use rand::TryRng;
use serde::{Deserialize, Serialize};
use sqlx::Connection;
use sqlx::{AnyConnection, AnyPool};

use crate::config::Config;
use crate::db::{account_profile, api_tokens, schema, session_tokens};
use crate::dedupe;
use crate::server::{ApiError, AppState, nonempty_query_account, resolve_auth};

/// How long Try it waits for an on-demand guest clone when the ready pool is empty.
const TRY_DEMO_CLONE_TIMEOUT: Duration = Duration::from_secs(60);

/// Hosted vaults reject password login as the shared `demo` template account.
pub(crate) fn reject_demo_password_login(enabled: bool, username: &str) -> bool {
    enabled && username.eq_ignore_ascii_case("demo")
}

/// Max password bytes accepted before hashing (registration / login / change).
const MAX_PASSWORD_BYTES: usize = 1024;
const MIN_PASSWORD_CHARS: usize = 8;
/// Max Hanko JSON Web Token string length accepted for exchange.
const MAX_HANKO_JWT_BYTES: usize = 16 * 1024;
/// Sliding window for unauthenticated auth endpoints.
const AUTH_RATE_WINDOW: Duration = Duration::from_secs(60);
const AUTH_RATE_MAX: usize = 20;
/// Per visitor address (`CF-Connecting-IP`, or `unknown` when missing).
const TRY_DEMO_PER_IP_RATE_MAX: usize = 60;
/// Whole-process Try it cap. Login stays at 20.
const TRY_DEMO_RATE_MAX: usize = 2000;
const _: () = assert!(TRY_DEMO_RATE_MAX > AUTH_RATE_MAX);
const JWKS_CACHE_TTL: Duration = Duration::from_secs(300);
const JWKS_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

static AUTH_RATE_LIMITS: Mutex<Option<HashMap<String, VecDeque<Instant>>>> = Mutex::new(None);
static JWKS_CACHE: Mutex<Option<(String, Instant, serde_json::Value)>> = Mutex::new(None);
static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();

/// Reject when `bucket` has seen more than [`AUTH_RATE_MAX`] hits in [`AUTH_RATE_WINDOW`].
fn check_auth_rate_limit(bucket: &str) -> Result<(), ApiError> {
    check_auth_rate_limit_max(bucket, AUTH_RATE_MAX)
}

/// Reject when `bucket` has seen at least `max` hits in [`AUTH_RATE_WINDOW`].
fn check_auth_rate_limit_max(bucket: &str, max: usize) -> Result<(), ApiError> {
    let mut guard = AUTH_RATE_LIMITS
        .lock()
        .map_err(|_| ApiError::Internal("auth rate limiter poisoned".into()))?;
    let map = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    let entry = map.entry(bucket.to_string()).or_default();
    while let Some(oldest) = entry.front() {
        if now.duration_since(*oldest) <= AUTH_RATE_WINDOW {
            break;
        }
        entry.pop_front();
    }
    if entry.len() >= max {
        return Err(ApiError::TooManyRequests(
            "too many authentication attempts; try again shortly".into(),
        ));
    }
    entry.push_back(now);
    Ok(())
}

fn check_try_demo_rate_limits(cf_connecting_ip: Option<&str>) -> Result<(), ApiError> {
    let per_ip = try_demo_client_key(cf_connecting_ip);
    check_auth_rate_limit_max(&per_ip, TRY_DEMO_PER_IP_RATE_MAX)?;
    check_auth_rate_limit_max("try-demo", TRY_DEMO_RATE_MAX)?;
    Ok(())
}

fn try_demo_client_key(cf_connecting_ip: Option<&str>) -> String {
    match cf_connecting_ip.and_then(parse_single_ip) {
        Some(ip) => format!("try-demo:{ip}"),
        None => "try-demo:unknown".to_string(),
    }
}

fn parse_single_ip(raw: &str) -> Option<IpAddr> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains(',') || trimmed.contains(' ') {
        return None;
    }
    trimmed.parse().ok()
}

#[cfg(test)]
fn reset_auth_rate_limit_bucket_for_test(bucket: &str) {
    if let Ok(mut guard) = AUTH_RATE_LIMITS.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(bucket);
    }
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Body for local account registration.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    /// Login username.
    pub username: String,
    /// Local password; absent or empty registers an account without one.
    #[serde(default)]
    pub password: Option<String>,
    /// Display name shown in the vault.
    #[serde(default)]
    pub preferred_name: Option<String>,
    /// Phone number linked to the account.
    #[serde(default)]
    pub phone: Option<String>,
}

/// Username and password.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    /// Login username.
    pub username: String,
    /// Login password.
    #[serde(default)]
    pub password: String,
}

/// A raw Hanko session JWT from the client's onSessionCreated callback.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct HankoSessionRequest {
    /// The raw Hanko session JSON Web Token from the client-side
    /// `onSessionCreated` callback.
    pub hanko_jwt: String,
}

/// Session token plus the account id and username it belongs to.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuthTokenResponse {
    /// Session token to send as `Authorization: Bearer …`.
    pub token: String,
    /// Account id the session belongs to.
    pub account_id: String,
    /// Account username (falls back to the account id).
    pub username: String,
}

impl AuthTokenResponse {
    /// Issue (or reuse) the session token for an existing account. Uses the
    /// account id when the row has no username.
    async fn for_existing_account(
        conn: &mut AnyConnection,
        account_id: String,
    ) -> Result<AuthTokenResponse> {
        let token = session_tokens::get_or_create_session_token(conn, &account_id).await?;
        let username = account_profile::username_for_account(conn, &account_id)
            .await?
            .unwrap_or_else(|| account_id.clone());
        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
        })
    }
}

// ---------------------------------------------------------------------------
// Password helpers
// ---------------------------------------------------------------------------

/// Hash a plaintext password with argon2id.
///
/// # Errors
///
/// Returns an error when the password cannot be hashed.
fn hash_password(password: &str) -> Result<String> {
    let mut salt_bytes = [0u8; 16];
    rand::rngs::SysRng
        .try_fill_bytes(&mut salt_bytes)
        .context("fill password salt from system RNG")?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| anyhow::anyhow!("password salt encode failed: {e}"))?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("password hash failed: {e}"))?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against an argon2 hash.
fn verify_password(hash: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// True when `password` matches the stored hash.
///
/// A missing or empty hash means the account has no password, so only an empty
/// password is accepted. Otherwise argon2 is used.
fn passwords_match(password_hash: Option<&str>, password: &str) -> bool {
    match password_hash {
        None | Some("") => password.is_empty(),
        Some(hash) => verify_password(hash, password),
    }
}

/// A real argon2 hash used only so missing-account logins take similar time.
fn dummy_password_hash() -> &'static str {
    DUMMY_PASSWORD_HASH.get_or_init(|| {
        hash_password("timing-equalization-dummy-password").expect("dummy password hash")
    })
}

/// Always run Argon2 so missing accounts cost similar to wrong passwords.
/// Passwordless accounts (NULL hash) still accept an empty password only.
fn verify_login_password(password_hash: Option<&str>, password: &str) -> bool {
    match password_hash {
        None | Some("") => {
            let _ = verify_password(dummy_password_hash(), password);
            password.is_empty()
        }
        Some(hash) => verify_password(hash, password),
    }
}

/// Reject passwords that are too short or too long.
fn validate_password_policy(password: &str) -> Result<(), ApiError> {
    if password.len() < MIN_PASSWORD_CHARS {
        return Err(ApiError::BadRequest(format!(
            "password must be at least {MIN_PASSWORD_CHARS} characters"
        )));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(ApiError::BadRequest("password is too long".into()));
    }
    Ok(())
}

/// Fetch Hanko's public signing keys, reusing a cached copy for a few minutes.
///
/// # Errors
///
/// Returns an error when the HTTP client cannot be built, the keys cannot be
/// fetched, or the response is not JSON.
fn fetch_jwks_cached(jwk_url: &str) -> Result<serde_json::Value> {
    let now = Instant::now();
    if let Ok(guard) = JWKS_CACHE.lock()
        && let Some((url, fetched_at, json)) = guard.as_ref()
        && url == jwk_url
        && now.duration_since(*fetched_at) < JWKS_CACHE_TTL
    {
        return Ok(json.clone());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(JWKS_HTTP_TIMEOUT)
        .build()
        .context("build JWKS HTTP client")?;
    let jwks_json: serde_json::Value = client
        .get(jwk_url)
        .send()
        .with_context(|| format!("failed to fetch JWKS from {jwk_url}"))?
        .error_for_status()
        .with_context(|| format!("JWKS HTTP error from {jwk_url}"))?
        .json()
        .with_context(|| "failed to parse JWKS")?;

    if let Ok(mut guard) = JWKS_CACHE.lock() {
        *guard = Some((jwk_url.to_string(), now, jwks_json.clone()));
    }
    Ok(jwks_json)
}

// ---------------------------------------------------------------------------
// Username validation
// ---------------------------------------------------------------------------

fn normalize_username(raw: &str) -> String {
    raw.trim().to_string()
}

fn is_valid_username(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.len() > 128 {
        return false;
    }
    s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn nonempty_trimmed(value: Option<&str>) -> Option<String> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn nonempty_trimmed_lower(value: Option<&str>) -> Option<String> {
    nonempty_trimmed(value).map(|s| s.to_ascii_lowercase())
}

fn jwk_matching_kid<'a>(keys: &'a [serde_json::Value], kid: &str) -> Result<&'a serde_json::Value> {
    for key in keys {
        let key_id = key.get("kid").and_then(|v| v.as_str());
        if key_id == Some(kid) {
            return Ok(key);
        }
    }
    Err(anyhow::anyhow!("no JWK matching kid: {kid}"))
}

fn username_from_hanko_email_or_id(email: Option<&str>, hanko_user_id: &str) -> String {
    if let Some(email) = email
        && let Some(local_part) = email.split('@').next()
    {
        return local_part.to_string();
    }
    let short_id: String = hanko_user_id.chars().take(8).collect();
    format!("user_{short_id}")
}

async fn unique_hanko_username(
    conn: &mut AnyConnection,
    username: String,
    account_id: &str,
) -> Result<String> {
    if account_profile::lookup_account_ref(conn, &username)
        .await?
        .is_some()
    {
        Ok(format!("{}_{}", username, &account_id[..8]))
    } else {
        Ok(username)
    }
}

/// Sign-in mode and Hanko URL so clients can render the right login form.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AuthModeResponse {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hanko_api_url: Option<String>,
    pub try_demo: bool,
}

/// Returns the server's configured authentication mode so clients
/// can render the correct login form before authenticating.
#[utoipa::path(
    get,
    path = "/v1/auth/mode",
    tag = "Auth",
    responses((status = 200, description = "Sign-in mode", body = AuthModeResponse))
)]
pub(crate) async fn auth_mode_handler(State(state): State<AppState>) -> Json<AuthModeResponse> {
    let mode = crate::config::AuthMode::from_env();
    let hanko_api_url = std::env::var("HANKO_API_URL")
        .ok()
        .or_else(|| std::env::var("NEXT_PUBLIC_HANKO_API_URL").ok());
    Json(AuthModeResponse {
        mode: match mode {
            crate::config::AuthMode::Hanko => "hanko".into(),
            crate::config::AuthMode::Local => "local".into(),
        },
        hanko_api_url,
        try_demo: state.guest.enabled,
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct AuthCheckQuery {
    #[serde(default)]
    account: Option<String>,
}

/// Token check result: account, username, sources.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AuthCheckResponse {
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

/// Check the Bearer token and return the account it resolves to, its username,
/// and its import sources.
#[utoipa::path(
    get,
    path = "/v1/auth/check",
    tag = "Auth",
    security(("bearer" = [])),
    params(("account" = Option<String>, Query, description = "Must match the token account")),
    responses(
        (status = 200, body = AuthCheckResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn auth_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuthCheckQuery>,
) -> Result<Json<AuthCheckResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account_id = auth.account_id;
    let username = load_username(&state.db, &account_id).await?;

    if let Some(q) = nonempty_query_account(query.account.as_deref()) {
        let resolved = lookup_or_resolve_query(&state.db, q).await?;
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
    let sources = list_account_sources(&state.db, &account_id).await?;
    Ok(Json(AuthCheckResponse {
        ok: true,
        sources,
        account_id: Some(account_id),
        username,
        account_ok: Some(true),
        admin: None,
    }))
}

async fn list_account_sources(pool: &AnyPool, account_id: &str) -> Result<Vec<String>, ApiError> {
    let account_id = account_id.to_string();
    // Read-only: do not run ensure_vault_schema (avoids write locks on auth).
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    dedupe::source_priority_from_db(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

async fn lookup_or_resolve_query(
    pool: &AnyPool,
    account_ref: &str,
) -> Result<Option<String>, ApiError> {
    let account_ref = account_ref.to_string();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    account_profile::lookup_account_ref(&mut conn, &account_ref)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

async fn load_username(pool: &AnyPool, account_id: &str) -> Result<Option<String>, ApiError> {
    let account_id = account_id.to_string();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    account_profile::username_for_account(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Create a local vault account and return its session token.
#[utoipa::path(
    post,
    path = "/v1/auth/register",
    tag = "Auth",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Session issued", body = AuthTokenResponse),
        (status = 400, description = "Invalid input", body = crate::server::ErrorBody),
        (status = 429, description = "Rate limited", body = crate::server::ErrorBody)
    )
)]
pub async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let username = normalize_username(&req.username);
    if !is_valid_username(&username) {
        return Err(ApiError::BadRequest(
            "username must be 1–128 chars (alphanumeric, _, -, .)".into(),
        ));
    }
    check_auth_rate_limit(&format!("register:{username}"))?;

    let password_plain = req.password.as_deref().unwrap_or("").to_string();
    if !password_plain.is_empty() {
        validate_password_policy(&password_plain)?;
    }
    let password_hash: Option<String> = if password_plain.is_empty() {
        None
    } else {
        Some(hash_password(&password_plain).map_err(|e| ApiError::Internal(e.to_string()))?)
    };

    let preferred_name = nonempty_trimmed(req.preferred_name.as_deref());
    let phone = nonempty_trimmed(req.phone.as_deref());

    let account_id = uuid::Uuid::new_v4().to_string();

    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if account_profile::lookup_account_ref(&mut tx, &username)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "username already taken: {username}"
        )));
    }

    account_profile::insert_account(
        &mut tx,
        &account_id,
        &username,
        password_hash.as_deref(),
        preferred_name.as_deref(),
        None,  // hanko_user_id
        false, // read_only
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if let Some(ref phone) = phone {
        account_profile::upsert_account_phone(&mut tx, &account_id, phone)
            .await
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }

    let token = session_tokens::insert_account_session_token(&mut tx, &account_id)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(AuthTokenResponse {
        token,
        account_id,
        username,
    }))
}

/// Verify a local username and password and return a session token.
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Session issued", body = AuthTokenResponse),
        (status = 400, description = "Invalid input", body = crate::server::ErrorBody),
        (status = 401, description = "Invalid credentials", body = crate::server::ErrorBody),
        (status = 429, description = "Rate limited", body = crate::server::ErrorBody)
    )
)]
pub async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let username = normalize_username(&req.username);
    if username.is_empty() {
        return Err(ApiError::BadRequest("username is required".into()));
    }
    check_auth_rate_limit(&format!("login:{username}"))?;
    if req.password.len() > MAX_PASSWORD_BYTES {
        return Err(ApiError::BadRequest("password is too long".into()));
    }

    let password = req.password.clone();
    let guest_enabled = state.guest.enabled;

    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let Some(account_id) = account_profile::lookup_account_ref(&mut conn, &username)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    else {
        let _ = verify_password(dummy_password_hash(), &password);
        return Err(ApiError::Unauthorized(
            "invalid username or password".into(),
        ));
    };

    if reject_demo_password_login(guest_enabled, &username) {
        return Err(hosted_demo_login_rejected());
    }

    let password_hash = account_profile::load_password_hash(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !verify_login_password(password_hash.as_deref(), &password) {
        return Err(ApiError::Unauthorized(
            "invalid username or password".into(),
        ));
    }

    let response = AuthTokenResponse::for_existing_account(&mut conn, account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(response))
}

/// Verify a Hanko session JSON Web Token and exchange it for a vault session
/// token.
#[utoipa::path(
    post,
    path = "/v1/auth/hanko/session",
    tag = "Auth",
    request_body = HankoSessionRequest,
    responses(
        (status = 200, description = "Session issued", body = AuthTokenResponse),
        (status = 400, description = "Invalid input", body = crate::server::ErrorBody),
        (status = 429, description = "Rate limited", body = crate::server::ErrorBody)
    )
)]
pub async fn hanko_session_handler(
    State(state): State<AppState>,
    Json(req): Json<HankoSessionRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    check_auth_rate_limit("hanko:session")?;
    if req.hanko_jwt.len() > MAX_HANKO_JWT_BYTES {
        return Err(ApiError::BadRequest("hanko_jwt is too long".into()));
    }

    let hanko_api_url = match std::env::var("HANKO_API_URL") {
        Ok(url) => url,
        Err(_) => std::env::var("NEXT_PUBLIC_HANKO_API_URL").unwrap_or_default(),
    };

    if hanko_api_url.is_empty() {
        return Err(ApiError::Internal("HANKO_API_URL is not configured".into()));
    }

    let jwk_url = format!(
        "{}/.well-known/jwks.json",
        hanko_api_url.trim_end_matches('/')
    );
    let jtw = req.hanko_jwt.clone();
    let hanko_issuer = hanko_api_url.trim_end_matches('/').to_string();

    // JWKS fetch and JWT verification are blocking (HTTP + crypto); keep them
    // off the async runtime. DB work below runs on the sqlx pool.
    let (hanko_user_id, email) =
        tokio::task::spawn_blocking(move || -> Result<(String, Option<String>)> {
            let jwks_json = fetch_jwks_cached(&jwk_url)?;

            let header = jsonwebtoken::decode_header(&jtw)
                .map_err(|e| anyhow::anyhow!("JWT header decode: {e}"))?;
            let kid = header.kid.as_deref().unwrap_or("");

            let keys = jwks_json["keys"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("JWKS has no keys array"))?;
            let key = jwk_matching_kid(keys, kid)?;

            let n_b64 = key["n"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("JWK missing n"))?;
            let e_b64 = key["e"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("JWK missing e"))?;
            let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(n_b64, e_b64)
                .map_err(|e| anyhow::anyhow!("decoding key: {e}"))?;

            let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
            validation.set_required_spec_claims(&["exp", "sub"]);
            validation.set_issuer(&[hanko_issuer.as_str()]);

            #[derive(Debug, Deserialize)]
            struct HankoClaims {
                sub: String,
                #[serde(default)]
                email: Option<String>,
            }

            let token_data = jsonwebtoken::decode::<HankoClaims>(&jtw, &decoding_key, &validation)
                .map_err(|e| anyhow::anyhow!("JWT verification: {e}"))?;

            let hanko_user_id = token_data.claims.sub.trim().to_string();
            if hanko_user_id.is_empty() {
                bail!("invalid Hanko session: missing sub");
            }

            let email = nonempty_trimmed_lower(token_data.claims.email.as_deref());
            Ok((hanko_user_id, email))
        })
        .await
        .map_err(|e| ApiError::Internal(format!("hanko session task: {e}")))?
        .map_err(|_| ApiError::Unauthorized("invalid or expired session".into()))?;

    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let account_id = match account_profile::lookup_account_by_hanko(&mut conn, &hanko_user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some(id) => id,
        None => {
            let account_id = uuid::Uuid::new_v4().to_string();
            let username = username_from_hanko_email_or_id(email.as_deref(), &hanko_user_id);
            let username = unique_hanko_username(&mut conn, username, &account_id)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;

            account_profile::insert_account(
                &mut conn,
                &account_id,
                &username,
                None, // Hanko accounts have no local password
                None, // Display name is set later in onboarding
                Some(&hanko_user_id),
                false,
            )
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

            if let Some(email) = &email {
                let _ = account_profile::upsert_account_email(&mut conn, &account_id, email, true)
                    .await;
            }

            account_id
        }
    };

    let response = AuthTokenResponse::for_existing_account(&mut conn, account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(response))
}

/// Self-hosted Try it: session for the shared demo account.
async fn try_demo_self_hosted(conn: &mut AnyConnection) -> Result<AuthTokenResponse, ApiError> {
    if account_profile::username_for_account(conn, account_profile::DEMO_ACCOUNT_ID)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::ServiceUnavailable(
            "demo account is not available; run reset-demo first".into(),
        ));
    }
    AuthTokenResponse::for_existing_account(conn, account_profile::DEMO_ACCOUNT_ID.to_string())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

/// Hosted Try it: take a ready guest, or clone the template when `clone_if_empty`.
async fn try_demo_from_pool(
    conn: &mut AnyConnection,
    cfg: &Config,
    session_secs: u64,
    clone_if_empty: bool,
) -> Result<Option<AuthTokenResponse>, ApiError> {
    if let Some((account_id, username, token)) =
        crate::guest_pool::assign_ready_guest(conn, session_secs)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Ok(Some(AuthTokenResponse {
            token,
            account_id,
            username,
        }));
    }
    if !clone_if_empty {
        return Ok(None);
    }
    let (account_id, username, token) = crate::guest_clone::clone_and_assign_guest(
        conn,
        cfg,
        account_profile::DEMO_ACCOUNT_ID,
        session_secs,
    )
    .await
    .map_err(|e| ApiError::ServiceUnavailable(e.to_string()))?;
    Ok(Some(AuthTokenResponse {
        token,
        account_id,
        username,
    }))
}

fn record_try_demo_assignment(state: &AppState) {
    match state.guest_demand.lock() {
        Ok(mut demand) => demand.record_assignment(),
        Err(_) => {
            eprintln!("guest demand lock poisoned; Try it still succeeded");
        }
    }
}

fn hosted_demo_login_rejected() -> ApiError {
    ApiError::Unauthorized("use Try it to open a sample account".into())
}

/// Open a sample account session: the shared demo account self-hosted, or a
/// private guest copy when the hosted pool is enabled.
#[utoipa::path(
    post,
    path = "/v1/auth/try-demo",
    tag = "Auth",
    responses(
        (status = 200, description = "Session issued", body = AuthTokenResponse),
        (status = 429, description = "Rate limited", body = crate::server::ErrorBody),
        (status = 503, description = "Guest copy unavailable", body = crate::server::ErrorBody)
    )
)]
pub async fn try_demo_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let cf_ip = headers
        .get("cf-connecting-ip")
        .and_then(|value| value.to_str().ok());
    check_try_demo_rate_limits(cf_ip)?;

    if !state.guest.enabled {
        let mut conn = state
            .db
            .acquire()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let result = try_demo_self_hosted(&mut conn).await?;
        return Ok(Json(result));
    }

    let cfg = std::sync::Arc::clone(&state.cfg);
    let session_secs = state.guest.session_secs;
    let assigned = {
        let mut conn = state
            .db
            .acquire()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        try_demo_from_pool(&mut conn, &cfg, session_secs, false).await?
    };

    if let Some(response) = assigned {
        record_try_demo_assignment(&state);
        return Ok(Json(response));
    }

    // Own the lock in a detached task so a 60s client timeout cannot drop the
    // guard while the clone is still running. Dropping the JoinHandle
    // detaches; the lock stays held until the clone returns.
    let lock = state.guest_clone_lock.clone();
    let pool = state.db.clone();
    let cfg = std::sync::Arc::clone(&state.cfg);
    let session_secs = state.guest.session_secs;
    let clone_task = tokio::spawn(async move {
        let _guard = lock.lock().await;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        try_demo_from_pool(&mut conn, &cfg, session_secs, true).await
    });

    match tokio::time::timeout(TRY_DEMO_CLONE_TIMEOUT, clone_task).await {
        Ok(Ok(Ok(Some(response)))) => {
            record_try_demo_assignment(&state);
            Ok(Json(response))
        }
        Ok(Ok(Ok(None))) => Err(ApiError::ServiceUnavailable(
            "guest demo copy is not available".into(),
        )),
        Ok(Ok(Err(err))) => Err(err),
        Ok(Err(join_err)) => Err(ApiError::Internal(format!("try-demo clone: {join_err}"))),
        Err(_) => Err(ApiError::ServiceUnavailable(
            "guest demo copy timed out; try again shortly".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Change-password / delete-account request types
// ---------------------------------------------------------------------------

/// Current and new password.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    /// The account's current password.
    pub current_password: String,
    /// Replacement password.
    pub new_password: String,
}

/// Fresh session token issued after the password change.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChangePasswordResponse {
    /// Always true when a response is returned.
    pub ok: bool,
    /// Replacement session token after password change (previous sessions are revoked).
    pub token: String,
}

/// Confirmation flag and the current password when one is set.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeleteAccountRequest {
    /// Must be `true`; anything else is rejected.
    pub confirm: bool,
    /// Required when the account has a local password.
    #[serde(default)]
    pub current_password: Option<String>,
}

/// Deletion acknowledgement.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteAccountResponse {
    /// Always true when a response is returned.
    pub ok: bool,
}

/// Revocation acknowledgement.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LogoutResponse {
    /// Always true when a response is returned.
    pub ok: bool,
}

/// Check the current password, store `new_hash`, drop named API tokens, and
/// issue a fresh session token. All of that happens in one database transaction
/// so a failure leaves the old credentials in place.
///
/// # Errors
///
/// Returns an error when the current password is wrong or a database write fails.
async fn change_password_on_conn(
    conn: &mut AnyConnection,
    account_id: &str,
    current_password: &str,
    new_hash: &str,
) -> Result<String> {
    let mut tx = conn.begin().await?;
    let current_hash = account_profile::load_password_hash(&mut tx, account_id).await?;
    if !passwords_match(current_hash.as_deref(), current_password) {
        bail!("current password is incorrect");
    }
    account_profile::update_password_hash(&mut tx, account_id, new_hash).await?;
    api_tokens::delete_all_api_tokens(&mut tx, account_id).await?;
    let token = session_tokens::rotate_account_session_token(&mut tx, account_id).await?;
    tx.commit().await?;
    Ok(token)
}

// ---------------------------------------------------------------------------
// Change-password / delete-account / logout handlers
// ---------------------------------------------------------------------------

/// Revoke the session token. Guest accounts are deleted with their data dir.
async fn logout_on_conn(conn: &mut AnyConnection, token: &str, data_dir: &Path) -> Result<()> {
    let account_id = session_tokens::lookup_account_for_token(conn, token).await?;
    let _ = session_tokens::revoke_session_token(conn, token).await?;
    let Some(account_id) = account_id else {
        return Ok(());
    };
    if account_profile::is_guest_account(conn, &account_id).await? {
        account_profile::delete_account(conn, &account_id).await?;
        let dir = data_dir.join(&account_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("remove guest data dir {}", dir.display()))?;
        }
    }
    Ok(())
}

/// Revoke the presented session token. Guest account data is deleted with the
/// session.
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    tag = "Auth",
    security(("bearer" = [])),
    responses(
        (status = 200, body = LogoutResponse),
        (status = 401, body = crate::server::ErrorBody)
    )
)]
pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<LogoutResponse>, ApiError> {
    let token = crate::server::bearer_token(&headers)?;
    let data_dir = state.cfg.paths.data_dir.clone();
    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    schema::ensure_accounts_schema(&mut conn)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    logout_on_conn(&mut conn, &token, &data_dir)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(LogoutResponse { ok: true }))
}

/// Verify the current password, store the new one, revoke API tokens, and
/// issue a fresh session token.
#[utoipa::path(
    post,
    path = "/v1/auth/change-password",
    tag = "Auth",
    security(("bearer" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, body = ChangePasswordResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn change_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiError> {
    let new_password = req.new_password.trim();
    validate_password_policy(new_password)?;
    if req.current_password.len() > MAX_PASSWORD_BYTES {
        return Err(ApiError::BadRequest("password is too long".into()));
    }
    let auth = crate::server::resolve_auth(&headers, &state).await?;
    crate::server::require_full_access(&auth)?;
    crate::server::reject_if_guest_account(&state.cfg.paths.db, &auth.account_id).await?;
    let account_id = auth.account_id;
    let current_password = req.current_password.clone();
    let new_hash = hash_password(new_password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let token = change_password_on_conn(&mut conn, &account_id, &current_password, &new_hash)
        .await
        .map_err(|e| {
            if e.to_string().contains("current password is incorrect") {
                ApiError::BadRequest(e.to_string())
            } else {
                ApiError::Internal(e.to_string())
            }
        })?;

    Ok(Json(ChangePasswordResponse { ok: true, token }))
}

/// Permanently delete the account and its data directory.
#[utoipa::path(
    post,
    path = "/v1/auth/delete-account",
    tag = "Auth",
    security(("bearer" = [])),
    request_body = DeleteAccountRequest,
    responses(
        (status = 200, body = DeleteAccountResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn delete_account_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteAccountRequest>,
) -> Result<Json<DeleteAccountResponse>, ApiError> {
    if !req.confirm {
        return Err(ApiError::BadRequest(
            "confirmation flag must be true".into(),
        ));
    }
    let auth = crate::server::resolve_auth(&headers, &state).await?;
    crate::server::require_full_access(&auth)?;
    let account_id = auth.account_id;
    if account_profile::is_demo_account(&account_id) {
        return Err(ApiError::BadRequest(
            "the demo account cannot be deleted; use reset-demo to restore it".into(),
        ));
    }
    let current_password = req.current_password.clone();
    let account_root = state.cfg.paths.data_dir.join(&account_id);

    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let password_hash = account_profile::load_password_hash(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let has_local_password = matches!(password_hash.as_deref(), Some(hash) if !hash.is_empty());
    if has_local_password {
        let Some(pw) = current_password.as_deref() else {
            return Err(ApiError::BadRequest(
                "current password is required to delete this account".into(),
            ));
        };
        if !passwords_match(password_hash.as_deref(), pw) {
            return Err(ApiError::BadRequest("current password is incorrect".into()));
        }
    }
    account_profile::delete_account(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if account_root.exists() {
        std::fs::remove_dir_all(&account_root)
            .with_context(|| format!("remove account data dir {}", account_root.display()))?;
    }

    Ok(Json(DeleteAccountResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api_tokens::ApiTokenScopes;
    use crate::db::engine;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const OTHER_ACCOUNT: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

    /// Test database with the vault schema applied. The temp dir is returned
    /// too: dropping it deletes the database file out from under the checked-out
    /// connection, after which SQLite rejects writes with SQLITE_READONLY.
    async fn test_conn() -> (tempfile::TempDir, sqlx::pool::PoolConnection<sqlx::Any>) {
        let (pool, dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        (dir, conn)
    }

    async fn password_change_setup() -> (
        tempfile::TempDir,
        sqlx::pool::PoolConnection<sqlx::Any>,
        String,
        Vec<String>,
        String,
    ) {
        let (pool, dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        let old_hash = hash_password("old-password").unwrap();
        account_profile::insert_account(
            &mut conn,
            TEST_ACCOUNT,
            "alice",
            Some(&old_hash),
            None,
            None,
            false,
        )
        .await
        .unwrap();
        account_profile::insert_account(
            &mut conn,
            OTHER_ACCOUNT,
            "bob",
            Some(&old_hash),
            None,
            None,
            false,
        )
        .await
        .unwrap();
        let old_session = session_tokens::insert_account_session_token(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let (_, _, _, _, _, first_api_token) = api_tokens::create_api_token(
            &mut conn,
            TEST_ACCOUNT,
            "backup client",
            ApiTokenScopes::Both,
            None,
        )
        .await
        .unwrap();
        let (_, _, _, _, _, second_api_token) = api_tokens::create_api_token(
            &mut conn,
            TEST_ACCOUNT,
            "export client",
            ApiTokenScopes::Export,
            None,
        )
        .await
        .unwrap();
        let (_, _, _, _, _, other_account_token) = api_tokens::create_api_token(
            &mut conn,
            OTHER_ACCOUNT,
            "other account client",
            ApiTokenScopes::Both,
            None,
        )
        .await
        .unwrap();
        (
            dir,
            conn,
            old_session,
            vec![first_api_token, second_api_token],
            other_account_token,
        )
    }

    #[test]
    fn auth_rate_limit_trips_after_max() {
        let bucket = "test:rate-limit-unique";
        reset_auth_rate_limit_bucket_for_test(bucket);
        for _ in 0..AUTH_RATE_MAX {
            check_auth_rate_limit(bucket).unwrap();
        }
        let err = check_auth_rate_limit(bucket).unwrap_err();
        match err {
            ApiError::TooManyRequests(_) => {}
            other => panic!("expected TooManyRequests, got {other:?}"),
        }
        reset_auth_rate_limit_bucket_for_test(bucket);
    }

    #[test]
    fn try_demo_rate_limit_allows_more_than_login() {
        let bucket = "test:try-demo-rate-limit";
        reset_auth_rate_limit_bucket_for_test(bucket);
        for _ in 0..TRY_DEMO_RATE_MAX {
            check_auth_rate_limit_max(bucket, TRY_DEMO_RATE_MAX).unwrap();
        }
        let err = check_auth_rate_limit_max(bucket, TRY_DEMO_RATE_MAX).unwrap_err();
        match err {
            ApiError::TooManyRequests(_) => {}
            other => panic!("expected TooManyRequests, got {other:?}"),
        }
        reset_auth_rate_limit_bucket_for_test(bucket);
    }

    fn with_try_demo_rate_buckets(f: impl FnOnce()) {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_auth_rate_limit_bucket_for_test("try-demo:203.0.113.10");
        reset_auth_rate_limit_bucket_for_test("try-demo:198.51.100.1");
        reset_auth_rate_limit_bucket_for_test("try-demo");
        f();
        reset_auth_rate_limit_bucket_for_test("try-demo:203.0.113.10");
        reset_auth_rate_limit_bucket_for_test("try-demo:198.51.100.1");
        reset_auth_rate_limit_bucket_for_test("try-demo");
    }

    #[test]
    fn try_demo_per_ip_trips_at_60_and_does_not_block_another_ip() {
        with_try_demo_rate_buckets(|| {
            for _ in 0..TRY_DEMO_PER_IP_RATE_MAX {
                check_try_demo_rate_limits(Some("203.0.113.10")).unwrap();
            }
            match check_try_demo_rate_limits(Some("203.0.113.10")).unwrap_err() {
                ApiError::TooManyRequests(_) => {}
                other => panic!("expected TooManyRequests, got {other:?}"),
            }
            check_try_demo_rate_limits(Some("198.51.100.1")).unwrap();
        });
    }

    #[test]
    fn try_demo_per_ip_429_does_not_increment_global() {
        with_try_demo_rate_buckets(|| {
            for _ in 0..TRY_DEMO_PER_IP_RATE_MAX {
                check_try_demo_rate_limits(Some("203.0.113.10")).unwrap();
            }
            let _ = check_try_demo_rate_limits(Some("203.0.113.10")).unwrap_err();
            // Global saw only the 60 accepts, not the rejected 61st.
            for _ in 0..(TRY_DEMO_RATE_MAX - TRY_DEMO_PER_IP_RATE_MAX) {
                check_auth_rate_limit_max("try-demo", TRY_DEMO_RATE_MAX).unwrap();
            }
            match check_auth_rate_limit_max("try-demo", TRY_DEMO_RATE_MAX).unwrap_err() {
                ApiError::TooManyRequests(_) => {}
                other => panic!("expected TooManyRequests, got {other:?}"),
            }
        });
    }

    #[test]
    fn try_demo_client_key_accepts_single_ipv4_and_ipv6() {
        assert_eq!(
            try_demo_client_key(Some("203.0.113.10")),
            "try-demo:203.0.113.10"
        );
        assert_eq!(
            try_demo_client_key(Some(" 2001:db8::1 ")),
            "try-demo:2001:db8::1"
        );
    }

    #[test]
    fn try_demo_client_key_rejects_missing_list_and_garbage() {
        assert_eq!(try_demo_client_key(None), "try-demo:unknown");
        assert_eq!(try_demo_client_key(Some("")), "try-demo:unknown");
        assert_eq!(
            try_demo_client_key(Some("203.0.113.10, 198.51.100.1")),
            "try-demo:unknown"
        );
        assert_eq!(try_demo_client_key(Some("not-an-ip")), "try-demo:unknown");
    }

    #[tokio::test]
    async fn change_password_transaction_updates_all_credentials() {
        let (_dir, mut conn, old_session, api_tokens, other_account_token) =
            password_change_setup().await;
        let new_hash = hash_password("new-password").unwrap();

        let new_session =
            change_password_on_conn(&mut conn, TEST_ACCOUNT, "old-password", &new_hash)
                .await
                .unwrap();

        let stored_hash = account_profile::load_password_hash(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap()
            .unwrap();
        assert!(passwords_match(Some(&stored_hash), "new-password"));
        assert!(
            session_tokens::lookup_account_for_token(&mut conn, &old_session)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            session_tokens::lookup_account_for_token(&mut conn, &new_session)
                .await
                .unwrap()
                .as_deref(),
            Some(TEST_ACCOUNT)
        );
        for api_token in api_tokens {
            assert!(
                crate::db::api_tokens::lookup_account_for_api_token(&mut conn, &api_token)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(
            crate::db::api_tokens::lookup_account_for_api_token(&mut conn, &other_account_token)
                .await
                .unwrap()
                .unwrap()
                .account_id,
            OTHER_ACCOUNT
        );
    }

    #[test]
    fn reject_demo_password_login_only_when_hosted_demo() {
        assert!(reject_demo_password_login(true, "demo"));
        assert!(reject_demo_password_login(true, "DEMO"));
        assert!(!reject_demo_password_login(true, "alice"));
        assert!(!reject_demo_password_login(false, "demo"));
    }

    #[tokio::test]
    async fn logout_on_conn_deletes_guest_row_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let (_dir, mut conn) = test_conn().await;
        let guest_id = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        account_profile::insert_guest_account(&mut conn, guest_id, "guest-cccc", None)
            .await
            .unwrap();
        account_profile::set_guest_status(&mut conn, guest_id, "assigned")
            .await
            .unwrap();
        let token = session_tokens::insert_account_session_token(&mut conn, guest_id)
            .await
            .unwrap();
        let guest_dir = data_dir.join(guest_id);
        std::fs::create_dir_all(&guest_dir).unwrap();
        std::fs::write(guest_dir.join("marker.txt"), "x").unwrap();

        logout_on_conn(&mut conn, &token, &data_dir).await.unwrap();

        assert!(
            account_profile::username_for_account(&mut conn, guest_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!guest_dir.exists());
        assert!(
            session_tokens::lookup_account_for_token(&mut conn, &token)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn logout_on_conn_leaves_registered_account() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None, None, false)
            .await
            .unwrap();
        let token = session_tokens::insert_account_session_token(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let account_dir = data_dir.join(TEST_ACCOUNT);
        std::fs::create_dir_all(&account_dir).unwrap();

        logout_on_conn(&mut conn, &token, &data_dir).await.unwrap();

        assert_eq!(
            account_profile::username_for_account(&mut conn, TEST_ACCOUNT)
                .await
                .unwrap()
                .as_deref(),
            Some("alice")
        );
        assert!(account_dir.exists());
        assert!(
            session_tokens::lookup_account_for_token(&mut conn, &token)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn try_demo_self_hosted_issues_demo_session() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(
            &mut conn,
            account_profile::DEMO_ACCOUNT_ID,
            "demo",
            None,
            None,
            None,
            true,
        )
        .await
        .unwrap();

        let response = try_demo_self_hosted(&mut conn).await.unwrap();
        assert_eq!(response.account_id, account_profile::DEMO_ACCOUNT_ID);
        assert_eq!(response.username, "demo");
        assert!(response.token.starts_with("mv-user-"));
        assert_eq!(
            session_tokens::lookup_account_for_token(&mut conn, &response.token)
                .await
                .unwrap()
                .as_deref(),
            Some(account_profile::DEMO_ACCOUNT_ID)
        );
    }

    #[tokio::test]
    async fn try_demo_self_hosted_missing_account_is_unavailable() {
        let (_dir, mut conn) = test_conn().await;
        let err = try_demo_self_hosted(&mut conn).await.unwrap_err();
        match err {
            ApiError::ServiceUnavailable(msg) => {
                assert!(
                    msg.to_ascii_lowercase().contains("demo"),
                    "expected a clear demo-account message, got {msg}"
                );
            }
            other => panic!("expected ServiceUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn try_demo_assigns_ready_guest() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        let guest_id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
        account_profile::insert_guest_account(&mut conn, guest_id, "guest-dddd", None)
            .await
            .unwrap();
        let cfg = crate::config::Config {
            paths: crate::config::PathsConfig {
                db: std::path::PathBuf::from(":memory:"),
                data_dir: std::path::PathBuf::from("/tmp"),
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
            database: crate::config::DatabaseConfig::default(),
        };

        let response = try_demo_from_pool(&mut conn, &cfg, 120, false)
            .await
            .unwrap()
            .expect("ready guest");
        assert_eq!(response.account_id, guest_id);
        assert_eq!(response.username, "guest-dddd");
        assert!(response.token.starts_with("mv-user-"));
        assert_eq!(
            account_profile::guest_status(&mut conn, guest_id)
                .await
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
    }

    #[test]
    fn hosted_demo_login_rejected_is_unauthorized() {
        match hosted_demo_login_rejected() {
            ApiError::Unauthorized(msg) => {
                assert_eq!(msg, "use Try it to open a sample account");
            }
            other => panic!("expected Unauthorized, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn try_demo_empty_pool_overlapping_clones_both_get_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("vault.db");
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (pool, _dir) = engine::test_pool().await;
        {
            let mut conn = pool.acquire().await.unwrap();
            schema::ensure_vault_schema(&mut conn).await.unwrap();
            account_profile::insert_account(
                &mut conn,
                account_profile::DEMO_ACCOUNT_ID,
                "demo",
                None,
                None,
                None,
                true,
            )
            .await
            .unwrap();
        }
        let cfg = crate::config::Config {
            paths: crate::config::PathsConfig {
                db: db.clone(),
                data_dir,
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
            database: crate::config::DatabaseConfig::default(),
        };

        let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        // Production serializes clones with `AppState::guest_clone_lock`; the
        // plain clone transaction cannot retry a second concurrent writer once
        // it has read from a stale snapshot, so mirror that lock here.
        let clone_lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let pool = pool.clone();
            let cfg = cfg.clone();
            let results = std::sync::Arc::clone(&results);
            let clone_lock = std::sync::Arc::clone(&clone_lock);
            handles.push(tokio::spawn(async move {
                let _guard = clone_lock.lock().await;
                let mut conn = pool.acquire().await.unwrap();
                let assigned = try_demo_from_pool(&mut conn, &cfg, 120, true)
                    .await
                    .unwrap();
                results.lock().unwrap().push(assigned);
            }));
        }
        for handle in handles {
            handle.await.expect("clone task");
        }
        let got = results.lock().unwrap();
        let responses: Vec<_> = got
            .iter()
            .map(|row| row.as_ref().expect("empty-pool Try it returned no session"))
            .collect();
        assert_eq!(responses.len(), 2);
        assert_ne!(
            responses[0].account_id, responses[1].account_id,
            "overlapping empty-pool Try it shared a guest"
        );
        assert_ne!(responses[0].token, responses[1].token);
    }

    #[tokio::test]
    async fn change_password_transaction_rolls_back_every_credential() {
        let (_dir, mut conn, old_session, api_tokens, other_account_token) =
            password_change_setup().await;
        sqlx::query(
            "CREATE TRIGGER fail_session_rotation
             BEFORE UPDATE ON account_session_tokens
             BEGIN
                 SELECT RAISE(FAIL, 'injected session rotation failure');
             END",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let new_hash = hash_password("new-password").unwrap();

        assert!(
            change_password_on_conn(&mut conn, TEST_ACCOUNT, "old-password", &new_hash)
                .await
                .is_err()
        );

        let stored_hash = account_profile::load_password_hash(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap()
            .unwrap();
        assert!(passwords_match(Some(&stored_hash), "old-password"));
        assert_eq!(
            session_tokens::lookup_account_for_token(&mut conn, &old_session)
                .await
                .unwrap()
                .as_deref(),
            Some(TEST_ACCOUNT)
        );
        for api_token in api_tokens {
            assert!(
                crate::db::api_tokens::lookup_account_for_api_token(&mut conn, &api_token)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        assert_eq!(
            crate::db::api_tokens::lookup_account_for_api_token(&mut conn, &other_account_token)
                .await
                .unwrap()
                .unwrap()
                .account_id,
            OTHER_ACCOUNT
        );
    }
}
