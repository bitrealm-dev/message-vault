# NAME

sms-backup-plus-exporter - convert SMS Backup+ EML exports via common message to JSON/CSV/EML/MBOX/JSONL/XML

# SYNOPSIS

```text
sms-backup-plus-exporter [-v|--verbose] [--no-summary] convert
    --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--input <PATH>]...
    [--owner-phone <PHONE>]... [--owner-email <EMAIL>]...
    [--contacts <PATH> | --vcf <PATH>] [--name-mapping <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Converts offline **SMS Backup+** `.eml` exports (targeted **1.5.11**) into a common message per conversation, then projects JSON (default) or another `--format`. This tool does **not** sign in to email or talk to IMAP — only files on disk.

Backups appear as **one file per message** or **archive emails** (many messages in one body). Multiple `--input` roots are merged and path-deduped; duplicate messages are kept once.

Owner phone and email may come from flags or `config/owner.toml` (`phones`, `emails`, optional `source_dirs`). Optional `--name-mapping` defaults to `config/name-mapping.csv` when present. Contacts resolve name↔phone; without `--contacts`/`--vcf`, a warning is printed. The desktop app converts with verbose logging.

# OPTIONS

## Global

**-v**, **--verbose**
: Log progress to stderr.

**--no-summary**
: Skip the end-of-run summary on stdout.

## convert

**--input** *PATH*
: An `.eml` file or directory of EMLs. Repeatable. Default: `source_dirs` from `config/owner.toml` when set. The desktop app requires exactly one path.

**--output** *DIR*
: Destination for packaging output and `attachments/`.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message (`json` default).

**--owner-phone** *PHONE*
: Owner number(s). Default: `phones` in `config/owner.toml`.

**--owner-email** *EMAIL*
: Owner email(s) for sent detection when `X-smssync-type` is missing. Default: `emails` in `config/owner.toml`.

**--contacts** *PATH*
: Contacts file (VCF or vCard CSV). Optional.

**--vcf** *PATH*
: Contacts VCF (alternate to `--contacts`).

**--name-mapping** *PATH*
: CSV `Phone,Incorrect Name` for EML aliases. Default: `config/name-mapping.csv` when present.

**--start-date** / **--end-date** *YYYY-MM-DD*
: Local date filter (inclusive start, exclusive end).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

# EXIT STATUS

Non-zero on missing identity/input, convert errors, or total media-tool failure.

# FILES

**Input**
: Offline EML export tree (not live IMAP).

**Output**
: Output in the selected format. JSON, JSONL, CSV, EML, and MBOX are organized per conversation; XML writes one `smses.xml` backup. Media can be written under `attachments/`. Unresolved peers use an `unknown` conversation stem.

**config/owner.toml**
: Optional defaults for phones, emails, and `source_dirs` (relative to the config directory).

# ENVIRONMENT

**PATH**
: Needs `ffmpeg` / `ffprobe` for `convert` / `compress` media modes.

# EXAMPLES

```bash
sms-backup-plus-exporter -v convert \
  --input /path/to/eml_export \
  --output ./staging/sms-backup-plus \
  --owner-phone +15555550100 \
  --owner-email you@example.com \
  --contacts /path/to/contacts.csv
```

# NOTES

Experimental in the GUI. Attachment→message pairing in archives is heuristic. EML layouts: [FORMAT.md](FORMAT.md). Field mapping: [IMPORT_MAPPING.md](IMPORT_MAPPING.md).

# SEE ALSO

- [SMS Backup+ user guide](https://bitrealm-dev.github.io/message-vault-io/other-app-exports/sms-backup-plus/)
- [Input EML format](FORMAT.md)
- [Import mapping](IMPORT_MAPPING.md)
