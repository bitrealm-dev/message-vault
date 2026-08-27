# Fix PR #82 Exhaustive-Deps Regressions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore intentional React effect dependencies that PR [#82](https://github.com/bitrealm-io/message-vault/pull/82) emptied or dropped while silencing Biome `useExhaustiveDependencies`, without reintroducing lint warnings or weakening `web/biome.json`.

**Architecture:** Stay on branch `fix/clear-product-linter-warnings`. Revert the broken deps to their pre-regression shapes from `origin/main` (or equivalent), keep legitimate a11y/`biome-ignore` exceptions, and add hook tests so `queryKey` / `reload()` cannot regress again. Prefer `void someDep` or a targeted `biome-ignore` with a reason when a dependency exists only to invalidate an effect (not read inside the body). Do **not** rename deps to `_foo` and drop them.

**Tech Stack:** React 19, Vitest + Testing Library (`renderHook`), Biome (`web/biome.json`), Vite SPA in `web/`, git branch already open as PR #82.

## Global Constraints

- Work on `fix/clear-product-linter-warnings` (checkout / pull latest before editing). Do not commit to `main`.
- Do **not** soften `web/biome.json` rules (keep a11y/correctness at error via recommended).
- Do **not** reintroduce `#[allow(clippy::too_many_arguments)]`.
- Prefer restoring real dependencies over `biome-ignore`. Use ignore only when the rule fights a known-good pattern (e.g. VirtualList throttle deps on `nextRange.start`/`end` only).
- Every ignore must include a short reason after the colon.
- Product lint path: `cd web && npx biome ci .` and `cd web && npm test` must pass after the fixes.
- Skip docs/web-next/Slint except the one `scripts/lint-all.sh` comment fix in Task 6.

## File map

| File | Role |
|------|------|
| `web/src/lib/usePagedList.ts` | First-page load must re-run when `queryKey` changes |
| `web/src/lib/usePagedList.test.tsx` | New: prove `queryKey` change reloads |
| `web/src/lib/useResource.ts` | `reload()` must bump `reloadToken` into effect deps |
| `web/src/lib/useResource.test.tsx` | Extend: prove `reload()` refetches |
| `web/src/screens/ContactList.tsx` | Clear checks on filter; debounce must see `catalogComplete`; checkbox click |
| `web/src/screens/ConversationList.tsx` | Clear checks/tag overrides when `query` changes |
| `web/src/components/CheckedContactsPanel.tsx` | Reload metrics when selection key changes |
| `web/src/components/contactDrawer/useHandleMutations.ts` | Reset dialog state when `contactId` changes |
| `web/src/components/contactDrawer/ContactDrawerHandles.tsx` | Reset sort when `contactId` changes |
| `web/src/screens/MessageView.tsx` | Re-open participants when conversation changes |
| `web/src/lib/ThemeProvider.tsx` | Re-apply CSS when OS dark preference changes |
| `web/src/components/import/VirtualizedImportIssuesTable.tsx` | Remeasure on expand / issues change |
| `web/src/components/InfiniteOffsetList.tsx` | Republish range when `items` identity/content changes |
| `web/src/components/VirtualList.tsx` | Throttle deps on start/end only; ResizeObserver includes `count` |
| `web/src/screens/message/ConversationHeader.tsx` | Stable unique React keys |
| `scripts/lint-all.sh` | Comment matches Biome error severity |

---

### Task 1: Restore `usePagedList` queryKey + failing/passing tests

**Files:**
- Modify: `web/src/lib/usePagedList.ts`
- Create: `web/src/lib/usePagedList.test.tsx`
- Test: `web/src/lib/usePagedList.test.tsx`

**Interfaces:**
- Consumes: existing `usePagedList(queryKey, fetchPage, options?)` API
- Produces: same public API; effect deps must include `queryKey` and `firstPageSize`

- [ ] **Step 1: Checkout the PR branch and confirm you are not on main**

```bash
cd /home/mbeisser/repo/message-vault
git fetch origin fix/clear-product-linter-warnings
git checkout fix/clear-product-linter-warnings
git pull --ff-only
git status -sb
```

Expected: branch name `fix/clear-product-linter-warnings`, clean or only your WIP.

- [ ] **Step 2: Write the failing test for queryKey reload**

