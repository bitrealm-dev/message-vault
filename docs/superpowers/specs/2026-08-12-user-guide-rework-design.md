# User Guide rework: tutorial, handbook, and Developer topic

## Context

The public site at https://bitrealm.io/ is an Astro Starlight project under `docs/`. The User Guide (Introduction through Browse) was written against the 2026-08-07 unified-guidebook spec. That spec assumed two product stories: a demo vault (`DEMO_DATA=true`) and a separate empty personal vault (`DEMO_DATA=false`). It also treated Extract-to-JSONL and Import as equal first-run steps, and it mixed CLI, HTTP API, Compose, and format mapping into the same sidebar as “open the website and look around.”

The product is one vault process, one SQLite database, many accounts. Username `demo` (empty password, read-only sample conversations) is a login on that instance. A personal archive is a second account the reader creates after signing out. The website is enough to browse. Importing a phone backup needs the desktop app. The signed-in **Import** screen can read a backup and push in one run. Writing JSONL (JSON Lines) folders to disk, converting formats, and exporting from the vault are later tasks.

The [documentation layers spec](2026-08-12-docs-layers-overhaul-design.md) moved CLI pages and Format Reference onto the site and said the User Guide information architecture would stay. This spec is the User Guide change. Format files stay at `/formats/…`. CLI pages stay at `/reference/cli/…`. The header topic named “Format Reference” goes away; those pages sit under **Developer**.

This rewrite does not add redirects. Old User Guide URLs 404. GitHub `README.md` and crate READMEs that pointed at those URLs are updated in the same change.

## Goals

- A reader who finishes the User Guide tutorial has: started the vault, browsed as `demo` in the browser (or skipped), created an account on that same instance, installed the desktop app, imported a real backup, and opened Conversations.
- “Try it” is low friction and recommends the website. The desktop app is named in one sentence and taught when Import starts.
- CLI tools, the HTTP API, operator Docker (Compose, bind-mounts, SQLite browser on port 8081), format/mapping tables, `config.toml`, the database schema, JSONL layout, CSV columns, and server CLI live under a **Developer** header topic.
- Building from source is a real install path. The tutorial points at a Developer page that runs the vault and the desktop app. `CONTRIBUTING.md` stays the contributor checklist (tests, pull requests, WSL, Linux packages).

## Non-goals

- Changing runtime code, exporters, the desktop app, or the vault server
- HTTP redirects from old User Guide paths
- A new screenshot set
- Rewriting Format mapping tables or per-command CLI pages (sidebar parent and a one-line audience note only)
- Copying `CONTRIBUTING.md` onto the site
- Public pages for internal libraries (`message-ir`, `message-phone`, and the rest)
- Moving `docs/maintainers/` architecture, signing, GUI notes, or roadmap onto the site
- Documenting Hanko or other non-local auth modes in the User Guide (local username and password only)

## Architecture: two header topics

```text
User Guide     What it is → try it (demo in the browser) → own account
               → backup → desktop app → Import → browse
               then “How do I…” (search, convert, JSONL files, …)

Developer      Run from source, operator Docker, CLI, HTTP API,
               Formats (/formats/…), instance internals
```

`starlight-sidebar-topics` keeps two topics. The second topic’s `label` is **Developer**. Its `link` is `/developer/run-from-source/` so the first click is how to run the vault and the desktop app from a checkout, not a mapping table. Format page URLs stay under `/formats/`.

## Voice

Rules from the 2026-08-07 spec stay on User Guide pages:

| Call it | Do not call it |
|---|---|
| the vault | `message-vault-rs`, the backend, Next.js |
| the desktop app | `message-vault-io`, the GUI, the Tauri app |
| JSONL (JSON Lines) | message-ir, message-ir JSONL |

Project name: Message Vault (two words, title case). Repo: `github.com/bitrealm-io/message-vault`.

Developer pages may name binaries, crate folders, HTTP paths, and vendor field names.

## Facts the new pages must state

- Messages stay on a machine the reader controls. There is no Bitrealm cloud account. The vault has a **local** username and password. Do not write “no account is required.”
- `demo` is a read-only sample account. Creating a user does not require a second container or `DEMO_DATA=false`.
- `docker run` for Try the vault still uses `DEMO_DATA=true` so an empty volume seeds sample data. Changing `DEMO_DATA` on an existing volume does not add or remove accounts. The guide does not tell people to delete the volume in order to “go personal.”
- Try-it recommends **http://localhost:8080** in the browser. One sentence: viewing uses the website; importing later needs the desktop app.
- Happy-path Import starts from a **phone backup** (or WhatsApp database/key), not from a JSONL folder.
- Desktop Import uses the signed-in session. API tokens (Settings → Account) are for CLI (`vault-push` / `vault-pull`).
- The website and API share port **8080**. Do not teach port 3000. Do not teach “this is not Next.js.”
- Offline **Extract** and **Format** on the login screen exist; they belong in the handbook, not the happy path.

