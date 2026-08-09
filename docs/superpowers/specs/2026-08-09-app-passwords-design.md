# Named app passwords (CLI import/export)

**Date:** 2026-08-09  
**Status:** approved (execute now)  
**Scope:** `message-vault-server` auth + Settings Account tab in `web/`. No `web-next/` changes in this pass.

## Problem

Login returns a Bearer token that is also the only Import API credential. That token is stored as a SHA-256 hash and **rotated on every login**. A user who copies the token into `vault-push` / `vault-pull` loses CLI access after the next GUI sign-in.

Settings currently shows that session token under “Message import”, which encourages treating a rotating GUI credential as a stable CLI secret.

## Goals

1. Keep GUI login behavior: each successful login (and Hanko session exchange) continues to issue a fresh **session** Bearer token with full API access.
2. Add **named app passwords** that the user creates in Settings. These stay valid until revoked. They authenticate only the routes that `vault-push` and `vault-pull` need (import, export messages/assets, auth check).
3. Stop presenting the session token as the CLI Import API token.

## Non-goals

- Contacts/browse grants on app passwords (import still applies contact names server-side).
- Multiple simultaneous GUI sessions with non-rotating tokens.
- Changes to `web-next/`.
- Changing `vault-push` / `vault-pull` CLI flag names (`--key` / `VAULT_KEY` still accept the Bearer secret).

## Design

### Two credential kinds

| Kind | Storage | Lifetime | Access |
|------|---------|----------|--------|
| Session token | Existing `account_api_tokens` (one row per account) | Rotates on login / Hanko exchange | Full API (browse, settings, import, export, danger zone) |
| App password | New `account_app_passwords` (many rows per account) | Until user revokes | Import/export only |

Plaintext is shown once at create time (prefix `mv-app-…`). Only the hash is stored.

### Auth resolution

`resolve_auth` looks up the Bearer value against session tokens first, then app passwords. `AuthIdentity` gains a capability:

- `Full` — session
- `ImportExport` — app password

Handlers that are not in the import/export allow-list call `require_full_access(&auth)?` and return 403 for app passwords.

**Import/export allow-list:**

- `GET /v1/auth/check`
- `POST /v1/import`
- `GET` / `POST /v1/imports`, `POST /v1/imports/{id}/complete`
- Asset `GET` / `PUT` / `HEAD` and multipart upload routes under `/v1/assets/…`
- `GET /v1/export/messages`, `GET /v1/export/messages/count`

**Denied for app passwords (examples):** profile, storage, change-password, delete-account, delete-messages, `/v1/export/contacts`, `/v1/export/conversations`.

### Schema

```sql
CREATE TABLE IF NOT EXISTS account_app_passwords (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_account_app_passwords_account
    ON account_app_passwords(account_id);
```

Update `fixtures/schema/current-schema.json` accordingly.

### API (session-only management)

- `GET /v1/account/app-passwords` → `{ items: [{ id, label, created_at }] }` (never hashes or plaintext)
- `POST /v1/account/app-passwords` body `{ label }` → `{ id, label, created_at, token }` (token once)
- `DELETE /v1/account/app-passwords/{id}` → `{ ok: true }`

### UI (Settings → Account)

Replace the “Message import” session-token display with an **App passwords** section:

- List label + created date + Revoke
- Create: label field + button; dialog shows plaintext once with copy
- Short note that GUI sign-in uses a separate session token that changes on login; CLI tools should use an app password

### Migration / compatibility

Existing `account_api_tokens` rows remain session tokens. Anyone who stored a pre-change login token for CLI must create an app password. Docs that say “Settings → Profile Import API token” should say “Settings → Account → App passwords”.

## Success criteria

1. Login still rotates the session token; GUI continues to work with the new session Bearer.
2. An app password works with `vault-push` / `vault-pull` after a later GUI login.
3. An app password cannot call profile or delete-account (403).
4. Settings Account tab manages named app passwords; does not display the session token as a CLI secret.
5. `cargo test -p message-vault-server` and `cd web && npm run build` succeed.
