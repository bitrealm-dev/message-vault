# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repo identity

This repository is named **message-vault**. It was formed by merging the former `message-vault-io` (desktop app + exporter libraries) and `message-vault-rs` (vault server) repos. Many Rust package names still use `message-vault-io` for historical reasons — that is the package namespace, not the repo name. The public docs site and GitHub org remain under `bitrealm-dev`.

## Docs site

The public docs site is the Astro Starlight project under `docs/`, live at **https://bitrealm.dev/**. Edit content in `docs/src/content/docs/`. GitHub Pages deploys from this repo via `.github/workflows/docs.yml` on `workflow_dispatch` or when `docs/**` (or that workflow) changes on `main`.

```bash
cd docs && npm ci && npm run dev   # local preview
cd docs && npm run check && npm run build
```

**Domain cutover (one-time, after first merge):** In this repo’s GitHub Settings → Pages, set source to GitHub Actions and custom domain `bitrealm.dev`. Remove the custom domain from `bitrealm-dev/bitrealm-dev.github.io`. Keep Cloudflare apex A records pointing at GitHub Pages (`185.199.108–111.153`); add any TXT verification record GitHub shows. Leave `api` / `app` / R2 alone. Then run the Docs workflow once.

## Quick start

```bash
# One-time setup
cargo install tauri-cli --version "^2"
cd web && npm ci && cd ..

# Run the Tauri desktop app in dev mode (hot reload)
cargo tauri dev
```

## Build and test

```bash
# Build everything (first build takes several minutes)
cargo build --workspace

# Release build (substantially faster for real export work)
cargo build --workspace --release

# Build / lint / test the Vite SPA under web/ (not web-next/)
cd web && npm ci          # first time or after lockfile changes
cd web && npm run lint    # ESLint (errors fail; warnings do not)
cd web && npm test        # Vitest unit tests
cd web && npm run build   # tsc typecheck + Vite production build

# Fix frontend check failures: open the file:line from the tool output, prefer
# a real fix, use a leading `_` for intentionally unused bindings, and avoid
# broad eslint-disable. Config: web/eslint.config.js. Same lint/test commands
# run on every push/PR to main (CI jobs Lint (web) and Test (web)).

# Run all tests (uses committed fixtures, no personal backups needed)
cargo test --workspace

# Run a single crate's tests
cargo test -p sms-backup-restore-exporter

# Build and check docs site
cd docs && npm ci && npm run check && npm run build
cd docs && npm run dev   # local preview
```

**Requirements**: Rust 1.85+ (edition 2024), Node.js 22+ (for the web frontend and docs). Linux needs WebKit2GTK and GTK3 system libraries (see `CONTRIBUTING.md`). `ffmpeg`/`ffprobe` on PATH for media convert/compress features. WSL2 needs WSLg (Windows 11) or an X server like VcXsrv (Windows 10).

## Architecture

This is a Rust workspace that converts phone message backups into a shared conversation structure, then packages each conversation in the user's chosen output format. It also contains the **unified GUI** (Tauri v2 desktop app + Vite SPA) and the **vault server** (HTTP API + SQLite storage for browsing imported messages).

**Pipeline**: `vendor backup → parse → ConversationDocument (schema v3) → FormatSink → output format`

### Layer model

1. **`crates/libs/ir/`** (`message-ir`) — Schema types only: `ConversationDocument`, `IrMessage`, `IrAttachment`, `IrParticipant` (with `HandleType`), enums. No I/O, no formatting. Attachment bytes are never serialized to JSON (`#[serde(skip)]`); paths and digests point at sidecar files.

2. **`crates/libs/ir-format/`** (`message-ir-format`) — `FormatSink` that takes parsed conversations and writes the chosen output format (JSON, JSONL, CSV, EML, MBOX, or a single SyncTech `smses.xml`). Runs media transforms (copy/convert/compress) and obfuscation during `FormatSink::finish`. Readers exist for every format to enable round-trip conversion.

