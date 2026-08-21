# Documentation Layers Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make https://bitrealm.io/ the only full copy of CLI and format docs, rewrite the GitHub root and crate READMEs as short front doors, and ship the work as four reviewable commits matching the four PRs in the spec.

**Architecture:** Root files and crate READMEs stay on GitHub. CLI manpages become committed Starlight pages under `docs/src/content/docs/reference/cli/`. Format and mapping docs become a second Starlight topic under `/formats/`. Crate `docs/` folders and the three maintainer format/matrix files are deleted. In-repo links (including rustdoc) point at bitrealm.io.

**Tech Stack:** Astro Starlight 0.41, `starlight-sidebar-topics`, GitHub Markdown, Contributor Covenant 2.1.

**Spec:** `docs/superpowers/specs/2026-08-12-docs-layers-overhaul-design.md`

## Global Constraints

- User Guide pages and URLs stay as they are except link fixes required by the moves.
- Call it “the vault” and “the desktop app”; never `message-vault-rs` or `message-vault-io` as product names.
- JSONL is “JSON Lines” on user-facing pages. Crate READMEs and Format Reference may name binaries and crate folders.
- No `CHANGELOG.md`. No issue/PR templates. No READMEs for `src-tauri` or `web/`.
- Do not invent format pages for WhatsApp, iMessage, or OpenExtract.
- Do not move `docs/maintainers/architecture/message-ir.md` onto the public site.
- Do not change runtime behavior. Rustdoc comment URL updates are documentation only.
- Good Docs templates apply to root README, CONTRIBUTING, CODE_OF_CONDUCT, and crate READMEs. Moved CLI/format pages keep current headings; add Starlight frontmatter, fix links, voice pass.
- After each task that touches `docs/`: `cd docs && npm run check && npm run build`.
- Four commits, in order, so the work can become four PRs.

## File map

| Area | Create | Modify | Delete |
|------|--------|--------|--------|
| Root | `CODE_OF_CONDUCT.md` | `README.md`, `CONTRIBUTING.md` | — |
| CLI site | `docs/src/content/docs/reference/cli/<slug>.md` (10 files, currently gitignored) | `docs/package.json`, `.gitignore`, `docs/maintainers/README.md`, `CONTRIBUTING.md` (CLI rule) | `docs/scripts/sync-cli-reference.mjs`; every `docs/MANPAGE.md` and `MESSAGE_REEXPORTER.md` |
| Format site | `docs/src/content/docs/formats/**`, `docs/astro.config.mjs` topics | `docs/package.json` (plugin), rustdoc in `mail` and `sbr`, `message-ir.md`, root README format link | crate remaining `docs/`; `docs/maintainers/exporter-matrix.md`; `docs/maintainers/formats/` |
| Crate READMEs | missing library/server READMEs | every existing workspace crate README | — |

---

### Task 1: Root front door

**Files:**
- Create: `CODE_OF_CONDUCT.md`
- Modify: `README.md`, `CONTRIBUTING.md`
- Test: visual review of the three files; no docs-site build required unless `docs/` is untouched (it is)

**Interfaces:**
- Consumes: guidebook facts at https://bitrealm.io/ (install, quick start, Settings → Account, port 8080)
- Produces: CoC for later crate/root links; CONTRIBUTING still documents `sync:cli` until Task 2; README may still link `docs/maintainers/exporter-matrix.md` until Task 3

- [ ] **Step 1: Write `CODE_OF_CONDUCT.md`**

Use the official [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) text in full. In “Enforcement”, do not invent an email. Use:

```text
Instances of abusive, harassing, or otherwise unacceptable behavior may be
reported by contacting the maintainers of the bitrealm-dev/message-vault
GitHub repository (https://github.com/bitrealm-dev/message-vault).
```

- [ ] **Step 2: Rewrite `README.md`**

Replace the file. Do not keep the WSL/rustup/nvm/apt block. Exact content:

```markdown
# Message Vault

Extract messages from phone backups, import them into a local vault, and browse them in a website you control.

## What it is

Message Vault has two parts that run on a machine you control:

- **The vault** — a Docker container with a REST API and a SQLite database. It stores your messages and serves them through a website in your browser.
- **The desktop app** — a program that extracts messages from Apple and Android phone backups, converts them between formats, and imports them into the vault.

There is no cloud account. Messages are not uploaded to a Message Vault service. The vault you run has a local login (the demo user, or an account you create).

## Who it is for

People who have phone backups and want to extract, convert, and browse those messages locally.

## Getting started

**Desktop app:** Download the archive for your operating system from the latest [Release](https://github.com/bitrealm-dev/message-vault/releases). Extract it, keep every file in the same folder, and run the app. Install steps: [Install the desktop app](https://bitrealm.io/introduction/install/).

**Demo vault:**

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e DEMO_DATA=true \
  -v message-vault-data:/app/data \
  bitrealm/message-vault:latest
```

