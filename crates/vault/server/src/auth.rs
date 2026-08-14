//! Authentication handlers: register, login, and Hanko session exchange.
//!
//! All three return a Bearer API token the rest of the API already accepts.
//! There is no separate session layer — these are additional ways to get a
//! token. Hanko is an external sign-in service. A Hanko session is a signed
//! JSON Web Token (a signed claim of who the user is) that this server checks
//! and then exchanges for a vault token.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::http::HeaderMap;
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::db::{account_profile, api_tokens, schema, session_tokens};
use crate::server::{ApiError, AppState, JoinBlocking};

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
const JWKS_CACHE_TTL: Duration = Duration::from_secs(300);
const JWKS_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

static AUTH_RATE_LIMITS: Mutex<Option<HashMap<String, VecDeque<Instant>>>> = Mutex::new(None);
static JWKS_CACHE: Mutex<Option<(String, Instant, serde_json::Value)>> = Mutex::new(None);
static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();

/// Reject when `bucket` has seen more than [`AUTH_RATE_MAX`] hits in [`AUTH_RATE_WINDOW`].
fn check_auth_rate_limit(bucket: &str) -> Result<(), ApiError> {
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
    if entry.len() >= AUTH_RATE_MAX {
        return Err(ApiError::TooManyRequests(
            "too many authentication attempts; try again shortly".into(),
        ));
    }
    entry.push_back(now);
    Ok(())
}

