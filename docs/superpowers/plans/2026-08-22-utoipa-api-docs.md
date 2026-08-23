# Generate vault HTTP API docs with utoipa Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate an OpenAPI document from every `message-vault-server` HTTP handler, commit it for Starlight at `/vault/developer/reference/http/`, and serve an opt-in Swagger UI on the vault when `[server] openapi_ui` is true.

**Architecture:** `utoipa-axum` `OpenApiRouter` registers each handler once for both Axum and the spec. `dump-openapi` serializes the full surface (including Local register/login and Hanko session) without opening SQLite. `serve` builds a live spec from routes that process actually mounted. Starlight renders the committed JSON. The existing `/vault/developer/reference/api/` page becomes a tool-writing guide.

**Tech Stack:** Axum 0.8, utoipa 5 + utoipa-axum 0.2 + utoipa-swagger-ui 9, clap, serde_json, Astro 7 / Starlight 0.41, starlight-openapi 0.26+

## Global Constraints

- Axum stays on 0.8. utoipa crates must be crates.io versions that declare Axum 0.8 (utoipa 5.x, utoipa-axum 0.2.x, utoipa-swagger-ui 9.x with `axum`).
- `[server] openapi_ui` defaults to `false`. Docker and release `serve` do not mount the explorer unless the flag is set.
- Explorer paths are `/docs` (Swagger UI) and `/openapi.json`. Opening them does not require a Bearer token. Calling a secured route from “Try it” still does.
- `dump-openapi` does not read `config.toml`, bind a port, or open SQLite.
- Dumped spec is the full handler surface. Live explorer omits register/login when `VAULT_AUTH=hanko`.
- Guide URL stays `/vault/developer/reference/api/`. Generated catalog is `/vault/developer/reference/http/`.
- Error JSON stays `{ "ok": false, "error": "…" }`. Internal bugs still return `"internal server error"`.
- `POST /v1/import` is `application/x-ndjson` (also document `application/jsonl` and `multipart/form-data` as accepted today). Asset PUT/parts are `application/octet-stream`.
- Dump JSON is `serde_json::to_string_pretty` of the `OpenApi` value (Rust struct field order is the stable key order). Same function for CLI and the stale-spec test.
- OpenAPI `info.version` is `env!("CARGO_PKG_VERSION")`.
- Do not generate TypeScript for `web/`. Do not put Message-IR JSONL record fields in OpenAPI.
- Docs copy: short sentences, no “we” / “us” / “our”. Starlight `title=""` on code fences.
- HTTP behavior of existing routes must not change. Existing server tests must keep passing.
- Docs CI stays Node-only. Stale-spec check is `cargo test -p message-vault-server`.

## File map

| File | Responsibility |
|------|----------------|
| `crates/vault/server/Cargo.toml` | utoipa dependencies |
| `crates/vault/server/src/openapi.rs` | Document info, tags, Bearer scheme, `SpecAuth`, `openapi_router`, `dump_openapi_json` |
| `crates/vault/server/src/main.rs` | `dump-openapi` subcommand; `mod openapi` |
| `crates/vault/server/src/config.rs` | `ServerConfig.openapi_ui` default false |
| `crates/vault/server/src/server.rs` | Extract `http_app`; switch to `OpenApiRouter`; mount Swagger UI; annotate handlers that live here |
| `crates/vault/server/src/auth.rs` | Auth path attributes and `ToSchema` |
| `crates/vault/server/src/profile.rs` | Account profile path attributes and `ToSchema` |
| `crates/vault/server/src/api_tokens_api.rs` | API token path attributes and `ToSchema` |
| `crates/vault/server/src/export_api.rs` | Export message schema derives (handlers stay in `server.rs`) |
| `crates/vault/server/src/contacts_api.rs` | Contact schema derives |
| `crates/vault/server/src/conversations_api.rs` | Conversation schema derives |
| `config/config.toml.example` | Comment for `openapi_ui` |
| `docs/src/assets/openapi.json` | Committed dump |
| `docs/package.json` / lockfile | `starlight-openapi` ≥ 0.26 |
| `docs/astro.config.mjs` | Plugin + sidebar |
| `docs/src/content/docs/vault/developer/reference/api.md` | Tool-writing guide |
| `docs/src/content/docs/vault/developer/reference/server-cli.md` | `dump-openapi` |
| `docs/src/content/docs/vault/developer/index.md` | Link to generated catalog |
| `CHANGELOG.md` | Unreleased notes |

`#[utoipa::path]` goes on the handler function. JSON structs used as request/response bodies get `#[derive(ToSchema)]` next to existing `Serialize`/`Deserialize`.

---

### Task 1: Dump helper and CLI

**Files:**
- Modify: `crates/vault/server/Cargo.toml`
- Create: `crates/vault/server/src/openapi.rs`
- Modify: `crates/vault/server/src/main.rs`

**Interfaces:**
- Consumes: crate version via `CARGO_PKG_VERSION`
- Produces: `pub fn dump_openapi_json() -> String`; `pub fn write_openapi(path: Option<&Path>) -> anyhow::Result<()>`; clap variant `DumpOpenapi { output: Option<PathBuf> }`

