# Host the HTTP API route catalog next to rustdoc

**Date:** 2026-08-22  
**Status:** Draft pending review

## Context

The vault HTTP process is `message-vault-server`. Other programs talk to it with URLs such as `POST /v1/import`. Route comments and JSON types in that crate already feed **utoipa** (a Rust helper that writes a machine-readable route list). A command `dump-openapi` writes that list to `docs/src/assets/openapi.json`. `cargo test` fails if the committed file does not match a fresh dump.

The public docs site is Astro Starlight on GitHub Pages at [bitrealm.io](https://bitrealm.io). Today `starlight-openapi` (an npm package) turns that JSON into many Starlight pages at `/vault/developer/reference/http/`. A short prose guide stays at `/vault/developer/reference/api/` (tokens, import flow, search syntax). That guide is not the route catalog.

Crate docs from `cargo doc` already publish at `/vault/developer/rustdoc/` (see the clap and rustdoc spec). Docs CI copies that HTML into `docs/public/vault/developer/rustdoc/`, then builds Starlight. The Developer sidebar and the Developer index already link to crate rustdoc.

`cargo doc` describes Rust modules, functions, and types. It does not draw the HTTP route list from `openapi.json`. A separate HTML page next to rustdoc has to do that.

The clap and rustdoc spec left the Starlight HTTP catalog unchanged. This spec changes **where the catalog is hosted**, not how routes are described in Rust.

## Goal

Readers looking up methods, paths, parameters, and JSON fields use a full route catalog at:

**`https://bitrealm.io/vault/developer/rustdoc/http/`**

That page is copied into the rustdoc folder during the same docs CI job that publishes crate docs. The Starlight-generated catalog at `/vault/developer/reference/http/` is removed. In-repo links are updated. There is no redirect and no stub page at the old URL.

Astro still points people at crate rustdoc (sidebar + Developer index). The HTTP prose guide points at both the new catalog and crate rustdoc.

The JSON file and `dump-openapi` stay the source of truth for the route list. The drawing tool is `@scalar/api-reference`, an npm dependency of `docs/`, installed with `npm ci` and copied onto the site. No extra file host (no CDN). Readers never talk to a running vault to read the public catalog.

## Non-goals

- Changing handler behavior, auth, or status codes
- Removing `dump-openapi` or the committed `docs/src/assets/openapi.json`
- Changing the optional live explorer on a running vault (`[server] openapi_ui`, Swagger UI at `/docs`)
- Generating TypeScript for `web/` from the JSON
- Turning rustdoc comments into the route catalog (handlers in rustdoc stay crate docs; the catalog is Scalar + JSON)
- Loading Scalar from jsDelivr or any other public file host
- Committing Scalar’s JavaScript into git (it comes from `package-lock.json` via `npm ci`)
- Keeping `/vault/developer/reference/http/` as a redirect or “moved” stub
- Browser tests of Scalar
- Failing CI on rustdoc warnings
- Rewriting User Guide pages

## Decisions

1. **Same catalog, new host.** The public route list is still the dumped JSON (tags, every `/health` and `/v1/*` route, schemas). It is not a rustdoc page per handler. It sits under `/vault/developer/rustdoc/http/` so it ships with crate docs, not as Starlight pages.

2. **Scalar from npm, copied at docs-build time.** `docs/package.json` adds `@scalar/api-reference` and drops `starlight-openapi`. After `npm ci`, a copy step takes (a) `node_modules/@scalar/api-reference/dist/browser/standalone.js`, (b) `docs/src/assets/openapi.json`, and (c) `docs/src/assets/http-api-reference.html` into `docs/public/vault/developer/rustdoc/http/` as `standalone.js`, `openapi.json`, and `index.html`. The HTML loads Scalar with a **relative** `./openapi.json`. No Scalar proxy. “Try it” against a live server is not this page.

3. **Astro catalog goes away.** Remove the `starlight-openapi` plugin, sidebar groups for each tag, and topic routes for `/vault/developer/reference/http`. Old bookmarks 404. Update links in this repo.

4. **Astro still links to rustdoc.** Keep the sidebar item “Rust crate docs” → `/vault/developer/rustdoc/`. Keep the Developer index bullet for crate docs. Change the HTTP route-reference link on that index to `/vault/developer/rustdoc/http/`. On the prose guide (`api.md`), link the catalog to the new URL and add a sentence that crate types and functions are at `/vault/developer/rustdoc/`.

5. **Sidebar HTTP item becomes one link.** “HTTP API reference” points at `/vault/developer/rustdoc/http/`, same pattern as the crate-docs sidebar link (`target: '_self'`). The prose guide slug `vault/developer/reference/api` stays.

6. **Copy after rustdoc copy.** Docs CI already deletes and replaces `docs/public/vault/developer/rustdoc/` from `target/doc/`. The HTTP folder must be created **after** that copy so crate HTML does not wipe it. `npm ci` must run before the Scalar copy.

7. **Committed JSON path unchanged.** `dump-openapi` still writes `docs/src/assets/openapi.json`. The stale-spec test is unchanged. CI copies that file into `rustdoc/http/openapi.json` for the viewer.

## Architecture

Public URLs on the same hostname:

| URL | What it is |
|-----|------------|
| `/vault/developer/rustdoc/` | Normal `cargo doc` crate list |
| `/vault/developer/rustdoc/http/` | Scalar viewer + `openapi.json` |
| `/vault/developer/reference/api/` | Hand-written Starlight guide (tokens, import, search) |

Docs CI:

1. `cargo doc --workspace --no-deps --exclude message-vault-io-gui`
2. Copy `target/doc/` → `docs/public/vault/developer/rustdoc/`
3. `npm ci` in `docs/`
4. Copy Scalar `standalone.js`, `openapi.json`, and `index.html` → `docs/public/vault/developer/rustdoc/http/`
5. `npm run check` and `npm run build` in `docs/`
6. Deploy `docs/dist`

Local `astro dev` does not include rustdoc or the HTTP catalog unless those copy steps have been run. Publish is what matters.

A running vault with `openapi_ui = true` still serves Swagger UI at `/docs`. That is a local explorer, not the public catalog.

## Components

| Piece | Role |
|-------|------|
| `crates/vault/server` utoipa + `dump-openapi` | Unchanged source of the route list |
| `docs/src/assets/openapi.json` | Committed dump; stale-spec test |
| `@scalar/api-reference` | npm dependency of `docs/`; browser file copied, not imported by Astro |
| `docs/src/assets/http-api-reference.html` | Tiny page that loads `./standalone.js` and `./openapi.json`; copied to `rustdoc/http/index.html` |
| Docs CI copy step (after rustdoc copy and `npm ci`) | Fills `rustdoc/http/` |
| `docs/astro.config.mjs` | Drop OpenAPI plugin; HTTP sidebar link; keep rustdoc sidebar link |
| `docs/src/content/docs/vault/developer/index.md` | Crate rustdoc bullet kept; catalog URL updated |
| `docs/src/content/docs/vault/developer/reference/api.md` | Catalog + crate rustdoc links |
| `[server] openapi_ui` | Unchanged live explorer |

## Data flow

**When a route changes.** Edit utoipa annotations in the server crate. Run:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

Commit the JSON in the same PR. `cargo test -p message-vault-server` must pass.

**On merge to `main`.** Docs CI rebuilds rustdoc, copies Scalar + JSON, builds Starlight, deploys Pages. Readers open `/vault/developer/rustdoc/http/`.

**From Astro.** Developer index and sidebar send people to crate rustdoc and to the HTTP catalog. The prose guide does the same in sentences.

## Error handling

- **Stale JSON:** `cargo test` fails. The message tells the developer to run `dump-openapi`. There is no silent fallback to an old catalog on a new deploy of crate code without an updated dump (same as today: the committed file is what CI copies).
- **`npm ci` or Scalar copy fails:** docs job fails. GitHub Pages is not updated. The previous site stays up.
- **`cargo doc` fails:** same as today; docs job fails.
- **Scalar cannot load `openapi.json`:** relative path is wrong or copy skipped. The copy step should fail the job if `index.html` or `openapi.json` is missing in `rustdoc/http/` after the copy.

## Testing

Existing server tests and the dump ≡ committed JSON check stay.

New or updated checks:

- After docs CI, `docs/dist/vault/developer/rustdoc/http/index.html` and `openapi.json` exist, and `standalone.js` exists next to them.
- Starlight build does **not** emit `/vault/developer/reference/http/`.
- Sidebar contains “Rust crate docs” → `/vault/developer/rustdoc/` and “HTTP API reference” → `/vault/developer/rustdoc/http/`.
- Developer index and `api.md` use the new catalog URL and still link crate rustdoc.
- `starlight-openapi` is gone from `docs/package.json`.
- `cd docs && npm run check && npm run build` still works without rustdoc/HTTP files copied in (local docs-only).

Not in scope: clicking around Scalar in a browser in CI.

## What changes

| Path | Change |
|------|--------|
| `docs/package.json` / lockfile | Add `@scalar/api-reference`; remove `starlight-openapi` |
| `docs/astro.config.mjs` | Remove OpenAPI plugin and `/http` topic routes; HTTP sidebar link; keep rustdoc sidebar link |
| `docs/src/assets/http-api-reference.html` | Committed Scalar shell; published as `rustdoc/http/index.html` |
| `.github/workflows/docs.yml` | After rustdoc copy and `npm ci`, copy HTTP catalog files |
| `docs/src/content/docs/vault/developer/index.md` | Catalog URL; keep rustdoc bullet |
| `docs/src/content/docs/vault/developer/reference/api.md` | Catalog URL; crate rustdoc sentence |
| Any other in-repo `/vault/developer/reference/http/` links | Point at `/vault/developer/rustdoc/http/` |
| `CHANGELOG.md` | Unreleased: HTTP route catalog lives next to rustdoc |

utoipa annotations, `dump-openapi`, and `openapi.json` stay. Server `openapi_ui` stays.

## Verification

- `cargo test -p message-vault-server` still includes dump ≡ committed JSON
- Docs CI artifact contains `/vault/developer/rustdoc/` and `/vault/developer/rustdoc/http/`
- Opening the catalog URL shows tags (Auth, Import, Export, …) and routes without loading a third-party script host
- `/vault/developer/reference/http/` is absent from the Astro output
- `/vault/developer/reference/api/` still builds
- Developer sidebar still has a rustdoc link that is not the HTTP catalog
- Docker / default `serve` still does not enable the live explorer unless `openapi_ui` is set

## Success criteria

- A developer looking up `POST /v1/import` uses bitrealm.io rustdoc HTTP catalog, not Starlight OpenAPI pages
- A developer looking up a workspace type still uses `/vault/developer/rustdoc/` from Astro (sidebar and Developer index)
- Changing a route without dumping JSON still fails `cargo test`
- The public catalog does not call a running vault
- Docs CI stays on the existing job; no extra website is required to draw the catalog
