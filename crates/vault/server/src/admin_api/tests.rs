use axum::http::StatusCode;

use super::*;
use crate::test_support::{
    delete_json, delete_status, get_json, get_status, login_status, patch_status, post_json,
    post_status, put_status, register_via_api, seed_one_message, test_vault,
};

#[tokio::test]
async fn non_admins_are_refused() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let _admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let ordinary = register_via_api(&state, "bob", "hunter2hunter2").await;

    let status = get_status(&state, "/v1/admin/users", &ordinary.token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "GET /v1/admin/users");
}

/// One case per route: an ordinary (non-admin) session gets 403 on every
/// handler, not just the list endpoint. The `Admin` extractor now makes a
/// missing guard a compile error (a handler cannot take the wrong
/// parameter type unnoticed), so this test's remaining job is smaller but
/// real: it pins the wire behavior — 403, on every route, through the
/// real HTTP stack — which the type system alone does not promise.
#[tokio::test]
async fn every_admin_route_refuses_an_ordinary_session() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let ordinary = register_via_api(&state, "bob", "hunter2hunter2").await;
    let target = &admin.account_id;

    assert_eq!(
        get_status(&state, "/v1/admin/users", &ordinary.token).await,
        StatusCode::FORBIDDEN,
        "GET /v1/admin/users"
    );
    assert_eq!(
        post_status(
            &state,
            "/v1/admin/users",
            &ordinary.token,
            serde_json::json!({ "username": "carol", "password": "hunter2hunter2" }),
        )
        .await,
        StatusCode::FORBIDDEN,
        "POST /v1/admin/users"
    );
    assert_eq!(
        patch_status(
            &state,
            &format!("/v1/admin/users/{target}"),
            &ordinary.token,
            serde_json::json!({ "can_export": false }),
        )
        .await,
        StatusCode::FORBIDDEN,
        "PATCH /v1/admin/users/{{id}}"
    );
    assert_eq!(
        put_status(
            &state,
            &format!("/v1/admin/users/{target}/password"),
            &ordinary.token,
            serde_json::json!({ "password": "irrelevant123" }),
        )
        .await,
        StatusCode::FORBIDDEN,
        "PUT /v1/admin/users/{{id}}/password"
    );
    assert_eq!(
        delete_status(
            &state,
            &format!("/v1/admin/users/{target}/messages"),
            &ordinary.token,
        )
        .await,
        StatusCode::FORBIDDEN,
        "DELETE /v1/admin/users/{{id}}/messages"
    );
    assert_eq!(
        delete_status(
            &state,
            &format!("/v1/admin/users/{target}"),
            &ordinary.token
        )
        .await,
        StatusCode::FORBIDDEN,
        "DELETE /v1/admin/users/{{id}}"
    );
}

#[tokio::test]
async fn api_tokens_are_refused_even_for_the_admins_own_token() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    // No token-creation helper in this module's test surface; assert the
    // guard directly instead of round-tripping through the token API.
    let mut conn = state.db.acquire().await.unwrap();
    let auth = crate::server::resolve_auth_on_conn(&mut conn, &admin.token)
        .await
        .unwrap();
    assert!(auth.is_admin());
    let token_auth = crate::server::AuthIdentity {
        account_id: auth.account_id.clone(),
        capability: crate::server::AuthCapability::ApiToken(auth.permissions()),
    };
    assert!(!token_auth.is_admin());
    assert!(crate::server::require_admin(&token_auth).is_err());
}

#[tokio::test]
async fn the_admin_sees_every_account_but_no_messages() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let _other = register_via_api(&state, "bob", "hunter2hunter2").await;

    let body: ListUsersResponse = get_json(&state, "/v1/admin/users", &admin.token).await;

    assert_eq!(body.items.len(), 2);
    let bob = body.items.iter().find(|u| u.username == "bob").unwrap();
    assert_eq!(bob.message_count, 0);
    assert!(!bob.is_admin);
    assert!(!bob.disabled);
}

/// Sort and return an object's keys. Panics if `v` is not an object —
/// every wire body this test touches is expected to be one.
fn sorted_keys(v: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = v
        .as_object()
        .unwrap_or_else(|| panic!("expected a JSON object, got {v}"))
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort_unstable();
    keys
}

