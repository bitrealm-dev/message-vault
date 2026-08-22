---
title: "iMazing"
description: "Command-line options for rescuing messages from an iMazing CSV export."
tableOfContents:
  minHeadingLevel: 2
  maxHeadingLevel: 4
---

## NAME

imazing-exporter - convert iMazing Messages / WhatsApp CSV exports via common message to JSON/CSV/EML/MBOX/JSONL/XML

## SYNOPSIS

```text title="Synopsis"
imazing-exporter --input <PATH> --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--contacts <PATH>] [--timezone <OFFSET>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

## DESCRIPTION

Normalizes **iMazing** Messages and/or WhatsApp CSV exports (targeted **3.5.5**) into a common message per conversation, then projects JSON (default) or another `--format`. WhatsApp chats use the `__whatsapp` filename suffix and stay separate from SMS/iMessage.

`Chat Session` is often a name, not a phone — pass `--contacts` from the same backup so chat ids become E.164 when possible. Prefer iMazing’s **All backup** export when attachment filenames matter. Distinct from `imessage-ir-exporter`, which reads Apple `chat.db` directly.

## OPTIONS

**--input** *PATH*
: Messages/WhatsApp CSV, chat folder, `Messages/`, `WhatsApp/`, or a full device export root (recursive).

**--output** *DIR*
: Destination for packaging output and media.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message (`json` default).

**--contacts** *PATH*
: vCard CSV from the same backup. Optional; without it phones are not resolved to names.

**--timezone** *OFFSET*
: UTC offset for naive Message Date values (e.g. `UTC-05:00` or `America/New_York`). Default: host local.

**--start-date** / **--end-date** *YYYY-MM-DD*
: Date filter interpreted at timezone midnight (inclusive start, exclusive end).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

## EXIT STATUS

Non-zero on missing paths, convert failure, invalid timezone/dates, or total media-tool failure.

## FILES

**Input**
: iMazing CSV export tree; optional Contacts CSV.

**Output**
: Output in the selected format. JSON, JSONL, CSV, EML, and MBOX are organized per conversation; XML writes one `smses.xml` backup. WhatsApp conversation stems include `__whatsapp`. Media can be written under `attachments/`.

## ENVIRONMENT

**PATH**
: Needs `ffmpeg` / `ffprobe` for `convert` / `compress`.

## EXAMPLES

```bash title="Examples"
imazing-exporter \
  --input "/path/to/Device Export Root" \
  --output ./staging/imazing \
  --contacts "/path/to/Contacts - ….csv" \
  --timezone America/New_York
```

## NOTES

Experimental in the GUI. Outgoing sender identity, WhatsApp group roster, and reaction/reply fidelity are limited by the upstream CSV.

## SEE ALSO

- [iMazing user guide](/vault/user/how-to/rescue-imports/)
- [Input format and source limitations](/vault/developer/formats/imazing/input/)
- [Importer design](/vault/developer/formats/imazing/design/)
