# Contacts Search Popdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the contacts-column filter field and “Advanced filters” link with a Fastmail-style search field (magnifying glass, “Search contacts”, focus popdown with recent searches and Advanced search).

**Architecture:** When `ListColumn` is in contacts mode, render a new `ContactSearch` control. Persist recent queries in `localStorage` via a small helper. Reuse existing `AdvancedSearchForm` (`mode="contacts"`). Conversations column keeps `GlobalSearch` + advanced toggle.

**Tech Stack:** React 19, TypeScript, React Aria Components (optional for popover/dialog patterns already used), Vite SPA under `web/`, `localStorage`, existing `node:test` unit tests.

**Spec:** [docs/superpowers/specs/2026-08-10-contacts-search-popdown-design.md](../specs/2026-08-10-contacts-search-popdown-design.md)

## Global Constraints

- Contacts only; do not change conversation search UX.
- Placeholder exactly: `Search contacts`.
- Storage key exactly: `mv-contact-recent-searches:v1`.
- Cap recent at **10**; newest first; dedupe by moving existing query to front.
- Save recent on Enter, selecting a recent row, or Advanced Apply — not on every keystroke.
- Omit Recent section until at least one query is saved; Advanced search always visible in the popdown.
- No “Narrow your search” chips in this plan.
- Empty/whitespace queries are never saved.

## File map

| File | Responsibility |
|------|----------------|
| Create `web/src/lib/contactRecentSearches.ts` | Read/write/clear/push recent query strings |
| Create `web/src/lib/contactRecentSearches.test.ts` | Unit tests for the helper |
| Create `web/src/components/ContactSearch.tsx` | Field + popdown + advanced handoff |
| Modify `web/src/components/ListColumn.tsx` | Branch contacts → `ContactSearch`; keep conversations path |

---

### Task 1: Recent searches helper

**Files:**
- Create: `web/src/lib/contactRecentSearches.ts`
- Test: `web/src/lib/contactRecentSearches.test.ts`

**Interfaces:**
- Produces:
  - `CONTACT_RECENT_SEARCHES_KEY = "mv-contact-recent-searches:v1"`
  - `CONTACT_RECENT_SEARCHES_MAX = 10`
  - `loadContactRecentSearches(): string[]`
  - `saveContactRecentSearches(queries: string[]): void`
  - `clearContactRecentSearches(): void`
  - `pushContactRecentSearch(query: string): string[]` — trims; no-op for empty; dedupes; caps at 10; returns new list

- [ ] **Step 1: Write the failing tests**

```typescript
import assert from "node:assert/strict";
import { describe, it, beforeEach } from "node:test";
import {
  CONTACT_RECENT_SEARCHES_KEY,
  clearContactRecentSearches,
  loadContactRecentSearches,
  pushContactRecentSearch,
} from "./contactRecentSearches.ts";

const mem = new Map<string, string>();

beforeEach(() => {
  mem.clear();
  (globalThis as { localStorage?: Storage }).localStorage = {
    getItem: (k) => mem.get(k) ?? null,
    setItem: (k, v) => {
      mem.set(k, String(v));
    },
    removeItem: (k) => {
      mem.delete(k);
    },
    clear: () => mem.clear(),
    key: () => null,
    length: 0,
  };
});

describe("contactRecentSearches", () => {
  it("returns empty for missing or corrupt JSON", () => {
    assert.deepEqual(loadContactRecentSearches(), []);
    mem.set(CONTACT_RECENT_SEARCHES_KEY, "{not-json");
    assert.deepEqual(loadContactRecentSearches(), []);
  });

  it("pushes newest first, dedupes, and caps at 10", () => {
    for (let i = 0; i < 12; i++) pushContactRecentSearch(`q${i}`);
    const list = loadContactRecentSearches();
    assert.equal(list.length, 10);
    assert.equal(list[0], "q11");
    assert.equal(list[9], "q2");
    pushContactRecentSearch("q5");
    assert.equal(loadContactRecentSearches()[0], "q5");
    assert.equal(loadContactRecentSearches().filter((q) => q === "q5").length, 1);
  });

  it("ignores empty and whitespace-only pushes", () => {
    pushContactRecentSearch("  ");
    assert.deepEqual(loadContactRecentSearches(), []);
  });

  it("clear removes the key", () => {
    pushContactRecentSearch("alice");
    clearContactRecentSearches();
    assert.deepEqual(loadContactRecentSearches(), []);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web && npx --yes tsx --test src/lib/contactRecentSearches.test.ts`

