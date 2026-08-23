//! OpenAPI document for message-vault-server HTTP routes.

use std::io::Write;
use std::path::Path;

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::config::AuthMode;
use crate::server::AppState;

/// Title used in the generated OpenAPI document.
pub const API_TITLE: &str = "Message Vault HTTP API";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Message Vault HTTP API",
        description = "HTTP API for a local Message Vault. Bearer session tokens come from login. API tokens come from Settings → Account and can import and export only. Register and login exist when VAULT_AUTH is local (the default). POST /v1/auth/hanko/session exists for Hanko sign-in.",
        version = env!("CARGO_PKG_VERSION")
    ),
    modifiers(&BearerAddon),
    tags(
        (name = "Health", description = "Process liveness"),
        (name = "Auth", description = "Sign-in, session, and token check"),
        (name = "Account", description = "Profile, storage, and API tokens"),
        (name = "Import", description = "JSONL import sessions and ingest"),
        (name = "Export", description = "Read-only messages and counts"),
        (name = "Assets", description = "Attachment bytes"),
        (name = "Contacts", description = "Address book and contact groups"),
        (name = "Conversations", description = "Conversation list and sources"),
        (name = "Thread tags", description = "Labels on conversations")
    )
)]
/// OpenAPI document definition assembled from the utoipa-annotated handlers.
pub struct ApiDoc;

struct BearerAddon;

impl Modify for BearerAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_default();
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

/// Which auth endpoints the OpenAPI document includes.
pub enum SpecAuth {
    /// Auth endpoints enabled by the running auth mode; local register/login
    /// only when [`AuthMode::Local`].
    Live(AuthMode),
    /// Every auth endpoint, including local register/login regardless of mode.
    Full,
}

/// Unauthenticated auth JSON (Hanko, Try it, and Local register/login).
pub fn auth_public_openapi(auth: SpecAuth) -> OpenApiRouter<AppState> {
    let mut router = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(crate::auth::hanko_session_handler))
        .routes(routes!(crate::auth::try_demo_handler));

    let include_local = match auth {
        SpecAuth::Full => true,
        SpecAuth::Live(AuthMode::Local) => true,
        SpecAuth::Live(AuthMode::Hanko) => false,
    };
    if include_local {
        router = router
            .routes(routes!(crate::auth::register_handler))
            .routes(routes!(crate::auth::login_handler));
    }
    router
}

/// Health, session-backed auth, account settings, and browse routes.
pub fn api_openapi() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(crate::server::health))
        .routes(routes!(crate::auth::auth_mode_handler))
        .routes(routes!(crate::auth::auth_check))
        .routes(routes!(crate::auth::logout_handler))
        .routes(routes!(crate::auth::change_password_handler))
        .routes(routes!(crate::auth::delete_account_handler))
        .routes(routes!(crate::profile::account_profile_handler))
        .routes(routes!(crate::profile::account_profile_update_handler))
        .routes(routes!(crate::profile::delete_messages_handler))
        .routes(routes!(crate::profile::account_storage_handler))
        .routes(routes!(crate::api_tokens_api::list_api_tokens_handler))
        .routes(routes!(crate::api_tokens_api::create_api_token_handler))
        .routes(routes!(crate::api_tokens_api::delete_api_token_handler))
        .routes(routes!(crate::api_tokens_api::rename_api_token_handler))
        .routes(routes!(crate::export_api::export_messages_handler))
        .routes(routes!(crate::export_api::export_messages_count_handler))
        .routes(routes!(crate::contacts_api::contacts_list_handler))
        .routes(routes!(crate::contacts_api::contact_summaries_handler))
        .routes(routes!(crate::contacts_api::contact_detail_handler))
        .routes(routes!(crate::contacts_api::contact_mutate_handler))
        .routes(routes!(crate::server::contact_groups_list_handler))
        .routes(routes!(crate::server::contact_groups_create_handler))
        .routes(routes!(crate::server::contact_groups_rename_handler))
        .routes(routes!(crate::server::contact_groups_delete_handler))
        .routes(routes!(crate::server::contact_groups_members_handler))
        .routes(routes!(crate::server::contact_groups_membership_handler))
        .routes(routes!(crate::server::thread_tags_list_handler))
        .routes(routes!(crate::server::thread_tags_create_handler))
        .routes(routes!(crate::server::thread_tags_rename_handler))
        .routes(routes!(crate::server::thread_tags_delete_handler))
        .routes(routes!(crate::server::thread_tags_members_handler))
        .routes(routes!(crate::server::thread_tags_membership_handler))
        .routes(routes!(
            crate::conversations_api::conversations_list_handler
        ))
        .routes(routes!(
            crate::conversations_api::conversation_sources_handler
        ))
        .routes(routes!(crate::server::imports_list_handler))
        .routes(routes!(crate::server::imports_create_handler))
        .routes(routes!(crate::server::imports_get_handler))
        .routes(routes!(crate::server::imports_complete_handler))
        .routes(routes!(crate::server::import_handler))
        .routes(routes!(crate::server::asset_head_handler))
        .routes(routes!(crate::server::asset_get_handler))
        .routes(routes!(crate::server::asset_put_handler))
        .routes(routes!(crate::server::asset_upload_start_handler))
        .routes(routes!(crate::server::asset_upload_part_handler))
        .routes(routes!(crate::server::asset_upload_complete_handler))
        .routes(routes!(crate::server::asset_upload_abort_handler))
}