Create `web/src/lib/usePagedList.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { usePagedList } from "./usePagedList";

describe("usePagedList", () => {
  it("reloads the first page when queryKey changes", async () => {
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => {
      const q = fetchPage.mock.calls.length;
      return {
        items: [{ id: `q${q}-o${offset}` }],
        total: 1,
      };
    });

    const { result, rerender } = renderHook(
      ({ queryKey }: { queryKey: string }) => usePagedList(queryKey, fetchPage),
      { initialProps: { queryKey: "alpha" } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items[0]?.id).toMatch(/^q1-/);
    const callsAfterFirst = fetchPage.mock.calls.length;

    act(() => {
      rerender({ queryKey: "beta" });
    });

    await waitFor(() => expect(fetchPage.mock.calls.length).toBeGreaterThan(callsAfterFirst));
    await waitFor(() => expect(result.current.loading || result.current.refreshing).toBe(false));
    expect(result.current.items[0]?.id).not.toMatch(/^q1-/);
  });
});
```

- [ ] **Step 3: Run the test and confirm it fails on the broken branch**

Run: `cd web && npm test -- src/lib/usePagedList.test.tsx`

Expected: FAIL — after `rerender({ queryKey: "beta" })`, fetch call count does not increase (or items stay `q1-…`).

- [ ] **Step 4: Restore `queryKey` in the hook**

In `web/src/lib/usePagedList.ts`:

1. Rename parameter `_queryKey` back to `queryKey`.
2. Change the first-page effect dependency array from `[firstPageSize]` to `[queryKey, firstPageSize]`.
3. At the top of that effect body, add `void queryKey;` so Biome knows the dependency is intentional (invalidates the load) even though the fetcher comes from a ref:

```ts
export function usePagedList<T>(
  queryKey: string,
  fetchPage: PagedFetchPage<T>,
  options?: UsePagedListOptions,
): UsePagedListResult<T> {
  // ... unchanged state/refs ...

  useEffect(() => {
    void queryKey;
    const ac = new AbortController();
    // ... rest of existing effect body unchanged ...
  }, [queryKey, firstPageSize]);
```

Do not leave `_queryKey`. Do not empty the deps.

- [ ] **Step 5: Re-run the test and confirm it passes**

Run: `cd web && npm test -- src/lib/usePagedList.test.tsx`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/usePagedList.ts web/src/lib/usePagedList.test.tsx
git commit -m "$(cat <<'EOF'
fix(web): reload paged lists when queryKey changes

Restore usePagedList effect deps so search/filter updates refetch
instead of keeping a stale first page.

EOF
)"
```

---

### Task 2: Restore `useResource.reload` + test

**Files:**
- Modify: `web/src/lib/useResource.ts`
- Modify: `web/src/lib/useResource.test.tsx`

**Interfaces:**
- Consumes: existing `reload: () => void` return
- Produces: effect deps `[key, reloadToken]` again

- [ ] **Step 1: Write the failing reload test**

Append to `web/src/lib/useResource.test.tsx`:

```tsx
  it("refetches when reload is called", async () => {
    let n = 0;
    const { result } = renderHook(() =>
      useResource("k1", async () => {
        n += 1;
        return `v${n}`;
      }),
    );

    await waitFor(() => expect(result.current.data).toBe("v1"));

    act(() => {
      result.current.reload();
    });

    await waitFor(() => expect(result.current.data).toBe("v2"));
  });
```

Keep the existing `import { act, renderHook, waitFor } from "@testing-library/react";` (already present).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npm test -- src/lib/useResource.test.tsx`

Expected: FAIL — `data` stays `"v1"` after `reload()`.

- [ ] **Step 3: Restore `reloadToken` in the effect**

In `web/src/lib/useResource.ts`:

```ts
  const [reloadToken, setReloadToken] = useState(0);
  // ...
  useEffect(() => {
    if (key === null) {
      setData(null);
      setLoading(false);
      setError("");
      return;
    }
    // ... existing fetch body unchanged ...
    return () => controller.abort();
  }, [key, reloadToken]);
```

Rename `_reloadToken` back to `reloadToken`. Do not drop it from deps.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web && npm test -- src/lib/useResource.test.tsx`

Expected: PASS (all cases including the new one)

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/useResource.ts web/src/lib/useResource.test.tsx
git commit -m "$(cat <<'EOF'
fix(web): make useResource.reload refetch again

Put reloadToken back in the effect dependency list so trash/profile
reload after mutations.

EOF
)"
```

---

### Task 3: Restore UI reset / filter effects

**Files:**
- Modify: `web/src/screens/ContactList.tsx`
- Modify: `web/src/screens/ConversationList.tsx`
- Modify: `web/src/components/CheckedContactsPanel.tsx`
- Modify: `web/src/components/contactDrawer/useHandleMutations.ts`
- Modify: `web/src/components/contactDrawer/ContactDrawerHandles.tsx`
- Modify: `web/src/screens/MessageView.tsx`
- Modify: `web/src/lib/ThemeProvider.tsx`