Expected: FAIL (module not found)

- [ ] **Step 3: Implement the helper**

```typescript
export const CONTACT_RECENT_SEARCHES_KEY = "mv-contact-recent-searches:v1";
export const CONTACT_RECENT_SEARCHES_MAX = 10;

function readRaw(): unknown {
  try {
    const raw = localStorage.getItem(CONTACT_RECENT_SEARCHES_KEY);
    if (!raw) return null;
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export function loadContactRecentSearches(): string[] {
  const parsed = readRaw();
  if (!Array.isArray(parsed)) return [];
  return parsed
    .filter((x): x is string => typeof x === "string")
    .map((s) => s.trim())
    .filter(Boolean)
    .slice(0, CONTACT_RECENT_SEARCHES_MAX);
}

export function saveContactRecentSearches(queries: string[]): void {
  try {
    localStorage.setItem(
      CONTACT_RECENT_SEARCHES_KEY,
      JSON.stringify(queries.slice(0, CONTACT_RECENT_SEARCHES_MAX)),
    );
  } catch {
    /* private mode / quota */
  }
}

export function clearContactRecentSearches(): void {
  try {
    localStorage.removeItem(CONTACT_RECENT_SEARCHES_KEY);
  } catch {
    /* ignore */
  }
}

export function pushContactRecentSearch(query: string): string[] {
  const q = query.trim();
  if (!q) return loadContactRecentSearches();
  const next = [q, ...loadContactRecentSearches().filter((x) => x !== q)].slice(
    0,
    CONTACT_RECENT_SEARCHES_MAX,
  );
  saveContactRecentSearches(next);
  return next;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web && npx --yes tsx --test src/lib/contactRecentSearches.test.ts`

Expected: PASS (all tests)

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/contactRecentSearches.ts web/src/lib/contactRecentSearches.test.ts
git commit -m "feat(web): add contact recent searches storage helper"
```

---

### Task 2: ContactSearch component

**Files:**
- Create: `web/src/components/ContactSearch.tsx`
- Consumes: helper from Task 1; `AdvancedSearchForm` from `web/src/components/AdvancedSearchForm.tsx`

**Interfaces:**
- Consumes: `pushContactRecentSearch`, `loadContactRecentSearches`, `clearContactRecentSearches`
- Produces: `ContactSearch` default export with props:

```typescript
{
  value: string;
  onChange: (v: string) => void;
  onSubmit: (q: string) => void;
}
```

- [ ] **Step 1: Implement `ContactSearch`**

Build a self-contained control:

- Outer `relative` wrapper.
- Field row: `rounded-xl border border-border bg-bg focus-within:border-accent`, left SVG magnifying glass (`aria-hidden`), `<input type="search" placeholder="Search contacts" />`, clear × when `value` non-empty.
- `onChange` of input → parent `onChange` (live filter).
- Focus / click on field → `setPopdownOpen(true)`.
- Escape (when popdown open, not advanced) → close popdown only.
- Enter → `onSubmit(value)`; if `value.trim()` then `pushContactRecentSearch(value)` and refresh local recent state; close popdown.
- Click-outside via `useEffect` + `mousedown` on `document` (ignore clicks inside wrapper / advanced panel).
- Popdown: `absolute left-0 right-0 top-full z-50 mt-1 rounded-md border border-border bg-popover shadow-md`.
  - If `recents.length > 0`: section “Recent searches” + “Clear all” button; list buttons with clock icon; on click set value via `onChange` + `onSubmit`, push/bump recent, close.
  - Divider only when recents visible.
  - “Advanced search” row (sliders icon) → `setShowAdvanced(true)` (can keep popdown or close it; prefer close popdown and show advanced overlay like today’s ListColumn absolute panel, or embed advanced below — **use absolute panel under the field at `w-[min(100%,560px)]` or full column width**, matching existing advanced panel pattern).
- When advanced open: render `AdvancedSearchForm mode="contacts"`; `onApply` → `onChange(q)`, `onSubmit(q)`, `pushContactRecentSearch(q)`, close advanced + popdown; `onClose` → close advanced only.

Use existing Tailwind tokens (`text-muted`, `bg-hover`, `border-border`, etc.). Match Fastmail density: section headers `text-[0.688rem] text-muted`, rows `text-[0.875rem]` with `hover:bg-hover`.

- [ ] **Step 2: Typecheck**

Run: `cd web && npx tsc --noEmit`

Expected: PASS (no errors in ContactSearch)

- [ ] **Step 3: Commit**

```bash
git add web/src/components/ContactSearch.tsx
git commit -m "feat(web): add ContactSearch field with recent and advanced popdown"
```

---

### Task 3: Wire ListColumn contacts mode

**Files:**
- Modify: `web/src/components/ListColumn.tsx`

**Interfaces:**
- Consumes: `ContactSearch` from Task 2
- Existing props unchanged: `searchQuery`, `searchMode`, `onSearchChange`, `onSearch`, `children`

- [ ] **Step 1: Branch the header**

When `searchMode === "contacts"`:

```tsx
<ContactSearch
  value={searchQuery}
  onChange={onSearchChange}
  onSubmit={(q) => onSearch(q)}
