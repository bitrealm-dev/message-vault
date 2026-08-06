---
title: What’s inside an export
description: Learn how Message Exporters represents conversations and arranges the output files.
---

Every source is first converted into the same per-conversation structure: participants, timestamps, text, attachment details, and any extra fields the source can provide. The selected output format then writes that structure as JSON, JSON Lines, CSV, EML, MBOX, or Android XML.

```text
Phone backup or app export
        ↓
Shared conversation structure
        ↓
JSON · JSONL · CSV · EML · MBOX · Android XML
```

Choose the final format on the first run. JSON is the default because it keeps the shared structure with the least loss and can be converted later.

## How conversations are named

Most formats create one artifact per conversation. The filename usually comes from the peer phone number. Untitled groups use a `group_` name built from participant handles. WhatsApp names include `__whatsapp`.

## Files with an attachments folder

JSON, JSON Lines, and CSV use a shared sidecar folder when media copying is enabled:

```text
output/
├── +15555550101.json
├── group_+15555550101_+15555550102.json
└── attachments/
    └── IMG_0001.jpg
```

CSV uses `.csv`, and JSON Lines uses `.jsonl`, but the folder arrangement is the same. Conversation files refer to media by relative path.

## EML folders

EML creates a directory for each conversation and an email file for each message:

```text
output/
└── +15555550101/
    ├── 000001_2021-03-28_165031_a1b2c3d4.eml
    └── 000002_2021-03-28_170102_e5f6a7b8.eml
```

Attachments are embedded in each EML file.

## MBOX files

MBOX creates one mailbox per conversation:

```text
output/
├── +15555550101.mbox
└── group_+15555550101_+15555550102.mbox
```

Attachments are embedded in the mailbox.

## Android XML

Android XML creates one file for the complete export:

```text
output/
└── smses.xml
```

MMS media is base64-encoded inside the XML.

The selected attachment mode can change whether media exists, but it does not change the basic conversation layout. See [Handle media and private information](/work-with-exports/media-and-privacy/).
