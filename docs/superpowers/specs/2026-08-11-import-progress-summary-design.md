# Import progress summary and stage timings

**Date:** 2026-08-11  
**Status:** Draft — pending user review  
**Scope:** Import Messages UI (`web/src/screens/ImportScreen.tsx`), Tauri extract/push progress events, vault import session API and SQLite (`vault_imports`, new issues table)

## Problem

During Import Messages, the step list mostly shows vague status text such as “Extraction complete.” Real progress appears only in a noisy log (`files N/M … total time=…`), where `total time` is the wall clock for a push progress chunk, not the whole import. After the run, there is no user-readable summary of totals, errors/skips, or where time was spent. Settings → Storage → Import history lists session counts but cannot reopen a diagnostic summary.

## Goals

- Show live per-step counts (`done / total`) on the Import Messages step list while a run is in progress.
- Show one overall elapsed wall clock during the run, plus per-active-step elapsed when useful.
- After finish (or failure), show a Summary on the same screen: step totals, **per-stage and total timings**, and an **Errors & skips** list only (no full success file list).
- Persist that summary, stage timings, total duration, and error/skip rows in the vault database so Import history can reopen the same view later.
- Start the vault import session at the beginning of the GUI import so one `vault_imports` row covers parse, convert, and upload.

## Non-goals

- Persisting or displaying a full successful-file log.
- Driving live `xxx/yyy` from SQLite (progress stays in process memory / Tauri events).
- Resuming a mid-import run after app restart.
- Changing extract → JSONL → push architecture (still stage files, then upload).
- Reworking CLI vault-push progress UX beyond what is needed so the GUI can receive structured counts and stage timings.

## Decisions

| Topic | Choice |
|--------|--------|
| Primary UI | Step list with live counts; Summary after run (not the raw log as the main surface) |
| Per-file detail | Totals + **errors/skips only** |
| Persistence | Light: summary + timings + issue rows in DB; live counters in memory only |
| Architecture | Extend `vault_imports` + child `vault_import_issues`; structured progress events from extract/push |
| Timings | Store **parse_ms**, **convert_ms**, **upload_ms**, and **duration_ms** (total) in the database with the session |
| Import session start | `POST /v1/imports` when the GUI import starts (before extract), not only before push |

## Approach

Emit structured progress from the desktop extract and push jobs so the Import screen can update each step’s `done/total` and measure stage wall times. Keep an in-memory list of errors and skips. On completion, POST an enriched complete payload that writes session summary fields (including stage and total timings) and issue rows. Reuse the same Summary layout when opening a history row via `GET /v1/imports/{id}`.

## Live Import UI

Three steps (labels may match today’s copy):

1. **Parse backup** — `messages processed / total` while parsing; final total when done.
2. **Convert attachments** — `files done / total` when convert/compress runs; if the job copies or skips media transforms, show a short status (for example `Copied with extract` / `Skipped`) instead of a fake counter.
3. **Upload to vault** — `files done / total` (conversation payload files uploaded).

While running:

- Overall **Elapsed** wall clock from Import start.
- Optional per-step elapsed on the active step.
- Indeterminate or coarse activity indicator is fine; exact percent is secondary to `done/total`.
- Cancel remains available.
- Do not treat the free-text log as the primary progress surface (optional “details” may remain for debugging later; not required for v1).

When finished or failed: switch to **Summary** on the same screen — status, step totals, stage timings + total time, Errors & skips (item + reason). Successful paths are not listed.

## Timing model

Measure wall-clock milliseconds on the client (or job boundaries) for:

| Field | Meaning |
|--------|---------|
| `parse_ms` | Time in Parse backup |
| `convert_ms` | Time in Convert attachments; `0` or null if that stage did not run as a distinct phase |
| `upload_ms` | Time in Upload to vault |
| `duration_ms` | Entire import from user Start until complete/fail (may be slightly larger than the sum of stages) |

Do **not** use vault-push chunk `total time=` as the import total.

**Both the live Summary UI and the vault database** store these four values so Import history and later diagnosis can see whether convert or upload dominated a run.

## Persistence (SQLite)

### Extend `vault_imports`

Keep existing columns (`source`, `tool`, `mode`, `status`, `started_at`, `finished_at`, `message_count`, `attachment_count`, `bytes_uploaded`).

Add at least:

- `duration_ms` INTEGER — total import wall time (nullable for older rows).
- `parse_ms`, `convert_ms`, `upload_ms` INTEGER — per-stage wall times (nullable).
- `summary_json` TEXT — short structured totals/notes for step display (counts, convert status text, etc.).

Timing columns on `vault_imports` are the source of truth for persisted timings (not only UI state, and not only nested inside JSON). `summary_json` may echo display-oriented fields; it must not be the only place timings are stored.

Schema changes follow the project’s existing wipe/no-migration policy for local vault DBs when that is the established pattern for this schema area; document any required wipe in the implementation plan.

### New `vault_import_issues`

One row per error or skip only:

- `id`, `import_id` (FK → `vault_imports`, cascade delete)
- `kind` — `error` | `skip`
- `step` — `parse` | `convert` | `upload` (or equivalent)
- `item` — file or logical item id/path (short)
- `reason` — human-readable cause
- `created_at`

No rows for successful files.

## API

- `POST /v1/imports` — unchanged shape; GUI calls this at **import start**.
- `POST /v1/imports/{id}/complete` — accept `ok`, message/attachment/bytes counts (as today), plus `duration_ms`, `parse_ms`, `convert_ms`, `upload_ms`, `summary` (object), and `issues` (array of `{ kind, step, item, reason }`). Persist timings on the session row and insert issue rows. On failure paths, still persist timings and issues when available and mark status failed.
- `GET /v1/imports` — list may include `duration_ms` for the history table (optional column).
- `GET /v1/imports/{id}` — new detail: session fields including timings + `summary` + `issues[]`.

CLI/single-POST import paths that already create a `vault_imports` row should leave new timing/issue fields null/empty when not supplied.

## Import history UI

Settings → Storage → Import history keeps the existing table. Make each row openable to a detail view that reuses the Summary layout (step totals, stage + total timings, Errors & skips) from `GET /v1/imports/{id}`.

## Progress events (desktop)

Replace reliance on free-text log lines for primary UI with structured events the web layer can apply to steps, for example:

- step start / progress (`step`, `done`, `total`) / step done
- issue (`kind`, `step`, `item`, `reason`)
- job finished with enough info to finalize counts

Parse already reports message progress at a finer grain (for example every 1,000 messages); surface that under the Parse step. Upload should expose file `done/total` without presenting chunk wall time as total import time.

## Out of scope / follow-ups

- Optional full raw log export for support.
- Server-side live progress polling.
- ETA calculation.
- Changing Import history list columns beyond optional duration (detail view is the main addition).

## Testing

- Server: complete import persists timings and issues; get-by-id returns them; list still works for rows without timings.
- GUI (manual or automated where practical): live counts update per step; Summary shows stage + total times; history detail matches what was just imported; failed convert/upload appears under Errors & skips only.
- Confirm push chunk `total time` is not stored as `duration_ms`.