- [ ] **Step 1: Add dependencies**

In `crates/vault/server/Cargo.toml` under `[dependencies]`:

```toml
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-axum = "0.2"
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

Run: `cargo metadata -p message-vault-server --format-version 1 >/dev/null`
Expected: resolves without version conflicts with `axum 0.8`.

- [ ] **Step 2: Write the failing tests**

Create `crates/vault/server/src/openapi.rs` with tests first. Until the functions exist, the crate will not compile — put the tests under `#[cfg(test)]` after a stub, or write the tests in the same file after a `todo!()` stub. Prefer: add `mod openapi;` in `main.rs` and a file that does not compile until Step 3. Alternative that follows TDD: put tests in `openapi.rs` calling `dump_openapi_json`, then add a stub that returns `""` so the test fails on assertions.

```rust
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
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p message-vault-server dump_is_openapi_3 -- --nocapture`
Expected: FAIL (module missing, or stub JSON missing `info.title`).

- [ ] **Step 4: Implement `openapi.rs` and the CLI**

`crates/vault/server/src/openapi.rs` (this task: document metadata only; empty paths are OK):

```rust
//! OpenAPI document for message-vault-server HTTP routes.

use std::io::Write;
use std::path::Path;

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

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
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .build(),
            ),
        );
    }
}

/// Pretty OpenAPI JSON. Same string the CLI writes and the stale-spec test compares.
pub fn dump_openapi_json() -> String {
    let api = ApiDoc::openapi();
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
```

If `get_or_insert_default` is not on this utoipa version, use `get_or_insert_with(Default::default)`.

In `main.rs`:

```rust
mod openapi;
```

Add to `Commands`:

```rust
    /// Write the OpenAPI document (JSON) to stdout or --output. Does not open the database.
    DumpOpenapi {
        /// Destination file. Omit to print stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
```

In `match cli.command`:

```rust
        Commands::DumpOpenapi { output } => {
            crate::openapi::write_openapi(output.as_deref())?;
        }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p message-vault-server dump_is_openapi_3 dump_pretty_print -- --nocapture`
Expected: PASS

Run: `cargo run -p message-vault-server -- dump-openapi`
Expected: pretty JSON on stdout, process exits 0, no `config.toml` required.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/Cargo.toml crates/vault/server/src/openapi.rs crates/vault/server/src/main.rs Cargo.lock
git commit -m "$(cat <<'EOF'
feat(server): add dump-openapi command

Give the vault a stable OpenAPI JSON dump so docs can be generated
from handlers instead of a hand-written endpoint list.
EOF
)"
```

---

### Task 2: `openapi_ui` config flag

**Files:**
- Modify: `crates/vault/server/src/config.rs`
- Modify: `crates/vault/server/src/server.rs` (`test_state` `ServerConfig { ... }`)
- Modify: `config/config.toml.example`

**Interfaces:**
- Consumes: `ServerConfig` serde defaults
- Produces: `pub openapi_ui: bool` with `#[serde(default = "default_openapi_ui")]`; `fn default_openapi_ui() -> bool { false }`

- [ ] **Step 1: Write the failing test**

In `crates/vault/server/src/config.rs` `mod tests`:

```rust
    #[test]
    fn openapi_ui_defaults_false() {
        let raw = r#"
bind = "127.0.0.1:8080"
"#;
        let cfg: ServerConfig = toml::from_str(raw).unwrap();
        assert!(!cfg.openapi_ui);
    }

    #[test]
    fn openapi_ui_can_enable() {
        let raw = r#"
bind = "127.0.0.1:8080"
openapi_ui = true
"#;
        let cfg: ServerConfig = toml::from_str(raw).unwrap();
        assert!(cfg.openapi_ui);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server openapi_ui_defaults -- --nocapture`
Expected: FAIL (unknown field or missing field on `ServerConfig`).

- [ ] **Step 3: Add the field**

On `ServerConfig`:

```rust
    /// Serve Swagger UI at `/docs` and the spec at `/openapi.json`. Default false.
    #[serde(default = "default_openapi_ui")]
    pub openapi_ui: bool,
```

```rust
fn default_openapi_ui() -> bool {
    false
}
```

Update the only other struct literal in `server.rs` `test_state()`:

```rust
                server: Some(crate::config::ServerConfig {
                    bind: "127.0.0.1:0".into(),
                    asset_max_bytes: 8 * 1024 * 1024,
                    asset_part_size: 1024 * 1024,
                    asset_hash_threshold_bytes: 1024 * 1024,
                    cors_origins: Vec::new(),
                    openapi_ui: false,
                }),
```

In `config/config.toml.example` under `[server]`, after the CORS comments:

```toml
# Serve an interactive OpenAPI explorer at /docs (and /openapi.json).
# Default false. Calls from the explorer still need a Bearer token.
# openapi_ui = false
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p message-vault-server openapi_ui_ -- --nocapture`
Expected: PASS

