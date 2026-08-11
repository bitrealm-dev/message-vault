# Contact identity edit-mode affordances Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make identity-row edit mode obvious with an accent-tinted row and green Save / red Cancel icons, without changing edit behavior.

**Architecture:** Style-only change in `ContactDrawerHandles.tsx`. Add dedicated ok/danger icon button classes (do not inherit idle muted→text hover). Apply an editing-row class on the React Aria `Row` when that identity is being edited. Keep ✓ / × glyphs and existing idle hover reveal rules.

**Tech Stack:** React 19, React Aria Table/Button, Tailwind + existing theme tokens (`ok`, `ok-soft-*`, `danger`, `danger-soft-*`, `accent`).

## Global Constraints

- Compact ✓ / × only (no text Save/Cancel, no expanded mini-form).
- Touch only `web/src/components/contactDrawer/ContactDrawerHandles.tsx`.
- No new theme tokens; use existing `--ok` / `--danger` / `--accent` soft mixes.
- Preserve: single-row edit, Enter/Escape, hover-only idle icons, hide idle icons on other rows while editing, Remove-identity confirm.

**Spec:** `docs/superpowers/specs/2026-08-11-contact-identity-edit-mode-design.md`

## File map

| File | Responsibility |
|------|----------------|
| `web/src/components/contactDrawer/ContactDrawerHandles.tsx` | Editing-row chrome; Save ok button; Cancel danger button |

---

### Task 1: Button classes + editing row chrome

**Files:**
- Modify: `web/src/components/contactDrawer/ContactDrawerHandles.tsx`

**Interfaces:**
- Consumes: existing `editing` boolean (`editingHandle === h.handle`), `iconBtnDangerClass` (already present for trash)
- Produces: `iconBtnOkClass`, `editingRowClass` constants; Row/Save/Cancel use them

- [ ] **Step 1: Add `iconBtnOkClass` and `editingRowClass` next to the existing icon button constants**

Place after `iconBtnDangerClass` (around line 45):

```tsx
const iconBtnOkClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !border-transparent !bg-transparent !p-0 !font-normal !leading-none !text-ok hover:!border-ok-soft-border hover:!bg-ok-soft-bg hover:!text-ok data-hovered:!border-ok-soft-border data-hovered:!bg-ok-soft-bg data-hovered:!text-ok data-pressed:!border-ok-soft-border data-pressed:!bg-ok-soft-bg data-pressed:!text-ok";
/** Soft accent fill + 3px left accent bar while the row is in edit mode. */
const editingRowClass =
  "bg-[color-mix(in_srgb,var(--accent)_12%,var(--panel))] shadow-[inset_3px_0_0_0_var(--accent)]";
```

Keep `iconBtnDangerClass` as-is (already soft-danger hover). Do not build ok/danger classes by appending onto `iconBtnClass` — idle hover `!text-text` fights semantic colors.

- [ ] **Step 2: Apply `editingRowClass` on the data `Row` when `editing`**

Replace:

```tsx
<Row id={h.id} className="group/handle-row outline-none">
```

with:

```tsx
<Row
  id={h.id}
  className={`group/handle-row outline-none${editing ? ` ${editingRowClass}` : ""}`}
>
```

- [ ] **Step 3: Wire Save to `iconBtnOkClass` and Cancel to `iconBtnDangerClass`**

In the editing branch of the actions cell, change the two buttons from `className={iconBtnClass}` to:

```tsx
<Button
  variant="ghost"
  disabled={busy || !editHandle.trim()}
  title="Save"
  aria-label="Save"
  onClick={() => void saveEdit()}
  className={iconBtnOkClass}
>
  ✓
</Button>
<Button
  variant="ghost"
  title="Cancel"
  aria-label="Cancel"
  onClick={cancelEdit}
  className={iconBtnDangerClass}
>
  ×
</Button>
```

Leave idle pencil on `iconBtnClass` and trash on `iconBtnDangerClass`. Leave the `editingHandle != null ? "pointer-events-none opacity-0" : rowActionsRevealClass` branch unchanged.

- [ ] **Step 4: Typecheck**

Run: `cd web && npx tsc --noEmit`  
Expected: exit 0

- [ ] **Step 5: Manual verify in contact drawer**

1. Hover idle row → muted pencil/trash; leave → hide.  
2. Edit → accent tint + left bar; green ✓ and red × always on that row.  
3. While editing, hover other rows → no pencil/trash.  
4. Save / Cancel / Enter / Escape still work; empty identity disables Save.  
5. Trash still reddish on hover when not editing.

- [ ] **Step 6: Commit**

```bash
git add web/src/components/contactDrawer/ContactDrawerHandles.tsx
git commit -m "$(cat <<'EOF'
feat(contacts): clarify identity edit-mode actions

Tint the editing row and color Save/Cancel so they no longer
read like the idle pencil/trash pair.
EOF
)"
```

---

## Spec coverage

| Spec item | Task |
|-----------|------|
| Accent-tinted editing row + left bar | Task 1 Step 2 |
| Green Save ✓ | Task 1 Steps 1, 3 |
| Red Cancel × | Task 1 Step 3 |
| Idle hover / hide-while-editing | Unchanged (verify Step 5) |
| No Approach B/C, no API changes | Global constraints |
