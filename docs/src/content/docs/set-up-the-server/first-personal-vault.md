---
title: Your first personal vault
description: Set up a vault for your own messages — create an account, get an import token, and push in your first export.
---

A personal vault stores your messages behind an account you create. No demo data — it starts empty and you import your own exports.

## 1. Start the vault in personal mode

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e VAULT_MODE=personal \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

If you are using Compose, set `VAULT_MODE=personal` in your environment or `.env` file before running `docker compose up`.

## 2. Create your account

Open **http://localhost:8080** and create an account. This is your vault login — the username and password you choose here. On first sign-in you may be asked to add your own handles (phone numbers or emails) so the vault can label messages you sent.

## 3. Generate an import token

1. In the web interface (or desktop app after login), open **Settings**
2. Open the **Profile** tab
3. Generate an Import API token and copy it — it is shown only once

This token authenticates CLI import (`vault-push`) and some automated workflows. The desktop app **Import** screen can also sign in with your username and password against the same server URL.

## 4. Connect the desktop app

Open the desktop app. On the login screen, enter:

- **Server URL**: `http://localhost:8080`
- Your vault username and password

After you sign in, use **Import** in the sidebar to push a JSONL export into the vault. For CLI-only workflows, use the Import API token from Settings → Profile.

## 5. Extract and import

Now you are ready to import messages:

1. [Prepare a backup](/prepare-your-backups/iphone-ipad/) from your phone
2. Use the desktop app to extract messages from the backup (or use **Import**, which can extract and push in one flow)
3. Browse conversations in the app or at **http://localhost:8080**

The desktop app sends messages to the vault over your local connection — nothing leaves your machine.

## Next steps

- [Extract messages](/use-the-desktop-app/extract-messages/)
- [Import into the vault](/use-the-desktop-app/import-into-vault/)
- [Export from the vault](/use-the-desktop-app/export-from-vault/)
- [Docker install details](/set-up-the-server/docker-install/)
