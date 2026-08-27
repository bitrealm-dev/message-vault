# Analyze before promote, vacuum after demo — 2026-08-27

Give Postgres (and SQLite) planner statistics before each promote,
and run one explicit `VACUUM` only after `--reset-demo` has finished
every source. This spec records decisions from the 2026-08-27 design
conversation. It is not an implementation plan.

This is a follow-on to
`docs/superpowers/specs/2026-08-27-import-promote-throughput-design.md`.
That work stays as written (one promote transaction, parallel
content-key hash, batched writes). This spec does not change the
lookup search that builds `_promote_msg_map`.

## Goal

A second or third import against an already-full `messages` table
should use the guid index for the staging→production id map, instead
of scanning a table the planner thinks is empty. After the sample
inbox is fully loaded, one vacuum reclaims dead row versions so the
demo vault is not left at ~2 dead rows per live row.

Judge the lookup search from logs (`message id map written (phase
X.Xs)`), not from a fixed time cap. Decide later whether to build
that map during append inserts.

## Current product

`promote_append` copies staging into the lasting tables in **one**
transaction. Early in that transaction it runs `ALTER TABLE messages
DISABLE TRIGGER` so full-text search triggers do not fire on every
insert. That lock (`ShareRowExclusiveLock`) lasts until `COMMIT`.

Autovacuum stays **on**. It is not disabled for import. During
promote it cannot vacuum `messages` (`skipping vacuum of "messages"
--- lock not available`, or `canceling autovacuum task`). Other
tables can still be vacuumed if they are not locked the same way.

After a ~543,000-row iMessage promote, `messages` has had no
`ANALYZE` and about two dead versions per live row (full-text search
update, then content-key update). The next source (SMS Backup &
Restore in `--reset-demo`) then runs:

```sql
INSERT INTO _promote_msg_map (staging_id, prod_id)
SELECT sm.id, m.id
FROM staging_messages sm
JOIN messages m
  ON m.account_id = sm.account_id
 AND m.source = sm.source
 AND m.guid = sm.guid
…
```

Append mode only records ids for empty-guid rows, so the log says
`writing message id map (0 pairs)…` and that join does all the work.
On an unanalyzed, bloated heap that join ran for many minutes.

`--reset-demo` imports iMessage, then SMS, then WhatsApp, then
dedupe, then `process-assets`. There is no extract/attachment gap
between those three sources. The demo bundle is already on disk.
A later **user** import is different: extract and attachment copy do
not lock `messages`, so autovacuum can run while that work happens.

`VACUUM` cannot run inside a transaction. `ANALYZE` can, but it
must not run while the trigger lock is held, and it only sees
**committed** rows.

## Non-goals

- Changing how append builds `_promote_msg_map` (recording ids
  during insert, or skipping the join). Revisit only if a later
  `--reset-demo` still shows a multi-minute map step after `ANALYZE`.
- Explicit `VACUUM` after each source, after a CLI import, or after
  an HTTP import.
- Turning autovacuum off, or `VACUUM FULL`.
- Splitting the promote transaction, or letting browse see rows
  before `COMMIT`.
- Import UI or `/v1/imports` progress.
- The content-key hashing plan (Rayon, batched writes, fill-missing
  dedupe). That stays on its own spec and plan.
- Raising `max_wal_size` or other Postgres server settings.

## Decisions

1. **`ANALYZE` at the start of every `promote_append`.** Before
   `BEGIN`, run `ANALYZE` on `messages`, `attachments`, and
   `tapbacks`. That updates planner statistics on rows that already
   committed (a previous source, or a previous run that committed
   before the process died). The lookup join in **this** promote
   can use `ix_messages_account_source_guid`. Same statements on
   SQLite and Postgres.

2. **No `ANALYZE` after `COMMIT`.** The next promote (or a restart)
   analyzes at the start. A second analyze after commit is extra
   time for no extra help on this promote’s join.

