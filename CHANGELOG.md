# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Dating rule:** every bullet under `[Unreleased]` and under a version heading must
start with an ISO date (`YYYY-MM-DD`), the day the change landed (or is recorded).
Released version headings also carry a date: `## [0.8.0] - 2026-08-24`.

## [Unreleased]

### Fixed

- 2026-08-24: Document and ship CORS origins for packaged desktop builds (`tauri://localhost`, `http://tauri.localhost`, `https://tauri.localhost`). Release AppImages were blocked from Connect when the vault only allowed Vite `:5173` origins.

### Changed

- 2026-08-24: Faster HTTP vault-push: skip per-file asset HEAD until this run sees
  `already_present`, raise JSONL import batches from 8 MiB to 50 MiB, let the
  desktop app flush imports on size only, overlap more prepare/upload work, and
  keep 64 idle HTTP connections per host.
- 2026-08-24: Make the Messages/Contacts nav width-draggable, and let the conversation/contact list shrink to nothing when the window is narrow so the thread stays readable.
- 2026-08-24: **Desktop host:** share the cancel/spawn/error scaffolding across the four
  job commands, wire the shared cancel flag into push, drop the runtime
  `MESSAGE_VAULT_IO_BIN` environment writes (sound env access), split the
  extract progress parser into its own module, document the IPC DTO wire
  contract, and gate src-tauri with clippy and tests in CI. Push now honors
  Cancel; the only other product delta is that a KnugiHK binary placed in a
  custom tools folder is no longer found by WhatsApp-Android export.
- 2026-08-23: Server crate cleanup: rustdoc and HTTP API descriptions rewritten, handlers
  moved out of `server.rs`, thread-tag and contact-group CRUD unified, and
  API-token label validation typed. No API behavior change.
- 2026-08-23: **Libraries:** add the `missing_docs` gate to every lib crate and document
  the full public surface, share one `AttachmentMeta` across the IR, CSV,
  and mail layers, switch csv parsers to `anyhow` errors, expose the
  unsafe-attachment-path message as a const, share one test fixture, and
  split the go-sms-mms unit decoders into their own module. No API behavior
  change.
- 2026-08-23: **Exporters:** hoist the duplicated exporter pipeline, CLI driver, output
  preamble, attachment naming, and mechanical helpers into
  `message-vault-io-core` and the shared lib crates, document and gate the
  core config/form surfaces, split the four oversized emit.rs files, and
  wire imessage-ir's previously ignored media flags. CLI help text and
  exported output are unchanged; imessage-ir now honors `--media-mode`
  convert/compress when passed.
- 2026-08-23: **CLI tools:** extract the duplicated JSONL journal and vault HTTP client
  into two shared lib crates, replace substring retry classification with a
  typed classifier (all 4xx failures are permanent), wire demo-seed's
  name-shape and label-name config into the generator, and document the
  dump-cli-docs surface. Retry and truncation edge cases fixed; journal
  files, error text, and the demo dataset unchanged.

### Added

- 2026-08-23: Generated HTTP API route catalog at `/vault/developer/rustdoc/http/`, plus an optional explorer at `/docs` when `[server] openapi_ui` is true
- 2026-08-22: CLI reference pages on the docs site generated from clap
- 2026-08-22: Workspace rustdoc on the docs site at `/vault/developer/rustdoc/`

Installable builds and release notes also appear on
[GitHub Releases](https://github.com/bitrealm-io/message-vault/releases).

The public site summary is at <https://bitrealm.io/changelog/>.
