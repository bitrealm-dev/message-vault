# csv-ingest

Convert per-conversation **CSV** into **vault NDJSON** using Python converters. Rust provides the CLI and a thin dispatcher that shells out to `python3`.

**Deprecated:** `vault-push` / `vault-push-gui` remain for CSV remote push. Prefer Message Exporters JSONL import (`message-vault-client` / GUI Vault tab).

Legacy **`vault-push`** converts a local CSV export directory and uploads **one conversation at a time** to a remote `message-vault-rs serve` (multipart NDJSON + attachments). Legacy **`vault-push-gui`** wraps that CLI.

Ingest contract: [`../message-json/docs/CSV_INGEST.md`](../message-json/docs/CSV_INGEST.md).

## Why CSV exists

CSV is the **human checkpoint**: open it in a spreadsheet, spot bad phones / wrong chats / junk rows, edit or re-export, then convert.

## Convert only

```bash
cargo run -p csv-ingest -- \
  --input ./staging/go-sms-pro \
  --output ./staging/go-sms-pro
```

## Remote push (CLI)

```bash
cargo run -p csv-ingest --bin vault-push --release -- \
  --input ./staging/go-sms-pro \
  --source-id go-sms-pro \
  --url http://vault-host:8080 \
  --token "$VAULT_API_TOKEN" \
  --account <username> \
  --mode append
```

Omit `--account` when using a personal Import API token from web Settings (the token binds the account).

| Flag | Role |
|------|------|
| `--input` | Export dir from message-exporters |
| `--source-id` | Vault source + converter |
| `--url` / `VAULT_URL` | Vault base URL |
| `--token` / `VAULT_API_TOKEN` | Bearer token |
| `--account` | Username or UUID (optional for user tokens; required for admin `server.api_token`) |
| `--mode append` | Default; resume-safe |
| `--mode replace` | Wipe source on first conversation, then append |
| `--continue-on-error` | Keep going after a failed chat |
| `--force-repush` | Ignore checkpoint |
| `--report` / `--log` / `--checkpoint` | Paths (default: under export dir) |

**Per conversation:** multipart `ndjson` + `file` parts. Attachment paths come from each message’s data; vault-push resolves them (export folder, absolute paths, or next to the conversation file) and uploads the files. Missing files → that chat fails (no silent skip). Re-run with `append` **skips** chats listed in `vault-push-done.json`.

Artifacts: `vault-push.log`, `vault-push-report.json`, `vault-push-done.json`.

Stdout progress (for the GUI): `PROGRESS 12/400 ok chat.json …` / `fail` / `skip`.

## GUI wrapper

```bash
cargo run -p csv-ingest --bin vault-push-gui --features gui --release
```

Requires `vault-push` on `PATH` or next to the GUI binary. **Authenticate** (calls `GET /v1/auth/check`) must succeed before **Start import**; the GUI then spawns the CLI and shows a live log.

Fields:

| Field | What it is |
|-------|------------|
| **API token** | Your per-account import token from web **Settings → Import API token**. Identifies your account (no UUID needed). **Not** your web UI password. |
| **Export folder** | Directory that contains your exported conversations. Attachment files are located from paths stored on each message (in the CSV / NDJSON), not by assuming a fixed `attachments/` folder. |

## Converters

| Source id | Targets | Script |
|-----------|---------|--------|
| `imessage` | iMessage Exporter **4.2.0** | [`python/imessage_to_vault.py`](python/imessage_to_vault.py) |
| `sms-backup-plus` | SMS Backup+ **1.5.11** | [`python/sms_backup_plus_to_vault.py`](python/sms_backup_plus_to_vault.py) |
| `sms-backup-restore` | SMS Backup & Restore **10.26.003** | [`python/sms_backup_restore_to_vault.py`](python/sms_backup_restore_to_vault.py) |
| `go-sms-pro` | GO SMS Pro (version TBD) | [`python/go_sms_pro_to_vault.py`](python/go_sms_pro_to_vault.py) |
| `imazing` | iMazing **3.5.5** | [`python/imazing_to_vault.py`](python/imazing_to_vault.py) |

## Samples

- [`samples/csv/`](samples/csv/), [`samples/vault/`](samples/vault/), [`samples/converted/`](samples/converted/)

Smoke: `../../scripts/smoke-vault-push.sh`.

## License

MIT (or match your vault repos).
