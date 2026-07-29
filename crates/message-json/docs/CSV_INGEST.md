# CSV → vault JSONL (historical)

CSV → vault conversion no longer lives in this repository. Use
[message-exporters](https://github.com/bitrealm-dev/message-exporters) to turn
phone backups into **vault JSONL** (one JSON object per line) that Message Vault
imports via CLI `ingest` / `import` or `POST /v1/import` (`application/jsonl`).

## Why CSV still appears in some exporter workflows

CSV can be a plain spreadsheet file (for example `_14075551234.csv`) that a
person opens to fix phones or junk rows before the exporter emits vault JSONL.

Typical pipeline:

```
backup + contacts.csv or .vcf
  → message-exporters (name/phone lookup + reshape)
  → vault JSONL (+ attachments)
  → vault import / vault-push
```

## Vault import contract

- Files: `*.jsonl` under a staging folder, or a raw / multipart HTTP body.
- Content-Type for raw bodies: `application/jsonl`.
- Multipart: field `jsonl` plus optional `file` parts (relative attachment paths).
- Source id is a validated lowercase slug supplied at import time (not listed in
  vault `config.toml`).

See the root README and [`crates/message-json/README.md`](../README.md) for the
vault wire schema.
