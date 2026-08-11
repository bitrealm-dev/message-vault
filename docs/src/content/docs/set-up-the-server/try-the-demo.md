---
title: Try the demo
description: Browse a sample vault with synthetic conversations — no phone backup required.
---

The demo vault comes with a built-in dataset: synthetic messages across hundreds of conversations from two backup sources (`imessage` and `sms-backup-restore`). A small set of threads appears in both so you can try the Sources panel and cross-source dedupe. It is a quick way to try search, browse, and media features without extracting a real backup.

## Run the demo

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e DEMO_DATA=true \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

The container seeds the demo dataset on first start. Subsequent restarts reuse the existing database. Changing `DEMO_DATA` does not seed or remove data in an existing volume. To refresh the dataset in place, run the server CLI's [`reset-demo`](/reference/server-cli/#reset-demo) command.

## Browse the demo

Open **http://localhost:8080** and sign in as username `demo` with an empty password.

## Reset the demo

To wipe the demo and start fresh:

```bash
docker rm -f message-vault
docker volume rm message-vault-data
docker run -d --name message-vault \
  -p 8080:8080 \
  -e DEMO_DATA=true \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Removing the container and volume deletes the database. The new container seeds a fresh demo on start.

## Connect the desktop app

You can also connect the desktop app to the demo vault. Enter `http://localhost:8080` as the server URL and sign in as `demo` with an empty password. The demo lets you try browse and import workflows before working with your own messages.

## Running from source

If you prefer to build from source instead of using the published image, see the [Docker install](/set-up-the-server/docker-install/) page for Compose instructions or the repository [README](https://github.com/bitrealm-dev/message-vault) for native build steps.
