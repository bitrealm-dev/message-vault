---
title: "GO SMS Pro"
description: "Command-line options for rescuing messages from a GO SMS Pro XML export."
tableOfContents:
  minHeadingLevel: 2
  maxHeadingLevel: 4
---

## NAME

go-sms-pro-exporter - convert GO SMS Pro XML+PDU backups via common message to JSON/CSV/EML/MBOX/JSONL/XML

## SYNOPSIS

```text
go-sms-pro-exporter --input <DIR> --output <DIR> --owner-phone <PHONE>...
    [--format json|jsonl|csv|eml|mbox|xml]
    [--contacts <PATH> | --vcf <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

## DESCRIPTION

Reads a **GO SMS Pro** (GOMO / Jiubang) backup folder — `gosms_sys*.xml` for SMS and `I_*.pdu` for MMS — into a common message per conversation, then projects JSON (default) or another `--format`. Media lands under `attachments/` when enabled. Skip diagnostics may write `skipped_invalid_address.csv`, `skipped_empty_pdu.csv`, `skipped_no_party.csv`.

PDU decoding is heuristic (MMS Encapsulation / WSP-inspired); many stub PDUs are empty. Owner phone(s) are required for PDU direction and chat grouping — wrong values flip sent/received. Pass `--contacts` or `--vcf` to fill display names.

Thanks: [python-messaging](https://github.com/pmarti/python-messaging).

## OPTIONS

**--input** *DIR*
: Backup folder containing XML and PDU files.

**--output** *DIR*
: Destination for packaging output and `attachments/`.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message (`json` default).

**--owner-phone** *PHONE*
: Owner number (E.164 or digits). Repeat for multiple owner numbers. Required.

**--contacts** *PATH*
: Contacts file for phone→name fill (VCF or vCard CSV). Optional.

**--vcf** *PATH*
: Contacts VCF (alternate to `--contacts`). At most one of `--contacts` / `--vcf`.

**--start-date** *YYYY-MM-DD*
: Include messages on or after this local date (inclusive).

**--end-date** *YYYY-MM-DD*
: Include messages before this local date (exclusive).

**--media-mode** *MODE*
: `disabled` (no files), `clone` (default), `convert`, or `compress`. Convert/compress need ffmpeg/ffprobe.

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

## EXIT STATUS

Exits non-zero on invalid arguments, missing paths, convert failure, or when media convert/compress fails for all candidates. Warnings (e.g. missing contacts) go to stderr; a summary is printed to stdout on success.

## FILES

**Input**
: Directory with `gosms_sys*.xml` and matching `I_*.pdu` blobs.

**Output**
: Output in the selected format. JSON, JSONL, CSV, EML, and MBOX are organized per conversation; XML writes one `smses.xml` backup. Media can be written under `attachments/`, and optional `skipped_*.csv` diagnostics explain rejected source records.

## ENVIRONMENT

**PATH**
: Must include `ffmpeg` and `ffprobe` when `--media-mode` is `convert` or `compress`.

## EXAMPLES

```bash
go-sms-pro-exporter \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

## NOTES

Experimental in the desktop GUI. Field mapping and skip counters: [IMPORT_MAPPING.md](https://github.com/bitrealm-dev/message-vault/blob/main/crates/exporters/go-sms-pro-exporter/docs/IMPORT_MAPPING.md).

## SEE ALSO

- [GO SMS Pro user guide](https://bitrealm-dev.github.io/message-vault/other-app-exports/go-sms-pro/)
- [Import mapping](https://github.com/bitrealm-dev/message-vault/blob/main/crates/exporters/go-sms-pro-exporter/docs/IMPORT_MAPPING.md)
