---
title: Config and accounts
description: Instance config.toml, per-account data paths, and multi-tenant accounts.
---

## Instance config

Copy [`config/config.toml.example`](https://github.com/bitrealm-dev/message-vault-rs/blob/main/config/config.toml.example)
to `config/config.toml` (gitignored).

```toml
[paths]
db = "data/vault.db"
data_dir = "data"
assets_dir = "assets"
assets_converted_dir = "assets_converted"

[server]
bind = "127.0.0.1:8080"
```

- Paths resolve relative to the repo root (parent of `config/`).
- `[server]` is required for `serve`. The demo config comments it out.
- Source names are **not** listed in TOML — each import registers its own
  source slug for that account under `data/<account_id>/<source_id>/`.

Web env overrides (optional): `VAULT_DB`, `VAULT_DATA_DIR`.

## Per-account files

Created on first use if missing:

- `data/<account_id>/contacts.csv`
- `data/<account_id>/exclude.csv`
- `data/<account_id>/<source_id>/assets/`
- `data/<account_id>/<source_id>/assets_converted/`

## Accounts

Rows are scoped by `account_id` in a shared `vault.db`.

- Web login uses username + password (Argon2id hash in `accounts.password_hash`).
  Accounts may opt into no password (`password_hash` NULL); empty password is
  accepted only for those accounts.
- Each account can generate a Vault Import API token for `serve` / vault-push
  (stored hashed; shown once when created).
- New accounts start with browsing edits enabled.
- Demo seed identity: username `demo` (`demo/config/seed.toml`), always
  no-password and read-only by default.

See [Settings](/browse/settings/).
