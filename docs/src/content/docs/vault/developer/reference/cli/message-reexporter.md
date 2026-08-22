---
title: "Convert an existing export"
description: "Command-line options for converting a Message Vault output directory to another format."
tableOfContents:
  minHeadingLevel: 2
  maxHeadingLevel: 4
---

## NAME

message-reexporter - convert an existing Message Vault output directory to another packaging format

## SYNOPSIS

```text title="Synopsis"
message-reexporter --input <DIR> --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

## DESCRIPTION

Auto-detects a single input format among `csv`, `eml`, `mbox`, `json`, `jsonl`, and `xml` (`smses.xml`) in `--input`, loads conversations into the common message, then writes `--format` (default `json`) via the shared packaging pipeline (media modes + obfuscate included).

`--output` must differ from `--input`. The desktop GUI **Convert** tab uses the same library path.

## OPTIONS

**--input** *DIR*
: Prior export directory (exactly one format class must be detected).

**--output** *DIR*
: Destination directory (must differ from input).

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging (`json` default).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

## EXIT STATUS

Exits non-zero when arguments are invalid, input and output paths are the same, no supported format can be detected, multiple format classes are present, an input file cannot be read, or output conversion fails.

## FILES

**Input**
: A Message Vault output directory containing exactly one supported format class: CSV, EML, MBOX, JSON, JSONL, or one `smses.xml`.

**Output**
: A different directory containing the selected format. JSON, JSONL, CSV, EML, and MBOX are organized per conversation; XML writes one `smses.xml` backup. Media can be written under `attachments/`.

## ENVIRONMENT

**PATH**
: Must include `ffmpeg` and `ffprobe` when `--media-mode` is `convert` or `compress`.

## EXAMPLES

```bash title="Examples"
cargo run -p message-reexport --bin message-reexporter -- \
  --input /path/to/prior-export \
  --output /path/to/new-export \
  --format eml
```

## NOTES

The detector ignores `attachments/` and legacy `*.meta.json` files. It rejects arbitrary vendor exports and directories that mix supported format classes.

## SEE ALSO

- [Convert formats](/vault/user/how-to/convert-formats/)