## User Guide: tutorial

New files under `docs/src/content/docs/`. Old User Guide files are deleted after their content is rewritten into this tree (backup-platform pages are rewritten in place of a copy-paste).

| Site path | Page |
|---|---|
| `/` | Splash: what it is; primary action Try the vault; secondary skip to your own messages |
| `/get-started/what-is-message-vault/` | Two pieces; local login; website for browse; app for import |
| `/get-started/why-you-provide-backups/` | Platforms do not expose message APIs; manual backups are the path |
| `/get-started/try-the-vault/` | One `docker run`; sign in as `demo`; browse. Skip link. One line about the app. Pointer: build from source → Developer |
| `/get-started/your-own-messages/` | Sign out; register on the same vault; onboarding handles. Skip landing |
| `/get-started/install-the-desktop-app/` | GitHub Releases; keep helpers beside the binary; pointer to Developer → run from source |
| `/prepare-a-backup/` | Hub to the four supported platform pages |
| `/prepare-a-backup/iphone-ipad/` | iPhone / iPad Messages backup or `chat.db` |
| `/prepare-a-backup/iphone-whatsapp/` | WhatsApp from an iPhone backup |
| `/prepare-a-backup/android-sms/` | SMS Backup & Restore XML |
| `/prepare-a-backup/android-whatsapp/` | Android WhatsApp database or encrypted backup plus key |
| `/import-from-a-backup/` | Desktop app, signed in as the new account, **Import** from a backup |
| `/browse-your-messages/` | Conversations after the first import; closes the tutorial |

Home hero actions: Try the vault (primary), Use your own messages (secondary). Do not put “Try the demo vault” and “Install” as three competing stories. Install is a tutorial step before Import, not a home-page equal to Try it.

“Already sure?” on Try the vault and on Home jumps to `/get-started/your-own-messages/`.

Rescue formats are not in this sequence.

## User Guide: handbook (“How do I…”)

| Site path | Page |
|---|---|
| `/how-to/search/` | Search box and operators, including `source:…` |
| `/how-to/contacts-and-labels/` | Contacts, labels, and how Import fills names |
| `/how-to/saved-searches/` | Saved groups in the sidebar (reusable searches) |
| `/how-to/trash/` | Soft-delete and undo |
| `/how-to/settings/` | Account, Profile, Storage, Appearance. Tokens: “for CLI; see Developer” |
| `/how-to/convert-formats/` | Offline **Format** on the login screen |
| `/how-to/extract-to-files/` | Offline **Extract**; JSONL (JSON Lines) on disk |
| `/how-to/export-from-the-vault/` | Desktop **Export** |
| `/how-to/media-and-privacy/` | Copy, convert, compress, obfuscate |
| `/how-to/rescue-imports/` | GO SMS Pro, iMazing, OpenExtract, SMS Backup+ (Limited badge) |
| `/how-to/update/` | Pull a new Docker image; keep the named volume; download a new desktop archive. Compose rebuild stays in Developer |
| `/how-to/troubleshooting/` | Cannot reach 8080, missing helpers, Gatekeeper/SmartScreen, login failures. Not schema or `config.toml` |
| `/glossary/` | JSONL, JSON, CSV, EML, MBOX, XML, VCF, E.164, Docker, SQLite. JSONL is defined here because the handbook uses it |

Merge today’s thin pages instead of keeping them:

- Navigation “this is not Next.js” is deleted. Sidebar labels are taught on Browse your messages and Search.
- Group chats are a short note on Browse your messages and `is:group` on Search. No standalone group page.
- “Supported output formats” as a separate desktop-app chapter is folded into Convert formats (what Format reads/writes) plus Developer format pages for field-level detail.
- Work with contacts as a desktop-only chapter is folded into Contacts and labels plus a short note on Import.

## Developer topic

Keep these URLs (layers overhaul and crate READMEs):

- `/formats/` and every current child
- `/reference/cli/` and every current command page
- `/reference/api/`
- `/reference/config-and-accounts/`
- `/reference/database/`
- `/reference/export-structure/`
- `/reference/csv-columns/`
- `/reference/server-cli/`

Add:

