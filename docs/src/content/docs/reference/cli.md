---
title: CLI reference
description: Cargo subcommands for import, dedupe, contacts, demo reset, and serve.
---

Run from the repository root with `cargo run --release -- <command>`.

| Command | Purpose |
|---------|---------|
| `import` | Import a JSONL directory (source from IR `export.source` unless `--source`), then soft-dedupe |
| `dedupe-cross-source` | Soft-hide the same SMS across sources |
| `import-contacts` | Load an address book into SQLite (**VCF** or **vCard CSV**) |
| `process-assets` | Generate and register converted media assets |
| `reset-demo` | Regenerate demo bundle, clear demo account data, import, and process assets |
| `serve` | HTTP import API (`[server]` required in config) |

## Shared flags

Most tenant-scoped commands take:

- `--config` (default `config/config.toml`)
- `--account <username|uuid>`

## `import`

```bash
cargo run --release -- import \
  --account yourusername \
  --input /path/to/jsonl-dir \
  [--source imessage] \
  [--contacts contacts.vcf] \
  [--media copy|none|convert|compress] \
  [--mode replace|append] \
  [--skip-dedupe] \
  [--window-secs 2]
```

- **`--input`**: directory of `*.jsonl` files (aliases: `--dir`, `--staging-dir`, `--export-dir`). Attachment paths resolve relative to that directory.
- **Source**: from each conversation’s IR `export.source` by default. A directory may mix sources; `--mode replace` wipes each source found in the batch. Pass `--source` to force one source for every conversation.
- **`--contacts`**: load VCF or vCard CSV into SQLite (same as `import-contacts`).
- **`--media`**: `copy` (default), `none` (skip attachments), `convert`, or `compress` (ffmpeg required for convert/compress). Rewrites happen before files land in `assets/`.
- Soft-dedupe runs after import unless `--skip-dedupe`.

HTTP `serve` import is unchanged and still takes `source` as a query parameter.

## Examples

```bash
cargo run --release -- import \
  --account yourusername \
  --input staging/imessage \
  --mode replace

cargo run --release -- import \
  --account yourusername \
  --input staging/imessage \
  --contacts path/to/contacts.vcf \
  --media copy \
  --skip-dedupe

cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/Contacts.csv

cargo run --release -- dedupe-cross-source --account yourusername

cargo run --release -- serve
cargo run --release -- reset-demo
cargo run --release -- process-assets
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
