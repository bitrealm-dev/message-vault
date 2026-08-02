---
title: CLI reference
description: Cargo subcommands for import, dedupe, contacts, export, demo reset, and serve.
---

Run from the repository root with `cargo run --release -- <command>`.

| Command | Purpose |
|---------|---------|
| `ingest` | Import a JSONL staging folder for one source, then soft-dedupe |
| `import` | Import JSONL only (no automatic cross-source dedupe) |
| `dedupe-cross-source` | Soft-hide the same SMS across sources |
| `import-contacts` | Load an address book into SQLite (**iMazing Contacts CSV** or **VCF**) |
| `vcf-to-contacts` | Convert `.vcf` → vault mirror `contacts.csv` (optional; prefer `import-contacts --contacts`) |
| `export-markdown` | Obsidian bubble markdown export |
| `reset-demo` | Restore the committed demo bundle |
| `serve` | HTTP import API (`[server]` required in config) |

## Shared flags

Most tenant-scoped commands take:

- `--config` (default `config/config.toml`)
- `--account <username|uuid>`

Import commands also accept `--mode replace|append` (default **replace** for
CLI).

## Examples

```bash
cargo run --release -- ingest imessage \
  --account yourusername \
  --staging-dir staging/imessage \
  --mode replace

cargo run --release -- import \
  --source imessage \
  --export-dir staging/imessage \
  --mode replace \
  --account yourusername \
  --contacts path/to/contacts.vcf

cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/imazing-contacts.csv

cargo run --release -- dedupe-cross-source --account yourusername

cargo run --release -- serve
cargo run --release -- reset-demo
```

CLI contact import accepts the same address-book formats as Message Exporters:

- **VCF** (`.vcf` / `.vcard`)
- **iMazing Contacts CSV** with `First Name`, `Last Name`, and at least one
  column whose header contains `Phone` (for example `Mobile Phone`)

Message import does **not** load `data/<account>/contacts.csv` by default; pass
`--contacts` (or run `import-contacts`) when you want names from an address book.
The per-account vault CSV remains a dual-write mirror for web edits / unknown-handle
backfill, not the CLI import format.

Prefer the web **Import VCF** preview when you want to import only message-matched
contacts and map categories to vault labels interactively. The web **Export
contacts CSV** action downloads the vault projection from SQLite.

Helpers: `./scripts/ingest-staging.sh`, `./scripts/import-staging.sh`,
`./scripts/setup-demo.sh`.
