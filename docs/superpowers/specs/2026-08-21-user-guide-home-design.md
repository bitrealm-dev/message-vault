# User Guide home: map page, not a second landing

**Date:** 2026-08-21  
**Status:** Approved for planning

## Context

https://bitrealm.io/ is a Message Vault product landing page (`docs/src/pages/index.astro`). Header **Docs** and **Get started** both go to `/vault/user/`.

`/vault/user/` is still a Starlight splash page. Starlight is the documentation theme this site uses. A splash page is its marketing homepage layout: a large hero, action buttons, and a card grid, instead of a normal article. The current hero title is “Your messages, your way,” with Try the vault / Use your own messages buttons, an ASCII “how it works” block, and chapter cards.

That page was written when the site root *was* the User Guide. After the landing shipped, `/vault/user/` reads as a second home page instead of the start of the guidebook.

The first real Get started article is already [What is Message Vault?](/vault/user/get-started/what-is-message-vault/). Developer docs already have an index at `/vault/developer/`.

## Goal

Clicking Docs (or Get started) on the landing opens `/vault/user/` inside the User Guide (sidebar visible, normal doc layout). That page tells the reader how to use the documentation: **Get started** for using the vault, **Developer** for compile / Compose / API, each with a link.

## Non-goals

- Changing landing header, footer, or `docs/src/lib/landing-links.ts` (both Docs and Get started stay on `/vault/user/`)
- Redirecting `/vault/user/` to the first Get started article
- Rewriting Get started chapters, How do I… pages, or the Developer index
- Restoring splash action buttons, chapter cards, or the “supported paths” aside on the User Guide home
- New URLs, HTTP redirects, or Astro config sidebar edits (keep **Home** → `vault/user`)
- Runtime, exporter, desktop app, or vault-server code

## Why `/vault/user/` stays a short map

The landing already sells the product. The User Guide home’s job is to name the two books and send the reader into one of them. Jumping straight into “What is Message Vault?” hides that map on top of a product-intro article.

Landing **Docs** and **Get started** both keep the same URL. The map page is still the start of the guide. The next click on that page is Get started.

## What changes

Rewrite `docs/src/content/docs/vault/user/index.mdx` in place.

Remove:

- `template: splash`
- the `hero` block (title, tagline, actions)
- Starlight `Card` / `CardGrid` / `Aside` imports and usage
- the ASCII “How it works” block
- the “Choose a chapter” cards
- the supported-paths aside

Those topics already live on `/` (product story) or in Get started / How do I… / Rescue imports.

Keep the file at `index.mdx` (plain Markdown inside it is fine). No rename, no new route.

### Frontmatter

| Field | Value |
|-------|--------|
| `title` | User Guide |
| `description` | How to use the Message Vault documentation. Get started to run a vault and import backups. Developer docs for source, Compose, and the API. |

Do not set `template: splash`.

### Body (intended copy)

This is the User Guide. These pages explain how to run a vault, import phone backups, and browse messages on a machine you control.

**Get started** — If the goal is to use Message Vault, start with [What is Message Vault?](/vault/user/get-started/what-is-message-vault/). That chapter, then Prepare a backup and Import, is the path for a first archive.

**Developer** — If the goal is to compile the project, run Docker Compose, call the HTTP API, or read file-format tables, use the [Developer](/vault/developer/) docs.

Voice matches the rest of the User Guide: “the vault,” “the desktop app,” not crate names or “the backend.”

## What does not change

| Piece | Behavior |
|-------|----------|
| Landing Docs and Get started links | `/vault/user/` |
| Sidebar `{ label: 'Home', slug: 'vault/user' }` | Still this page |
| `/vault/user/get-started/what-is-message-vault/` | First Get started chapter |
| `/vault/developer/` | Developer index, including its existing User Guide sentence |
| `docs/astro.config.mjs` topic `link: '/vault/user/'` | Unchanged |

## In-repo mentions

If README, CONTRIBUTING, or other guidebook pages still describe `/vault/user/` as a splash or as the product home, update those sentences in the same change. Do not rewrite unrelated chapters.

## Verification

- `cd docs && npm run check && npm run build`
- Preview `/vault/user/`: normal guidebook page, sidebar visible, title **User Guide**, Get started and Developer links resolve
- From `/`, **Docs** and **Get started** still open `/vault/user/`
- `/vault/user/get-started/what-is-message-vault/` and `/vault/developer/` are unchanged

## Success criteria

- https://bitrealm.io/vault/user/ is the User Guide map, not a second landing
- A reader can tell Get started from Developer in one screen and click through
- Product marketing stays on https://bitrealm.io/
