# Group identical Import Errors — 2026-08-27

Collapse repeated Import Error rows that share the same cause into one
table row with a file count. This spec records decisions from the
2026-08-27 design conversation for [issue 202](https://github.com/bitrealm-io/message-vault/issues/202).
It is not an implementation plan.

## Goal

After a desktop Import finishes, the Import Errors table should read as
one row per distinct error, not one row per conversation file.

When every file fails for the same reason, the table is one row that
says how many files hit that error. Expanding that row shows the full
reason and the filenames. Unique errors still show the filename, as they
do today.

Settings import history uses the same summary panel, so it shows the
same grouped table without a second code path.

## Current product

The desktop Import job records every failure as it arrives. Each
recorded issue is one object with four fields:

- `kind` — `error` or `skip`
- `step` — `parse`, `convert`, or `upload`
- `item` — usually the conversation `.jsonl` filename
- `reason` — the error text

The job hook (`web/src/screens/import/useImportJob.ts`) appends each
Tauri `onIssue` event as-is. When the run finishes, that full list is
shown on the Import summary and posted to
`/v1/imports/{id}/complete`. Settings import history loads the stored
list and passes it into the same summary panel
(`web/src/components/import/ImportSummaryPanel.tsx`).

The table (`web/src/components/import/VirtualizedImportIssuesTable.tsx`)
draws one virtualized row per issue. Columns are Parse File (`item`),
Step (`step`), and Error Message (`reason`). Clicking a row expands or
collapses the reason text. There is no grouping.

A 2026-08-27 iPhone backup import made this unreadable. Staging folder
`staging-iphone-ios-260827-003815`. `vault-push-report.json`: 681
conversations failed, 0 succeeded. Every row showed:

```text
import 5 source mismatch (session=imessage-ios, request=imessage)
```

`import 5` is the vault import session id (`vault_imports.id`), not a
count of five mismatches. The same check failed 681 times. The
conversations themselves are not duplicates. That run stored zero
messages. The table repeated the same `reason` once per filename.

## Non-goals

- Fix the session/request source slug mismatch that produced the 681
  identical rows. The desktop job opens `/v1/imports` with
  `form.source` (`imessage-ios`, the Platform method id). `vault-push`
  sends `export.source` from the JSONL header (`imessage`, written by
  `imessage-ir-exporter`). The server requires those strings to match
  (`require_reusable_import` in
  `crates/vault/server/src/db/vault_imports.rs`). WhatsApp method ids
  (`whatsapp-android` / `whatsapp-ios`) can hit the same class of
  mismatch. That is a separate issue.
- Change how issues are recorded, logged, counted, or stored. One
  issue per file stays in the job hook, the import log, message
  counts, and Settings history.
- Change `vault-push` or the Tauri `onIssue` events.
- Add Playwright coverage. Import is desktop-only. Tests are Vitest.

## Decisions

1. **Group only when drawing the table.** The stored `issues` array
   stays one record per file. Grouping is a display step inside the
   table, from a pure helper the table calls.
2. **A group is the same `kind` plus the same `step` plus the same
   `reason`.** An `error` and a `skip` with the same step and reason
   stay two rows. The table already treats `kind` as part of the
   issue. Mixing them under one count would hide that difference.
3. **First-seen order.** A group appears where that `kind` + `step` +
   `reason` first showed up. Filenames inside a group stay in the
   order those issues were recorded. The table still reads like the
   original list, with repeats collapsed. Sorting by count would hide
   a unique parse error that happened first.
4. **One unique error keeps today’s row.** Parse File shows the
   filename. Collapsed Error Message still shows two lines. Expanding
   it still shows the full reason only. No extra filename list.
5. **Two or more copies become one row.** Parse File shows `N files`
   (for example `681 files`). Step is unchanged. Collapsed Error
   Message still shows two lines of the shared reason. Expanding the
   row shows the full reason, then a short scrollable list of the
   filenames in that group. Those names stay inside the expanded
   cell. They do not become extra virtualized table rows.
6. **The filename list scrolls on its own.** A 681-file group must
   not stretch the row down the page. The list shows at most six
   filenames at a time, then scrolls inside the expanded cell. Do not
   drop duplicate filenames if the same `item` appears more than once
   in the stored list.
7. **Settings history is automatic.** The history view already uses
   `ImportSummaryPanel`. Grouping lives in the table (or the helper
   the table calls), so history gets the same rows.
8. **Helper sentence mentions grouping.** The muted line under Import
   Errors changes to: “Identical errors are grouped. Error messages
   show two lines. Click a row to expand the full message and the
   file list.”

## Architecture

```text
Tauri onIssue
  → useImportJob appends ImportIssue (unchanged)
  → summary.issues / POST /v1/imports/{id}/complete (unchanged)
  → ImportSummaryPanel passes summary.issues (unchanged)
  → VirtualizedImportIssuesTable calls groupImportIssues
  → table virtualizes groups, not files
```

`useImportJob` still appends every Tauri `onIssue` event. Completing
an import still posts the full list. Settings import history still
loads that full list and passes it into `ImportSummaryPanel`. None of
that recording path changes.

A new pure function, `groupImportIssues`, sits next to the table
(`web/src/components/import/groupImportIssues.ts`). Input is the
stored `ImportIssue[]` list. Output is groups in first-seen order.

A group is created the first time a `kind` + `step` + `reason`
combination appears. Later issues with the same three fields add
their `item` (the `.jsonl` filename) to that group’s `items` list,
also in first-seen order.

Each group is:

- `kind` — `error` or `skip`
- `step` — `parse`, `convert`, or `upload`
- `reason` — the error text
- `items` — the filenames in that group

`VirtualizedImportIssuesTable` still receives the raw `issues` array.
It calls the helper when rendering, then virtualizes **groups**, not
files. 681 identical failures become one virtualized row.

`ImportSummaryPanel` keeps passing `summary.issues` through unchanged.
The only panel change is the muted sentence under Import Errors.

The table columns stay Parse File, Step, and Error Message. Clicking
a row still expands or collapses that one row.

`aria-rowcount` counts groups, not raw files. A grouped row’s
accessible name says it covers N files, not one filename.

Empty `issues` still hides the Import Errors section. That is current
behavior. If the helper gets an empty list, it returns no groups. Bad
or missing fields on an old stored issue are treated as part of the
group key as-is, so history rows still render.

## Files

| Path | Change |
|------|--------|
| `web/src/components/import/groupImportIssues.ts` | New pure helper |
| `web/src/components/import/groupImportIssues.test.ts` | Helper tests |
| `web/src/components/import/VirtualizedImportIssuesTable.tsx` | Virtualize groups; unique vs `N files`; expanded filename list |
| `web/src/components/import/VirtualizedImportIssuesTable.test.tsx` | Table tests |
| `web/src/components/import/ImportSummaryPanel.tsx` | Helper sentence copy only |
| `web/src/components/import/ImportSummaryPanel.test.tsx` | Assert the helper sentence mentions grouping |

Leave `useImportJob` and `vault-push` alone.

## Testing

Import is desktop-only. Cover this with Vitest, not Playwright against
Vite.

Helper:

- Three issues with the same `kind`, `step`, and `reason` become one
  group with three filenames.
- Two different reasons stay two groups.
- An `error` and a `skip` with the same step and reason stay two
  groups.
- Group order and filename order stay first-seen.

Table:

- A unique file still shows its name in Parse File.
- A group shows `N files`.
- Expanding that group shows the full reason and those filenames.
- A unique row still expands to the reason only.

Panel:

- Existing tests stay.
- Add a check that the helper sentence mentions grouping.
