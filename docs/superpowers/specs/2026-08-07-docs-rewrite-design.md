# Docs Rewrite: Unified Guidebook

## Context

The existing documentation was written when the project was two separate repos
(`message-vault-rs` and `message-vault-io`). It references outdated repo names,
a Next.js web UI that no longer exists, and uses developer-centric jargon
("message-ir JSONL," "FormatSink," "exporter crate") throughout user-facing
pages. There is no single guidebook voice.

The rewrite produces a guidebook-style user documentation site that leads a
reader from "I have messages on my phone" to "I can browse and search them," with
a clean Reference section for the CLI/power-user audience.

## Architecture

The product has two pieces:

- **The vault** (vault server) — Docker container running the REST API and
  SQLite database. Provides endpoints the desktop app uses to import, browse,
  search, and manage messages.
- **The desktop app** — the GUI for extracting messages from backups,
  converting formats, managing contacts, and pushing data to the vault.

These are one repo, one project, one docs site. The old names (`message-vault-rs`,
`message-vault-io`) and the Next.js legacy web UI are purged from every page.

## Audience

Two audiences, two tones:

1. **User-facing pages** (the guidebook) — Light-technical plain language.
   Introduce JSONL as "JSON Lines" (link to jsonlines.org). Never use internal
   jargon. The pipeline is an implementation detail; users see "pick a backup,
   pick a format, get messages."

2. **Reference section** — Precision for the self-hosted Docker and CLI audience.
   Real binary names (`vault-push`, `message-vault-server`), API docs, config,
   database schema.

## Voice and naming rules

| Call it... | Never... |
|---|---|
| the vault (or vault server) | `message-vault-rs`, the backend, Next.js |
| the desktop app | `message-vault-io`, the GUI, the Tauri app |
| JSONL (JSON Lines) | message-ir, message-ir JSONL |

- Project name: "Message Vault" (two words, title case)
- Repo URL: `github.com/bitrealm-io/message-vault` (one URL, no old repo links)
- User pages: "extract" or "export"; never "parse," "run the exporter,"
  "FormatSink"
- Reference: real binary names; link to Reference from user pages for CLI
  details

## Information architecture

```
Introduction
  What is Message Vault?
  Why manual backups?                     ← NEW
  Quick start (Docker + demo in 5 min)
  Install the desktop app
  Glossary                                ← NEW

Prepare your backups
  Back up an iPhone / iPad
  Back up iPhone WhatsApp
  Back up Android SMS & MMS
  Back up Android WhatsApp
  Rescue imports (other formats)

Set up the server
  Docker install
  First personal vault
  Try the demo
  Updating

Use the desktop app
  Extract messages
  Convert between formats
  Work with contacts
  Import into the vault
  Media and privacy options
  Supported output formats

Browse the vault
  Navigation and sources
  Search
  Groups
  Contacts and labels
  Trash and undo
  Settings

Reference
  CLI tools (vault-push, vault-pull, exporters)
  API
  Config and accounts
  Server CLI
  Database schema
  Export structure (JSONL format spec)
  CSV columns
```

## Key new pages

### Why manual backups?

Explains that Apple, Google, and WhatsApp do not expose API access to messages.
Messages live only in local databases and backups on the device. The desktop app
reads those local files directly — no login, no account, no third-party server
in the middle. This is a platform limitation, not a Message Vault limitation.
Manual backup steps are the only path today.

### Glossary

JSONL, JSON, CSV, EML, MBOX, XML (smses.xml), VCF, E.164, PDU, Docker, SQLite.
One or two sentences each. No internal project jargon.

### Per-format backup guides

Under Prepare your backups. Each page covers:
- What you need (file, folder, or backup)
- Where to get it (link to Apple/Google official docs or trusted third-party
  tools)
- What the desktop app can do with it
- Any known limitations

Replaces the current scattered `/apple/`, `/android/`, and `/other-app-exports/`
sections.

### CLI tools (Reference)

Lists each binary with purpose and man-page-style docs. Serves the power-user
audience through the server CLI.

## Migration rules

- Purge every occurrence of `message-vault-rs`, `message-vault-io`,
  `message-vault-rs/`, `message-vault-io/` in docs (23+ references)
- Purge all mentions of Next.js — it no longer exists
- Purge "message-ir" from all user-facing pages; keep it only in the Reference
  export structure page as a parenthetical "(also called message-ir internally)"
  if needed for CLI-compatibility context
- Replace repo links with `github.com/bitrealm-io/message-vault`
- Rewrite every page in the new voice — not find-and-replace, full rewrite
- Existing screenshots may need updating if they show old branding or app names

## Verification

1. `cd docs && npm run build` — build passes with no broken links
2. Manual read-through of every page for voice consistency
3. No hits for: `message-vault-rs`, `message-vault-io` (outside of historical
   note if one exists), `message-ir`, `Next.js`, `FormatSink`, `exporter crate`,
   `owner phone set`
4. Every external link resolves
5. Glossary covers every technical term used in user-facing pages
