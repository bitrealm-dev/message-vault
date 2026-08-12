//! Authentication handlers: register, login, and Hanko session exchange.
//!
//! All three return a Bearer API token the rest of the API already accepts.
//! No new session layer — just new ways to *get* a token.

use anyhow::{Context, Result, bail};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::http::HeaderMap;
use axum::{Json, extract::State};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{account_profile, schema, session_tokens};
use crate::server::{ApiError, AppState};

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
    /// The raw Hanko session JWT from the client-side `onSessionCreated` callback.
    pub hanko_jwt: String,
}

#[derive(Debug, Serialize)]
pub struct AuthTokenResponse {
    pub token: String,
    pub account_id: String,
    pub username: String,
}

// ---------------------------------------------------------------------------
// Password helpers
// ---------------------------------------------------------------------------

/// Hash a plaintext password with argon2id.
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

/// Authenticate: null/empty hash → empty password accepted;
/// otherwise argon2 verify.
fn passwords_match(password_hash: Option<&str>, password: &str) -> bool {
    match password_hash {
        None | Some("") => password.is_empty(),
        Some(hash) => verify_password(hash, password),
    }
}

// ---------------------------------------------------------------------------
// Username validation
// ---------------------------------------------------------------------------

fn normalize_username(raw: &str) -> String {
    raw.trim().to_string()
}

fn is_valid_username(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
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

    let password_plain = req.password.as_deref().unwrap_or("").to_string();
    let password_hash: Option<String> = if password_plain.is_empty() {
        None
    } else {
        Some(hash_password(&password_plain).map_err(|e| ApiError::Internal(e.to_string()))?)
    };

    let preferred_name = req
        .preferred_name
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let phone: Option<String> = req
        .phone
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let account_id = uuid::Uuid::new_v4().to_string();

    let db = state.cfg.paths.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;

        if account_profile::lookup_account_ref(&conn, &username)?.is_some() {
            bail!("username already taken: {username}");
        }

        account_profile::insert_account(
            &conn,
            &account_id,
            &username,
            password_hash.as_deref(),
            preferred_name.as_deref(),
            None,  // hanko_user_id
            false, // read_only
        )?;

        if let Some(ref phone) = phone {
            account_profile::upsert_account_phone(&conn, &account_id, phone)?;
        }

        let token = session_tokens::insert_account_session_token(&conn, &account_id)?;
        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(format!("register task: {e}")))?
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

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

    let password = req.password.clone();
    let db = state.cfg.paths.db.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;

        let account_id = account_profile::lookup_account_ref(&conn, &username)?
            .ok_or_else(|| anyhow::anyhow!("account not found: {username}"))?;

        let password_hash = account_profile::load_password_hash(&conn, &account_id)?;

        if !passwords_match(password_hash.as_deref(), &password) {
            bail!("invalid password");
        }

        let token = session_tokens::get_or_create_session_token(&conn, &account_id)?;
        let username = account_profile::username_for_account(&conn, &account_id)?
            .unwrap_or_else(|| account_id.clone());

        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(format!("login task: {e}")))?
    .map_err(|e| ApiError::Unauthorized(e.to_string()))?;

    Ok(Json(result))
}

