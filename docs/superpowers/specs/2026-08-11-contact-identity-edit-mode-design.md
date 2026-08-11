# Contact identity edit-mode affordances

**Date:** 2026-08-11  
**Status:** Approved for planning  
**Scope:** Contact drawer identity (handles) table edit actions in `web/src/components/contactDrawer/ContactDrawerHandles.tsx`

## Problem

Idle row actions use muted pencil and trash icon buttons. Edit mode swaps those for ✓ and × in the same size, place, and muted ghost style. The glyph swap is easy to miss, so Save/Cancel feel like a different pair of the same control family as Edit/Remove. The editing row also lacks a clear “you are editing this row” surface cue beyond the Service select and Identity input.

## Goals

- Make edit mode visually distinct from idle/hover: both the row and the confirm/cancel controls.
- Keep the compact icon action column (Approach A from brainstorming).
- Preserve existing edit behavior (single-row edit, Enter save, Escape cancel, hover-only idle icons, hide idle icons on other rows while editing).

## Non-goals

- Text-labeled Save/Cancel buttons (Approach B).
- Expanding the editing row into a multi-line mini-form (Approach C).
- Changing Add-row UI, API payloads, or Remove-identity confirm wording beyond what already exists.
- Global table selection or multi-row edit.

## Decision

| Topic | Choice |
|--------|--------|
| Action style while editing | Compact ✓ / × icons with semantic color |
| Save styling | Theme `ok` / `ok-soft-*` (green) |
| Cancel styling | Theme `danger` / `danger-soft-*` (red), same family as Remove identity hover |
| Edit-mode row cue | Soft accent-tinted row background + 3px left accent bar |
| Idle actions | Unchanged: muted pencil/trash; reveal on row hover/focus; hidden on other rows while any row is editing |

## Visual rules

### Editing row

- Apply when `editingHandle` matches that row’s identity.
- Background: soft accent mix against the card/panel surface (same spirit as `color-mix` with `--accent` used elsewhere for info soft fills).
- Left edge: ~3px solid `accent` inset bar (box-shadow or border) so the row reads as active even at a glance.
- Service select and Identity input stay as today (elevated fields inside the cells).

### Save control

- Always visible while that row is editing (not hover-gated).
- Default: `text-ok` (or ok-soft-text if contrast needs it on light themes).
- Hover / RAC `data-hovered`: soft green background (`ok-soft-bg`) and ok-colored icon; optional soft ok border.
- Disabled when busy or identity input is empty (same rules as today).
- `title` / `aria-label`: **Save**.

### Cancel control

- Always visible while that row is editing.
- Default: `text-danger`.
- Hover / `data-hovered`: soft danger background and danger text (reuse the Remove-identity danger icon button pattern).
- `title` / `aria-label`: **Cancel**.

### Idle rows

- Pencil / trash stay muted ghost buttons with existing reveal-on-hover rules.
- While `editingHandle != null`, non-editing rows keep action icons at `opacity-0` and non-interactive.

## Behavior (unchanged)

- One identity row in edit mode at a time.
- Starting edit cancels Add; starting Add cancels edit.
- Enter saves; Escape cancels.
- Remove identity still uses `window.confirm` and the danger trash control on idle rows only.

## Implementation sketch

Touch only `ContactDrawerHandles.tsx` (plus class constants in that file):

1. Add `iconBtnOkClass` (and keep/adjust `iconBtnDangerClass`) so Save/Cancel do not inherit muted → `text-text` hover from the idle icon button class.
2. When `editing`, add an editing-row class on the React Aria `Row` (tint + left accent).
3. Wire Save to ok classes and Cancel to danger classes; keep ✓ / × glyphs.

No new components or theme tokens required; use existing `--ok`, `--ok-soft-*`, `--danger`, `--danger-soft-*`, `--accent`.

## Testing

Manual check in the contact drawer (dark and light if available):

1. Hover idle row → pencil/trash appear; leave row → they hide.
2. Enter edit → row tint + left bar; green ✓ and red × always visible on that row.
3. Hover other rows while editing → no pencil/trash.
4. Save / Cancel / Enter / Escape still work; empty identity disables Save.
5. Remove identity trash still shows reddish hover when not editing.

## Out of scope follow-ups

- Matching Add-row confirm controls to the same ok/danger icon treatment (optional polish later).
