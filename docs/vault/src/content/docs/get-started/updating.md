---
title: Updating
description: How to upgrade Message Vault and Message Exporters to a new version.
---

## Updating Message Vault

### Docker (recommended)

Stop the container, pull the new image, and restart:

```bash
docker stop message-vault
docker rm message-vault
docker pull mbeisser1/message-vault:latest
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=personal \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

The named volume (`message-vault-data`) keeps your database and assets. The new container picks them up on restart. Database schema upgrades apply automatically when the server starts — nothing to run by hand.

### Compose (dev)

```bash
git pull
docker compose up --build -d
```

Compose rebuilds the image from your updated checkout. The bind-mounted volume keeps your data.

### Native (build from source)

```bash
git pull
cargo build --release
cd web && npm ci && npm run build
```

Restart the `serve` process and the Next.js dev server afterward. Schema upgrades apply on the first request after starting the new binary.

### Before updating

Back up your data first. See [Backup and restore](/get-started/backup-and-restore/) for the full procedure. Updates are designed to be safe, but having a backup means you can always go back.

## Updating Message Exporters

Exports are plain files — there is nothing to migrate. Download the new release archive for your platform, extract it to the same permanent folder you use now, and keep `lib/` and `cli/` next to the app binary.

If you use `vault-push`, the `.vault-import-state.jsonl` journal file in your export directory is forward-compatible. Re-runs resume where the previous version left off.

## Compatibility between the tools

Message Vault and Message Exporters use a shared data format (message-ir, schema version 3). New versions of either tool may add fields, but they will not remove or rename existing ones. An export made with an older version of the desktop app imports into a newer vault without changes, and vice versa.

The Docker image tag `latest` always points to the most recent release. For a specific version, use `mbeisser1/message-vault:v0.3.0` (or whatever the current version is — check the [Releases](https://github.com/bitrealm-dev/message-vault-rs/releases) page).

## Data portability

Both products produce and consume open formats:

- Message Vault stores everything in a single SQLite database and plain asset files.
- Message Exporters writes JSONL, JSON, CSV, EML, MBOX, and XML.

You can move your vault to another machine by copying the database, the `data/` directory, and `config/config.toml`. You can open an export in any text editor. Nothing is locked into either tool.
