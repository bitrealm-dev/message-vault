# Dual-source demo design

## Goal

The demo vault must look like a vault that imported two phone backups:

1. **`imessage`** — Apple Messages-style export (primary volume)
2. **`sms-backup-restore`** — Android SMS Backup & Restore–style export (smaller set)

**Sources panel:** per-conversation counts keyed by those source ids.

**Header label** (replaces empty/`unknown` `conversations.service` display):

| Distinct message sources | Header shows |
|--------------------------|--------------|
| `imessage` only | `imessage` |
| `sms-backup-restore` only | `SMS/MMS` |
| both | `SMS/MMS` |
| neither / other only | `unknown` (or existing column if set) |

## Bundle layout

```
demo/
  config/…
  staging/imessage/           # export.source = imessage
    attachments/
  staging/sms-backup-restore/ # export.source = sms-backup-restore
    attachments/
```

JSONL only (no raw `smses.xml`). Android messages use SMS/MMS kinds without iMessage-only decorations.

## Generation (light overlap)

Configurable in `demo_seed.toml` `[sources]`:

- Most 1:1 and all groups → iMessage-only
- A fraction of 1:1 contacts → Android-only
- A small fixed overlap count → both trees; shared messages share content fingerprints; each side keeps unique messages

## Import (`reset-demo`)

1. Regenerate bundle when `demo_seed.toml` is available
2. Wipe demo account
3. Import `staging/imessage` (Replace, source `imessage`)
4. Import `staging/sms-backup-restore` (Append, source `sms-backup-restore`)
5. Cross-source dedupe + process-assets

Release images use the committed `demo/` tree (no in-image regen).

## API

- Conversation list `service` field: derive from distinct `messages.source` as above
- `GET /v1/export/conversations/:id/sources`: per-source `backup_name`, counts, percentage (implement if missing)

## Out of scope

- Raw XML in the demo bundle
- Additional sources (WhatsApp, etc.)
- Changing how exporters fill `conversations.service` on real imports
- Sources panel UI redesign