const ADMIN_USER_FIELDS: [&str; 9] = [
    "account_id",
    "can_delete",
    "can_export",
    "can_import",
    "disabled",
    "is_admin",
    "message_count",
    "storage_bytes",
    "username",
];

#[tokio::test]
async fn list_response_has_no_message_content_fields() {
    // Decode into raw JSON, not the typed `ListUsersResponse` — serde
    // silently drops unknown fields on decode, so asserting on a
    // re-serialized typed value would only prove the struct's own shape,
    // not what the server actually put on the wire. This reads the wire
    // payload directly.
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    seed_one_message(&state, &admin.account_id).await;

    let body: serde_json::Value = get_json(&state, "/v1/admin/users", &admin.token).await;
    let item = &body["items"][0];
    assert_eq!(
        sorted_keys(item),
        ADMIN_USER_FIELDS.to_vec(),
        "admin user rows must carry only metadata, never message content"
    );
}

#[tokio::test]
async fn delete_messages_response_has_no_message_content_fields() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let victim = register_via_api(&state, "bob", "hunter2hunter2").await;
    seed_one_message(&state, &victim.account_id).await;

    let body: serde_json::Value = delete_json(
        &state,
        &format!("/v1/admin/users/{}/messages", victim.account_id),
        &admin.token,
    )
    .await;
    assert_eq!(
        sorted_keys(&body),
        vec!["attachments", "conversations"],
        "delete-messages response must carry only counts, never message content"
    );
}

#[tokio::test]
async fn delete_account_response_has_no_message_content_fields() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let victim = register_via_api(&state, "bob", "hunter2hunter2").await;
    seed_one_message(&state, &victim.account_id).await;

    let status = delete_status(
        &state,
        &format!("/v1/admin/users/{}", victim.account_id),
        &admin.token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "delete-account is an acknowledgement with no body"
    );
}

#[tokio::test]
async fn the_last_admin_cannot_be_demoted_disabled_or_deleted() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let path = format!("/v1/admin/users/{}", admin.account_id);

    // Asserting only the status code would still pass an implementation
    // that mutated the row first and returned 400 second (the row would
    // already be wrong by the time the caller could react). Re-read the
    // account after each refusal and assert it is byte-for-byte
    // unchanged.
    async fn snapshot(state: &AppState, account_id: &str) -> AdminUser {
        let mut conn = state.db.acquire().await.unwrap();
        load_admin_user(&mut conn, account_id)
            .await
            .unwrap()
            .unwrap()
    }

    let demoted = patch_status(
        &state,
        &path,
        &admin.token,
        serde_json::json!({ "is_admin": false }),
    )
    .await;
    assert_eq!(demoted, StatusCode::BAD_REQUEST);
    let after_demote = snapshot(&state, &admin.account_id).await;
    assert!(after_demote.is_admin, "refused demote must not demote");
    assert!(!after_demote.disabled);

    let disabled = patch_status(
        &state,
        &path,
        &admin.token,
        serde_json::json!({ "disabled": true }),
    )
    .await;
    assert_eq!(disabled, StatusCode::BAD_REQUEST);
    let after_disable = snapshot(&state, &admin.account_id).await;
    assert!(after_disable.is_admin);
    assert!(!after_disable.disabled, "refused disable must not disable");

    let deleted = delete_status(&state, &path, &admin.token).await;
    assert_eq!(deleted, StatusCode::BAD_REQUEST);
    // The account must still exist and be reachable to prove the refused
    // delete did not run.
    let still_exists = snapshot(&state, &admin.account_id).await;
    assert_eq!(still_exists.username, "alice");
}

#[tokio::test]
async fn once_a_second_admin_exists_the_first_can_be_demoted() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

    let promoted = patch_status(
        &state,
        &format!("/v1/admin/users/{}", bob.account_id),
        &admin.token,
        serde_json::json!({ "is_admin": true }),
    )
    .await;
    assert_eq!(promoted, StatusCode::OK);

    let demoted = patch_status(
        &state,
        &format!("/v1/admin/users/{}", admin.account_id),
        &admin.token,
        serde_json::json!({ "is_admin": false }),
    )
    .await;
    assert_eq!(demoted, StatusCode::OK);
}

