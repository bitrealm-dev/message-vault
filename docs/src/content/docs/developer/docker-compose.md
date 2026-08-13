---
title: Operator Docker
description: Run the vault from a git checkout with Docker Compose — bind-mounts, hot reload, and the SQLite browser.
---

Use these Compose files when working on the vault from this repository. To try the published image without cloning, save [`docker/compose.yml`](https://github.com/bitrealm-dev/message-vault/blob/main/docker/compose.yml) as described on [Try the vault](/get-started/try-the-vault/). That sample pulls `bitrealm/message-vault`; it is not used from a clone.

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS
- [Docker Engine](https://docs.docker.com/engine/install/) on Linux with the Compose v2 plugin

## Profiles

Run these from the **repository root**.

| File | Command | Best for |
|---|---|---|
| `docker/compose.dev.yml` (default) | `docker compose up` | Laptop; bind-mounted source, hot reload |
| `docker/compose.release.yml` | `docker compose -f docker/compose.release.yml up --build` | Slimmer runtime image from the checkout |

A committed `.env` sets `COMPOSE_FILE=docker/compose.dev.yml`, so bare `docker compose up` uses the dev profile. Override with `-f` or change the variable in `.env`.

### Dev mode

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
docker compose up
```

Bind-mounts the repo root so code changes reload at runtime. Serves the vault on port **8080**. Includes a SQLite browser on port **8081** (localhost only).

### Release mode

```bash
docker compose -f docker/compose.release.yml up --build
```

Builds a slimmer production-shaped image from the checkout. Serves the vault on port **8080**.

Do not run both compose files at once. They share the default ports.

## Published image

The image `bitrealm/message-vault:latest` is the User Guide path. Sample data seeds on first boot when `DEMO_DATA=true` and the volume is empty. Creating a second account on that instance is [Use your own messages](/get-started/your-own-messages/). Changing `DEMO_DATA` on an existing volume does not add or remove accounts.

## What is in the container

| Component | Purpose |
|---|---|
| Vault server | Website and `/v1/*` API on port **8080** |
| SQLite | Database for messages, contacts, and settings |
| FFmpeg | Media conversion for browser playback |
| Demo dataset | Sample conversations when `DEMO_DATA=true` and the volume is new |

## Related

- [Run from source](/developer/run-from-source/)
- [HTTP API](/reference/api/)
- [Config and accounts](/reference/config-and-accounts/)