3. **`crates/libs/reexport/`** (`message-reexport`) — Directory converter (GUI **Format** tab). Auto-detects input format in an export folder, reads all conversations, writes them in a target format via `FormatSink`.

4. **Exporter crates** under `crates/exporters/` — Each parses one backup source into `ConversationDocument` and feeds it to `FormatSink`. The GUI links them as libraries (`default-features = false` in GUI Cargo.toml, which drops the `cli` feature). Each crate has a `cli` feature (default on) that gates the standalone binary behind `dep:clap`. Three tiers:
   - **Primary**: iMessage (`imessage-ir-exporter`), WhatsApp (`whatsapp-exporter`, shells out to `wtsexporter`), SMS Backup & Restore (`sms-backup-restore-exporter`)
   - **Experimental**: GO SMS Pro, iMazing, OpenExtract, SMS Backup+
   - See https://bitrealm.dev/formats/ for per-converter capability gaps

5. **`crates/core/message-vault-io-core/`** — Shared form model (`ExporterConfig`, `Exporter` enum, `Form` trait for GUI validation), job spawning (`spawn_job` with `CancelFlag` + `mpsc::Sender<ProcessEvent>`), and ini persistence (`ExportIniState`).

6. **`src-tauri/`** and **`web/`** — Tauri v2 desktop app + Vite SPA (the **unified GUI**). `src-tauri/` is excluded from the workspace (`exclude = ["src-tauri"]` in root `Cargo.toml`) and built via `cargo tauri`. Architecture:
   - `src-tauri/src/main.rs` — Tauri entry point, registers commands and plugins
   - `src-tauri/src/state.rs` — `AppState` with `CancelFlag` and `ExportIniState`
   - `src-tauri/src/commands/` — Tauri commands wrapping exporter/format/push/pull crates
   - `web/src/` — React 19 + TypeScript + Vite SPA (~20 screens, shared components, typed API layer)
   - `web/src/screens/` — Full app screens: Extract, Format, Push, Pull, Home, Contacts, ConversationList, MessageView, SearchResults, ImportScreen, ExportScreen, ImportHistoryScreen, LoginScreen, RegisterScreen, OnboardingScreen, Settings, SettingsScreen, ProfileScreen, TrashScreen, ContactList
   - `web/src/components/` — Shared UI components (message renderers, contact cards, search, forms, etc.)
   - `web/src/lib/` — `tauri.ts` (typed `invoke()` wrappers), `api.ts` (vault server API client), `auth.tsx` (auth context with token persistence), `types.ts`, `savedGroups.ts`, `tauri-check.ts`
   - Jobs run on `std::thread`; progress streams to the frontend via Tauri events
   - `export.ini` persistence reuses `message-vault-io-core::ExportIniState`

7. **`crates/cli/vault-push/`** and **`crates/cli/vault-pull/`** — CLI/library crates for importing messages into / exporting from a Message Vault server. The GUI links them as libraries. `vault-pull` depends on `vault-push` for shared types. Each has a `cli` feature (default on) gating the standalone binary.

8. **`crates/vault/server/`** (`message-vault-server`) — HTTP API server backed by SQLite. Provides import (`POST /v1/import`), export, contacts, search, auth, and asset endpoints. The server can also run CLI commands (`import`, `dedupe-cross-source`, `import-contacts`, `reset-demo`, `serve`, `process-assets`) via its `main.rs`. Built as a Docker image (`docker/Dockerfile`); also usable directly via `cargo run --release -p message-vault-server -- serve`.

9. **`crates/vault/demo-seed/`** (`demo-seed`) — Generates synthetic conversation data for the demo vault (`staging/`, `config/`, README). Has both a library (`src/lib.rs`) and a CLI binary (`src/main.rs`). Used by `message-vault-server`'s `reset-demo` command.

### Supporting libraries