/>
```

Do **not** render the “Advanced filters” toggle or the shared advanced panel for contacts (advanced lives inside `ContactSearch`).

When `searchMode !== "contacts"`: keep current `GlobalSearch` + advanced toggle + `AdvancedSearchForm` panel exactly as today.

Raise list-column `zIndex` when contacts popdown/advanced is open **or** let `ContactSearch` use a high enough `z-50`/`z-[100]` that it overlays the main pane (same as current advanced panel). Prefer: `ContactSearch` reports open state via optional callback, **or** simply use `z-50` on the popdown/advanced absolute layers inside `ContactSearch` without changing ListColumn z-index. Simplest: absolute layers inside `ContactSearch` with `z-50` are enough because the header is already in a stacking context — if clipped, set ListColumn `overflow-visible` (already set) and optionally `zIndex: 40` when contacts UI open via a small `onOpenChange` prop:

```typescript
// ContactSearch optional:
onOpenChange?: (open: boolean) => void; // true if popdown or advanced open
```

ListColumn: `const [contactsSearchOpen, setContactsSearchOpen] = useState(false)` and `zIndex: showAdvancedSearch || contactsSearchOpen ? 40 : 1`.

- [ ] **Step 2: Typecheck and build**

Run: `cd web && npm run build`

Expected: PASS

- [ ] **Step 3: Manual verification checklist**

- Contacts: magnifying glass + placeholder “Search contacts”.
- Focus opens popdown; with empty recent, only Advanced search row.
- Type filters list live; Enter saves recent and closes; recent section appears.
- Clear all removes recents; Advanced Apply fills + saves + closes.
- Conversations: unchanged GlobalSearch + Advanced search link.

- [ ] **Step 4: Commit**

```bash
git add web/src/components/ListColumn.tsx web/src/components/ContactSearch.tsx
git commit -m "feat(web): use ContactSearch in contacts list column"
```

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| Placeholder + magnifying glass | Task 2 |
| No Advanced filters button (contacts) | Task 3 |
| Popdown on focus | Task 2 |
| Live filter name/handles | Task 2–3 (existing onChange path) |
| Recent searches + Clear all | Tasks 1–2 |
| Advanced at bottom of popdown | Task 2 |
| Omit recent until first save | Task 2 |
| Cap 10 / dedupe / key | Task 1 |
| Conversations unchanged | Task 3 |
| No narrow chips | (omitted intentionally) |

## Placeholder scan

No TBD/TODO steps; helper and component APIs named consistently (`pushContactRecentSearch`, `ContactSearch`).
