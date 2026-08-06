# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Message Vault is a local, self-hosted text-message vault. It imports message-ir JSONL exports into a SQLite database and serves a read-only REST API for browsing, search, and export. The sibling repo `message-vault-io` provides the Vite SPA frontend that talks to this API, and also produces the JSONL exports this repo consumes.

**Docs**: The public documentation is published from the unified [bitrealm-dev.github.io](https://bitrealm-dev.github.io/) hub repo. Edit content in `docs/src/content/docs/` — there is no docs deploy workflow in this repo anymore. The hub syncs content from here at build time.

## Build and test

```bash
# Rust workspace
cargo build --workspace
cargo build --workspace --release
cargo test --workspace
cargo test -p demo-seed                        # single crate
cargo test golden_parse_cases_match_typescript  # single test by name

# Docs site (Astro Starlight)
cd docs
npm ci
npm run check
npm run build

# Shared schema sync (must be clean before committing)
node scripts/sync-vault-schema.mjs --check

# Search grammar goldens (must match TypeScript in message-vault-io)
node scripts/regen-search-goldens.mjs
git diff --exit-code fixtures/search/parse-cases.json
```

**Requirements**: Rust 1.95+ (edition 2024; `rusqlite` 0.40 needs `cfg_select!`), Node.js 20.9+, native C/C++ toolchain, optional FFmpeg for media conversion. Windows: Visual Studio Build Tools with C++ desktop workload; `.cargo/config.toml` bumps MSVC stack to 16 MiB (demo import overflows the 1 MiB default).

## Architecture

### Repository layout

```
src/                  # Single Rust binary crate (message-vault-rs CLI)
  main.rs             # clap CLI: serve, import, reset-demo, process-assets
  server.rs           # axum HTTP API + static file serving (tower-http ServeDir)
  auth.rs             # Auth endpoints: register, login (argon2), Hanko sessions
  contacts_api.rs     # Contacts CRUD API
  profile.rs          # Account profile read/update
  import.rs           # message-ir JSONL ingestion, staging tables, handles resolution
  dedupe.rs           # Cross-source soft-dedupe: Pass A exact, Pass B near-time
  process_assets.rs   # FFmpeg-derived browser media under assets_converted/
  export_api.rs       # GET /v1/export/messages (+ /count, /contacts) read-only export
  search_query.rs     # Search grammar parser (must match TS in message-vault-io)
  jsonl.rs            # JSONL reader
  models.rs           # Shared types
  config.rs           # TOML config loading
  db/                 # SQLite layer: schema, accounts, API tokens, contacts, vault_imports, account_profile
crates/demo-seed/     # Demo bundle generator (bin + lib): demo_seed.toml + public-domain corpus
schema/sql/           # SOURCE OF TRUTH for DB schema
fixtures/             # search/parse-cases.json goldens, schema/current-schema.json
config/               # config.toml.example, config.docker.toml
scripts/              # setup-demo.sh, smoke tests, docker entrypoints, sync/regen scripts
demo/                 # Committed demo bundle (VCF + 390 JSONL files, ~627k messages)
static/               # Vite SPA build output (served at /) — built from message-vault-io/web/
```

### Handles table (typed identifier model)

All handles (phone numbers, emails) are stored in a central `handles` table with a typed ID, replacing raw text handles across the schema:

- **`handles`**: `(id, account_id, raw, normalized, normalized_note, handle_type, service)` — unique on `(account_id, normalized, handle_type)`
- `normalized_note` flags ambiguous E.164 normalizations that need human review (e.g., fabricated numbers)
- `handle_type` distinguishes phone, email, and other handle kinds
- Every table that used raw `TEXT` handles now uses `handle_id INTEGER REFERENCES handles(id)`:
  - `conversations.chat_handle_id`, `participants.handle_id`, `messages.sender_handle_id`, `tapbacks.sender_handle_id`
  - `contact_handles.handle_id`, `trashed_handles.handle_id`
  - `staging_*` tables mirror the same handle_id pattern
- `participants` gained a `contact_id` FK for direct contact resolution
- Import resolves handles to IDs at staging time (upsert into `handles`, then reference by ID)
- The workspace pulls `message-ir`, `contacts`, and `phone` crates via path deps from `../message-vault-io/crates/message/` (local development) — these need to be present at build time

### HTTP API (axum)

`serve` starts the API on `127.0.0.1:8080` and serves the Vite SPA at `/` via `tower-http` `ServeDir`. Endpoints:

- `GET /health`
- `GET /v1/auth/mode` — auth mode discovery (local vs hanko)
- `POST /v1/auth/register` / `POST /v1/auth/login` — local auth (argon2)
- `POST /v1/auth/hanko/session` — Hanko passwordless session exchange
- `GET /v1/auth/check` — validates bearer token
- `GET/POST /v1/account/profile` — account profile read/update
- `POST /v1/account/change-password` / `POST /v1/account/delete`
- `POST /v1/imports` / `POST /v1/imports/{id}/complete` — import session management
- `GET /v1/imports` — import history listing
- `POST /v1/import` — bulk message-ir JSONL import
- `GET /v1/export/messages` — offset-paginated message export (`?offset=N&limit=N`)
- `GET /v1/export/messages/count` — message count
- `GET /v1/export/contacts` — contacts list
- `PUT/GET/HEAD /v1/assets/{sha256}` — asset storage
- Resumable upload endpoints for large attachments

Auth uses per-account Import API tokens (SHA-256 hashed, generated from Settings → Access). There is also a `VAULT_AUTH=hanko` mode for Hanko passwordless (used in the Bitrealm production VPS, configured in the private `message-vault-ops` repo). CORS headers are enabled for GUI connections.

`serve` keeps a warm SQLite mutex only for short import-session rows. Bulk `POST /v1/import` and export endpoints open their own connections so they don't block on the Rust mutex. Same-account imports use a per-account lock so staging is not wiped mid-run.

### Data flow

```
phone backup → message-exporters (sibling repo) → message-ir JSONL + attachments
    → POST /v1/import or CLI `import` → SQLite staging tables
    → handles resolution (upsert raw→normalized→handle_id)
    → dedupe (exact + near-time) → messages / conversations / participants / attachments
    → process-assets (ffmpeg) → API serves data to message-vault-io web UI
```

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
- **local** (default): username + password, argon2 hashed. Auth is handled server-side in Rust (`src/auth.rs`); the web UI calls `/v1/auth/login` and `/v1/auth/register`.
- **hanko**: Hanko passwordless (production VPS). Requires `HANKO_API_URL` env var.

New accounts start with browsing edits enabled. View-only mode can be set under Settings → Access without blocking imports.

### Two cross-language contracts

**Schema sync**: `schema/sql/` is the source of truth for table definitions. `node scripts/sync-vault-schema.mjs` regenerates `fixtures/schema/current-schema.json`. Run with `--check` to fail if outputs are stale. Commit both the SQL edits and the generated files together.

**Search grammar**: TypeScript `message-vault-io/web/src/lib/searchQuery.ts` is the behavior reference. Rust `src/search_query.rs` must produce identical parse results. Shared cases live in `fixtures/search/parse-cases.json`. After changing the TS parser in message-vault-io, run `node scripts/regen-search-goldens.mjs` and then `cargo test golden_parse_cases_match_typescript`. Commit updated goldens with the parser changes.

### Dedupe

When importing overlapping backups from different sources (e.g., iPhone and Android), a two-pass soft dedupe runs:
- **Pass A**: exact match on sender + recipient + timestamp + text hash
- **Pass B**: near-time match on sender + recipient + close timestamps + text hash

Messages matched in either pass are skipped. Duplicate detection is cross-source; messages already stored for other sources are not re-inserted.

### Multi-tenant data model

All tables keyed by `account_id`. Core tables: `handles` (typed identifier registry), `conversations`, `participants`, `messages`, `attachments`, `tapbacks`, `contacts`/`contact_handles`/`contact_labels`, `accounts`/`account_emails`/`account_phones`/`account_api_tokens`/`account_prefs`, `vault_imports` (import session tracking). FTS5 virtual table for full-text search. `trashed_handles` and `trashed_conversations` for soft-delete.

### Rust crate structure

This is a workspace with two members: the root binary crate (`message-vault-rs`) and `crates/demo-seed`. There is no `lib.rs` split — all logic lives under `src/` in the binary crate. The workspace pulls `message-ir`, `contacts`, and `phone` via path deps from `../message-vault-io/crates/message/` during local development; these are the shared message schema types.

`crates/demo-seed` generates the demo bundle from `demo_seed.toml` + public-domain text corpus (Pride & Prejudice sentences, name lists). Run `cargo run --release -- reset-demo` to regenerate the demo database.

### Docker/Compose

- `compose-dev.yml` (default, via `COMPOSE_FILE` in `.env`): bind-mount toolchain image, ports 8080, optional sqlite-web on 127.0.0.1:8081
- `compose-release.yml`: slim multi-stage image from `Dockerfile.release` — builds Rust binary and copies in the Vite SPA `static/` directory
- `docker compose up` builds and runs the full stack
- The Vite SPA is built separately from `message-vault-io/web/` and the output placed in `static/`

## Test conventions

- **Rust**: inline `#[cfg(test)] mod tests` within each source file. DB tests use `tempfile` and in-memory/on-disk SQLite with `PRAGMA foreign_keys = ON`. The `golden_parse_cases_match_typescript` test in `src/search_query.rs` runs shared fixtures.
- **Shell smoke tests**: `scripts/smoke-import-api.sh`, `smoke-vault-push.sh`, `smoke-export-api.sh` — build release binary, start `serve` on temp port with temp config/DB, curl the API, assert with grep, cleanup via trap.

There is no CI workflow that runs `cargo test`. All tests are run locally.

## CI/CD

Two GitHub Actions workflows:
- **docker.yml**: on tag `v*` push or manual dispatch — builds `Dockerfile.release` and pushes to Docker Hub `mbeisser1/message-vault` (semver/latest/sha tags)
- **docs.yml**: on push to `main` touching `docs/**` — checks and deploys the Starlight site to GitHub Pages

## Key design decisions

- **Import is append-only**. Messages are never updated in place. Re-importing the same data uses the dedupe passes to skip already-stored messages.
- **Handles are typed and normalized**. The `handles` table centralizes all identifiers (phone, email, etc.) with E.164 normalization for phones. `normalized_note` flags ambiguous cases for human review in the UI.
- **Frontend is served as static files**. The Vite SPA from `message-vault-io/web/` is built and placed in `static/`. Axum serves it at `/` via `tower-http::ServeDir`. There is no Next.js app or SSR.
- **`export.ini` in the sibling project stores vault URL and key**. The vault itself stores accounts/passwords/tokens in SQLite, not in TOML config.
- **No host-level secrets in config.toml**. API tokens are per-account, hashed, and stored in SQLite.
- **Config is for paths and server bind only**; identity and auth live in the database.

## Common development workflows

### Demo setup quick start
```bash
./scripts/setup-demo.sh
cargo run --release -- serve &
# Open http://localhost:8080 — serve serves the static SPA at /
```

### Personal data import
```bash
cp config/config.toml.example config/config.toml
# Edit config.toml: adjust [paths], ensure [server] is uncommented
cargo run --release -- serve &
# Create account via the web UI at http://localhost:8080
# Generate Import API token under Settings → Access
# Push JSONL from message-vault-io Vault tab or vault-push CLI
```

### Full local checks before committing
```bash
cargo build --workspace && cargo test --workspace
node scripts/sync-vault-schema.mjs --check
node scripts/regen-search-goldens.mjs && git diff --exit-code fixtures/search/parse-cases.json
```

## Licensing

AGPL-3.0. The sibling `message-exporters` crates (`message-ir`, `contacts`, `phone`) are MIT/Apache-2.0.
