---
title: Handle media and private information
description: Copy, convert, compress, omit, or obscure attachments and personal data.
---

Choose the attachment and privacy settings before starting an export or re-export.

## Choose an attachment mode

| Setting | What it does |
| --- | --- |
| **Do not copy** | Leaves media out of the export. |
| **Copy** | Includes the original media files. This is the default. |
| **Convert** | Converts common media types to `.jpg`, `.mp4`, or `.mp3`. |
| **Convert & compress** | Re-encodes media with the size and quality limits you choose. |

**Convert** and **Convert & compress** require `ffmpeg` and `ffprobe`. Release ZIPs ship both beside the desktop app; otherwise place them in `MESSAGE_VAULT_IO_BIN` or on `PATH`. Copying does not need those tools.

When compression is selected, you can set:

- maximum resolution, such as 1080p;
- maximum frame rate;
- minimum file size, such as `20M`, below which videos are not re-encoded; and
- whether efficient HEVC video under the resolution limit should be left unchanged.

JSON, JSONL, and CSV keep copied media in `attachments/`. EML, MBOX, and Android XML embed transformed media and do not keep a sidecar folder.

## Replace personal information before sharing

Enable **Obfuscate** to replace display names, phone numbers, message text, and media with stable substitutes. This is intended for copies shared in demonstrations or support reports.

When **Obfuscate** is on, the export does not copy or convert real attachment files. Attachment modes such as **Copy** or **Convert** are ignored for staging: the output uses three shared placeholders (`placeholder.jpg`, `placeholder.mp4`, `placeholder.bin`) chosen from each attachment’s type. That avoids copying media only to delete it afterward.

You can enter a seed of exactly eight hexadecimal characters to make the same input produce repeatable substitutions. Leaving it blank creates a seed for the run.

Obfuscation applies to every output format. It changes the output copy, not the source backup.

## Practical defaults

- Use **Copy** for a private personal archive.
- Use **Convert** or **Convert & compress** only when the next program needs more common or smaller media.
- Use **Do not copy** when message text is enough.
- Enable **Obfuscate** before an export leaves a private machine.
