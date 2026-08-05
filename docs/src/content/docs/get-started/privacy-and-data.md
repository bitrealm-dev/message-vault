---
title: Privacy and data
description: Where your messages live, what is stored, and how the vault keeps your data under your control.
---

Message Vault runs on a computer you control. Your messages are not uploaded to a cloud service by this project.

## Where your data lives

| Data | Storage location |
|------|-----------------|
| Messages, conversations, contacts, labels, preferences | `data/vault.db` (SQLite) |
| Original attachments (photos, videos, etc.) | `data/<account>/<source>/assets/` |
| Browser-converted media | `data/<account>/<source>/assets_converted/` |
| Config (paths, bind address) | `config/config.toml` |

Everything lives in directories you configure. There is no telemetry, no analytics, and no phoning home.

## What is stored

**Per message:** sender, recipients, text content, timestamps, service (SMS, iMessage, WhatsApp, RCS), direction (incoming/outgoing), attachment metadata, tapbacks and reactions, and source-specific fields.

**Per attachment:** the file bytes, MIME type, original filename, and a SHA-256 content hash.

**Per account:** username, a hashed password (Argon2id), API tokens (SHA-256 hashed — shown once in plain text at creation time, never stored in the clear), email addresses, phone numbers, and import history.

**Not stored:** plain-text passwords after hashing; plain-text API tokens after the one-time display; encryption keys for message content (the database is not encrypted at rest).

## Network exposure

The vault exposes two ports:

| Port | Service | Purpose |
|------|---------|---------|
| `3000` | Next.js web UI | Browsing, search, settings, account management |
| `8080` | Rust import/export API | `vault-push` uploads, `vault-pull` downloads |

By default, both bind to `127.0.0.1` (localhost only). They are not reachable from other machines on your network unless you change the bind address in `config/config.toml`.

If you expose the vault beyond `localhost`, put it behind a reverse proxy with TLS (nginx, Caddy, Cloudflare Tunnel). The vault itself does not implement TLS. See the [Docker guide](/get-started/docker/) for an example nginx configuration.

## Authentication

Two modes, controlled by the `VAULT_AUTH` environment variable:

- **`local`** (default): username and password, hashed with Argon2id. API tokens are generated per-account from **Settings → Access** in the web UI. All `v1/` API endpoints require a `Bearer` token.
- **`hanko`**: Hanko passwordless authentication. Used by the hosted Bitrealm service. Requires a Hanko project and API URL.

There is no host-wide admin account. Each account is independent. API tokens grant access to one account's data only — a token for account A cannot read account B's messages.

## Trash and deletion

Messages moved to the trash are soft-deleted (a `trashed_at` timestamp in the database). They are hidden from search and browse but recoverable. The vault does not currently have a destructive delete API — there is no way to permanently remove messages through the HTTP API. This is an intentional V1 design choice. Trash is emptied from the web UI under **Browse → Trash**.

## Encryption at rest

The vault does not encrypt the SQLite database or asset files. If full-disk encryption is important to you, use your operating system's built-in encryption (FileVault on macOS, BitLocker on Windows, LUKS on Linux) or place the `data/` directory on an encrypted volume.

## Exporters — your files, your machine

The [Message Exporters](https://bitrealm-dev.github.io/message-exporters/) desktop app runs entirely on your machine. It reads from local files, writes to local files, and never sends data over the network unless you explicitly use the Vault tab to push a JSONL export into Message Vault. The obfuscation feature can replace real names, numbers, and text with pseudonyms before writing — see the [media and privacy](https://bitrealm-dev.github.io/message-exporters/work-with-exports/media-and-privacy/) guide.

For the vault's detailed data model, see the [database schema reference](/reference/database/).
