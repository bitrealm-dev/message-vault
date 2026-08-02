---
title: First personal import
description: Configure the vault, create an account, and import your first JSONL export.
---

## 1. Configure the vault machine

```bash
cp config/config.toml.example config/config.toml
```

If you previously ran the demo, `config/config.toml` may still be the demo
copy. Prefer re-copying from `config.toml.example` so `[server]` is enabled.

## 2. Start the import API and web UI

Terminal 1:

```bash
cargo build --workspace --release
cargo run --release -- serve
```

Terminal 2:

```bash
cd web && npm ci && npm run dev
```

## 3. Create an account and copy your token

Open <http://localhost:3000>, create an account, then go to
**Settings → Access → Vault Import**, generate an **API token**, and copy it
from the dialog (it is shown only once). That token identifies your account.
You do **not** need an account UUID.

New accounts start with browsing edits enabled. You can turn on view-only mode
under **Settings → Access** without blocking imports.

Keep `serve` running while you import remotely.

## 4. Export from Message Exporters

On the machine that has your backup files, use
[Message Exporters](https://bitrealm-dev.github.io/message-exporters/)
(`message-exporter` from a
[release archive](https://bitrealm-dev.github.io/message-exporters/get-started/install/),
or build from source). Prefer a **message-ir JSONL** export folder (plus
`attachments/`).

## 5. Push into the vault

See [Import from Message Exporters](/import/from-message-exporters/) for the
Vault tab (`message-exporter`) and `cli/vault-push`. Push projects message-ir
into vault JSONL for the import API. Then refresh the website and
[browse](/browse/navigation-and-sources/).

If the export folder already lives on the vault machine, you can skip the
network push and use [same-machine ingest](/import/same-machine/) instead.
