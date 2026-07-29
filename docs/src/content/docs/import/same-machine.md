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

```bash
./scripts/ingest-staging.sh --account yourusername \
  --source imessage --source go-sms-pro
```

Default staging path per source: `staging/<source_id>/`.

## Import without auto-dedupe

```bash
cargo run --release -- import \
  --source imessage \
  --export-dir staging/imessage \
  --mode replace \
  --account yourusername

cargo run --release -- dedupe-cross-source --account yourusername
```

Helpers: `./scripts/ingest-staging.sh`, `./scripts/import-staging.sh`.

## Media for the browser

```bash
cd web && npm run process-assets
```