Open **http://localhost:8080** and sign in with username `demo` and an empty password. The website and the API share that origin. More: [Quick start](https://bitrealm.io/introduction/quick-start/).

## What you can do

- **Extract** Apple Messages (`chat.db` or an iPhone backup), Android SMS/MMS from SMS Backup & Restore XML, and WhatsApp. GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+ are limited rescue imports for files you already have.
- **Convert** an existing Message Vault folder between JSON Lines, JSON, CSV, EML, MBOX, and XML.
- **Import, browse, and export** using the desktop app and the vault.

Full guide: **https://bitrealm.io/**

Converter capability details today: [exporter capability matrix](docs/maintainers/exporter-matrix.md) (moves to the Format Reference topic in a later change).

## From source

Build and run instructions: [CONTRIBUTING.md](CONTRIBUTING.md).

## Get involved

- [CONTRIBUTING.md](CONTRIBUTING.md) — setup, tests, and pull-request rules
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)

## License

This project is licensed under the GNU Affero General Public License v3.0 — see [LICENSE](LICENSE). `imessage-ir-exporter` still depends on `imessage-database` (GPL-3.0-or-later); the combined binaries are AGPL-3.0.
```

- [ ] **Step 3: Restructure `CONTRIBUTING.md`**

Keep working commands. Apply these fact fixes while reordering:

1. After the title, add: this project follows [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
2. Node.js 22+ is required for `web/` and `docs/`, not only the docs site.
3. Vault server: API and website share **http://localhost:8080**. Remove the claim that the web UI is on port 3000.
4. API tokens: **Settings → Account**, not Settings → Access.
5. Leave the `sync:cli` / manpage instructions in place until Task 2.
6. Keep WSL, Linux packages, helper binaries, test, workspace map, contribution rules, troubleshooting.
7. Further reading still links `docs/maintainers/exporter-matrix.md` until Task 3.

Do not copy WSL into README. Do not invent a changelog.

- [ ] **Step 4: Review the three files**

Confirm README has no rustup/nvm/apt. Confirm CONTRIBUTING still has those. Confirm CoC has no invented email.

- [ ] **Step 5: Commit**

```bash
git add README.md CONTRIBUTING.md CODE_OF_CONDUCT.md
git commit -m "$(cat <<'EOF'
docs: rewrite GitHub front door and add code of conduct

