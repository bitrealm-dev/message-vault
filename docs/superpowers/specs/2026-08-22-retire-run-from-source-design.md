# Retire Run from source

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

Contributing (`docs/src/content/docs/vault/developer/contributing.md`) already covers first-time setup and **Build and Run**: Ubuntu packages, Rust, Node, clone, `./scripts/run-vault-dev.sh --reset-demo`, Vite, and `cargo tauri dev`.

A second page, **Run from source** (`docs/src/content/docs/vault/developer/run-from-source.md`), repeats that same path in shorter form. It is live at `https://bitrealm.io/vault/developer/run-from-source/`. The older address `https://bitrealm.io/developer/run-from-source/` already 404s. Guidebook files live only under `docs/src/content/docs/vault/`. There is no second copy of this page on disk.

A few commands still appear only on Run from source:

- `./scripts/run-vault-dev.sh` with no flags (keep `data/` if it exists)
- `--reset` (wipe `data/`, start empty, no sample inbox)
- `./scripts/build-static.sh` (copy `web/dist` into `static/` so the vault serves the website on port 8080)
- `cargo tauri build` (release-shaped desktop binary)

`--reset` and `--reset-demo` cannot be combined. That is already how `scripts/run-vault-dev.sh` behaves.

## Goal

One published place tells a contributor how to compile and run from a git checkout: Contributing **Build and Run**. The Run from source page is gone. Links that used to point there point at Contributing. `cargo tauri build` is documented as an alternate to `cargo tauri dev`, not as the day-to-day command.

## Non-goals

- An HTTP redirect from `/vault/developer/run-from-source/` (that URL 404s after publish)
- A stub page that only links to Contributing
- A WSL section on Contributing
- Repeating the Ubuntu package list
- Naming the `message-vault-io-tauri` Cargo package
- Editing `AGENTS.md`, `README.md`, the company home page (`docs/src/pages/index.astro`), or FAQ prose
- Editing files under `docs/superpowers/specs/` except this spec
- Cleaning unpublished leftover guidebook trees (none exist on disk)
- Runtime, exporter, desktop app, or vault-server product code

## Decisions

