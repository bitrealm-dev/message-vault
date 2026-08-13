---
title: Operator Docker
description: Run the vault from a git checkout with Docker Compose — bind-mounts, hot reload, and the SQLite browser.
---

Use Compose when working on the vault from this repository. To try the published image without cloning, see [Try the vault](/get-started/try-the-vault/).

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS
- [Docker Engine](https://docs.docker.com/engine/install/) on Linux with the Compose v2 plugin

## Profiles

| File | Command | Best for |
|---|---|---|
| `compose-dev.yml` (default) | `docker compose up` | Laptop; bind-mounted source, hot reload |
| `compose-release.yml` | `docker compose -f compose-release.yml up --build` | Slimmer runtime image from the checkout |

A committed `.env` sets `COMPOSE_FILE=compose-dev.yml`, so bare `docker compose up` uses the dev profile. Override with `-f` or change the variable in `.env`.

### Dev mode

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
docker compose up
```

Bind-mounts the repo root so code changes reload at runtime. Serves the vault on port **8080**. Includes a SQLite browser on port **8081** (localhost only).

### Release mode

```bash
docker compose -f compose-release.yml up --build
```

Builds a slimmer production-shaped image from the checkout. Serves the vault on port **8080**.

Do not run both compose files at once. They share the default ports and the volume name.

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
