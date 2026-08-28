# Authorization model — design

Date: 2026-08-28
Status: draft, awaiting review
Scope: `crates/vault/server/`, `schema/sql/`, `web/`. Breaking schema change.

## Problem

Message Vault has no concept of a role. Every signed-in session can do everything, and the
mechanisms that look like they limit an account do not.

- `AuthCapability::Full` is an unconditional yes. Each guard in `server.rs` matches
  `AuthCapability::Full => Ok(())` before considering anything else, so only API tokens are ever
  scope-limited.
- `accounts.read_only` is decoration. Its one production caller is `profile.rs:46`, which copies it
  into the `/v1/account/profile` response for the web UI to respect. No guard consults it, so a
  read-only account that talks to the API directly can still write.
- The one restriction that is genuinely enforced belongs to a different concept: `guest_status`,
  policed by `reject_if_guest` and `reject_if_guest_account` at roughly a dozen call sites across
  `assets.rs`, `api_tokens_api.rs`, and `import/`.
- The guest system exists to serve `POST /v1/auth/try-demo`, which the product can no longer reach.
  `web/src/lib/tryDemo.ts` is the only mention of it in `web/`, and nothing imports that file. Behind
  that unreachable entry point sit `guest_pool.rs` and `guest_clone.rs` — 2,490 lines.
- Hanko is a second sign-in mechanism selected by the `VAULT_AUTH` environment variable, carrying a
  JWKS fetch and cache, JWT verification, username derivation, and a schema column.

The result is three overlapping half-mechanisms — capability, `read_only`, `guest_status` — where a
permission decision can be made, and no way to answer the question an operator actually has: who can
use this vault, and what may they do with it.

## Goals

1. One place where authorization is decided, consulted by one set of guards.
2. An administrator who can manage the vault's user accounts.
3. Per-user permissions for import, export, and deleting message data.
4. The ability to disable an account without destroying it.
5. Fewer concepts than we started with.

## Non-goals

- **The login screen redesign is not in this spec.** It remains unspecified and will be designed
  separately. This document covers only the authorization model behind it.
- No cross-tenant data access. Multitenancy is a standing constraint: `messages`, `conversations`,
  and `contacts` stay scoped by `account_id` and no feature added here reads across that boundary.
- No impersonation, acting-as, or session switching.
- No message browsing for administrators, in any form.
- No encryption at rest, and therefore no claim that an administrator cannot read the data by other
  means. See "What this does not protect against".

## Relationship to existing work

The branch `auth-entry-flow` carries seven commits, including five implemented tasks from
`docs/superpowers/plans/2026-08-28-auth-entry-flow.md` (error-envelope parsing, per-service field
examples, connection vocabulary, the fixed 448 × 560 card frame, the vault line). Tasks 6–8 of that
plan — the two screen rewrites and the browser pass — are unimplemented.

This spec does not depend on that work and does not supersede the five landed tasks. The login screen
design that follows this one will decide the fate of tasks 6–8.

## What is removed

### Hanko

Every trace, including the schema column.

| Location | Removal |
|---|---|
| `crates/vault/server/src/auth.rs` | `hanko_session_handler`, JWKS fetch and cache, JWT verification, `username_from_hanko_email_or_id`, `unique_hanko_username`, `MAX_HANKO_JWT_BYTES`, the `hanko:session` rate-limit bucket |
| `crates/vault/server/src/config.rs` | the `AuthMode` enum and `VAULT_AUTH` |
| `crates/vault/server/src/server.rs` | the `/v1/auth/hanko/session` route |
| `crates/vault/server/src/openapi.rs` | its OpenAPI registration |
| `crates/vault/server/src/db/account_profile.rs` | `hanko_user_id` parameters and reads |
| `schema/sql/accounts.sql`, `pg_accounts.sql` | the `hanko_user_id` column and `ix_accounts_hanko_user_id` |
| `web/package.json` | `@teamhanko/hanko-elements` |
| `web/src/screens/LoginScreen.tsx` | the `<hanko-auth>` branch |
| `web/src/lib/authGuards.ts` | the `AuthMode` union |
| `.env.example` | the Hanko block |

