# Import History Details Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make import history rows expand in place, show uploaded bytes instead of duration, and keep large import-error lists responsive with row virtualization.

**Architecture:** `StorageSection` owns the selected import id and inserts the detail card as a second table row immediately after the selected history row. `ImportSummaryPanel` delegates issue rendering to a focused `VirtualizedImportIssuesTable` component backed by `@tanstack/react-virtual`. Existing import detail data and exact reporting counters remain unchanged.

**Tech Stack:** React 19, TypeScript, Tailwind CSS, `@tanstack/react-virtual`, Vite.

## Global Constraints

- The Import history columns are Date, Import type, Messages, Attachments, and Uploaded size.
- Uploaded size displays `vault_imports.bytes_uploaded`; it does not claim to be the total source attachment size.
- Duration appears only inside the expanded detail card as Parse, Convert, Upload, and Total.
- Import Errors shows approximately 15–20 rows at once and renders only visible rows.
- History rows remain operable with mouse, Enter, and Space.
- Do not add dependencies.

---

### Task 1: Add uploaded bytes to the history row model

**Files:**
- Modify: `web/src/screens/settings/StorageSection.tsx`

**Interfaces:**
- Consumes: `GET /v1/imports`, whose row objects already contain `bytes_uploaded`.
- Produces: `ImportRow.bytes_uploaded: number`.

- [ ] **Step 1: Update the history row type**

Add `bytes_uploaded: number` to `ImportRow`. Remove the unused `duration_ms` field from the history-row interface.

- [ ] **Step 2: Replace the history Duration column**

Render a right-aligned `Uploaded size` column with `formatBytes(row.bytes_uploaded)`. Keep the detailed Parse, Convert, Upload, and Total durations in the expanded detail card.

- [ ] **Step 3: Run the TypeScript production build**

Run: `cd web && npm run build`

Expected: TypeScript and Vite finish successfully.

### Task 2: Expand details directly below the selected history row

**Files:**
- Modify: `web/src/screens/settings/StorageSection.tsx`

**Interfaces:**
- Consumes: `selectedImportId`, `selectedImport`, `selectedImportLoading`, and `selectedImportError`.
- Produces: `toggleImportDetail(importId: number): void` and one inline detail `<tr>` with `colSpan={5}`.

- [ ] **Step 1: Make selection toggleable**

Replace the one-way open function with:

```ts
const toggleImportDetail = (importId: number) => {
  if (selectedImportId === importId) {
    setSelectedImportId(null);
    setSelectedImport(null);
    setSelectedImportLoading(false);
    setSelectedImportError("");
    return;
  }
  setSelectedImportId(importId);
  setSelectedImport(null);
  setSelectedImportError("");
};
```

- [ ] **Step 2: Render each history row with an optional detail sibling**

Map history entries to keyed fragments. Render the clickable summary `<tr>` first. When its id is selected, render a second `<tr>` immediately afterward. The detail row contains loading, error, and loaded states inside a full-width cell.

- [ ] **Step 3: Keep keyboard and screen-reader behavior**

Use Enter and Space to call the same toggle function. Add `aria-expanded` and `aria-controls` to the summary row. Give the detail container a stable id derived from the import id.

- [ ] **Step 4: Remove the detached detail card**

Delete the detail block that currently renders after the entire table. Keep the close button in the inline card so users can collapse without returning to the summary row.

- [ ] **Step 5: Run the TypeScript production build**

Run: `cd web && npm run build`

Expected: TypeScript and Vite finish successfully.

### Task 3: Virtualize the Import Errors table

**Files:**
- Create: `web/src/components/import/VirtualizedImportIssuesTable.tsx`
- Modify: `web/src/components/import/ImportSummaryPanel.tsx`

**Interfaces:**
- Consumes: `issues: ImportIssue[]`.
- Produces: `VirtualizedImportIssuesTable({ issues }: { issues: ImportIssue[] })`.

- [ ] **Step 1: Create the virtualized issue table**

Use `useVirtualizer` with a fixed 40-pixel row estimate and an overscan of 5. Use a scrollable parent ref. The viewport height is `min(issues.length, 18) * 40`, capped at 720 pixels. Render a sticky header above an absolutely positioned virtual row layer.

Each row uses a three-column CSS grid:

```tsx
grid-template-columns: minmax(10rem, 0.9fr) minmax(6rem, 0.35fr) minmax(18rem, 1.75fr)
```

Apply `role="table"`, `role="rowgroup"`, `role="row"`, `role="columnheader"`, and `role="cell"` so the virtual grid retains table semantics.

- [ ] **Step 2: Replace the eager issue-row map**

Keep the `Import Errors` heading in `ImportSummaryPanel`. Replace the current full `<table>` with `<VirtualizedImportIssuesTable issues={summary.issues} />`.

- [ ] **Step 3: Verify long and short lists**

Use the existing import with six errors to verify compact height. Temporarily inspect with at least 30 entries in development to confirm that scrolling renders a bounded number of rows and the header remains visible.

- [ ] **Step 4: Run frontend checks**

Run: `cd web && npm run build`

Expected: TypeScript and Vite finish successfully.

Run IDE diagnostics for:

- `web/src/components/import/VirtualizedImportIssuesTable.tsx`
- `web/src/components/import/ImportSummaryPanel.tsx`
- `web/src/screens/settings/StorageSection.tsx`

Expected: no new diagnostics.

### Task 4: Verify the complete reporting and history flow

**Files:**
- Review: `docs/superpowers/specs/2026-08-11-import-progress-summary-design.md`
- Review: all files changed by Tasks 1–3.

**Interfaces:**
- Consumes: exact reporting fields persisted by the import completion flow.
- Produces: verified import history behavior.

- [ ] **Step 1: Check the reporting requirements**

Confirm the detail still displays:

```text
parsed = attempted + not attempted
attempted = new + deduped + failed
```

Confirm the error table continues to show Parse File, Step, and Error Message.

- [ ] **Step 2: Check history behavior**

Confirm Uploaded size uses `bytes_uploaded`. Confirm duration is absent from the history summary and present in the expanded detail. Confirm only one row can be expanded at a time.

- [ ] **Step 3: Run final verification**

Run: `cd web && npm run build`

Expected: exit code 0.

Run: `git diff --check`

Expected: no whitespace errors.
