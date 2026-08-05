# NAME

sms-backup-restore-exporter - convert SMS Backup & Restore XML via common message to JSON/CSV/EML/MBOX/JSONL/XML

# SYNOPSIS

```text
sms-backup-restore-exporter --input <PATH> --output <DIR> --owner-phone <PHONE>...
    [--format json|jsonl|csv|eml|mbox|xml]
    [--contacts <PATH> | --vcf <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Reads SyncTech **SMS Backup & Restore** XML (`sms-….xml`, targeted **10.26.003**) into a common message per conversation, then writes JSON (default), JSONL, CSV, EML, MBOX, or SyncTech XML (`--format`).

`--input` may be one XML file or a directory of `.xml` backups (combined into one export). Encrypted `.zip` backups must be unlocked and unzipped first; this tool does not open them.

**Owner phone(s) are required** so MMS chat keys, group membership, and senders resolve correctly. For ordinary SMS, sent vs received also comes from the backup `type` field. Pass `--contacts` or `--vcf` to fill blank display names; without either, a warning is printed and names stay unresolved.

MMS media lands under `attachments/` when media copy is enabled; EML/MBOX embed bytes instead. Media convert/compress need `ffmpeg`/`ffprobe`. Call logs are not supported. Drafts, failed, and queued messages are skipped.

# OPTIONS

**--input** *PATH*
: An `sms-*.xml` file, or a directory of `.xml` files.

**--output** *DIR*
: Destination for packaging output and `attachments/`.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging. `json` (default) one file per conversation; `jsonl` lines; `csv` one CSV per conversation; `eml` / `mbox` mail archives; `xml` one `smses.xml`.

**--owner-phone** *PHONE*
: Owner number (E.164 or digits). Repeat for multiple. Required.

**--contacts** *PATH*
: Contacts file (VCF or vCard CSV). Optional.

**--vcf** *PATH*
: Contacts VCF (alternate to `--contacts`). At most one of `--contacts` / `--vcf`.

**--start-date** *YYYY-MM-DD*
: Include messages on or after this local date (inclusive).

**--end-date** *YYYY-MM-DD*
: Include messages before this local date (exclusive).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution** *RES*
: Compress only: max long edge (`720p`, `1080p` default, `4k`).

**--media-max-fps** *N*
: Compress only: max frame rate (default `30`).

**--media-min-size** *SIZE*
: Compress only: re-encode videos at/above this size (default `20M`).

**--media-skip-efficient** *true|false*
: Compress only: skip efficient HEVC under max resolution (default `true`).

**--obfuscate**
: Rewrite names, numbers, text, and media with stable fakes after export.

**--obfuscate-seed** *8-hex*
: Exactly eight hexadecimal characters; implies `--obfuscate`.

# EXIT STATUS

Exits non-zero on invalid arguments, missing input, convert failure, or total media-tool failure. Progress/warnings on stderr; summary on stdout.

# FILES

**Input**
: SyncTech XML with SMS/MMS (and optional embedded MMS parts).

**Output**
: Per-conversation packaging for the chosen format; `attachments/` when media is copied.

# ENVIRONMENT

**PATH**
: Must include `ffmpeg` and `ffprobe` for `convert` / `compress`.

# EXAMPLES

```bash
cargo run --release -p sms-backup-restore-exporter -- \
  --input /path/to/sms-20210328165031.xml \
  --output ./staging/sms-backup-restore \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

# NOTES

Input XML reference: [INPUT_FORMAT.md](INPUT_FORMAT.md). Source → common-message mapping: [IMPORT_MAPPING.md](IMPORT_MAPPING.md).

# SEE ALSO

- [Android text-message user guide](https://bitrealm-dev.github.io/message-vault-io/android/text-messages/)
- [Input format](INPUT_FORMAT.md)
- [Import mapping](IMPORT_MAPPING.md)