| Crate | Purpose |
|-------|---------|
| `message-csv` | CSV helpers shared across the workspace |
| `message-mail` | EML/MBOX generation from `ConversationDocument` |
| `message-sbr` | SyncTech SMS Backup & Restore XML read/write |
| `message-phone` | Phone number parsing and guarded E.164 normalization |
| `message-contacts` | Contact file parsing (VCF, CSV, AddressBook) and handle-generic name resolution |
| `message-media` | FFmpeg wrapper for attachment convert/compress |
| `message-obfuscate` | Deterministic pseudonym generation for obfuscated exports |
| `message-go-sms-mms` | GO SMS Pro PDU decode helpers |

### Key design decisions

- **JSONL is the canonical Extract Messages output.** The Extract Messages tab always writes JSONL. Users convert to other formats via the Format tab (`message-reexport`), which reads JSONL back through the common message model and writes the target format. This keeps the core pipeline simple and makes format conversion reproducible without re-parsing the backup.

- **Exporters are libraries, not subprocesses.** The GUI links every exporter crate with `default-features = false` (no `cli`/`clap`). Exporter CLIs ship from a separate [message-exporters](https://github.com/bitrealm-dev/message-exporters) repo. Only `wtsexporter` (WhatsApp, Python) and `ffmpeg`/`ffprobe` run as external processes.

- **Schema version 3 only.** The code makes no attempt to read older common-message JSON. Breaking changes to the schema are handled by regenerating exports.

- **`ExporterConfig` drives everything.** The GUI's per-source form validates, then calls `Form::to_config()` to produce an `ExporterConfig`. That config is passed to the exporter's `run()` function (library path) or to `FormatSink` (format path). CLI binaries parse the same config from clap args.

- **Obfuscation and media transforms run inside `FormatSink::finish`**, not as separate post-processing steps. When obfuscation is on, exporters skip staging real attachment bytes and `FormatSink` writes placeholder files.

- **`export.ini` persistence**: loaded from working directory first, then beside the binary; saved on tab switch, run, clear, and window exit. Passwords are never written. The vault key is stored in plain text under `[vault]` (file mode `0600` on Linux/macOS).

- **Unified GUI**: The Vite SPA serves as both the Tauri desktop app UI and (when built and placed in `message-vault-server/static/`) the web deployment. It connects to the vault API (`/v1/*`) for auth, import, export, contacts, messages, search, and settings. Screens are gated by auth state and Tauri availability (`isTauri()`). Import/Export require Tauri; Extract/Format require Tauri but not auth; Login/Register/Onboarding are public.

- **WSL detection**: Tauri uses the system file dialog (via `tauri-plugin-dialog`), which opens native dialogs on each platform.

### Vault server (message-vault-server)

The Push/Pull/Export/Import screens require a Message Vault server. The server is part of this workspace under `crates/vault/server/`. From a clone, run it on the host:

```bash
./scripts/run-vault-dev.sh                 # API at http://127.0.0.1:8080
./scripts/run-vault-dev.sh --reset-demo    # wipe data/, seed sample inbox
cd web && npm run dev                      # website at http://localhost:5173
```

A release-shaped image from this checkout:

```bash
docker compose -f docker/compose.release.yml up --build
```

An empty vault (no sample inbox): `./scripts/run-vault-dev.sh --reset`, or `DEMO_DATA=false` with the release Compose file.

The server exposes HTTP API + Web UI at `http://localhost:8080`. Create an account through the web UI, then use the Import API token from Settings for Push operations.

SQLite schema lives under `schema/sql/` (messages, contacts, accounts, FTS, staging). Server config templates are in `config/` (copy `config.toml.example` to `config.toml` for local use; `config.docker.toml` is used by the Docker entrypoint).

### Vault API client

The web app communicates with the message-vault-server API through typed wrappers in `web/src/lib/api.ts`. The `AuthProvider` in `web/src/lib/auth.tsx` manages login state with localStorage persistence and token validation. When running in Tauri, the app uses Tauri commands for Extract/Format/Push/Pull. When running in a browser (web deployment), it uses the vault HTTP API for everything.

### Vault import state

`vault-push` writes `.vault-import-state.jsonl` to track which conversations, messages, and assets have already been uploaded to a given vault URL + username. Re-runs skip already-recorded entries unless **Force reprocessing** is enabled (which uses `JournalState::default()` — an empty journal — so everything is re-submitted). Server-side dedup (`messages_deduped`, `already_present` for assets by SHA-256) prevents actual duplicates.

### CI workflow

A single workflow (`.github/workflows/ci.yml`) runs on push/PR to `main`:
- **Always**: `cargo fmt --all -- --check` (plus `src-tauri`), then `cargo build --workspace && cargo test --workspace` on ubuntu-latest
- **On tag `v*`**: additionally builds and pushes Docker image (`bitrealm/message-vault`) and builds Tauri desktop app for Linux/Windows/macOS, creating a GitHub Release

## Git workflow

- Use `gh` for all GitHub remote operations — `gh pr create`, `gh pr view`, `gh auth setup-git`
- Before `git push`, run `gh auth setup-git` if the remote is HTTPS; the environment has no credential helper but `gh` is authenticated with SSH

## Test conventions

- Exporters have smoke tests at `crates/exporters/*/tests/convert_smoke.rs` using committed fixtures under `crates/*/tests/fixtures/`.
- `vault-push` has mock tests at `crates/cli/vault-push/tests/push_mock.rs` using `httpmock`.
- Unit tests use `#[cfg(test)] mod tests` within source files.
- Integration tests live in each crate's `tests/` directory.
- No personal phone backups are needed to run the test suite.

## UI component conventions

- React 19 + TypeScript under `web/src/` with Vite bundling.
- Screens live in `web/src/screens/`; shared components in `web/src/components/`.
- Auth gating via `useAuth()` hook from `web/src/lib/auth.tsx`; Tauri gating via `isTauri()` from `web/src/lib/tauri-check.ts`.
- Typed API calls via `web/src/lib/api.ts` (vault server) and `web/src/lib/tauri.ts` (Tauri commands).
- The frontend has a unified GUI design spec at `docs/superpowers/specs/2026-08-06-unified-gui-design.md`.

## Cursor rules

This project has a communication-style rule (`.cursor/skills/communication-style/SKILL.md`): write for an experienced engineer with no project context; never use compressed shorthand like "parity", "hardening", "normalization", "cleanup"; avoid "we/us/our"; explain what changes, why, and what benefit it provides. This applies to documentation, code reviews, commit messages, and design writeups within this repo.

## Output format notes

- JSON/JSONL: attachment bytes are never stored in the document (`#[serde(skip)]`). Paths + digests point at sidecar `attachments/` directory.
- EML/MBOX/XML: `FormatSink` loads attachment files, embeds the bytes, then removes the staged `attachments/` directory.
- XML (`smses.xml`): one file for the entire export (not per-conversation). iMessage-only fields are dropped. SBR-origin `source.fields` can restore many SyncTech attrs on write-back.
- CSV columns are defined by `CSV_HEADERS` in `crates/libs/ir-format/src/write.rs`.

## Release process

Push a tag (`v*`) to trigger the CI workflow (`.github/workflows/ci.yml`), which builds and pushes the Docker image, builds Tauri desktop installers for Linux/Windows/macOS, and creates a GitHub Release with all artifacts. Bump `version` in `src-tauri/Cargo.toml` before tagging. Nothing builds or releases on push/PR without a tag.

## Licensing

The project is AGPL-3.0. `imessage-ir-exporter` still depends on `imessage-database` / `crabapple` (GPL-3.0-or-later). Combined binaries are AGPL-3.0. New exporter crates that wrap GPL libraries must keep that dependency visible in docs.