3. **Analyze failure does not fail the import.** Print a warning
   with the error text (`sql:` prefix). Continue into `BEGIN` and
   promote as today. The inbox is unchanged at that point. The next
   lookup search may be slow again, same as today.

4. **Do not change the lookup search.** Keep the guid join. Log
   times (`analyze … (X.Xs)` and the existing `message id map
   written (phase X.Xs)`). No pass/fail time cap in this change.

5. **One `VACUUM` only after all demo data is imported.** In
   `reset-demo`, after the three source imports, `run_dedupe`, and
   `process-assets`, run `VACUUM` on `messages`, `attachments`, and
   `tapbacks` (Postgres). On SQLite, `VACUUM` rewrites the whole
   database file; run that once at the same point so both engines
   compact after the sample inbox is complete. Log
   `vacuum … (X.Xs)`. CLI and HTTP import do not vacuum.

6. **Vacuum failure does not fail `reset-demo`.** Rows are already
   committed. Print a warning and return success.

7. **Autovacuum stays on.** The skip/cancel log lines during
   promote are expected. Autovacuum does the same reclaim as a
   hand `VACUUM`, in the background, when it can get the lock. A
   later user import’s extract and attachment steps are that quiet
   window. `--reset-demo` does not have that window between
   sources, which is why start-of-promote `ANALYZE` exists and why
   the explicit vacuum waits until the whole demo job is done.

8. **Server logs only.** No Import screen or API payload.

## Architecture

```text
promote_append
  ANALYZE messages, attachments, tapbacks     ← new (warn, continue)
  log: analyze … (X.Xs)
  BEGIN
    … existing promote (unchanged) …
  COMMIT

reset-demo
  import iMessage     (skip_dedupe)
  import SMS          (skip_dedupe)
  import WhatsApp     (skip_dedupe)
  run_dedupe
  process-assets
  VACUUM …                                    ← new, demo only
  log: vacuum … (X.Xs)
```

Each import still calls `promote_append`. SMS and WhatsApp therefore
analyze the table left by the previous source before they take the
trigger lock.

## Files

| Path | Change |
|------|--------|
| `crates/vault/server/src/import/promote.rs` | `ANALYZE` + log + warn-on-failure, before `BEGIN` |
| `crates/vault/server/src/db/dialect.rs` | `ANALYZE` and `VACUUM` SQL for each engine (`VACUUM` on named tables on Postgres; whole-file `VACUUM` on SQLite) |
| `crates/vault/server/src/reset_demo.rs` | After dedupe and `process-assets`, `VACUUM` + log + warn-on-failure |
| `CHANGELOG.md` | One dated Changed note |

Leave JSONL staging, the lookup join, content-key fill, `process-assets`
internals, and the web Import screen alone.

## Testing

Server crate tests (`cargo test -p message-vault-server --lib` for
promote / reset-demo as they already exist).

- A second `promote_append` on the same connection still inserts
  the expected rows. `ANALYZE` must not break promote.
- On Postgres (`MV_TEST_POSTGRES_URL`): after one small promote,
  `last_analyze` on `messages` is set **before** a second
  `promote_append` begins. That is the stop/restart case in
  miniature.
- Existing promote tests keep passing; they go through the new
  start step.
- `reset-demo` tests (including the Postgres `--db-url` path when
  that URL is set) still create the `demo` account and at least
  one conversation. A vacuum warning must not fail the command.

No Playwright. No time cap on the map join. A manual
`./scripts/run-vault-pg-dev.sh --reset-demo` after this lands is
how the map-step and vacuum durations are judged.

## Rollout

Follow-on branch or a later commit after the content-key plan’s
remaining tasks (fill-missing dedupe, changelog). Rebuild the vault
before judging `--reset-demo`. Confirm on Postgres first (the path
that stalled on `writing message id map (0 pairs)…`), then a
SQLite `--reset-demo` so both engines run the same analyze-at-start
path.