With `AuthMode` gone there is one sign-in mechanism, so `GET /v1/auth/mode` has nothing left to
report. The endpoint is removed. The client's mode detection goes with it; what replaces it as the
reachability probe is a question for the login screen design, not this one.

`web-next/` is legacy and out of scope. Its Hanko files stay as they are.

### Try-demo and the guest pool

`POST /v1/auth/try-demo`, `web/src/lib/tryDemo.ts`, `guest_pool.rs`, `guest_clone.rs`, the
`guest_status` column and its reads, and `reject_if_guest` / `reject_if_guest_account` with all their
call sites.

What guests actually were decomposes cleanly, and only one part is not a permission:

| What `guest_status` did | Where it goes |
|---|---|
| Blocked import, export, and API-token creation | `can_import` / `can_export` flags on the account |
| Gave short-lived sessions | already a parameter: `insert_account_session_token_with_ttl` |
| Tracked `'ready'` / `'assigned'` pool state | deleted with the pool it described |

The demo *account* survives. `reset-demo` seeding is an independent path, so
`./scripts/run-vault-dev.sh --reset-demo` continues to work; the demo account becomes an ordinary
account with `can_import`, `can_export`, and `can_delete` set to 0. `DEMO_ACCOUNT_ID`
(`00000000-0000-0000-0000-00000000d001`) and the existing protections against deleting or
password-logging-into the demo account are unchanged.

### read_only

The `read_only` column and `account_is_read_only`. The `read_only` field is removed from the
`/v1/account/profile` response.

The name described a state this product does not have. A restricted user here still edits identities,
still edits contacts, and still imports and exports; the only thing withheld is the destruction of
message data. That is `can_delete`, and there is no read-only preset, because "read only" would be a
false description of it.

## The account model

`accounts` after this change:

| Column | Type | Meaning |
|---|---|---|
| `id` | TEXT PK | unchanged |
| `username` | TEXT | unchanged |
| `password_hash` | TEXT | unchanged |
| `preferred_name` | TEXT | unchanged |
| `is_admin` | INTEGER NOT NULL DEFAULT 0 | may manage users |
| `disabled` | INTEGER NOT NULL DEFAULT 0 | may not sign in |
| `can_import` | INTEGER NOT NULL DEFAULT 1 | may import |
| `can_export` | INTEGER NOT NULL DEFAULT 1 | may export |
| `can_delete` | INTEGER NOT NULL DEFAULT 1 | may delete message data |

Removed: `hanko_user_id`, `read_only`, `guest_status`, and the `ix_accounts_hanko_user_id` index.

The `disabled` column follows a pattern already proven in this schema: `account_api_tokens.disabled`,
commented "Soft-disable without deleting the row."

## Authorization

### Capabilities replace the unconditional yes

`AuthCapability` today is `Full | ApiToken(scopes)`, where `Full` short-circuits every guard. It
becomes:

```rust
enum AuthCapability {
    Session { is_admin: bool, permissions: Permissions },   // was Full
    ApiToken(Permissions),                                  // account ∩ token, resolved already
}
```

`Permissions` is the shared set defined below. For a session it is the account's own; for an API
token it is the account's intersected with the token's, computed in `resolve_auth` so no guard has to
remember to do it.

`resolve_auth` (`server.rs:587`) already performs a database lookup on every request to resolve the
bearer token. It loads the account's flags in that same lookup, so the capabilities cost no
additional round trip.

### Permissions intersect

An API token belongs to an account. Its effective permission is the intersection of the token's
scopes and its owner's flags, so revoking a user's import right cannot be routed around by minting a
token. Concretely: `require_import_access` passes only when the owning account has `can_import` and,
for `ApiToken`, when the token's scopes include import.

### One permission type, two places it is stored

`ApiTokenScopes` today is an enum of three values — `Import`, `Export`, `Both` — stored as a string
in `account_api_tokens.scopes`. That shape cannot express three independent permissions: adding
delete would need seven variants to cover the combinations.

It is replaced by a set, shared by accounts and tokens:

```rust
struct Permissions {
    import: bool,
    export: bool,
    delete: bool,
}
```

