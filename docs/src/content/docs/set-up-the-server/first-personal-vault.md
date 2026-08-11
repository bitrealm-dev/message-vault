---
title: Your first personal vault
description: Set up a vault for your own messages — create an account, create an API token, and push in your first export.
---

A personal vault stores your messages behind an account you create. No demo data — it starts empty and you import your own exports.

## 1. Start with an empty vault

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e DEMO_DATA=false \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

If you are using Compose, run `DEMO_DATA=false docker compose up` or set `DEMO_DATA=false` in your `.env` file.

## 2. Create your account

Open **http://localhost:8080** and create an account. This is your vault login — the username and password you choose here. On first sign-in you may be asked to add your own handles (phone numbers or emails) so the vault can label messages you sent.

## 3. Create an API token

1. In the web interface (or desktop app after login), open **Settings**
2. Open the **Account** tab
3. Under **API tokens**, create a named token and copy it — it is shown only once

This secret authenticates CLI import (`vault-push`) and export (`vault-pull`). The desktop app **Import** screen uses your signed-in session against the same server URL.

## 4. Connect the desktop app

Open the desktop app. On the login screen, enter:

- **Server URL**: `http://localhost:8080`
- Your vault username and password

After you sign in, use **Import** in the sidebar to push a JSONL export into the vault. For CLI-only workflows, use an API token from Settings → Account.

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
