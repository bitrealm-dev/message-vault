# Documentation layers: GitHub front door, crate READMEs, and Format Reference

## Context

Message Vault documentation lives in three places that do not share one source of truth.

1. **GitHub front door** — `README.md` and `CONTRIBUTING.md` at the repository root. There is no `CODE_OF_CONDUCT.md`. There is no `CHANGELOG.md`; GitHub Releases are the version history.

2. **Crate pages on GitHub** — Each crate under `crates/` is its own folder. Some have a `README.md`. Some also have a `docs/` folder with command manpages, vendor input-format notes, and field-mapping tables. Opening `crates/exporters/sms-backup-restore-exporter/` on GitHub is a different experience from opening `crates/libs/ir/`, which has no README.

3. **The public site** — Astro Starlight under `docs/`, live at https://bitrealm.dev/. That site is the user guidebook from [the 2026-08-07 rewrite](2026-08-07-docs-rewrite-design.md). The sidebar already lists per-command CLI pages, but those files are generated at build time from crate manpages and are not committed (they are gitignored except `reference/cli/index.md`). Format details still live in crate `docs/` folders and in `docs/maintainers/exporter-matrix.md`, which is not on the public site.

A visitor who starts on GitHub and a visitor who starts on bitrealm.dev do not see the same facts. Crate READMEs point at `docs/MANPAGE.md`. The root README mixes release download, Docker demo, and WSL contributor setup. Several crate READMEs still describe the old Slint GUI or tools that are no longer how the product is named.

