# Group Import Errors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw one Import Errors row per distinct `kind` + `step` + `reason`, with `N files` and an expanded filename list when several conversation files failed the same way.

**Architecture:** Keep recording one issue per file. A pure helper, `groupImportIssues`, collapses the stored list at render time. `VirtualizedImportIssuesTable` virtualizes those groups. `ImportSummaryPanel` only changes the helper sentence. Settings import history already uses that panel, so it picks grouping up automatically.

**Tech Stack:** React 19 + TypeScript in `web/`, Vitest + Testing Library, `@tanstack/react-virtual`.

**Spec:** `docs/superpowers/specs/2026-08-27-group-import-errors-design.md`

## Global Constraints

- Group only when drawing the table. Do not change `useImportJob`, `vault-push`, Tauri `onIssue` events, or stored history payloads.
- A group is the same `kind` plus the same `step` plus the same `reason`.
- Group order and filename order are first-seen. Do not sort by count. Do not drop duplicate filenames.
- Unique group (`items.length === 1`): Parse File shows the filename. Expand shows the reason only.
- Two or more files: Parse File shows `N files`. Expand shows the full reason plus a scrollable filename list inside that cell (at most six names visible).
- Do not turn filenames into extra virtualized table rows.
- Helper sentence under Import Errors: “Identical errors are grouped. Error messages show two lines. Click a row to expand the full message and the file list.”
- Do not fix the session/request source slug mismatch (`imessage-ios` vs `imessage`).
- Import is desktop-only. Prove this with Vitest, not Playwright against Vite.
- Prefer a real fix over `biome-ignore`. Prefix unused bindings with `_`.
- Never commit to `main`. Work on `fix/group-import-errors`.
- Product version files stay at the current lockstep value. Do not bump versions.
- Do not commit `docs/package.json` or `docs/package-lock.json` if they are dirty from an unrelated install.

## File map

| File | Responsibility |
|---|---|
| `web/src/components/import/groupImportIssues.ts` | Pure helper: `ImportIssue[]` → groups in first-seen order |
| `web/src/components/import/groupImportIssues.test.ts` | Helper tests |
| `web/src/components/import/VirtualizedImportIssuesTable.tsx` | Virtualize groups; unique vs `N files`; expanded filename list |
| `web/src/components/import/VirtualizedImportIssuesTable.test.tsx` | Table tests |
| `web/src/components/import/ImportSummaryPanel.tsx` | Helper sentence copy only |
| `web/src/components/import/ImportSummaryPanel.test.tsx` | Assert the helper sentence mentions grouping |
| `CHANGELOG.md` | Unreleased Fixed note dated 2026-08-27 |

Out of scope files: `web/src/screens/import/useImportJob.ts`, `src-tauri/**`, `crates/**`, Playwright specs.

---

### Task 0: Branch and record the plan

**Files:**
- Create: this plan at `docs/superpowers/plans/2026-08-27-group-import-errors.md`
- Existing: `docs/superpowers/specs/2026-08-27-group-import-errors-design.md`

**Interfaces:**
- Consumes: locked spec on disk
- Produces: git branch `fix/group-import-errors` with spec + plan committed

- [ ] **Step 1: Confirm or create the branch**

```bash
cd /home/mbeisser/repo/message-vault
git fetch
git branch --show-current
```

If the current branch is `main`, stop and create `fix/group-import-errors` from the commit that already has the spec:

```bash
git checkout -b fix/group-import-errors
```

If the current branch is `docs/group-import-errors-design` (or already has the spec), rename or branch from it:

```bash
git checkout -b fix/group-import-errors
```

Expected: `git branch --show-current` prints `fix/group-import-errors`.

- [ ] **Step 2: Commit this plan** (skip if `git status` already shows it committed)

```bash
git add docs/superpowers/plans/2026-08-27-group-import-errors.md
git commit -m "$(cat <<'EOF'
docs: add import-errors grouping plan

The spec locks display-only grouping. This plan is the TDD sequence
for the helper, table, and copy.

Related to #202
EOF
)"
```

