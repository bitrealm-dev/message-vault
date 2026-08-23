---
title: Vault server CLI
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
| `dump-openapi` | Write the OpenAPI JSON to stdout or `--output`. Does not open the database |

## Shared flags

Most tenant-scoped commands take:

- `--config` (default `config/config.toml`)
- `--account <username|uuid>`
- `--db <path>` — override the database path from config (available on `import`, `dedupe-cross-source`, `import-contacts`, `process-assets`)

## `import`

```bash title="import"
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
- **`--contacts`** (alias `--contacts-csv`): load VCF or vCard CSV into SQLite (same as `import-contacts`).
- **`--overwrite-contacts`**: reload contacts even if the contacts table is non-empty. The default behavior skips with a hint when contacts are already loaded.
- **`--assets-dir <dir>`**: override the per-account originals store. Only meaningful with `--source` (fixed-source mode).
- **`--media`**: `copy` (default), `none` (skip attachments), `convert`, or `compress` (ffmpeg required for convert/compress). Rewrites happen before files land in `assets/`.
- Soft-dedupe runs after import unless `--skip-dedupe`.

HTTP `serve` import is unchanged and still takes `source` as a query parameter.

## `process-assets`

Create browser-friendly previews under `assets_converted/` (JPEG / MP4 / MP3) while leaving the originals in `assets/` unchanged. Requires ffmpeg.

**Attachment records vs media files:** an *attachment* is a database row that links a message to media. An *asset* (media file) is the unique on-disk blob under `assets/`, named by content hash. Many attachment records can point at the same file.

Summary counters mean:

| Counter | Meaning |
|---------|---------|
| `converted_for_web` | New preview written under `assets_converted/` |
| `left_as_is` | Already converted, non-media (PDF, etc.), or small JPEG that needs no rewrite |
| `conversion_failures` | ffmpeg (or related) error for that file |

```bash title="process-assets"
cargo run --release -- process-assets [--force] [--dry-run] \
  [--skip-image] [--skip-video] [--skip-audio] \
  [--source <id>] [--db <path>]
```

| Flag | Description |
|------|-------------|
| `--force` | Re-convert even if a converted asset already exists |
| `--dry-run` | Convert and log, but do not write to disk |
| `--skip-image` | Skip image conversion |
| `--skip-video` | Skip video conversion |
| `--skip-audio` | Skip audio conversion |
| `--source <id>` | Process a single source only (omit for all sources) |
| `--db <path>` | Override the database path from config |

## `reset-demo`

Regenerate the demo bundle, clear demo account data, import, and process assets.

```bash title="reset-demo"
cargo run --release -- reset-demo \
  [--bundle crates/vault/demo-seed] \
  [--config config/config.toml]
```

| Flag | Description |
|------|-------------|
| `--bundle` | Bundle directory (default `crates/vault/demo-seed`) |
| `--config` | Config path (default `config/config.toml` — **overwrites** it with the demo config, which comments out `[server]`) |

The final summary groups counters so they are easier to read:

- **Imported into vault** — conversations, messages, attachment records (links), tapbacks, contacts
- **Media files on disk** — unique blobs stored under `assets/`, plus attachment paths whose files were missing
- **Duplicate detection** — *fingerprints set* is one fingerprint per message (used later to match the same SMS across sources). It is **not** a count of duplicates found
- **Browser previews** — how many media files were converted for the web, left unchanged, or failed conversion

After running `reset-demo`, copy `config/config.toml.example` back (or uncomment `[server]`) before running `serve`.

## `dump-openapi`

Write the HTTP OpenAPI document. Used to refresh `docs/src/assets/openapi.json`.

```bash title="dump-openapi"
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

No `--config`. The committed file must match this output; `cargo test -p message-vault-server` checks that.

## Examples

```bash title="Examples"
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

CLI contact import accepts the same address-book formats as the desktop extract/import flows:

- **VCF** (`.vcf` / `.vcard`)
- **vCard CSV** (VCF exported as CSV) with `First Name`, `Last Name`, and at
  least one column whose header contains `Phone` (for example `Mobile Phone`)

Pass `--contacts` (or run `import-contacts`) when you want names from an
external address book. Contact files are never stored under the account data
directory.

Prefer the web **Import VCF** preview when you want to import only message-matched
contacts and map categories to vault labels interactively. The web **Export
contacts CSV** action downloads the vault projection from SQLite.

Helper (deprecated, non-Docker): `./scripts/deprecated/setup-demo.sh`.
