---
title: Update the vault
description: Upgrade the published Docker image and the desktop app without losing the database volume.
---

## Updating the vault (published image)

Stop the container, pull the new image, and start it again on the **same** named volume:

```bash
docker stop message-vault
docker rm message-vault
docker pull mbeisser1/message-vault:latest
docker run -d --name message-vault \
  -p 8080:8080 \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

The named volume keeps the database and assets. `DEMO_DATA` only controls seeding when the volume is empty, so it does not need to be set during an upgrade. Schema upgrades apply when the server starts.

Copy the database and `data/` directory somewhere safe before you upgrade.

Compose rebuilds from a git checkout: [Operator Docker](/developer/docker-compose/).

## Updating the desktop app

Download the new [release archive](https://github.com/bitrealm-dev/message-vault/releases) for your platform. Extract it to the same folder you use now — keep all the files together.

If you use Import, the `.vault-import-state.jsonl` journal in the work directory is forward-compatible.

## Compatibility

The vault and the desktop app share JSONL schema version 3. New versions may add fields but will not remove or rename existing ones.

The Docker tag `latest` points at the most recent release. For a specific version, use `mbeisser1/message-vault:v0.7.0` or a tag from [Releases](https://github.com/bitrealm-dev/message-vault/releases).
