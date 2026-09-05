use super::*;
use crate::db::engine;

/// Passwords hashed before the argon2 0.6 upgrade must still let people in.
///
/// These three strings were produced by argon2 0.5.3, the version the vault
/// shipped with, using the salt scheme it used then: sixteen bytes from the
/// system RNG, base64-encoded, passed to `Argon2::default()`. If a future
/// upgrade stops them verifying, every account created before that upgrade
/// is locked out, and no other test in this suite would notice — the rest
/// all hash and verify with the same version in the same process.
#[test]
fn hashes_written_by_argon2_0_5_still_verify() {
    const LEGACY: &[(&str, &str)] = &[
        (
            "hunter2hunter2",
            "$argon2id$v=19$m=19456,t=2,p=1$/ic5l4xy5HAgEHBiuv0t3A$iI1c7vmoqfa79pGmE3/iquM09ezwKoYA8U1dxtWH/rg",
        ),
        (
            "",
            "$argon2id$v=19$m=19456,t=2,p=1$Mp73mogaQlz3ZqmokZzY/A$CVBX4QUiJTjS5u4sLQ7rvMEuSo8e6c1izYXZTtJo+RQ",
        ),
        (
            "pässwörd with spaces 🔐",
            "$argon2id$v=19$m=19456,t=2,p=1$l9S0iAuO7kK+iuZf99EN9g$FQ35n77YztPzFZAYqhZnyRLK6TUg7nuXUGiO7Nx1s90",
        ),
    ];
    for (password, hash) in LEGACY {
        assert!(
            verify_password(hash, password),
            "argon2 0.5 hash must still verify for {password:?}"
        );
        assert!(
            !verify_password(hash, "wrong-password"),
            "the wrong password must still be refused for {password:?}"
        );
    }
}

/// The parameters the vault writes must not drift silently. A weaker
/// memory or time cost would be a security regression that still passes
/// every round-trip test, because hashing and verifying would agree.
#[test]
fn a_new_hash_keeps_argon2id_and_its_default_cost() {
    let hash = hash_password("hunter2hunter2").expect("hash");
    assert!(
        hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "unexpected argon2 parameters: {hash}"
    );
    assert!(verify_password(&hash, "hunter2hunter2"));
    assert!(!verify_password(&hash, "hunter2hunter3"));
}

