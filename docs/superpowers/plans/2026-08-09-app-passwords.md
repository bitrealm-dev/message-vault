# Named App Passwords Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split rotating GUI session tokens from long-lived named app passwords scoped to import/export so CLI tools survive GUI login.

**Architecture:** Keep `account_api_tokens` as the one-per-account session credential (rotate on login). Add `account_app_passwords` for many named hashed secrets. `resolve_auth` returns a capability (`Full` vs `ImportExport`); non-allow-listed handlers reject `ImportExport`. Settings Account manages app passwords; stop showing the session token as a CLI key.

**Tech Stack:** Rust (`message-vault-server`), SQLite, Axum, React/TypeScript (`web/`).

## Global Constraints

- Scope: `crates/vault/server/`, `schema/sql/`, `fixtures/schema/`, `web/src/screens/settings/`, docs under `docs/src/content/docs/` that mention Import API token location. No `web-next/` in this pass.
- App password prefix: `mv-app-` + 32 alphanumeric characters.
- Session token prefix stays `mv-user-`.
- Communication style: plain sentences; no review-note shorthand in user-facing copy.
- Do not commit secrets (`config/config.toml`, etc.).

## File map

| File | Role |
|------|------|
| `schema/sql/accounts.sql` | Add `account_app_passwords` + index |
| `fixtures/schema/current-schema.json` | Register new table/index |
| `crates/vault/server/src/db/app_passwords.rs` | Create / list / revoke / lookup by hash |
| `crates/vault/server/src/db/mod.rs` | Export module |
| `crates/vault/server/src/db/api_tokens.rs` | Leave session rotate behavior; clarify comments |
| `crates/vault/server/src/server.rs` | `AuthIdentity` capability; resolve both; `require_full_access` on handlers |
| `crates/vault/server/src/auth.rs` or new `app_passwords.rs` handlers | CRUD HTTP API |
| `web/src/screens/settings/AccountSettingsPanel.tsx` | App passwords UI |
| `web/src/components/AppPasswordRevealDialog.tsx` | One-time reveal dialog (optional extract) |
| Docs that say Settings → Profile for tokens | Point at Account → App passwords |

---

### Task 1: Schema + app password DB helpers

**Files:**
- Modify: `schema/sql/accounts.sql`
- Modify: `fixtures/schema/current-schema.json`
- Create: `crates/vault/server/src/db/app_passwords.rs`
- Modify: `crates/vault/server/src/db/mod.rs`
- Test: unit tests in `app_passwords.rs`

**Interfaces:**
- Produces: `create_app_password(conn, account_id, label) -> Result<(id, token)>`, `list_app_passwords`, `delete_app_password`, `lookup_account_for_app_password(conn, token) -> Result<Option<String>>`, `generate_app_password() -> String`

- [ ] Add `account_app_passwords` table and index to `accounts.sql`
- [ ] Update `current-schema.json` tables/indexes lists
- [ ] Implement `app_passwords.rs` with hash lookup and CRUD; unit tests for create/list/revoke/lookup
- [ ] `cargo test -p message-vault-server app_passwords -- --nocapture`

---

### Task 2: Auth capability + route guards

**Files:**
- Modify: `crates/vault/server/src/server.rs`
- Test: add auth tests (in-memory or handler-level) for session vs app password allow/deny

**Interfaces:**
- Consumes: `api_tokens::lookup_account_for_token`, `app_passwords::lookup_account_for_app_password`
- Produces: `AuthCapability { Full, ImportExport }`, `AuthIdentity { account_id, capability }`, `require_full_access(auth) -> Result<(), ApiError>`

- [ ] Extend `AuthIdentity` with capability
- [ ] `resolve_auth`: session hit → `Full`; else app password hit → `ImportExport`; else 401
- [ ] Call `require_full_access` on profile, storage, change-password, delete-account, delete-messages, export contacts/conversations (and any other non-allow-listed authenticated handlers)
- [ ] Leave import/export/assets/auth-check without that guard
- [ ] Tests: app password passes auth/check and is forbidden on profile; session still reaches profile
- [ ] `cargo test -p message-vault-server`

---

### Task 3: App password HTTP API

**Files:**
- Create or modify: `crates/vault/server/src/app_passwords_api.rs` (or handlers in `auth.rs` / `profile.rs`)
- Modify: `crates/vault/server/src/server.rs` routes + `main`/`lib` module wiring
- Modify: `crates/vault/server/src/lib.rs` or `main.rs` as needed

**Interfaces:**
- `GET/POST /v1/account/app-passwords`, `DELETE /v1/account/app-passwords/{id}` — all require `Full`

- [ ] Implement list / create / delete handlers
- [ ] Register routes
- [ ] Smoke test via unit/integration style already used in server crate
- [ ] `cargo test -p message-vault-server`

---

### Task 4: Settings Account UI

**Files:**
- Modify: `web/src/screens/settings/AccountSettingsPanel.tsx`
- Create: `web/src/screens/settings/AppPasswordsSection.tsx` (preferred split)
- Create: `web/src/components/AppPasswordRevealDialog.tsx` if dialog is non-trivial

- [ ] Remove session token “Message import” display
- [ ] Add App passwords section: list, create with label, reveal once, revoke
- [ ] Copy explains CLI vs rotating GUI session
- [ ] `cd web && npm run build`

---

### Task 5: Docs pointers

**Files:**
- Modify docs under `docs/src/content/docs/` that say Settings → Profile for Import API token

- [ ] Update to Settings → Account → App passwords
- [ ] Note that login session tokens are separate and rotate

---

### Task 6: Verification

- [ ] `cargo test -p message-vault-server`
- [ ] `cd web && npm run build`
- [ ] Manual checklist: login → create app password → use as Bearer on `/v1/auth/check` → confirm `/v1/account/profile` returns 403 → re-login GUI still works
