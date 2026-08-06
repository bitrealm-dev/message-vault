---
title: Quick start
description: Get Message Vault running in under five minutes with Docker.
---

The fastest way to run Message Vault is with the published Docker image. It bundles the vault server, the web UI, FFmpeg for media conversion, and a demo dataset — one container, two ports.

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) or Docker Engine with Compose v2.

## Run the demo

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Open **http://localhost:3000/login** and sign in as username `demo` with an empty password. The demo dataset has about 627,000 synthetic messages across 390 conversations — enough to try search, browse, and media features.

Port `8080` is the import API. You do not need it for the demo, but the container must expose it for the health check.

The `message-vault-data` volume persists the SQLite database and assets between restarts. To wipe the demo and start fresh, remove the volume:

```bash
docker rm -f message-vault
docker volume rm message-vault-data
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

## Run with your own messages

For a personal vault, use `VAULT_MODE=personal` and create an account:

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=personal \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

1. Open **http://localhost:3000/login** and create an account.
2. Go to **Settings → Access** and generate a **Vault Import API token**. Copy it — it is shown only once.
3. Use the [Message Exporters](https://bitrealm-dev.github.io/message-exporters/) desktop app or `vault-push` CLI to push a JSONL export into the vault. Point it at `http://127.0.0.1:8080` with the token from step 2.

See [First personal import](/get-started/first-personal-import/) for a walkthrough of the full import flow.

## What is inside the container

| Component | Purpose |
|-----------|---------|
| `message-vault-rs` binary | Import API (`:8080`) and CLI tools |
| Next.js web UI | Browse, search, and settings (`:3000`) |
| FFmpeg | Media conversion for browser playback |
| Demo dataset | 390 conversations, ~627k messages (demo mode only) |

## Next steps

- [Install from source or Compose](/get-started/install/) — all installation options including building from source.
- [Docker guide](/get-started/docker/) — Compose dev and release images, volume details, mTLS, and troubleshooting.
- [Try the demo with native tools](/get-started/try-the-demo/) — no Docker, build the demo from source.
- [Updating Message Vault](/get-started/updating/) — how to upgrade when a new version is available.