Stored the same way in both tables — `can_import`, `can_export`, `can_delete` columns on `accounts`
and on `account_api_tokens` — so the intersection is a field-wise AND rather than a translation
between two vocabularies. `account_api_tokens.scopes` is dropped.

`is_admin` and `disabled` stay account-only. Neither is a permission a user could sensibly delegate
to an external tool.

### API tokens may delete

Deletion is currently session-only: `delete-messages` calls `require_full_access`, which rejects API
tokens outright. It moves to `require_delete_access`, so a token holding delete can call it.

These stay on `require_full_access` and remain session-only, because they are not operations to hand
to an external tool:

- `POST /v1/auth/delete-account` — closing the account, which belongs behind the danger zone
- `POST /v1/auth/change-password`
- the `/v1/account/api-tokens` management endpoints — a token must not mint tokens
- every `/v1/admin/*` route

**A safety consequence that must not be missed.** The web UI has no scope selector: `useApiTokens.ts`
hardcodes `scopes: "both"` on creation, so every token made from Settings today receives every
permission that exists. If delete joins the vocabulary while that stays hardcoded, every token
silently gains the right to destroy message data. The create-token form must gain a real selector as
part of this work — it is required, not cosmetic. New tokens default to import and export only;
delete is opt-in.

### The guards

Five, four of which exist:

| Guard | Location | Change |
|---|---|---|
| `require_full_access` | `server.rs:60` | unchanged: rejects API tokens, session-only operations |
| `require_import_access` | `server.rs` | checks `permissions.import` for either credential |
| `require_export_access` | `server.rs` | checks `permissions.export` |
| `require_import_or_export_access` | `server.rs` | checks either |
| `require_delete_access` | new | checks `permissions.delete`; accepts API tokens |
| `require_admin` | new | checks `is_admin`; rejects API tokens outright |

Because `resolve_auth` has already intersected account and token permissions, each guard reads one
boolean and does not need to know which kind of credential it holds. This is why delete rides the
existing wiring instead of arriving by a separate path: the guards keep their shape and gain one
sibling.

`reject_if_guest` and `reject_if_guest_account` are deleted, and their call sites in `assets.rs`,
`api_tokens_api.rs`, and `import/mod.rs` fall back to the capability guards already present at those
sites.

The `disabled` check is deliberately not one of these guards. It belongs in `resolve_auth`, so that
it applies to every request rather than only to those a particular guard protects — see "Disabled
accounts" below.

### What `can_delete` gates

The flag protects message data. It does not protect the metadata describing who the messages are
with, and it does not restrain a user from leaving.

**Blocked when `can_delete = 0`:**

- moving a conversation or its messages to trash (`trashed_conversations`)
- purging trashed conversations
- `POST /v1/account/delete-messages`
- deleting attachments

**Still allowed:**

- trashing and untrashing contacts and handles (`trashed_contacts`, `trashed_handles`)
- editing identities, contacts, contact groups, and thread tags
- import and export
- `POST /v1/auth/delete-account`

The last one is deliberate. Deleting your own account cascades every message you own, which does
destroy exactly the data `can_delete` protects — but it sits behind the danger zone
(`web/src/screens/settings/ProfileDangerZone.tsx`) and a current-password confirmation, and a user
who wants to leave is entitled to. `can_delete` governs deletion of data inside the vault, not the
right to close the account.

**Implementation note for the plan:** trash mutations do not currently live in a dedicated route
group. Trashed rows are written from handlers under the `/v1/export/*` namespace
(`export_api.rs:1797` inserts into `trashed_conversations`). The implementation plan must enumerate
the exact handler call sites rather than assume a `/v1/trash` route group exists.

### Disabled accounts

`disabled = 1` blocks sign-in and, crucially, is checked in `resolve_auth` rather than only at login.
Session tokens live 30 days (`SESSION_TTL_SECS`), so a login-only check would leave a disabled user
working for up to a month. Checking in `resolve_auth` ends their session on their next request, which
matches the behavior Jellyfin documents for its own disable control. `resolve_auth` already queries
the account row, so this costs nothing extra.

A disabled account's API tokens stop working by the same path.

## The first administrator

