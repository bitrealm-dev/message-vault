# Message Vault landing page design

**Date:** 2026-08-21  
**Status:** Approved for planning  
**Source visual:** SingleFile capture of `https://bitrealm.share.landingsite.dev/` extracted to `~/Downloads/message-vault-landing/`

## Goal

Replace the Bitrealm company splash at `https://bitrealm.io/` (`docs/src/pages/index.astro`) with a Message Vault product landing page. Keep most of the landingsite look (dark palette, teal accent, section layout). Drop weak or premature sections. Wire navigation and footer to real docs and GitHub. Add thin stub pages so every footer link resolves.

Starlight user and developer guides stay under `/vault/user/` and `/vault/developer/`. This work does not move guidebook content.

## Non-goals

- Adding Tailwind to the docs site
- Shipping a pixel-perfect class-for-class port of the landingsite HTML
- Writing full FAQ / About / Contact / Changelog editorial content beyond stubs
- Refreshing outdated GitHub Release binaries
- Legal entity / copyright footer copy (no © line)
- Bulk rename of remaining `bitrealm-dev` links across the whole docs tree (landing and new stubs use `bitrealm-io`; broader cleanup is separate)

## Page architecture (`/`)

| Block | Behavior |
|--------|----------|
| Sticky header | Mark “Message Vault” → `/`. Links: Features (`/#features`), Download (GitHub Releases), Docs (`/vault/user/`). Primary button: Get started → `/vault/user/`. |
| Hero | Capture mood and layout; hero imagery from saved AVIFs. Primary CTA: Download → Releases. Secondary: Features or Docs. |
| Features (`id="features"`) | Six-capability grid in the same visual language as the capture. |
| How it works | Three steps, aligned with the real product flow (run/install vault → import backups → search and export). |
| Footer | Three columns below. No copyright / “© 2026” strip. |

### Dropped from the capture

- Audiences (personal / business / historical)
- Final “Take ownership of your message history” CTA band
- Footer “Security and privacy”
- Bottom copyright line

### Footer columns

**Product**

- Features → `/#features`
- Download → `https://github.com/bitrealm-io/message-vault/releases`
- Changelog → `/changelog`

**Resources**

- Getting started → `/vault/user/`
- Developer documentation → `/vault/developer/`
- FAQ → `/faq`

**Company** (Bitrealm; not an LLC story)

- About → `/about`
- Contact → `/contact`

## Stub routes (site root, not under `/vault/`)

Marketing stubs share landing header/footer chrome. They are **not** Starlight sidebar pages.

| Path | Purpose |
|------|---------|
| `/changelog` | Thin Astro page with the same stub narrative as root `CHANGELOG.md` (Unreleased / empty history). Link out to the GitHub file and to Releases for installable builds. Do not build a Markdown import pipeline in v1; keep the two texts in sync by hand until a later change. |
| `/faq` | A few editable placeholder Q&As (what Message Vault is, local/self-hosted, backups required, where guides live). |
| `/about` | Short Bitrealm + Message Vault product blurb; no corporate legal framing. |
| `/contact` | Point to GitHub Issues for the repo; optional mailto later if an address exists. |

Also add a root repo file `CHANGELOG.md` (Keep a Changelog–style skeleton with an Unreleased section). Footer Changelog links to `/changelog`, not raw GitHub, so the site owns the URL.

## Implementation approach

**Astro page + dedicated CSS (no Tailwind).**

1. Rewrite `docs/src/pages/index.astro` as the landing.
2. Add `docs/src/styles/landing.css` with CSS variables for the capture palette (backgrounds in the `#070b12` / `#0a0f18` / `#05080d` family; teal brand accent). Scope selectors so Starlight’s `docs/src/styles/custom.css` stays for guide pages only.
3. Extract shared header and footer into Astro components (or equivalent partials) reused by `/` and the stub pages.
4. Copy usable images from the extract into `docs/public/landing/`. Missing section photos from the SingleFile save stay decorative placeholders; they do not block shipping.
5. Add stub pages under `docs/src/pages/` (`faq.astro`, `about.astro`, `contact.astro`, `changelog.astro`).
6. Add root `CHANGELOG.md`.

### Copy rules

- Prefer the capture’s tone and hierarchy.
- Correct product claims that do not match the guides (for example Discord as a first-class import source, or Docker-only deployment). Describe phone-backup extract/import into a local vault, browse/search/export, and self-host / from-source / desktop as the docs already do.

### Link constants

Use `https://github.com/bitrealm-io/message-vault` (and its `/releases`) everywhere on the landing and stubs.

## Verification

- `cd docs && npm ci && npm run check && npm run build`
- Manually open `/`, `/faq`, `/about`, `/contact`, `/changelog`, `/vault/user/`, `/vault/developer/` in preview
- Confirm footer and nav targets resolve; confirm no © strip on the landing

## Success criteria

- `https://bitrealm.io/` reads as Message Vault product marketing, not a Bitrealm company card
- Visual language clearly descends from the landingsite capture without requiring Tailwind
- Every header and footer link in scope goes somewhere real (stub or external)
- Guidebook URLs under `/vault/…` remain unchanged