The root README mixed release, Docker, and WSL setup and drifted from the
product. Point visitors at the site and contributing; add Contributor Covenant.
EOF
)"
```

---

### Task 2: CLI pages on the site

**Files:**
- Create (committed): `docs/src/content/docs/reference/cli/{imessage-ir-exporter,sms-backup-restore-exporter,whatsapp-exporter,message-reexporter,vault-push,vault-pull,go-sms-pro-exporter,imazing-exporter,openextract-exporter,sms-backup-plus-exporter}.md`
- Modify: `.gitignore` (remove the two CLI ignore lines), `docs/package.json` (remove `sync:cli` and `predev`/`precheck`/`prebuild`), `CONTRIBUTING.md` (CLI rule), `docs/maintainers/README.md`
- Create: `crates/libs/reexport/README.md`
- Modify crate READMEs: every exporter under `crates/exporters/*/README.md`, `crates/cli/vault-push/README.md`, `crates/cli/vault-pull/README.md`
- Delete: `docs/scripts/sync-cli-reference.mjs`; `crates/**/docs/MANPAGE.md`; `crates/libs/reexport/docs/MESSAGE_REEXPORTER.md`
- Leave crate format files (`INPUT_FORMAT.md`, etc.) until Task 3

**Interfaces:**
- Consumes: current manpage bodies and `docs/scripts/sync-cli-reference.mjs` frontmatter/title map
- Produces: stable `/reference/cli/<slug>/` pages; crate READMEs that link those URLs only (not format URLs yet)

- [ ] **Step 1: Generate the CLI pages once, then stop generating**

```bash
cd docs && npm ci && npm run sync:cli
```

Expected: ten `.md` files appear under `docs/src/content/docs/reference/cli/` besides `index.md`.

- [ ] **Step 2: Un-ignore and keep those files**

In `.gitignore`, delete:

```
/docs/src/content/docs/reference/cli/*.md
!/docs/src/content/docs/reference/cli/index.md
```

- [ ] **Step 3: Remove the sync script and npm hooks**

Delete `docs/scripts/sync-cli-reference.mjs`.

Set `docs/package.json` scripts to:

```json
"scripts": {
  "dev": "astro dev",
  "check": "astro check",
  "build": "astro build",
  "preview": "astro preview"
}
```

- [ ] **Step 4: Fix relative crate links inside the committed CLI pages**

The old script rewrote relative links to GitHub blob URLs. After manpages are deleted, remaining links to `INPUT_FORMAT.md` / `IMPORT_MAPPING.md` will die in Task 3. For Task 2, leave those GitHub blob URLs (they still exist). Voice pass: replace “GUI Vault tab” / Slint wording if present.

- [ ] **Step 5: Update CONTRIBUTING and maintainer index**

Replace “edit crate manpage then sync” with: edit `docs/src/content/docs/reference/cli/<command>.md` directly. Remove `npm run sync:cli`. `docs/maintainers/README.md` must not say generated pages are not edited directly.

- [ ] **Step 6: Rewrite CLI crate READMEs**

Each file: what it is, `cargo test -p <pkg>` and `cargo run -p <pkg> -- --help`, link `https://bitrealm.io/reference/cli/<slug>/`, license. No flag lists. No `docs/MANPAGE.md`.

Packages and slugs:

| Package | README path | CLI slug |
|---------|-------------|----------|
| `imessage-ir-exporter` | `crates/exporters/imessage-ir-exporter/README.md` | `imessage-ir-exporter` (license AGPL-3.0; `imessage-database` is GPL-3.0-or-later) |
| `sms-backup-restore-exporter` | `crates/exporters/sms-backup-restore-exporter/README.md` | `sms-backup-restore-exporter` |
| `whatsapp-exporter` | `crates/exporters/whatsapp-exporter/README.md` | `whatsapp-exporter` |
| `go-sms-pro-exporter` | `crates/exporters/go-sms-pro-exporter/README.md` | `go-sms-pro-exporter` |
| `imazing-exporter` | `crates/exporters/imazing-exporter/README.md` | `imazing-exporter` |
| `openextract-exporter` | `crates/exporters/openextract-exporter/README.md` | `openextract-exporter` |
| `sms-backup-plus-exporter` | `crates/exporters/sms-backup-plus-exporter/README.md` | `sms-backup-plus-exporter` |
| `vault-push` | `crates/cli/vault-push/README.md` | `vault-push` — desktop app **Import** screen, not Vault tab |
| `vault-pull` | `crates/cli/vault-pull/README.md` | `vault-pull` — desktop app **Export** screen, not Vault Export / Query |
| `message-reexport` | `crates/libs/reexport/README.md` (new) | `message-reexporter` — desktop app **Format** tab |

Template (swap names):

```markdown
# sms-backup-restore-exporter

Convert an SMS Backup & Restore (SyncTech) XML backup into JSON Lines, JSON, CSV, EML, MBOX, or SMS Backup & Restore XML.

The desktop app Extract Messages screen uses this crate as a library. The `sms-backup-restore-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p sms-backup-restore-exporter
cargo run -p sms-backup-restore-exporter --features cli -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/reference/cli/sms-backup-restore-exporter/

## License

AGPL-3.0. See the repository root `LICENSE`.
```

Check each crate’s `Cargo.toml` for whether `--features cli` is required (`cli` feature is default-on for exporters). If default features include `cli`, `cargo run -p <pkg> -- --help` is enough.

- [ ] **Step 7: Delete manpages only**

Delete every `MANPAGE.md` and `crates/libs/reexport/docs/MESSAGE_REEXPORTER.md`. Keep `INPUT_FORMAT.md`, `IMPORT_MAPPING.md`, `DESIGN.md`, `FORMAT.md`, `REEXPORT.md` until Task 3.

- [ ] **Step 8: Verify**

```bash
cd docs && npm run check && npm run build
rg -n 'sync:cli|docs/MANPAGE.md' --glob '!docs/superpowers/**'
```

Expected: check/build pass; no live `sync:cli` or `docs/MANPAGE.md` instructions outside this spec/plan.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
docs: commit CLI reference pages and drop manpage sync

CLI pages were generated at build time and gitignored, so the sidebar pointed
at missing files. Edit those pages on the site; crate READMEs link there.
EOF
)"
```

Do not add unrelated untracked files (`.cursor/skills/`).

---

### Task 3: Format Reference topic

**Files:**
- Create: pages under `docs/src/content/docs/formats/` as in the spec table
- Modify: `docs/astro.config.mjs` (install and configure `starlight-sidebar-topics`), `docs/package.json` / lockfile, `README.md` (matrix link → https://bitrealm.io/formats/), `CONTRIBUTING.md`, `docs/maintainers/README.md`, `docs/maintainers/architecture/message-ir.md`, `crates/libs/mail/src/lib.rs`, `crates/libs/sbr/src/lib.rs`, `crates/core/message-vault-io-core/src/config.rs` (comment URL only)
- Delete: remaining crate `docs/` files and directories; `docs/maintainers/exporter-matrix.md`; `docs/maintainers/formats/mail-archive.md`; `docs/maintainers/formats/sms-backup-restore-xml.md`; empty `docs/maintainers/formats/`
- Update CLI crate READMEs that now have format pages to add the extra bitrealm.io links

**Site paths (create with Starlight frontmatter `title` + `description`):**

| Path | Source |
|------|--------|
| `docs/src/content/docs/formats/index.md` | `docs/maintainers/exporter-matrix.md` |
| `docs/src/content/docs/formats/mail-archive.md` | `docs/maintainers/formats/mail-archive.md` |
| `docs/src/content/docs/formats/sms-backup-restore-xml.md` | `docs/maintainers/formats/sms-backup-restore-xml.md` |
| `docs/src/content/docs/formats/convert.md` | `crates/libs/reexport/docs/REEXPORT.md` |
| `docs/src/content/docs/formats/sms-backup-restore/input.md` | SMS Backup & Restore `INPUT_FORMAT.md` |
| `docs/src/content/docs/formats/sms-backup-restore/mapping.md` | SMS Backup & Restore `IMPORT_MAPPING.md` |
| `docs/src/content/docs/formats/sms-backup-plus/format.md` | SMS Backup+ `FORMAT.md` |
| `docs/src/content/docs/formats/sms-backup-plus/mapping.md` | SMS Backup+ `IMPORT_MAPPING.md` |
| `docs/src/content/docs/formats/go-sms-pro/mapping.md` | GO SMS Pro `IMPORT_MAPPING.md` |
| `docs/src/content/docs/formats/imazing/input.md` | iMazing `INPUT_FORMAT.md` |
| `docs/src/content/docs/formats/imazing/design.md` | iMazing `DESIGN.md` |

Keep current headings and tables. Fix `crates/message/` paths to `crates/libs/`. Link JSONL layout to `/reference/export-structure/`. Link CLI flags to `/reference/cli/...`. Voice pass: old GUI names, old repo names.

No stubs at the old maintainer paths.

- [ ] **Step 1: Install `starlight-sidebar-topics`**

```bash
cd docs && npm install starlight-sidebar-topics
```

Configure two topics in `docs/astro.config.mjs` per the plugin docs for Starlight 0.41: **User Guide** (current sidebar) and **Format Reference** (the `/formats/` tree). CLI stays in User Guide → Reference.

- [ ] **Step 2: Copy and frontmatter the format pages**

Each file starts with:

```markdown
---
title: "<page title>"
description: "<one sentence>"
---
```

Then the existing body with links rewritten to site paths or `https://github.com/bitrealm-dev/message-vault/blob/main/crates/libs/...`.

- [ ] **Step 3: Delete sources and retarget links**

Delete crate `docs/` directories that become empty. Delete the three maintainer files and `docs/maintainers/formats/`.

Replace in-repo references:

- `docs/maintainers/exporter-matrix.md` → https://bitrealm.io/formats/
- `docs/maintainers/formats/mail-archive.md` → https://bitrealm.io/formats/mail-archive/
- `docs/maintainers/formats/sms-backup-restore-xml.md` → https://bitrealm.io/formats/sms-backup-restore-xml/

including rustdoc in `crates/libs/mail/src/lib.rs` and `crates/libs/sbr/src/lib.rs`.

- [ ] **Step 4: Add format links on crate READMEs that have format pages**

SMS Backup & Restore, SMS Backup+, GO SMS Pro, iMazing, `message-reexport` (convert page).

- [ ] **Step 5: Verify**

```bash
cd docs && npm run check && npm run build
rg -n 'exporter-matrix\.md|maintainers/formats/|docs/INPUT_FORMAT\.md|docs/IMPORT_MAPPING\.md|docs/REEXPORT\.md' --glob '!docs/superpowers/**'
```

Expected: build passes; no live instructions pointing at deleted files.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
docs: add Format Reference topic and remove crate format docs

Format and mapping notes lived only on GitHub. Publish them on the site and
delete the duplicate maintainer and crate copies.
EOF
)"
```

---

### Task 4: Remaining crate READMEs

**Files:**
- Create: `crates/libs/ir/README.md`, `crates/libs/ir-format/README.md`, `crates/libs/phone/README.md`, `crates/libs/mail/README.md`, `crates/libs/csv/README.md`, `crates/libs/sbr/README.md`, `crates/libs/go-sms-mms/README.md`, `crates/vault/server/README.md`
- Modify: `crates/libs/contacts/README.md`, `crates/libs/media/README.md`, `crates/libs/obfuscate/README.md`, `crates/core/message-vault-io-core/README.md`, `crates/vault/demo-seed/README.md`, `crates/message-vault-io-gui/README.md`
- Do not add READMEs for `src-tauri` or `web/`
- Do not invent public Starlight pages

**Interfaces:**
- Consumes: crate `Cargo.toml` package names; existing maintainer writeups
- Produces: every workspace member has a fact-checked README

Four blocks: what it is, build/test this crate, docs (site or “used by …”), license.

Library docs block example:

```markdown
## Docs

This crate is a library. The SMS Backup & Restore converter, other exporters, and the desktop app use it.

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).
Shared message types: [docs/maintainers/architecture/message-ir.md](../../../docs/maintainers/architecture/message-ir.md) (for `message-ir` only).
```

Vault server README points at:

- https://bitrealm.io/set-up-the-server/docker-install/
- https://bitrealm.io/reference/server-cli/
- `cargo test -p message-vault-server`

`message-vault-io-gui`: deprecated banner, do not develop, pointer to the Tauri desktop app (`src-tauri/` + `web/`), keep `cargo run -p message-vault-io-gui` for historical reference, strip the Slint look-and-feel essay.

Fact-check:

- `message-vault-io-core`: used by `src-tauri/`, not the Slint GUI as the primary consumer
- `contacts`: drop `contacts-validate` / `csv-ingest` as product names unless the binary still exists (`contacts-validate` is a bin in that crate — mention it as a helper binary, not a product)
- `media`: convert/compress run during format packaging, not a GUI CSV post-step
- `vault-push`/`vault-pull` already fixed in Task 2

- [ ] **Step 1: Write the missing library and server READMEs**
- [ ] **Step 2: Rewrite the stale existing library/core/demo/gui READMEs**
- [ ] **Step 3: Confirm every `Cargo.toml` workspace member has a README**

```bash
# from repo root
python3 - <<'PY'
from pathlib import Path
text = Path("Cargo.toml").read_text()
# members listed as quoted paths
import re
members = re.findall(r'"crates/[^"]+"', text)
missing = []
for m in members:
    p = Path(m.strip('"')) / "README.md"
    if not p.exists():
        missing.append(str(p))
print("missing:", missing or "none")
PY
```

Expected: `missing: none`

- [ ] **Step 4: Grep stale product names in crate READMEs**

```bash
rg -n 'Vault tab|Slint desktop GUI|csv-ingest|docs/MANPAGE.md' crates --glob '**/README.md'
```

Expected: only `message-vault-io-gui` README may mention Slint, and only as deprecated.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "$(cat <<'EOF'
docs: add fact-checked READMEs for every workspace crate

GitHub crate folders had missing or stale READMEs. Each crate now states
what it is, how to test it, and where the long docs live.
EOF
)"
```

---

## Spec coverage

| Spec section | Task |
|--------------|------|
| Root README rewrite, CONTRIBUTING, CoC, no CHANGELOG | 1 |
| CLI committed, sync removed, CLI crate READMEs, manpages deleted | 2 |
| Format topic, plugin, delete crate/maintainer format copies, no stubs, rustdoc URLs | 3 |
| Remaining crate READMEs, internal-library split, deprecated GUI | 4 |
| User Guide unchanged | all (do not edit those pages except if a link must change) |
| Verification `npm run check && npm run build` | 2, 3 |

## After all tasks

Use superpowers:finishing-a-development-branch. Prefer four PRs stacked or split from these four commits, matching the spec delivery section.
