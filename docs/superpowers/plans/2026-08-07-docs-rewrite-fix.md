# Docs Rewrite: Fix Outdated References and Unify Voice

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite root-level README and CONTRIBUTING, fix the landing page index.mdx, update broken links and outdated references across all user-facing and maintainer docs, and ensure a unified voice for two audiences (users and developers).

**Architecture:** The message-vault repo is the unified project (was `message-vault-io` + `message-vault-rs`). User-facing docs live under `docs/src/content/docs/` and are published via the `bitrealm-dev.github.io` hub. Maintainer docs live under `docs/maintainers/`. This plan fixes all files — user-facing pages get plain-language rewrites; maintainer docs get corrected repo URLs, docs URLs, and paths while keeping technical precision.

**Tech Stack:** Markdown/MDX (Astro Starlight). No code changes — documentation only.

## Global Constraints

- Design spec: `docs/superpowers/specs/2026-08-07-docs-rewrite-design.md` — follow voice/naming rules, IA, and migration rules verbatim
- User-facing voice: plain language, no internal jargon ("message-ir" → "JSONL", "FormatSink" → never use, "exporter crate" → never use)
- Developer voice: technical precision OK; crate names like `message-vault-io-gui` are legitimate internal names
- All repo URLs: `github.com/bitrealm-dev/message-vault` (never `message-vault-io`, `message-vault-rs`, `message-vault-server`)
- All docs URLs: `bitrealm.dev/vault/` or `bitrealm-dev.github.io/vault/` (never `.../message-vault-io/`, `.../message-vault-rs/`, `.../exporters/`)
- Project name: "Message Vault" (two words, title case)
- Binary name: `message-vault` (the desktop app binary — was `message-vault-io`)
- Verify: `cd docs && npm run build` passes with no broken links after all changes
- No hits for: `message-vault-io` (except crate names in maintainer docs), `message-vault-rs`, `message-vault-server` (except historical), `message-ir` (in user-facing docs), `Next.js`, `FormatSink` (in user-facing docs), `exporter crate` (in user-facing docs)

---

### Task 1: Rewrite root README.md

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: Design spec voice/naming rules
- Produces: Unified README with correct project name, URLs, and architecture

- [ ] **Step 1: Replace entire README.md content**

The current README is titled `# message-vault-io` and references old repos, old URLs, old binary names throughout. Replace with the following content that reflects the unified project:

```markdown
# Message Vault

Extract messages from phone backups, import them into a local vault, and browse them in an interface you control.

Message Vault has two parts that work together:

- **The vault** — a Docker container running a REST API and SQLite database. It stores your messages and serves them through a web interface you open in your browser.
- **The desktop app** — a Tauri desktop application that extracts messages from Apple and Android phone backups, converts them between formats, and imports them into the vault.

Your messages stay on your own machine — nothing is uploaded to a cloud service, and no account is required.

## Docs

Read the full guide (install, desktop app, supported backups, formats, API):

**https://bitrealm.dev/vault/**

Source Markdown lives in [`docs/src/content/docs/`](docs/src/content/docs/) and is published from the [unified docs hub](https://github.com/bitrealm-dev/bitrealm-dev.github.io).

## Quick start

**Desktop app:** Download the platform archive from the latest [Release](https://github.com/bitrealm-dev/message-vault/releases). Extract it and keep every file in the same folder. Run `message-vault`.

**Vault server (Docker):**

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Open **http://localhost:3000** and sign in with username `demo` and an empty password.

**From source:**

```bash
cargo build --workspace --release
cargo run --release -p message-vault-io-gui
```

### WSL2 development

Use WSL2 with WSLg enabled and keep the repository in the Linux filesystem (`~/repo/...`), not under `/mnt/c`. From Windows PowerShell, update WSL before setting up the Linux environment:

```powershell
wsl --update
wsl --shutdown
```

Inside Ubuntu, install the compiler and GUI libraries:

```bash
sudo apt update
sudo apt install \
  build-essential pkg-config curl git libfontconfig1-dev \
  libxkbcommon-x11-0 libxkbcommon0 \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
```

Install Rust inside WSL rather than using a Windows Rust installation:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install [nvm](https://github.com/nvm-sh/nvm) and Node.js 24 inside WSL. This prevents WSL from invoking Windows `npm.cmd`, which fails when the current directory is a `\\wsl.localhost\...` path:

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.6/install.sh | bash
source ~/.bashrc
nvm install 24
nvm alias default 24
```

Confirm that Linux owns the active tools:

```bash
command -v cargo node npm
node --version
npm --version
```

The paths should be under `/home/...`, not `/mnt/c/...` or `C:\...`. Build in release mode for realistic export performance:

```bash
cargo run --release -p message-vault-io-gui
```

More Linux package details and optional helpers such as `ffmpeg` are documented in [CONTRIBUTING.md](CONTRIBUTING.md).

## Supported backups

| Backup | Converter |
|--------|-----------|
| Apple Messages (`chat.db`) | `imessage-ir-exporter` |
| SMS Backup & Restore (SyncTech XML) | `sms-backup-restore-exporter` |
| WhatsApp (native DB / crypt) | `whatsapp-exporter` |

