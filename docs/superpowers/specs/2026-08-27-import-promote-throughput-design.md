# Faster promote with louder server logs — 2026-08-27

Keep one all-or-nothing promote transaction. Hash and write content
keys in batches, stop hashing the same rows twice after import, and
print phase lines so a large `--reset-demo` or CLI import does not sit
on `filling content keys…`. This spec records decisions from the
2026-08-27 design conversation. It is not an implementation plan.

## Goal

A large import (hundreds of thousands of messages) should finish
sooner and name each long step in the server log. Browse and search
keep seeing the old vault until promote commits. Either every new
message from that promote lands, or none do.

“More responsive” here means **progress in stdout**, not other
requests sneaking into the write, and not the desktop Import screen.

## Current product

Import writes JSONL into `staging_*` tables (commit every 50 files),
then `promote_append` copies those rows into the lasting tables in
**one** transaction:

1. Conversations and participants (set-based `INSERT … SELECT`).
2. Messages in 50,000-id windows (restored 2026-08-27).
3. Attachments and tapbacks (set-based).
4. Full-text search bulk fill, then triggers restored.
5. Content keys: SHA-256 fingerprints used later to hide
   cross-source duplicates.
6. Commit.

A content key is a hash of chat identity, direction, sender, UTC
time, collapsed body, and attachment hashes. Staging does not store
it. Promote fills it after attachments exist.

`fill_missing_content_keys` today:

- Loads every production row with a null or empty key (one SELECT).
- Hashes each row on the Tokio worker, one after another.
- Inserts one `(id, key)` row at a time into a temp table.
- Applies all keys with one `UPDATE … FROM`.

On a ~543,000-message demo seed that last step is hundreds of
thousands of Postgres round-trips. The log shows one line,
`filling content keys…`, until the batch finishes.

`import_cli` always sets `fill_content_keys: true`. If dedupe is not
skipped, `dedupe_cross_source` then calls `recompute_all_content_keys`
and hashes **every** message on the account again. `--reset-demo`
imports iMessage, SMS, and WhatsApp with `skip_dedupe: true` (keys
still filled during each promote), then `run_dedupe` rebuilds every
key a fourth time.

Dedupe does not delete rows. It sets `duplicate_of` so browse can
hide extras. Survivor: first imported source, then source name.
Exact pass: same content key. Near pass: same chat/body within a
time window (default ±2 seconds).

## Non-goals

- Import UI or import-session API progress.
- Committing promote in slices (partial inbox visible mid-run).
- Postgres `COPY` / `UNNEST` (revisit only if batched inserts still
  dominate a large Postgres import).
- Rewriting JSONL staging or `process-assets` (ffmpeg).
- Letting browse or search see new rows before promote commits.
- An admin “rebuild every fingerprint” command in this change.
  `recompute_all_content_keys` stays in the crate for later.

## Decisions

1. **One promote transaction.** Speed and logs stay inside the
   current all-or-nothing boundary. A hash-task panic or a write
   error rolls the transaction back. The inbox stays as it was
   before this import.
2. **Same SQL on SQLite and Postgres.** No engine-specific COPY
   path in this change.
3. **Parallel hashes, batched writes.** After the SELECTs, hashing
   runs on a Rayon pool off the Tokio worker (`spawn_blocking`).
   Temp-table inserts use multi-row `VALUES` chunks of at most
   `SQLITE_IN_CHUNK` (400) pairs, the same bind-limit helper as the
   promote message-id map. One `UPDATE … FROM` still copies keys
   onto `messages`. The fingerprint formula does not change.
4. **Message windows stay 50,000 ids.** Those lines already exist.
5. **Logs name the silent work.** Content-key fill prints:
   - `hashing content keys (N messages)…` when the SELECT returns
   - `writing content keys … running=X/N` about every 50,000 keys
     written, and on the last chunk
   - the existing done line with count and seconds
   Full-text search, attachments, and tapbacks already have
   start/done lines. Do not add extra chatter there unless a phase
   can sit silent for a long time with no count.
6. **Dedupe after import fills missing keys only.**
   `dedupe_cross_source` calls `fill_missing_content_keys` instead
   of `recompute_all_content_keys`. Exact and near flagging stay
   the same. Promote already wrote keys for rows it just inserted,
   so a following dedupe pass should mostly flag, not hash again.
   `recompute_all_content_keys` remains for an explicit rebuild
   later (formula change or admin). It is not the default after a
   promote that just filled keys.
7. **Server logs only.** No Import screen or `/v1/imports` progress
   payload in this change.

## Architecture

```text
import_jsonl_files_on_conn
  → staging (unchanged commit-every-50-files)
  → promote_append  [one transaction]
       messages in 50k id windows
       attachments / tapbacks
       FTS bulk fill
       fill_missing_content_keys
         SELECT rows without a key
         spawn_blocking + Rayon hash
         multi-row INSERT into _content_keys
         UPDATE messages FROM _content_keys
       COMMIT
  → (optional) dedupe_cross_source
       fill_missing_content_keys   ← was recompute_all
       flag exact content_key
       flag near-time
```

`--reset-demo` still imports three sources with `skip_dedupe: true`,
then `run_dedupe`. After this change that last pass should write
near-zero new keys if each promote already filled them, then run
the two flag passes.

## Files

| Path | Change |
|------|--------|
| `crates/vault/server/src/dedupe.rs` | Parallel hash; batched `_content_keys` inserts; `dedupe_cross_source` fills missing keys only; progress prints |
| `crates/vault/server/Cargo.toml` | Add `rayon` (already used in the workspace) |
| `crates/vault/server/src/import/promote.rs` | Keep 50k windows; content-key phase still calls `fill_missing_content_keys` |

Leave JSONL staging, `process-assets`, and the web Import screen
alone.

## Testing

Server crate tests (`cargo test -p message-vault-server --lib dedupe`).

- Parallel hash of a small row set matches the serial helper
  (same fingerprints, same id order).
- Existing cross-source exact/near/priority tests still pass.
  They go through fill + flag.
- A second `fill_missing_content_keys` on rows that already have
  keys writes **zero** new keys, so the demo path does not hash
  twice.
- Hash-task failure is an error from `spawn_blocking`. Promote
  still uses one transaction, so that error rolls back the whole
  promote. No extra engine harness for this.

No Playwright. This is server stdout and SQL.

## Rollout

Land on the same branch as the content-key batching work. Rebuild
the vault before judging `--reset-demo`: a running debug binary
will not pick this up. Confirm on Postgres first (the path that
showed the stuck line), then a smaller SQLite import so both
engines stay on the shared SQL.
