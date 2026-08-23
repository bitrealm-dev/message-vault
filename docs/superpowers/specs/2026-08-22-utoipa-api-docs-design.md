# Generate vault HTTP API docs with utoipa

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

`message-vault-server` is the vault HTTP process. It stores messages in SQLite and serves `/v1/*` with Axum 0.8. The website (`web/`), the desktop app, `vault-push`, and `vault-pull` all call that API. A Bearer token in the `Authorization` header is how a caller proves who they are. A **session token** comes from login and can call browse and settings routes. An **API token** is created under Settings → Account and can import and export only.

Axum registers about forty routes in `crates/vault/server/src/server.rs`: health, auth, account, import, assets, message export, contacts, conversations, contact groups, and thread tags. Request and response bodies are already serde structs.

The published developer page at `docs/src/content/docs/vault/developer/reference/api.md` (live URL `/vault/developer/reference/api/`) is hand-written. It lists import, export, assets, and token check. It does not list most routes the website actually calls. There is no OpenAPI file in the repo. There is no interactive explorer on a running vault. Frontend TypeScript types are hand-written and are not generated from the server.

**utoipa** is a Rust crate that builds an OpenAPI document from handler attributes and schema derives. OpenAPI is a machine-readable description of HTTP routes, query parameters, and JSON bodies. **utoipa-axum** registers a handler on the Axum router and in that document at the same time. It supports Axum 0.8.

The docs site is Astro Starlight, published to GitHub Pages at bitrealm.io. Docs CI (`docs.yml`) is Node-only. It does not compile Rust. The Rust test job ignores PRs that only touch `docs/`.

Sign-in mode is chosen by `VAULT_AUTH`. **Local** (default) mounts `POST /v1/auth/register` and `POST /v1/auth/login`. **Hanko** does not mount those two; the website uses `POST /v1/auth/hanko/session` instead. Other `/v1` routes are the same in both modes.

## Goal

The OpenAPI document generated from `message-vault-server` handlers is the source of truth for the HTTP API. Every API handler in the crate is documented (`/health` and all `/v1/*` routes), not only import and export. A given process may omit register/login when `VAULT_AUTH=hanko`; those routes still appear in the committed spec.

That document is committed under `docs/` and hosted by Starlight on bitrealm.io as a browsable reference at `/vault/developer/reference/http/` (not the existing guide URL). It is grouped by tags (Auth, Account, Import, Export, Assets, Contacts, Conversations, Thread tags, Health). Tool authors start at Import, Export, and Assets. A short markdown guide stays at `/vault/developer/reference/api/` for session vs API token, import session flow, search syntax, and JSONL plus asset upload. That guide does not list every method and path.

A running vault can serve Swagger UI (an interactive explorer) and the JSON spec when `[server] openapi_ui` is true. The default is false, including Docker and release builds.

A `dump-openapi` command writes the spec without opening the database or binding a port. `cargo test` fails if the committed JSON does not match the dump.

## Non-goals

- Generating TypeScript for `web/` from the spec
- Putting Message-IR JSONL record fields into OpenAPI (those stay on the export-structure page)
- Turning the explorer on by default in Docker or release builds
- Requiring a token to open Swagger UI (calls still require a token)
- Browser tests of Swagger UI
- Replacing `vault-push` / `vault-pull` with a generated client
- Documenting the static website fallback (`ServeDir` on `static/`) as an API
- Changing auth, scopes, or route behavior

## Decisions

1. **utoipa-axum `OpenApiRouter`.** Each handler is registered once. That registration builds the Axum router and the OpenAPI document. Annotating handlers while keeping a separate `.route(...)` list is rejected because it recreates two lists to keep in sync.

2. **Document every API route.** The website, CLI tools, and future scripts share one spec. Tags are the subsections. Prose guides sit beside the spec; they do not replace it.

3. **Committed spec on Starlight, plus an opt-in explorer.** `dump-openapi` writes `docs/src/assets/openapi.json`. That file is committed. Starlight renders it on bitrealm.io. Docs CI stays Node-only. The explorer is a separate, local surface.