Run: `cargo test -p message-vault-server -- --test-threads=1`
Expected: existing tests still PASS (compile after the new field).

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/config.rs crates/vault/server/src/server.rs config/config.toml.example
git commit -m "$(cat <<'EOF'
feat(server): add openapi_ui config flag

Keep the explorer off unless an operator turns it on, including
Docker and release serve.
EOF
)"
```

---

### Task 3: OpenApiRouter, health, and opt-in explorer

**Files:**
- Modify: `crates/vault/server/src/openapi.rs`
- Modify: `crates/vault/server/src/server.rs`

**Interfaces:**
- Consumes: `ApiDoc`, `ServerConfig.openapi_ui`, `AuthMode`
- Produces: `pub enum SpecAuth { Live(AuthMode), Full }`; `pub fn openapi_router(auth: SpecAuth) -> utoipa_axum::router::OpenApiRouter<crate::server::AppState>`; `pub(crate) fn http_app(state: AppState) -> axum::Router` used by `run` and tests; dump merges router spec + `ApiDoc`

This task wires **health only** onto `OpenApiRouter`. Later tasks add `.routes(routes!(...))` calls. `http_app` still registers the rest of the API with the existing `.route(...)` so behavior does not change yet. Mixing `OpenApiRouter` (health + swagger) with `.merge(Router::new().route(...existing...))` is allowed for this slice.

- [ ] **Step 1: Write failing tests**

In `openapi.rs` tests, add:

```rust
    #[test]
    fn dump_includes_health() {
        let v: serde_json::Value = serde_json::from_str(&dump_openapi_json()).unwrap();
        assert!(
            v["paths"]["/health"]["get"].is_object(),
            "expected GET /health in dump"
        );
    }
