# Demo data as a Compose option

## Goal

There is one Docker vault runtime. Seeding the committed demo dataset is an optional first-boot step controlled by `DEMO_DATA`, not a separate `VAULT_MODE`.

Operators should not think in terms of “demo mode vs personal mode.” They run the same image and either seed sample data on an empty volume or leave the volume empty for account creation and import.

## Current behavior (replaced)

- Env `VAULT_MODE=demo|personal` (default `demo`).
- Entrypoints branch on that value when `data/vault.db` is missing: `demo` runs `reset-demo` + `process-assets`; `personal` leaves data empty.
- Compose and docs present two “modes.”

## Design

### Single vault process

Compose always starts the same service (`message-vault-server serve`). Auth and bind config stay as today (`VAULT_AUTH`, `AUTH_MODE`, etc.). No new server binary flags for this change.

### `DEMO_DATA` env

| Value | Behavior when `data/vault.db` is missing |
|-------|------------------------------------------|
| `true` (default) | Run `reset-demo`, then `process-assets` (warn-and-continue on asset failure, same as today) |
| `false` | Ensure config under `data/`, leave DB empty; create account in the UI |

When `data/vault.db` already exists, skip seeding regardless of `DEMO_DATA` (unchanged).

Truthy parsing: treat `true` / `1` / `yes` (case-insensitive) as seed; everything else including empty/`false`/`0` as no seed. Compose default string is `true`.

### Compose

Both `compose-dev.yml` and `compose-release.yml`:

```yaml
environment:
  DEMO_DATA: ${DEMO_DATA:-true}
```

Remove `VAULT_MODE`. Comments show blank vault as `DEMO_DATA=false docker compose …`.

### Entrypoints

`scripts/docker-entrypoint-dev.sh` and `scripts/docker-entrypoint-release.sh`:

- Read `DEMO_DATA="${DEMO_DATA:-true}"`.
- Drop the `VAULT_MODE` case statement.
- If DB missing and demo requested → seed path; else → empty personal path (log clearly).
- Reject nothing for unknown values: non-truthy means no seed (simpler than requiring exactly `true`/`false`).

### Docs and maintainer notes

Update user-facing snippets to drop `VAULT_MODE`:

- `README.md`
- `CLAUDE.md` (compose quick start)
- `docs/src/content/docs/introduction/quick-start.md`
- `docs/src/content/docs/set-up-the-server/docker-install.md`
- `docs/src/content/docs/set-up-the-server/try-the-demo.md`
- `docs/src/content/docs/set-up-the-server/first-personal-vault.md`
- `docs/src/content/docs/set-up-the-server/updating.md`

Wording: demo dataset is optional seed data, not a vault mode. Personal vault docs use `DEMO_DATA=false`.

Do not rewrite historical plan archives under `docs/superpowers/plans/` unless they are still used as runbooks.

### Out of scope

- Changing `reset-demo` CLI or demo account semantics.
- Persisting `demo_data` in `config.toml`.
- Backward-compat alias for `VAULT_MODE` (callers must switch to `DEMO_DATA`).
- Re-seeding an existing volume when `DEMO_DATA=true`.

## Acceptance

- `docker compose -f compose-release.yml up` with empty volume seeds demo (default).
- `DEMO_DATA=false docker compose -f compose-release.yml up` with empty volume starts without demo rows.
- Existing volume never re-seeds solely because `DEMO_DATA` is true.
- Grep of compose + entrypoints + live docs shows no `VAULT_MODE`.
