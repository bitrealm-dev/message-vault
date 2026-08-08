# NAME

whatsapp-exporter - convert WhatsApp DB/backup (via wtsexporter) via common message to JSON/CSV/EML/MBOX/JSONL/XML

# SYNOPSIS

```text
whatsapp-exporter --output <DIR> --platform android|ios
    [--format json|jsonl|csv|eml|mbox|xml]
    [--input <PATH>] [--json <PATH>]
    [--key <KEY|PATH>] [--backup <PATH>] [--wa <PATH>] [--media <PATH>] [--db <PATH>]
    [--business]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Shells out to KnugiHK **wtsexporter** ([WhatsApp-Chat-Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter) ≥ 0.13), then maps its JSON into the common message and projects JSON (default) or another `--format`. Conversation stems use the `__whatsapp` suffix.

Extraction runs in a temp directory under `--output` (removed afterward). `--platform` is required unless `--json` (convert-only, no extract).

Install the helper with `pip install 'whatsapp-chat-exporter[android_backup,crypt15]'`, use the release-bundled binary beside this tool, or set `WTSEXPORTER`. Prefer this over iMazing WhatsApp CSV when you have the native DB/backup.

# OPTIONS

**--output** *DIR*
: Destination for packaging output, `attachments/`, and `wtsexporter_result.json`.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message (`json` default).

**--platform** *android|ios*
: Target platform for wtsexporter (`-a` / `-i`). Required unless `--json`.

**--input** *PATH*
: Search root for relative defaults (`msgstore.db`, `wa.db`, `WhatsApp/`, …). Default: process cwd. Not the extract cwd.

**--json** *PATH*
: Skip wtsexporter; convert an existing `result.json`.

**--key** *KEY|PATH*
: Crypt key file path or crypt15 hex string (forwarded as `-k`).

**--backup** *PATH*
: Encrypted Android backup or iOS backup path (forwarded as `-b`). Required for iOS.

**--wa** *PATH*
: Contacts DB `wa.db` / `ContactsV2.sqlite` (forwarded as `-w`).

**--media** *PATH*
: WhatsApp media folder (forwarded as `-m`).

**--db** *PATH*
: Explicit message database (forwarded as `-d`).

**--business**
: Use WhatsApp Business package defaults.

**--start-date** / **--end-date** *YYYY-MM-DD*
: Local date filter (inclusive start, exclusive end).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

# EXIT STATUS

Non-zero if `wtsexporter` is missing/fails, JSON is missing, convert fails, or media tools fail entirely.

# FILES

**Output**
: Output in the selected format with `__whatsapp` in conversation stems. JSON, JSONL, CSV, EML, and MBOX are organized per conversation; XML writes one `smses.xml` backup. The output can also contain `attachments/` and `wtsexporter_result.json`. Scratch `wtsexporter-*` directories exist during extraction and are removed afterward.

**Upstream**
: Requires `wtsexporter` on `PATH`, beside this binary, in `MESSAGE_VAULT_IO_BIN`, or via `WTSEXPORTER`.

# ENVIRONMENT

**WTSEXPORTER**
: Absolute path to the `wtsexporter` binary.

**MESSAGE_VAULT_IO_BIN**
: Directory searched for `wtsexporter` / `wtsexporter.exe`.

**TQDM_DISABLE**
: Set to `1` by this tool when spawning wtsexporter (progress bars off).

**PATH**
: Needs `ffmpeg` / `ffprobe` for `convert` / `compress`; also used to find `wtsexporter`.

# EXAMPLES

```bash
# Android crypt15
whatsapp-exporter \
  --platform android \
  --key /path/to/key-or-hex \
  --backup msgstore.db.crypt15 \
  --output ./staging/whatsapp

# iOS backup
whatsapp-exporter \
  --platform ios \
  --backup ~/Library/Application\ Support/MobileSync/Backup/DEVICE_ID \
  --output ./staging/whatsapp

# Convert-only
whatsapp-exporter \
  --json /path/to/result.json \
  --output ./staging/whatsapp
```

# NOTES

Supported exporter. Android needs an already extracted / decryptable database or crypt backup; iOS uses a WhatsApp-capable iPhone backup path.

# SEE ALSO

- [Android WhatsApp user guide](https://bitrealm-dev.github.io/message-vault/android/whatsapp/)
- [Apple WhatsApp user guide](https://bitrealm-dev.github.io/message-vault/apple/whatsapp/)
