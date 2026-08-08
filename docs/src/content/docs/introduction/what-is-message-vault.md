---
title: What is Message Vault?
description: Extract messages from phone backups, import them into a local vault, and browse them in an interface you control.
---

Message Vault helps you extract messages from your phone backups and browse them in an interface you control. Your messages stay on your own machine — nothing is uploaded to a cloud service, and no account is required.

## Two pieces, one project

Message Vault has two parts that work together:

- **The vault** — a small server that runs in Docker. It stores your messages in a SQLite database on your computer and serves them to you through a website you open in your browser.
- **The desktop app** — a program that runs on your computer. It extracts messages from phone backups, converts them between formats, and imports them into the vault.

The two talk to each other over a local connection on your machine. You can also use the desktop app on its own — extract a backup and export the messages as files without ever starting the vault.

## What you can do

- **Extract messages** from iPhone and Android backups, and from WhatsApp
- **Import** the extracted messages into the vault
- **Browse and search** conversations, contacts, and media in your browser
- **Convert** exports between formats — JSONL (JSON Lines), JSON, CSV, EML, MBOX, XML, and VCF contacts
- **Keep your media** — photos and videos from conversations are saved alongside the messages

## Your data stays local

- Messages are stored in a SQLite database on your machine, not in the cloud
- The desktop app reads your backup files directly on your computer
- No third-party connections, no telemetry, no account to create
- Everything runs on your machine: extract, store, browse, export

## Where to go next

- [Why manual backups?](/introduction/why-manual-backups/) — why you provide backups yourself
- [Quick start](/introduction/quick-start/) — run the demo vault with Docker in a few minutes
- [Install the desktop app](/introduction/install/)
- [Glossary](/introduction/glossary/) — plain-language definitions of the formats and terms you will meet
