# Exporters follow-up design — 2026-08-23

Product Rust audit follow-up, group 3 of 5: the 19 Exporters findings from
`docs/superpowers/reports/2026-08-23-rust-audit.md`. Scope: the 7 exporter
crates, `message-vault-io-core`, plus targeted additions to `libs/media` and
`libs/csv` (both already missing_docs-gated by the Libs group — new public
items land documented).

## Goal

Hoist the duplicated exporter helpers — the `run()` skeleton, the `main()`
CLI driver, the convert-export preamble, attachment naming/copy, and the
small mechanical helpers — into `message-vault-io-core` and the shared lib
crates; wire imessage-ir's silently dropped media flags; document the core
config/form surfaces; and split the four oversized emit.rs files — with
byte-identical serialized output and byte-identical CLI help everywhere.

## The 19 findings

| # | Sev | Category | Finding | Anchor |
|---|---|---|---|---|
| 1 | medium | duplication | bump / prepare_conversation / pending_to_document triads in 5 exporters | `go-sms-pro-exporter/src/emit.rs:55` |
| 2 | medium | duplication | Content-addressed attachment naming + skip-if-exists copy in 4 exporters | `go-sms-pro-exporter/src/emit.rs:239` |
| 3 | medium | duplication | convert_export output-overlap preamble in 4 exporters | `go-sms-pro-exporter/src/emit.rs:754` |
| 4 | medium | duplication | run() pipeline skeleton in 5 exporters | `go-sms-pro-exporter/src/run.rs:19` |
| 5 | medium | duplication | main() CLI driver in 6 of 7 binaries | `go-sms-pro-exporter/src/main.rs:13` |
| 6 | medium | duplication | convert_smoke test scaffolding across exporters | `go-sms-pro-exporter/tests/convert_smoke.rs:36` |
| 7 | low | duplication | clap_command() + test duplicated in all 7 cli.rs | `go-sms-pro-exporter/src/cli.rs:27` |
| 8 | low | duplication | bump() helper identical in 5 exporters | `go-sms-pro-exporter/src/emit.rs:55` |
| 9 | low | duplication | 64KB-chunk SHA-256 hasher written three times | `whatsapp-exporter/src/emit.rs:440` |
| 10 | low | duplication | CSV col()/field() helpers in two exporters | `openextract-exporter/src/parse.rs:140` |
| 11 | low | duplication | clap_command() + binary-name test copy-pasted ×7 | `go-sms-pro-exporter/src/cli.rs:34` |
| 12 | low | duplication | mime_for_ext maps partially duplicated | `go-sms-pro-exporter/src/emit.rs:76` |
| 13 | medium | docs-coverage | ExporterConfig pub fields undocumented | `core/message-vault-io-core/src/config.rs:104` |
| 14 | medium | docs-coverage | Form: 1 of 32 pub fields documented | `core/message-vault-io-core/src/exporters.rs:354` |
| 15 | low | docs-coverage | imazing emit.rs private helpers lack docs | `imazing-exporter/src/emit.rs:422` |
| 16 | low | docs-quality | Stale `message-media` flag name in AttachmentMedia doc | `core/message-vault-io-core/src/exporters.rs:262` |
| 17 | low | docs-quality | `` [`crate`]-style run `` phrasing confusing | `core/message-vault-io-core/src/pipeline.rs:52` |
| 18 | medium | api-design | imessage-ir silently drops --media-mode and media flags | `imessage-ir-exporter/src/main.rs:38` |
| 19 | low | structure | Four emit.rs files exceed 1000 lines | imazing 1225, imessage-ir 1197, go-sms-pro 1162, sms-backup-plus 1010 |

## Workstream 1: shared extraction into message-vault-io-core

Findings 1, 2, 3, 4, 5, 6, 7, 8, 11.

- **`ExportReport::bump(key, by)`** as a method on core's `ExportReport`
  (findings 1's bump half + 8); the five private copies are deleted.
- **`prepare_conversation` / `pending_to_document`** (finding 1's other
  half): hoist the shared portions into core — `prepare_conversation`'s
  identical signature + near-identical body is shared outright;
  `pending_to_document`'s signatures and bodies differ per exporter (the
  audit noted this), so only the common conversion skeleton is shared, with
  each exporter keeping its source-specific field mapping and its exact
  emitted values.
- **`run_pipeline`** (finding 4): the shared skeleton — check_cancel,
  contacts resolution, `ExportTransforms::from_configs`, the
  media-failed-for-all bail, `sink.log_lines` / `report.summary_lines`, and
  `RunResult` — parameterized over a per-exporter convert closure. Each
  exporter's `run()` keeps only source-specific conversion. Every log-line
  and error string stays byte-identical.
- **`run_cli`** (finding 5): the shared main driver — `parse_date_range`,
  `OutputFormat::parse`, `compress_options_from_cli`, `ExporterConfig`
  construction, `run`, `print_result` — with source-specific args via a
  builder closure. imessage-ir's divergent main is left as-is. Each binary's
  flags, defaults, and help text stay byte-identical.
- **`ExporterConfig::prepare_outputs`** (finding 3): create_dir_all +
  canonicalize + the overlap bail with its exact current text
  (`output {} must not be the same as, or contain, the input {}`).