| Site path | Page |
|---|---|
| `/developer/run-from-source/` | Clone; Rust 1.85+; Node 22+; `cargo run --release -p message-vault-server -- serve`; `cargo tauri dev` after the one-time `web/` npm install. Link `CONTRIBUTING.md` for WSL, packages, tests, PR rules |
| `/developer/docker-compose/` | `compose-dev.yml`, `compose-release.yml`, bind-mounts, port 8081 SQLite browser. Not the User Guide try-it `docker run` |

Sidebar groups:

1. Run from source
2. Operator Docker
3. CLI tools (existing `/reference/cli/` pages)
4. HTTP API
5. Formats (existing `/formats/` tree)
6. Instance internals (`config.toml`, database, JSONL layout, CSV columns, server CLI)

CLI index and `/formats/` overview each get one sentence: these pages are Developer; the User Guide Import chapter does not require these commands.

`vault-push` / `vault-pull` link to `/how-to/extract-to-files/` and `/how-to/export-from-the-vault/`, not only to happy-path Import.

Today’s `docs/src/content/docs/set-up-the-server/docker-install.md` is split: the published-image `docker run` belongs on Try the vault; Compose belongs on `/developer/docker-compose/`. Delete the old path.

## Deleted User Guide paths (no redirects)

Remove after the new tree exists, including at least:

- `/introduction/*` (what-is, why-manual-backups, quick-start, install, glossary)
- `/set-up-the-server/*` (docker-install, first-personal-vault, try-the-demo, updating)
- `/use-the-desktop-app/*`
- `/browse/*`
- `/troubleshooting/` (content moves to `/how-to/troubleshooting/`)
- `/prepare-your-backups/*` (content rewritten under `/prepare-a-backup/`)

`/reference/*` and `/formats/*` are not deleted.

## GitHub front door (link retarget only)

Update URLs, not the README structure from the layers spec:

- Root `README.md`: install → `/get-started/install-the-desktop-app/`; try-it → `/get-started/try-the-vault/`; “Format Reference” wording → Developer formats at `/formats/`
- Crate READMEs that point at `/use-the-desktop-app/…` or `/set-up-the-server/try-the-demo/` (for example `message-media`, `message-obfuscate`, `demo-seed`)
- `docs/maintainers/gui.md` and other in-repo links to deleted User Guide files
- Developer pages that still link `/use-the-desktop-app/import-into-vault/` (`reference/cli/index.md`, `reference/database.md`)

## Screenshots

No new screenshot campaign. If an existing image shows the old demo-versus-empty-vault split or old chrome, delete it. Prose is checked against current screens: login, register, onboarding (handles), Import, Conversations, Settings tabs (Account, Profile, Storage, Appearance).

## Verification

```bash
cd docs && npm run check && npm run build
```

Manual:

- Header shows User Guide and Developer only.
- Walk the tutorial paths in the built sidebar without opening Developer.
- Skip link from Home and Try the vault lands on Use your own messages.
- Import page default is a backup source, not “point at JSONL.”
- `/formats/sms-backup-restore/mapping/` and `/reference/cli/vault-push/` still build.

Grep on User Guide Markdown (`get-started/`, `how-to/`, `prepare-a-backup/`, splash, glossary) must not instruct:

- `DEMO_DATA=false` as the way to start a personal vault
- “no account is required”
- port 3000
- Next.js as current UI
- `message-vault-rs` or `message-vault-io` as the current product name

Grep the repo for live links to deleted paths (`/introduction/quick-start/`, `/set-up-the-server/`, `/use-the-desktop-app/`, `/browse/`) and retarget them.

No new Cargo tests. This change is documentation.

## Delivery

One implementation pass on top of the layers-overhaul docs (Format pages and committed CLI pages already in the tree). One pull request is enough.

Suggested order of work (for the later plan, not a second spec):

1. Add Developer pages and reparent `starlight-sidebar-topics` (User Guide stub sidebar + Developer groups) so `/formats/` and `/reference/` keep building.
2. Write the tutorial pages; point Home at them.
3. Write the handbook; delete old User Guide files.
4. Retarget GitHub and in-repo links; run `npm run check` and `npm run build`.

## Relationship to earlier specs

| Topic | 2026-08-07 | 2026-08-12 layers | This spec |
|---|---|---|---|
| Voice | Defines it | Keeps it | Keeps it |
| User Guide IA | Introduction → Browse | Unchanged | Replaced |
| Demo vs personal | Separate Docker stories | Unchanged | Same instance, second account |
| Happy-path Import | Extract JSONL then push | Unchanged | Import from a backup; JSONL in handbook |
| Format / CLI on the site | Sidebar slots / later | Format topic + committed CLI | Same URLs; under Developer |
| Old User Guide URLs | Created them | Kept them | Deleted, no redirects |
