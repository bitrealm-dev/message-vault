# Postgres Vault Dev Script Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `./scripts/run-vault-pg-dev.sh` so a checkout can start, reset, and demo-seed the vault against compose Postgres the same way the SQLite script does.

**Architecture:** The script starts and stops `docker-compose.pg.yml` (named volume `vault_pg_data`), wipes that volume plus host `data/` on `--reset` / `--reset-demo`, and always passes `--db-url postgres://vault:vault@127.0.0.1:5432/vault`. `--reset-demo` calls `reset-demo --db-url`, which seeds the `demo` account and sample inbox into that database instead of replacing `data/vault.db`.

**Tech Stack:** Bash, Docker Compose, `message-vault-server` (sqlx Any, Postgres).

**Spec:** `docs/superpowers/specs/2026-08-27-run-vault-pg-dev-design.md`

## Global Constraints

- Flag names match the SQLite script: `--reset`, `--reset-demo`. Cannot combine them. Reject `--demo` with the same rename message. No `--sqlweb`.
- Do not write `[database] url` into `config/config.toml`. Pass `--db-url postgres://vault:vault@127.0.0.1:5432/vault` on `serve` and `reset-demo`.
- `--reset` / `--reset-demo` run `docker compose -f docker-compose.pg.yml down -v`, delete host `data/`, then `up -d`, then wait for `pg_isready`.
- Named volume is `vault_pg_data`.
- Trap on `EXIT` / `INT` / `TERM` runs `docker compose … down` without `-v`.
- `reset-demo` without `--db-url` still refuses when config has `[database] url`.
- `reset-demo --db-url` does not replace `data/vault.db`. It seeds Postgres and writes attachments under `data/<account>/`.
- Do not change `./scripts/run-vault-dev.sh` behavior.
- Do not bump product versions.

## File map

| File | Responsibility |
|---|---|
| `docker-compose.pg.yml` | Named volume `vault_pg_data` |
| `crates/vault/server/src/process_assets.rs` | Optional `db_url` on `ProcessAssetsOptions` |
| `crates/vault/server/src/reset_demo.rs` | URL seed/import/assets path |
| `crates/vault/server/src/cli.rs` | `reset-demo --db-url` |
| `scripts/run-vault-pg-dev.sh` | Host orchestrator |
| `docs/src/content/docs/vault/developer/contributing.md` | How to start the Postgres script |
| `AGENTS.md` | Point day-to-day Postgres at the new script |

## Tasks

1. Name the compose volume `vault_pg_data`.
2. Add `reset-demo --db-url` plus `ProcessAssetsOptions.db_url`. Keep no-flag URL refusal.
3. Add `scripts/run-vault-pg-dev.sh` with the same reset flags as the SQLite script.
4. Document the script in Contributing and AGENTS.md.

## Tests

- `refuse_url_config_without_flag` errors without `--db-url` and succeeds with it.
- `reset_demo_db_url_creates_demo_account_on_postgres` (needs `MV_TEST_POSTGRES_URL`): username `demo`, no password hash, at least one conversation.
- Script: `--reset` plus `--reset-demo` exits 1; `--demo` and `--sqlweb` exit 1; `--help` lists the reset flags and does not mention sqlite-web.
