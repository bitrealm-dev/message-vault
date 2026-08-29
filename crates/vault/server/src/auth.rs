//! Authentication handlers: register, login, session check, and logout.

use std::collections::{HashMap, VecDeque};
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

use crate::db::{account_profile, api_tokens, schema, session_tokens};
use crate::dedupe;
use crate::server::{ApiError, AppState, nonempty_query_account, resolve_auth};

/// Max password bytes accepted before hashing (registration / login / change).
const MAX_PASSWORD_BYTES: usize = 1024;
const MIN_PASSWORD_CHARS: usize = 8;
/// Sliding window for unauthenticated auth endpoints.
const AUTH_RATE_WINDOW: Duration = Duration::from_secs(60);
const AUTH_RATE_MAX: usize = 20;

static AUTH_RATE_LIMITS: Mutex<Option<HashMap<String, VecDeque<Instant>>>> = Mutex::new(None);
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

#[cfg(test)]
pub(crate) fn reset_auth_rate_limit_bucket_for_test(bucket: &str) {
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
pub(crate) fn hash_password(password: &str) -> Result<String> {
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
pub(crate) fn validate_password_policy(password: &str) -> Result<(), ApiError> {
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

// ---------------------------------------------------------------------------
// Username validation
// ---------------------------------------------------------------------------

pub(crate) fn normalize_username(raw: &str) -> String {
    raw.trim().to_string()
}

pub(crate) fn is_valid_username(s: &str) -> bool {
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

    let first_account = account_profile::vault_has_no_real_accounts(&mut tx)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    account_profile::insert_account(
        &mut tx,
        &account_id,
        &username,
        password_hash.as_deref(),
        preferred_name.as_deref(),
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if first_account {
        account_profile::set_admin(&mut tx, &account_id, true)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

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
        (status = 403, description = "Account is disabled", body = crate::server::ErrorBody),
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

    let password_hash = account_profile::load_password_hash(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !verify_login_password(password_hash.as_deref(), &password) {
        return Err(ApiError::Unauthorized(
            "invalid username or password".into(),
        ));
    }

    let auth = account_profile::load_account_auth(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("invalid username or password".into()))?;
    if auth.disabled {
        return Err(ApiError::Forbidden("this account is disabled".into()));
    }

    let response = AuthTokenResponse::for_existing_account(&mut conn, account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(response))
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

/// Revoke the session token.
async fn logout_on_conn(conn: &mut AnyConnection, token: &str) -> Result<()> {
    let _ = session_tokens::revoke_session_token(conn, token).await?;
    Ok(())
}

/// Revoke the presented session token.
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
    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    schema::ensure_accounts_schema(&mut conn)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    logout_on_conn(&mut conn, &token)
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
        let root = account_root.clone();
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&root))
            .await
            .map_err(|e| ApiError::Internal(format!("remove account data dir task: {e}")))?
            .with_context(|| format!("remove account data dir {}", account_root.display()))?;
    }

    Ok(Json(DeleteAccountResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::engine;
    use crate::db::permissions::Permissions;
    use crate::test_support::*;
    use axum::http::StatusCode;

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

    #[tokio::test]
    async fn first_real_account_becomes_admin_and_second_does_not() {
        let (_dir, mut conn) = test_conn().await;

        // The demo account exists first and must not count.
        account_profile::insert_account(
            &mut conn,
            account_profile::DEMO_ACCOUNT_ID,
            "demo",
            None,
            None,
        )
        .await
        .unwrap();
        assert!(
            account_profile::vault_has_no_real_accounts(&mut conn)
                .await
                .unwrap(),
            "the demo account must not occupy first place"
        );

        account_profile::insert_account(&mut conn, "acct-1", "alice", None, None)
            .await
            .unwrap();
        assert!(
            !account_profile::vault_has_no_real_accounts(&mut conn)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn last_admin_is_protected() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, "acct-1", "alice", None, None)
            .await
            .unwrap();
        account_profile::set_admin(&mut conn, "acct-1", true)
            .await
            .unwrap();
        account_profile::insert_account(&mut conn, "acct-2", "bob", None, None)
            .await
            .unwrap();

        assert!(
            account_profile::is_last_admin(&mut conn, "acct-1")
                .await
                .unwrap()
        );
        assert!(
            !account_profile::is_last_admin(&mut conn, "acct-2")
                .await
                .unwrap()
        );

        account_profile::set_admin(&mut conn, "acct-2", true)
            .await
            .unwrap();
        assert!(
            !account_profile::is_last_admin(&mut conn, "acct-1")
                .await
                .unwrap(),
            "with two admins neither is the last"
        );
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
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", Some(&old_hash), None)
            .await
            .unwrap();
        account_profile::insert_account(&mut conn, OTHER_ACCOUNT, "bob", Some(&old_hash), None)
            .await
            .unwrap();
        let old_session = session_tokens::insert_account_session_token(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let (_, _, _, _, _, first_api_token) = api_tokens::create_api_token(
            &mut conn,
            TEST_ACCOUNT,
            "backup client",
            Permissions::all(),
            None,
        )
        .await
        .unwrap();
        let (_, _, _, _, _, second_api_token) = api_tokens::create_api_token(
            &mut conn,
            TEST_ACCOUNT,
            "export client",
            Permissions {
                import: false,
                export: true,
                delete: false,
            },
            None,
        )
        .await
        .unwrap();
        let (_, _, _, _, _, other_account_token) = api_tokens::create_api_token(
            &mut conn,
            OTHER_ACCOUNT,
            "other account client",
            Permissions::all(),
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

    #[tokio::test]
    async fn logout_on_conn_leaves_registered_account() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None)
            .await
            .unwrap();
        let token = session_tokens::insert_account_session_token(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();

        logout_on_conn(&mut conn, &token).await.unwrap();

        assert_eq!(
            account_profile::username_for_account(&mut conn, TEST_ACCOUNT)
                .await
                .unwrap()
                .as_deref(),
            Some("alice")
        );
        assert!(
            session_tokens::lookup_account_for_token(&mut conn, &token)
                .await
                .unwrap()
                .is_none()
        );
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

    #[tokio::test]
    async fn register_grants_admin_to_the_first_user_only() {
        let vault = test_vault().await;
        let state = vault.state.clone();

        let first = register_via_api(&state, "alice", "hunter2hunter2").await;
        let second = register_via_api(&state, "bob", "hunter2hunter2").await;

        let mut conn = state.db.acquire().await.unwrap();
        assert!(
            account_profile::load_account_auth(&mut conn, &first.account_id)
                .await
                .unwrap()
                .unwrap()
                .is_admin
        );
        assert!(
            !account_profile::load_account_auth(&mut conn, &second.account_id)
                .await
                .unwrap()
                .unwrap()
                .is_admin
        );
    }

    #[tokio::test]
    async fn disabled_account_cannot_sign_in() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let created = register_via_api(&state, "alice", "hunter2hunter2").await;

        let mut conn = state.db.acquire().await.unwrap();
        sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = $1")
            .bind(&created.account_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let status = login_status(&state, "alice", "hunter2hunter2").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}
