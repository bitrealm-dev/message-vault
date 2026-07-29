# Web UI

Next.js app that browses the vault SQLite database (`data/vault.db` by default).

## Demo quick start

From the repo root:

```bash
./scripts/setup-demo.sh
cd web && npm install && npm run dev
```

Open [http://localhost:3000](http://localhost:3000). On first visit you will land on **/login** — create a user or pick an existing one (no password). Use **Reset demo** in the sidebar footer to wipe local edits and re-import the committed bundle.

Multi-user note: each web account has its own vault partition in the shared `vault.db` (`account_id` on rows). CLI ingest/import accepts `--account <username>` (or UUID). Remote vault-push with a personal Import API token does not need `--account`.

## Setup

From this directory:

```bash
npm install
```

Ensure the vault has been imported (see the [root README](../README.md) ingest flow). Then convert media for the browser:

```bash
npm run process-assets
```

Flags: `--force`, `--dry-run`, `--skip-image`, `--skip-video`, `--skip-audio`.

## Dev server

```bash
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

## Source filter

The sidebar **Source** dropdown lists configured sources from `config/config.toml`.

- A single source shows every message from that archive.
- **All (combined)** merges person threads and hides soft-deduped copies (`duplicate_of`).

## Contact sections

Manage visibility with the `exclude` column in `config/contacts.csv` only:

| Section | Meaning |
|---------|---------|
| **Contacts** | Non-excluded contacts with messages (All − Excluded) |
| **All** | Every contact with messages, including excluded |
| **Excluded** | `exclude=true` |

Labels and No label list only non-excluded contacts.

`contacts.csv` is **phone-only**. SQLite `contact_handles` holds phones plus optional iMessage emails for thread linking; emails are not written to the CSV. Leave unmapped email peers in Unassigned. Older `contact_phones` DBs are not upgraded — wipe `data/vault.db` and re-ingest.

## Notes

- Paths and DB location are read from the repo-root `config/config.toml`.
- Converted assets land under `data/<account_id>/<source_id>/assets_converted`.
- **Reset demo** is CLI-only: `cargo run --release -- reset-demo`. The web UI only shows that hint; JSONL import is the Rust `serve` API / CLI, not Next.js.
- Ingest: `message-vault-rs serve` → `POST /v1/import` (`application/jsonl`).