Do not stage `docs/package.json` or `docs/package-lock.json`.

---

### Task 1: `groupImportIssues` helper

**Files:**
- Create: `web/src/components/import/groupImportIssues.ts`
- Test: `web/src/components/import/groupImportIssues.test.ts`

**Interfaces:**
- Consumes: `ImportIssue` from `web/src/components/import/ImportSummaryPanel.tsx` (`kind`, `step`, `item`, `reason`: all `string`)
- Produces:

```ts
export type ImportIssueGroup = {
  kind: string;
  step: string;
  reason: string;
  items: string[];
};

export function groupImportIssues(issues: ImportIssue[]): ImportIssueGroup[]
```

A group is created the first time a `kind` + `step` + `reason` combination appears. Later issues with the same three fields append `item` onto that group’s `items`. Empty input returns `[]`. Missing or odd field values are part of the key as-is.

- [ ] **Step 1: Write the failing tests**

Create `web/src/components/import/groupImportIssues.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { ImportIssue } from "./ImportSummaryPanel";
import { groupImportIssues } from "./groupImportIssues";

function issue(
  partial: Partial<ImportIssue> & Pick<ImportIssue, "item" | "reason">,
): ImportIssue {
  return {
    kind: "error",
    step: "upload",
    ...partial,
  };
}

describe("groupImportIssues", () => {
  it("returns no groups for an empty list", () => {
    expect(groupImportIssues([])).toEqual([]);
  });

  it("collapses three identical reasons into one group with three items", () => {
    const groups = groupImportIssues([
      issue({ item: "a.jsonl", reason: "source mismatch" }),
      issue({ item: "b.jsonl", reason: "source mismatch" }),
      issue({ item: "c.jsonl", reason: "source mismatch" }),
    ]);
    expect(groups).toEqual([
      {
        kind: "error",
        step: "upload",
        reason: "source mismatch",
        items: ["a.jsonl", "b.jsonl", "c.jsonl"],
      },
    ]);
  });

  it("keeps two different reasons as two groups", () => {
    const groups = groupImportIssues([
      issue({ item: "a.jsonl", reason: "source mismatch" }),
      issue({ item: "b.jsonl", reason: "HTTP 500 from vault" }),
    ]);
    expect(groups).toHaveLength(2);
    expect(groups[0]?.reason).toBe("source mismatch");
    expect(groups[0]?.items).toEqual(["a.jsonl"]);
    expect(groups[1]?.reason).toBe("HTTP 500 from vault");
    expect(groups[1]?.items).toEqual(["b.jsonl"]);
  });

  it("keeps error and skip with the same step and reason as two groups", () => {
    const groups = groupImportIssues([
      issue({ kind: "error", item: "a.jsonl", reason: "source mismatch" }),
      issue({ kind: "skip", item: "b.jsonl", reason: "source mismatch" }),
    ]);
    expect(groups).toHaveLength(2);
    expect(groups[0]?.kind).toBe("error");
    expect(groups[0]?.items).toEqual(["a.jsonl"]);
    expect(groups[1]?.kind).toBe("skip");
    expect(groups[1]?.items).toEqual(["b.jsonl"]);
  });

  it("keeps first-seen group order and filename order", () => {
    const groups = groupImportIssues([
      issue({ step: "upload", item: "b.jsonl", reason: "shared" }),
      issue({ step: "parse", item: "early.jsonl", reason: "unique parse" }),
      issue({ step: "upload", item: "c.jsonl", reason: "shared" }),
    ]);
    expect(groups.map((group) => group.reason)).toEqual(["shared", "unique parse"]);
    expect(groups[0]?.items).toEqual(["b.jsonl", "c.jsonl"]);
    expect(groups[1]?.items).toEqual(["early.jsonl"]);
  });

  it("keeps a duplicate filename when the stored list repeats it", () => {
    const groups = groupImportIssues([
      issue({ item: "same.jsonl", reason: "shared" }),
      issue({ item: "same.jsonl", reason: "shared" }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.items).toEqual(["same.jsonl", "same.jsonl"]);
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/groupImportIssues.test.ts
```

