---
title: Same machine
description: Import a local JSONL staging folder without the network push tools.
---

If the export folder already lives on the vault machine, import without
`serve` or `vault-push`. Create a staging directory yourself (for example
`staging/<source>/`); nothing under that path is committed in the repo.

## One source

```bash
cargo run --release -- import go-sms-pro \
  --account yourusername \
  --dir staging/go-sms-pro
```

`import` loads message-ir JSONL and runs cross-source soft-dedupe afterward
(unless `--skip-dedupe`). CLI default mode is **replace**.

`--dir` also accepts the aliases `--staging-dir` and `--export-dir`.

## Several sources

Run `import` once per source. Use `--skip-dedupe` on every source except the
last so soft-dedupe runs once:

```bash
cargo run --release -- import imessage \
  --account yourusername \
  --dir staging/imessage \
  --skip-dedupe

cargo run --release -- import go-sms-pro \
  --account yourusername \
  --dir staging/go-sms-pro
```

Or import each source with `--skip-dedupe`, then:

```bash
cargo run --release -- dedupe-cross-source --account yourusername
```

## Media for the browser

```bash
cargo run --release -- process-assets
```
