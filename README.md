# Message Vault

Message Vault keeps your text-message history in one place so you can browse it
in a local website—search conversations, open photos and attachments, and see
iPhone and Android backups side by side.

You run it on a computer you control. Your messages stay in a SQLite database on
that machine; they are not uploaded to a cloud service by this project.

Turning a phone backup into files the vault understands is done by a separate
project, [message-exporters](https://bitrealm-dev.github.io/message-exporters/).
This repository is the vault itself: storage, import, and the browser UI.

## Docs

Read the full guide (install, demo, import, browse UI, CLI, HTTP API):

**https://bitrealm-dev.github.io/message-vault-rs/**

Source Markdown lives in [`docs/src/content/docs/`](docs/src/content/docs/) and
is published with Astro Starlight.

Contributor setup and troubleshooting:
[`docs/maintainers/development.md`](docs/maintainers/development.md).

## Quick start (demo)

```bash
./scripts/setup-demo.sh
cd web && npm ci && npm run process-assets && npm run dev
```

Open <http://localhost:3000/login> and sign in as **`demo`** (or create another
account).

Windows PowerShell steps and full prerequisites are in the
[developer setup guide](docs/maintainers/development.md).

## Import your own messages

1. Copy `config/config.toml.example` → `config/config.toml` (ensure `[server]`).
2. `cargo run --release -- serve` and `cd web && npm ci && npm run dev`.
3. Create an account; copy the Import API token from **Settings → Account**.
4. Export JSONL with [Message Exporters](https://bitrealm-dev.github.io/message-exporters/),
   then push via the Vault tab or `vault-push`.

Details: [First personal import](https://bitrealm-dev.github.io/message-vault-rs/get-started/first-personal-import/).

## Repository layout

```text
src/                # CLI binary: import, ingest, serve, export, demo reset
crates/
  message-json/     # vault JSONL schemas
  demo-seed/        # regenerate committed demo data
demo/               # committed demo bundle
config/             # config.toml.example and CSV/VCF examples
scripts/            # setup-demo, ingest helpers, smoke tests
web/                # Next.js UI
docs/               # Starlight site + maintainers/
```