1. **Delete the page.** Remove `docs/src/content/docs/vault/developer/run-from-source.md`. Remove `vault/developer/run-from-source` from the Developer sidebar in `docs/astro.config.mjs`. Sidebar order stays: Developer index, Contributing, Architecture, Operator Docker, then the rest.
2. **No redirect.** Matches earlier guidebook path moves: in-repo links are rewritten; retired URLs 404.
3. **Fold leftover commands into Contributing Build and Run.** First-run stays `--reset-demo`, Vite, and `cargo tauri dev`. Add the no-flag vault command, `--reset`, `build-static.sh`, and `cargo tauri build` in that section. Do not add a new Contributing heading above **Making Code Changes**.
4. **`cargo tauri build` is an alternate.** Day-to-day UI work stays `cargo tauri dev` (hot reload). `cargo tauri build` is for a release-shaped binary: faster on real backups, and the command to use when packaging installers. It does not reload.
5. **Retarget live links** to `/vault/developer/contributing/#build-and-run`, except the glossary (Developer index). See [What changes](#what-changes).

## What changes

| Path | Change |
|------|--------|
| `docs/src/content/docs/vault/developer/contributing.md` | Add vault flags, optional `build-static.sh`, and `cargo tauri build` under **Build and Run** (copy below) |
| `docs/src/content/docs/vault/developer/run-from-source.md` | Delete |
| `docs/astro.config.mjs` | Drop `vault/developer/run-from-source` from `developerItems` |
| `docs/src/content/docs/vault/developer/index.md` | Drop the Run from source bullet. Change the page description so it no longer says “Run Message Vault from source, then…”. Contributing line stays about environment setup, tests, and pull requests |
| `docs/src/content/docs/vault/developer/docker-compose.md` | Three Run from source mentions → Contributing **Build and Run**. The Related list item becomes Contributing, not a deleted page |
| `docs/src/content/docs/vault/user/get-started/try-the-vault.md` | “Build from source instead”: compile → Contributing. Operator Docker stays for a checkout image |
| `docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md` | “Build from source”: one link to Contributing. Drop the extra sentence that Linux packages and WSL notes are on Contributing |
| `docs/src/content/docs/vault/user/glossary.md` | “Command flags and vendor field names” → [Developer](/vault/developer/), not a compile page |

## Intended copy

Keep the existing first-run blocks (Terminal 1 `--reset-demo`, `--sqlweb`, Terminal 2 Vite, demo login). Keep **Desktop app** opening with `cargo tauri dev`. Insert the new material as follows. Voice: short sentences, concrete commands, no “we” / “us” / “our”. Starlight `title=""` on code fences. Do not use GitHub `> [!TIP]` alerts.

**After** the later-sessions sentence (“Later sessions, skip `npm ci`…”), add vault flags and optional static build. The later-sessions sentence can stay; the no-flag command is the concrete form of “skip `--reset-demo`”.

### Vault flags

First run uses `--reset-demo`. Later sessions, start without flags so `data/` stays:

```bash title="Start the vault, keep data"
./scripts/run-vault-dev.sh
```

`--reset` wipes `data/` and starts empty (no sample inbox). Do not combine `--reset` and `--reset-demo`. `--sqlweb` still works with any of these.

### Serve the website from the vault (optional)

Vite is the usual UI. To have the vault itself serve the website at **http://127.0.0.1:8080**:

```bash title="Build the website into static/"
./scripts/build-static.sh
```

That copies `web/dist` into `static/`. Do not run the host vault and `docker compose -f docker/compose.release.yml` at the same time; both use port 8080.

**After** the `cargo tauri dev` block and the “point it at http://127.0.0.1:8080” sentence, before **Stopping and restarting**:

For a release-shaped desktop binary (faster on real backups, or when packaging installers):

```bash title="Build a release-shaped desktop app"
cargo tauri build
```

Do not use `cargo tauri build` for day-to-day UI work. `cargo tauri dev` reloads. The build command does not.

**Developer index description** (frontmatter): replace the “Run Message Vault from source, then…” sentence with one that starts from environment setup (Contributing), then architecture, CLI, API, formats, and internals. Do not mention a Run from source page.

**Operator Docker** intro: day-to-day host run uses `./scripts/run-vault-dev.sh` — see [Contributing](/vault/developer/contributing/#build-and-run). The rest of that paragraph (this page is Docker) stays. The in-body sentence “For local development without Docker…” and the Related list item use the same Contributing URL.

**Try the vault** “Build from source instead”: Compiling the vault and the desktop app: [Contributing](/vault/developer/contributing/#build-and-run). A release-shaped image from a git checkout: [Operator Docker](/vault/developer/docker-compose/).

**Install the desktop app** “Build from source”: Compiling the app and the vault from a git checkout: [Contributing](/vault/developer/contributing/#build-and-run).

**Glossary** intro: Short definitions of terms used in the User Guide. Command flags and vendor field names live under [Developer](/vault/developer/).

## Voice

Match Contributing and the User Guide: short sentences, concrete commands, no “we” / “us” / “our”. Starlight asides are optional. Do not use GitHub `> [!TIP]` alerts.

## Verification

- Contributing **Build and Run** documents: later sessions with no flags, `--reset`, `./scripts/build-static.sh`, and `cargo tauri build` as an alternate to `cargo tauri dev`
- `run-from-source.md` is gone; sidebar has no `vault/developer/run-from-source`
- Developer index has no Run from source bullet
- Operator Docker, Try the vault, and Install the desktop app link to Contributing `#build-and-run`
- Glossary links to `/vault/developer/`
- Grep of live files (exclude `docs/superpowers/` and `docs/dist/`) finds no `/vault/developer/run-from-source/`
- Built site has no `dist/vault/developer/run-from-source/`
- `cd docs && npm run check && npm run build` succeeds
- `AGENTS.md`, `README.md`, `docs/src/pages/index.astro`, and FAQ are unchanged

## Success criteria

- A first-time contributor compiles and runs from Contributing alone
- A reader who wants a release-shaped desktop binary finds `cargo tauri build` next to `cargo tauri dev`, marked as not the day-to-day command
- Bookmarks to `/vault/developer/run-from-source/` 404 after publish
- User Guide “Build from source” remains a secondary path (not the default install), pointing at Contributing