**Interfaces:**
- Consumes: existing component props/state (`filter`, `groupFilter`, `query`, `contactId`, `conversation.id`, `prefersDark`)
- Produces: same UX as `origin/main` for these effects

- [ ] **Step 1: Restore ContactList check-clear and catalog debounce deps**

In `web/src/screens/ContactList.tsx`, replace the empty clear effect and incomplete debounce deps:

```tsx
  useEffect(() => {
    void filter;
    void groupFilter;
    setCheckedIds(new Set());
  }, [filter, groupFilter]);

  useEffect(() => {
    setGroupOverrides({});
    const combined = groupListQuery(groupFilter, filter);
    if (!combined.trim()) {
      setServerQ("");
      return;
    }
    if (fullCatalogRef.current && !advancedActive) {
      setServerQ("");
      return;
    }
    if (groupActive && !filter.trim()) {
      setServerQ(combined);
      return;
    }
    if (catalogCompleteRef.current && !advancedActive) return;

    const t = window.setTimeout(() => setServerQ(combined), FILTER_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [filter, catalogComplete, advancedActive, groupFilter, groupActive]);
```

Also restore explicit toggle on the avatar control (checkbox uses `pointer-events-none`, so label association alone is fragile). Keep the `<label>` wrapper if present for a11y, but call `toggleChecked`:

```tsx
            <label
              className="group/avatar relative flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center self-center"
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                skipRowSelectRef.current = true;
                toggleChecked(c.id);
                queueMicrotask(() => {
                  skipRowSelectRef.current = false;
                });
              }}
              onKeyDown={(e) => e.stopPropagation()}
            >
```

If the file still uses `<span>`, keep `<span>` and the same `toggleChecked` handler (do not invent a second pattern).

- [ ] **Step 2: Restore ConversationList clear-on-query**

In `web/src/screens/ConversationList.tsx`:

```tsx
  useEffect(() => {
    void query;
    setCheckedIds(new Set());
    setTagOverrides({});
  }, [query]);
```

- [ ] **Step 3: Restore CheckedContactsPanel metrics key**

In `web/src/components/CheckedContactsPanel.tsx`:

```tsx
  const contactKey = contacts.map((c) => c.id).join(",");
  // ...
  useEffect(() => {
    void contactKey;
    const selected = contactsRef.current;
    // ... existing body unchanged ...
    return () => ac.abort();
  }, [contactKey]);
```

Rename `_contactKey` → `contactKey`. Do not leave deps as `[]`.

- [ ] **Step 4: Restore contact drawer resets**

In `web/src/components/contactDrawer/useHandleMutations.ts`:

```tsx
  useEffect(() => {
    void contactId;
    setAdding(false);
    setBusy(false);
    setRemoveTarget(null);
  }, [contactId]);
```

In `web/src/components/contactDrawer/ContactDrawerHandles.tsx`:

```tsx
  useEffect(() => {
    void contactId;
    setSortDescriptor(null);
  }, [contactId]);
```

- [ ] **Step 5: Restore MessageView participants open**

In `web/src/screens/MessageView.tsx`:

```tsx
  useEffect(() => {
    void conversation.id;
    setParticipantsOpen(true);
  }, [conversation.id]);
```

- [ ] **Step 6: Restore ThemeProvider `prefersDark` dep**

In `web/src/lib/ThemeProvider.tsx`:

```tsx
  useEffect(() => {
    if (!hydrated) return;
    applyTheme(mode, seeds);
  }, [hydrated, mode, seeds, prefersDark]);
```

`applyTheme` reads the live media query internally; this dependency forces CSS variables to refresh when OS light/dark flips while mode is `"system"`.

- [ ] **Step 7: Lint the touched web files**

Run: `cd web && npx biome check src/lib/usePagedList.ts src/lib/useResource.ts src/screens/ContactList.tsx src/screens/ConversationList.tsx src/components/CheckedContactsPanel.tsx src/components/contactDrawer/useHandleMutations.ts src/components/contactDrawer/ContactDrawerHandles.tsx src/screens/MessageView.tsx src/lib/ThemeProvider.tsx`

Expected: no errors. If Biome still flags a dep, fix with `void dep` (already included) — do not empty the array.

- [ ] **Step 8: Commit**

