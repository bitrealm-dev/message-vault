---
title: Rescue a GO SMS Pro backup
description: Convert GO SMS Pro XML and PDU files when no supported backup remains.
---

GO SMS Pro import is a rescue path. Its MMS PDU decoding uses best-effort rules, and many PDU files are empty placeholders.

## Required input

- One backup directory containing `gosms_sys*.xml` files for SMS and matching `I_*.pdu` files for MMS.
- At least one owner phone number, in E.164 form or as digits. Add every number that belonged to the backed-up device.
- A separate output directory.

A contacts VCF or vCard CSV is optional and can fill display names.

## Run the import

In **Export**, choose **GO SMS Pro (experimental)**. Select the backup directory, enter the owner phone number or numbers, choose the output directory and format, then run the exporter.

## Known limitations

- PDU decoding is best effort. Structured MMS fields are used when available, with simpler pattern matching as a fallback.
- Many GO SMS Pro PDU files contain no participants, text, or media and are skipped.
- Owner phone numbers determine PDU direction and grouping. Wrong values can reverse sent and received messages.
- XML SMS entries with unusable addresses, invalid dates, or types other than inbox (`1`) and sent (`2`) are skipped.
- A non-empty PDU with no party other than the owner is skipped.
- Contacts are optional. Without them, display names can remain unresolved.

The exporter may write `skipped_invalid_address.csv`, `skipped_empty_pdu.csv`, and `skipped_no_party.csv`. These files explain skipped records; they are not conversation exports.

## Use the command line

See the [`go-sms-pro-exporter` reference](/reference/cli/go-sms-pro-exporter/) for repeatable owner-phone flags, media settings, skip diagnostics, and all available options.
