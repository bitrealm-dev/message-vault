# Dual-source demo design

## Goal

The demo vault must look like a vault that imported multiple phone backups:

1. **`imessage`** — Apple Messages-style export (primary volume)
2. **`sms-backup-restore`** — Android SMS Backup & Restore–style export (smaller set)
3. **`whatsapp`** — WhatsApp-style export for a tuneable fraction of contacts (same phone numbers, platform `whatsapp`)

**Sources panel:** per-conversation counts keyed by those source ids.

**Header label** (from distinct `messages.source` values — not conversation-level transport):

| Distinct message sources | Header shows |
|--------------------------|--------------|
| `imessage` only | `imessage` |
| `sms-backup-restore` only | `SMS/MMS` |
| both iMessage + Android | `SMS/MMS` |
| `whatsapp` only | `WhatsApp` |
| neither / other only | `unknown` |

Per-message SMS / iMessage / RCS live on `messages.service` (transport). Handle platform identity is `handles.service` (`phone` | `whatsapp`).

## Bundle layout

```
demo/
  config/…
  staging/imessage/           # export.source = imessage
    attachments/
  staging/sms-backup-restore/ # export.source = sms-backup-restore
    attachments/
  staging/whatsapp/           # export.source = whatsapp
    attachments/
```

JSONL only (no raw `smses.xml`). Android messages use SMS/MMS kinds without iMessage-only decorations.

## Generation (light overlap)

Configurable in `demo_seed.toml` `[sources]` / `[messages]`:

- Most 1:1 and all groups → iMessage-only
- A fraction of 1:1 contacts → Android-only
- A small fixed overlap count → both trees; shared messages share content fingerprints; each side keeps unique messages
- `whatsapp_contact_fraction` (default 0.20) → additional WhatsApp 1:1 threads
- `apple_fallback_transport_fraction` (default 0.20) → SMS/RCS mix inside iMessage threads

## Import (`reset-demo`)

1. Regenerate bundle when `demo_seed.toml` is available
2. Wipe demo account
3. Import `staging/imessage` (Replace, source `imessage`)
4. Import `staging/sms-backup-restore` (Append, source `sms-backup-restore`)
5. Import `staging/whatsapp` (Append, source `whatsapp`)
6. Cross-source dedupe + process-assets

Release images use the committed `demo/` tree (no in-image regen).

## API

- Conversation list `service` field: derive from distinct `messages.source` as above
- `GET /v1/export/conversations/:id/sources`: per-source `backup_name`, counts, percentage (implement if missing)

## Out of scope

- Raw XML in the demo bundle
- Sources panel UI redesign