/// Two hashes of the same password must differ, or the salt is not doing
/// its job. argon2 0.6 generates the salt itself now, so this is the check
/// that the generated salt is actually random rather than fixed.
#[test]
fn the_same_password_hashes_differently_every_time() {
    let a = hash_password("hunter2hunter2").expect("hash");
    let b = hash_password("hunter2hunter2").expect("hash");
    assert_ne!(a, b, "a repeated hash means the salt is not random");
    assert!(verify_password(&a, "hunter2hunter2"));
    assert!(verify_password(&b, "hunter2hunter2"));
}

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
async fn auth_check_names_the_account_without_an_ok_flag_and_logout_is_204() {
    let vault = crate::test_support::test_vault().await;
    let state = vault.state.clone();
    let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
    let body: serde_json::Value =
        crate::test_support::get_json(&state, "/v1/auth/check", &user.token).await;
    assert_eq!(body["username"], "alice");
    assert!(
        body.get("ok").is_none() && body.get("account_ok").is_none(),
        "{body}"
    );
    let status = crate::test_support::post_status(
        &state,
        "/v1/auth/logout",
        &user.token,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
}

/// `GET /v1/auth/check?account=` naming a different account is refused,
/// even with an otherwise valid token — the near-identical branch to
/// `POST /v1/import`'s account query, but with a longer sentence that
/// names the token's own user.
#[tokio::test]
async fn auth_check_refuses_an_account_query_naming_someone_else() {
    let vault = crate::test_support::test_vault().await;
    let state = vault.state.clone();
    let alice = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = crate::test_support::register_via_api(&state, "bob", "hunter2hunter2").await;

    let (status, text) = crate::test_support::get_raw(
        &state,
        &format!("/v1/auth/check?account={}", bob.username),
        &alice.token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "{text}");
    let err: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        err["error"],
        "account query does not match token's account (token is for alice)"
    );

    // Positive control: naming her own account must succeed outright —
    // unlike import, a GET has nothing left to fail on afterward.
    let status = crate::test_support::get_status(
        &state,
        &format!("/v1/auth/check?account={}", alice.username),
        &alice.token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "alice naming herself");
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
    let first_api_token = api_tokens::create_api_token(
        &mut conn,
        TEST_ACCOUNT,
        "backup client",
        Permissions::all(),
        None,
    )
    .await
    .unwrap()
    .token;
    let second_api_token = api_tokens::create_api_token(
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
    .unwrap()
    .token;
    let other_account_token = api_tokens::create_api_token(
        &mut conn,
        OTHER_ACCOUNT,
        "other account client",
        Permissions::all(),
        None,
    )
    .await
    .unwrap()
    .token;
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
    let limits: AuthRateLimits = Arc::new(Mutex::new(HashMap::new()));
    let bucket = "register:someone";
    for _ in 0..AUTH_RATE_MAX {
        check_auth_rate_limit(&limits, bucket).unwrap();
    }
    let err = check_auth_rate_limit(&limits, bucket).unwrap_err();
    match err {
        ApiError::TooManyRequests(_) => {}
        other => panic!("expected TooManyRequests, got {other:?}"),
    }
}

#[test]
fn auth_rate_limits_do_not_cross_vaults() {
    let one: AuthRateLimits = Arc::new(Mutex::new(HashMap::new()));
    let two: AuthRateLimits = Arc::new(Mutex::new(HashMap::new()));
    let bucket = "register:someone";
    for _ in 0..AUTH_RATE_MAX {
        check_auth_rate_limit(&one, bucket).unwrap();
    }
    check_auth_rate_limit(&one, bucket).unwrap_err();
    check_auth_rate_limit(&two, bucket)
        .expect("a second vault's limiter must not see the first vault's hits");
}

#[tokio::test]
async fn change_password_transaction_updates_all_credentials() {
    let (_dir, mut conn, old_session, api_tokens, other_account_token) =
        password_change_setup().await;
    let new_hash = hash_password("new-password").unwrap();

    let new_session = change_password_on_conn(&mut conn, TEST_ACCOUNT, "old-password", &new_hash)
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
    if crate::test_support::on_postgres() {
        return; // SQLite-only: the failure is injected with a trigger in SQLite's RAISE syntax
    }
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
async fn first_account_registration_requires_a_password() {
    let vault = test_vault().await;
    let state = vault.state.clone();

    let status = post_status(
        &state,
        "/v1/auth/register",
        "irrelevant-no-token-needed",
        serde_json::json!({ "username": "passwordless-first" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the vault's first account must not be created without a password \
         (it becomes an administrator)"
    );

    let mut conn = state.db.acquire().await.unwrap();
    assert!(
        account_profile::vault_has_no_real_accounts(&mut conn)
            .await
            .unwrap(),
        "the rejected registration must not have created an account"
    );
}

#[tokio::test]
async fn second_account_may_still_register_without_a_password() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let _first = register_via_api(&state, "has-a-password", "hunter2hunter2").await;

    let status = post_status(
        &state,
        "/v1/auth/register",
        "irrelevant-no-token-needed",
        serde_json::json!({ "username": "passwordless-second" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "only the first (admin-granting) account requires a password"
    );

    let mut conn = state.db.acquire().await.unwrap();
    let account_id = account_profile::lookup_account_ref(&mut conn, "passwordless-second")
        .await
        .unwrap()
        .unwrap();
    let auth = account_profile::load_account_auth(&mut conn, &account_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!auth.is_admin);
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

// -----------------------------------------------------------------
// Self-service account deletion vs. the last administrator
// -----------------------------------------------------------------

#[tokio::test]
async fn solo_admin_can_delete_their_own_account() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "solo-admin", "hunter2hunter2").await;

    let status = post_status(
        &state,
        "/v1/auth/delete-account",
        &admin.token,
        serde_json::json!({ "confirm": true, "current_password": "hunter2hunter2" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the only administrator on their own vault must still be able to leave"
    );

    let mut conn = state.db.acquire().await.unwrap();
    assert!(
        account_profile::username_for_account(&mut conn, &admin.account_id)
            .await
            .unwrap()
            .is_none(),
        "the account must actually be gone"
    );
}

#[tokio::test]
async fn last_admin_with_another_account_present_is_refused() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "team-admin", "hunter2hunter2").await;
    let _other = register_via_api(&state, "team-member", "hunter2hunter2").await;

    let status = post_status(
        &state,
        "/v1/auth/delete-account",
        &admin.token,
        serde_json::json!({ "confirm": true, "current_password": "hunter2hunter2" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the last administrator must not be able to strand the other account"
    );

    let mut conn = state.db.acquire().await.unwrap();
    assert!(
        account_profile::username_for_account(&mut conn, &admin.account_id)
            .await
            .unwrap()
            .is_some(),
        "the refused deletion must not have removed the account"
    );
}

#[tokio::test]
async fn non_admin_account_deletion_is_unaffected_by_the_last_admin_check() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let _admin = register_via_api(&state, "org-admin", "hunter2hunter2").await;
    let member = register_via_api(&state, "org-member", "hunter2hunter2").await;

    let status = post_status(
        &state,
        "/v1/auth/delete-account",
        &member.token,
        serde_json::json!({ "confirm": true, "current_password": "hunter2hunter2" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "an ordinary account must be able to delete itself regardless of the admin count"
    );
}
