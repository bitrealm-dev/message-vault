# Hosted guest demo (private copies, ready pool)

## Goal

A hosted vault offers a **Try it** button. One click signs the visitor in through the **website** (the same Vite SPA the desktop app already ships). They land in a **private copy** of the sample conversations. They can browse, search, edit, and delete. They cannot import or export a backup. They do not need to download the desktop app.

Self-hosted keeps today’s shared `demo` login (username `demo`, empty password). The pool and guest clones run only when a hosted flag is on.

## Why the shared `demo` account is not enough

The vault already isolates rows by `account_id`. `reset-demo` fills one account: fixed id `00000000-0000-0000-0000-00000000d001`, username `demo`, no password, `read_only`. Anyone can sign in as that user.

That fails on a public host for two reasons:

1. **One session token per account.** Login rotates `account_session_tokens`. The next sign-in invalidates the previous visitor.
2. **One inbox.** Edits and deletes would collide if many people shared that account.

A full `reset-demo` import per visitor is too slow. The sample set is large (about 200 contacts and years of messages). The clone copies database rows and hard-links already-processed attachment files.

## Product decisions

| Question | Choice |
|---|---|
| Who is this for? | Visitors on a hosted vault; one-click Try it in the browser |
| Shared inbox or private copy? | Private copy |
| What can they do? | Browse, search, edit, delete. No backup import or export |
| How long does the copy last? | While the session token is valid (24 hours). Then the guest account is deleted. A later Try it starts a fresh copy |
| How is the first click fast? | Small ready pool of unused clones; the pool can grow up to a ceiling |
| Client | Website only for this flow. Same `web/` SPA as the desktop app. Import, Export, Extract, and Format stay desktop-only |

## Account model

Three kinds of account:

1. **Template `demo`** — the account `reset-demo` already creates. Read-only, empty password, fixed id. Clone source only on a hosted vault. Password login as `demo` remains for self-hosted. On a hosted vault (`try_demo: true`), `POST /v1/auth/login` as `demo` is rejected so visitors cannot share the template inbox; they use Try it.
2. **Guest** — a private copy of that template. No password. Username `guest-<short-id>` so it never collides with a registration. Not `read_only` (edits and deletes are allowed). Column `guest_status` is `'ready'` or `'assigned'` (`NULL` on non-guest rows). That flag blocks backup import, backup export, and API-token creation on the server. The UI hides those actions.
3. **Normal registered account** — unchanged. Empty inbox, import allowed. Still how someone keeps a real archive on the same vault.

Guests already have a display name and handle rows from the clone, so they skip onboarding.

`account_emails` is unique across the whole vault. Do **not** copy those rows. “You” in the UI still works because each guest gets its own `handles` and `account_handles` rows (unique per account).

## Client: one SPA, browser disables desktop work

The Vite app under `web/` is already both the Tauri desktop UI and the files the vault server serves at the same origin. Hosted Try it uses that website. The flow must not tell the visitor to install the desktop app.

The sidebar already hides **Import** and **Export** when `isTauri()` is false. The login screen already hides **Extract messages** and **Format conversion** the same way. System settings already show a “desktop app only” note in the browser. Keep that split.

Additional UI for this feature:

- Login: when `GET /v1/auth/mode` reports `try_demo: true`, show a **Try it** button that calls `POST /v1/auth/try-demo` and stores the token. Username/password and Register stay for people who want a real account.
- Login: when `try_demo: false`, the same button signs in as `demo` with an empty password (self-hosted).
- After sign-in as a guest: hide Import, Export, and API-token settings even if someone pointed the desktop app at the hosted vault. Show a short note that this is a temporary sample (24 hours).
- Routes `/import` and `/export` redirect away for guests and for non-Tauri sessions.
- Logout deletes the guest account immediately so an edited copy cannot return to the pool.

Browse, search, contacts, trash, settings (appearance / profile that does not create tokens), and media viewing stay in the browser. Those already talk to `/v1/export/messages`, `/v1/export/contacts`, and `/v1/export/conversations`. Those paths are the **browse** API. Guests must keep them.

## Try it API

`POST /v1/auth/try-demo`

- No body.
- Rate-limited like login.
- Response matches login: `{ token, account_id, username }`.
- Assigns a ready guest or waits for one on-demand clone (timeout, then 503).

`GET /v1/auth/mode` adds `try_demo: true|false` so the login screen can choose the button behavior.

### Server rejects for guests (403)

- `POST /v1/import` and `POST /v1/imports` (start an import)
- `PUT /v1/assets/...` (upload attachment bytes)
- Create or rename API tokens
- Change password (guests have none)

Reads stay allowed, including import history and media `GET`. Message and contact edits and deletes stay allowed.

