# Skip oversized and missing attachments

**Date:** 2026-08-11
**Status:** Approved for implementation
**Scope:** `vault-push` prepare/upload, message-ir `IrAttachment`, vault import staging/promote, message/export APIs, message bubble UI

## Problem

If one attachment in a conversation JSONL is larger than `asset_max_bytes` or missing on disk, `vault-push` fails the entire conversation file. Text messages and other attachments in that chat are not imported.

## Goal

Import every message. Skip only the bad attachment bytes. Keep a visible placeholder so the user can see that the message had an attachment, including filename and mime type when known. Record the skip under Import Errors.

## Behavior

1. Do not fail the whole JSONL for an oversized or missing attachment file.
2. Upload other attachments in that conversation normally.
3. Import every message.
4. Persist the bad attachment with `original_name`, `mime_type`, `size_bytes`, and `path` when known; leave `sha256` and `assets_path` null; set `missing_reason`.
5. Emit an Import Errors row with `kind: "skip"` for that attachment.
6. Conversation file status stays `ok` if messages import. Message accounting still counts these messages as attempted/inserted (or deduped). Only the attachment is skipped.

## `missing_reason` values

| Value | Meaning |
|--------|---------|
| `too_large` | File length exceeds `asset_max_bytes` |
| `file_missing` | Path is not present on disk during prepare |

If `missing_reason` is set, `sha256` and `assets_path` stay null.

## Data model

- `IrAttachment.missing_reason: Option<String>` (omit when none)
- SQLite `attachments.missing_reason TEXT` and `staging_attachments.missing_reason TEXT`
- Existing DBs get the column via schema ensure (`ALTER TABLE ... ADD COLUMN`)
- Import `AttachmentRecord`, staging insert, and promote copy the field
- Message/export API JSON includes `missing_reason`
- Web `MessageAttachment.missing_reason: string | null`

## Push pipeline

During attachment scan in `vault-push`:

1. Classify each attachment as uploadable or skipped (`too_large` / `file_missing`).
2. Build the unique digest upload set only from uploadable attachments.
3. When projecting the message JSONL line, keep skipped attachment objects with metadata, clear digest, set `missing_reason`.
4. Emit skip issues (`extract:issue` with `kind: "skip"`).
5. Do not mark the conversation `failed` solely because attachments were skipped.

## UI

`AttachmentThumbnail` renders a non-clickable chip when `missing_reason` is set:

- `too_large` → `{name} · {mime} (missing — too large)`
- `file_missing` → `{name} · {mime} (missing — file not found)`

Use `original_name` or path basename for the name. Omit the mime segment when unknown.

## Testing

- One oversized attachment plus one normal attachment: conversation `ok`, messages attempted, oversized not PUT, skip issue present, imported attachment has `missing_reason: "too_large"`.
- Missing file on disk: `missing_reason: "file_missing"`, conversation still proceeds.
- Server persists and returns `missing_reason` with null sha256.
- Message chip shows the copy above.

## Out of scope

- Raising `asset_max_bytes` defaults.
- Inline text notes in message body.
- Additional missing-reason values.
- Special journal handling for conversations that previously failed for this reason.