Experimental converters also ship in the desktop app: GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+. Use those when they are the only backup on hand. Details: the [docs site](https://bitrealm.dev/vault/) and [exporter capability matrix](docs/maintainers/exporter-matrix.md).

Already exported? The desktop app **Format** tab converts a prior output folder to another format (CSV ↔ EML ↔ MBOX ↔ JSON ↔ JSONL ↔ XML).

Import into Message Vault with the desktop app **Vault** tab (JSONL export folder + Import API token). For standalone CLI tools (`vault-push`, `vault-pull`, exporter CLIs), build from source in this repo.

## Contributing

Setup, build, run, test, and contribution rules: [CONTRIBUTING.md](CONTRIBUTING.md).

## Releases

Prebuilt Linux (`.tgz`), Windows, and macOS Apple Silicon (`.zip`) archives — **GUI only** plus `lib/` (ffmpeg/ffprobe), `cli/wtsexporter`, and `licenses/`: [Releases](https://github.com/bitrealm-dev/message-vault/releases).

Maintainer documentation (architecture, GUI design, signing): [`docs/maintainers/`](docs/maintainers/README.md). Release steps: [Development and releases](docs/maintainers/developing.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database`).
```

- [ ] **Step 2: Verify no old references remain**

Run: `grep -n 'message-vault-io\|message-vault-rs\|bitrealm-dev.github.io/message-vault-io\|bitrealm-dev.github.io/message-vault-rs\|bitrealm-dev.github.io/exporters' README.md`
Expected: No output (except `message-vault-io-gui` crate name which is legitimate)

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README for unified message-vault project

Replace all message-vault-io references with message-vault. Update
docs URLs to bitrealm.dev/vault/. Add vault server quick-start.
Remove references to separate message-exporters repo.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Rewrite CONTRIBUTING.md

**Files:**
- Modify: `CONTRIBUTING.md`

**Interfaces:**
- Consumes: README.md (updated in Task 1) for consistent naming
- Produces: Contributor guide with correct repo URLs and unified project structure

- [ ] **Step 1: Replace entire CONTRIBUTING.md content**

The current file references `message-vault-io` throughout, has old clone URLs, references a separate `message-vault-server` repo, and points to old docs URLs. Replace with:

```markdown
# Contributing

How to set up, build, run, and contribute to Message Vault.

End-user guides (install, first export, formats) live on the [docs site](https://bitrealm.dev/vault/). Architecture, releases, signing, and GUI design notes live under [`docs/maintainers/`](docs/maintainers/README.md).

## Prerequisites

| Tool | Notes |
|------|--------|
| **Rust** | Stable toolchain via [rustup](https://rustup.rs/). This workspace uses Rust edition **2024**, which needs **Rust 1.85+**. CI builds with the latest stable. |
| **Windows** | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload (MSVC). |
| **macOS** | Xcode Command Line Tools (`xcode-select --install`). |
| **Linux** | C toolchain plus GUI system libs (see [Linux packages](#linux-packages) below). |
| **Node.js 22+** | For the docs site (`docs/`). |

Optional for full WhatsApp / media features while developing: Python (`pip`) for `wtsexporter`, and `ffmpeg` / `ffprobe` on `PATH` (or see [Helper binaries](#helper-binaries-and-environment-variables)).

### Linux packages

The Tauri desktop app needs a C toolchain and WebKit2GTK system libraries at **build time** and **runtime**. On Debian/Ubuntu:

```bash
sudo apt update
sudo apt install \
  build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev \
  libappindicator3-dev librsvg2-dev patchelf \
  libssl-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev
```

On Fedora:

```bash
sudo dnf install \
  gcc pkgconf-pkg-config \
  webkit2gtk4.1-devel gtk3-devel \
  libappindicator-gtk3-devel librsvg2-devel \
  openssl-devel javascriptcoregtk4.1-devel libsoup3-devel
```

## Clone and build

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
cargo build --workspace
```

The first build compiles every workspace crate and can take several minutes.

Release profile:

```bash
cargo build --workspace --release
```

Release packaging uses `cargo tauri build` which bundles the desktop app frontend, Rust backend, and all exporter libraries into a single platform installer. Exporters are linked as libraries. Standalone exporter CLIs can be built from this repo as well.

## Run the app

### One-time setup

```bash
# Install Tauri CLI
cargo install tauri-cli --version "^2"
```

### Dev mode (hot reload)

```bash
cargo tauri dev
```

This starts the Vite dev server on `localhost:5173` and opens a native window. Editing files under `web/src/` triggers instant reload; changes to Rust code under `src-tauri/` recompile and restart the backend.

### Release mode (no hot reload, faster exports)

```bash
cargo build --release --workspace
./target/release/message-vault
```

Use a release build when testing real exports. Debug builds compile faster, but parsing, attachment hashing, and JSON serialization can be substantially slower.

### WSL2

On WSL2, the Tauri window requires **WSLg** (Windows 11, built-in) or an X server like **VcXsrv** (Windows 10). Set `DISPLAY` if using a standalone X server:

```bash
export DISPLAY=$(cat /etc/resolv.conf | grep nameserver | awk '{print $2}'):0
cargo tauri dev
```

### Vault server

The vault server (`message-vault-server`) is built from this repo and runs in Docker:

```bash
docker compose up
```

The server's API is available at `http://localhost:8080` by default. The web interface is at `http://localhost:3000`. Create an account and API key through the web UI under **Settings → Access**.

Settings persist in `export.ini` (working directory or next to the binary). Template: [`crates/core/message-vault-io-core/export.example.ini`](crates/core/message-vault-io-core/export.example.ini). Backup passwords are never written.

## Helper binaries and environment variables

Most export work runs in-process as Rust libraries. A few features still shell out to sibling tools:

| Helper | Used for |
|--------|----------|
| `wtsexporter` | WhatsApp extract step |
| `ffmpeg` / `ffprobe` | Media convert / compress |

Lookup order: beside the current executable → `lib/` / `cli/` next to the GUI (or `../lib/` from `cli/`) → legacy one directory up → directory in `MESSAGE_VAULT_IO_BIN` → `PATH`. WhatsApp also accepts an explicit `WTSEXPORTER` path.

| Variable | Purpose |
|----------|---------|
| `MESSAGE_VAULT_IO_BIN` | Directory that contains helper binaries |
| `WTSEXPORTER` | Full path to the WhatsApp extractor |

Local options:

- Install WhatsApp helper: `pip install 'whatsapp-chat-exporter>=0.13'`
- Install system `ffmpeg` / `ffprobe`, or copy them from a [release archive](https://github.com/bitrealm-dev/message-vault/releases) next to your built GUI
- After `cargo build --workspace --release`, point helpers at the build output:

```powershell
# Windows PowerShell
$env:MESSAGE_VAULT_IO_BIN = "$PWD\target\release"
./target/release/message-vault.exe
```

```bash
# Linux / macOS
export MESSAGE_VAULT_IO_BIN="$PWD/target/release"
./target/release/message-vault
```

## Test

```bash
cargo test --workspace
```

Run a single crate:

```bash
cargo test -p go-sms-pro-exporter
```

Exporter smoke tests under `crates/*/tests/convert_smoke.rs` use committed fixtures. You do not need personal phone backups to run the suite.

## Docs site (optional)

User-facing docs are Astro Starlight under `docs/`:

```bash
cd docs
npm ci
npm run dev
```

Before publishing doc changes: `npm run check` and `npm run build`.

CLI reference pages are generated from crate manpages. Edit `crates/<name>/docs/MANPAGE.md`, then:

```bash
cd docs
npm run sync:cli
npm run check
npm run build
```

Do not edit generated files under `docs/src/content/docs/reference/cli/` by hand.

## Workspace map

- **Libraries:** under `crates/libs/` — `ir`, `contacts`, `media`, `mail`, `sbr`, `phone`, `csv`, `obfuscate`; plus `message-vault-io-core`
- **Exporter crates:** under `crates/exporters/` — `imessage-ir-exporter`, `whatsapp-exporter`, `sms-backup-restore-exporter`, and experimental converters (GO SMS Pro, iMazing, OpenExtract, SMS Backup+)
- **GUI:** Tauri v2 app in `src-tauri/` with React + Vite frontend in `web/`
- **Server:** `message-vault-server` crate — the vault REST API, SQLite database, and web UI
- **CLI tools:** `vault-push`, `vault-pull`, `message-reexport` (package `message-reexport`), and individual exporter CLIs — built from this repo

Most crates are MIT. `imessage-ir-exporter` is **GPL-3.0-or-later** (via `imessage-database`). The GUI binary therefore includes GPL-licensed code.

## Contribution rules

1. **Keep changes focused.** Prefer small PRs that do one job over mixed refactors and features.
2. **Match existing style.** Follow patterns already used in nearby crates; do not add drive-by renames or unrelated cleanup.
3. **Verify before you open a PR.** At minimum: `cargo build --workspace` and `cargo test --workspace`. If you touched docs under `docs/`, also run `npm run check` there.
4. **No secrets or personal data.** Do not commit passwords, vault keys, certificates, `.env` files with credentials, or real message backups. Use fixtures under `crates/*/tests/fixtures/` for test data.
5. **Respect licenses.** Call out GPL implications when changing `imessage-ir-exporter` or anything that pulls it into new binaries.
6. **Document CLI changes in the crate manpage** (`crates/<name>/docs/MANPAGE.md`), then sync the docs site as above.
7. **Put design depth in maintainer docs**, not in this file. Architecture, format contracts, GUI option matrices, releases, and signing stay under [`docs/maintainers/`](docs/maintainers/README.md).

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| `webkit2gtk` / `libsoup` not found | Install WebKit2GTK and GTK3 dev packages; see [Linux packages](#linux-packages) |
| "Could not find wtsexporter / ffmpeg / ffprobe" | Install the helper, put it on `PATH`, or set `MESSAGE_VAULT_IO_BIN` / `WTSEXPORTER` |
| Windows linker / `link.exe` errors | Install MSVC Build Tools with the C++ desktop workload |
| `cargo tauri` not found | Install with `cargo install tauri-cli --version "^2"` |
| Frontend not loading in dev mode | Run `cd web && npm ci` first, then `cargo tauri dev` |

## Further reading

- [Maintainer documentation index](docs/maintainers/README.md)
- [Development and releases](docs/maintainers/developing.md)
- [Exporter capability matrix](docs/maintainers/exporter-matrix.md)
- [Code signing](docs/maintainers/signing.md)
- End-user docs: <https://bitrealm.dev/vault/>
```

- [ ] **Step 2: Verify no old references remain**

Run: `grep -n 'message-vault-io.git\|message-vault-server.git\|bitrealm-dev.github.io/message-vault-io\|message-exporters/releases' CONTRIBUTING.md`
Expected: No output

- [ ] **Step 3: Commit**

```bash
git add CONTRIBUTING.md
git commit -m "docs: rewrite CONTRIBUTING for unified message-vault project

Replace all message-vault-io references. Remove separate vault-server
repo instructions (now built from this repo). Update docs URLs to
bitrealm.dev/vault/. Remove references to separate message-exporters
repo.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Rewrite index.mdx landing page

**Files:**
- Modify: `docs/src/content/docs/index.mdx`

**Interfaces:**
- Consumes: New IA from design spec (Introduction, Prepare your backups, Set up the server, Use the desktop app, Browse the vault, Reference)
- Produces: Landing page with correct links, no internal jargon, unified voice

- [ ] **Step 1: Replace entire index.mdx content**

The current index.mdx uses old IA paths (`/get-started/`, `/apple/`, `/android/`, `/work-with-exports/`), references "message-ir JSONL" and "Message Exporters" as a separate project. Replace with content that matches the new IA:

```mdx
---
title: Message Vault
description: Extract messages from phone backups, import them into a local vault, and browse them in a website you control.
template: splash
hero:
  title: Your messages, your way
  tagline: Extract messages from Apple and Android phone backups. Import them into a local SQLite vault. Browse, search, and export in formats you can keep.
  actions:
    - text: Quick start
      link: /introduction/quick-start/
      icon: right-arrow
      variant: primary
    - text: Try the demo vault
      link: /set-up-the-server/try-the-demo/
      icon: rocket
      variant: secondary
    - text: Install the desktop app
      link: /introduction/install/
      icon: download
---

import { Aside, Card, CardGrid } from '@astrojs/starlight/components';

## How it works

```text
1. Extract    Phone backup → JSONL + attachments
              (Message Vault desktop app)

2. Import     Push or CLI-import JSONL into the vault

3. Browse     Open the website on your computer — search, browse contacts, view media
```

Your messages stay in a SQLite database on a machine you control. They are not uploaded to a cloud service by this project.

## Choose what you want to do

<CardGrid>
  <Card title="Back up an iPhone or iPad">
    Use an iPhone backup or a Mac chat.db to export Messages, or use an iPhone backup for WhatsApp. [Apple backup guide](/prepare-your-backups/iphone-ipad/).
  </Card>
  <Card title="Back up an Android phone">
    Use SMS Backup & Restore XML for text messages, or provide a WhatsApp database or encrypted backup with its key. [Android backup guide](/prepare-your-backups/android-sms/).
  </Card>
  <Card title="Convert an existing export">
    Change a Message Vault folder from JSON, JSONL, CSV, EML, MBOX, or XML into another format. [Convert between formats](/use-the-desktop-app/convert-formats/).
  </Card>
  <Card title="Import into the vault">
    Upload a JSONL archive to the vault server, resume safely, or force reprocessing after a partial run. [Import into the vault](/use-the-desktop-app/import-into-vault/).
  </Card>
  <Card title="Browse the vault">
    Use contacts, labels, group messages, sources, trash, and settings. [Navigation and sources](/browse/navigation-and-sources/).
  </Card>
  <Card title="Install the desktop app">
    Download the release for Linux, Windows, or macOS and keep the helper programs with the app. [Install Message Vault](/introduction/install/).
  </Card>
  <Card title="Try sample data">
    Load the committed demo vault and click through contacts without a real backup. [Try the demo](/set-up-the-server/try-the-demo/).
  </Card>
  <Card title="Protect private information">
    Choose whether to copy media, convert it, compress it, or replace personal data before sharing. [Media and privacy options](/use-the-desktop-app/media-and-privacy/).
  </Card>
</CardGrid>

<Aside title="Supported paths and limited rescue imports">
  The supported paths are Apple Messages from chat.db or an iPhone backup, Android SMS/MMS from SMS Backup & Restore XML, and WhatsApp through wtsexporter. GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+ are rescue imports for files you already have. Their source formats can omit identity, group, attachment, reply, or reaction details, so those imports cannot always recreate the original messages.
</Aside>
```

- [ ] **Step 2: Verify no old references remain**

Run: `grep -n 'message-ir\|message-vault-io\|message-vault-rs\|/get-started/\|/apple/\|/android/\|/work-with-exports/\|Message Exporters' docs/src/content/docs/index.mdx`
Expected: No output

- [ ] **Step 3: Commit**

```bash
git add docs/src/content/docs/index.mdx
git commit -m "docs: rewrite index.mdx for new IA and unified voice

Replace old section paths with new IA structure. Remove 'message-ir'
jargon and 'Message Exporters' references. Use plain language for
user-facing landing page.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Fix broken links and jargon in user-facing reference docs

**Files:**
- Modify: `docs/src/content/docs/reference/api.md`
- Modify: `docs/src/content/docs/troubleshooting.md`

**Interfaces:**
- Consumes: `/reference/export-structure/` (exists) replaces broken `/reference/message-ir/` (does not exist)
- Produces: Correct internal links and user-appropriate language

- [ ] **Step 1: Fix api.md — replace "message-ir JSONL" and broken link**

Edit `docs/src/content/docs/reference/api.md`:

Change lines 6-10 from:
```
Prefer Message Exporters **`vault-push`** / the **`message-exporter`**
Vault tab for day-to-day use; this page documents the HTTP surface those tools
call. They send [message-ir JSONL](/reference/message-ir/) (and upload
attachments by SHA-256) to these endpoints.
```
To:
```
Prefer the desktop app **Vault tab** or **`vault-push`** CLI for
day-to-day use; this page documents the HTTP surface those tools
call. They send [JSONL](/reference/export-structure/) (and upload
attachments by SHA-256) to these endpoints.
```

- [ ] **Step 2: Fix troubleshooting.md — replace broken link**

Edit `docs/src/content/docs/troubleshooting.md`:

Change line 109 from:
```
**Fix**: the import API expects message-ir schema version 3. If the export was made with an older version of the desktop app, re-export it with the current version. See the [message-ir reference](/reference/message-ir/).
```
To:
```
**Fix**: the import API expects JSONL schema version 3. If the export was made with an older version of the desktop app, re-export it with the current version. See the [export structure reference](/reference/export-structure/).
```

- [ ] **Step 3: Verify fixes**

Run: `grep -n 'message-ir\|/reference/message-ir/' docs/src/content/docs/reference/api.md docs/src/content/docs/troubleshooting.md`
Expected: No output

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs/reference/api.md docs/src/content/docs/troubleshooting.md
git commit -m "docs: fix broken /reference/message-ir/ links in user-facing docs

Replace broken /reference/message-ir/ links with /reference/export-structure/.
Remove 'message-ir' jargon and 'Message Exporters' references from api.md.
Use 'JSONL' instead of 'message-ir JSONL' in user-facing context.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Fix maintainer docs — outdated URLs, paths, and repo references

**Files:**
- Modify: `docs/maintainers/README.md`
- Modify: `docs/maintainers/developing.md`
- Modify: `docs/maintainers/development.md`
- Modify: `docs/maintainers/roadmap.md`
- Modify: `docs/maintainers/gui.md`
- Modify: `docs/maintainers/signing.md`
- Modify: `docs/maintainers/exporter-matrix.md`
- Modify: `docs/maintainers/formats/mail-archive.md`

**Interfaces:**
- Consumes: Correct repo URL (`bitrealm-dev/message-vault`), docs URL (`bitrealm.dev/vault/`), new doc paths
- Produces: Maintainer docs with accurate references while keeping internal technical names

- [ ] **Step 1: Fix maintainers/README.md — one URL fix**

Edit `docs/maintainers/README.md` line 3:
From: `<https://bitrealm-dev.github.io/message-vault-io/>`
To: `<https://bitrealm.dev/vault/>`

- [ ] **Step 2: Fix maintainers/developing.md — old URLs, release filenames, docs references**

Apply these edits to `docs/maintainers/developing.md`:

Line 5 — fix old doc path:
From: `(start with [What's inside an export](../src/content/docs/understand-output/export-structure.md))`
To: `(start with [Export structure](../src/content/docs/reference/export-structure.md))`

Lines 17, 21, 70, 72, 75 — `message-vault-io-gui` crate references are legitimate internal names; keep them.

Lines 22, 26 — old repo URLs:
From: `https://github.com/bitrealm-dev/message-vault-io/actions/workflows/release.yml`
To: `https://github.com/bitrealm-dev/message-vault/actions/workflows/release.yml`

From: `https://github.com/bitrealm-dev/message-vault-io/releases`
To: `https://github.com/bitrealm-dev/message-vault/releases`

Lines 36-38 — archive filenames (keep `message-vault-io` in filenames if they reflect actual release artifact names):
These archive names reflect the current crate name `message-vault-io-gui`. Keep them for now — they're factual.

Line 44 — binary name:
From: `message-vault-io` (`.exe` on Windows)
To: `message-vault` (`.exe` on Windows)

Lines 58-60 — message-exporters reference (this is factually correct — standalone CLIs ship from that repo):
Keep this reference but update URL to current if needed.

Line 92 — docs URL:
From: `https://bitrealm-dev.github.io/message-vault-io/`
To: `https://bitrealm.dev/vault/`

- [ ] **Step 3: Fix maintainers/development.md — old repo references, Next.js mentions**

Apply these edits to `docs/maintainers/development.md`:

Line 4 — old docs URL:
From: `<https://bitrealm-dev.github.io/message-vault-rs/>`
To: `<https://bitrealm.dev/vault/>`

Line 11 — Next.js mention:
From: `- a Next.js application in `web/` for browsing the SQLite vault.`
To: `- a web interface served by the Rust server for browsing the SQLite vault.`

Line 61 — old repo name in path:
From: `Set-Location C:\path\to\message-vault-rs`
To: `Set-Location C:\path\to\message-vault`

Line 105 — Next.js mention:
From: `that is too old for Next.js 16.`
To: `that is too old for the current Node.js toolchain.`

Lines 169-170 — message-exporters reference and message-ir jargon:
From:
```
Keep the import API running while pushing a message-ir export from
[message-exporters](https://bitrealm-dev.github.io/message-exporters/)
```
To:
```
Keep the import API running while pushing a JSONL export from the
desktop app or vault-push CLI.
```

- [ ] **Step 4: Fix maintainers/roadmap.md — Next.js mentions, message-exporters references**

Apply these edits to `docs/maintainers/roadmap.md`:

Line 14 — message-exporters reference:
From: `[Message Exporters](https://bitrealm-dev.github.io/message-exporters/)`
To: `the desktop app`

Line 121 — Next.js mention:
From: `the Message Vault release container, with the Next.js UI on port 3000 and`
To: `the Message Vault release container, with the web UI on port 3000 and`

Line 128 — Next.js mention:
From: `import.example.com`. Bundle Hanko Elements into the Next.js application.`
To: `import.example.com`. Bundle Hanko Elements into the web application.`

- [ ] **Step 5: Fix maintainers/gui.md — old repo URLs**

Apply these edits to `docs/maintainers/gui.md`:

Line 88 — message-vault-rs reference:
From: `Matches message-vault-rs Fastmail-style seeds.`
To: `Matches the vault server Fastmail-style search seeds.`

Line 415 — old repo URL:
From: `[imessage-ir-exporter](https://github.com/bitrealm-dev/message-vault-io/tree/main/crates/exporters/imessage-ir-exporter)`
To: `[imessage-ir-exporter](https://github.com/bitrealm-dev/message-vault/tree/main/crates/exporters/imessage-ir-exporter)`

Search for and fix any remaining `message-vault-io` URLs in the file that reference the old repo location (not crate names).

- [ ] **Step 6: Fix maintainers/signing.md — old binary names**

Apply these edits to `docs/maintainers/signing.md`:

Line 61 — binary name:
From: `submit a zip of `message-vault-io` to `notarytool --wait``
To: `submit a zip of `message-vault` to `notarytool --wait``

Line 71 — binary names:
From: `macOS: `codesign -dv --verbose=4 ./message-vault-io` and `spctl -a -vv ./message-vault-io``
To: `macOS: `codesign -dv --verbose=4 ./message-vault` and `spctl -a -vv ./message-vault``

- [ ] **Step 7: Fix maintainers/exporter-matrix.md — old doc paths**

Apply these edits to `docs/maintainers/exporter-matrix.md`:

Line 72 — old doc paths:
From: `end-user [export structure](../src/content/docs/understand-output/export-structure.md); schema [message-ir architecture](architecture/message-ir.md).`
To: `end-user [export structure](../src/content/docs/reference/export-structure.md); schema [message-ir architecture](architecture/message-ir.md).`

Also fix any other `understand-output/` paths in the file to point to `reference/`.

- [ ] **Step 8: Fix maintainers/formats/mail-archive.md — old doc paths and domain reference**

Apply these edits to `docs/maintainers/formats/mail-archive.md`:

Line 5 — old doc paths:
From: `[shared conversation structure](../../src/content/docs/understand-output/export-structure.md)`
To: `[shared conversation structure](../../src/content/docs/reference/export-structure.md)`

Also fix: `../../src/content/docs/understand-output/csv-columns.md` → `../../src/content/docs/reference/csv-columns.md`

Line 94 — old domain:
From: `@message-vault-io.local`
To: `@message-vault.local`

- [ ] **Step 9: Verify all maintainer doc fixes**

Run:
```bash
grep -rn 'bitrealm-dev.github.io/message-vault-io\|bitrealm-dev.github.io/message-vault-rs\|/understand-output/\|Next\.\?[Jj]s' docs/maintainers/
```
Expected: No output (except legitimate `message-vault-io-gui` crate references which should be reviewed case-by-case)

- [ ] **Step 10: Commit**

```bash
git add docs/maintainers/
git commit -m "docs: fix outdated URLs and references in maintainer docs

Update docs URLs to bitrealm.dev/vault/. Fix old repo URLs to
bitrealm-dev/message-vault. Replace Next.js references with
'web UI' or 'web interface'. Fix old understand-output/ doc paths
to reference/. Update binary names in signing docs.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 7: Fix broken links in remaining user-facing docs

**Files:**
- Modify: `docs/src/content/docs/browse/navigation-and-sources.md`
- Modify: `docs/src/content/docs/browse/settings.md`
- Modify: `docs/src/content/docs/reference/database.md`
- Modify: `docs/src/content/docs/reference/cli/index.md`

**Interfaces:**
- Consumes: New IA path structure
- Produces: All internal links in user-facing docs resolve correctly

- [ ] **Step 1: Fix browse/navigation-and-sources.md — broken `/import/modes-and-dedupe/` link**

Find the reference to `/import/modes-and-dedupe/` and replace with `/use-the-desktop-app/import-into-vault/` (the closest equivalent page that covers import modes and deduplication).

- [ ] **Step 2: Fix browse/settings.md — stale `/get-started/try-the-demo/` link**

Replace `/get-started/try-the-demo/` with `/set-up-the-server/try-the-demo/`.

- [ ] **Step 3: Fix reference/database.md — broken `/import/modes-and-dedupe/` link**

Replace `/import/modes-and-dedupe/` with `/use-the-desktop-app/import-into-vault/`.

- [ ] **Step 4: Fix reference/cli/index.md — stale `/work-with-exports/import-to-vault/` link**

Replace `/work-with-exports/import-to-vault/` with `/use-the-desktop-app/import-into-vault/`.

- [ ] **Step 5: Verify**

Run: `grep -rn '/get-started/\|/apple/\|/android/\|/work-with-exports/\|/import/modes-and-dedupe/' docs/src/content/docs/`
Expected: No output

- [ ] **Step 6: Commit**

```bash
git add docs/src/content/docs/browse/navigation-and-sources.md \
        docs/src/content/docs/browse/settings.md \
        docs/src/content/docs/reference/database.md \
        docs/src/content/docs/reference/cli/index.md
git commit -m "docs: fix broken internal links in user-facing docs

Fix stale paths in browse/ and reference/ pages that referenced
the old route structure (get-started, work-with-exports, import/).
Point to current paths under introduction/, set-up-the-server/,
and use-the-desktop-app/.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: Fix additional broken links in maintainer docs

**Files:**
- Modify: `docs/maintainers/signing.md`
- Modify: `docs/maintainers/architecture/message-ir.md`
- Modify: `docs/maintainers/formats/mail-archive.md`
- Modify: `docs/maintainers/formats/sms-backup-restore-xml.md`
- Modify: `docs/maintainers/gui.md` (broken internal links only — full rewrite of Slint→Tauri content is out of scope)

**Interfaces:**
- Consumes: New doc paths
- Produces: All internal links in maintainer docs resolve correctly

- [ ] **Step 1: Fix maintainers/signing.md — stale `get-started/install.mdx` link**

Replace `get-started/install.mdx` with `introduction/install.md`.

- [ ] **Step 2: Fix maintainers/architecture/message-ir.md — broken `understand-output/` links**

Replace `../../src/content/docs/understand-output/export-structure.md` with `../../src/content/docs/reference/export-structure.md`.
Replace `../../src/content/docs/understand-output/csv-columns.md` with `../../src/content/docs/reference/csv-columns.md`.

- [ ] **Step 3: Fix maintainers/formats/mail-archive.md — broken doc paths and old domain**

Replace `../../src/content/docs/understand-output/export-structure.md` with `../../src/content/docs/reference/export-structure.md`.
Replace `../../src/content/docs/understand-output/csv-columns.md` with `../../src/content/docs/reference/csv-columns.md`.
Replace `@message-vault-io.local` with `@message-vault.local`.

- [ ] **Step 4: Fix maintainers/formats/sms-backup-restore-xml.md — broken `understand-output/` link**

Replace `understand-output/export-structure.md` references with `reference/export-structure.md`.

- [ ] **Step 5: Fix maintainers/gui.md — broken internal links (Slint→Tauri full rewrite out of scope)**

Note: `gui.md` describes the old Slint GUI framework, which has been replaced by Tauri. A full rewrite is out of scope for this plan, but fix the broken links:

Replace `work-with-exports/import-to-vault.mdx` with `use-the-desktop-app/import-into-vault.md`.
Replace `get-started/first-export.mdx` with `use-the-desktop-app/extract-messages.md`.
Replace `understand-output/export-structure.md` with `reference/export-structure.md`.

- [ ] **Step 6: Verify**

Run: `grep -rn '/understand-output/\|/get-started/\|/work-with-exports/' docs/maintainers/`
Expected: No output

- [ ] **Step 7: Commit**

```bash
git add docs/maintainers/signing.md \
        docs/maintainers/architecture/message-ir.md \
        docs/maintainers/formats/mail-archive.md \
        docs/maintainers/formats/sms-backup-restore-xml.md \
        docs/maintainers/gui.md
git commit -m "docs: fix broken internal links in maintainer docs

Fix stale understand-output/ paths to reference/. Fix old route
paths (get-started, work-with-exports) in signing.md and gui.md.
Fix message-vault-io.local domain reference in mail-archive.md.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 9: Handle obsolete development.md

**Files:**
- Modify: `docs/maintainers/development.md`

**Interfaces:**
- Consumes: None (this file is pre-merge, Next.js-era)
- Produces: Decision — either delete or add deprecation notice

**Context:** `docs/maintainers/development.md` describes the pre-merge layout (two repos, Next.js web UI, port 3000). It is largely obsolete. The current contributor setup is covered by `CONTRIBUTING.md` (rewritten in Task 2).

- [ ] **Step 1: Assess whether to delete or deprecate**

The file covers:
- Pre-merge `message-vault-rs` layout (Next.js, separate repos)
- Fastmail-style search query operators (still relevant but covered in `browse/search.md`)
- Rust+Node.js setup (covered in `CONTRIBUTING.md`)

Since `CONTRIBUTING.md` covers current setup and `browse/search.md` covers search operators, this file is obsolete.

- [ ] **Step 2: Delete the file**

```bash
git rm docs/maintainers/development.md
```

- [ ] **Step 3: Remove any references to development.md from other maintainer docs**

Check if any other files link to `development.md`:
```bash
grep -rn 'development\.md' docs/maintainers/
```
If hits are found, update those links.

- [ ] **Step 4: Commit**

```bash
git rm docs/maintainers/development.md
git commit -m "docs: remove obsolete maintainers/development.md

This file described the pre-merge message-vault-rs layout with
Next.js web UI. Current contributor setup is in CONTRIBUTING.md.
Search operator docs are in browse/search.md.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 10: Fix outdated references in crate-level docs (MANPAGE, README, DESIGN, IMPORT_MAPPING)

**Files:**
- Modify: `crates/cli/vault-push/README.md`
- Modify: `crates/cli/vault-push/docs/MANPAGE.md`
- Modify: `crates/cli/vault-pull/README.md`
- Modify: `crates/libs/contacts/README.md`
- Modify: `demo/README.md`
- Modify: `crates/exporters/go-sms-pro-exporter/docs/IMPORT_MAPPING.md`
- Modify: `crates/exporters/imazing-exporter/docs/DESIGN.md`
- Modify: `crates/exporters/sms-backup-plus-exporter/docs/IMPORT_MAPPING.md`
- Modify: `crates/exporters/sms-backup-restore-exporter/docs/IMPORT_MAPPING.md`

**Interfaces:**
- Consumes: Correct repo URL, correct crate paths (`crates/libs/` not `crates/message/`), new doc paths
- Produces: Crate-level docs with accurate references; MANPAGE.md fixes propagate to generated CLI reference pages

**Context:** MANPAGE.md files feed into the user-facing CLI reference via `npm run sync:cli`. Changes here affect published docs. Internal jargon like "message-ir" is acceptable in crate-level technical docs where it refers to the actual schema/crate name, but user-visible MANPAGE text should use "JSONL" for the format.

- [ ] **Step 1: Fix crates/cli/vault-push/README.md — old repo URL**

Line 3 — old repo reference:
From: `Push a Message Vault **JSONL** export folder into [Message Vault](https://github.com/bitrealm-dev/message-vault-rs).`
To: `Push a Message Vault **JSONL** export folder into a running Message Vault server.`

- [ ] **Step 2: Fix crates/cli/vault-push/docs/MANPAGE.md — Next.js reference**

Line 23 — obsolete Next.js caveat:
From: `| `--url` | `VAULT_URL` | Base URL of `message-vault-server serve` (e.g. `http://127.0.0.1:8080` or `https://app.bitrealm.dev`), **not** the Next.js UI on `:3000` |`
To: `| `--url` | `VAULT_URL` | Base URL of `message-vault-server serve` (e.g. `http://127.0.0.1:8080` or `https://app.bitrealm.dev`), **not** the web UI on `:3000` |`

Line 15 — "message-ir schema v3": this is a technical schema reference in a MANPAGE, acceptable to keep as-is.

- [ ] **Step 3: Fix crates/cli/vault-pull/README.md — message-ir jargon in intro**

Line 4:
From: `**message-ir** export folder`
To: `**JSONL** export folder`

- [ ] **Step 4: Fix crates/libs/contacts/README.md — old repo name**

Line 10:
From: `Name resolution belongs in **message-vault-io**`
To: `Name resolution belongs in **the desktop app**`

- [ ] **Step 5: Fix demo/README.md — message-ir jargon**

Line 3:
From: `Committed message-ir JSONL bundle`
To: `Committed JSONL bundle`

- [ ] **Step 6: Fix crate IMPORT_MAPPING.md and DESIGN.md files — stale paths**

These files reference `crates/message/ir/`, `crates/message/ir-format/`, `crates/message/mail/` paths that are now `crates/libs/ir/`, `crates/libs/ir-format/`, `crates/libs/mail/`. Also fix `understand-output/` → `reference/` doc paths.

In `crates/exporters/go-sms-pro-exporter/docs/IMPORT_MAPPING.md`:
- `../../../message/ir-format/src/write.rs` → `../../../libs/ir-format/src/write.rs`
- `../../../../docs/src/content/docs/understand-output/csv-columns.md` → `../../../../docs/src/content/docs/reference/csv-columns.md`
- `../../../message/ir-format/src/format_sink.rs` → `../../../libs/ir-format/src/format_sink.rs`

In `crates/exporters/imazing-exporter/docs/DESIGN.md`:
- `../../../message/ir-format/src/write.rs` → `../../../libs/ir-format/src/write.rs`
- `../../../message/ir-format/src/format_sink.rs` → `../../../libs/ir-format/src/format_sink.rs`

In `crates/exporters/sms-backup-plus-exporter/docs/IMPORT_MAPPING.md`:
- `../../../message/ir-format/src/write.rs` → `../../../libs/ir-format/src/write.rs`
- `../../../../docs/src/content/docs/understand-output/csv-columns.md` → `../../../../docs/src/content/docs/reference/csv-columns.md`
- `../../../message/ir-format/src/format_sink.rs` → `../../../libs/ir-format/src/format_sink.rs`

In `crates/exporters/sms-backup-restore-exporter/docs/IMPORT_MAPPING.md`:
- `../../../../docs/src/content/docs/understand-output/csv-columns.md` → `../../../../docs/src/content/docs/reference/csv-columns.md`

- [ ] **Step 7: Verify**

Run:
```bash
grep -rn 'crates/message/ir\|crates/message/ir-format\|crates/message/mail' crates/exporters/*/docs/ crates/libs/*/README.md 2>/dev/null
```
Expected: No output

Run:
```bash
grep -rn 'understand-output/' crates/ 2>/dev/null
```
Expected: No output

- [ ] **Step 8: Commit**

```bash
git add crates/cli/vault-push/README.md \
        crates/cli/vault-push/docs/MANPAGE.md \
        crates/cli/vault-pull/README.md \
        crates/libs/contacts/README.md \
        demo/README.md \
        crates/exporters/go-sms-pro-exporter/docs/IMPORT_MAPPING.md \
        crates/exporters/imazing-exporter/docs/DESIGN.md \
        crates/exporters/sms-backup-plus-exporter/docs/IMPORT_MAPPING.md \
        crates/exporters/sms-backup-restore-exporter/docs/IMPORT_MAPPING.md
git commit -m "docs: fix outdated references in crate-level docs

Fix old repo URLs, Next.js references, and message-ir jargon in
crate READMEs and MANPAGE files. Fix stale crates/message/ paths
to crates/libs/. Fix understand-output/ doc paths to reference/.
MANPAGE fixes propagate to generated CLI reference pages.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 11: Final verification — build, link check, and grep sweep

**Files:**
- No files created or modified (verification only)

**Interfaces:**
- Consumes: All changes from Tasks 1-10
- Produces: Confirmation that docs build passes, no broken links, no remaining outdated references

- [ ] **Step 1: Build the docs site**

Run:
```bash
cd ~/repo/message-vault/docs && npm run build
```
Expected: Build passes with no errors

- [ ] **Step 2: Comprehensive grep for ALL banned terms**

Run:
```bash
cd ~/repo/message-vault
grep -rn 'message-vault-io' docs/src/content/docs/ README.md CONTRIBUTING.md 2>/dev/null | grep -v 'message-vault-io-gui\|message-vault-io-core\|MESSAGE_VAULT_IO_BIN'
```
Expected: No output (crate names `message-vault-io-gui`, `message-vault-io-core`, and env var `MESSAGE_VAULT_IO_BIN` are legitimate)

```bash
grep -rn 'message-vault-rs' docs/src/content/docs/ README.md CONTRIBUTING.md docs/maintainers/README.md docs/maintainers/developing.md 2>/dev/null
```
Expected: No output

```bash
grep -rn 'bitrealm-dev.github.io/message-vault-io\|bitrealm-dev.github.io/message-vault-rs\|bitrealm-dev.github.io/exporters' docs/src/content/docs/ README.md CONTRIBUTING.md docs/maintainers/ 2>/dev/null
```
Expected: No output

```bash
grep -rn 'message-ir' docs/src/content/docs/ 2>/dev/null | grep -v 'reference/cli/' | grep -v 'reference/export-structure'
```
Expected: No output (message-ir is OK only in CLI reference and export-structure page)

```bash
grep -rn 'Next\.\?[Jj]s' docs/src/content/docs/ README.md CONTRIBUTING.md docs/maintainers/ 2>/dev/null
```
Expected: No output

```bash
grep -rn '/understand-output/' docs/ crates/ 2>/dev/null
```
Expected: No output

```bash
grep -rn '/get-started/\|/apple/\|/android/\|/work-with-exports/' docs/src/content/docs/index.mdx 2>/dev/null
```
Expected: No output

- [ ] **Step 3: Check for broken internal links**

Run:
```bash
cd ~/repo/message-vault/docs && npm run check
```
Expected: Astro check passes with no broken link warnings

- [ ] **Step 4: Manual review checklist**

Read through these files and confirm voice consistency:
- `README.md` — reads as project overview for both audiences
- `CONTRIBUTING.md` — reads as developer setup guide
- `docs/src/content/docs/index.mdx` — reads as user-facing landing page, no jargon
- `docs/src/content/docs/introduction/what-is-message-vault.md` — clean user voice
- `docs/src/content/docs/reference/api.md` — precise but jargon-free
- `docs/maintainers/README.md` — accurate index with correct URLs

- [ ] **Step 5: Commit verification results (if any fixups needed) or final sign-off**

If any issues found, fix and commit. Otherwise, the verification is complete.
