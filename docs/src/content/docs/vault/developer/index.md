---
title: Developer
description: Set up a development environment, then vault design, message transfer, CLI tools, the HTTP API, formats, and instance internals.
---

These pages are for people who compile the vault, run Compose, or call the HTTP API. The [User Guide](/vault/user/) is the try-it and import path.

- [Contributing](/vault/developer/contributing/) — environment setup, tests, pull requests
- **Architecture** — [Vault Design](/vault/developer/vault-design/), [Message Transfer](/vault/developer/message-transfer/), [Common message](/vault/developer/architecture/common-message/)
- [Operator Docker](/vault/developer/docker-compose/) — release-shaped Compose from a checkout
- [Command-line tools](/vault/developer/reference/cli/) — exporter binaries, `vault-push`, `vault-pull`
- [HTTP API](/vault/developer/reference/api/) — tokens and import flow; [route reference](/vault/developer/reference/http/)
- [Rust crate docs](/vault/developer/rustdoc/) — `cargo doc` HTML for workspace crates (not the HTTP route list)
- [Formats](/vault/developer/formats/) — converter capabilities and mapping tables
- [Config and accounts](/vault/developer/reference/config-and-accounts/) — `config.toml` and local accounts
- [Database](/vault/developer/reference/database/)
- [Export structure](/vault/developer/reference/export-structure/) — JSONL folder layout
- [CSV columns](/vault/developer/reference/csv-columns/)
- [Server CLI](/vault/developer/reference/server-cli/)
