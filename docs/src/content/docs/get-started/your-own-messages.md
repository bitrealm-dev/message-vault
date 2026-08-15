---
title: Use your own messages
description: Create an account on the same vault you used for demo, then import a real backup.
---

A personal archive is a **second account** on the vault that is already running. Do not delete the Docker volume or start a second container to “go personal.” Sign out of `demo`, register, then import.

If the vault is not running yet, start it with the command on [Try the vault](/get-started/try-the-vault/), then come back here instead of signing in as `demo`.

## 1. Create an account

Open **http://localhost:8080** (or sign out if you are still `demo`). Create an account with a username and password. That pair is the local vault login.

On first sign-in the site asks for a **display name** and **handles** (phone numbers or emails). Those handles mark messages you sent when you import.

## 2. Prepare a backup

Follow [Prepare a backup](/prepare-a-backup/) for your phone and app.

## 3. Install the desktop app

Import is not in the browser. [Install the desktop app](/get-started/install-the-desktop-app/), then sign in with the same server URL (`http://127.0.0.1:8080`) and the account you just created.

## 4. Import and browse

[Import from a backup](/import-from-a-backup/), then [browse your messages](/browse-your-messages/).
