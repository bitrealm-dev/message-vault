---
title: "OpenExtract"
description: "Command-line options for rescuing messages from an OpenExtract export."
tableOfContents:
  minHeadingLevel: 2
  maxHeadingLevel: 4
---

## NAME

openextract-exporter - convert OpenExtract conversation CSV (+ VCF) via common message to JSON/CSV/EML/MBOX/JSONL/XML

## SYNOPSIS

```text
openextract-exporter --input <PATH> --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--vcf <PATH> | --contacts <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

## DESCRIPTION

Reads **OpenExtract** conversation CSV (targeted **0.5.1**) — `all_conversations.csv` or `conversation_*.csv`, file or directory — into a common message per conversation, then projects JSON (default) or another `--format`. `*_attachments.csv` sidecars are ignored; this converter does not extract binary media.

`Sender` may be a phone, a display name, or `me`. Pass `--vcf` or `--contacts` so phones and names resolve; without either, a warning is printed. Name-only chats still write (name-based filename) but may be weak for later ingest.

## OPTIONS

**--input** *PATH*
: Conversation CSV file or directory of OpenExtract CSVs.

**--output** *DIR*
: Destination for packaging output.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message (`json` default).

**--vcf** *PATH*
: Contacts VCF from the OpenExtract export.

**--contacts** *PATH*
: Contacts file instead of `--vcf` (VCF or vCard CSV). At most one of `--contacts` / `--vcf`.

**--start-date** *YYYY-MM-DD*
: Include messages on or after this local date (inclusive).

**--end-date** *YYYY-MM-DD*
: Include messages before this local date (exclusive).

**--obfuscate**
: Rewrite names, numbers, and text with stable fakes after export.

**--obfuscate-seed** *8-hex*
: Exactly eight hexadecimal characters; implies `--obfuscate`.

## EXIT STATUS

Non-zero on invalid paths, parse/convert failure, or bad date/seed arguments.

## FILES

**Input**
: OpenExtract conversation CSV(s); optional contacts VCF/CSV.

**Output**
: Output in the selected format. JSON, JSONL, CSV, EML, and MBOX are organized per conversation; XML writes one `smses.xml` backup. Name-only chat ids may remain unresolved for later tools that require phone identifiers. No output contains binary media because OpenExtract attachment sidecars are not imported.

## ENVIRONMENT

None required beyond a normal process environment.

## EXAMPLES

```bash
openextract-exporter \
  --input /path/to/openextract_csv_dir \
  --output ./staging/openextract \
  --vcf /path/to/contacts.vcf
```

## NOTES

Experimental in the GUI. Thin source format: no groups, no media extraction; contacts strongly recommended.

## SEE ALSO

- [OpenExtract user guide](https://bitrealm.io/vault/user/how-to/rescue-imports/)
