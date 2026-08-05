# Tauri Desktop App for message-vault-io

**Date**: 2026-08-05
**Status**: draft

## Context

message-vault-io today is a Slint-based desktop GUI that handles extracting messages from phone backups, converting formats, and pushing/pulling data to a Message Vault server. message-vault-rs is a separate project — a Docker-based vault server with an axum HTTP API for import/export and a Next.js web app for browsing messages.

The user wants a desktop app that uses web technology for the UI (replacing Slint) while keeping all existing Rust extraction code. The vault server always runs in Docker — even locally — so the desktop app always communicates with it over HTTP. The browsing experience (Next.js) stays in the vault server for remote browser access.

**Goal**: Replace the Slint desktop GUI with a Tauri-based desktop app that reuses all existing Rust extraction, format conversion, and HTTP client code. The Tauri frontend is a lightweight SPA focused on extraction, vault-push/pull, and format conversion screens.

## Architecture

```
message-vault-io/                       # Desktop extraction app
  src-tauri/                            # Tauri shell
    Cargo.toml                          # depends on all exporter crates + vault-push/pull
    src/
      main.rs                           # Tauri entry point, registers commands
      commands/
        extract.rs                      # spawn exporter, stream progress events
        format.rs                       # wrap message-reexport
        push.rs                         # wrap vault-push::run()
        pull.rs                         # wrap vault-pull::run()
        contacts.rs                     # wrap message-contacts
      state.rs                          # CancelFlag, export.ini persistence
    tauri.conf.json                     # window config, capabilities, bundling
  web/                                  # Lightweight SPA (Vite + React)
    src/
      screens/                          # Home, Extract, Format, Push, Pull, Contacts, Settings
      components/                       # Shared UI components
      lib/                              # Tauri invoke wrappers, progress event helpers
  crates/                               # Existing crates (unchanged)
    exporters/                          # 7 exporter crates
    vault-push/, vault-pull/            # HTTP clients
    message/message-ir, ir-format, ...  # Shared libraries
    message-vault-io-core/              # ExporterConfig, Form trait, ExportIniState, CancelFlag
    message-vault-io-gui/               # Slint GUI — removed in final phase

message-vault-rs/                       # Server (unchanged)
  web/                                  # Next.js browsing UI
  src/                                  # axum import/export API
  Dockerfile, compose-*.yml             # Docker deployment
```

**Why two separate repos and deliverables**: The vault server is always in Docker. The desktop app talks to it over HTTP (using the existing vault-push/vault-pull crates). The Next.js browsing UI stays in the server repo because it's deployed as part of the Docker image. The Tauri app's SPA is a separate, focused frontend for extraction workflows — it is not a replacement for the browsing UI.

### Component interaction

```
Tauri Desktop App (message-vault-io)
+------------------------------------------+
|  SPA (React + Vite)                      |
|  Extract | Format | Push | Pull | Home   |
+------------------------------------------+
        | invoke()                | Tauri events (progress)
        v                        v
+------------------------------------------+
|  Rust Backend                             |
|  Exporters | vault-push | vault-pull     |
|  FormatSink | Contacts | ffmpeg          |
|  CancelFlag | export.ini                 |
+------------------------------------------+
        |
        | HTTP (Bearer token)
        v
+------------------------------------------+
|  Vault Server (Docker)                    |
|  Next.js (browsing) | axum (import API) |
|  SQLite                                   |
+------------------------------------------+
```

The frontend never touches the filesystem or spawns processes. It calls Tauri commands and renders results. The Rust backend reuses existing crates with minimal wrapping.

## Tauri command layer

Each Tauri command is a thin wrapper around an existing crate's `run()` function. The pattern:

1. Frontend calls `invoke("command_name", { config })`
2. Tauri command spawns the work on a `std::thread` (non-blocking)
3. Work function drains the existing `mpsc::Sender<ProcessEvent>` channel
4. Progress events are forwarded to the frontend via Tauri events
5. Frontend renders progress bar, log lines, completion status

Commands:

| Command | Wraps | Inputs | Events |
|---------|-------|--------|--------|
| `extract` | Exporter `run()` | ExporterConfig (source type, paths, media options) | Progress: files found, messages parsed, attachments processed, done |
| `format` | message-reexport | Input dir, target format | Progress: conversations written, attachments converted |
| `push` | vault-push::run() | Server URL, API key, input dir, source name | Progress: assets uploaded, messages sent, dedup stats |
| `pull` | vault-pull::run() | Server URL, API key, search query, output dir | Progress: pages fetched, messages written, assets downloaded |
| `contacts` | contacts library | Input file (VCF/CSV) | Progress: contacts parsed |
| `cancel` | CancelFlag.set() | — | — |

## Tauri frontend SPA

Lightweight React + Vite app (no Next.js needed — served locally, no SSR requirement).

### Screens