- **Attachment helpers** (finding 2): `attachment_dest_name`
  (`{local-date}-{digest16}{ext}`) and a copy-if-missing helper, used by the
  four exporters that duplicate them.
- **`clap_command` scaffolding** (findings 7, 11): `clap_command()` plus the
  `clap_command_uses_binary_name` test provided once from core's
  feature-gated `cli` module; each exporter delegates. The rendered `--help`
  output of every binary must stay byte-identical (pinned by the committed
  CLI-pages test).
- **`convert_smoke` scaffolding** (finding 6): a core `testutil` feature
  (same pattern as message-ir's) with `empty_contacts`, the convert wrapper,
  and `assert_csv_output`; every `convert_smoke.rs` adopts it.

## Workstream 2: media and csv helper homes

Findings 9, 10, 12.

- **`media::file_sha256`** (finding 9): the 64KB-chunk SHA-256 helper lands
  once in `libs/media`; whatsapp, imazing, and ir-format call it. The
  existing callers' outputs are identical (same chunking, same hex finish).
- **`mime_for_ext`** (finding 12): the shared table lands in `libs/media`;
  each exporter keeps only its source-specific extra arms (e.g. go-sms-pro's
  `.wav`).
- **`col()` / `field()`** (finding 10): move byte-identical into
  `libs/csv`; openextract and imazing import them.

## Workstream 3: wire imessage-ir's media flags

Finding 18. User-approved ruling: wire the flags.

- imessage-ir's `main.rs` builds `MediaConfig` from `common.media_mode` plus
  `compress_options_from_cli` (max resolution, fps, min size,
  skip-efficient), exactly like go-sms-pro and imazing.
- Behavior change is the point: `--media-mode convert/compress` on
  imessage-ir now converts media instead of silently ignoring the flag.
  Tests gain media fixtures covering convert and compress.

## Workstream 4: documentation

Findings 13, 14, 15, 16, 17.

- **`ExporterConfig`** (finding 13): every field documented, pointing CLI
  readers at the same-named `CommonCli` flag.
- **`Form`** (finding 14): all 32 fields documented — it is the GUI's wire
  contract (`web/src/lib/types.ts`).
- **imazing private helpers** (finding 15): one-line docs for `resolve_tz`,
  `parse_message_date`, `is_outgoing`; `resolve_chat_identifier`'s doc
  expanded to explain when `unresolved_phone` is set.
- **Doc-quality** (findings 16, 17): `message-media` reworded to reference
  the real `--media-mode` flag with a [`FormatSink`] link; the
  `` [`crate`]-style `` phrasing reworded to "Result of a successful
  exporter run: human-readable log lines."
- **Scope addition (flagged for review):** add `#![warn(missing_docs)]` to
  `message-vault-io-core` in the same task as the config/form docs, and
  document whatever else the gate surfaces there. This goes beyond the 19
  findings but completes the audit's "every lib crate" language for core —
  strike it if you want core left ungated.

## Workstream 5: emit.rs splits

Finding 19. The four files over 1,000 lines split into
parsing / document-building / attachment modules per exporter; exact cut
lines come from each file's function inventory at plan time. The shared
parts then consume the Workstream 1/2 helpers (the second half of the
audit's "split, then share the common parts"). Crate-internal moves only —
no public API change, decode/emit behavior unchanged (each exporter's
convert_smoke suite is the pin).

## Global constraints

- **Behavior-preserving** except the sanctioned media-flags wiring: byte-
  identical serialized output (CSV, EML, JSON, XML), byte-identical CLI help
  text, identical error text everywhere.
- **Green after every task.** `cargo test --workspace` (67 targets), fmt,
  check, clippy `-D warnings`.
- **Docs gates.** `cargo doc --no-deps` zero warnings for
  message-vault-io-core (if gated), media, and csv.
- **Generated artifacts.** `openapi.json` untouched; CLI reference pages
  byte-identical unless a task's own clap change alters rendered help — in
  which case the page is regenerated in the same task
  (`cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference`).
  The committed-dump tests must stay green.
- **Doc style.** `docs/src/content/docs/vault/developer/rustdoc-style.md`
  governs all doc text. No `#[allow(missing_docs)]`.
- **No new crates, no dependency version bumps.** The testutil and cli
  modules live inside `message-vault-io-core` behind features.

## Non-goals

Other groups' crates (CLI tools, server, Tauri, GUI); dependency bumps;
changes to the exporters' documented CLI surfaces beyond the media-flags
wiring; the obfuscation source-bag leak (#97 — tracked separately).

## Testing and verification

- Behavior pins: every exporter's `convert_smoke` and fixture tests; the
  committed CLI-pages test (byte-identical help); ir-format round-trip tests
  (untouched); imessage-ir's new media fixtures.
- Final gate: `cargo test --workspace`, fmt/check/clippy, per-crate
  `cargo doc --no-deps`, both committed-dump tests.

## Process

Same cycle as the previous groups: this spec → PR; implementation plan
(`docs/superpowers/plans/2026-08-23-exporters-followup.md`) → PR;
subagent-driven development in a fresh worktree — per-task implementer +
spec/quality review, fix rounds, whole-branch review on the strongest model
— ending in a PR.
