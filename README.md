# Message Vault

Extract messages from phone backups, import them into a local vault, and browse them in a website you control.

## What it is

Message Vault has two parts that run on a machine you control:

- **The vault** — a Docker container with a REST API and a SQLite database. It stores your messages and serves them through a website in your browser.
- **The desktop app** — a program that extracts messages from Apple and Android phone backups, converts them between formats, and imports them into the vault.

There is no cloud account. Messages are not uploaded to a Message Vault service. The vault you run has a local login (the demo user, or an account you create).

## Who it is for

People who have phone backups and want to extract, convert, and browse those messages locally.

## Getting started

**Desktop app:** Download the archive for your operating system from the latest [Release](https://github.com/bitrealm-dev/message-vault/releases). Extract it, keep every file in the same folder, and run the app. Install steps: [Install the desktop app](https://bitrealm.dev/get-started/install-the-desktop-app/).

**Demo vault:**

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e DEMO_DATA=true \
  -v message-vault-data:/app/data \
  bitrealm/message-vault:latest
```

Open **http://localhost:8080** and sign in with username `demo` and an empty password. The website and the API share that origin. More: [Try the vault](https://bitrealm.dev/get-started/try-the-vault/).

## What you can do

- **Extract** Apple Messages (`chat.db` or an iPhone backup), Android SMS/MMS from SMS Backup & Restore XML, and WhatsApp. GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+ are limited rescue imports for files you already have.
- **Convert** an existing Message Vault folder between JSON Lines, JSON, CSV, EML, MBOX, and XML.
- **Import, browse, and export** using the desktop app and the vault.

Full guide: **https://bitrealm.dev/**

Converter and mapping details: [Formats](https://bitrealm.dev/formats/) (Developer).

## From source

Build and run instructions: [CONTRIBUTING.md](CONTRIBUTING.md).

## Get involved

- [CONTRIBUTING.md](CONTRIBUTING.md) — setup, tests, and pull-request rules
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)

## License

This project is licensed under the GNU Affero General Public License v3.0 — see [LICENSE](LICENSE). `imessage-ir-exporter` still depends on `imessage-database` (GPL-3.0-or-later); the combined binaries are AGPL-3.0.
