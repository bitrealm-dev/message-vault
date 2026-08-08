# vault-pull

Pull messages from a Message Vault Rust `serve` instance into a local
**JSONL** export folder (`*.jsonl` + `attachments/`).

Uses the same Import API Bearer token as `vault-push`. Export HTTP routes are
**read-only** (`GET /v1/export/messages`, `GET /v1/export/messages/count`,
`GET /v1/assets/{sha256}`). The GUI **Query** button prefers the count route
(falls back to paging on older vaults); **Export** pages messages once and
downloads attachments.

```bash
vault-pull \
  --url http://127.0.0.1:8080 \
  --key "$VAULT_KEY" \
  --out ./from-vault \
  --query 'has:attachment' \
  --after 2020-01-01 \
  --before 2021-01-01
```

Also wired from the GUI **Vault Export** screen.
