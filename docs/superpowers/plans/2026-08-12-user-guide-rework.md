# User Guide Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the bitrealm.io User Guide with a try-it tutorial plus a “How do I…” handbook, and move CLI, formats, Compose, and internals under a Developer header topic.

**Architecture:** New Markdown under `docs/src/content/docs/get-started/`, `prepare-a-backup/`, `how-to/`, and `developer/`. `docs/astro.config.mjs` lists User Guide then Developer. `/formats/` and `/reference/` URLs stay. Old User Guide files are deleted with no redirects.

**Tech Stack:** Astro Starlight, `starlight-sidebar-topics`.

**Spec:** `docs/superpowers/specs/2026-08-12-user-guide-rework-design.md`

## Global Constraints

- Call it “the vault” and “the desktop app”. Never `message-vault-rs` or `message-vault-io` as product names.
- JSONL is “JSON Lines” on User Guide pages.
- `demo` is a second account on the same instance. Do not instruct `DEMO_DATA=false` as the way to go personal.
- Do not write “no account is required.” Local username/password is the vault login.
- Happy-path Import starts from a phone backup. JSONL folders are handbook.
- Try-it recommends the website on port 8080. The desktop app is taught at Import.
- No redirects. No new screenshots. No runtime code changes.
- Do not copy `CONTRIBUTING.md` onto the site.
- User Guide local auth only (no Hanko).
- After pages exist: `cd docs && npm run check && npm run build`.

## File map

| Area | Create | Delete |
|------|--------|--------|
| Tutorial | `get-started/*`, `prepare-a-backup/*`, `import-from-a-backup.md`, `browse-your-messages.md`, rewrite `index.mdx` | `introduction/`, `set-up-the-server/`, `prepare-your-backups/` |
| Handbook | `how-to/*`, `glossary.md` | `use-the-desktop-app/`, `browse/`, `troubleshooting.md` |
| Developer | `developer/run-from-source.md`, `developer/docker-compose.md` | — |
| Chrome | `docs/astro.config.mjs` topics | — |
| Links | README, crate READMEs, `docs/maintainers/gui.md`, `signing.md`, CLI/database/config pages | — |

---

### Task 1: New pages + sidebar

**Files:** Create every path in the spec tables. Modify `docs/astro.config.mjs`.

**Interfaces:**
- Consumes: current backup/search/settings/import prose; Import source labels in `web/src/lib/exportSources.ts`
- Produces: Starlight slugs listed in the spec

- [ ] **Step 1: Write Developer pages** `developer/run-from-source.md`, `developer/docker-compose.md`
- [ ] **Step 2: Write tutorial + handbook + splash** (all spec URLs)
- [ ] **Step 3: Point `starlight-sidebar-topics` at the new trees** (User Guide + Developer; Format Reference label gone)
- [ ] **Step 4: Delete old User Guide directories listed in the spec**
- [ ] **Step 5: Retarget live links** (README, crate READMEs, maintainers, `/reference/*` internals). Leave historical specs/plans as history.
- [ ] **Step 6: Add audience one-liners** on `formats/index.md` and `reference/cli/index.md`. Point vault-push/pull at handbook extract/export.
- [ ] **Step 7: `npm run check && npm run build`**
- [ ] **Step 8: Grep User Guide for forbidden strings; grep repo for deleted path URLs in live files**
- [ ] **Step 9: Commit**

Backup platform pages keep current “what you need / how to get it / limitations” facts. Change “next step” from Extract Messages to **Import** with the matching source label (iPhone - iOS, iMessage - macOS, WhatsApp - iOS/Android, SMS Backup & Restore).

Handbook contacts: Import name-fill options. Do not put `import-contacts` cargo in the User Guide; that stays on `/reference/server-cli/`.

Handbook troubleshooting: desktop start, helpers, Gatekeeper/SmartScreen, cannot reach 8080, SQLITE_CANTOPEN. CLI/API schema errors stay on Developer pages (API / vault-push), not in the User Guide.
