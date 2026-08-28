# Larger Postgres staging chunks — 2026-08-28

Raise the JSONL → `staging_*` insert chunk on Postgres to 1000
rows. SQLite stays at the 999-bind cap. This spec records decisions
from the 2026-08-27 design conversation. It is not an implementation
plan.

This is a follow-on to
`docs/superpowers/specs/2026-08-27-import-staging-batch-design.md`.
That work batched staging writes and used one chunk size on both
engines (about 55 message rows).

## Goal

The first import step should not spend most of its remaining time on
Docker round-trips for ~55-row inserts. CLI import, website upload,
and `--reset-demo` all use this path.

Judge the win from a real `--reset-demo` log: the first-source
`[N/334] … (Xs)` lines. No pass/fail time cap in tests.

## Current product

`max_rows_for_bind_limit` divides 999 by the column count. A message
row has 18 columns, so each `INSERT` holds 55 rows. A ~543,000-row
iMessage source therefore issues about 9,900 message statements,
plus attachment and tapback chunks. Each statement is vault → TCP
`127.0.0.1:5432` → `docker-proxy` → the container.

Postgres allows 65,535 bind parameters. The statement shape is
already `INSERT … VALUES (…), (…), …`. Only the row count is shared
with SQLite.

## Non-goals

- Postgres `COPY` or `UNNEST`.
- `UNLOGGED` staging tables or dropping extra indexes for the load.
- Changing JSONL parsing, attachment file copy, or `process-assets`.
- Changing promote, `ANALYZE`, or `VACUUM`.
- Changing the 50-file commit cadence.
- Import UI or `/v1/imports` progress fields.

## Decisions

1. **Same `INSERT … VALUES` on both engines.** Only the chunk size
   changes. sqlx Any still builds a new statement string per chunk.

2. **Postgres cap is 1000 rows**, still limited by 65,535 binds
   (`columns × rows ≤ 65_535`). Message, attachment, and tapback
   chunks all use that helper. 1000 is where round-trips flatten
   out without a half-megabyte statement. Raise to 2000 only if a
   later `--reset-demo` log is still dominated by those progress
   lines.

3. **SQLite stays at 999 binds** (55 / 99 / 166 rows for 18 / 10 /
   6 columns).

4. **A failed chunk still fails the import.** No retry of a
   half-written chunk.

5. **Server logs stay as they are.** Keep
   `[N/total] name  msgs=… attachments=… assets_copied=… missing=… (Xs)`.

## Architecture

```text
max_rows_for_bind_limit(engine, columns)
  SQLite:   999 / columns
  Postgres: min(65_535 / columns, 1000)

import_file_to_staging
  … unchanged until flush …
  for each message chunk (55 on SQLite, 1000 on Postgres):
    INSERT staging_messages … RETURNING id, sort_order
    INSERT staging_attachments …   ← same cap
    INSERT staging_tapbacks …      ← same cap
```

## Files

| Path | Change |
|------|--------|
| `crates/vault/server/src/db/sql.rs` | Engine-aware chunk size; `POSTGRES_INSERT_MAX_ROWS = 1000` |
| `crates/vault/server/src/import/staging.rs` | Pass `engine_of(tx)` into the helper |
| `CHANGELOG.md` | One dated Changed note |

Leave promote, dialect analyze/vacuum, `process-assets`, and `web/`
alone.

## Testing

Server crate tests (`cargo test -p message-vault-server --lib`).

- Helper: SQLite 18/10/6/0 columns still 55/99/166/0.
- Helper: Postgres 18/10/6 columns are 1000; 0 columns is 0; 70
  columns is 936 (bind cap wins).
- Existing import tests keep passing; they go through the new
  write path. The 56-message chunk-boundary test still covers
  SQLite splits and child-row matching.

No Playwright. No time cap. A manual
`./scripts/run-vault-pg-dev.sh --reset-demo` (prefer `--release`)
is how the first-source `[N/334] … (Xs)` line is judged.

## Rollout

New branch from current `main`. Rebuild the vault before judging
`--reset-demo`. Confirm on Postgres first, then SQLite so both
engines still use the same insert shape.
