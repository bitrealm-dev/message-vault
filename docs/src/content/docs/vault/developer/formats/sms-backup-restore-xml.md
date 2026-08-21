---
title: "SMS Backup & Restore XML output"
description: "How Message Vault writes a SyncTech smses.xml file from the shared conversation structure."
---

Exporters can project the common message into a **single** SyncTech-style backup file:

`{output}/smses.xml`

Root structure:

```xml
<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="N">
  <sms … />
  <mms …>…</mms>
</smses>
```

This is the same family of files that [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/) reads. The importer’s source-format reference is [SMS Backup & Restore input format](/vault/developer/formats/sms-backup-restore/input/).

**Motivation:** Android compatibility. Full-device Android backup/restore without third-party tooling requires root and often an unlocked bootloader; the SMS Backup & Restore app restores `smses.xml` without either, so this format is the practical path for putting messages back onto an Android phone.

## Layout

| Piece | Crate / API |
|-------|-------------|
| XML codec (streaming read/write, SMIL, MMS media) | [`message-sbr`](https://github.com/bitrealm-io/message-vault/blob/main/crates/libs/sbr/) |
| SBR → common message | [`message_ir_format::read_sbr_documents`](../../../crates/libs/ir-format/src/read_sbr.rs) |
| Common message → SBR + export sink | [`message_ir_format::FormatSink`](../../../crates/libs/ir-format/) (XML uses `SbrBackupSession` internally) |
| CLI / GUI | `--format xml` / `OutputFormat::Xml` |

Exporters use `FormatSink::open` → `write_document` per conversation → `finish`. Do **not** call `write_format(..., Xml, …)` (returns an error — a single shared file cannot be safely rewritten per chat).

## Mapping rules

- **SMS** when 1:1 and no attachments: `<sms>` with `type` `1`/`2`, `date` = `timestamp_unix_ms`, `body` = text.
- **MMS** when group and/or attachments (or `message_kind=mms`): `<mms>` with `<parts>` / `<addrs>`; attachment bytes base64 in `data` when available on disk or in memory.
- If `source.fields` has `kind: "sms"|"mms"` (as produced by the SBR importer), attrs / parts / addrs are preferred and overlaid with common-message date/direction/body.
- **Dropped:** entire `imessage` bag (tapbacks, replies, balloons, send effects, edits, announcements, …). Text and media still export as SMS/MMS.

## Related

- [Message-ir architecture](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/architecture/message-ir.md) — shared model and projectors
- [What’s inside an export](/vault/developer/reference/export-structure/) — end-user workflow
- [SMS Backup & Restore import mapping](/vault/developer/formats/sms-backup-restore/mapping/) — XML source → shared model
