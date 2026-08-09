# Promote message insert: clearer logs + faster bulk load

## Problem

Promote logs `pausing FTS triggers…` then sits for minutes. Dropping FTS triggers is fast; the time is spent in the set-based `INSERT OR IGNORE … SELECT` of staging messages into `messages` (and the staging→prod id map), with no intermediate progress.

## Goals

1. **Visibility** — phase logs with local and total elapsed time; chunk progress while inserting.
2. **Speed** — drop non-unique secondary indexes on `messages` for large promotes, insert in chunks, rebuild indexes afterward; set-based fill of `_promote_msg_map`.

## Design

### Logging

- Separate logs for: FTS trigger pause, secondary index drop (when used), each message insert chunk, id-map build, index rebuild, attachments, FTS bulk index, content keys, commit.
- Each completion line includes `phase Xs, total Ys`.

### Chunked inserts

- Batch size: 10_000 staging message ids (range on `sm.id`).
- Same SQL as today (`INSERT` / `INSERT OR IGNORE … SELECT`), plus `sm.id > ?lo AND sm.id <= ?hi`.
- Append empty-guid rows remain a final non-chunked pass (outside the partial unique index).
- Per chunk: insert → extend id map → progress log.

### Secondary indexes

Drop inside the promote transaction (keep PK + unique `ix_messages_account_source_guid`):

- `ix_messages_conversation_timestamp`
- `ix_messages_conversation_source_timestamp`
- `ix_messages_account_id`
- `ix_messages_content_key`
- `ix_messages_duplicate_of`
- `ix_messages_import_id`
- `ix_messages_source`

Recreate after all message chunks (before attachments).

**When to drop:** only if staging message count ≥ 5_000 and staging count × 5 ≥ existing `messages` row count. Small appends into a large table keep indexes (rebuild would cost more than the insert).

### `_promote_msg_map`

Fill with batched multi-row `INSERT … VALUES` (≤ 400 pairs per statement) instead of one `execute` per pair.

### Tests

Existing promote / append / deferred-FTS tests must still pass; behavior unchanged aside from performance and logging.
