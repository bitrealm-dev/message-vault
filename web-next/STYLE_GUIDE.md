# Message Vault style guide

Tokens live in [`src/app/globals.css`](src/app/globals.css). Use semantic Tailwind utilities (`bg-panel`, `text-muted`) — never raw hex in components.

Appearance: **Settings → Appearance**. Live token swatches are in `StyleGuidePreview` (developer preview component; not shown on the Appearance page).

## Four-seed themes

Themes are four hex seeds (Fastmail-style):

| Seed | Role |
|------|------|
| Light header | Light-mode chrome (`--sidebar` / header strip) |
| Light accent | Light-mode accent / selection / sent bubbles |
| Dark header | Dark-mode chrome |
| Dark accent | Dark-mode accent / selection / sent bubbles |

Mode is `light` | `dark` | `system`. Only the active header+accent pair is applied as `--header` and `--accent` on `<html>` (`data-theme` is the resolved light/dark). Everything else is **derived** with `color-mix` in CSS.

**Default (Ocean Depths):** `#f1faee,#2d8b8b,#1a2332,#a8dadc`

**Share string:** same comma-separated form — copy/paste under Settings → Appearance. Stored in `localStorage` as `mv-theme` (mode) and `mv-theme-seeds` (share string or JSON).

Presets (“tried and true”) are built-in four-hex packs in [`src/lib/theme.ts`](src/lib/theme.ts).

## Derivation

| Token | Source |
|-------|--------|
| `--sidebar` | active **header** |
| `--accent` / `--sent` | active **accent** |
| `--bg`, `--panel`, `--elevated` | mixes of header with black/white |
| `--popover` | alias of `--elevated` |
| `--border`, `--text`, `--muted` | contrast from header |
| `--received` | muted mix from header; `--received-text` = `--text` |
| `--hover` / `--hover-strong` / `--scrim` | black/white alpha |
| `--danger` | fixed red per mode (not user-seeded) |

**Utilities:** `bg-bg` `bg-panel` `bg-sidebar` `bg-elevated` `bg-popover` `border-border` `text-text` `text-muted` `text-accent` `bg-accent` `bg-hover` `bg-hover-strong` `bg-scrim` `text-danger` `bg-sent` `text-sent-text` `bg-received` `text-received-text`

## Type

| Role | Spec |
|------|------|
| Font | Geist Sans (`--font-geist-sans`) |
| UI body | `text-[13px]`–`text-[14px]` |
| Section label | `text-[12px] font-semibold uppercase tracking-wider text-muted` |
| Page title | `text-2xl font-semibold tracking-tight` |

## Surfaces

| Layer | Token / pattern |
|-------|-----------------|
| Canvas | `bg-bg` |
| Nav | `bg-sidebar`, 45px header rows, `border-border` |
| Content | `bg-panel` |
| Raised | `bg-elevated` / `bg-popover` |
| Active nav | `bg-accent/35` + accent bar; idle `text-muted` + `hover:bg-hover` |

## Interaction

- Hover: `hover:bg-hover` or `hover:bg-hover-strong`
- Focus: accent border / outline
- Disabled: `opacity-40`–`50`
- Bubbles: `--sent` (= accent) / `--received` (derived)

## Rules

1. New UI uses tokens only — no hard-coded surface hexes or `bg-white/20`.
2. User color comes from the four seeds; do not invent a second accent family in components.
3. Check light, dark, and a custom seed under Settings → Appearance before shipping UI.
4. Keep browse density: compact rows, 45px tool headers.
