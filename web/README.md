# Web UI

Next.js app that browses the vault SQLite database (`data/vault.db` by default).

## Demo quick start

From the repo root:

```bash
./scripts/setup-demo.sh
cd web && npm ci && npm run process-assets && npm run dev
```

Open [http://localhost:3000/login](http://localhost:3000/login). Sign in as the
seeded **`demo`** account, or create another user (no password).

**Reset demo** is CLI-only:

```bash
cargo run --release -- reset-demo
```

The sidebar menu shows that hint; the web app does not run the reset itself.
Note that `reset-demo` overwrites `config/config.toml` with the demo config
(`[server]` disabled). See the [root README](../README.md) before using remote
import again.

Multi-user note: each web account has its own vault partition in the shared
`vault.db` (`account_id` on rows). CLI ingest/import accepts
`--account <username>` (or UUID). Remote vault-push with a personal Import API
token does not need an account UUID.

## Setup

From this directory:

```bash
npm ci
```

Ensure the vault has been imported (see the [root README](../README.md) ingest
flow). Then convert media for the browser:

```bash
npm run process-assets
```

Flags: `--force`, `--dry-run`, `--skip-image`, `--skip-video`, `--skip-audio`.

## Dev server

```bash
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

## Navigation

| Label | Route | Meaning |
|-------|-------|---------|
| **Home** | `/` | Dashboard stats |
| **All** | `/all` | Every contact with messages (including inactive) |
| **Active** | `/contacts` | Non-excluded contacts with messages |
| **Inactive** | `/excluded` | Contacts with `exclude=true` |
| **Group Messages** | `/group-messages` | Multi-party threads |
| **Trash** | `/trash` | Soft-deleted contacts and group chats |
| **Settings** | `/settings/account` | Account, Import API token, appearance |

Additional contact views: **No Messages** (`/no-messages`), **No label**
(`/no-label`), and per-label pages under `/label/[slug]`.

Contact pages use a multi-panel layout (list → threads → messages / details).
**Group Messages** uses a four-panel layout: vault owner (read-only), group chat
list, and thread view.

## Message Sources

The **Message Sources** control lists sources discovered from imported data
(`data/<account_id>/<source_id>/` and the database). They are **not** configured
in `config.toml`.

- A single source shows every message from that archive.
- **Combined** merges person threads and hides soft-deduped copies
  (`duplicate_of`).

## Contact visibility

Manage visibility with the `exclude` column in the **per-account** contacts CSV:

`data/<account_id>/contacts.csv`

| Section | Meaning |
|---------|---------|
| **Active** | Non-excluded contacts with messages |
| **All** | Every contact with messages, including inactive |
| **Inactive** | `exclude=true` |

Labels list only non-excluded contacts by default. Create and manage labels in
the sidebar (stored in SQLite, not as CSV “groups”).

`contacts.csv` is **phone-only**. SQLite `contact_handles` holds phones plus
optional iMessage emails for thread linking; emails are not written to the CSV.
Unmapped handles appear in Trash workflows / unassigned APIs rather than a
dedicated browse route. Older `contact_phones` DBs are not upgraded — wipe
`data/vault.db` and re-ingest.

## Settings

- **Account** — username, emails, read-only mode, Import API token (copy /
  regenerate), vault identity (owner name/phones), danger zone (delete messages
  / account).
- **Appearance** — theme, message badges, date/time format.

New accounts start **read-only**. Turn that off before editing contacts or
trashing items. Imports through the Rust API/CLI remain available while
read-only is on.

## Undo / redo

Soft-delete contacts, soft-delete group chats, and create/delete label actions
are undoable from the list actions menu. After an undoable action, a snackbar
appears at the bottom of the screen for 15 seconds with an **Undo** control.
Choosing Undo/Redo from the menu does not show that snackbar.

## Notes

- Paths and DB location are read from the repo-root `config/config.toml`
  (override with `VAULT_DB` / `VAULT_DATA_DIR` if needed).
- Converted assets land under
  `data/<account_id>/<source_id>/assets_converted`.
- JSONL import is the Rust `serve` API / CLI, not Next.js.
- Checks: `npm run lint`, `npm test`, `npm run build`.
