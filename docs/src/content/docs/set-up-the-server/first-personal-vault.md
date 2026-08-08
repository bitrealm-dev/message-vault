---
title: Your first personal vault
description: Set up a vault for your own messages — create an account, get an import token, and push in your first export.
---

A personal vault stores your messages behind an account you create. No demo data — it starts empty and you import your own exports.

## 1. Start the vault in personal mode

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=personal \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

If you are using Compose, set `VAULT_MODE=personal` in your environment or `.env` file before running `docker compose up`.

## 2. Create your account

Open **http://localhost:3000** and create an account. This is your vault login — the username and password you choose here.

## 3. Generate an import token

1. In the web interface, go to **Settings → Access**
2. Choose **Generate token**
3. Copy the token — it is shown only once

This token authenticates the desktop app when it pushes messages into the vault.

## 4. Connect the desktop app

Open the desktop app on your computer. Go to the import view and enter:

- **Vault address**: `http://localhost:8080`
- **Token**: the import token from step 3

## 5. Extract and import

Now you are ready to import messages:

1. [Prepare a backup](/prepare-your-backups/iphone-ipad/) from your phone
2. Use the desktop app to extract messages from the backup
3. Import the result into the vault

The desktop app sends messages to the vault over your local connection — nothing leaves your machine.

## Next steps

- [Extract messages](/use-the-desktop-app/extract-messages/)
- [Import into the vault](/use-the-desktop-app/import-into-vault/)
- [Docker install details](/set-up-the-server/docker-install/)
