# Currently Under Heavy Development

# Message Vault

Message Vault keeps your text-message history in one place so you can browse it
in a local website—search conversations, open photos and attachments, and see
iPhone and Android backups side by side.

You run it on a computer you control. Your messages stay in a SQLite database on
that machine; they are not uploaded to a cloud service by this project.

Turning a phone backup into a **message-ir** JSONL export is done by a separate
project, [message-exporters](https://bitrealm-dev.github.io/message-exporters/)
(desktop binary `message-exporter`). Push those message-ir JSONL exports into
this repository—the vault itself: storage, import, and the browser UI.

## Docs

Read the full guide (install, demo, import, browse UI, CLI, HTTP API):

**https://bitrealm-dev.github.io/message-vault-rs/**

Source Markdown lives in [`docs/src/content/docs/`](docs/src/content/docs/) and
is published with Astro Starlight.

Contributor setup and troubleshooting:
[`docs/maintainers/development.md`](docs/maintainers/development.md).

## Quick start (demo)

**Native:**

```bash
./scripts/setup-demo.sh
cd web && npm ci && npm run dev
```

**Docker** (no host Rust/Node toolchain; pulls/builds from your checkout):

```bash
docker compose up
```

(Uses `compose-dev.yml` via `COMPOSE_FILE` in `.env`. Also:
`compose-release.yml`.)

Open <http://localhost:3000/login> and sign in as username **`demo`** with an
empty password (or create another account).

Windows PowerShell steps and full prerequisites are in the
[developer setup guide](docs/maintainers/development.md). Docker modes:
[Docker guide](https://bitrealm-dev.github.io/message-vault-rs/get-started/docker/).
Bitrealm production VPS (Cloudflare + Hanko + Ansible) lives in the private
`message-vault-ops` repo.

## Import your own messages

1. Copy `config/config.toml.example` → `config/config.toml` (ensure `[server]`).
2. `cargo run --release -- serve` and `cd web && npm ci && npm run dev`.
3. Create an account; generate a Vault Import API token under **Settings → Access**
   (copy it from the one-time dialog).
4. Export message-ir JSONL with
   [Message Exporters](https://bitrealm-dev.github.io/message-exporters/)
   (`message-exporter`), then push via the Vault tab or `cli/vault-push`.

Details: [First personal import](https://bitrealm-dev.github.io/message-vault-rs/get-started/first-personal-import/).

## Repository layout

```text
src/                # CLI binary: import, serve, demo reset
crates/
  demo-seed/        # regenerate committed demo data (message-ir JSONL)
demo/               # committed demo bundle
config/             # config.toml.example, config.docker.toml, CSV/VCF examples
scripts/            # setup-demo, docker entrypoints, smoke tests
web/                # Next.js UI
docs/               # Starlight site + maintainers/
Dockerfile.dev       # compose-dev.yml (toolchain + bind mount)
Dockerfile.release   # compose-release.yml (slim multi-stage image)
compose-dev.yml
compose-release.yml
```


