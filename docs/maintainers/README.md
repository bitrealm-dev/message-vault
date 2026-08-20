# Maintainer documentation

This directory contains implementation and release documentation for contributors. End-user guides live in the [Starlight source](../src/content/docs/) and are published at <https://bitrealm.dev/>.

## Start here

- [Setup, build, and contributing](../../CONTRIBUTING.md) — prerequisites, workspace build, helpers, tests, and PR rules.
- [Develop and publish releases](developing.md) — release workflow, documentation build, and local preview.
- [Code signing (Windows / macOS)](signing.md) — certificates, GitHub secrets, and gated release workflow steps.
- [GUI design](gui.md) — Desktop app architecture: Tauri v2 shell with React + Vite frontend, Tauri commands wrapping exporter crates, and progress events.
- [Converter capabilities](https://bitrealm.dev/vault/developer/formats/) — supported inputs, known source limitations, and format mapping pages.

## Architecture and output formats

- [C4 diagrams](architecture/puml/) — from-source system, container, and deployment views. Preview the `.puml` files with current C4-PlantUML.
- [Developer session sequences](architecture/sequence_diagram.md) — four Mermaid diagrams: start the vault, sign in, import a backup, export from the vault. Desktop App (Tauri), Vite `:5173`, and Vault `:8080` are participants.
- [Shared message model](architecture/message-ir.md) — `ConversationDocument`, common fields, source-specific data, and output projectors.
- [Mail archive format](https://bitrealm.dev/vault/developer/formats/mail-archive/) — EML/MBOX layout and `X-ME-*` metadata.
- [SMS Backup & Restore XML output](https://bitrealm.dev/vault/developer/formats/sms-backup-restore-xml/) — Android-compatible `smses.xml` output and mapping rules.

## Crate-specific documentation

Shared libraries live under `crates/libs/` (`message-ir`, `contacts`, `media`, `go-sms-mms`, …). Exporter crates live under `crates/exporters/<name>/`. Command-line reference pages are edited on the docs site:

`docs/src/content/docs/vault/developer/reference/cli/<command>.md`

Input-format and mapping pages are edited under `docs/src/content/docs/vault/developer/formats/`.

After changing those pages:

```bash
cd docs
npm run check
npm run build
```