This overhaul makes the public site the only full copy of converter, format, and CLI documentation. Crate `docs/` folders are removed after that content is on the site. Root files and crate READMEs become short GitHub front doors that point at the site. Work follows [The Good Docs Project](https://www.thegooddocsproject.dev/template) for README, contributing, and code-of-conduct files.

This spec does not replace the 2026-08-07 guidebook. User Guide pages and URLs stay as they are, except for link fixes required by the moves below.

## Goals

- One full copy of CLI, format, and converter-capability documentation: https://bitrealm.dev/
- A GitHub visitor can start from the root README or any workspace crate folder without hitting a dead `docs/MANPAGE.md` link
- Root `README.md` matches the product as it works today (vault + desktop app, local login, current install paths)
- Every Cargo workspace crate has a short, fact-checked README
- The empty Reference → CLI sidebar slots are filled with committed pages
- Format and mapping docs are a second Starlight topic, not mixed into the user guidebook sidebar

## Non-goals

- Adding `CHANGELOG.md` (GitHub Releases stay the version history)
- Adding GitHub issue templates or pull-request templates
- Rewriting the user guidebook (Introduction through Browse)
- Rebuilding every manpage into a full Good Docs “reference” skeleton
- Inventing format pages for WhatsApp, iMessage, or OpenExtract (those crates have manpages only)
- Adding READMEs for `src-tauri` or `web/` (not workspace crates)
- Moving `docs/maintainers/architecture/message-ir.md` onto the public site
- Changing runtime code, exporters, or the desktop app

## Architecture: three layers

```text
GitHub root          README, CONTRIBUTING, CODE_OF_CONDUCT
                     Short. Points at the site for the long guide.

Crate folder         README.md only
                     What the crate is, how to test it, link to the site
                     or “library used by …”. No docs/ folder.

bitrealm.dev         User Guide (unchanged IA)
                     Reference → CLI (committed manpage pages)
                     Format Reference (new topic)
```

Root files stay on GitHub because they are what a repository visitor sees first. They do not copy the user guidebook.

A crate README is a front door, not a second manual. After crate `docs/` folders are removed, READMEs must not link to `docs/MANPAGE.md`.

Voice from the 2026-08-07 spec stays in force on user-facing pages: “the vault” and “the desktop app”; never the old repo names `message-vault-rs` or `message-vault-io` as product names. JSONL is introduced as JSON Lines on user pages. Crate READMEs and Format Reference pages may name binaries, crate folders, and field names because those readers opened a crate or a format topic.

## Public site

Install `starlight-sidebar-topics` so the header offers two topics.

### Topic 1: User Guide

Keep today’s groups and URLs: Introduction, Prepare your backups, Set up the server, Use the desktop app, Browse the vault, Reference (including CLI, API, config, database, export structure, CSV columns, troubleshooting).

CLI pages stay under Reference, not in Format Reference. Existing URLs such as `/reference/cli/sms-backup-restore-exporter/` do not change.

JSONL layout stays at `/reference/export-structure/`. Format pages link there instead of copying it.

### Topic 2: Format Reference

Paths under `/formats/`:

| Site path | Source today |
|-----------|----------------|
| `/formats/` (overview) | `docs/maintainers/exporter-matrix.md` |
| `/formats/mail-archive/` | `docs/maintainers/formats/mail-archive.md` |
| `/formats/sms-backup-restore-xml/` | `docs/maintainers/formats/sms-backup-restore-xml.md` |
| `/formats/convert/` | `crates/libs/reexport/docs/REEXPORT.md` (how conversion works, not CLI flags) |
| `/formats/sms-backup-restore/input/` | `crates/exporters/sms-backup-restore-exporter/docs/INPUT_FORMAT.md` |
| `/formats/sms-backup-restore/mapping/` | `crates/exporters/sms-backup-restore-exporter/docs/IMPORT_MAPPING.md` |
| `/formats/sms-backup-plus/format/` | `crates/exporters/sms-backup-plus-exporter/docs/FORMAT.md` |
| `/formats/sms-backup-plus/mapping/` | `crates/exporters/sms-backup-plus-exporter/docs/IMPORT_MAPPING.md` |
| `/formats/go-sms-pro/mapping/` | `crates/exporters/go-sms-pro-exporter/docs/IMPORT_MAPPING.md` |
| `/formats/imazing/input/` | `crates/exporters/imazing-exporter/docs/INPUT_FORMAT.md` |
| `/formats/imazing/design/` | `crates/exporters/imazing-exporter/docs/DESIGN.md` |

WhatsApp, iMessage, and OpenExtract do not get new format pages. The overview table plus the existing CLI page is enough.

Moved pages keep their current headings and tables. They get Starlight frontmatter, working links, and a voice pass (old paths, old GUI names, `crates/message/` which is no longer the library location). They are not rebuilt into a Good Docs reference skeleton.

### CLI pages: stop generating, start committing

Today `docs/scripts/sync-cli-reference.mjs` copies crate `docs/MANPAGE.md` (and `MESSAGE_REEXPORTER.md`) into `docs/src/content/docs/reference/cli/` at `predev` / `precheck` / `prebuild`. The root `.gitignore` ignores those generated files except `index.md`. `CONTRIBUTING.md` tells people not to edit the generated files.

After this overhaul:

- The Markdown files under `docs/src/content/docs/reference/cli/` are committed and edited there
- `docs/scripts/sync-cli-reference.mjs` is removed
- The npm hooks that call `sync:cli` are removed
- The `.gitignore` rules for those CLI pages are removed
- Each page keeps the frontmatter the sync script already produced (title, description, heading demotion). Relative crate links become site URLs or current GitHub blob paths
- `docs/src/content/docs/reference/cli/index.md` stays and remains hand-edited

Command slugs stay as in `docs/astro.config.mjs`: `imessage-ir-exporter`, `sms-backup-restore-exporter`, `whatsapp-exporter`, `message-reexporter`, `vault-push`, `vault-pull`, `go-sms-pro-exporter`, `imazing-exporter`, `openextract-exporter`, `sms-backup-plus-exporter`.

### Stays on GitHub only

These files remain under `docs/maintainers/` and are not copied onto the public site:

- `developing.md`, `signing.md`, `gui.md`, `roadmap.md`
- `architecture/message-ir.md` (shared message-model writeup for contributors)
- `README.md` (maintainer index), updated so it no longer says crate manpages are the source for CLI pages

### Delete the maintainer copies (no stubs)

After the Format Reference pages exist, delete:

- `docs/maintainers/exporter-matrix.md`
- `docs/maintainers/formats/mail-archive.md`
- `docs/maintainers/formats/sms-backup-restore-xml.md`

and the empty `docs/maintainers/formats/` directory.

Retarget every in-repo link to the matching https://bitrealm.dev/formats/… URL. That includes `README.md`, `CONTRIBUTING.md`, `docs/maintainers/README.md`, `docs/maintainers/architecture/message-ir.md`, and rustdoc comments in `crates/libs/mail` and `crates/libs/sbr`.

Do not leave pointer files at the old paths. GitHub blob URLs on `main` for those files will 404. That is accepted.

## Root front door

Templates: [The Good Docs Project](https://www.thegooddocsproject.dev/template) README, contributing, and code of conduct.

### `README.md` — rewrite, do not lightly reorder

The current file is out of date as a repository front door. It mixes three audiences (release download, Docker demo, WSL contributor setup), contradicts itself (it says to build in release mode then runs `cargo tauri dev`), pins Node.js 24 while the rest of the repo documents Node 22+, names converters as if those were the user-facing product, and says “no account is required” in a way that is easy to misread.

The rewritten README follows the Good Docs README shape and matches the guidebook:

1. **What it is** — vault + desktop app; messages stay on a machine you control. There is no cloud account. The vault you run has a local login (demo, or an account you create).
2. **Who it is for** — people with phone backups who want to extract, convert, and browse locally.
3. **Getting started** — two short paths only: download the desktop app from GitHub Releases (link [Install](https://bitrealm.dev/introduction/install/)), and run the demo vault with the same `docker run` the quick-start page already uses. No WSL, rustup, nvm, or apt lists.
4. **What you can do** — extract (Apple Messages, SMS Backup & Restore, WhatsApp; rescue imports named as limited), convert formats, import/browse/export. User-facing names, not crate names. Link https://bitrealm.dev/ for the long guide. After Format Reference exists, link `/formats/` instead of `docs/maintainers/exporter-matrix.md`.
5. **From source** — one sentence and a link to `CONTRIBUTING.md`.
6. **Get involved** — `CONTRIBUTING.md` and `CODE_OF_CONDUCT.md`.
7. **License** — MIT for most crates; `imessage-ir-exporter` is GPL-3.0-or-later, so the desktop app binary includes GPL code.

WSL, Linux packages, Node version, and `cargo tauri` live only in `CONTRIBUTING.md`, checked against current prerequisites (Rust 1.85+, Node 22+). The README does not keep a second copy that can drift.

### `CONTRIBUTING.md`

Keep the commands that already work. Reorder into the template’s sections: welcome, link to the code of conduct, prerequisites, clone/build/run/test, workspace map, PR rules, troubleshooting.

Rule updates that must match the later PRs:

- Point at `CODE_OF_CONDUCT.md` in the root-files PR
- CLI changes are edited in `docs/src/content/docs/reference/cli/`, not in a crate `docs/MANPAGE.md`, in the CLI PR
- Architecture, signing, and release steps stay under `docs/maintainers/`

### `CODE_OF_CONDUCT.md`

Add Contributor Covenant 2.1 at the repository root. Enforcement is through GitHub (maintainers of `bitrealm-dev/message-vault`). Do not invent a public email unless one already exists for the project.

`LICENSE` is unchanged.

## Crate README contract

Every Cargo workspace member listed in the root `Cargo.toml` `members` array gets a `README.md`. `src-tauri` is excluded from the workspace and is out of scope. `web/` is not a crate and is out of scope.

Each README is a front door, not a second manual. Existing crate READMEs are rewritten against current behavior, not only given new headings and URLs. Known stale claims to fix include:

- `vault-push` still says the GUI has a **Vault** tab (old Slint app). The supported UI is the Tauri desktop app **Import** screen.
- `message-vault-io-core` still says it exists for the Slint GUI. `src-tauri/` uses those form models and job helpers now.
- `message-contacts` still talks about `contacts-validate` and vault `csv-ingest`.
- `message-media` still describes convert/compress as a GUI post-step on CSV paths. Media transforms run inside format packaging for every output format.
- Exporter READMEs still point at `docs/MANPAGE.md`.

Four blocks on every crate README:

1. **What it is** — crate name, what it does, who uses it (CLI, library linked by the desktop app, vault server).
2. **Build and test this crate** — `cargo test -p <package>` and, if there is a binary, `cargo run -p <package> -- --help`. Workspace-wide setup stays in `CONTRIBUTING.md`.
3. **Docs** — see the split below.
4. **License** — MIT, except `imessage-ir-exporter` (GPL-3.0-or-later, and the desktop app therefore includes GPL code).

No manpage flags, mapping tables, or vendor XML field lists in the README.

### Docs links: public page vs internal library

The public site documents things a person runs or a format they need to understand: the user guidebook, CLI commands, and Format Reference.

Converters and CLI tools already have (or will have) a real site page. Their README points at `/reference/cli/<command>/` and, when a format page exists, at `/formats/...`.

The vault server already has User Guide pages (Docker install, first personal vault, demo) and `/reference/server-cli/`. Its README points at those URLs. Do not invent a Format Reference page for it.

Many workspace crates are not a command or a format. Examples: `message-ir` (shared conversation types), `message-phone` (phone-number parsing), `message-csv` (CSV helpers), `message-mail` (EML/MBOX). Do not invent a Starlight page for each. Those READMEs state:

1. What the library is
2. Which other crates use it
3. How to test this crate
4. Where to read more when changing the code — `CONTRIBUTING.md` for build/setup, or `docs/maintainers/` when a maintainer writeup already exists (for example `architecture/message-ir.md`)

Crates that need a new README because none exists today:

- **CLI PR:** `message-reexport` (the `message-reexporter` command)
- **Remaining-README PR:** `message-ir`, `message-ir-format`, `message-phone`, `message-mail`, `message-csv`, `message-sbr`, `message-go-sms-mms`, `message-vault-server`

Package directory names follow `crates/libs/` and `crates/vault/server/` as in the workspace.

### Exception: `message-vault-io-gui`

Keep a deprecated banner: do not change this crate; the supported UI is the Tauri desktop app. Strip the Slint look-and-feel essay. Leave run commands for historical reference only.

## Move and delete

CLI and format text move once, then the old copies go away.

1. Commit CLI pages; remove sync script, npm hooks, and gitignore exception; delete crate manpages (`MANPAGE.md`, `MESSAGE_REEXPORTER.md`) with the `docs/` folders that only held them.
2. Copy format sources into `docs/src/content/docs/formats/` with Starlight frontmatter; delete remaining crate `docs/` directories; delete the three maintainer files listed above.
3. Grep the repository for `docs/MANPAGE.md`, `docs/INPUT_FORMAT.md`, `sync:cli`, `exporter-matrix.md`, `maintainers/formats/`, and `crates/message/` (the old library path). Every live instruction becomes a site URL or a current `crates/libs/` path.

`docs/maintainers/` architecture, developing, signing, GUI notes, and roadmap are not deleted.

## Verification

After each PR that touches `docs/` or `docs/package.json`:

```bash
cd docs && npm run check && npm run build
```

After the CLI PR, that command must succeed without `sync:cli`.

Grep for `docs/MANPAGE.md`, `sync:cli`, `exporter-matrix.md`, `maintainers/formats/`, `message-vault-rs`, `message-vault-io` used as a current product name, and `crates/message/` as a live path. Those strings must not remain as instructions.

Root and crate READMEs: the first screen of the file must answer what the thing is, how to build/test it (or where contributing lives), and where the long docs are.

No new Cargo tests. This overhaul does not change runtime behavior.

## Delivery: four pull requests

1. **Root front door** — Rewrite `README.md`. Restructure `CONTRIBUTING.md`. Add `CODE_OF_CONDUCT.md`. WSL and toolchain details live only in contributing. The README may still link the exporter comparison table on GitHub until PR 3.

2. **CLI on the site** — Commit per-command pages. Remove sync. Rewrite or add READMEs for crates that have a CLI (including `message-reexport`) so they point at `/reference/cli/...`. Delete those manpages. `npm run check` passes without `sync:cli`.

3. **Format Reference** — Add `starlight-sidebar-topics`. Add `/formats/` pages. Delete remaining crate `docs/` folders and the three maintainer format/matrix files. Retarget in-repo links (including rustdoc) to bitrealm.dev. Point the root README at `/formats/`.

4. **Remaining crate READMEs** — Libraries, vault server, demo-seed, `message-vault-io-core`, deprecated Slint GUI. Fact-check against current code. No new public pages.

Done means: a GitHub visitor can start from the root README or any crate folder without a dead manpage link; bitrealm.dev has working CLI pages and a Format topic; `cd docs && npm run check && npm run build` succeeds; crate `docs/` folders and the three maintainer format/matrix files are gone.

## Relationship to the 2026-08-07 guidebook spec

| Topic | 2026-08-07 | This spec |
|-------|------------|-----------|
| User Guide IA and voice | Defines it | Unchanged |
| CLI in Reference | Sidebar slots; manpages generated from crates | Same URLs; pages committed; crate manpages deleted |
| Format / mapping / capability matrix | Maintainer GitHub files and crate `docs/` | Format Reference topic on the public site |
| Root README | Not in scope | Rewritten as the GitHub front door |
| Crate READMEs | Not in scope | Every workspace crate, fact-checked |
