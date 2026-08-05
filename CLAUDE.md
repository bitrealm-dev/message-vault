# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Docs site

The public documentation is published from the unified [bitrealm-dev.github.io](https://bitrealm-dev.github.io/) hub repo. Edit content in `docs/src/content/docs/` — there is no docs deploy workflow in this repo anymore. The hub syncs content from here at build time.

## Build and test

```bash
# Build everything (first build takes several minutes)
cargo build --workspace

# Release build (substantially faster for real export work)
cargo build --workspace --release

# Run the GUI (use --release for realistic export performance)
cargo run -p message-vault-io-gui
cargo run --release -p message-vault-io-gui

# Run all tests (uses committed fixtures, no personal backups needed)
cargo test --workspace

# Run a single crate's tests
cargo test -p sms-backup-restore-exporter

# Build and check docs site
cd docs && npm ci && npm run check && npm run build
cd docs && npm run dev   # local preview
```

**Requirements**: Rust 1.85+ (edition 2024), Node.js 24 (docs only). Linux needs fontconfig and X11 keyboard libs (see CONTRIBUTING.md). `ffmpeg`/`ffprobe` on PATH for media convert/compress features.

## Architecture

This is a Rust workspace that converts phone message backups into a shared conversation structure, then packages each conversation in the user's chosen format.

**Pipeline**: `vendor backup → parse → ConversationDocument (schema v3) → FormatSink → output format`

### Layer model

1. **`crates/message/ir/`** (`message-ir`) — Schema types only: `ConversationDocument`, `IrMessage`, `IrAttachment`, enums. No I/O, no formatting. Attachment bytes are never serialized to JSON (`#[serde(skip)]`); paths and digests point at sidecar files.

2. **`crates/message/ir-format/`** (`message-ir-format`) — `FormatSink` that takes parsed conversations and writes the chosen output format (JSON, JSONL, CSV, EML, MBOX, or a single SyncTech `smses.xml`). Runs media transforms (copy/convert/compress) and obfuscation during `FormatSink::finish`. Readers exist for every format to enable round-trip conversion.

3. **`crates/message/reexport/`** (`message-reexport`) — Directory converter (GUI **Format** tab). Auto-detects input format in an export folder, reads all conversations, writes them in a target format via `FormatSink`.

4. **Exporter crates** under `crates/exporters/` — Each parses one backup source into `ConversationDocument` and feeds it to `FormatSink`. The GUI links them as libraries (`default-features = false` in GUI Cargo.toml, which drops the `cli` feature). Each crate has a `cli` feature (default on) that gates the standalone binary behind `dep:clap`. Three tiers:
   - **Primary**: iMessage (`imessage-ir-exporter`), WhatsApp (`whatsapp-exporter`, shells out to `wtsexporter`), SMS Backup & Restore (`sms-backup-restore-exporter`)
   - **Experimental**: GO SMS Pro, iMazing, OpenExtract, SMS Backup+
   - See `docs/maintainers/exporter-matrix.md` for per-exporter capability gaps

5. **`crates/message-vault-io-core/`** — Shared form model (`ExporterConfig`, `Exporter` enum, `Form` trait for GUI validation), job spawning (`spawn_job` with `CancelFlag` + `mpsc::Sender<ProcessEvent>`), and ini persistence (`ExportIniState`).

6. **`crates/message-vault-io-gui/`** — Slint 1.17 desktop app. Architecture:
   - `ui/app-window.slint` — root window, tabs, error banner
   - `ui/pages/*.slint` — one page per tab, each exports a `*Adapter` global
   - `src/main.rs` — constructs window, wires callbacks, persists `export.ini`
   - `src/state.rs` — `AppState` behind `Arc<Mutex<_>>`
   - `src/jobs.rs` — in-process library dispatch for exporters/contacts/format/vault
   - `src/sync.rs` — push/pull between `AppState` and Slint adapters
   - Jobs run on `std::thread`; a bridge thread drains the channel and posts log lines onto the Slint UI thread via `slint::Weak::upgrade_in_event_loop`.

7. **`crates/vault-push/`** and **`crates/vault-pull/`** — CLI/library crates for importing messages into / exporting from a Message Vault server. The GUI links them as libraries. `vault-pull` depends on `vault-push` for shared types.

### Supporting libraries

| Crate | Purpose |
|-------|---------|
| `message-csv` | CSV helpers shared across the workspace |
| `message-mail` | EML/MBOX generation from `ConversationDocument` |
| `message-sbr` | SyncTech SMS Backup & Restore XML read/write |
| `message-phone` | Phone number parsing and normalization |
| `message-contacts` | Contact file parsing (VCF, CSV, AddressBook) and name resolution |
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

- **WSL detection**: the GUI auto-opens Windows-native file dialogs and the Windows browser when it detects WSL (see `src/wsl.rs`).

### Vault import state

`vault-push` writes `.vault-import-state.jsonl` to track which conversations, messages, and assets have already been uploaded to a given vault URL + username. Re-runs skip already-recorded entries unless **Force reprocessing** is enabled (which uses `JournalState::default()` — an empty journal — so everything is re-submitted). Server-side dedup (`messages_deduped`, `already_present` for assets by SHA-256) prevents actual duplicates.

## Test conventions

- Exporters have smoke tests at `crates/exporters/*/tests/convert_smoke.rs` using committed fixtures under `crates/*/tests/fixtures/`.
- `vault-push` has mock tests at `crates/vault-push/tests/push_mock.rs` using `httpmock`.
- Unit tests use `#[cfg(test)] mod tests` within source files.
- Integration tests live in each crate's `tests/` directory.
- No personal phone backups are needed to run the test suite.

## UI component conventions

- Slint `.slint` files under `crates/message-vault-io-gui/ui/`.
- Pages export a single `*Adapter` global singleton for property/callback binding.
- Form controls use dense `FormRow` widgets from `ui/widgets.slint` with a fixed label column.
- The Log tab's viewer is the only element that grows on vertical window resize.
- Wizard-style flows (Guided Import, Vault Export) use screens composed in `app-window.slint`.

## Cursor rules

This project has a communication-style rule (`.cursor/skills/communication-style/SKILL.md`): write for an experienced engineer with no project context; never use compressed shorthand like "parity", "hardening", "normalization", "cleanup"; avoid "we/us/our"; explain what changes, why, and what benefit it provides. This applies to documentation, code reviews, commit messages, and design writeups within this repo.

## Output format notes

- JSON/JSONL: attachment bytes are never stored in the document (`#[serde(skip)]`). Paths + digests point at sidecar `attachments/` directory.
- EML/MBOX/XML: `FormatSink` loads attachment files, embeds the bytes, then removes the staged `attachments/` directory.
- XML (`smses.xml`): one file for the entire export (not per-conversation). iMessage-only fields are dropped. SBR-origin `source.fields` can restore many SyncTech attrs on write-back.
- CSV columns are defined by `CSV_HEADERS` in `crates/message/ir-format/src/write.rs`.

## Release process

A manual GitHub Actions workflow (`.github/workflows/release.yml`) builds three platform archives from the crate GUI's `Cargo.toml` version. Nothing builds or releases on push/PR by default. Bump `version` in `crates/message-vault-io-gui/Cargo.toml` before running the workflow. Docs deploy automatically on push to `main` when files under `docs/` change (`.github/workflows/docs.yml`).

## Licensing

Most crates are MIT. `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database` / `crabapple` dependencies), which means the GUI binary includes GPL-licensed code. New exporter crates that wrap GPL libraries must propagate that license.