The first account created through `POST /v1/auth/register` that is neither the demo account nor a
guest becomes an administrator.

```sql
SELECT COUNT(*) = 0 FROM accounts WHERE id != <DEMO_ACCOUNT_ID>
```

The predicate is evaluated inside the register transaction, alongside the insert, because that is
where the decision belongs. With `guest_status` removed, excluding the demo account by id is
sufficient; before removal the predicate would also have needed `guest_status IS NULL`.

This matters because a `--reset-demo` vault is not empty. It contains the seeded demo account, so a
naive `COUNT(*) = 0` would give the first real person an ordinary account and leave the vault with no
administrator at all.

### The last-admin guard

Demoting, disabling, or deleting the only remaining administrator is refused with `400`. Without this
the vault becomes unadministrable and there is no recovery path: the first-administrator rule only
fires when the vault has no non-demo accounts, which is no longer true.

This is a single-request correctness rule, not a concurrency defense.

## The admin surface

A new route group. Existing routes are untouched and stay scoped to the caller's own `account_id`.

| Endpoint | Purpose |
|---|---|
| `GET /v1/admin/users` | username, `is_admin`, `disabled`, the three capability flags, message count, storage bytes |
| `POST /v1/admin/users` | create an account with an initial password and flags |
| `PATCH /v1/admin/users/{id}` | set `disabled`, `is_admin`, `can_import`, `can_export`, `can_delete` |
| `PUT /v1/admin/users/{id}/password` | set a new password for that account |
| `DELETE /v1/admin/users/{id}/messages` | delete that account's messages and attachments |
| `DELETE /v1/admin/users/{id}` | delete the account, cascading its data |

Every route is behind `require_admin`. Every response carries counts and metadata; none carries
message content, so the panel has nothing to render even in error.

`DELETE /v1/admin/users/{id}/messages` reuses the functions the self-service endpoint already calls —
both are parameterized by account id today:

```rust
account_profile::delete_all_messages_for_account(&mut conn, &account_id)
remove_account_asset_trees(&data_dir, &account_id, &assets_name, &converted_name)
```

`DELETE /v1/admin/users/{id}` reuses `account_profile::delete_account` and the data-directory removal
from `delete_account_handler`, minus the current-password check, which does not apply when an
administrator is the caller.

Granularity is all-of-an-account's-messages. Per-import and per-conversation deletion are deferred;
they would be new functionality rather than a reuse of what exists.

## Self-service lifecycle

Unchanged, and open. Users register themselves at `POST /v1/auth/register`, and delete themselves at
`POST /v1/auth/delete-account`, which verifies the current password, cascades every message,
conversation, and contact through the existing `ON DELETE CASCADE` foreign keys, and removes the
account's data directory from disk.

This is a deliberate divergence from Jellyfin, which has no self-registration and creates every
account from the dashboard. Here the door stays open and the administrator's controls — disable,
delete messages, delete account — are the remedy after the fact.

## What this does not protect against

Nothing in this design prevents an administrator from reading another user's messages by means
outside the product. Message bodies are stored as plaintext (`messages.body TEXT`, commented
"Plain-text body"), the FTS5 index holds a second plaintext copy of every body, subject, and
attachment transcription, and attachments are ordinary files under `data/assets/<account_id>/`. An
administrator with shell access reads all of it with one `sqlite3` or `psql` query.

What the design provides is that the product never puts one tenant's messages in front of another.
That is a real property and worth having, but it must not be described in the UI or the docs as a
privacy guarantee, because the storage does not provide one. Genuine read protection would require
per-tenant encryption keyed on the user's password, which would end server-side search, make an
administrator's password reset destroy access to the data, and is explicitly out of scope.

## Schema changes

Both engines, per the dual-engine rule: `schema/sql/accounts.sql` and `schema/sql/pg_accounts.sql`
change together. Every new column needs a `--` comment on the line above it or
`scripts/check-sql-column-comments.mjs` fails.

Two tables change. On `accounts`: add `is_admin`, `disabled`, `can_import`, `can_export`,
`can_delete`; drop `hanko_user_id`, `read_only`, `guest_status`, and `ix_accounts_hanko_user_id`. On
`account_api_tokens`: drop `scopes` and add the same three `can_*` columns, so both tables describe
permission with identical names and types.