4. **`[server] openapi_ui` defaults to false.** When true, `serve` mounts Swagger UI at `/docs` and the spec at `/openapi.json`. Opening those URLs does not require a token. “Try it” still sends the Bearer header and the server still enforces session vs API-token scopes. When the flag is false, those paths are not API routes (they fall through to `static/` like any unknown path).

5. **Dumped spec is the full surface.** It includes register and login (Local sign-in mode) and the Hanko session route, with a note that they depend on `VAULT_AUTH`. A running explorer lists only routes that process actually mounted, so “Try it” does not offer register/login on a Hanko vault.

6. **Starlight plugin.** Use `starlight-openapi` to turn the committed JSON into pages under `/vault/developer/reference/http/`. If that plugin cannot ship with the current Astro 7 / Starlight 0.41 docs site, the approved fallback is one Starlight page at that same path that embeds Scalar API Reference pointed at the same JSON. Do not hand-write endpoint tables again. Do not put generated pages at `/vault/developer/reference/api/` — that URL stays the guide.

7. **Error body stays `{ "ok": false, "error": "…" }`.** The spec reuses that schema. Internal failures still return the public string `"internal server error"`.

8. **Non-JSON bodies stay non-JSON.** `POST /v1/import` is `application/x-ndjson`. Asset PUT and part uploads are `application/octet-stream`. The spec does not invent a JSON object for a photo.

9. **No TypeScript codegen in this work.** Hand-written `web/` types stay. A later project can generate a client from the committed JSON.

10. **Dump JSON is pretty-printed and stable.** `dump-openapi` writes formatted JSON with a deterministic key order (the same serializer the test uses). The stale-spec test compares strings. A product version bump in `crates/vault/server/Cargo.toml` changes `info.version` in the spec, so the committed JSON is regenerated in that same release PR.

## Architecture

utoipa lives inside `message-vault-server`. A small `openapi.rs` module owns document metadata: title “Message Vault HTTP API”, version equal to the crate version (today `0.7.3`), Bearer security scheme, and tags. Handlers stay in the modules they already live in. Each handler gets `#[utoipa::path]`. JSON request and response structs get `ToSchema`.

`server.rs` switches route setup from `Router::new().route(...)` to `OpenApiRouter`. CORS, body-size limits, and the `static/` fallback stay on the Axum `Router` after `split_for_parts()`.

Two consumers share generation, not a running vault:

| Consumer | When | What it shows |
|----------|------|----------------|
| bitrealm.io (Starlight) | Every docs deploy | Full spec from committed JSON, at `/vault/developer/reference/http/` |
| Running vault explorer | `openapi_ui = true` | Spec for routes that process mounted |

Layers and auth extractors do not change. Guest/demo account restrictions stay enforced in handlers.

## Components

| Piece | Role |
|-------|------|
| `utoipa`, `utoipa-axum`, `utoipa-swagger-ui` | Generate spec, register routes, serve explorer. Use crates.io versions compatible with Axum 0.8 |
| `crates/vault/server/src/openapi.rs` | Document info, tags, security scheme, dump helper |
| Handler modules (`auth.rs`, `export_api.rs`, …) | `#[utoipa::path]` and `ToSchema` next to existing serde types |
| `[server] openapi_ui` | Boolean, default `false`, documented in `config/config.toml.example` |
| `message-vault-server dump-openapi` | Write spec to stdout or `--output <path>`. No SQLite, no bind, no `config.toml` |
| `docs/src/assets/openapi.json` | Committed dump. Source for Starlight |
| `starlight-openapi` (or Scalar fallback) | Render the JSON on bitrealm.io |
| `docs/src/content/docs/vault/developer/reference/api.md` | Tool-writing guide at the existing URL. Links into the generated reference |
| Server crate test | Fail if dump ≠ committed JSON |

## Data flow

**Public docs.** A handler change updates `#[utoipa::path]` and schemas in the same edit. From the repository root:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

That file is committed in the same PR. GitHub Pages builds Starlight from `docs/`. A reader on bitrealm.io never talks to a vault.

**Live explorer.** `serve` starts as today. If `openapi_ui` is false, nothing extra is mounted. If true, `/docs` loads Swagger UI from this process and `/openapi.json` serves this process’s spec. “Try it” is a real HTTP call to that vault. The developer pastes a Bearer token in the UI Authorize box. The spec file contains no tokens.

