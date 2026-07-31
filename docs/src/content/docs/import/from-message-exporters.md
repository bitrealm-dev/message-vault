---
title: From Message Exporters
description: Push a message-ir JSONL export folder into Message Vault with the Vault tab or vault-push CLI.
---

Prefer **Message Exporters** for remote import. Export a **message-ir JSONL**
folder (one `*.jsonl` per conversation, plus `attachments/`), then push that
folder through the Vault tab or `vault-push`. Push **projects** message-ir into
[vault JSONL](/reference/vault-jsonl/) before `POST /v1/import`.

Full exporter docs: <https://bitrealm-dev.github.io/message-exporters/>

Release layout (`message-exporter`, `lib/`, `cli/`):
[Install Message Exporters](https://bitrealm-dev.github.io/message-exporters/get-started/install/).

## Prerequisites

- Vault `serve` running with `[server]` enabled in `config/config.toml`
- Web account created; **Import API token** copied from Settings → Account
- A message-ir JSONL export folder (one `*.jsonl` per conversation, plus
  `attachments/`)

## Desktop app (Vault tab)

Prefer a [release archive](https://github.com/bitrealm-dev/message-exporters/releases).
Keep the extracted tree together (`message-exporter`, `lib/`, `cli/`,
`licenses/`), then run **`message-exporter`** and open the **Vault** tab.

From source in the
[message-exporters](https://github.com/bitrealm-dev/message-exporters) repo:

```bash
cargo run --release -p message-exporter-gui
```

Fill in:

- URL (for example `http://127.0.0.1:8080`)
- Username
- Vault key (Import API token)
- Input directory (your message-ir JSONL export folder)

## CLI (`vault-push`)

From a release archive, use the binary under `cli/`:

```bash
./cli/vault-push \
  --input ./path/to/your-jsonl-export \
  --url http://vault-host:8080 \
  --username yourusername \
  --key "$VAULT_KEY"
```

From source:

```bash
cargo run --release -p vault-push --features cli -- \
  --input ./path/to/your-jsonl-export \
  --url http://vault-host:8080 \
  --username yourusername \
  --key "$VAULT_KEY"
```

The token identifies your account; you do not need an account UUID.

`vault-push` reads message-ir JSONL, uploads attachments by SHA-256, and sends
vault JSONL batches to the import API. See
[Vault JSONL](/reference/vault-jsonl/) for the wire the vault accepts.

## After import

Refresh the Message Vault website. Use **Message Sources** to view one archive
or **Combined**. Optionally run media conversion:

```bash
cd web && npm run process-assets
```

See also [HTTP import API](/import/http-api/) and
[import modes and dedupe](/import/modes-and-dedupe/).
