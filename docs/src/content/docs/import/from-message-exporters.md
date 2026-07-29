---
title: From Message Exporters
description: Push a JSONL export folder into Message Vault with the Vault tab or vault-push CLI.
---

Prefer **Message Exporters** for remote import. Export with **JSONL** in the
Message tab, then import that folder through the Vault tab or `vault-push`.

Full exporter docs: <https://bitrealm-dev.github.io/message-exporters/>

## Prerequisites

- Vault `serve` running with `[server]` enabled in `config/config.toml`
- Web account created; **Import API token** copied from Settings → Account
- A JSONL export folder (one `*.jsonl` per conversation, plus `attachments/`)

## Desktop app (Vault tab)

In the [message-exporters](https://github.com/bitrealm-dev/message-exporters)
repo:

```bash
cargo run -p message-exporters-gui --release
```

Open the **Vault** tab and fill in:

- URL (for example `http://127.0.0.1:8080`)
- Username
- Vault key (Import API token)
- Input directory (your JSONL export folder)

## CLI (`vault-push`)

```bash
cargo run -p message-vault-client --bin vault-push --features cli --release -- \
  --input ./path/to/your-jsonl-export \
  --url http://vault-host:8080 \
  --username yourusername \
  --key "$VAULT_KEY"
```

The token identifies your account; you do not need an account UUID.

## After import

Refresh the Message Vault website. Use **Message Sources** to view one archive
or **Combined**. Optionally run media conversion:

```bash
cd web && npm run process-assets
```

See also [HTTP import API](/import/http-api/) and
[import modes and dedupe](/import/modes-and-dedupe/).