/// `POST /v1/auth/hanko/session` — verify a Hanko session JWT and return a vault API token.
pub async fn hanko_session_handler(
    State(state): State<AppState>,
    Json(req): Json<HankoSessionRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let hanko_api_url = std::env::var("HANKO_API_URL")
        .ok()
        .or_else(|| std::env::var("NEXT_PUBLIC_HANKO_API_URL").ok())
        .unwrap_or_default();

    if hanko_api_url.is_empty() {
        return Err(ApiError::Internal("HANKO_API_URL is not configured".into()));
    }

    let jwk_url = format!(
        "{}/.well-known/jwks.json",
        hanko_api_url.trim_end_matches('/')
    );
    let db = state.cfg.paths.db.clone();
    let jtw = req.hanko_jwt.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
        // Fetch JWKS (blocking HTTP in spawn_blocking)
        let jwks_json: serde_json::Value = reqwest::blocking::get(&jwk_url)
            .with_context(|| format!("failed to fetch JWKS from {jwk_url}"))?
            .json()
            .with_context(|| "failed to parse JWKS")?;

        // Decode JWT header to get kid
        let header = jsonwebtoken::decode_header(&jtw)
            .map_err(|e| anyhow::anyhow!("JWT header decode: {e}"))?;
        let kid = header.kid.as_deref().unwrap_or("");

        // Find matching key in JWKS
        let keys = jwks_json["keys"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("JWKS has no keys array"))?;

        let key = keys
            .iter()
            .find(|k| k.get("kid").and_then(|v| v.as_str()) == Some(kid))
            .ok_or_else(|| anyhow::anyhow!("no JWK matching kid: {kid}"))?;

        // from_rsa_components takes base64-encoded strings directly
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

        let token_data =
            jsonwebtoken::decode::<serde_json::Value>(&jtw, &decoding_key, &validation)
                .map_err(|e| anyhow::anyhow!("JWT verification: {e}"))?;

        let hanko_user_id = token_data.claims["sub"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("invalid Hanko session: missing sub"))?;

        let email: Option<String> = token_data.claims["email"]
            .as_str()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());

        // Open DB and find or create account
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;

        let account_id = match account_profile::lookup_account_by_hanko(&conn, &hanko_user_id)? {
            Some(id) => id,
            None => {
                // Auto-provision a new account
                let account_id = uuid::Uuid::new_v4().to_string();
                let username = email
                    .as_ref()
                    .and_then(|e| e.split('@').next())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        format!(
                            "user_{}",
                            &hanko_user_id.chars().take(8).collect::<String>()
                        )
                    });

                // Ensure username is unique
                let username = if account_profile::lookup_account_ref(&conn, &username)?.is_some() {
                    format!("{}_{}", username, &account_id[..8])
                } else {
                    username
                };

                account_profile::insert_account(
                    &conn,
                    &account_id,
                    &username,
                    None, // no password for hanko accounts
                    None, // preferred_name (set during onboarding)
                    Some(&hanko_user_id),
                    false,
                )?;

                if let Some(email) = &email {
                    let _ = account_profile::upsert_account_email(&conn, &account_id, email, true);
                }

                account_id
            }
        };

        let token = session_tokens::get_or_create_session_token(&conn, &account_id)?;
        let username = account_profile::username_for_account(&conn, &account_id)?
            .unwrap_or_else(|| account_id.clone());

        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(format!("hanko session task: {e}")))?
    .map_err(|e| ApiError::Unauthorized(e.to_string()))?;

    Ok(Json(result))
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
}

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    pub confirm: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteAccountResponse {
    pub ok: bool,
}

// ---------------------------------------------------------------------------
// Change-password / delete-account handlers
// ---------------------------------------------------------------------------

/// `POST /v1/auth/change-password` — verify the current password, set a new one.
pub async fn change_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiError> {
    let new_password = req.new_password.trim();
    if new_password.len() < 8 {
        return Err(ApiError::BadRequest(
            "new password must be at least 8 characters".into(),
        ));
    }
    let auth = crate::server::resolve_auth(&headers, &state).await?;
    crate::server::require_full_access(&auth)?;
    let account_id = auth.account_id;
    let current_password = req.current_password.clone();
    let db = state.cfg.paths.db.clone();
    let new_hash = hash_password(new_password).map_err(|e| ApiError::Internal(e.to_string()))?;

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        let current_hash = account_profile::load_password_hash(&conn, &account_id)?;
        if !passwords_match(current_hash.as_deref(), &current_password) {
            bail!("current password is incorrect");
        }
        account_profile::update_password_hash(&conn, &account_id, &new_hash)?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("change password task: {e}")))?
    .map_err(|e| {
        if e.to_string().contains("current password is incorrect") {
            ApiError::BadRequest(e.to_string())
        } else {
            ApiError::Internal(e.to_string())
        }
    })?;

    Ok(Json(ChangePasswordResponse { ok: true }))
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
    let db = state.cfg.paths.db.clone();
    let account_root = state.cfg.paths.data_dir.join(&account_id);

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        account_profile::delete_account(&conn, &account_id)?;
        if account_root.exists() {
            std::fs::remove_dir_all(&account_root)
                .with_context(|| format!("remove account data dir {}", account_root.display()))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("delete account task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(DeleteAccountResponse { ok: true }))
}
