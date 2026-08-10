# Contact handles table

**Date:** 2026-08-10  
**Status:** Approved for implementation  
**Scope:** Contact drawer handles UI + contact detail/mutation API

## Problem

The contact info panel lists handles as simple service + value rows and keeps a separate Messages section with contact-level totals. Users cannot see per-handle conversation/message breakdowns, edit or unlink handles, or jump from a handle into the matching conversation search.

## Goals

- One table of handles with columns: Service | Handle | Date Range | Conversations | Message Count | edit | trash
- Top-right **Add** for a new handle row
- Date range as `YYYY-MM-DD` – `YYYY-MM-DD`
- Conversations and Message Count each show two stacked lines: individual and group
- Click handle → search all conversations for that handle
- Click individual / group conversation lines → filtered conversation search
- Message Count lines mirror conversation counts (individual / group message totals)
- Footer totals across all handles
- Inline add/edit; trash unlinks the handle from the contact only
- Remove the separate Messages section

## Non-goals

- Deleting handle rows from the global `handles` table
- Soft-deleting / trashing handles
- Redesigning contact name editing
- Per-service stats beyond the service column on each row

## Approach

Extend `GET /v1/export/contacts/{id}` with per-handle individual/group conversation and message counts. Implement `POST /v1/export/contacts/{id}` for rename, add, update, and remove (unlink). Rebuild the drawer handles UI as a react-aria table and widen the drawer.

## API

### Detail GET — per handle

```json
{
  "handle": "+17035564549",
  "service": "phone",
  "start_date": "2017-03-12T00:00:00Z",
  "end_date": "2026-01-08T00:00:00Z",
  "individual_conversations": 1,
  "group_conversations": 3,
  "individual_message_count": 120,
  "group_message_count": 4003
}
```

Contact-level `direct_conversations`, `group_conversations`, and `total_messages` remain for footer/summary. Replace the old direct-only `message_count` field with the split fields above.

### Mutations POST `/v1/export/contacts/{id}`

- `{ "name": "…" }`
- `{ "add_handle": { "handle", "service" } }`
- `{ "update_handle": { "previous_handle", "handle", "service" } }`
- `{ "remove_handle": { "handle" } }` — delete `contact_handles` row only

## UI

- Drawer width ~560–640px
- Handles table + footer totals; no Messages section
- Zero counts are plain text (not links)
- Browse uses `handle:…`, `handle:… is:direct`, `handle:… is:group` (footer may use `contact:id` when aggregating)

## Testing

- Server unit tests for split per-handle stats and POST add/update/remove
- Manual: open contact → table → click handle / individual / group → conversation search; add/edit/unlink refresh counts
