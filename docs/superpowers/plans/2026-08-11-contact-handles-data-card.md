# Contact handles DataCard contrast Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable DataCard shell and restyle the contact handles table for medium contrast plus a clear Threads link cue.

**Architecture:** New `DataCard` wraps bordered `bg-panel` card + optional toolbar. Shared header/body class helpers sit beside it. `ContactDrawerHandles` adopts DataCard and upgrades clickable `CountCell` (accent, underline, chevron).

**Tech Stack:** React 19, Tailwind theme tokens (`panel`, `elevated`, `text`, `muted`, `accent`), React Aria Table (unchanged).

## Global Constraints

- No new CSS theme tokens beyond existing variables and soft mixes.
- Do not migrate Settings tables in this change.
- Do not make Direct/Group message counts into links.
- Preserve sort, Add/Edit/Unlink, max-w-4xl, and browse-on-Threads behavior.

**Spec:** `docs/superpowers/specs/2026-08-11-contact-handles-data-card-design.md`

## File map

| File | Responsibility |
|------|----------------|
| `web/src/components/DataCard.tsx` | Card shell + exported chrome class helpers |
| `web/src/components/contactDrawer/ContactDrawerHandles.tsx` | Consume DataCard; header band; CountCell link cue |

---

### Task 1: DataCard + chrome helpers

**Files:**
- Create: `web/src/components/DataCard.tsx`

- [x] Add `DataCard` with `children`, optional `toolbar`, optional `className`, default `max-w-4xl`
- [x] Structure: card (`bg-panel border border-border rounded-lg p-4`) → toolbar row → `overflow-x-auto` → children
- [x] Export helpers: `dataCardHeaderRowClass`, `dataCardHeaderCellClass`, `dataCardBodyCellClass` (header band `bg-elevated`, headers `text-text`, body `text-text`)
- [x] Commit: `feat(web): add DataCard shell for contrast tables`

### Task 2: Wire handles table + Threads cue

**Files:**
- Modify: `web/src/components/contactDrawer/ContactDrawerHandles.tsx`

- [x] Wrap table in `<DataCard toolbar={Add}>`
- [x] Apply header helpers on `TableHeader` / `SortableColumn` / empty actions column
- [x] Body/total cells use body helper; keep muted only for dates / em dashes / zero counts
- [x] Clickable `CountCell`: accent, always underline, small chevron, `aria-label` “Open N threads”
- [x] Manual check dark/light: contrast + Threads affordance
- [x] Commit: `feat(contacts): restyle handles table with DataCard`

### Task 3: Verify

- [x] `cd web && npx tsc --noEmit`
- [ ] Spot-check drawer: headers stand out, Threads looks clickable, sort/Add still work
