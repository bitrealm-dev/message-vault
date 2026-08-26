---
title: Update the vault
description: Upgrade the published Docker image and the desktop app without losing the database volume.
---

## Updating the vault (published image)

Stop the container, pull the new image, and start it again on the **same** named volume:

```bash title="Upgrade with docker run"
docker stop message-vault
docker rm message-vault
docker pull bitrealm/message-vault:latest
docker run -d --name message-vault \
  -p 8080:8080 \
  -v message-vault-data:/app/data \
  bitrealm/message-vault:latest
```

The named volume keeps the database and assets. `DEMO_DATA` only controls seeding when the volume is empty, so it does not need to be set during an upgrade. Schema upgrades apply when the server starts.

Copy the database and `data/` directory somewhere safe before you upgrade.

If you started from [docker/compose.yml](https://github.com/bitrealm-io/message-vault/blob/main/docker/compose.yml), upgrade in that same folder:

```bash title="Upgrade with Compose"
docker compose pull
docker compose up -d
```

A release-shaped image from a git checkout: [Docker](/vault/developer/docker/).

## Updating the desktop app

Download the new installer from [GitHub Releases](https://github.com/bitrealm-io/message-vault/releases) and install it over the current app (`.deb` / AppImage, `.msi`, or `.dmg`).

If Import is in use, the `.vault-import-state.jsonl` journal in the work directory is forward-compatible.

## Compatibility

The vault and the desktop app share JSONL schema version 3. New versions may add fields but will not remove or rename existing ones.

The Docker tag `latest` points at the most recent release. For a specific version, use `bitrealm/message-vault:0.8.1` (no `v` prefix) or a tag from [Releases](https://github.com/bitrealm-io/message-vault/releases).
