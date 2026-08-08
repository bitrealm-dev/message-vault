# Maintainer documentation

This directory contains implementation and release documentation for contributors. End-user guides live in the [Starlight source](../src/content/docs/) and are published at <https://bitrealm.dev/vault/>.

## Start here

- [Setup, build, and contributing](../../CONTRIBUTING.md) — prerequisites, workspace build, helpers, tests, and PR rules.
- [Develop and publish releases](developing.md) — release workflow, documentation build, and local preview.
- [Code signing (Windows / macOS)](signing.md) — certificates, GitHub secrets, and gated release workflow steps.
- [GUI design](gui.md) — Desktop app architecture: Tauri v2 shell with React + Vite frontend, Tauri commands wrapping exporter crates, and progress events.
- [Exporter capability matrix](exporter-matrix.md) — supported inputs, known source limitations, and links to crate-specific technical documents.

## Architecture and output formats

- [Shared message model](architecture/message-ir.md) — `ConversationDocument`, common fields, source-specific data, and output projectors.
- [Mail archive format](formats/mail-archive.md) — EML/MBOX layout and `X-ME-*` metadata.
- [SMS Backup & Restore XML output](formats/sms-backup-restore-xml.md) — Android-compatible `smses.xml` output and mapping rules.

## Crate-specific documentation

Shared libraries live under `crates/message/<name>/` (`message-ir`, `contacts`, `media`, `go-sms-mms`, …). Exporter crates live under `crates/exporters/<name>/` and keep their command reference in `docs/MANPAGE.md`. Other binary crates use `crates/<name>/docs/MANPAGE.md`. Importers may also provide:

- `INPUT_FORMAT.md` for facts about the vendor or source format;
- `IMPORT_MAPPING.md` for source fields, skip rules, and conversion into the shared message model;
- `DESIGN.md` for parser algorithms, validation history, and implementation decisions.

The Starlight build generates its command-line reference from the crate manpages. Edit the crate file, then run:

```bash
cd docs
npm run sync:cli
npm run check
npm run build
```

Generated pages under `docs/src/content/docs/reference/cli/` are not edited directly.
