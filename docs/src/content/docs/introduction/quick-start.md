---
title: Quick start
description: Try Message Vault in a few minutes — run the demo vault with Docker and connect the desktop app.
---

The fastest way to try Message Vault is the demo vault: a Docker container that comes with a built-in sample dataset. No phone backup needed.

## Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) on Windows or macOS, or [Docker Engine](https://docs.docker.com/engine/install/) on Linux

## Run the demo vault

Open a terminal and run:

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

This starts the vault in demo mode with sample conversations ready to browse. The web interface and the import API share **port 8080**. The `message-vault-data` volume keeps the database between restarts.

## Browse the demo

Open **http://localhost:8080** and sign in with username `demo` and an empty password. You will find sample conversations to search, browse, and explore.

## Connect the desktop app

1. Open the desktop app on your computer.
2. Enter the vault address **http://localhost:8080** and connect (same URL as the browser).

The app and the vault are now linked. When you are ready for your own messages, the desktop app extracts them from a backup and imports them into the vault — all on your machine.

## More details

The [demo vault page](/set-up-the-server/try-the-demo/) covers the sample data and demo mode in more depth. When you are ready for a vault of your own, see [your first personal vault](/set-up-the-server/first-personal-vault/) and [install the desktop app](/introduction/install/).
