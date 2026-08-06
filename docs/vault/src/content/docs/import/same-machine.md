---
title: Same machine
description: Import a local JSONL directory without the network push tools.
---

If message-ir JSONL already lives on the vault machine, import without `serve` or
`vault-push`. Point `--input` at any directory of `*.jsonl` files (plus relative
attachment paths). Source identity comes from each conversation’s IR
`export.source` — or pass `--source` to force one source for the whole batch.

## Import a directory

```bash
cargo run --release -- import \
  --account yourusername \
  --input /path/to/jsonl-dir \
  --mode replace
```

`import` loads JSONL into SQLite, copies (or transforms) attachments into the
account/source asset trees from `config.toml`, then runs cross-source soft-dedupe
unless `--skip-dedupe`. CLI default mode is **replace** (wipes each distinct
`export.source` found in the input batch).

`--input` also accepts the aliases `--dir`, `--staging-dir`, and `--export-dir`.

Optional flags:

- `--source <slug>` — force one source (ignore IR `export.source`)
- `--contacts contacts.vcf|contacts.csv` — load address book into SQLite
- `--media copy|none|convert|compress` — attachment handling before the
  content-addressed store (`copy` is default; `convert`/`compress` need ffmpeg)

## Several sources in one folder

A single `--input` directory may contain conversations from more than one
`export.source`. Replace mode wipes each of those sources before reload.

To import separate folders one at a time (with soft-dedupe once at the end):

```bash
cargo run --release -- import \
  --account yourusername \
  --input staging/imessage \
  --skip-dedupe

cargo run --release -- import \
  --account yourusername \
  --input staging/go-sms-pro

# or:
cargo run --release -- dedupe-cross-source --account yourusername
```

## Media for the browser

Import-time `--media convert|compress` stores browser-friendly bytes as the
canonical original for that import. For vaults that already imported originals,
generate derived sidecars afterward:

```bash
cargo run --release -- process-assets
```