Defaults differ deliberately. On `accounts` the three permissions default to 1, because a new user
can do everything until an administrator says otherwise. On `account_api_tokens`, `can_delete`
defaults to 0 — a token is a narrowing of its owner's rights, and destruction should be asked for
rather than inherited.

`SCHEMA_VERSION` in `crates/vault/server/src/db/schema.rs` goes from `2` to `3`. There are no
in-place migrations, so every existing database is rebuilt empty and its owner re-imports. This is
accepted.

`scripts/sync-vault-schema.mjs` regenerates the `web-next/` copy and the `tests/fixtures/schema/`
snapshot. `docs/src/assets/openapi.json` is regenerated for the removed and added routes.

## Web changes

A **Users** panel in Settings, rendered only when the signed-in account has `is_admin`. Settings is
already composed of independent panels (`AccountSettingsPanel`, `ApiTokensSection`, `StorageSection`,
`SystemSection`), so this adds a sibling rather than restructuring anything.

The panel lists accounts with username, status, message count, and storage used, and offers create,
disable, permission edit, password reset, delete-messages, and delete-account. Destructive actions
confirm, and the delete confirmations state the message count that will be destroyed rather than
asking for a blind yes.

Labels are written for the reader, not after the columns:

| Column | Label |
|---|---|
| `is_admin` | Allow this user to manage the vault |
| `disabled` | Disable this user |
| `can_import` | Allow importing messages |
| `can_export` | Allow exporting messages |
| `can_delete` | Allow deleting messages and attachments |

`web/src/lib/api.ts` gains the admin client calls; the profile response drops `read_only` and gains
the capability flags so the UI can hide what the account may not do. Hiding remains a courtesy — the
server is now the thing that actually enforces it.

### The API token form

`web/src/screens/settings/useApiTokens.ts` currently posts `scopes: "both"` as a literal — there is
no scope selector anywhere in Settings, so every token ever created from the UI holds every
permission. That is tolerable while the permissions are import and export. It is not tolerable once
delete exists.

The create-token form gains three checkboxes — Import, Export, Delete — with import and export
checked and delete unchecked by default. A checkbox for a permission its owner does not hold is
disabled, with a line saying why, since a token can never exceed its account. `scopesLabel` in
`apiTokensUtils.ts` is replaced by rendering the set, and `ApiTokensTable` shows which permissions
each token carries rather than one word.

## Testing

- The first non-demo registration becomes an administrator; the second does not.
- On a `--reset-demo` vault, the first human registration still becomes the administrator.
- Each capability flag, off, produces `403` from its guard, and on, produces success.
- An API token whose owner lost `can_import` cannot import, even with import permission — the
  intersection rule.
- A token holding delete can call `POST /v1/account/delete-messages`; one without it gets `403`.
- A token cannot call `delete-account`, `change-password`, the API-token management routes, or any
  `/v1/admin/*` route, whatever permissions it holds.
- A token created through the API without an explicit delete permission does not get one.
- A disabled account's existing session token is rejected on its next request, not merely at login.
- The last administrator cannot be demoted, disabled, or deleted.
- `require_admin` rejects both non-admin sessions and API tokens.
- Admin message deletion removes the target's messages and asset trees and leaves other accounts
  untouched — the multitenancy assertion.
- No admin endpoint returns message content.
- Schema: both engines build at version 3; the fixture snapshot matches; every new column has a
  comment.

Postgres engine tests need a reachable dev Postgres:
`MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server`.

## Decisions taken by default

Both were raised and not ruled on; either can be reversed on review.

1. **The last-admin guard is included.** The alternative is an unadministrable vault with no recovery
   path.
2. **Admin deletion is all-of-an-account's-messages.** Per-import and per-conversation granularity is
   deferred as new functionality.

## Deferred

- The login screen redesign — the original request, still unspecified.
- Per-import and per-conversation admin deletion.
- Folding `web-next/` into any of this. It stays legacy.
- Any audit log of administrative actions. Nothing records who disabled or deleted whom.