```bash
git add \
  web/src/screens/ContactList.tsx \
  web/src/screens/ConversationList.tsx \
  web/src/components/CheckedContactsPanel.tsx \
  web/src/components/contactDrawer/useHandleMutations.ts \
  web/src/components/contactDrawer/ContactDrawerHandles.tsx \
  web/src/screens/MessageView.tsx \
  web/src/lib/ThemeProvider.tsx
git commit -m "$(cat <<'EOF'
fix(web): restore intentional effect resets and theme deps

Re-clear selection/dialog state on navigation and filter changes, and
re-apply theme CSS when the OS color scheme preference changes.

EOF
)"
```

---

### Task 4: Restore virtualization measure / range deps

**Files:**
- Modify: `web/src/components/import/VirtualizedImportIssuesTable.tsx`
- Modify: `web/src/components/InfiniteOffsetList.tsx`
- Modify: `web/src/components/VirtualList.tsx`

**Interfaces:**
- Consumes: existing `virtualizer`, `expandedIndex`, `issues`, `items`, `nextRange`, `count`
- Produces: main-branch dependency behavior for measure/throttle/resize

- [ ] **Step 1: Restore import issues remeasure**

In `web/src/components/import/VirtualizedImportIssuesTable.tsx`:

```tsx
  useEffect(() => {
    virtualizer.measure();
  }, [expandedIndex, issues, virtualizer]);
```

Keep existing `biome-ignore` comments for `useSemanticElements` / `useFocusableInteractive` on the virtualized grid — those are valid and unrelated.

- [ ] **Step 2: Restore InfiniteOffsetList items-driven publish**

Replace the `useCallback(..., [items.length])` + layout effect pattern with the main-branch shape so same-length list replacements still republish:

```tsx
  const publishVisibleRange = (root: HTMLElement) => {
    const rootRect = root.getBoundingClientRect();
    const rows = root.querySelectorAll("[data-contact-index]");
    let start = 0;
    let end = 0;
    for (const row of rows) {
      const rect = row.getBoundingClientRect();
      if (rect.bottom <= rootRect.top || rect.top >= rootRect.bottom) continue;
      const raw = row.getAttribute("data-contact-index");
      const idx = raw == null ? Number.NaN : Number(raw);
      if (!Number.isFinite(idx)) continue;
      const oneBased = idx + 1;
      if (start === 0) start = oneBased;
      end = oneBased;
    }
    onRangeRef.current({ start, end });
    if (hasMoreRef.current && items.length > 0 && end >= items.length - NEAR_END_THRESHOLD) {
      requestMoreRef.current();
    }
  };

  useLayoutEffect(() => {
    const root = scrollerRef.current;
    if (root) publishVisibleRange(root);
    // biome-ignore lint/correctness/useExhaustiveDependencies: republish when the item list changes (identity or content)
  }, [items]);
```

If Biome requires `publishVisibleRange` in the array and that causes a loop, keep `[items]` and the ignore above (do not depend only on `items.length`).

Also restore section keys to avoid letter collisions across groups:

```tsx
      {groups.map(([letter, groupItems], groupIndex) => (
        <section key={`${letter}-${groupIndex}`} aria-label={`Names starting with ${letter}`}>
```

Remove the unused `useCallback` import from this file if nothing else needs it.

- [ ] **Step 3: Restore VirtualList throttle and ResizeObserver deps**

In `web/src/components/VirtualList.tsx`, throttle effect (replace eslint comment with Biome):

```tsx
    // Depend on start/end numbers, not the range object. A new object each
    // render would restart the timer forever.
    // biome-ignore lint/correctness/useExhaustiveDependencies: nextRange object identity would restart the throttle forever
  }, [nextRange.start, nextRange.end, count, nearEndThreshold]);
```

Do **not** include `nextRange` itself in that array.

ResizeObserver effect:

```tsx
  }, [virtualizer, count, columnResizing]);
```

- [ ] **Step 4: Lint these three files**

Run: `cd web && npx biome check src/components/import/VirtualizedImportIssuesTable.tsx src/components/InfiniteOffsetList.tsx src/components/VirtualList.tsx`

Expected: clean (or only pre-existing a11y ignores on the issues table).

- [ ] **Step 5: Commit**

```bash
git add \
  web/src/components/import/VirtualizedImportIssuesTable.tsx \
  web/src/components/InfiniteOffsetList.tsx \
  web/src/components/VirtualList.tsx
git commit -m "$(cat <<'EOF'
fix(web): restore virtual list measure and range dependencies

Remeasure import issues on expand, republish contact ranges when items
change, and keep VirtualList throttle deps on start/end numbers only.

EOF
)"
```

---

### Task 5: ConversationHeader keys

**Files:**
- Modify: `web/src/screens/message/ConversationHeader.tsx`

