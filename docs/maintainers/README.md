# Maintainer documentation

This directory contains implementation and release documentation for contributors. End-user guides live in the [Starlight source](../src/content/docs/) and are published at <https://bitrealm.io/>.

## Start here

- [Setup, build, and contributing](https://bitrealm.io/vault/developer/contributing/) — prerequisites, workspace build, helpers, tests, and PR rules. Short pointer in the repo: [`CONTRIBUTING.md`](../../CONTRIBUTING.md).
- [Release Process](https://bitrealm.io/vault/developer/contributing/#release-process) — version bump, changelog, git tag, Docker image, and Tauri installers.
- [Code signing (Windows / macOS)](signing.md) — certificates, GitHub secrets, and gated release workflow steps.
- [GUI design](../../crates/message-vault-io-gui/gui.md) — Desktop app architecture: Tauri v2 shell with React + Vite frontend, Tauri commands wrapping exporter crates, and progress events.
- [Converter capabilities](https://bitrealm.io/vault/developer/formats/) — supported inputs, known source limitations, and format mapping pages.

## Architecture and output formats

- [Vault Design](https://bitrealm.io/vault/developer/vault-design/) — tree, binaries, C4 views, and session sequences. PlantUML sources: [`architecture/puml/`](architecture/puml/). Mermaid sources: [`architecture/sequence_diagram.md`](architecture/sequence_diagram.md).
- [Message Transfer](https://bitrealm.io/vault/developer/message-transfer/) — exporter → JSONL → import, supported vs rescue commands.
- [Shared message model](architecture/message-ir.md) — `ConversationDocument`, common fields, source-specific data, and output projectors.
- [Mail archive format](https://bitrealm.io/vault/developer/formats/mail-archive/) — EML/MBOX layout and `X-ME-*` metadata.
- [SMS Backup & Restore XML output](https://bitrealm.io/vault/developer/formats/sms-backup-restore-xml/) — Android-compatible `smses.xml` output and mapping rules.

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