`GET /v1/export/messages` is the conversation view. Do not block it.

## Clone

Integer primary keys (`handles`, `contacts`, `conversations`, `messages`, `attachments`, `tapbacks`, labels, import rows, and the rest) are global in the shared SQLite file. A clone inserts new rows and rewrites every foreign key. `account_id` becomes the guest id.

Copy every account-scoped table **except**:

- `account_emails` (vault-wide unique)
- `account_session_tokens` (ready guests have no session)
- `account_api_tokens` (guests cannot create tokens)

Inserting messages fires the existing `messages_fts` triggers, so search works without a separate rebuild. That cost is why clones run in the background.

Attachment `assets_path` values are relative to the account asset directory (hash-based). After the SQL clone, hard-link every file under the template’s `data/<demo-id>/<source>/assets` and `assets_converted` into the guest directories. Same Docker volume means the same filesystem. If a hard link fails, copy that file. Deleting a guest removes its directory; the template files stay.

One clone at a time. `reset-demo` already takes a lock; clones wait on that lock.

## Ready pool

Unused ready guests sit in the pool with **no** session token and status **ready**. Assignment is one transaction: pick a ready row, mark it **assigned**, write a 24-hour session token.

An assigned guest never returns to the pool. Logout deletes it. A sweeper deletes assigned guests whose session has expired.

| Knob | Default | Meaning |
|---|---|---|
| Floor | 2 | After a handoff, refill unused ready guests back to this count |
| Ceiling | 20 | Unused ready guests never exceed this |
| Session | 24 hours | Guest token lifetime (normal accounts stay at 30 days) |

In-use guests do not count toward the unused pool. Fifty concurrent visitors means 50 assigned copies plus a few unused ready ones.

Refill target: `max(floor, assignments in the last 15 minutes)` capped at the ceiling. If unused ready exceeds the ceiling, delete the oldest ready guests.

Empty pool: Try it waits for one on-demand clone. Refill continues in the background.

Worker (only when the hosted flag is on):

1. On `serve` start, fill unused ready guests up to the floor.
2. On a timer: refill, shrink if over the ceiling, delete assigned guests with expired sessions.
3. After `reset-demo`: delete unused ready guests (stale vs the new template) and refill. Assigned guests keep the old snapshot until they expire.

## Config

Off by default so self-hosted is unchanged. Env (Compose) or matching `[server]` keys:

| Key | Default | Meaning |
|---|---|---|
| `GUEST_DEMO_POOL` | `false` | Enable the pool, `try_demo` on `/v1/auth/mode`, and reject password login as `demo` |
| `GUEST_POOL_MIN` | `2` | Unused ready floor |
| `GUEST_POOL_MAX` | `20` | Unused ready ceiling |
| `GUEST_SESSION_SECS` | `86400` | Guest session lifetime |

`DEMO_DATA` still controls whether first boot runs `reset-demo`. The pool needs that template account. A hosted image that sets the pool flag should keep `DEMO_DATA=true`.

## Tests

- Clone remaps ids and foreign keys; the guest sees the same conversation count as the template; template rows are unchanged.
- Second clone does not fail on `account_emails` uniqueness.
- Assignment is atomic: two concurrent `try-demo` calls never receive the same guest.
- Assigned guest is not reused after logout; the account row is gone.
- Expired session: sweeper deletes the guest and its `data/<id>/` tree; template files remain.
- Guest `POST /v1/imports` and `PUT` asset return 403; `GET /v1/export/messages` succeeds.
- `try_demo: false`: Try it path logs in as `demo`; no guest rows are created.
- `try_demo: true`: `POST /v1/auth/login` as `demo` is rejected.
- After `reset-demo`, unused ready guests are dropped and refilled from the new template.

## Docs

Hosted Try it: open the vault URL in a browser, click Try it, browse. Do not send that reader to install the desktop app.

Self-hosted: username `demo`, empty password, still documented. The existing “create an account, then import with the desktop app” path stays for people who want their own messages.

## Out of scope

- Turning a guest into a registered account
- Keeping a copy after the session ends
- Changing the sample dataset itself
- Allowing many concurrent sessions on the shared self-hosted `demo` user
- Copying attachment bytes instead of hard links
- Building a separate web app; this is the existing `web/` SPA

## Acceptance

- Hosted: Try it in the browser signs in in well under a second when the pool has a ready guest, and shows a short wait only if the pool is empty.
- Two visitors get different `account_id`s and do not see each other’s deletes.
- A guest cannot start an import or create an API token.
- Import, Export, Extract, and Format do not appear in the browser UI.
- After 24 hours (or logout), the guest account and its rows are gone.
- Self-hosted with the flag off still signs in as the shared `demo` account.
