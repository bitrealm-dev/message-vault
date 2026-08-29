//! Shared HTTP helpers for the server's own tests. Each call starts the real
//! axum app on an ephemeral port, issues one request, and shuts it down.
//!
//! Distinct from `server.rs`'s `test_state()`, which returns a four-tuple
//! `(TempDir, AppState, String, i64)` for handler-level tests that call a
//! handler function directly. This module drives the whole stack over real
//! HTTP, for tests in `auth.rs`, and (per the authorization-model plan)
//! `accounts_api.rs` and related modules.

use axum::http::StatusCode;
use serde::de::DeserializeOwned;
use tempfile::TempDir;

use crate::server::{AppState, http_app};

/// A vault plus its temp directory. Drop the `TempDir` last.
pub struct TestVault {
    /// Keeps the temp directory alive for the test's lifetime.
    pub _tmp: TempDir,
    /// The server state every helper drives.
    pub state: AppState,
}

/// An account created through the API, with its session token.
pub struct RegisteredAccount {
    /// The new account's id.
    pub account_id: String,
    /// The username it was created with.
    pub username: String,
    /// A live session token for it.
    pub token: String,
}

/// An empty vault with schema applied and no accounts.
pub async fn test_vault() -> TestVault {
    let (pool, tmp) = crate::db::engine::test_pool().await;
    {
        let mut conn = pool.acquire().await.unwrap();
        crate::db::schema::ensure_vault_schema(&mut conn)
            .await
            .unwrap();
        crate::db::schema::ensure_accounts_schema(&mut conn)
            .await
            .unwrap();
    }
    let state = crate::server::test_app_state(pool, tmp.path()).await;
    TestVault { _tmp: tmp, state }
}

async fn request(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> reqwest::Response {
    let app = http_app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut req = reqwest::Client::new().request(method, format!("http://{address}{path}"));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req.send().await.unwrap();
    server.abort();
    response
}

/// Register an account through the API and return it with a live token.
///
/// Resets the register rate-limit bucket for `username` first. The limiter
/// is a process-global static (`auth::AUTH_RATE_LIMITS`), and this whole
/// suite reuses a handful of literal usernames ("alice", "bob", ...) across
/// many test functions that all run in the same test binary; without this,
/// enough tests registering the same name inside one 60-second window trips
/// `AUTH_RATE_MAX` and fails an unrelated test with a 429.
pub async fn register_via_api(
    state: &AppState,
    username: &str,
    password: &str,
) -> RegisteredAccount {
    crate::auth::reset_auth_rate_limit_bucket_for_test(&format!("register:{username}"));
    let response = request(
        state,
        reqwest::Method::POST,
        "/v1/auth/register",
        None,
        Some(serde_json::json!({ "username": username, "password": password })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "register must succeed");
    let body: serde_json::Value = response.json().await.unwrap();
    RegisteredAccount {
        account_id: body["account_id"].as_str().unwrap().to_string(),
        username: body["username"].as_str().unwrap().to_string(),
        token: body["token"].as_str().unwrap().to_string(),
    }
}

/// The status of a login attempt.
pub async fn login_status(state: &AppState, username: &str, password: &str) -> StatusCode {
    request(
        state,
        reqwest::Method::POST,
        "/v1/auth/login",
        None,
        Some(serde_json::json!({ "username": username, "password": password })),
    )
    .await
    .status()
}

/// GET a path with a Bearer token, returning only the status.
pub async fn get_status(state: &AppState, path: &str, token: &str) -> StatusCode {
    request(state, reqwest::Method::GET, path, Some(token), None)
        .await
        .status()
}

/// GET a path with a Bearer token and decode the JSON body.
pub async fn get_json<T: DeserializeOwned>(state: &AppState, path: &str, token: &str) -> T {
    let response = request(state, reqwest::Method::GET, path, Some(token), None).await;
    assert_eq!(response.status(), StatusCode::OK, "GET {path} must succeed");
    response.json().await.unwrap()
}

/// POST a JSON body with a Bearer token and decode the JSON response.
pub async fn post_json<T: DeserializeOwned>(
    state: &AppState,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> T {
    let response = request(state, reqwest::Method::POST, path, Some(token), Some(body)).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "POST {path} must succeed"
    );
    response.json().await.unwrap()
}

/// POST a JSON body with a Bearer token, returning only the status.
pub async fn post_status(
    state: &AppState,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> StatusCode {
    request(state, reqwest::Method::POST, path, Some(token), Some(body))
        .await
        .status()
}

/// PATCH a JSON body with a Bearer token, returning only the status.
pub async fn patch_status(
    state: &AppState,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> StatusCode {
    request(state, reqwest::Method::PATCH, path, Some(token), Some(body))
        .await
        .status()
}

/// DELETE a path with a Bearer token, returning only the status.
pub async fn delete_status(state: &AppState, path: &str, token: &str) -> StatusCode {
    request(state, reqwest::Method::DELETE, path, Some(token), None)
        .await
        .status()
}

/// DELETE a path with a Bearer token and decode the JSON response.
pub async fn delete_json<T: DeserializeOwned>(state: &AppState, path: &str, token: &str) -> T {
    let response = request(state, reqwest::Method::DELETE, path, Some(token), None).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "DELETE {path} must succeed"
    );
    response.json().await.unwrap()
}

/// Give an account one conversation holding one message, so counts are non-zero.
///
/// `messages.conversation_id` and `conversations.chat_handle_id` are integer
/// foreign keys, so this first creates a `handles` row (the way every real
/// importer does) rather than binding a string straight into `chat_handle_id`.
pub async fn seed_one_message(state: &AppState, account_id: &str) {
    let mut conn = state.db.acquire().await.unwrap();
    let handle_id: i64 = sqlx::query_scalar(
        "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
         VALUES ($1, $2, $2, 'phone', 'phone') RETURNING id",
    )
    .bind(account_id)
    .bind(format!("+1555{account_id}"))
    .fetch_one(&mut *conn)
    .await
    .unwrap();

    let conversation_id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
         VALUES ($1, $2, 'individual', 'seed.jsonl') RETURNING id",
    )
    .bind(account_id)
    .bind(handle_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO messages (
            conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
         ) VALUES ($1, $2, 'imessage', '2020-01-01T00:00:00Z', 1, 0, 'hello')",
    )
    .bind(conversation_id)
    .bind(account_id)
    .execute(&mut *conn)
    .await
    .unwrap();
}
