# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Message Vault is a local, self-hosted text-message vault. It imports message-ir JSONL exports into a SQLite database and serves a local Next.js website for browsing. The sibling repo `message-vault-io` produces the JSONL exports this repo consumes.

**Docs**: The public documentation is published from the unified [bitrealm-dev.github.io](https://bitrealm-dev.github.io/) hub repo. Edit content in `docs/src/content/docs/` — there is no docs deploy workflow in this repo anymore. The hub syncs content from here at build time.

## Build and test

```bash
# Rust workspace
cargo build --workspace
cargo build --workspace --release
cargo test --workspace
cargo test -p demo-seed                        # single crate
cargo test golden_parse_cases_match_typescript  # single test by name

# Web application (Next.js 16)
cd web
npm ci
npm run lint      # eslint
npm test          # node:test via tsx
npm run build     # next build (output: "standalone")

# Docs site (Astro Starlight)
cd docs
npm ci
npm run check
npm run build

# Shared schema sync (must be clean before committing)
node scripts/sync-vault-schema.mjs --check

# Search grammar goldens (must match TypeScript)
node scripts/regen-search-goldens.mjs
git diff --exit-code fixtures/search/parse-cases.json
```

**Requirements**: Rust 1.95+ (edition 2024; `rusqlite` 0.40 needs `cfg_select!`), Node.js 20.9+, native C/C++ toolchain, optional FFmpeg for media conversion. Windows: Visual Studio Build Tools with C++ desktop workload; `.cargo/config.toml` bumps MSVC stack to 16 MiB (demo import overflows the 1 MiB default).

## Architecture

### Repository layout

```
src/                  # Single Rust binary crate (message-vault-rs CLI)
  main.rs             # clap CLI: serve, import, reset-demo, process-assets, import-contacts
  server.rs           # axum HTTP API (AppState = config + DB mutex + per-account locks)
  import.rs           # message-ir JSONL ingestion (1.7k lines), staging tables, replace/append
  dedupe.rs           # Cross-source soft-dedupe: Pass A exact, Pass B near-time
  import_media.rs     # Attachment handling: copy, none, convert, compress
  process_assets.rs   # FFmpeg-derived browser media under assets_converted/
  export_api.rs       # GET /v1/export/messages (+ /count) read-only export
  asset_uploads.rs    # Multipart + resumable part uploads by SHA-256
  search_query.rs     # Search grammar parser (must match TS web/src/lib/searchQuery.ts)
  jsonl.rs            # JSONL reader
  models.rs           # Shared types
  config.rs           # TOML config loading
  db/                 # SQLite layer: schema, accounts, API tokens, contacts, vault_imports
crates/demo-seed/     # Demo bundle generator (bin + lib): demo_seed.toml + public-domain corpus
web/                  # Next.js 16 App Router (~70 API routes, ~100 components)
  src/lib/            # Shared utilities, searchQuery.ts, vaultSchema.generated.ts
  src/app/            # App Router pages and API route handlers
  src/components/     # React components
  src/proxy.ts        # Middleware: auth gating
schema/sql/           # SOURCE OF TRUTH for DB schema (FTS5, accounts, messages, staging, contacts)
fixtures/             # search/parse-cases.json goldens, schema/current-schema.json
config/               # config.toml.example, config.docker.toml
scripts/              # setup-demo.sh, smoke tests, docker entrypoints, sync/regen scripts
demo/                 # Committed demo bundle (VCF + 390 JSONL files, ~627k messages)
```

### Data flow

```
phone backup → message-exporters (sibling repo) → message-ir JSONL + attachments
    → POST /v1/import or CLI `import` → SQLite staging tables
    → dedupe (exact + near-time) → messages / conversations / participants / attachments
    → process-assets (ffmpeg) → web UI browse
```

### HTTP API (axum)

`serve` starts the import API on `127.0.0.1:8080`. Endpoints:
- `GET /health`
- `GET /v1/auth/check` — validates bearer token
- `POST /v1/imports` / `POST /v1/imports/{id}/complete` — session management
- `POST /v1/import` — bulk message-ir JSONL import
- `GET /v1/export/messages` / `GET /v1/export/messages/count` — read-only export
- `PUT/GET/HEAD /v1/assets/{sha256}` — asset storage
- Resumable upload endpoints for large attachments

Auth uses per-account Import API tokens (SHA-256 hashed, generated from web Settings → Access). There is also a `VAULT_AUTH=hanko` mode for Hanko passwordless (used in the Bitrealm production VPS, configured in the private `message-vault-ops` repo).

`serve` keeps a warm SQLite mutex only for short import-session rows. Bulk `POST /v1/import` and export endpoints open their own connections so they don't block on the Rust mutex. Same-account imports use a per-account lock so staging is not wiped mid-run.

### Config

`config/config.toml` (gitignored; copy from `config.toml.example`):

```toml
[paths]
db = "data/vault.db"
data_dir = "data"
assets_dir = "assets"
assets_converted_dir = "assets_converted"

[server]
bind = "127.0.0.1:8080"
```

Source names are not listed in config — each import registers its own source. Asset folders live under `data/<account_id>/<source_id>/`.

### Auth model

Dual-mode, controlled by `VAULT_AUTH` env:
- **local** (default): username + password, argon2 hashed via `@node-rs/argon2`. Web accounts are created in the UI; Import API tokens are generated per-account from Settings → Access.
- **hanko**: Hanko passwordless (production VPS). Requires `NEXT_PUBLIC_HANKO_API_URL` at Next.js build time.

New accounts start with browsing edits enabled. View-only mode can be set under Settings → Access without blocking imports.

### Two cross-language contracts

**Schema sync**: `schema/sql/` is the source of truth for table definitions. `node scripts/sync-vault-schema.mjs` regenerates `web/src/lib/vaultSchema.generated.ts` and `fixtures/schema/current-schema.json`. Run with `--check` to fail if outputs are stale. Commit both the SQL edits and the generated files together.

**Search grammar**: TypeScript `web/src/lib/searchQuery.ts` is the behavior reference. Rust `src/search_query.rs` must produce identical parse results. Shared cases live in `fixtures/search/parse-cases.json`. After changing the TS parser, run `node scripts/regen-search-goldens.mjs` and then `cargo test golden_parse_cases_match_typescript` + `cd web && npm test`. Commit updated goldens with the parser changes.

### Dedupe

When importing overlapping backups from different sources (e.g., iPhone and Android), a two-pass soft dedupe runs:
- **Pass A**: exact match on sender + recipient + timestamp + text hash
- **Pass B**: near-time match on sender + recipient + close timestamps + text hash

Messages matched in either pass are skipped. Duplicate detection is cross-source; messages already stored for other sources are not re-inserted.

### Multi-tenant data model

All tables keyed by `account_id`. Core tables: `conversations`, `participants`, `messages`, `attachments`, `contacts`/`contact_handles`/`contact_labels`, `accounts`/`account_emails`/`account_phones`/`account_api_tokens`/`account_prefs`, `vault_imports` (import session tracking). FTS5 virtual table for full-text search.

### Rust crate structure

This is a workspace with two members: the root binary crate (`message-vault-rs`) and `crates/demo-seed`. There is no `lib.rs` split — all logic lives under `src/` in the binary crate. The workspace pulls `message-ir`, `contacts`, and `phone` over git from `bitrealm-dev/message-exporters` (these are the shared message schema types, not vendored locally).

`crates/demo-seed` generates the demo bundle from `demo_seed.toml` + public-domain text corpus (Pride & Prejudice sentences, name lists). Run `cargo run --release -- reset-demo` to regenerate the demo database.

### Docker/Compose

- `compose-dev.yml` (default, via `COMPOSE_FILE` in `.env`): bind-mount toolchain image, ports 3000+8080, optional sqlite-web on 127.0.0.1:8081
- `compose-release.yml`: slim multi-stage image from `Dockerfile.release`
- `docker compose up` builds and runs the full stack

## Test conventions

- **Rust**: inline `#[cfg(test)] mod tests` within each source file. DB tests use `tempfile` and in-memory/on-disk SQLite with `PRAGMA foreign_keys = ON`. The `golden_parse_cases_match_typescript` test in `src/search_query.rs` runs shared fixtures.
- **Web**: `node:test` via tsx (`npm test`). Test files colocated as `*.test.ts` (e.g., `web/src/lib/searchQuery.test.ts`). Uses `node:assert/strict`.
- **Shell smoke tests**: `scripts/smoke-import-api.sh`, `smoke-vault-push.sh`, `smoke-export-api.sh` — build release binary, start `serve` on temp port with temp config/DB, curl the API, assert with grep, cleanup via trap.

There is no CI workflow that runs `cargo test` or `npm test`. All tests are run locally.

## CI/CD

Two GitHub Actions workflows:
- **docker.yml**: on tag `v*` push or manual dispatch — builds `Dockerfile.release` and pushes to Docker Hub `mbeisser1/message-vault` (semver/latest/sha tags)
- **docs.yml**: on push to `main` touching `docs/**` — checks and deploys the Starlight site to GitHub Pages

## Key design decisions

- **Import is append-only**. Messages are never updated in place. Re-importing the same data uses the dedupe passes to skip already-stored messages.
- **Web UI is read-only by default** for viewing. Editable mode is enabled per-account. Delete is gated behind `web/src/lib/v1Capabilities.ts` (V1 product boundary).
- **`export.ini` in the sibling projects stores vault URL and key**. The vault itself stores accounts/passwords/tokens in SQLite, not in TOML config.
- **No host-level secrets in config.toml**. API tokens are per-account, hashed, and stored in SQLite.
- **Config is for paths and server bind only**; identity and auth live in the database.

## Common development workflows

### Demo setup quick start
```bash
./scripts/setup-demo.sh
cd web && npm ci && npm run dev
# Open http://localhost:3000/login — sign in as "demo" with empty password
```

### Personal data import
```bash
cp config/config.toml.example config/config.toml
# Edit config.toml: adjust [paths], ensure [server] is uncommented
cargo run --release -- serve &
cd web && npm run dev &
# Create account at http://localhost:3000
# Generate Import API token under Settings → Access
# Push JSONL from message-exporters Vault tab or vault-push CLI
```

### Full local checks before committing
```bash
cargo build --workspace && cargo test --workspace
cd web && npm run lint && npm test && npm run build
node scripts/sync-vault-schema.mjs --check
node scripts/regen-search-goldens.mjs && git diff --exit-code fixtures/search/parse-cases.json
```

## Web UI notes

- Next.js 16 App Router with `output: "standalone"` for Docker release builds
- Tailwind v4 with custom four-seed theme tokens (see `web/STYLE_GUIDE.md`)
- `web/src/proxy.ts` middleware: redirects to `/login` unless a vault account cookie exists (public paths: `/login`, `/api/auth`)
- SQLite access via `better-sqlite3` (synchronous, no async DB wrapper)
- `web/AGENTS.md` instructs reading `node_modules/next/dist/docs/` before writing Next.js code — this version has breaking changes vs training data

## Licensing

AGPL-3.0. The sibling `message-exporters` crates pulled over git (`message-ir`, `contacts`, `phone`) are MIT/Apache-2.0.
