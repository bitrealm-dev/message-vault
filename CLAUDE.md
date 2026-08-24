# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Message Vault pulls conversations out of chat apps (iMessage, WhatsApp, SMS backups) and stores them in a self-hosted, searchable vault. Three pieces:

- **Vault server** (`crates/vault/server/`) — Axum HTTP API (`/v1/*`) over SQLite at `data/vault.db` by default; set the `[database] url` config (or `serve --db-url`) to run on Postgres instead. Login is a local vault account (Argon2 + JWT session tokens); named API tokens with import/export scopes also exist.
- **Desktop app** (`src-tauri/` + `web/`) — Tauri v2 shell around a Vite + React 19 + TypeScript SPA. It reads phone backups, writes JSONL, and imports into a running vault. Browse/search work in the browser too; importing needs the desktop app.
- **Website** — the same `web/` SPA served from the vault's `static/`.

**AGENTS.md is the canonical operations guide** (first-time setup, dev run instructions, release process, PR workflow) and is read by Claude Code automatically. This file covers the architecture and the rules that are easy to get wrong; see AGENTS.md for anything operational not covered here. Published docs live at bitrealm.io (Astro Starlight in `docs/`).

## Data flow (the big picture)

```
vendor backup (chat.db, SMS XML, WhatsApp crypt15, …)
  → exporter crate (crates/exporters/*) parses it into message-ir types
  → ConversationDocument (schema_version 3) written as JSONL
  → vault-push CLI / Tauri push command → POST /v1/... → SQLite
  → web/ SPA reads threads back through the /v1/ API
```

- **`crates/libs/ir`** (`message-ir`) is the shared conversation model every exporter writes: `ConversationDocument` holds export metadata, participants, and messages. `schema_version` is `3` and independent of the product version.
- **`crates/libs/ir-format`** reads/writes on-disk formats (JSON, CSV, EML, SBR XML) to/from IR; **`crates/libs/reexport`** converts between existing export formats.
- **`crates/core/message-vault-io-core`** — shared export pipeline, jobs, form model, `export.ini`. Avoids `anyhow` so the desktop app stays lightweight; callers map `String` errors at the edge.
- **`crates/vault/server`** — each `*_api.rs` file is one Axum route group; `db/` modules mirror the table sources in `schema/sql/*.sql`, which the server embeds at compile time (`db/schema.rs`) — change tables there, not in a live db file. Import path: `jsonl.rs` → `import.rs` → `dedupe.rs`; demo mode runs through a guest pool (`guest_pool.rs`).
- **`src-tauri/`** is **not a workspace member** (own `Cargo.toml`, listed in the root workspace `exclude`). Its `commands/` wrap the exporter crates and push/pull for the desktop app. Format/build it with `--manifest-path`.
- **`web/src/lib/api.ts`** is the vault API client; `web/src/lib/tauri.ts` wraps desktop-only commands; `desktopFeatures.ts` gates them. Tests sit next to sources as `*.test.ts(x)` (Vitest + Testing Library).
- **Not the product path**: `crates/message-vault-io-gui/` (legacy Slint GUI) and `web-next/` (legacy Next.js browse UI). New features go in `web/` + `src-tauri/` + `crates/vault/server/`.
- Design specs and implementation plans live in `docs/superpowers/specs/` and `docs/superpowers/plans/` — check them before starting work that overlaps.

## Commands

Run from the repo root unless noted. Full setup instructions: AGENTS.md.

### Dev loop

```bash
./scripts/run-vault-dev.sh                # vault API on http://127.0.0.1:8080 (keeps data/)
./scripts/run-vault-dev.sh --reset-demo   # wipe data/, seed sample inbox (sign in: user `demo`, empty password)
cd web && npm run dev                     # browser UI on :5173, proxies /v1 — OR:
cargo tauri dev                           # desktop app (starts Vite itself; never run both at once)
```

Use **127.0.0.1**, not `localhost` (the latter can resolve to IPv6, which the vault does not listen on). Restart the vault script after edits under `crates/vault/server/` (debug `cargo run`; no hot reload).

### Verify

```bash
cargo fmt --all -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo build --workspace && cargo test --workspace
cargo test -p sms-backup-restore-exporter     # one crate
cargo build --manifest-path src-tauri/Cargo.toml
cd web && npm run lint && npm test            # Biome + Vitest (CI runs `biome ci`)
cd docs && npm run check && npm run build     # docs tree only
./scripts/format-all.sh                       # rewrite: rustfmt (workspace + src-tauri) + Biome
./scripts/lint-all.sh                         # Clippy (workspace except Slint GUI + src-tauri) + Biome
./scripts/check-pr.sh                         # all of the above in one pass; stops on first failure
```

## Rules that are easy to get wrong

- **Version lockstep** (current `0.7.3`): `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `web/package.json`, `crates/vault/server/Cargo.toml` all carry the product version. Leave other crates at `0.1.0`; never bump `message-vault-io-gui` (`0.6.0`) or `web-next` (`0.3.0`).
- **Pushing a `v*` tag ships a release** — CI builds the Docker image and desktop installers and creates a GitHub Release. Never create or push tags unless asked.
- **CI gates**: rustfmt, workspace build + test, Biome `ci` (lint and format drift), Vitest. Clippy is not gated — run `./scripts/lint-all.sh` locally.
- **Git workflow**: never commit to `main`; use a branch or worktree. Verify PR state with `gh pr view` / `gh pr list` / `gh pr checks` before pushing — don't assume. Don't merge PRs unless explicitly asked.
- **Biome**: prefer a real fix over `biome-ignore`; prefix unused bindings with `_`.
- **Tests** use committed fixtures in `tests/fixtures/`; never commit personal backups or real message data.

## Style

Write instructions and commit messages in plain, direct English (see AGENTS.md for the full communication rules).
