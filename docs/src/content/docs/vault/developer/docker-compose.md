---
title: Operator Docker
description: Build a release-shaped vault image from a git checkout, or run the published image with Compose.
---

Day-to-day work from a clone uses [`./scripts/run-vault-dev.sh`](https://github.com/bitrealm-io/message-vault/blob/main/scripts/run-vault-dev.sh) on the host — see [Run from source](/vault/developer/run-from-source/). This page is Docker: a checkout that should look like a shipped install, or the published Hub image without compiling.

To try the published image without cloning, save [`docker/compose.yml`](https://github.com/bitrealm-io/message-vault/blob/main/docker/compose.yml) as described on [Try the vault](/vault/user/get-started/try-the-vault/). That sample pulls `bitrealm/message-vault`.

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS
- [Docker Engine](https://docs.docker.com/engine/install/) on Linux with the Compose v2 plugin

## Checkout image

Run from the **repository root**. `docker/compose.release.yml` builds `docker/Dockerfile` from this tree (website baked in). After a code change, rebuild.

```bash title="Build the checkout image"
docker compose -f docker/compose.release.yml up --build
```

A committed `.env` sets `COMPOSE_FILE=docker/compose.release.yml`, so bare `docker compose up --build` from a clone uses that file. Override with `-f`.

Blank vault (no demo seed): `DEMO_DATA=false docker compose -f docker/compose.release.yml up --build`.

Serves the vault on port **8080**. Do not run this stack and the published-image Compose file at once. They share that port.

## Published image

The image `bitrealm/message-vault:latest` is the User Guide path. Sample data seeds on first boot when `DEMO_DATA=true` and the volume is empty. Creating a second account on that instance is [Use your own messages](/vault/user/get-started/your-own-messages/). Changing `DEMO_DATA` on an existing volume does not add or remove accounts.

## What is in the container

| Component | Purpose |
|---|---|
| Vault server | Website and `/v1/*` API on port **8080** |
| SQLite | Database for messages, contacts, and settings |
| FFmpeg | Media conversion for browser playback |
| Demo dataset | Sample conversations when `DEMO_DATA=true` and the volume is new |

## Related

- [Run from source](/vault/developer/run-from-source/)
- [HTTP API](/vault/developer/reference/api/)
- [Config and accounts](/vault/developer/reference/config-and-accounts/)
