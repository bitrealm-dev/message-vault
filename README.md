# Message Vault

Message Vault keeps your text-message history in one place so you can browse it
in a local website—search conversations, open photos and attachments, and see
iPhone and Android backups side by side.

You run it on a computer you control. Your messages stay in a SQLite database on
that machine; they are not uploaded to a cloud service by this project.

Turning a phone backup into files the vault understands is done by a separate
project, [message-exporters](https://github.com/bitrealm-dev/message-exporters).
This repository is the vault itself: storage, import, and the browser UI.

## How it works

```text
1. Export     Take a backup from your phone or backup app
              → JSONL + attachments/ (message-exporters)

2. Import     Send that folder into your vault
              → Message Exporters Vault tab / vault-push CLI
              → or local `ingest` on the vault machine

3. Browse     Open the website on your computer
              → read and search your history
```

An **export folder** (also called staging) is the folder the exporter wrote: one
`*.jsonl` file per conversation, plus any photos or other media (typically under
`attachments/`).

Two processes are involved when you import over the network:

- **Next.js web UI** (`cd web && npm run dev`) — browse and manage the vault
- **Rust import API** (`cargo run --release -- serve`) — receive remote pushes

Browsing alone only needs the web UI. Remote import needs both.

## Requirements

- Rust 1.85 or newer (edition 2024)
- Node.js 20.9 or newer and npm
- A native C/C++ build toolchain
- Optional: FFmpeg for video/audio/HEIC conversion (`npm run process-assets`)

Full Windows and Linux install steps: [docs/development.md](docs/development.md).

## Try it with sample data

No real backup needed. From this repository:

```bash
./scripts/setup-demo.sh
cd web && npm ci && npm run process-assets && npm run dev
```

Open <http://localhost:3000/login>. Sign in as the seeded **`demo`** account
(or create another account). You should see demo contacts and conversations.

On Windows, use the native PowerShell commands in the
[developer setup guide](docs/development.md).

To put the demo data back later (CLI only — the web menu shows this hint but
does not run the reset):

```bash
cargo run --release -- reset-demo
```

`reset-demo` overwrites `config/config.toml` with the demo config, which has
`[server]` commented out. Before using remote import (`serve`), restore the
example config (or uncomment `[server]`) — see below.

## Bring in your own messages

### 1. Start the vault and create an account

On the computer that will hold your archive:

1. Copy instance config (and optional CSV templates):

```bash
cp config/config.toml.example config/config.toml
cp config/contacts.csv.example config/contacts.csv   # optional template
cp config/exclude.csv.example config/exclude.csv     # optional template
```

If you previously ran the demo, `config/config.toml` may still be the demo
copy. Prefer re-copying from `config.toml.example` so `[server]` is enabled.

2. Build and start the import server:

```bash
cargo build --workspace --release
cargo run --release -- serve
```

3. In another terminal, start the website:

```bash
cd web && npm ci && npm run process-assets && npm run dev
```

4. Open the site, create an account, then go to **Settings → Account** and copy
   your **Import API token**. That token identifies your account. You do **not**
   need an account UUID.

Keep `serve` running while you import remotely.

New accounts start in **read-only** mode for the web UI. Turn that off in
Settings when you want to edit contacts, trash items, or manage labels.
Imports through the API or CLI still work while read-only is on.

### 2. Export your phone backup

On the machine that has your backup files, use
[message-exporters](https://github.com/bitrealm-dev/message-exporters). Pick the
converter that matches what you have (for example Apple Messages, SMS Backup &
Restore, SMS Backup+, GO SMS Pro).

Prefer a **JSONL** export folder (plus `attachments/`) from Message Exporters.

### 3. Send the export into the vault

Prefer **Message Exporters** (Vault tab or `vault-push` CLI). Export with
**JSONL** in the Message tab, then import that folder.

In the [message-exporters](https://github.com/bitrealm-dev/message-exporters)
repo:

```bash
# GUI
cargo run -p message-exporters-gui --release
# → Vault tab: URL, username, Vault key (Import API token), input directory

# CLI
cargo run -p message-vault-client --bin vault-push --features cli --release -- \
  --input ./path/to/your-jsonl-export \
  --url http://vault-host:8080 \
  --username yourusername \
  --key "$VAULT_KEY"
```

### 4. Browse

Refresh the website. Use **Message Sources** to look at one backup type or
**Combined**. Sidebar sections:

| Nav item | Route | Meaning |
|----------|-------|---------|
| **All** | `/all` | Every contact with messages |
| **Active** | `/contacts` | Non-excluded contacts with messages |
| **Inactive** | `/excluded` | Contacts marked `exclude` |
| **Group Messages** | `/group-messages` | Multi-party threads |
| **Trash** | `/trash` | Soft-deleted contacts and group chats |
| **Settings** | `/settings/account` | Account, Import API token, appearance |

Labels appear in the sidebar when you create them. More UI detail:
[`web/README.md`](web/README.md).

## Same computer (optional)

If the export folder already lives on the vault machine, you can import without
the network push tools. Create a staging directory yourself (for example
`staging/<source>/`); nothing under that path is committed in the repo.

```bash
# One source (username from your web account)
cargo run --release -- ingest go-sms-pro \
  --account yourusername \
  --staging-dir staging/go-sms-pro

# Or several sources that already have folders under staging/
./scripts/ingest-staging.sh --account yourusername \
  --source imessage --source go-sms-pro
```

`ingest` imports JSONL and runs cross-source soft-dedupe afterward (unless
`--skip-dedupe`). Then convert media for the web UI if needed:

```bash
cd web && npm run process-assets
```

## Browse tips

- **Message Sources** — one archive (e.g. iMessage only) or **Combined**
  (merges threads and hides soft-deduped copies). Sources come from imported
  data under `data/<account_id>/<source_id>/`, not from `config.toml`.
- **Active / Inactive** — visibility is driven by the `exclude` column in the
  per-account contacts CSV (`data/<account_id>/contacts.csv`), not the optional
  templates under `config/`.
- **Group Messages** — four-panel layout: your vault identity, all group chats,
  and the selected thread.
- **Trash / Undo** — soft-delete contacts or group chats; restore from Trash.
  Create/delete label and trash actions support undo/redo from the actions menu
  (with a short snackbar after undoable actions).
- **Read-only mode** — Settings → Account; blocks web edits while leaving CLI
  and import API available.

## For developers and operators

For complete Windows and Linux prerequisites, setup commands, checks, and
troubleshooting, see the [developer setup guide](docs/development.md).

### Repository layout

```text
src/                # CLI binary: import, ingest, serve, export, demo reset
crates/
  message-json/     # vault JSONL schemas
  demo-seed/        # regenerate committed demo data
demo/               # committed demo bundle (staging + seed)
config/             # config.toml.example and CSV/VCF examples
scripts/            # setup-demo, ingest-staging, smoke tests
web/                # Next.js UI
docs/               # schema, dedupe, development
```

Backup → JSONL exporters live in
[message-exporters](https://github.com/bitrealm-dev/message-exporters). Local
`ingest` takes `--staging-dir` with `*.jsonl` files; remote clients push over
the HTTP import API. Asset files land under
`data/<account_id>/<source_id>/…`.

### Config

See [`config/config.toml.example`](config/config.toml.example). Runtime config
is instance paths plus optional `[server]`. Source names are not listed in TOML —
each import registers its own source for that account.

Owner/login identity lives in SQLite. Demo seed identity:
`demo/config/seed.toml` (username `demo`, read-only by default).

Per-account files (created on first use if missing):

- `data/<account_id>/contacts.csv`
- `data/<account_id>/exclude.csv`

Vault JSONL contract: [`crates/message-json`](crates/message-json).

### CLI reference

| Command | Purpose |
|---------|---------|
| `ingest` | Import a JSONL staging folder for one source, then soft-dedupe |
| `import` | Import JSONL only (no automatic cross-source dedupe) |
| `dedupe-cross-source` | Soft-hide the same SMS across sources |
| `import-contacts` | Reload contacts CSV into SQLite |
| `vcf-to-contacts` | Convert `.vcf` → `contacts.csv` (+ optional `exclude.csv`) |
| `export-markdown` | Obsidian bubble markdown export |
| `reset-demo` | Restore the committed demo bundle |
| `serve` | HTTP import API (`[server]` required in config) |

```bash
# ingest (CLI default mode: replace)
cargo run --release -- ingest imessage \
  --account yourusername \
  --staging-dir staging/imessage \
  --mode replace

# import without auto-dedupe, then dedupe separately
cargo run --release -- import \
  --source imessage \
  --export-dir staging/imessage \
  --mode replace \
  --account yourusername
cargo run --release -- dedupe-cross-source --account yourusername

# VCF → contacts CSV for an account
cargo run --release -- vcf-to-contacts \
  --vcf path/to/contacts.vcf \
  --account yourusername
```

Helpers: `./scripts/ingest-staging.sh`, `./scripts/import-staging.sh`.

### HTTP import API

`serve` reads `[server]` in config (`bind`). Prefer Message Exporters
**vault-push** / Vault tab:

1. `GET /health` — liveness (no auth)
2. `GET /v1/auth/check` — verify username + Import API token (from web Settings)
3. `PUT /v1/assets/{sha256}?source=&account=` — upload each attachment by digest
4. `POST /v1/import?source=&account=&mode=&dedupe=` — vault JSONL

Auth is per-account only (no host-wide admin token). Multipart uploads use field
`jsonl` plus `file` parts.

Defaults for HTTP import (different from CLI):

- `mode=append` (CLI `ingest` / `import` default to `replace`)
- `dedupe=false` (pass `dedupe=true` to run cross-source soft-dedupe after import)
- `account=` optional when the Bearer token identifies the tenant

```bash
curl -sS "http://127.0.0.1:8080/v1/auth/check" \
  -H "Authorization: Bearer <import-api-token-from-settings>"
```

Smokes: `./scripts/smoke-import-api.sh`, `./scripts/smoke-vault-push.sh`.

### Import modes and dedupe

- **replace** — wipe that source’s messages for the account, then reload.
- **append** — keep existing rows; skip when `(account_id, source, guid)` already
  exists.

Cross-source soft-dedupe (exact / near-time matches): [docs/dedupe.md](docs/dedupe.md).  
Database tables: [docs/schema.md](docs/schema.md).

### Obsidian export

```bash
cargo run --release -- export-markdown \
  --out /path/to/Obsidian-Message-Vault \
  --account yourusername
```

Enable the `message-vault-bubbles` CSS snippet in Obsidian (from
`config/obsidian-message-vault.css`).

### Demo data for maintainers

```bash
cargo run -p demo-seed -- --out demo --seed 42
```

See [`demo/README.md`](demo/README.md).

### Common checks

```bash
cargo build --workspace
cargo test --workspace

cd web
npm run lint
npm test
npm run build
```

Health: <http://localhost:3000/login>, <http://127.0.0.1:8080/health>.