/// Regression test for the vault-lockout hole: `is_last_admin` used to
/// count every account with `is_admin = 1`, including disabled ones, so
/// three individually-legal calls could disable every administrator with
/// no credential left able to reach `/v1/admin/*`:
///   1. alice promotes bob             (allowed — alice is still enabled)
///   2. alice disables bob             (allowed — alice is still enabled)
///   3. alice disables herself         (used to be allowed: the guard
///      saw disabled-bob as "another admin" and let alice go too)
/// Step 3 must now be refused.
#[tokio::test]
async fn last_admin_exploit_sequence_is_blocked_at_final_step() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

    let promote_bob = patch_status(
        &state,
        &format!("/v1/admin/users/{}", bob.account_id),
        &admin.token,
        serde_json::json!({ "is_admin": true }),
    )
    .await;
    assert_eq!(promote_bob, StatusCode::OK, "step 1: promote bob");

    let disable_bob = patch_status(
        &state,
        &format!("/v1/admin/users/{}", bob.account_id),
        &admin.token,
        serde_json::json!({ "disabled": true }),
    )
    .await;
    assert_eq!(disable_bob, StatusCode::OK, "step 2: disable bob");

    let disable_self = patch_status(
        &state,
        &format!("/v1/admin/users/{}", admin.account_id),
        &admin.token,
        serde_json::json!({ "disabled": true }),
    )
    .await;
    assert_eq!(
        disable_self,
        StatusCode::BAD_REQUEST,
        "step 3: a disabled admin (bob) must not count as \"another admin\"; \
         alice is the vault's only usable administrator and must be refused"
    );
}

/// Same hole, phrased as a state check rather than a call sequence: even
/// with a disabled admin already on the books, the vault's one enabled
/// admin still cannot disable itself.
#[tokio::test]
async fn enabled_sole_admin_cannot_disable_self_when_a_disabled_admin_exists() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;
    assert_eq!(
        patch_status(
            &state,
            &format!("/v1/admin/users/{}", bob.account_id),
            &admin.token,
            serde_json::json!({ "is_admin": true, "disabled": true }),
        )
        .await,
        StatusCode::OK,
        "prep: bob is a disabled admin"
    );

    let disable_self = patch_status(
        &state,
        &format!("/v1/admin/users/{}", admin.account_id),
        &admin.token,
        serde_json::json!({ "disabled": true }),
    )
    .await;
    assert_eq!(disable_self, StatusCode::BAD_REQUEST);
}

/// The fix must not overreach: disabling an admin who is *not* the last
/// enabled one still has to work.
#[tokio::test]
async fn disabling_a_non_last_admin_still_works() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;
    assert_eq!(
        patch_status(
            &state,
            &format!("/v1/admin/users/{}", bob.account_id),
            &admin.token,
            serde_json::json!({ "is_admin": true }),
        )
        .await,
        StatusCode::OK,
        "prep: bob is an enabled admin too"
    );

    let disable_bob = patch_status(
        &state,
        &format!("/v1/admin/users/{}", bob.account_id),
        &admin.token,
        serde_json::json!({ "disabled": true }),
    )
    .await;
    assert_eq!(
        disable_bob,
        StatusCode::OK,
        "alice is still an enabled admin, so disabling bob is fine"
    );
}

/// Think-through case named in review: promoting an already-disabled
/// account to admin must not itself "free" the vault's last enabled
/// admin to disable themselves — the promoted account is still disabled
/// and still cannot administer anything.
#[tokio::test]
async fn promoting_a_disabled_account_does_not_unlock_the_last_admins_self_disable() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

    assert_eq!(
        patch_status(
            &state,
            &format!("/v1/admin/users/{}", bob.account_id),
            &admin.token,
            serde_json::json!({ "disabled": true }),
        )
        .await,
        StatusCode::OK,
        "prep: bob is disabled, ordinary (not yet admin)"
    );
    assert_eq!(
        patch_status(
            &state,
            &format!("/v1/admin/users/{}", bob.account_id),
            &admin.token,
            serde_json::json!({ "is_admin": true }),
        )
        .await,
        StatusCode::OK,
        "promoting a disabled account is allowed on its own"
    );

    let disable_self = patch_status(
        &state,
        &format!("/v1/admin/users/{}", admin.account_id),
        &admin.token,
        serde_json::json!({ "disabled": true }),
    )
    .await;
    assert_eq!(
        disable_self,
        StatusCode::BAD_REQUEST,
        "bob is an admin only on paper; disabled, he cannot administer anything, \
         so alice is still the vault's only usable administrator"
    );
}

