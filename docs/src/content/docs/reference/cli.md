---
title: CLI reference
description: Cargo subcommands for import, dedupe, contacts, demo reset, and serve.
---

Run from the repository root with `cargo run --release -- <command>`.

| Command | Purpose |
|---------|---------|
| `import` | Import a JSONL staging folder for one source, then soft-dedupe (unless `--skip-dedupe`) |
| `dedupe-cross-source` | Soft-hide the same SMS across sources |
| `import-contacts` | Load an address book into SQLite (**VCF** or **vCard CSV**) |
| `process-assets` | Generate and register converted media assets |
| `reset-demo` | Regenerate demo bundle, clear demo account data, import, and process assets |
| `serve` | HTTP import API (`[server]` required in config) |

## Shared flags

Most tenant-scoped commands take:

- `--config` (default `config/config.toml`)
- `--account <username|uuid>`

Import also accepts `--mode replace|append` (default **replace**).

## Examples

```bash
cargo run --release -- import imessage \
  --account yourusername \
  --dir staging/imessage \
  --mode replace

cargo run --release -- import imessage \
  --account yourusername \
  --dir staging/imessage \
  --contacts path/to/contacts.vcf \
  --skip-dedupe

cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/Contacts.csv

cargo run --release -- dedupe-cross-source --account yourusername

cargo run --release -- serve
cargo run --release -- reset-demo
```

CLI contact import accepts the same address-book formats as Message Exporters:

- **VCF** (`.vcf` / `.vcard`)
- **vCard CSV** (VCF exported as CSV) with `First Name`, `Last Name`, and at
  least one column whose header contains `Phone` (for example `Mobile Phone`)

Pass `--contacts` (or run `import-contacts`) when you want names from an
external address book. Contact files are never stored under the account data
directory.

Prefer the web **Import VCF** preview when you want to import only message-matched
contacts and map categories to vault labels interactively. The web **Export
contacts CSV** action downloads the vault projection from SQLite.

Helper: `./scripts/setup-demo.sh`.