**Auth in the spec.** HTTP Bearer is declared once. Routes that need a session or API token are marked secured. Public routes are not: `/health`, `/v1/auth/mode`, login, register, Hanko session, try-demo.

## Error handling

Existing `ApiError` status codes and `ErrorBody` stay. The spec lists per-route statuses (401, 403, 400, 404, 409, 429, 503, 500 as each handler already returns). No new error format.

`dump-openapi` exits non-zero and prints a plain stderr message if the output path cannot be written. CI treats that as a failed check.

A missing `Authorization` header on a secured route is still 401 with `{ "ok": false, "error": "…" }`. The explorer does not bypass that.

If someone adds a route and skips the dump, the committed-JSON test fails. There is no runtime fallback that silently serves an old spec on bitrealm.io.

## Testing

Existing handler tests keep passing after the `OpenApiRouter` rewrite. HTTP behavior does not change.

New tests:

- Dump JSON parses as OpenAPI 3 and includes `/health` and `/v1/auth/check`
- `openapi_ui = false` does not mount `/docs` as an API route
- `openapi_ui = true` serves `/docs` and `/openapi.json` without a token
- A secured route in the spec requires Bearer; a public route does not
- Dump output equals `docs/src/assets/openapi.json` (this is the stale-spec check; it runs under `cargo test -p message-vault-server`)

The Rust CI job already runs `cargo test --workspace`. No extra workflow file is required for the stale-spec check. Docs-only PRs still skip that job (existing `paths-ignore` for `docs/**`). A handler change without an updated JSON fails on the server PR, which is the case that matters.

Not in scope: browser tests of Swagger UI; generating `web/` types.

## What changes

| Path | Change |
|------|--------|
| `crates/vault/server/Cargo.toml` | Add utoipa, utoipa-axum, utoipa-swagger-ui |
| `crates/vault/server/src/openapi.rs` | New: document info, tags, dump |
| `crates/vault/server/src/main.rs` | `dump-openapi` subcommand; `mod openapi` |
| `crates/vault/server/src/config.rs` | `openapi_ui: bool` default false |
| `crates/vault/server/src/server.rs` | `OpenApiRouter`; mount explorer when enabled |
| `crates/vault/server/src/auth.rs` and other API modules | `#[utoipa::path]`, `ToSchema` |
| `config/config.toml.example` | Comment for `openapi_ui` |
| `docs/src/assets/openapi.json` | Generated, committed |
| `docs/package.json` / lockfile | `starlight-openapi` (or Scalar fallback) |
| `docs/astro.config.mjs` | Plugin + sidebar entries for `/vault/developer/reference/http/` |
| `docs/src/content/docs/vault/developer/reference/api.md` | Rewrite as tool-writing guide; drop endpoint tables; link to `/vault/developer/reference/http/` |
| `docs/src/content/docs/vault/developer/reference/server-cli.md` | Document `dump-openapi` |
| `CHANGELOG.md` | Unreleased: generated HTTP API reference and optional explorer |

Links that already point at `/vault/developer/reference/api/` keep working. They land on the guide, which points at the generated reference.

## Verification

- `cargo test -p message-vault-server` passes, including dump ≡ committed JSON
- Existing server handler tests still pass
- `dump-openapi` runs without `config.toml` or a database
- With `openapi_ui = false`, `/docs` is not the explorer
- With `openapi_ui = true`, `/docs` loads and `/openapi.json` is valid OpenAPI 3
- Starlight build (`cd docs && npm run check && npm run build`) includes `/vault/developer/reference/http/`
- `/vault/developer/reference/api/` is still the guide, not the generated catalog
- The old endpoint table is gone from `api.md`; token/session, import flow, search syntax, and JSONL + assets remain as prose
- `config.toml.example` documents `openapi_ui`
- Docker / default `serve` does not enable the explorer unless the flag is set

## Success criteria

- A developer looking up a `/v1` route finds it on bitrealm.io without reading `server.rs`
- Adding a route without updating annotations or the committed JSON fails `cargo test`
- Someone writing `vault-push`-style tooling can follow the markdown guide plus Import / Export / Assets tags
- A self-hosted vault does not serve an API console unless `openapi_ui` is turned on
- Login, profile, contacts, and tags are documented, not only import/export