**Interfaces:**
- Consumes: `displayParticipants` entries with `contact_id`, `label`
- Produces: unique React keys including index

- [ ] **Step 1: Restore index-qualified keys**

In the participants map, restore:

```tsx
          {displayParticipants.map((p, i) =>
            p.contact_id ? (
              <button
                key={`${p.contact_id}-${p.label}-${i}`}
                type="button"
                onClick={() => onOpenContact?.(p.contact_id!)}
                title={`Open contact for ${p.label}`}
                className="cursor-pointer rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent"
              >
                {p.label}
              </button>
            ) : (
              <span
                key={`${p.label}-${i}`}
                className="rounded-full border border-border bg-elevated px-2 py-0.5 text-[0.75rem] text-muted"
              >
                {p.label}
              </span>
            ),
          )}
```

Do not use bare `contactId` or `p.label` alone.

- [ ] **Step 2: Commit**

```bash
git add web/src/screens/message/ConversationHeader.tsx
git commit -m "$(cat <<'EOF'
fix(web): use unique keys for conversation participant chips

EOF
)"
```

---

### Task 6: lint-all comment + full verification + push

**Files:**
- Modify: `scripts/lint-all.sh`

**Interfaces:**
- Consumes: none
- Produces: accurate script header comment; green web CI locally

- [ ] **Step 1: Fix the stale Biome comment**

In `scripts/lint-all.sh`, replace the header wording that says Biome warnings do not fail with:

```bash
#!/usr/bin/env bash
# Run Rust Clippy and the web linter.
#
#   ./scripts/lint-all.sh
#
# Stops on the first failure. Clippy covers the workspace (except the legacy
# Slint GUI crate) and src-tauri. Biome lints web/; recommended rules are
# errors (same as CI `biome ci`). Clippy warnings do not fail this script.
# Does not format, test, or build. Runs npm ci in web/ only when that tree
# has no node_modules yet.
#
# Skips docs/, web-next/, and message-vault-io-gui (not the product path).
# CI does not run Clippy. This script is the local Clippy + web lint command.
```

- [ ] **Step 2: Run web tests**

Run: `cd web && npm test`

Expected: all tests PASS, including new `usePagedList` / `useResource.reload` cases.

- [ ] **Step 3: Run Biome CI**

Run: `cd web && npx biome ci .`

Expected: PASS (no errors, no format drift).

- [ ] **Step 4: Spot-check grep for the anti-patterns that caused the regression**

Run:

```bash
cd /home/mbeisser/repo/message-vault
rg '_queryKey|_reloadToken|_contactKey' web/src
rg -n 'setCheckedIds\(new Set\(\)\);\n  \}, \[\]' web/src -U || true
rg '}, \[\]\);' web/src/screens/ContactList.tsx web/src/screens/ConversationList.tsx web/src/screens/MessageView.tsx web/src/components/CheckedContactsPanel.tsx web/src/components/contactDrawer/useHandleMutations.ts web/src/components/contactDrawer/ContactDrawerHandles.tsx
```

Expected: no `_queryKey` / `_reloadToken` / `_contactKey`. The clear/reset effects must not use empty `[]` (MessageView / ContactList / ConversationList / CheckedContactsPanel / handle mutations / handles).

- [ ] **Step 5: Commit comment + push**

```bash
git add scripts/lint-all.sh
git commit -m "$(cat <<'EOF'
docs: clarify lint-all Biome failures match CI errors

EOF
)"
git push origin HEAD
```

- [ ] **Step 6: Confirm PR checks**

```bash
gh pr view 82 --json url,statusCheckRollup -q '{url: .url, checks: [.statusCheckRollup[] | {name, conclusion}]}'
```

Expected: PR URL still https://github.com/bitrealm-io/message-vault/pull/82 ; Lint (web) and Test (web) succeed after the new commits land.

---

## Self-review

**Spec coverage (code-review findings → tasks):**
- Critical usePagedList → Task 1
- Critical useResource.reload → Task 2
- Important empty reset deps + catalogComplete + ThemeProvider → Task 3
- Important virtualization / InfiniteOffsetList / VirtualList → Task 4
- Minor ConversationHeader keys → Task 5
- Minor lint-all comment + verification/push → Task 6
- Tauri IPC / Rust Args: no change needed (review found them OK)
- Contact checkbox: Task 3 restores `toggleChecked` under the label

**Placeholder scan:** none intentionally left.

**Type consistency:** `queryKey`, `reloadToken`, `contactKey` names match `origin/main` and the tests in Tasks 1–2.