Expected: FAIL because `./groupImportIssues` cannot be imported, or `groupImportIssues` is not defined.

- [ ] **Step 3: Write the minimal helper**

Create `web/src/components/import/groupImportIssues.ts`:

```ts
import type { ImportIssue } from "./ImportSummaryPanel";

export type ImportIssueGroup = {
  kind: string;
  step: string;
  reason: string;
  items: string[];
};

export function groupImportIssues(issues: ImportIssue[]): ImportIssueGroup[] {
  const groups: ImportIssueGroup[] = [];
  const indexByKey = new Map<string, number>();

  for (const issue of issues) {
    const key = `${issue.kind}\0${issue.step}\0${issue.reason}`;
    const existing = indexByKey.get(key);
    if (existing == null) {
      indexByKey.set(key, groups.length);
      groups.push({
        kind: issue.kind,
        step: issue.step,
        reason: issue.reason,
        items: [issue.item],
      });
      continue;
    }
    const group = groups[existing];
    if (group) {
      group.items.push(issue.item);
    }
  }

  return groups;
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/groupImportIssues.test.ts
```

Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add web/src/components/import/groupImportIssues.ts web/src/components/import/groupImportIssues.test.ts
git commit -m "$(cat <<'EOF'
feat(import): group identical import issues

The errors table needs one row per shared cause. This helper
collapses stored per-file issues at render time.

Related to #202
EOF
)"
```

---

### Task 2: Virtualize groups in the errors table

**Files:**
- Modify: `web/src/components/import/VirtualizedImportIssuesTable.tsx`
- Test: `web/src/components/import/VirtualizedImportIssuesTable.test.tsx`

**Interfaces:**
- Consumes: `groupImportIssues` and `ImportIssueGroup` from `./groupImportIssues`; still accepts `{ issues: ImportIssue[] }`
- Produces: table rows equal to group count. Unique Parse File = `items[0]`. Grouped Parse File = `${items.length} files`. Grouped expanded cell lists every `items` entry under the reason. Unique expanded cell lists the reason only.

Mock `@tanstack/react-virtual` in the test file so jsdom does not have to size the scroll element. The mock must still expose `getVirtualItems`, `getTotalSize`, `measure`, and `measureElement`.

- [ ] **Step 1: Write the failing table tests**

Create `web/src/components/import/VirtualizedImportIssuesTable.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ImportIssue } from "./ImportSummaryPanel";
import VirtualizedImportIssuesTable from "./VirtualizedImportIssuesTable";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        key: index,
        start: index * 56,
        size: 56,
        end: (index + 1) * 56,
      })),
    getTotalSize: () => count * 56,
    measure: () => {},
    measureElement: () => {},
  }),
}));

afterEach(() => {
  cleanup();
});

function issue(
  partial: Partial<ImportIssue> & Pick<ImportIssue, "item" | "reason">,
): ImportIssue {
  return {
    kind: "error",
    step: "upload",
    ...partial,
  };
}

