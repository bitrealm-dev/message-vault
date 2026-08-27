# Postgres vault dev script — 2026-08-27

Add `./scripts/run-vault-pg-dev.sh` so a checkout can run the vault against
the compose Postgres the same way `./scripts/run-vault-dev.sh` runs it
against SQLite. This spec records decisions from the 2026-08-27 design
conversation. It is not an implementation plan.

## Goal

One script starts Postgres (if it is not already up), starts the vault
from this checkout, and stops the Postgres container when the script
exits. `--reset` and `--reset-demo` wipe the database volume and host
`data/`. After `--reset-demo`, sign in as username `demo` with an empty
password, same as the SQLite script.

There is no SQL browser flag.

## Current product

`./scripts/run-vault-dev.sh` starts `message-vault-server` in debug
mode against `data/vault.db`. `--reset` deletes `data/` and starts
empty. `--reset-demo` deletes `data/`, runs
`message-vault-server reset-demo`, then `process-assets`. After
`--reset-demo`, login is `demo` with an empty password.

`docker compose -f docker-compose.pg.yml up -d` starts Postgres 16 on
`127.0.0.1:5432` (user, password, and database `vault`). That file
declares no named volume. Data lives in a Docker-managed volume that
is easy to leave behind by mistake.

`serve --db-url postgres://…` already selects the Postgres engine.
`reset-demo` does not. It replaces the on-disk SQLite file. If the
config has `[database] url`, it refuses so it cannot report a reset
that never touched Postgres.

A manual Postgres demo seed is: generate the bundle, register an
account, import three staging folders. That is too many steps for
day-to-day work.

## Non-goals

- A SQL browser flag (`--sqlweb`). Postgres has no sqlite-web equivalent
  in this script.
- Changing `./scripts/run-vault-dev.sh` behavior, except docs that
  mention both scripts.
- Writing `[database] url` into `config/config.toml`. That file stays
  the host SQLite config so the SQLite script still uses `data/vault.db`.
- Loading the sample inbox into an already-full Postgres without a
  wipe. `--reset-demo` always deletes the volume and `data/` first.
- Teaching `reset-demo` without `--db-url` to read a URL from
  `config.toml`. That no-flag path still refuses `[database] url`.
- Running both vault scripts at once. Both bind `127.0.0.1:8080`.
- Changing product versions or shipping a release.

## Decisions

1. **Same flag names as the SQLite script.** `--reset` and
   `--reset-demo` cannot be combined. `--demo` is rejected with the
   same rename message. No `--sqlweb`.
2. **Keep `config/config.toml` as the SQLite host config.** The Postgres
   script passes
   `--db-url postgres://vault:vault@127.0.0.1:5432/vault` on `serve`
   and on `reset-demo`. If `config/config.toml` is missing, copy it
   from the example and enable the Vite CORS line, same as the SQLite
   script.
3. **Named volume on the compose file.** `docker-compose.pg.yml` mounts
   a volume named `vault_pg_data` on Postgres data. `--reset` /
   `--reset-demo` run `docker compose -f docker-compose.pg.yml down -v`,
   delete host `data/`, then `up -d`, then wait until `pg_isready`
   succeeds.
4. **`--reset-demo` uses `reset-demo --db-url`.** After the empty
   database is healthy, the script runs
   `cargo run -p message-vault-server -- reset-demo --config
   config/config.toml --db-url postgres://vault:vault@127.0.0.1:5432/vault`.
   That command creates username `demo` with no password, imports the
   sample inbox, and converts media (the work `reset-demo` already
   does on SQLite). The script does not re-run `process-assets`; that
   command has no `--db-url` today and the reset already converts.
5. **`reset-demo --db-url` does not replace `data/vault.db`.** It
   opens the URL, ensures schema, writes the demo account row, imports
   the three staging trees, dedupes, and processes assets against
   that pool. Host attachment files go under `data/<account>/` as
   they do today. The no-flag `reset-demo` path still refuses a
   `[database] url` in config.
6. **Stop Postgres on script exit.** A trap on `EXIT`, `INT`, and
   `TERM` runs `docker compose -f docker-compose.pg.yml down` without
   `-v`. The container stops. The volume stays unless this run
   already deleted it for `--reset` or `--reset-demo`.
7. **Start Postgres if it is not up.** A run with no flags does
   `up -d` (safe if already running) and does not wipe.
8. **Need `docker`, Compose, and `cargo`.** `--reset-demo` also needs
   `ffmpeg` and `ffprobe`, same as the SQLite script.

## Architecture

```text
run-vault-pg-dev.sh
  ├─ ensure config/config.toml (CORS for :5173)
  ├─ if --reset or --reset-demo:
  │    compose down -v
  │    rm -rf data/
  ├─ compose up -d; wait pg_isready
  ├─ if --reset-demo:
  │    message-vault-server reset-demo --db-url postgres://…
  ├─ trap: compose down (no -v)
  └─ cargo run … serve --db-url postgres://…
```

`reset-demo --db-url` reuses the existing generate-bundle, import,
dedupe, and process-assets steps. The difference is the pool: open
from the URL instead of a temporary `vault.db` that later replaces
`data/vault.db`.

The compose project stays `docker-compose.pg.yml`. The volume
`vault_pg_data` is the one that `-v` removes. Connection settings stay
`postgres://vault:vault@127.0.0.1:5432/vault`.

## Files

| Path | Change |
|------|--------|
| `scripts/run-vault-pg-dev.sh` | New script |
| `docker-compose.pg.yml` | Named volume for Postgres data |
| `crates/vault/server/src/cli.rs` | `reset-demo` gains `--db-url` |
| `crates/vault/server/src/reset_demo.rs` | URL path: seed + import + assets; keep no-flag URL refusal |
| `docs/src/content/docs/vault/developer/contributing.md` | Short Postgres script block next to the SQLite script |
| `AGENTS.md` | Same start / reset / `--db-url` note (already documents compose + `--db-url`) |

## Testing

- Script: `--reset` and `--reset-demo` together exit non-zero. Unknown
  flags and `--demo` fail the same way as the SQLite script. `--help`
  lists `--reset` and `--reset-demo` and does not mention `--sqlweb`.
- `reset-demo --db-url` against `MV_TEST_POSTGRES_URL`: after a wipe
  of that test database, the `demo` username exists with no password
  hash, and at least one conversation is present. A `reset-demo`
  without `--db-url` and with `[database] url` in config still fails.
- Do not add Playwright. This is a host script plus a CLI path.
- Manual check: `./scripts/run-vault-pg-dev.sh --reset-demo`, sign in
  as `demo` with an empty password at `http://127.0.0.1:5173` (vault
  on `:8080`). Ctrl+C stops the vault and the Postgres container.
  A second run with no flags still has the demo inbox.

## Reproduce (after implementation)

1. From the repo root, with Docker available:
   `./scripts/run-vault-pg-dev.sh --reset-demo`
2. In another terminal: `cd web && npm run dev`
3. Open `http://127.0.0.1:5173`, sign in as `demo` (empty password).
4. Stop the script. `docker compose -f docker-compose.pg.yml ps`
   shows no running container. Start again with no flags; the inbox
   is still there.
5. `./scripts/run-vault-pg-dev.sh --reset` then the UI asks for a
   new account (no `demo` inbox).
