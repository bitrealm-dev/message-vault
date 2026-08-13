# Maintainer documentation

This directory contains implementation and release documentation for contributors. End-user guides live in the [Starlight source](../src/content/docs/) and are published at <https://bitrealm.dev/>.

## Start here

- [Setup, build, and contributing](../../CONTRIBUTING.md) — prerequisites, workspace build, helpers, tests, and PR rules.
- [Develop and publish releases](developing.md) — release workflow, documentation build, and local preview.
- [Code signing (Windows / macOS)](signing.md) — certificates, GitHub secrets, and gated release workflow steps.
- [GUI design](gui.md) — Desktop app architecture: Tauri v2 shell with React + Vite frontend, Tauri commands wrapping exporter crates, and progress events.
- [Converter capabilities](https://bitrealm.dev/formats/) — supported inputs, known source limitations, and format mapping pages.

## Architecture and output formats

- [Shared message model](architecture/message-ir.md) — `ConversationDocument`, common fields, source-specific data, and output projectors.
- [Mail archive format](https://bitrealm.dev/formats/mail-archive/) — EML/MBOX layout and `X-ME-*` metadata.
- [SMS Backup & Restore XML output](https://bitrealm.dev/formats/sms-backup-restore-xml/) — Android-compatible `smses.xml` output and mapping rules.

## Crate-specific documentation

Shared libraries live under `crates/libs/` (`message-ir`, `contacts`, `media`, `go-sms-mms`, …). Exporter crates live under `crates/exporters/<name>/`. Command-line reference pages are edited on the docs site:

`docs/src/content/docs/reference/cli/<command>.md`

Input-format and mapping pages are edited under `docs/src/content/docs/formats/`.

After changing those pages:

```bash
cd docs
npm run check
npm run build
```
