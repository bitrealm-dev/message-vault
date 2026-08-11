---
title: Updating
description: How to upgrade the vault and the desktop app to a new version.
---

## Updating the vault

### Docker (recommended)

Stop the container, pull the new image, and restart:

```bash
docker stop message-vault
docker rm message-vault
docker pull mbeisser1/message-vault:latest
docker run -d --name message-vault \
  -p 8080:8080 \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

The named volume keeps your database and assets. The new container picks them up on restart. `DEMO_DATA` only controls seeding when the container starts with an empty volume, so it does not need to be set during an upgrade. Database schema upgrades apply automatically when the server starts — nothing to run by hand.

### Compose

```bash
git pull
docker compose up --build -d
```

Compose rebuilds the image from your updated checkout. The bind-mounted volume keeps your data.

### Before updating

Back up your data first. Copy the database and `data/` directory to a safe location. Updates are designed to be safe, but having a backup means you can always go back.

## Updating the desktop app

Download the new [release archive](https://github.com/bitrealm-dev/message-vault/releases) for your platform. Extract it to the same folder you use now — keep all the files together. The desktop app, helpers, and licenses must stay in one folder.

If you use the import feature, the `.vault-import-state.jsonl` journal in your export directory is forward-compatible. Re-runs resume where the previous version left off.

## Compatibility

The vault and the desktop app share a common data format (JSONL, schema version 3). New versions may add fields but will not remove or rename existing ones. An export made with an older desktop app imports into a newer vault without changes, and vice versa.

The Docker image tag `latest` always points to the most recent release. For a specific version, use `mbeisser1/message-vault:v0.3.0` or the current version from the [Releases](https://github.com/bitrealm-dev/message-vault/releases) page.

## Data portability

Everything is open formats and local files:

- The vault stores messages in a single SQLite database and plain asset files
- The desktop app writes JSONL, JSON, CSV, EML, MBOX, and XML

You can move your vault to another machine by copying the database, the `data/` directory, and `config/config.toml`. You can open an export in any text editor. Nothing is locked in.