/// The full OpenAPI router: public auth endpoints for `auth` plus the
/// session-backed API routes.
pub fn openapi_router(auth: SpecAuth) -> OpenApiRouter<AppState> {
    auth_public_openapi(auth).merge(api_openapi())
}

/// Pretty OpenAPI JSON. Same string the CLI writes and the stale-spec test compares.
pub fn dump_openapi_json() -> String {
    let (_a, mut spec) = auth_public_openapi(SpecAuth::Full).split_for_parts();
    let (_b, rest) = api_openapi().split_for_parts();
    spec.merge(rest);
    serde_json::to_string_pretty(&spec).expect("OpenAPI document serializes to JSON")
}

/// Write the dump to `path`, or stdout when `path` is `None`.
pub fn write_openapi(path: Option<&Path>) -> anyhow::Result<()> {
    let json = dump_openapi_json();
    match path {
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(json.as_bytes())?;
            if !json.ends_with('\n') {
                out.write_all(b"\n")?;
            }
        }
        Some(p) => std::fs::write(p, json.as_bytes())
            .map_err(|e| anyhow::anyhow!("write {}: {e}", p.display()))?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SpecAuth, dump_openapi_json, openapi_router};
    use crate::config::AuthMode;

    #[test]
    fn dump_is_openapi_3_with_crate_version() {
        let v: serde_json::Value = serde_json::from_str(&dump_openapi_json()).unwrap();
        let openapi = v["openapi"].as_str().expect("openapi field");
        assert!(
            openapi.starts_with("3."),
            "expected OpenAPI 3.x, got {openapi}"
        );
        assert_eq!(v["info"]["title"], "Message Vault HTTP API");
        assert_eq!(v["info"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn dump_pretty_print_is_stable() {
        let a = dump_openapi_json();
        let b = dump_openapi_json();
        assert_eq!(a, b);
        assert!(a.contains('\n'), "expected pretty JSON");
    }

    #[test]
    fn dump_includes_health() {
        let v: serde_json::Value = serde_json::from_str(&dump_openapi_json()).unwrap();
        assert!(
            v["paths"]["/health"]["get"].is_object(),
            "expected GET /health in dump"
        );
    }

    #[test]
    fn dump_includes_auth_and_account_paths() {
        let v: serde_json::Value = serde_json::from_str(&dump_openapi_json()).unwrap();
        let paths = v["paths"].as_object().unwrap();
        for p in [
            "/v1/auth/register",
            "/v1/auth/login",
            "/v1/auth/hanko/session",
            "/v1/auth/try-demo",
            "/v1/auth/mode",
            "/v1/auth/check",
            "/v1/auth/logout",
            "/v1/auth/change-password",
            "/v1/auth/delete-account",
            "/v1/account/profile",
            "/v1/account/delete-messages",
            "/v1/account/storage",
            "/v1/account/api-tokens",
            "/v1/account/api-tokens/{id}",
        ] {
            assert!(paths.contains_key(p), "missing {p}");
        }
        assert!(
            !operation_has_bearer(&paths["/v1/auth/register"]["post"]),
            "register is public"
        );
        assert!(
            operation_has_bearer(&paths["/v1/auth/check"]["get"]),
            "GET /v1/auth/check must require bearer"
        );
    }

    fn operation_has_bearer(op: &serde_json::Value) -> bool {
        op["security"]
            .as_array()
            .is_some_and(|schemes| schemes.iter().any(|s| s.get("bearer").is_some()))
    }

    #[test]
    fn dump_includes_browse_paths() {
        let v: serde_json::Value = serde_json::from_str(&dump_openapi_json()).unwrap();
        let paths = v["paths"].as_object().unwrap();
        for p in [
            "/v1/export/messages",
            "/v1/export/messages/count",
            "/v1/export/contacts",
            "/v1/export/contacts/summaries",
            "/v1/export/contacts/{id}",
            "/v1/contact-groups",
            "/v1/contact-groups/members",
            "/v1/contacts/groups",
            "/v1/thread-tags",
            "/v1/thread-tags/members",
            "/v1/conversations/tags",
            "/v1/export/conversations",
            "/v1/export/conversations/{id}/sources",
        ] {
            assert!(paths.contains_key(p), "missing {p}");
        }
    }

    #[test]
    fn dump_includes_import_and_asset_paths() {
        let v: serde_json::Value = serde_json::from_str(&dump_openapi_json()).unwrap();
        let paths = v["paths"].as_object().unwrap();
        for p in [
            "/v1/imports",
            "/v1/imports/{id}",
            "/v1/imports/{id}/complete",
            "/v1/import",
            "/v1/assets/{sha256}",
            "/v1/assets/{sha256}/uploads",
            "/v1/assets/{sha256}/uploads/{upload_id}/parts/{part}",
            "/v1/assets/{sha256}/uploads/{upload_id}/complete",
            "/v1/assets/{sha256}/uploads/{upload_id}",
        ] {
            assert!(paths.contains_key(p), "missing {p}");
        }
        let import = &paths["/v1/import"]["post"]["requestBody"]["content"];
        for ct in [
            "application/x-ndjson",
            "application/jsonl",
            "multipart/form-data",
        ] {
            assert!(
                import.get(ct).is_some(),
                "POST /v1/import must document {ct}"
            );
        }
        let put = &paths["/v1/assets/{sha256}"]["put"]["requestBody"]["content"];
        assert!(
            put.get("application/octet-stream").is_some(),
            "PUT asset must be raw bytes"
        );
    }

    #[test]
    fn live_hanko_spec_omits_register_login() {
        let (_router, api) = openapi_router(SpecAuth::Live(AuthMode::Hanko)).split_for_parts();
        let v = serde_json::to_value(&api).unwrap();
        let paths = v["paths"].as_object().unwrap();
        assert!(!paths.contains_key("/v1/auth/register"));
        assert!(!paths.contains_key("/v1/auth/login"));
        assert!(paths.contains_key("/v1/auth/hanko/session"));
    }

    #[test]
    fn committed_openapi_matches_dump() {
        let dumped = dump_openapi_json();
        let committed = include_str!("../../../../docs/src/assets/openapi.json");
        assert_eq!(
            dumped.trim_end(),
            committed.trim_end(),
            "run: cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json"
        );
    }
}
