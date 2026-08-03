---
title: Check and clean contacts
description: Validate a contacts file or write a cleaned copy before exporting messages.
---

The Contacts tab checks phone numbers in a VCF or contacts CSV. Updating writes a new file; it does not overwrite the original. Contacts Check/Update runs inside the app as a library — no separate helper binary is required.

## Prepare the file

The input can be:

- a VCF; or
- a CSV with first name, last name, and phone columns. vCard CSV files are accepted where an exporter supports them.

## Check without changing anything

1. Open **Contacts**.
2. Choose the contacts **File**.
3. Leave **USA numbers** selected when numbers should be interpreted as US numbers. Clear it for international parsing.
4. Select **Check**.
5. Read **Log** for `UNCERTAIN`, `DUPLICATE`, and summary lines.

Check is a dry run. It does not write a corrected contacts file.

## Write a cleaned copy

1. Use the same file and country setting.
2. Select **Update**.
3. Read **Log** for the paths that were written.

The app writes a sibling such as `<stem>-update.vcf` or `<stem>-update.csv`. Repeated updates add a number so an earlier result is not overwritten. A `.log` is also written. Only unambiguous phone numbers are changed; uncertain values stay as they were. A CSV update may also create a VCF.

After checking the result, choose the cleaned contacts file in Message Vault when the format accepts contacts. A missing contacts file usually leaves names unresolved rather than preventing message conversion.
