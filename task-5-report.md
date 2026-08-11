Task 5 report

- Updated Errors & skips in `ImportSummaryPanel.tsx` to show kind, step, item, and reason (e.g. `Skip · convert · photo.heic — convert failed`).
- Added `formatIssueKind` to capitalize `error`/`skip` labels for display.
- No Tauri event changes; `extract:progress` / `extract:issue` from Task 4 unchanged.
- Verification: `cd web && npx tsc --noEmit` (pass).
