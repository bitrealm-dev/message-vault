---
title: Try the vault
description: Run the vault with Docker, sign in as demo in the browser, and look around before preparing a backup.
---

Getting a phone backup takes work. The sample account answers whether search, contacts, and media in the vault are useful before that work starts.

Connect with the **website**. Importing your own messages later needs the [desktop app](/get-started/install-the-desktop-app/) — more on that when you [use your own messages](/get-started/your-own-messages/).

Already sure you want your own data? Skip to [Use your own messages](/get-started/your-own-messages/).

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS, or [Docker Engine](https://docs.docker.com/engine/install/) on Linux

## Start the vault

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e DEMO_DATA=true \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

This starts the vault and, on an **empty** volume, seeds sample conversations. The website and the import API share **port 8080**. The `message-vault-data` volume keeps the database between restarts.

`DEMO_DATA=true` only seeds when the volume is new. Changing the variable later does not add or remove accounts.

## Sign in as demo

Open **http://localhost:8080**. Sign in with username `demo` and an empty password.

`demo` is a read-only sample account on this vault. Browse **Conversations**, open a thread, try [search](/how-to/search/).

## After you have looked around

Sign out. Create your own account on **this same vault** — not a second container. Continue at [Use your own messages](/get-started/your-own-messages/).

## Build from source instead

Compiling the vault and the desktop app: [Run from source](/developer/run-from-source/). Compose from a git checkout: [Operator Docker](/developer/docker-compose/).
