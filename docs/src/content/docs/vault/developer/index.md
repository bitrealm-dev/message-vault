---
title: Developer
description: Environment setup, releasing, vault design, message transfer, Docker, CLI tools, the HTTP API, formats, and instance internals.
---

These pages are for people who compile the vault, run Compose, or call the HTTP API. The [User Guide](/vault/user/) is the try-it and import path.

- [Contributing](/vault/developer/contributing/) — environment setup, tests, pull requests
- [Release](/vault/developer/release/) — how product versions ship
- **Architecture** — [Vault Design](/vault/developer/vault-design/), [Message Transfer](/vault/developer/message-transfer/), [Common message](/vault/developer/architecture/common-message/)
- [Docker](/vault/developer/docker/) — build a release-shaped image from a checkout, or run the published image
- [Command-line tools](/vault/developer/reference/cli/) — exporter binaries, `vault-push`, `vault-pull`
- [HTTP API](/vault/developer/reference/api/) — tokens and import flow; [route reference](/vault/developer/rustdoc/http/)
- [Rust crate docs](/vault/developer/rustdoc/) — `cargo doc` HTML for workspace crates (not the HTTP route list)
- [Formats](/vault/developer/formats/) — converter capabilities and mapping tables
- [Config and accounts](/vault/developer/reference/config-and-accounts/) — `config.toml` and local accounts
- [Database](/vault/developer/reference/database/)
- [Export structure](/vault/developer/reference/export-structure/) — JSONL folder layout
- [CSV columns](/vault/developer/reference/csv-columns/)
- [Server CLI](/vault/developer/reference/server-cli/)
