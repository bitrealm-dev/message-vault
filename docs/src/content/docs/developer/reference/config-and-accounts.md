---
title: Config and accounts
description: Instance config.toml, per-account data paths, and multi-tenant accounts.
---

## Instance config

Copy [`config/config.toml.example`](https://github.com/bitrealm-dev/message-vault/blob/main/config/config.toml.example)
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

### Server asset limits

`[server]` also accepts optional upload limits. Both keys default to sensible values for most installs:

| Key | Default | Description |
|-----|---------|-------------|
| `asset_max_bytes` | `536870912` (512 MiB) | Maximum size for one attachment — a single `PUT /v1/assets/{sha256}` body or the total declared bytes for a multipart upload. Must be greater than 0. |
| `asset_part_size` | `67108864` (64 MiB) | Chunk size advertised to clients for multipart uploads. Must not exceed `asset_max_bytes`. Keep under ~100 MiB for Cloudflare-proxied setups. |

The environment variable `VAULT_ASSET_PART_SIZE` can override the advertised part size at runtime (clamped to `1..=asset_part_size`). Set it before starting the server. This is a test and operations knob — use `config.toml` for normal configuration.

Web env overrides (optional): `VAULT_DB`, `VAULT_DATA_DIR`.

## Per-account asset files

Created on first use if missing:

- `data/<account_id>/<source_id>/assets/`
- `data/<account_id>/<source_id>/assets_converted/`

## Accounts

Rows are scoped by `account_id` in a shared `vault.db`.

- Web login uses username + password (Argon2id hash in `accounts.password_hash`).
  Accounts may opt into no password (`password_hash` NULL); empty password is
  accepted only for those accounts.
- Each account can create named **API tokens** for `vault-push` / `vault-pull`
  (stored hashed; shown once when created). GUI sessions use a separate rotating
  token.
- New accounts start with browsing edits enabled.
- Demo seed identity: username `demo` (`crates/vault/demo-seed/config/seed.toml`), always
  no-password and read-only by default. Self-hosted sign-in stays username `demo` and
  an empty password. On a hosted vault with the guest pool on, that account is the
  clone template; visitors use **Try it** instead of signing in as `demo`.

See [Settings](/user/how-to/settings/).

## Guest demo pool

Off by default so a local Compose vault still uses the shared `demo` user. When
`GUEST_DEMO_POOL` is true, **Try it** assigns a private sample account from a
ready pool. Password login as `demo` is rejected. The copy lasts
`GUEST_SESSION_SECS` (24 hours by default). Guests cannot import or export a
backup or create API tokens.

Set these as environment variables (Compose). A hosted image that turns the
pool on should keep `DEMO_DATA=true` so first boot still creates the template
`demo` account.

| Key | Default | Meaning |
|---|---|---|
| `GUEST_DEMO_POOL` | `false` | Enable the pool, `try_demo` on `/v1/auth/mode`, and reject password login as `demo` |
| `GUEST_POOL_MIN` | `2` | Unused ready floor |
| `GUEST_POOL_MAX` | `20` | Unused ready ceiling |
| `GUEST_SESSION_SECS` | `86400` | Guest session lifetime |

**Try it** is limited in two ways. Each visitor internet address
(`CF-Connecting-IP` on a host behind Cloudflare) may accept 60 Try it
calls per minute. The whole server may accept 2000 per minute. People
who share one building address share the 60. If that header is missing,
those calls share one pile of 60. Cloudflare bot rules can sit in front;
this server does not configure them. Login stays 20 attempts per username
per minute.