```

In `server.rs` tests, add helpers and tests. Reuse `test_state`. Build the app with `http_app` (does not exist yet):

```rust
    async fn get_path(state: AppState, path: &str) -> reqwest::Response {
        let app = http_app(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}{path}"))
            .send()
            .await
            .unwrap();
        server.abort();
        response
    }

    #[tokio::test]
    async fn health_still_ok() {
        let (_tmp, state, _token, _import_id) = test_state();
        let response = get_path(state, "/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "ok\n");
    }

    #[tokio::test]
    async fn openapi_ui_off_does_not_serve_spec() {
        let (_tmp, state, _token, _import_id) = test_state();
        assert!(!state.cfg.require_server().unwrap().openapi_ui);
        let response = get_path(state, "/openapi.json").await;
        assert_ne!(
            response.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            "application/json"
        );
    }

    #[tokio::test]
    async fn openapi_ui_on_serves_spec_without_token() {
        let (_tmp, mut state, _token, _import_id) = test_state();
        {
            let cfg = Arc::make_mut(&mut state.cfg);
            cfg.server.as_mut().unwrap().openapi_ui = true;
        }
        let response = get_path(state, "/openapi.json").await;
        assert_eq!(response.status(), StatusCode::OK);
        let v: serde_json::Value = response.json().await.unwrap();
        assert!(v["openapi"].as_str().unwrap().starts_with("3."));
    }
```

`AppState.cfg` is `Arc<Config>`. `Arc::make_mut` works if `Config: Clone`. `Config` currently has `#[derive(Debug, Clone, Deserialize)]` — it is Clone. If `make_mut` is awkward, build a second `AppState` in the test with `openapi_ui: true` instead of mutating.

If `http_app` takes ownership of `AppState` like `Router::with_state`, that matches.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p message-vault-server dump_includes_health openapi_ui_ -- --nocapture`
Expected: FAIL (`dump_includes_health` missing path; `http_app` undefined).

- [ ] **Step 3: Implement router extract + health annotation**

Make `ErrorBody` public to the crate and add `ToSchema`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorBody {
    pub ok: bool,
    pub error: String,
}
```

Annotate health (change `impl IntoResponse` to an explicit type utoipa can name):

```rust
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses((status = 200, description = "Process is up", body = String))
)]
async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok\n")
}
```

Confirm the body is still `ok\n` (existing tests and smoke scripts).

In `openapi.rs`:

```rust
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::config::AuthMode;
use crate::server::AppState;

pub enum SpecAuth {
    Live(AuthMode),
    Full,
}

pub fn openapi_router(_auth: SpecAuth) -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(crate::server::health))
}

pub fn dump_openapi_json() -> String {
    let (_router, api) = openapi_router(SpecAuth::Full).split_for_parts();
    serde_json::to_string_pretty(&api).expect("OpenAPI document serializes to JSON")
}
```

`health` must be `pub(crate)` for `routes!(crate::server::health)`.

Extract `http_app` from `run`. After `AppState` is built, replace the big `Router::new()...` with:

```rust
    let app = http_app(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    // existing eprintln lines stay
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
```

`http_app` outline:

```rust
pub(crate) fn http_app(state: AppState) -> Router {
    let openapi_ui = state
        .cfg
        .server
        .as_ref()
        .map(|s| s.openapi_ui)
        .unwrap_or(false);
    let mode = AuthMode::from_env();
    let (doc_router, spec) = crate::openapi::openapi_router(crate::openapi::SpecAuth::Live(mode))
        .split_for_parts();

    let mut api = Router::new()
        .merge(doc_router)
        .merge(auth_public_router(mode))
        // keep every existing .route(...) except /health (now on doc_router)
        .route("/v1/auth/mode", get(auth_mode_handler))
        // ... remainder unchanged ...
        .fallback_service(ServeDir::new("static"))
        .layer(build_cors_layer(&state.cfg.require_server().unwrap().cors_origins))
        .layer(RequestBodyLimitLayer::new(state.max_body_bytes));

    if openapi_ui {
        api = api.merge(
            utoipa_swagger_ui::SwaggerUi::new("/docs").url("/openapi.json", spec),
        );
    }

    api.with_state(state)
}
```

`require_server()` inside `http_app` panics tests if `server` is `None`. `test_state` always sets `server: Some`. In `run`, `require_server` already ran. Prefer cloning origins from `state.cfg.server` without extra require:

```rust
    let cors_origins = state
        .cfg
        .server
        .as_ref()
        .map(|s| s.cors_origins.clone())
        .unwrap_or_default();
```

CORS currently uses `&server.cors_origins` from the `run` local `server`. Pass the same values.

`auth_public_router` stays as today for this task (still `.route`, not OpenApiRouter).

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server dump_includes_health health_still_ok openapi_ui_ -- --nocapture`
Expected: PASS

Run: `cargo test -p message-vault-server -- --test-threads=1`
Expected: PASS (existing auth route tests still use `auth_public_router` directly).

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/openapi.rs crates/vault/server/src/server.rs
git commit -m "$(cat <<'EOF'
feat(server): serve optional OpenAPI explorer

Mount Swagger UI only when openapi_ui is true so a vault with
personal messages does not expose a click-to-call console by default.
EOF
)"
```

---

### Task 4: Annotate Auth and Account

**Files:**
- Modify: `crates/vault/server/src/auth.rs`
- Modify: `crates/vault/server/src/profile.rs`
- Modify: `crates/vault/server/src/api_tokens_api.rs`
- Modify: `crates/vault/server/src/server.rs` (`auth_mode_handler`, `auth_check`, storage handler types)
- Modify: `crates/vault/server/src/openapi.rs` (`openapi_router` registrations)

**Interfaces:**
- Consumes: `SpecAuth`, `openapi_router`
- Produces: paths listed below in the dump; `AuthModeResponse` replacing ad-hoc `serde_json::Value` for `/v1/auth/mode` without changing JSON field names

- [ ] **Step 1: Write failing tests**

```rust
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
        op["security"].as_array().is_some_and(|schemes| {
            schemes.iter().any(|s| s.get("bearer").is_some())
        })
    }
```

Public auth routes must not list the `bearer` scheme. Secured routes must include `"security": [{"bearer": []}]` (utoipa’s usual serde shape).

Also add:

```rust
    #[test]
    fn live_hanko_spec_omits_register_login() {
        let (_router, api) =
            openapi_router(SpecAuth::Live(AuthMode::Hanko)).split_for_parts();
        let v = serde_json::to_value(&api).unwrap();
        let paths = v["paths"].as_object().unwrap();
        assert!(!paths.contains_key("/v1/auth/register"));
        assert!(!paths.contains_key("/v1/auth/login"));
        assert!(paths.contains_key("/v1/auth/hanko/session"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p message-vault-server dump_includes_auth_and_account live_hanko_spec -- --nocapture`
Expected: FAIL (paths missing).

- [ ] **Step 3: Annotate and register**

Pattern for a public POST (copy for login, hanko, try-demo):

```rust
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
pub async fn register_handler(/* existing params */) -> Result<Json<AuthTokenResponse>, ApiError> {
```

Add `ToSchema` to `RegisterRequest`, `LoginRequest`, `HankoSessionRequest`, `AuthTokenResponse`. For change-password / delete-account, add `ToSchema` to those request structs too.

Pattern for a secured GET:

```rust
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
```

Replace `auth_mode_handler` JSON value with a struct (same keys: `mode`, `hanko_api_url`, `try_demo`):

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AuthModeResponse {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hanko_api_url: Option<String>,
    pub try_demo: bool,
}
```

Keep `mode` as `"hanko"` / `"local"` strings.

For `GET /v1/account/storage`, introduce a `ToSchema` struct matching the existing JSON (`total_bytes`, `attachment_count`, `top_attachments`). Do not change field names.

Register in `openapi_router`:

```rust
pub fn openapi_router(auth: SpecAuth) -> OpenApiRouter<AppState> {
    let mut router = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(crate::server::health))
        .routes(routes!(crate::auth::hanko_session_handler))
        .routes(routes!(crate::auth::try_demo_handler))
        .routes(routes!(crate::server::auth_mode_handler))
        .routes(routes!(crate::server::auth_check))
        .routes(routes!(crate::auth::logout_handler))
        .routes(routes!(crate::auth::change_password_handler))
        .routes(routes!(crate::auth::delete_account_handler))
        .routes(routes!(crate::profile::account_profile_handler))
        .routes(routes!(crate::profile::account_profile_update_handler))
        .routes(routes!(crate::profile::delete_messages_handler))
        .routes(routes!(crate::server::account_storage_handler))
        .routes(routes!(crate::api_tokens_api::list_api_tokens_handler))
        .routes(routes!(crate::api_tokens_api::create_api_token_handler))
        .routes(routes!(crate::api_tokens_api::delete_api_token_handler))
        .routes(routes!(crate::api_tokens_api::rename_api_token_handler));

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
```

Then **remove** the matching `.route(...)` entries from `http_app` / `auth_public_router` so each handler is registered once. `auth_public_router` should become OpenApiRouter routes merged in `http_app`, or `auth_public_router` should take `OpenApiRouter` and merge. Preferred: drop `auth_public_router`’s `.route` for hanko/try-demo/register/login and let `openapi_router` own them. Keep the 32 KiB body limit layer on those auth POSTs: nest them in a sub-router with `RequestBodyLimitLayer::new(32 * 1024)` **after** split, or apply the layer on a merged Axum router of only those paths.

Preserve today’s split: unauthenticated auth JSON has a 32 KiB cap; the rest of the app uses `max_body_bytes`. After `split_for_parts()`, something equivalent to:

```rust
    let (auth_small, spec_auth) = /* router with hanko, try-demo, optional register/login */;
    let auth_small = auth_small.layer(RequestBodyLimitLayer::new(32 * 1024));
```

If combining into one `OpenApiRouter` makes the 32 KiB layer apply globally, **do not do that**. Split `openapi_router` into `auth_public_openapi(auth)` (32 KiB) and `api_openapi()` (default limit), `split_for_parts` each, merge specs with `spec.merge(...)`, merge Axum routers with the correct layers.

`dump_openapi_json` must merge both specs:

```rust
pub fn dump_openapi_json() -> String {
    let (_a, mut spec) = auth_public_openapi(SpecAuth::Full).split_for_parts();
    let (_b, rest) = api_openapi().split_for_parts();
    spec.merge(rest);
    serde_json::to_string_pretty(&spec).expect("OpenAPI document serializes to JSON")
}
```

utoipa `OpenApi::merge` is the method to use. If the name differs in v5, use the documented combine API — the dump must contain every path.

Handlers this task must cover:

| Method | Path | Tag | Security |
|--------|------|-----|----------|
| POST | `/v1/auth/register` | Auth | none |
| POST | `/v1/auth/login` | Auth | none |
| POST | `/v1/auth/hanko/session` | Auth | none |
| POST | `/v1/auth/try-demo` | Auth | none |
| GET | `/v1/auth/mode` | Auth | none |
| GET | `/v1/auth/check` | Auth | bearer |
| POST | `/v1/auth/logout` | Auth | bearer |
| POST | `/v1/auth/change-password` | Auth | bearer |
| POST | `/v1/auth/delete-account` | Auth | bearer |
| GET | `/v1/account/profile` | Account | bearer |
| POST | `/v1/account/profile` | Account | bearer |
| POST | `/v1/account/delete-messages` | Account | bearer |
| GET | `/v1/account/storage` | Account | bearer |
| GET | `/v1/account/api-tokens` | Account | bearer |
| POST | `/v1/account/api-tokens` | Account | bearer |
| PATCH | `/v1/account/api-tokens/{id}` | Account | bearer |
| DELETE | `/v1/account/api-tokens/{id}` | Account | bearer |

Path params use Axum 0.8 `{id}` syntax (same as OpenAPI).

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server dump_includes_auth_and_account live_hanko_spec -- --nocapture`
Expected: PASS

Run: `cargo test -p message-vault-server -- --test-threads=1`
Expected: PASS (`hanko_router_excludes_local_auth_routes` still 404s register/login in Hanko live router).

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/auth.rs crates/vault/server/src/profile.rs crates/vault/server/src/api_tokens_api.rs crates/vault/server/src/server.rs crates/vault/server/src/openapi.rs
git commit -m "$(cat <<'EOF'
feat(server): document auth and account OpenAPI paths

Register sign-in and settings routes in the spec so the explorer
and the docs site match the handlers the website already calls.
EOF
)"
```

---

### Task 5: Annotate browse routes (export, contacts, conversations, tags)

**Files:**
- Modify: `crates/vault/server/src/export_api.rs` (`ToSchema` on export responses)
- Modify: `crates/vault/server/src/contacts_api.rs`
- Modify: `crates/vault/server/src/conversations_api.rs`
- Modify: `crates/vault/server/src/contact_groups_api.rs` / `thread_tags_api.rs` if request/response types live there
- Modify: `crates/vault/server/src/server.rs` (handler attributes)
- Modify: `crates/vault/server/src/openapi.rs`

**Interfaces:**
- Consumes: `api_openapi()` from Task 4
- Produces: dump paths listed below

- [ ] **Step 1: Write failing test**

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server dump_includes_browse_paths -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Annotate and register**

Add `ToSchema` to `ExportMessagesResponse`, `ExportCountResponse`, `ExportMessage`, and nested attachment/tapback structs in `export_api.rs`. Add `ToSchema` to contact and conversation page structs.

Example for export (query params must match the existing `Query` extractor fields):

```rust
#[utoipa::path(
    get,
    path = "/v1/export/messages",
    tag = "Export",
    security(("bearer" = [])),
    params(
        ("q" = String, Query, description = "Metadata search subset; empty is all non-trashed"),
        ("limit" = Option<usize>, Query, description = "Page size, default 100, max 500"),
        ("offset" = Option<usize>, Query, description = "Legacy offset; prefer cursor"),
        ("cursor" = Option<String>, Query, description = "Opaque next_cursor from a previous page"),
        ("account" = Option<String>, Query),
        ("source" = Option<String>, Query)
    ),
    responses(
        (status = 200, body = crate::export_api::ExportMessagesResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody)
    )
)]
async fn export_messages_handler(...) -> Result<Json<ExportMessagesResponse>, ApiError> {
```

`ExportMessagesQuery` fields are `q: String`, `limit: Option<usize>`, `offset: Option<usize>`, `cursor: Option<String>`, `account: Option<String>`, `source: Option<String>`. `ExportMessagesCountQuery` is `q`, `account`, `source` only. `AssetPutQuery` is `source: String`, `account: Option<String>`. `ImportQuery` is `source`, `account`, `mode`, `dedupe`, `import_id`, `contact_name_mode`.

All browse routes are bearer-secured. Tags: Export for `/v1/export/messages*`; Contacts for contact and contact-group paths; Conversations for `/v1/export/conversations*`; Thread tags for `/v1/thread-tags*` and `/v1/conversations/tags`.

Remove the old `.route(...)` for each handler once it is on `api_openapi()`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server dump_includes_browse_paths -- --nocapture`
Expected: PASS

Run: `cargo test -p message-vault-server -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/export_api.rs crates/vault/server/src/contacts_api.rs crates/vault/server/src/conversations_api.rs crates/vault/server/src/contact_groups_api.rs crates/vault/server/src/thread_tags_api.rs crates/vault/server/src/server.rs crates/vault/server/src/openapi.rs
git commit -m "$(cat <<'EOF'
feat(server): document browse OpenAPI paths

Cover messages, contacts, conversations, and tags so the generated
reference is not limited to import and export CLI routes.
EOF
)"
```

---

### Task 6: Annotate import and assets

**Files:**
- Modify: `crates/vault/server/src/server.rs` (import/asset handlers, `CreateImportBody`, `ImportQuery`, binary responses)
- Modify: `crates/vault/server/src/openapi.rs`

**Interfaces:**
- Consumes: `api_openapi()`
- Produces: dump paths listed below; import body documented as ndjson/jsonl/multipart; asset PUT as octet-stream

- [ ] **Step 1: Write failing test**

```rust
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
        assert!(
            import.get("application/x-ndjson").is_some()
                || import.get("application/jsonl").is_some(),
            "POST /v1/import must document JSONL, not a fake JSON object"
        );
        let put = &paths["/v1/assets/{sha256}"]["put"]["requestBody"]["content"];
        assert!(
            put.get("application/octet-stream").is_some(),
            "PUT asset must be raw bytes"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server dump_includes_import_and_asset_paths -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Annotate**

`ToSchema` on `CreateImportBody`, `CreateImportResponse`, `CompleteImportBody`, `CompleteImportResponse`, `ImportResponse`, `ImportQuery` fields as query params.

For JSONL import, do **not** use `request_body = SomeStruct`. Use:

```rust
#[utoipa::path(
    post,
    path = "/v1/import",
    tag = "Import",
    security(("bearer" = [])),
    params(
        ("source" = String, Query),
        ("account" = Option<String>, Query),
        ("mode" = Option<String>, Query, description = "Default append"),
        ("dedupe" = Option<bool>, Query),
        ("import_id" = Option<i64>, Query),
        ("contact_name_mode" = Option<String>, Query)
    ),
    request_body(
        content_type = "application/x-ndjson",
        description = "message-ir JSONL. application/jsonl and multipart/form-data (field jsonl plus file parts) are also accepted."
    ),
    responses(
        (status = 200, body = ImportResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody)
    )
)]
```

Match real `ImportQuery` field names in `server.rs`.

For `PUT /v1/assets/{sha256}`:

```rust
    request_body(content_type = "application/octet-stream", description = "Raw asset bytes")
