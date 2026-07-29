# Message Vault

Message Vault keeps your text-message history in one place so you can browse it in a local website—search conversations, open photos and attachments, and see iPhone and Android backups side by side.

You run it on a computer you control. Your messages stay in a database on that machine; they are not uploaded to a cloud service by this project.

Turning a phone backup into files the vault understands is done by a separate project, [message-exporters](https://github.com/bitrealm-dev/message-exporters). This repository is the vault itself: storage, import, and the browser UI.

## How it works

```text
1. Export     Take a backup from your phone or backup app
              → JSONL + photos (message-exporters Message tab)

2. Import     Send that folder into your vault
              → Message Exporters Vault tab (or vault-push CLI)

3. Browse     Open the website on your computer
              → read and search your history
```

An **export folder** (sometimes called staging) is simply the folder the exporter wrote: one spreadsheet file per conversation, plus any photos or other media next to those files.

## Try it with sample data

No real backup needed. From this repository:

```bash
# Needs Rust (https://www.rust-lang.org/tools/install) and Node.js for the website
./scripts/setup-demo.sh
cd web && npm install && npm run process-assets && npm run dev
```

Open the URL the web app prints (usually `http://localhost:3000`). You should see demo contacts and conversations you can click through.

On Windows, use the native PowerShell commands in the
[developer setup guide](docs/development.md). The guide also covers Linux
prerequisites and troubleshooting.

To put the demo data back later:

```bash
cargo run --release -- reset-demo
```

## Bring in your own messages

### 1. Start the vault and create an account

On the computer that will hold your archive:

1. Copy [`config/config.toml.example`](config/config.toml.example) to `config/config.toml` and adjust paths if you like.
2. Build and start the import server:

```bash
cargo build --workspace --release
cargo run --release -- serve
```

3. In another terminal, start the website:

```bash
cd web && npm install && npm run process-assets && npm run dev
```

4. Open the site, create an account, then go to **Settings** and copy your **Import API token**.  
   That token identifies your account. You do **not** need an account UUID.

Keep `serve` running while you import.

### 2. Export your phone backup

On the machine that has your backup files, use [message-exporters](https://github.com/bitrealm-dev/message-exporters). Pick the converter that matches what you have (for example Apple Messages, SMS Backup & Restore, SMS Backup+, GO SMS Pro).

Prefer a **JSONL** export folder (plus `attachments/`) from Message Exporters. CSV remains useful for spreadsheet review and local staging ingest.

### 3. Send the export into the vault

Prefer **Message Exporters** (Vault tab or `vault-push` CLI). Export with **JSONL** in the Message tab, then import that folder.

In the [message-exporters](https://github.com/bitrealm-dev/message-exporters) repo:

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

The old `csv-ingest` `vault-push` / `vault-push-gui` binaries remain buildable for CSV workflows but are **deprecated**.

### 4. Browse

Refresh the website. Use the **Source** control in the sidebar to look at one backup type or **All (combined)**. Under contacts you will see **Contacts**, **All**, and **Excluded**—see [`web/README.md`](web/README.md) for details.

## Same computer (optional)

If the export folder already lives on the vault machine next to your config, you can import without the network push tools:

```bash
# One source (username from your web account)
cargo run --release -- ingest go-sms-pro --account yourusername

# Or several sources that already have folders under staging/
./scripts/ingest-staging.sh --account yourusername
```

Then convert media for the web UI if needed: `cd web && npm run process-assets`.

## Browse tips

- **Source filter** — one archive (e.g. iMessage only) or everything together.
- **Contacts** — people with messages who are not marked excluded.
- **All / Excluded** — manage who appears in the main Contacts list via `exclude` in your contacts CSV (see config examples).

More UI detail: [`web/README.md`](web/README.md).

---

## For developers and operators

This section is for people maintaining the vault, wiring automation, or debugging imports.

For complete Windows and Linux prerequisites, setup commands, checks, and
troubleshooting, see the [developer setup guide](docs/development.md).

### Repository layout

```text
crates/
  message-json/     # vault NDJSON schemas
  csv-ingest/       # CSV → vault convert + vault-push / vault-push-gui
  demo-seed/        # regenerate committed demo data
config/             # config.toml.example and related examples
scripts/            # setup-demo, ingest-staging, smoke tests
web/                # Next.js UI
docs/               # schema, dedupe
```

Backup → CSV converters live in [message-exporters](https://github.com/bitrealm-dev/message-exporters), not in this repo. Fill each source’s `export_dir` (staging) before local `ingest`.

### Config sources

See [`config/config.toml.example`](config/config.toml.example). Each `[[sources]]` entry has an `id` and `export_dir`. Asset files default under `data/<account_id>/<source_id>/…`.

Ingest contract (CSV columns → vault NDJSON): [`crates/message-json/docs/CSV_INGEST.md`](crates/message-json/docs/CSV_INGEST.md).

### HTTP import API

`serve` reads `[server]` in config (`bind`, `api_token`). Prefer Message Exporters **vault-push** / Vault tab:

1. `GET /v1/auth/check` — verify username + Import API token  
2. `PUT /v1/assets/{sha256}?source=&account=` — upload each attachment by digest  
3. `POST /v1/import?source=&account=&mode=` — vault NDJSON (`application/x-ndjson`), with attachment `sha256` fields resolving to uploaded assets  

Per-account tokens come from web Settings; `[server] api_token` is an admin secret for ops/smoke. Legacy csv-ingest clients still use multipart `ndjson` + `file` parts.

```bash
curl -sS "http://127.0.0.1:8080/v1/auth/check" \
  -H "Authorization: Bearer <user-token-from-settings>"
```

Smokes: `./scripts/smoke-import-api.sh`, `./scripts/smoke-vault-push.sh`.

### Import modes and dedupe

- **replace** — wipe that source’s messages, then reload.
- **append** — keep existing rows; skip when `(account_id, source, guid)` already exists.

Cross-source soft-dedupe (exact / near-time matches): [docs/dedupe.md](docs/dedupe.md).  
Database tables: [docs/schema.md](docs/schema.md).

```bash
cargo run --release -- import --source imessage --mode replace --account yourusername
cargo run --release -- dedupe-cross-source --account yourusername
```

### Obsidian export

```bash
cargo run --release -- export-markdown --out /path/to/Obsidian-Message-Vault --account yourusername
```

Enable the `message-vault-bubbles` CSS snippet in Obsidian (from `config/obsidian-message-vault.css`).

### Demo data for maintainers

```bash
cargo run -p demo-seed -- --out demo --seed 42
```

See [`demo/README.md`](demo/README.md).

### csv-ingest CLI details

[`crates/csv-ingest/README.md`](crates/csv-ingest/README.md) — vault-push flags, checkpoints, GUI notes.
