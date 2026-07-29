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
| `import-contacts` | Reload contacts CSV into SQLite |
| `vcf-to-contacts` | Convert `.vcf` → `contacts.csv` (CATEGORIES + FN tags → `label_N`; optional `exclude.csv`) |
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
  --account yourusername
cargo run --release -- dedupe-cross-source --account yourusername

cargo run --release -- vcf-to-contacts \
  --vcf path/to/contacts.vcf \
  --account yourusername

cargo run --release -- serve
cargo run --release -- reset-demo
```

`vcf-to-contacts` is a one-shot file conversion. It does not store the VCF in
the database. Prefer the web **Import VCF** preview when you want to import
only message-matched contacts and map categories to vault labels interactively.
The web **Export contacts CSV** action downloads the vault projection from
SQLite (phones, names, inactive, all labels).

Helpers: `./scripts/ingest-staging.sh`, `./scripts/import-staging.sh`,
`./scripts/setup-demo.sh`.
