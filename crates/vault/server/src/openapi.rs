//! OpenAPI document for message-vault-server HTTP routes.

use std::io::Write;
use std::path::Path;

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::config::AuthMode;
use crate::server::AppState;

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

#[allow(dead_code)] // Live(AuthMode) is matched when later tasks filter routes by sign-in mode.
pub enum SpecAuth {
    Live(AuthMode),
    Full,
}

pub fn openapi_router(_auth: SpecAuth) -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(crate::server::health))
}

/// Pretty OpenAPI JSON. Same string the CLI writes and the stale-spec test compares.
pub fn dump_openapi_json() -> String {
    let (_router, api) = openapi_router(SpecAuth::Full).split_for_parts();
    serde_json::to_string_pretty(&api).expect("OpenAPI document serializes to JSON")
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
    use super::dump_openapi_json;

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
}
