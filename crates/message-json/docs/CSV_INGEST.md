# CSV → vault JSONL (historical)

CSV → vault conversion no longer lives in this repository. Use
[message-exporters](https://github.com/bitrealm-dev/message-exporters)
(`message-exporter`) to turn phone backups into **message-ir JSONL**. Their
`vault-push` tool projects that IR into **vault JSONL** (one JSON object per
line) for Message Vault CLI `ingest` / `import` or `POST /v1/import`
(`application/jsonl`).

## Why CSV still appears in some exporter workflows

CSV can be a plain spreadsheet file (for example `_14075551234.csv`) that a
person opens to fix phones or junk rows before the exporter emits message-ir
JSONL.

Typical pipeline:

```
backup + contacts.csv or .vcf
  → message-exporters / message-exporter (name/phone lookup + reshape)
  → message-ir JSONL (+ attachments)
  → vault-push (projects to vault JSONL) → vault import
```

## Vault import contract

- Files: `*.jsonl` under a staging folder, or a raw / multipart HTTP body.
- Content-Type for raw bodies: `application/jsonl`.
- Multipart: field `jsonl` plus optional `file` parts (relative attachment paths).
- Source id is a validated lowercase slug supplied at import time (not listed in
  vault `config.toml`).

See the root README and [`crates/message-json/README.md`](../README.md) for the
vault wire schema and the message-ir boundary.
