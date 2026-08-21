# User Guide Home Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `/vault/user/` splash with a short User Guide map that points at Get started vs Developer.

**Architecture:** Rewrite `docs/src/content/docs/vault/user/index.mdx` in place. Keep the URL, landing links, and sidebar Home slug. No new routes and no Astro config edits.

**Tech Stack:** Astro Starlight Markdown/MDX under `docs/`.

**Spec:** `docs/superpowers/specs/2026-08-21-user-guide-home-design.md`

## Global Constraints

- Landing Docs and Get started stay on `/vault/user/` (`docs/src/lib/landing-links.ts` unchanged).
- Do not set `template: splash` or a `hero` block on the User Guide home.
- Voice: “the vault,” “the desktop app,” not crate names or “the backend.”
- Do not rewrite Get started chapters, How do I… pages, or `docs/src/content/docs/vault/developer/index.md`.
- Keep sidebar `{ label: 'Home', slug: 'vault/user' }` in `docs/astro.config.mjs`.

---

## File map

| File | Role |
|------|------|
| `docs/src/content/docs/vault/user/index.mdx` | User Guide home. Rewrite body and frontmatter. |
| `docs/src/lib/landing-links.ts` | Do not edit. `docs` is already `/vault/user/`. |
| `docs/astro.config.mjs` | Do not edit. Home slug already `vault/user`. |

README.md and CONTRIBUTING.md link to `/vault/user/` without calling it a splash. Leave them unless a sentence still describes a marketing home.

### Task 1: Rewrite the User Guide home

**Files:**
- Modify: `docs/src/content/docs/vault/user/index.mdx`

**Interfaces:**
- Consumes: existing Starlight content collection path `vault/user` (file stays `index.mdx`)
- Produces: `/vault/user/` as a normal doc with title User Guide and two section links

- [ ] **Step 1: Replace `index.mdx` with the map page**

Use this file contents (no component imports):

```mdx
---
title: User Guide
description: How to use the Message Vault documentation. Get started to run a vault and import backups. Developer docs for source, Compose, and the API.
---

This is the User Guide. These pages explain how to run a vault, import phone backups, and browse messages on a machine you control.

## Get started

If the goal is to use Message Vault, start with [What is Message Vault?](/vault/user/get-started/what-is-message-vault/). That chapter, then Prepare a backup and Import, is the path for a first archive.

## Developer

If the goal is to compile the project, run Docker Compose, call the HTTP API, or read file-format tables, use the [Developer](/vault/developer/) docs.
```

- [ ] **Step 2: Confirm landing links still point at `/vault/user/`**

Read `docs/src/lib/landing-links.ts`. `docs` must be `"/vault/user/"`. Do not change the file.

- [ ] **Step 3: Confirm no live guidebook page still calls `/vault/user/` a splash**

Search `README.md`, `CONTRIBUTING.md`, and `docs/src/content/docs/` (not old `docs/superpowers/` plans) for `Your messages, your way` and `template: splash`. The only remaining splash in content, if any, must not be the User Guide home. Do not rewrite historical specs.

- [ ] **Step 4: Check and build**

```bash
cd docs && npm run check && npm run build
```

Expected: exit 0. `docs/dist/vault/user/index.html` exists, contains `User Guide` and `What is Message Vault?`, and does not contain `Your messages, your way`.

```bash
grep -q 'Your messages, your way' docs/dist/vault/user/index.html && echo FAIL || echo OK
grep -q 'What is Message Vault?' docs/dist/vault/user/index.html && echo OK
```

- [ ] **Step 5: Commit**

```bash
git add docs/src/content/docs/vault/user/index.mdx
git commit -m "docs: replace User Guide splash with a map page"
```