```

HEAD/GET asset: no JSON body; GET is raw bytes (`application/octet-stream` response).

Register remaining handlers on `api_openapi()` and delete leftover `.route(...)` entries. After this task, `http_app` should not list `/v1` paths with `.route` except if a path was missed — grep `.route(` in `server.rs` and `openapi.rs` and confirm every `/v1` and `/health` path exists in the dump.

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server dump_includes_import_and_asset_paths -- --nocapture`
Expected: PASS

Run: `cargo test -p message-vault-server -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/openapi.rs crates/vault/server/src/asset_uploads.rs
git commit -m "$(cat <<'EOF'
feat(server): document import and asset OpenAPI paths

Describe JSONL ingest and raw asset bytes as non-JSON content types
so the spec does not invent a JSON object for a photo.
EOF
)"
```

---

### Task 7: Commit the dump and fail tests on drift

**Files:**
- Create: `docs/src/assets/openapi.json`
- Modify: `crates/vault/server/src/openapi.rs` (stale-spec test)

**Interfaces:**
- Consumes: `dump_openapi_json()`
- Produces: committed JSON identical to `dump_openapi_json()`; test `committed_openapi_matches_dump`

- [ ] **Step 1: Write the failing test**

```rust
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
```

Path from `crates/vault/server/src/openapi.rs` to repo `docs/` is `../../../../docs/` (`src` → `server` → `vault` → `crates` → repo root). Confirm with a failed compile if the path is wrong, then fix the `include_str!`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server committed_openapi_matches_dump -- --nocapture`
Expected: FAIL (file missing or mismatch).

- [ ] **Step 3: Write the file**

From repository root:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

Do not hand-edit the JSON.

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server committed_openapi_matches_dump -- --nocapture`
Expected: PASS

Run: `cargo test -p message-vault-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add docs/src/assets/openapi.json crates/vault/server/src/openapi.rs
git commit -m "$(cat <<'EOF'
test(server): fail when committed OpenAPI dump drifts

Keep bitrealm.io from publishing a spec that does not match the
handlers in the same commit.
EOF
)"
```

---

### Task 8: Starlight catalog and tool-writing guide

**Files:**
- Modify: `docs/package.json`, `docs/package-lock.json`
- Modify: `docs/astro.config.mjs`
- Modify: `docs/src/content/docs/vault/developer/reference/api.md`
- Modify: `docs/src/content/docs/vault/developer/reference/server-cli.md`
- Modify: `docs/src/content/docs/vault/developer/index.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `docs/src/assets/openapi.json`
- Produces: pages under `/vault/developer/reference/http/`; guide remains `/vault/developer/reference/api/`

- [ ] **Step 1: Install the plugin**

From `docs/`:

```bash
npm install starlight-openapi@^0.26.0
```

Use 0.26 or newer so it supports Astro 7 / Starlight 0.41. If `npm run check` fails on the plugin, stop and use the spec fallback: one MDX page at `docs/src/content/docs/vault/developer/reference/http.mdx` that embeds Scalar pointed at `/src/assets/openapi.json`. Do not resurrect endpoint tables.

- [ ] **Step 2: Wire Starlight**

In `docs/astro.config.mjs`:

```javascript
import starlightOpenAPI, { openAPISidebarGroups } from 'starlight-openapi';
```

Inside `starlight({ plugins: [` **before** `starlightSidebarTopics`:

```javascript
        starlightOpenAPI([
          {
            base: 'vault/developer/reference/http',
            schema: './src/assets/openapi.json',
            label: 'HTTP API reference',
          },
        ]),
```

In `developerItems`, immediately after `'vault/developer/reference/api'`, insert:

```javascript
  {
    label: 'HTTP API reference',
    items: openAPISidebarGroups,
  },
```

If `openAPISidebarGroups` is empty until the plugin runs, follow the plugin README for sidebar-topics: the group must appear under Developer, not User Guide.

- [ ] **Step 3: Rewrite the guide**

Replace `docs/src/content/docs/vault/developer/reference/api.md` with this page (keep the URL). Do not include a method/path table.

```markdown
---
title: HTTP API
description: Tokens, import sessions, search syntax, and JSONL upload for people writing tools against the vault.
---

Route schemas, status codes, and JSON fields live in the generated [HTTP API reference](/vault/developer/reference/http/). This page is the prose those tools need that is not a JSON schema.

`message-vault-server serve` reads `[server]` in `config/config.toml` (`bind`). Day-to-day import still uses the desktop [Import](/vault/user/import-from-a-backup/) screen or **`vault-push`**. Download uses [Export](/vault/user/how-to/export-from-the-vault/) or **`vault-pull`**. Those tools call the HTTP API with [JSONL](/vault/developer/reference/export-structure/) and attachment bytes keyed by SHA-256.

## Tokens

Auth is per-account. There is no host-wide admin token.

Create a named **API token** under **Settings → Account** (shown once). Copy that value into `vault-push` / `vault-pull`. A website login uses a **session** Bearer that rotates on each login. Do not paste a session token into CLI tools.

Send either token as:

```http title="Bearer header"
Authorization: Bearer <token>
```

An API token may import (write) and export messages and assets (read). It may not change profile, settings, or browse-only website routes. Export routes never delete vault data.

Turn on a local explorer with `[server] openapi_ui = true`, then open `/docs` on that vault. The explorer is off by default. “Try it” still sends this header.

## Import session

`vault-push` starts a session with `POST /v1/imports`, passes `import_id` on each `POST /v1/import`, then `POST /v1/imports/{id}/complete` so Settings → Storage can list history. Messages promoted in that session store `messages.import_id`.

If `import_id` is omitted on `POST /v1/import`, the server starts and finishes a one-shot session so Storage still records the import.

Bulk `POST /v1/import` opens its own SQLite connection so it does not hold the serve process’s short session mutex across JSONL and asset work. Same-account imports stay serialized. Export and auth open their own connections and can proceed under WAL while an import runs.

## Import body

- `Content-Type: application/jsonl` or `application/x-ndjson` — body only; attachments already uploaded by SHA-256
- `Content-Type: multipart/form-data` — field `jsonl` plus `file` parts (relative paths such as `attachments/photo.jpg`)

Request body limit matches `[server] asset_max_bytes` (default 512 MiB).

HTTP `mode` defaults to `append` (CLI `import` defaults to `replace`). HTTP `dedupe` defaults to false (CLI runs dedupe unless `--skip-dedupe`). HTTP `source` is a required query parameter. `account` is optional when the Bearer token already identifies the tenant.

## Search operators (`q`)

Export uses a **metadata** search subset (sender, participants, contact `preferred_name`, attachment names/MIME, dates, source, group/direct, labels). It does **not** run the website full-text `messages_fts` path.

- Free text terms and `"quoted phrases"` (AND); `-term` / `-"phrase"` to exclude
- `from:`, `with:` / `to:`, `subject:`, `has:attachment`
- `after:YYYY-MM-DD`, `before:YYYY-MM-DD` (year-only `YYYY` → `YYYY-01-01`)
- `source:`, `is:group`, `is:direct` (individual)
- `people:` / `within:` / `label:` (threads that involve a contact group)
- `-people:` (hide those threads)
- `tag:` / `-tag:` (thread tags; `tag:none` for untagged threads)
- Trash is always excluded; legacy `in:trash` is ignored
- `search:contacts` on message export returns `400`

## Verify a token

```bash title="Verify a token"
curl -sS "http://127.0.0.1:8080/v1/auth/check" \
  -H "Authorization: Bearer <import-api-token-from-settings>"
```

## Smoke tests

```bash title="Smoke tests"
./scripts/test/smoke-import-api.sh
./scripts/test/smoke-vault-push.sh
./scripts/test/smoke-export-api.sh
```

Health check: <http://127.0.0.1:8080/health>
```

- [ ] **Step 4: Server CLI, developer index, changelog**

In `server-cli.md` command table add:

```markdown
| `dump-openapi` | Write the OpenAPI JSON to stdout or `--output`. Does not open the database |
```

After the `serve` section (or before Examples), add:

```markdown
## `dump-openapi`

Write the HTTP OpenAPI document. Used to refresh `docs/src/assets/openapi.json`.

```bash title="dump-openapi"
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

No `--config`. The committed file must match this output; `cargo test -p message-vault-server` checks that.
```

In `docs/src/content/docs/vault/developer/index.md`, change the HTTP API bullet to:

```markdown
- [HTTP API](/vault/developer/reference/api/) — tokens and import flow; [route reference](/vault/developer/reference/http/)
```

In `CHANGELOG.md` under `[Unreleased]` **Added**:

```markdown
- Generated OpenAPI reference for the vault HTTP API on the docs site, plus an optional explorer at `/docs` when `[server] openapi_ui` is true
```

- [ ] **Step 5: Verify the docs site**

Run: `cd docs && npm run check && npm run build`
Expected: success. `docs/dist/vault/developer/reference/api/index.html` exists. `docs/dist/vault/developer/reference/http/` exists (plugin may emit `index.html` plus per-operation pages). Grep `dist` for a known path string `/v1/auth/check`.

If the plugin cannot emit `/vault/developer/reference/http/`, use the Scalar fallback page at that slug so the URL in the spec still works.

- [ ] **Step 6: Commit**

```bash
git add docs/package.json docs/package-lock.json docs/astro.config.mjs docs/src/content/docs/vault/developer/reference/api.md docs/src/content/docs/vault/developer/reference/server-cli.md docs/src/content/docs/vault/developer/index.md CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs: host generated HTTP API reference on Starlight

Keep the existing API URL as a tool-writing guide and publish the
utoipa dump as the route catalog so endpoint tables stop drifting.
EOF
)"
```

---

## Self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| utoipa-axum OpenApiRouter, one registration | 3–6 |
| Document every `/health` and `/v1` handler | 3–6 path tests |
| Committed `docs/src/assets/openapi.json` | 7 |
| Starlight at `/vault/developer/reference/http/` | 8 |
| Guide stays `/vault/developer/reference/api/` | 8 |
| `openapi_ui` default false; `/docs` + `/openapi.json` | 2, 3 |
| Dump without DB/config | 1 |
| Full dump includes register/login; Hanko live omits them | 4 |
| Bearer on secured routes; public auth unsecured | 4 |
| ErrorBody unchanged | 3 (`ToSchema` only) |
| JSONL + octet-stream | 6 |
| Stale-spec `cargo test` | 7 |
| No TypeScript codegen / no IR schema in OpenAPI | out of scope; no task |
| CHANGELOG, config example, server-cli | 2, 8 |
| Existing HTTP tests still pass | 3–6 run full crate tests |
