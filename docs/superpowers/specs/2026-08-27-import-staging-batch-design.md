# Group staging writes — 2026-08-27

Write many staging rows in one statement, and remember sender handle
ids for the rest of an import. This spec records decisions from the
2026-08-27 design conversation. It is not an implementation plan.

This is a follow-on to
`docs/superpowers/specs/2026-08-27-import-promote-throughput-design.md`.
That work batched lasting-table writes and left JSONL staging as
one statement per row. This spec covers that first step only.

## Goal

The first import step (JSONL → `staging_*`) should not spend most of
its time on one database call per message. CLI import, website
upload, and `--reset-demo` all use this path, so all three get the
faster write.

Judge the win from a real `--reset-demo` log: the first-source
`[N/334] … (Xs)` lines. No pass/fail time cap in tests.

## Current product

Import reads each `.jsonl` file, copies the few real attachment
files that exist, then writes staging tables:

1. One `INSERT` for the conversation.
2. One handle upsert (insert-or-select) plus contact-link work per
   participant.
3. Per message: handle upsert for an incoming sender, then one
   `INSERT … ON CONFLICT DO NOTHING RETURNING id` into
   `staging_messages` (18 bind values), then one insert per
   attachment and per tapback.

sqlx Any has no reusable prepared statement. Each of those calls is
a full round trip. On Postgres in Docker the path is vault process →
TCP `127.0.0.1:5432` → `docker-proxy` → the container. A sample
inbox of ~543,000 iMessage rows therefore issues hundreds of
thousands of statements before promote even starts.

The progress line prints `assets_copied` and `missing`. On the demo
seed only a handful of files copy; `missing` is a cheap “file not
on disk” check. The clock time is the row writes.

Promote already inserts lasting messages in 50,000-id windows and
writes content-key pairs in multi-row `VALUES` chunks. Staging never
got that treatment.

Commits already happen every 50 files (`STAGING_COMMIT_EVERY`).

## Non-goals

- Postgres `COPY` or `UNNEST`. Same `INSERT … VALUES` shape on
  SQLite and Postgres.
- Changing JSONL parsing, attachment file copy, or `process-assets`.
- Changing promote, `ANALYZE`, or `VACUUM`.
- Import UI or `/v1/imports` progress fields.
- Letting browse see rows before promote commits.
- A CI time cap on `--reset-demo`.
- Changing the 50-file commit cadence unless a later run shows
  that cadence is the new bottleneck.

## Decisions

1. **Every import uses the new write.** CLI, HTTP, and `--reset-demo`
   share `import_file_to_staging`. Do not add a demo-only path.

2. **Same statements on both engines.** Multi-row `INSERT`. Chunk
   size is limited by SQLite’s 999 bind values. A message row has
   18 columns, so a message chunk is about 50 rows. Attachment and
   tapback chunks use the same rule (column count × row count ≤
   999). A helper in `db/sql.rs` picks the row count from the
   column count.

3. **Remember handle ids in memory for this import.** Key:
   account + normalized value + handle type + service. The first
   lookup still hits the database (`INSERT … ON CONFLICT` then
   `SELECT id`). Later messages from the same person reuse the id.
   The map lives for the whole import, including across the
   every-50-file commits (handle rows are already lasting).

4. **Flush messages per conversation, in chunks.** After disk
   attachment prep, buffer message rows for that chat, write about
   50 at a time:

   `INSERT INTO staging_messages (…) VALUES (…), (…), … ON CONFLICT DO NOTHING RETURNING id, guid`

   Match returned ids to buffered rows by `account_id` + `source` +
   `guid` (the unique index on `staging_messages`). Rows that do
   not return were skipped as duplicates. Count them as today’s
   `messages_deduped`. They get no attachments or tapbacks.

5. **Then flush attachments and tapbacks in chunks** for the
   messages that received ids. Same bind-limit helper.

6. **Conversations and participants stay one row at a time.**
   There are hundreds of those, not hundreds of thousands.

7. **A failed chunk fails the import.** No retry of a half-written
   chunk. Same as a failed single-row insert today.

8. **Server logs stay as they are.** Keep
   `[N/total] name  msgs=… attachments=… assets_copied=… missing=… (Xs)`.
   No new Import-screen or API fields.

## Architecture

```text
import_file_to_staging
  read JSONL
  prepare attachments on disk          ← unchanged
  upsert conversation + participants   ← one row (unchanged)
  for each message chunk (~50):
    INSERT staging_messages … RETURNING id, guid
    match ids; count skipped duplicates
    INSERT staging_attachments …       ← chunks
    INSERT staging_tapbacks …          ← chunks
  commit every 50 files                ← unchanged
  promote_append                       ← unchanged
```

Handle map: miss → database upsert → store id; hit → reuse id.

## Files

| Path | Change |
|------|--------|
| `crates/vault/server/src/db/sql.rs` | Max rows for a column count (999-bind cap) |
| `crates/vault/server/src/import/staging.rs` | Chunked message / attachment / tapback inserts; match `RETURNING` ids |
| `crates/vault/server/src/import/contact_name.rs` | Use the handle map when resolving an incoming sender |
| `crates/vault/server/src/db/handles.rs` | Accept a caller-owned map and fill it on upsert |
| `CHANGELOG.md` | One dated Changed note |

Leave promote, dialect analyze/vacuum, `process-assets`, and `web/`
alone.

## Testing

Server crate tests (`cargo test -p message-vault-server --lib`).
Postgres cases run when `MV_TEST_POSTGRES_URL` is set.

- A conversation with more than 50 messages stores every row.
  Attachments and tapbacks sit on the correct messages.
- A second insert of the same guid is skipped. The first row keeps
  its attachment. The skip count increases.
- Many messages from the same incoming sender create one handle
  row, not one per message.
- The chunk-size helper never chooses a row count that would
  exceed 999 binds.
- Existing import tests keep passing; they go through the new
  write path.

No Playwright. No time cap. A manual
`./scripts/run-vault-pg-dev.sh --reset-demo` is how the first-source
`[N/334] … (Xs)` line is judged.

## Rollout

New branch from current `main`. Rebuild the vault before judging
`--reset-demo`. Confirm on Postgres first (Docker round-trips were
the painful path), then SQLite so both engines use the same
chunked insert.