describe("VirtualizedImportIssuesTable", () => {
  it("shows the filename for a unique issue", () => {
    render(
      <VirtualizedImportIssuesTable
        issues={[issue({ item: "chat.jsonl", reason: "HTTP 500 from vault" })]}
      />,
    );
    expect(screen.getByRole("table", { name: "Import errors" })).toHaveAttribute(
      "aria-rowcount",
      "2",
    );
    expect(screen.getByText("chat.jsonl")).toBeInTheDocument();
    expect(screen.queryByText("1 files")).not.toBeInTheDocument();
  });

  it("shows N files for a group and lists names only after expand", async () => {
    const user = userEvent.setup();
    render(
      <VirtualizedImportIssuesTable
        issues={[
          issue({ item: "a.jsonl", reason: "source mismatch" }),
          issue({ item: "b.jsonl", reason: "source mismatch" }),
          issue({ item: "c.jsonl", reason: "source mismatch" }),
        ]}
      />,
    );
    expect(screen.getByRole("table", { name: "Import errors" })).toHaveAttribute(
      "aria-rowcount",
      "2",
    );
    expect(screen.getByText("3 files")).toBeInTheDocument();
    expect(screen.queryByText("a.jsonl")).not.toBeInTheDocument();

    await user.click(screen.getByRole("row", { name: /Expand error for 3 files/ }));

    expect(screen.getByRole("row", { name: /Collapse error for 3 files/ })).toBeInTheDocument();
    expect(screen.getByText("a.jsonl")).toBeInTheDocument();
    expect(screen.getByText("b.jsonl")).toBeInTheDocument();
    expect(screen.getByText("c.jsonl")).toBeInTheDocument();
    expect(screen.getByText("source mismatch")).toBeInTheDocument();
  });

  it("expands a unique row to the reason only", async () => {
    const user = userEvent.setup();
    render(
      <VirtualizedImportIssuesTable
        issues={[issue({ item: "chat.jsonl", reason: "HTTP 500 from vault" })]}
      />,
    );

    await user.click(screen.getByRole("row", { name: /Expand error for chat.jsonl/ }));

    expect(screen.getByText("HTTP 500 from vault")).toBeInTheDocument();
    expect(screen.getAllByText("chat.jsonl")).toHaveLength(1);
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/VirtualizedImportIssuesTable.test.tsx
```

Expected: FAIL. Unique file may still pass. The grouped case fails because the table still draws three rows (`aria-rowcount="4"`) and shows `a.jsonl` instead of `3 files`.

- [ ] **Step 3: Point the table at groups**

Replace `web/src/components/import/VirtualizedImportIssuesTable.tsx` with:

```tsx
import { useVirtualizer } from "@tanstack/react-virtual";
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { groupImportIssues, type ImportIssueGroup } from "./groupImportIssues";
import type { ImportIssue } from "./ImportSummaryPanel";

/** Collapsed row: file + step + two lines of error text. */
const COLLAPSED_ROW_HEIGHT = 56;
const MAX_VISIBLE_ROWS = 14;
const ISSUE_COLUMNS = "grid-cols-[minmax(0,1fr)_4.5rem_minmax(0,1.4fr)]";
const MAX_VISIBLE_FILENAMES = 6;
const FILENAME_ROW_PX = 20;

function estimateReasonHeight(reason: string): number {
  // Rough wrap estimate for the error column (~42 chars/line at this font size).
  const lines = Math.max(2, Math.ceil(reason.length / 42));
  return Math.min(220, 20 + lines * 18);
}

function estimateExpandedHeight(reason: string, fileCount: number): number {
  const reasonHeight = estimateReasonHeight(reason);
  if (fileCount <= 1) return reasonHeight;
  const visibleFiles = Math.min(fileCount, MAX_VISIBLE_FILENAMES);
  return reasonHeight + 8 + visibleFiles * FILENAME_ROW_PX;
}

function parseFileLabel(group: ImportIssueGroup): string {
  if (group.items.length === 1) {
    return group.items[0] ?? "";
  }
  return `${group.items.length} files`;
}

function rowAriaLabel(group: ImportIssueGroup, expanded: boolean): string {
  const verb = expanded ? "Collapse" : "Expand";
  return `${verb} error for ${parseFileLabel(group)}`;
}

export default function VirtualizedImportIssuesTable({ issues }: { issues: ImportIssue[] }) {
  const groups = useMemo(() => groupImportIssues(issues), [issues]);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);
  const virtualizer = useVirtualizer({
    count: groups.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) =>
      expandedIndex === index
        ? estimateExpandedHeight(
            groups[index]?.reason ?? "",
            groups[index]?.items.length ?? 0,
          )
        : COLLAPSED_ROW_HEIGHT,
    overscan: 6,
  });
  const virtualRows = virtualizer.getVirtualItems();
  const viewportHeight = Math.min(groups.length, MAX_VISIBLE_ROWS) * COLLAPSED_ROW_HEIGHT;

  useEffect(() => {
    void expandedIndex;
    void groups;
    virtualizer.measure();
  }, [expandedIndex, groups, virtualizer]);

  const toggleRow = (index: number) => {
    setExpandedIndex((current) => (current === index ? null : index));
  };

  const onRowKeyDown = (event: KeyboardEvent<HTMLDivElement>, index: number) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggleRow(index);
    }
  };

  return (
    // biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements
    <div
      role="table"
      aria-label="Import errors"
      aria-rowcount={groups.length + 1}
      className="mt-2 w-full min-w-0 max-w-full overflow-hidden rounded-lg border border-border text-left text-[0.813rem]"
    >
      {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
      <div role="rowgroup" className="border-b border-border bg-elevated">
        {/* biome-ignore lint/a11y/useFocusableInteractive: virtualized grid cannot use native table elements */}
        {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
        <div role="row" aria-rowindex={1} className={`grid ${ISSUE_COLUMNS} text-muted`}>
          {/* biome-ignore lint/a11y/useFocusableInteractive: virtualized grid cannot use native table elements */}
          {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
          <div role="columnheader" className="min-w-0 px-3 py-2 font-medium">
            Parse File
          </div>
          {/* biome-ignore lint/a11y/useFocusableInteractive: virtualized grid cannot use native table elements */}
          {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
          <div role="columnheader" className="px-3 py-2 font-medium">
            Step
          </div>
          {/* biome-ignore lint/a11y/useFocusableInteractive: virtualized grid cannot use native table elements */}
          {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
          <div role="columnheader" className="min-w-0 px-3 py-2 font-medium">
            Error Message
          </div>
        </div>
      </div>
      {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
      <div
        ref={scrollRef}
        role="rowgroup"
        className="overflow-x-hidden overflow-y-auto outline-none"
        style={{ height: viewportHeight }}
      >
        <div className="relative w-full min-w-0" style={{ height: virtualizer.getTotalSize() }}>
          {virtualRows.map((virtualRow) => {
            const group = groups[virtualRow.index];
            if (!group) return null;
            const expanded = expandedIndex === virtualRow.index;
            const fileLabel = parseFileLabel(group);
            return (
              // biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements
              <div
                key={`${group.kind}-${group.step}-${group.reason}-${virtualRow.index}`}
                data-index={virtualRow.index}
                ref={virtualizer.measureElement}
                role="row"
                tabIndex={0}
                aria-rowindex={virtualRow.index + 2}
                aria-expanded={expanded}
                aria-label={rowAriaLabel(group, expanded)}
                onClick={() => toggleRow(virtualRow.index)}
                onKeyDown={(event) => onRowKeyDown(event, virtualRow.index)}
                className={`absolute left-0 top-0 grid w-full min-w-0 cursor-pointer ${ISSUE_COLUMNS} items-start border-b border-border outline-none last:border-b-0 hover:bg-hover focus-visible:bg-hover focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent ${
                  expanded ? "bg-hover" : ""
                }`}
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
                <div
                  role="cell"
                  title={fileLabel}
                  className="min-w-0 overflow-hidden px-3 py-2 text-text"
                >
                  <span className="block truncate">{fileLabel}</span>
                </div>
                {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
                <div role="cell" className="overflow-hidden px-3 py-2 capitalize text-text">
                  <span className="block truncate">{group.step}</span>
                </div>
                {/* biome-ignore lint/a11y/useSemanticElements: virtualized grid cannot use native table elements */}
                <div
                  role="cell"
                  title={expanded ? undefined : group.reason}
                  className="min-w-0 overflow-hidden px-3 py-2 text-text"
                >
                  <span
                    className={
                      expanded
                        ? "block whitespace-pre-wrap break-words"
                        : "line-clamp-2 break-words"
                    }
                  >
                    {group.reason}
                  </span>
                  {expanded && group.items.length > 1 ? (
                    <ul
                      className="mt-2 overflow-y-auto text-muted"
                      style={{ maxHeight: MAX_VISIBLE_FILENAMES * FILENAME_ROW_PX }}
                    >
                      {group.items.map((name, fileIndex) => (
                        <li
                          key={`${name}-${String(fileIndex)}`}
                          title={name}
                          className="truncate"
                          style={{ height: FILENAME_ROW_PX }}
                        >
                          {name}
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
```

Keep the existing `biome-ignore` comments. They are for the virtualized grid, not for unused code.

- [ ] **Step 4: Run the table tests and confirm they pass**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/VirtualizedImportIssuesTable.test.tsx src/components/import/groupImportIssues.test.ts
```

Expected: PASS.

If Biome complains about import order, run:

```bash
cd /home/mbeisser/repo/message-vault/web
npm run format
```

Then re-run the same tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/import/VirtualizedImportIssuesTable.tsx web/src/components/import/VirtualizedImportIssuesTable.test.tsx
git commit -m "$(cat <<'EOF'
fix(import): group identical error table rows

Hundreds of files that fail the same way looked like hundreds of
bugs. The table now draws one row per shared cause.

Related to #202
EOF
)"
```

---

### Task 3: Helper sentence and changelog

**Files:**
- Modify: `web/src/components/import/ImportSummaryPanel.tsx` (the `<p>` under Import Errors, around line 162)
- Modify: `web/src/components/import/ImportSummaryPanel.test.tsx`
- Modify: `CHANGELOG.md` under `[Unreleased]` → `### Fixed` (add the heading if it is missing)

**Interfaces:**
- Consumes: unchanged `summary.issues` passed into `VirtualizedImportIssuesTable`
- Produces: exact helper sentence from the spec. Empty `issues` still hides the Import Errors section.

- [ ] **Step 1: Write the failing copy assertion**

In `web/src/components/import/ImportSummaryPanel.test.tsx`, keep the two existing tests. In `shows Import Errors heading and table when issues exist`, add:

```ts
    expect(
      screen.getByText(
        "Identical errors are grouped. Error messages show two lines. Click a row to expand the full message and the file list.",
      ),
    ).toBeInTheDocument();
```

Do not remove the heading or table assertions.

- [ ] **Step 2: Run the panel test and confirm it fails**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/ImportSummaryPanel.test.tsx
```

Expected: FAIL because the old sentence is still on the page (“Error messages show two lines. Click a row to expand or collapse the full message.”).

- [ ] **Step 3: Replace the helper sentence**

In `web/src/components/import/ImportSummaryPanel.tsx`, change the muted `<p>` under Import Errors to:

```tsx
            <p className="mb-0 mt-1 text-[0.75rem] text-muted">
              Identical errors are grouped. Error messages show two lines. Click a row to expand the full message and the file list.
            </p>
```

Do not change how `summary.issues` is passed to the table.

Add this bullet under `## [Unreleased]` in `CHANGELOG.md`. Create a `### Fixed` heading there if one is not present:

```md
- 2026-08-27: Import Errors groups identical `step` + `reason` + `kind` into one row with an `N files` count. Expanding the row lists the filenames. Stored per-file issues are unchanged.
```

- [ ] **Step 4: Run the related tests and confirm they pass**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/ImportSummaryPanel.test.tsx src/components/import/VirtualizedImportIssuesTable.test.tsx src/components/import/groupImportIssues.test.ts src/screens/import/ImportProgressView.test.tsx
```

Expected: PASS. `ImportProgressView` still finds the Import Errors heading and table; it does not assert the old helper sentence.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/import/ImportSummaryPanel.tsx web/src/components/import/ImportSummaryPanel.test.tsx CHANGELOG.md
git commit -m "$(cat <<'EOF'
fix(import): mention grouped import errors

The helper line under Import Errors still described one row per
file. It now says identical errors are grouped.

Related to #202
EOF
)"
```