#[tokio::test]
async fn create_user_never_grants_admin_by_default() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    let created: AdminUser = post_json(
        &state,
        "/v1/admin/users",
        &admin.token,
        serde_json::json!({ "username": "carol", "password": "hunter2hunter2" }),
    )
    .await;
    assert!(!created.is_admin);
    assert_eq!(created.username, "carol");
    assert_eq!(created.message_count, 0);

    // The created account can sign in with the password it was given.
    let login = login_status(&state, "carol", "hunter2hunter2").await;
    assert_eq!(login, StatusCode::OK);
}

#[tokio::test]
async fn create_user_can_grant_admin_explicitly() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    let created: AdminUser = post_json(
        &state,
        "/v1/admin/users",
        &admin.token,
        serde_json::json!({ "username": "carol", "password": "hunter2hunter2", "is_admin": true }),
    )
    .await;
    assert!(created.is_admin);
}

#[tokio::test]
async fn setting_a_password_lets_the_new_password_sign_in() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

    let status = put_status(
        &state,
        &format!("/v1/admin/users/{}/password", bob.account_id),
        &admin.token,
        serde_json::json!({ "password": "newpassword123" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let login = login_status(&state, "bob", "newpassword123").await;
    assert_eq!(login, StatusCode::OK);
}

#[tokio::test]
async fn setting_a_password_invalidates_the_targets_existing_session() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let bob = register_via_api(&state, "bob", "hunter2hunter2").await;

    // Bob's registration session must work before the reset.
    assert_eq!(
        get_status(&state, "/v1/auth/check", &bob.token).await,
        StatusCode::OK
    );

    let status = put_status(
        &state,
        &format!("/v1/admin/users/{}/password", bob.account_id),
        &admin.token,
        serde_json::json!({ "password": "newpassword123" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    assert_eq!(
        get_status(&state, "/v1/auth/check", &bob.token).await,
        StatusCode::UNAUTHORIZED,
        "an administrator resetting the password must end the target's existing session"
    );
}

#[tokio::test]
async fn deleting_one_users_messages_leaves_the_others_alone() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
    let victim = register_via_api(&state, "bob", "hunter2hunter2").await;
    seed_one_message(&state, &victim.account_id).await;
    seed_one_message(&state, &admin.account_id).await;

    let status = delete_status(
        &state,
        &format!("/v1/admin/users/{}/messages", victim.account_id),
        &admin.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let body: ListUsersResponse = get_json(&state, "/v1/admin/users", &admin.token).await;
    let bob = body.items.iter().find(|u| u.username == "bob").unwrap();
    let alice = body.items.iter().find(|u| u.username == "alice").unwrap();
    assert_eq!(bob.message_count, 0);
    assert_eq!(alice.message_count, 1, "the other tenant is untouched");
}

/// The four handlers that were given an existence check beyond what the
/// brief specified (so a bad account id can't silently no-op and still
/// answer 204) each get their own 404 case, so that guard
/// can't regress unnoticed.
#[tokio::test]
async fn patch_of_a_missing_account_is_404() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    let status = patch_status(
        &state,
        "/v1/admin/users/does-not-exist",
        &admin.token,
        serde_json::json!({ "can_export": false }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn setting_a_password_on_a_missing_account_is_404() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    let status = put_status(
        &state,
        "/v1/admin/users/does-not-exist/password",
        &admin.token,
        serde_json::json!({ "password": "irrelevant123" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_messages_of_a_missing_account_is_404() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    let status = delete_status(
        &state,
        "/v1/admin/users/does-not-exist/messages",
        &admin.token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_missing_account_is_404() {
    let vault = test_vault().await;
    let state = vault.state.clone();
    let admin = register_via_api(&state, "alice", "hunter2hunter2").await;

    let status = delete_status(&state, "/v1/admin/users/does-not-exist", &admin.token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