#[cfg(test)]
fn reset_auth_rate_limits_for_test() {
    if let Ok(mut guard) = AUTH_RATE_LIMITS.lock() {
        *guard = None;
    }
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub preferred_name: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct HankoSessionRequest {
    /// The raw Hanko session JSON Web Token from the client-side
    /// `onSessionCreated` callback.
    pub hanko_jwt: String,
}

#[derive(Debug, Serialize)]
pub struct AuthTokenResponse {
    pub token: String,
    pub account_id: String,
    pub username: String,
}

impl AuthTokenResponse {
    /// Issue (or reuse) the session token for an existing account. Uses the
    /// account id when the row has no username.
    fn for_existing_account(
        conn: &rusqlite::Connection,
        account_id: String,
    ) -> Result<AuthTokenResponse> {
        let token = session_tokens::get_or_create_session_token(conn, &account_id)?;
        let username = account_profile::username_for_account(conn, &account_id)?
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
    let salt = SaltString::generate(&mut rand::thread_rng());
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
    let Some(raw) = value else {
        return None;
    };
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

fn unique_hanko_username(
    conn: &rusqlite::Connection,
    username: String,
    account_id: &str,
) -> Result<String> {
    if account_profile::lookup_account_ref(conn, &username)?.is_some() {
        Ok(format!("{}_{}", username, &account_id[..8]))
    } else {
        Ok(username)
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /v1/auth/register` — create an account and return an API token.
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

    let db = state.cfg.paths.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
        let mut conn = schema::open_configured(&db)?;
        let tx = conn.transaction()?;

        if account_profile::lookup_account_ref(&tx, &username)?.is_some() {
            bail!("username already taken: {username}");
        }

        account_profile::insert_account(
            &tx,
            &account_id,
            &username,
            password_hash.as_deref(),
            preferred_name.as_deref(),
            None,  // hanko_user_id
            false, // read_only
        )?;

        if let Some(ref phone) = phone {
            account_profile::upsert_account_phone(&tx, &account_id, phone)?;
        }

        let token = session_tokens::insert_account_session_token(&tx, &account_id)?;
        tx.commit()?;
        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
        })
    })
    .await
    .join_map("register task", |e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(result))
}

/// `POST /v1/auth/login` — authenticate with username + password, return an API token.
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
    let db = state.cfg.paths.db.clone();
    let guest_enabled = state.guest.enabled;

    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
        let conn = schema::open_configured(&db)?;

        let Some(account_id) = account_profile::lookup_account_ref(&conn, &username)? else {
            let _ = verify_password(dummy_password_hash(), &password);
            bail!("invalid username or password");
        };

        if reject_demo_password_login(guest_enabled, &username) {
            bail!("use Try it to open a sample account");
        }

        let password_hash = account_profile::load_password_hash(&conn, &account_id)?;
        if !verify_login_password(password_hash.as_deref(), &password) {
            bail!("invalid username or password");
        }

        AuthTokenResponse::for_existing_account(&conn, account_id)
    })
    .await
    .join_map("login task", |e| {
        let msg = e.to_string();
        if msg.contains("use Try it") {
            ApiError::Unauthorized(msg)
        } else {
            ApiError::Unauthorized("invalid username or password".into())
        }
    })?;

    Ok(Json(result))
}

/// `POST /v1/auth/hanko/session` — check a Hanko session JSON Web Token and
/// return a vault API token.
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
    let db = state.cfg.paths.db.clone();
    let jtw = req.hanko_jwt.clone();
    let hanko_issuer = hanko_api_url.trim_end_matches('/').to_string();

    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
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

        let conn = schema::open_configured(&db)?;

        let account_id = match account_profile::lookup_account_by_hanko(&conn, &hanko_user_id)? {
            Some(id) => id,
            None => {
                let account_id = uuid::Uuid::new_v4().to_string();
                let username = username_from_hanko_email_or_id(email.as_deref(), &hanko_user_id);
                let username = unique_hanko_username(&conn, username, &account_id)?;

                account_profile::insert_account(
                    &conn,
                    &account_id,
                    &username,
                    None, // Hanko accounts have no local password
                    None, // Display name is set later in onboarding
                    Some(&hanko_user_id),
                    false,
                )?;

                if let Some(email) = &email {
                    let _ = account_profile::upsert_account_email(&conn, &account_id, email, true);
                }

                account_id
            }
        };

        AuthTokenResponse::for_existing_account(&conn, account_id)
    })
    .await
    .join_map("hanko session task", |_| {
        ApiError::Unauthorized("invalid or expired session".into())
    })?;

    Ok(Json(result))
}

/// Self-hosted Try it: session for the shared demo account.
fn try_demo_self_hosted(conn: &rusqlite::Connection) -> Result<AuthTokenResponse, ApiError> {
    if account_profile::username_for_account(conn, account_profile::DEMO_ACCOUNT_ID)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::ServiceUnavailable(
            "demo account is not available; run reset-demo first".into(),
        ));
    }
    AuthTokenResponse::for_existing_account(conn, account_profile::DEMO_ACCOUNT_ID.to_string())
        .map_err(|e| ApiError::Internal(e.to_string()))
}

/// Hosted Try it: take a ready guest, or clone the template when `clone_if_empty`.
fn try_demo_from_pool(
    conn: &mut rusqlite::Connection,
    cfg: &Config,
    session_secs: u64,
    clone_if_empty: bool,
) -> Result<Option<AuthTokenResponse>, ApiError> {
    if let Some((account_id, username, token)) =
        crate::guest_pool::assign_ready_guest(conn, session_secs)
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
    crate::guest_clone::clone_template_to_guest(conn, cfg, account_profile::DEMO_ACCOUNT_ID)
        .map_err(|e| ApiError::ServiceUnavailable(e.to_string()))?;
    let Some((account_id, username, token)) =
        crate::guest_pool::assign_ready_guest(conn, session_secs)
            .map_err(|e| ApiError::Internal(e.to_string()))?
    else {
        return Err(ApiError::ServiceUnavailable(
            "guest demo copy is not available".into(),
        ));
    };
    Ok(Some(AuthTokenResponse {
        token,
        account_id,
        username,
    }))
}

/// `POST /v1/auth/try-demo` — self-hosted demo session, or a private guest copy.
pub async fn try_demo_handler(
    State(state): State<AppState>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    check_auth_rate_limit("try-demo")?;

    if !state.guest.enabled {
        let db = state.cfg.paths.db.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse, ApiError> {
            let conn =
                schema::open_configured(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
            try_demo_self_hosted(&conn)
        })
        .await
        .join_map("try-demo self-hosted", |e| e)?;
        return Ok(Json(result));
    }

    let db = state.cfg.paths.db.clone();
    let cfg = std::sync::Arc::clone(&state.cfg);
    let session_secs = state.guest.session_secs;
    let assigned =
        tokio::task::spawn_blocking(move || -> Result<Option<AuthTokenResponse>, ApiError> {
            let mut conn =
                schema::open_configured(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
            try_demo_from_pool(&mut conn, &cfg, session_secs, false)
        })
        .await
        .join_map("try-demo assign", |e| e)?;

    if let Some(response) = assigned {
        state.guest_demand.lock().unwrap().record_assignment();
        return Ok(Json(response));
    }

    // Own the lock in a detached task so a 60s client timeout cannot drop the
    // guard while `spawn_blocking` is still cloning. Dropping the JoinHandle
    // detaches; the lock stays held until the clone returns.
    let lock = state.guest_clone_lock.clone();
    let db = state.cfg.paths.db.clone();
    let cfg = std::sync::Arc::clone(&state.cfg);
    let session_secs = state.guest.session_secs;
    let clone_task = tokio::spawn(async move {
        let _guard = lock.lock().await;
        tokio::task::spawn_blocking(move || -> Result<Option<AuthTokenResponse>, ApiError> {
            let mut conn =
                schema::open_configured(&db).map_err(|e| ApiError::Internal(e.to_string()))?;
            try_demo_from_pool(&mut conn, &cfg, session_secs, true)
        })
        .await
        .join_map("try-demo clone", |e| e)
    });

    match tokio::time::timeout(TRY_DEMO_CLONE_TIMEOUT, clone_task).await {
        Ok(Ok(Ok(Some(response)))) => {
            state.guest_demand.lock().unwrap().record_assignment();
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

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub ok: bool,
    /// Replacement session token after password change (previous sessions are revoked).
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    pub confirm: bool,
    /// Required when the account has a local password.
    #[serde(default)]
    pub current_password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteAccountResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub ok: bool,
}

/// Check the current password, store `new_hash`, drop named API tokens, and
/// issue a fresh session token. All of that happens in one database transaction
/// so a failure leaves the old credentials in place.
///
/// # Errors
///
/// Returns an error when the current password is wrong or a database write fails.
fn change_password_on_conn(
    conn: &mut rusqlite::Connection,
    account_id: &str,
    current_password: &str,
    new_hash: &str,
) -> Result<String> {
    let tx = conn.transaction()?;
    let current_hash = account_profile::load_password_hash(&tx, account_id)?;
    if !passwords_match(current_hash.as_deref(), current_password) {
        bail!("current password is incorrect");
    }
    account_profile::update_password_hash(&tx, account_id, new_hash)?;
    api_tokens::delete_all_api_tokens(&tx, account_id)?;
    let token = session_tokens::rotate_account_session_token(&tx, account_id)?;
    tx.commit()?;
    Ok(token)
}

// ---------------------------------------------------------------------------
// Change-password / delete-account / logout handlers
// ---------------------------------------------------------------------------

/// Revoke the session token. Guest accounts are deleted with their data dir.
fn logout_on_conn(conn: &rusqlite::Connection, token: &str, data_dir: &Path) -> Result<()> {
    let account_id = session_tokens::lookup_account_for_token(conn, token)?;
    let _ = session_tokens::revoke_session_token(conn, token)?;
    let Some(account_id) = account_id else {
        return Ok(());
    };
    if account_profile::is_guest_account(conn, &account_id)? {
        account_profile::delete_account(conn, &account_id)?;
        let dir = data_dir.join(&account_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("remove guest data dir {}", dir.display()))?;
        }
    }
    Ok(())
}

/// `POST /v1/auth/logout` — revoke the presented session token.
pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<LogoutResponse>, ApiError> {
    let token = crate::server::bearer_token(&headers)?;
    let db = state.cfg.paths.db.clone();
    let data_dir = state.cfg.paths.data_dir.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = schema::open_configured(&db)?;
        schema::ensure_accounts_schema(&conn)?;
        logout_on_conn(&conn, &token, &data_dir)
    })
    .await
    .join_blocking("logout task")?;
    Ok(Json(LogoutResponse { ok: true }))
}

/// `POST /v1/auth/change-password` — verify the current password, set a new one.
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
    let db = state.cfg.paths.db.clone();
    let new_hash = hash_password(new_password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let token = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let mut conn = schema::open_configured(&db)?;
        change_password_on_conn(&mut conn, &account_id, &current_password, &new_hash)
    })
    .await
    .join_map("change password task", |e| {
        if e.to_string().contains("current password is incorrect") {
            ApiError::BadRequest(e.to_string())
        } else {
            ApiError::Internal(e.to_string())
        }
    })?;

    Ok(Json(ChangePasswordResponse { ok: true, token }))
}

/// `POST /v1/auth/delete-account` — permanently delete the account.
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
    let db = state.cfg.paths.db.clone();
    let account_root = state.cfg.paths.data_dir.join(&account_id);

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = schema::open_configured(&db)?;
        let password_hash = account_profile::load_password_hash(&conn, &account_id)?;
        let has_local_password = match password_hash.as_deref() {
            Some(hash) if !hash.is_empty() => true,
            _ => false,
        };
        if has_local_password {
            let Some(pw) = current_password.as_deref() else {
                bail!("current password is required to delete this account");
            };
            if !passwords_match(password_hash.as_deref(), pw) {
                bail!("current password is incorrect");
            }
        }
        account_profile::delete_account(&conn, &account_id)?;
        if account_root.exists() {
            std::fs::remove_dir_all(&account_root)
                .with_context(|| format!("remove account data dir {}", account_root.display()))?;
        }
        Ok(())
    })
    .await
    .join_map("delete account task", |e| {
        let msg = e.to_string();
        if msg.contains("current password") {
            ApiError::BadRequest(msg)
        } else {
            ApiError::Internal(msg)
        }
    })?;

    Ok(Json(DeleteAccountResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api_tokens::ApiTokenScopes;
    use rusqlite::Connection;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const OTHER_ACCOUNT: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

    fn password_change_setup() -> (Connection, String, Vec<String>, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        let old_hash = hash_password("old-password").unwrap();
        account_profile::insert_account(
            &conn,
            TEST_ACCOUNT,
            "alice",
            Some(&old_hash),
            None,
            None,
            false,
        )
        .unwrap();
        account_profile::insert_account(
            &conn,
            OTHER_ACCOUNT,
            "bob",
            Some(&old_hash),
            None,
            None,
            false,
        )
        .unwrap();
        let old_session =
            session_tokens::insert_account_session_token(&conn, TEST_ACCOUNT).unwrap();
        let (_, _, _, _, _, first_api_token) = api_tokens::create_api_token(
            &conn,
            TEST_ACCOUNT,
            "backup client",
            ApiTokenScopes::Both,
            None,
        )
        .unwrap();
        let (_, _, _, _, _, second_api_token) = api_tokens::create_api_token(
            &conn,
            TEST_ACCOUNT,
            "export client",
            ApiTokenScopes::Export,
            None,
        )
        .unwrap();
        let (_, _, _, _, _, other_account_token) = api_tokens::create_api_token(
            &conn,
            OTHER_ACCOUNT,
            "other account client",
            ApiTokenScopes::Both,
            None,
        )
        .unwrap();
        (
            conn,
            old_session,
            vec![first_api_token, second_api_token],
            other_account_token,
        )
    }

    #[test]
    fn auth_rate_limit_trips_after_max() {
        reset_auth_rate_limits_for_test();
        let bucket = "test:rate-limit-unique";
        for _ in 0..AUTH_RATE_MAX {
            check_auth_rate_limit(bucket).unwrap();
        }
        let err = check_auth_rate_limit(bucket).unwrap_err();
        match err {
            ApiError::TooManyRequests(_) => {}
            other => panic!("expected TooManyRequests, got {other:?}"),
        }
        reset_auth_rate_limits_for_test();
    }

    #[test]
    fn change_password_transaction_updates_all_credentials() {
        let (mut conn, old_session, api_tokens, other_account_token) = password_change_setup();
        let new_hash = hash_password("new-password").unwrap();

        let new_session =
            change_password_on_conn(&mut conn, TEST_ACCOUNT, "old-password", &new_hash).unwrap();

        let stored_hash = account_profile::load_password_hash(&conn, TEST_ACCOUNT)
            .unwrap()
            .unwrap();
        assert!(passwords_match(Some(&stored_hash), "new-password"));
        assert!(
            session_tokens::lookup_account_for_token(&conn, &old_session)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            session_tokens::lookup_account_for_token(&conn, &new_session)
                .unwrap()
                .as_deref(),
            Some(TEST_ACCOUNT)
        );
        for api_token in api_tokens {
            assert!(
                crate::db::api_tokens::lookup_account_for_api_token(&conn, &api_token)
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(
            crate::db::api_tokens::lookup_account_for_api_token(&conn, &other_account_token)
                .unwrap()
                .unwrap()
                .account_id,
            OTHER_ACCOUNT
        );
    }

    fn memory_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn reject_demo_password_login_only_when_hosted_demo() {
        assert!(reject_demo_password_login(true, "demo"));
        assert!(reject_demo_password_login(true, "DEMO"));
        assert!(!reject_demo_password_login(true, "alice"));
        assert!(!reject_demo_password_login(false, "demo"));
    }

    #[test]
    fn logout_on_conn_deletes_guest_row_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let conn = memory_conn();
        let guest_id = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        account_profile::insert_guest_account(&conn, guest_id, "guest-cccc", None).unwrap();
        account_profile::set_guest_status(&conn, guest_id, "assigned").unwrap();
        let token = session_tokens::insert_account_session_token(&conn, guest_id).unwrap();
        let guest_dir = data_dir.join(guest_id);
        std::fs::create_dir_all(&guest_dir).unwrap();
        std::fs::write(guest_dir.join("marker.txt"), "x").unwrap();

        logout_on_conn(&conn, &token, &data_dir).unwrap();

        assert!(
            account_profile::username_for_account(&conn, guest_id)
                .unwrap()
                .is_none()
        );
        assert!(!guest_dir.exists());
        assert!(
            session_tokens::lookup_account_for_token(&conn, &token)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn logout_on_conn_leaves_registered_account() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let conn = memory_conn();
        account_profile::insert_account(&conn, TEST_ACCOUNT, "alice", None, None, None, false)
            .unwrap();
        let token = session_tokens::insert_account_session_token(&conn, TEST_ACCOUNT).unwrap();
        let account_dir = data_dir.join(TEST_ACCOUNT);
        std::fs::create_dir_all(&account_dir).unwrap();

        logout_on_conn(&conn, &token, &data_dir).unwrap();

        assert_eq!(
            account_profile::username_for_account(&conn, TEST_ACCOUNT)
                .unwrap()
                .as_deref(),
            Some("alice")
        );
        assert!(account_dir.exists());
        assert!(
            session_tokens::lookup_account_for_token(&conn, &token)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn try_demo_self_hosted_issues_demo_session() {
        let conn = memory_conn();
        account_profile::insert_account(
            &conn,
            account_profile::DEMO_ACCOUNT_ID,
            "demo",
            None,
            None,
            None,
            true,
        )
        .unwrap();

        let response = try_demo_self_hosted(&conn).unwrap();
        assert_eq!(response.account_id, account_profile::DEMO_ACCOUNT_ID);
        assert_eq!(response.username, "demo");
        assert!(response.token.starts_with("mv-user-"));
        assert_eq!(
            session_tokens::lookup_account_for_token(&conn, &response.token)
                .unwrap()
                .as_deref(),
            Some(account_profile::DEMO_ACCOUNT_ID)
        );
    }

    #[test]
    fn try_demo_self_hosted_missing_account_is_unavailable() {
        let conn = memory_conn();
        let err = try_demo_self_hosted(&conn).unwrap_err();
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

    #[test]
    fn try_demo_assigns_ready_guest() {
        let mut conn = memory_conn();
        let guest_id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
        account_profile::insert_guest_account(&conn, guest_id, "guest-dddd", None).unwrap();
        let cfg = crate::config::Config {
            paths: crate::config::PathsConfig {
                db: std::path::PathBuf::from(":memory:"),
                data_dir: std::path::PathBuf::from("/tmp"),
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
        };

        let response = try_demo_from_pool(&mut conn, &cfg, 120, false)
            .unwrap()
            .expect("ready guest");
        assert_eq!(response.account_id, guest_id);
        assert_eq!(response.username, "guest-dddd");
        assert!(response.token.starts_with("mv-user-"));
        assert_eq!(
            account_profile::guest_status(&conn, guest_id)
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
    }

    #[test]
    fn change_password_transaction_rolls_back_every_credential() {
        let (mut conn, old_session, api_tokens, other_account_token) = password_change_setup();
        conn.execute_batch(
            "CREATE TRIGGER fail_session_rotation
             BEFORE UPDATE ON account_session_tokens
             BEGIN
                 SELECT RAISE(FAIL, 'injected session rotation failure');
             END;",
        )
        .unwrap();
        let new_hash = hash_password("new-password").unwrap();

        assert!(
            change_password_on_conn(&mut conn, TEST_ACCOUNT, "old-password", &new_hash).is_err()
        );

        let stored_hash = account_profile::load_password_hash(&conn, TEST_ACCOUNT)
            .unwrap()
            .unwrap();
        assert!(passwords_match(Some(&stored_hash), "old-password"));
        assert_eq!(
            session_tokens::lookup_account_for_token(&conn, &old_session)
                .unwrap()
                .as_deref(),
            Some(TEST_ACCOUNT)
        );
        for api_token in api_tokens {
            assert!(
                crate::db::api_tokens::lookup_account_for_api_token(&conn, &api_token)
                    .unwrap()
                    .is_some()
            );
        }
        assert_eq!(
            crate::db::api_tokens::lookup_account_for_api_token(&conn, &other_account_token)
                .unwrap()
                .unwrap()
                .account_id,
            OTHER_ACCOUNT
        );
    }
}