| Screen | Purpose | Key components |
|--------|---------|----------------|
| Home | Dashboard: recent exports, vault connection status, quick action buttons | Status cards, recent activity list |
| Extract | Pick exporter, backup path, media options (convert/compress/resolution), output directory, run | Exporter picker (dropdown), path pickers, FormRow widgets, ProgressBar, LogViewer |
| Format | Pick input directory, target format, output directory, run | Format picker (JSONL/EML/MBOX/CSV/XML), path pickers, ProgressBar |
| Vault Push | Server URL, API key, source name, input directory, run | Credential fields, path picker, ProgressBar, PushReport summary |
| Vault Pull | Server URL, API key, search query builder, output directory, run | Credential fields, search query input, date pickers, ProgressBar, PullReport summary |
| Contacts | Import VCF/CSV file | File picker, preview table |
| Settings | Default paths, vault credentials, ffmpeg path, theme | Form fields, Save button |

### UI component conventions

Match the existing Slint GUI's design language where practical (dense form layouts with fixed label columns, anchored scroll pages) since users are familiar with it. Use the `frontend-design:frontend-design` skill when building the actual components.

## Build and packaging

```bash
# Development
cd message-vault-io
cd web && npm ci && cd ..
cargo tauri dev              # Tauri dev mode with SPA hot-reload

# Release
cargo tauri build            # produces platform-appropriate installer
```

Tauri v2 bundles the SPA's compiled output as static assets in the binary. No separate web server process. The only runtime dependency is ffmpeg (detected at startup, configurable path).

The message-vault-rs server deployment is unchanged: `docker compose up` builds and runs the full stack (axum API + Next.js browsing UI + SQLite).

## Transition plan

Four phases, each independently shippable:

### Phase 1: Scaffold
- Create `src-tauri/` with Cargo.toml depending on message-vault-io-core
- Create `web/` with Vite + React scaffold, Home screen only
- Implement ffmpeg detection command
- Implement `export.ini` load/save commands
- Verify Tauri builds and launches
- Slint GUI remains the primary app

### Phase 2: Extraction (prove the pattern)
- Port one exporter (SMS Backup & Restore — simplest, no external dependencies) to a Tauri command
- Build Extract screen in the SPA
- Wire progress events end-to-end
- Manual test: extract a real backup, verify output
- Slint GUI coexists

### Phase 3: Full parity
- Port remaining 6 exporters
- Build Format, Vault Push, Vault Pull, Contacts, Settings screens
- Wire vault-push and vault-pull (reuse existing HTTP clients, add Tauri progress events)
- Slint GUI removed

### Phase 4: Polish
- App icons and branding
- Platform installers (.deb, .AppImage, .dmg, .msi)
- Auto-update support (Tauri updater)
- Documentation updates
- Remove `crates/message-vault-io-gui/` and Slint dependency from workspace

## Error handling

- **Tauri command errors**: Structured error types returned to frontend. Displayed as toast or banner.
- **ffmpeg missing**: Detected at startup via `which ffmpeg`. Extraction screens show warning with install instructions when absent.
- **Permission prompts**: Tauri v2 capabilities file declares filesystem access. OS prompts on first run.
- **Crash recovery**: Existing journal files (`.vault-import-state.jsonl` for push, `.vault-pull-state.jsonl` for pull) enable resume on restart.
- **Cancel**: `CancelFlag` (shared between thread and command) allows clean cancellation mid-extraction.

## Testing strategy

| Layer | Method |
|-------|--------|
| Tauri commands | Unit tests in `src-tauri/src/commands/` with temp directories |
| Exporters | Existing smoke tests (`convert_smoke.rs`) — same code paths |
| vault-push/pull | Existing mock tests (`push_mock.rs`) using `httpmock` |
| SPA frontend | `npm test` with Vitest — component tests for forms and progress |
| End-to-end | Manual: extract test backup, push to local vault server, verify in browsing UI |

Most test coverage comes from the existing crate test suites. The Tauri layer is thin enough that it primarily needs integration tests to confirm the command wrappers work.

## Key design decisions

- **Tauri over Electron**: All extraction code is Rust. Tauri calls it directly via crate dependencies rather than spawning subprocesses or using native Node addons.
- **SPA over Next.js for the Tauri frontend**: No SSR needed for a locally-served app. Vite + React is simpler and faster to bundle.
- **Two repos stay separate**: The vault server (Docker) and desktop app are different deployment targets. The Next.js browsing UI stays with the server.
- **Slint GUI removed at the end**: Phased transition avoids maintaining two GUIs indefinitely.

## Scope boundaries

What this design covers:
- Desktop app with extraction, format conversion, vault-push/pull, contacts import
- Replacing the Slint GUI with a Tauri + web frontend

What this design does NOT cover:
- Changes to the vault server or its Next.js browsing UI
- Changes to the vault-push/vault-pull wire protocol
- New exporter sources
- Mobile apps or mobile browser UX
