---
title: Rescue an iMazing export
description: Convert iMazing Messages or WhatsApp CSV exports with documented source limitations.
---

The iMazing importer targets iMazing 3.5.5 CSV exports. Use it only when the original iPhone backup or WhatsApp database is unavailable.

## Required input

Choose any one of these:

- one Messages or WhatsApp `.csv` whose headers match the iMazing export;
- one chat folder;
- a `Messages/` or `WhatsApp/` directory; or
- a full iMazing device export root.

Discovery is recursive. A full export can contain Messages, WhatsApp, and Contacts trees. A vCard CSV from the same backup is optional but strongly recommended because many `Chat Session` values are names rather than phone numbers.

Set the timezone when the CSV's `Message Date` values should not use the computer's local timezone. The field accepts a UTC offset such as `UTC-05:00` or a zone such as `America/New_York`.

## Run the import

In **Export**, choose **iMazing (experimental)**. Select the input, optional contacts CSV, timezone, output format, and output directory. Prefer iMazing's **All backup** export when attachment filenames matter.

## Known limitations

- Outgoing rows do not contain the owner's number or name.
- Name-only chats may not resolve to phone numbers without the matching contacts CSV.
- WhatsApp CSV has no complete group roster. Members who never sent a message are absent.
- A silent Messages group member has no phone unless the session label can be matched through contacts.
- Message dates do not include a timezone.
- Long folder and label names can be cut off.
- Attachment names in the CSV can differ from filenames on disk. Suffix matching helps but can still miss files.
- Replies and reactions are free text rather than structured records.
- Group conversations have no stable group identifier.
- Edited and deleted details are limited to the fields that iMazing included.

Messages and WhatsApp conversations remain separate. WhatsApp output filenames include `__whatsapp`.

## Use the command line

See the [`imazing-exporter` reference](/reference/cli/imazing-exporter/) for recursive input discovery, timezone syntax, media settings, and all available flags.
