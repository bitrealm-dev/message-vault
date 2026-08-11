# Single env var for vault auth mode

## Goal

Auth mode is controlled by one environment variable: `VAULT_AUTH`. Operators set it once. The Rust vault server and the web-next frontend both honor that same name. There is no second variable and no backward compatibility for `AUTH_MODE`.

## Problem

Today two env vars mean the same thing:

| Variable | Reader |
|----------|--------|
| `AUTH_MODE` | Rust vault server (`AuthMode::from_env()`), exposed via `GET /v1/auth/mode` |
| `VAULT_AUTH` | web-next (`authMode.ts`), Compose, `.env`, public docs |

`compose-release.yml` sets both independently (`${VAULT_AUTH:-local}` and `${AUTH_MODE:-local}`), so they can disagree. `compose-dev.yml` only sets `VAULT_AUTH`, so a release-shaped stack and a laptop stack already diverge in what the server sees.

## Design

### Canonical variable

| Value | Meaning |
|-------|---------|
| `local` (default when unset or any non-`hanko` value) | Username/password accounts on the vault |
| `hanko` | Hanko passwordless auth; requires `HANKO_API_URL` |

Parsing matches current server behavior: case-insensitive; only the string `hanko` selects Hanko; everything else is local.

### Server

In `crates/vault/server/src/config.rs`, `AuthMode::from_env()` reads `VAULT_AUTH` instead of `AUTH_MODE`.

No fallback to `AUTH_MODE`. If an old deployment still sets only `AUTH_MODE`, the server treats auth as `local`.

`GET /v1/auth/mode` keeps returning `{ "mode": "local"|"hanko", "hanko_api_url": ... }`. The response field name stays `mode`; only the env source changes.

### Compose

`compose-release.yml`: drop the `AUTH_MODE` line. Keep:

```yaml
VAULT_AUTH: ${VAULT_AUTH:-local}
HANKO_API_URL: ${HANKO_API_URL:-}
```

`compose-dev.yml` already has only `VAULT_AUTH`; no change required for correctness.

### Frontend

- **web-next**: already reads `VAULT_AUTH`; no rename.
- **Vite SPA (`web/`)**: continues to call `GET /v1/auth/mode`; no env change.

### Docs and examples

Update live references that tell operators to set `AUTH_MODE`:

- Any user-facing or maintainer doc that still mentions `AUTH_MODE` as current config (replace with `VAULT_AUTH`).
- Historical plan files under `docs/superpowers/plans/` that describe the old name may stay as-is; they are frozen plans, not operator docs.

Also update the sentence in `docs/superpowers/specs/2026-08-11-demo-data-compose-option-design.md` that lists both `VAULT_AUTH` and `AUTH_MODE`, so that sibling design no longer documents two vars.

### Out of scope

- Changing Hanko session endpoints or JWT verification.
- Renaming the JSON field on `/v1/auth/mode`.
- Private ops Compose (`message-vault-ops`); that repo must set `VAULT_AUTH` after this lands.
- Deprecation warnings or dual-read periods.

## Acceptance

1. Grep of the repo for `AUTH_MODE` finds no runtime readers (Rust/Compose/env examples that operators copy). Historical plans may still mention the old name.
2. With `VAULT_AUTH=hanko` and `HANKO_API_URL` set, `/v1/auth/mode` returns `"mode":"hanko"`.
3. With `AUTH_MODE=hanko` and `VAULT_AUTH` unset, `/v1/auth/mode` returns `"mode":"local"`.
4. `compose-release.yml` and `compose-dev.yml` both expose only `VAULT_AUTH` for auth mode.
