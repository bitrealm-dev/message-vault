---
title: Same machine
description: Import a local JSONL staging folder with ingest without the network push tools.
---

If the export folder already lives on the vault machine, import without
`serve` or `vault-push`. Create a staging directory yourself (for example
`staging/<source>/`); nothing under that path is committed in the repo.

## One source

```bash
cargo run --release -- ingest go-sms-pro \
  --account yourusername \
  --staging-dir staging/go-sms-pro
```

`ingest` imports JSONL and runs cross-source soft-dedupe afterward (unless
`--skip-dedupe`). CLI default mode is **replace**.

## Several sources

Run `ingest` once per source (default staging path: `staging/<source_id>/`).
Use `--skip-dedupe` on every source except the last so soft-dedupe runs once:

```bash
cargo run --release -- ingest imessage \
  --account yourusername \
  --staging-dir staging/imessage \
  --skip-dedupe

cargo run --release -- ingest go-sms-pro \
  --account yourusername \
  --staging-dir staging/go-sms-pro
```

## Import without auto-dedupe

```bash
cargo run --release -- import \
  --source imessage \
  --export-dir staging/imessage \
  --mode replace \
  --account yourusername

cargo run --release -- dedupe-cross-source --account yourusername
```

## Media for the browser

```bash
cargo run --release -- process-assets
```
