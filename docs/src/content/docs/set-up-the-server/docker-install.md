---
title: Docker install
description: Run the vault server with Docker — the fastest setup, no host toolchain needed.
---

The vault runs as a Docker container. One command starts the server, the import API, and the web interface. No Rust or Node.js toolchain required.

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS
- [Docker Engine](https://docs.docker.com/engine/install/) on Linux (with the Compose v2 plugin)

## Docker Compose

Two compose profiles are available:

| File | Command | Best for |
|---|---|---|
| `compose-dev.yml` (default) | `docker compose up` | Laptop; bind-mounted source, hot reload |
| `compose-release.yml` | `docker compose -f compose-release.yml up --build` | Slimmer runtime image |

A committed `.env` sets `COMPOSE_FILE=compose-dev.yml`, so bare `docker compose up` uses the dev profile. Override with `-f` or change the variable in `.env`.

### Dev mode

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
docker compose up
```

Bind-mounts the repo root so code changes reload at runtime. Includes a SQLite browser on port 8081 (localhost only).

### Release mode

```bash
docker compose -f compose-release.yml up --build
```

Builds a slimmer production-shaped image from your checkout.

## Docker run (single command)

The published image is `mbeisser1/message-vault:latest`:

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Port 3000 is the web interface. Port 8080 is the import API the desktop app connects to. The `message-vault-data` volume persists the database across restarts.

### Personal vault

For your own messages, use `VAULT_MODE=personal`:

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=personal \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Then create an account through the web interface and generate an import token. See [your first personal vault](/set-up-the-server/first-personal-vault/).

## What is in the container

| Component | Purpose |
|---|---|
| Vault server | Import API (port 8080) and web interface (port 3000) |
| SQLite | Database for messages, contacts, and settings |
| FFmpeg | Media conversion for browser playback |
| Demo dataset | 390 conversations, ~627k messages (demo mode only) |
