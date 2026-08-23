# Libs follow-up design — 2026-08-23

Product Rust audit follow-up, group 2 of 5: the 13 Libs findings from
`docs/superpowers/reports/2026-08-23-rust-audit.md` (approved as part of PR #90).
Scope: the 11 crates under `crates/libs/`, plus ripple edits in
`message-vault-io-core` and `message-vault-server`.

## Goal

Add the `missing_docs` gate to every lib crate and document the full public
surface, fix the doc-quality and error-handling findings, consolidate the
triplicated attachment metadata and ConversationDocument test fixtures, and
split the go-sms-mms decode monolith — with zero change to serialized output
or user-visible behavior.

## The 13 findings

| # | Sev | Category | Finding | Anchor |
|---|---|---|---|---|
| 1 | high | docs-coverage | Core IR model types have no doc comments | `crates/libs/ir/src/lib.rs:20` |
| 2 | medium | docs-coverage | Re-exported sbr reader types undocumented | `crates/libs/sbr/src/read.rs:71` |
| 3 | medium | docs-coverage | Re-exported ffmpeg probe API undocumented | `crates/libs/media/src/tools.rs:63` |
| 4 | medium | docs-coverage | ValidateMode/ValidateReport/VcfCard lack docs | `crates/libs/contacts/src/validate.rs:13`, `vcf.rs:8` |
| 5 | medium | docs-quality | Contradictory field doc: "Absent when present." | `crates/libs/ir/src/lib.rs:296` |
| 6 | medium | docs-quality | Broken doc link to docs/maintainers/architecture/message-ir.md | `crates/libs/ir/src/lib.rs:7` |
| 7 | low | docs-quality | Cryptic public field name `fn_attr` with no doc | `crates/libs/sbr/src/read.rs:35` |
| 8 | medium | best-practices | No `#![warn(missing_docs)]` in any lib crate | all 11 lib crates |
| 9 | medium | error-handling | "unsafe attachment path" message is a cross-crate string contract | `crates/libs/ir-format/src/util.rs:82` |
| 10 | low | error-handling | String error types at the crate boundary | `crates/libs/csv/src/utc_offset.rs:12`, `date_range.rs:18` |
| 11 | medium | duplication | Attachment metadata shape triplicated across libs | `crates/libs/ir/src/lib.rs:285`, `csv/src/lib.rs:15`, `mail/src/lib.rs:59`, `ir-format/src/write.rs:258-270` |
| 12 | low | duplication | ConversationDocument test fixtures duplicated across crates | `ir-format/src/format_sink.rs:195`, `ir-format/src/lib_tests.rs:14`, `reexport/src/lib.rs:398` |
| 13 | low | structure | mms_enc.rs is a 2021-line monolith | `crates/libs/go-sms-mms/src/mms_enc.rs` |

## Workstream 1: missing_docs gate + full public-surface documentation

Findings 1, 2, 3, 4, 8.

- **Gate strategy.** Each of the 11 lib crates gets `#![warn(missing_docs)]`
  (`crates/libs/{ir,ir-format,sbr,media,contacts,csv,go-sms-mms,obfuscate,mail,phone,reexport}`).
  A crate's gate lands in the same task as that crate's documentation sweep,
  so the workspace is green after every task. The gate is **warn**, not deny
  (matching the server crate). No `#[allow(missing_docs)]` anywhere.
- **Not** a workspace `[lints]` table: exporters and the GUI are not clean,
  and those groups are not this one.
- **message-ir (finding 1, high).** Every `pub` type, variant, and field gets
  a doc comment describing what that serialized field means — this is the
  wire model every exporter writes and every reader parses. Bare items today
  include `ConversationDocument`, `ExportMeta`, `IrConversationType`,
  `HandleType`, `ConversationMeta`, `ConversationStats`, `IrParticipant`,
  `IrService`, `IrMessage`, `IrDirection`, `IrAttachment`,
  `ConversationHeader`, and most of their fields.
- **Other named sweeps.** sbr's re-exported reader types
  (`Record` + fields, `ConversationKind`, `AttachmentBlob`, `SourceFields`,
  `ParseStats`); media's `FfmpegToolsProbe` + fields, `ffmpeg_available`,
  `probe_ffmpeg_tools`, `MediaReport`; contacts' `ValidateMode`,
  `ValidateReport`, `VcfCard` + `fn_raw`/`n_family`/`n_given`/`n_middle`
  fields. Whatever else the gate surfaces in the remaining crates gets
  documented in the same sweep.
- **Style.** All doc prose follows
  `docs/src/content/docs/vault/developer/rustdoc-style.md`.
- **Per-crate check.** `cargo doc --no-deps -p <crate>` emits zero warnings.

## Workstream 2: doc-quality fixes

Findings 5, 6, 7.

- **Broken link (finding 6).** Replace the relative
  `../../../docs/maintainers/architecture/message-ir.md` link with
  `https://bitrealm.io/vault/developer/architecture/common-message/` — the
  same published-URL pattern sbr's intro already uses.
- **"Absent when present" (finding 5).** Reword to: "None when the attachment
  was imported; set (`too_large` / `file_missing`) only when bytes were
  skipped." (audit's suggested wording).
- **`fn_attr` (finding 7).** Rename `MmsPart.fn_attr` to `filename_attr` and
  document `MmsPart` and its `ct`/`cl`/`name` fields. Verified: no consumers
  outside `sbr/src/read.rs` itself, and `MmsPart` is not re-exported.

## Workstream 3: error handling

Findings 9, 10.

- **The string contract (finding 9).** `ir-format` gains
  `pub const UNSAFE_ATTACHMENT_PATH_PREFIX: &str = "unsafe attachment path (contains ..)";`
  re-exported from `lib.rs`. `util.rs:82` formats its bail from the const, so
  the emitted text is byte-identical. The server's two string-match sites
  (`crates/vault/server/src/import/mod.rs:2105,2166`, both test asserts)
  import the const instead of hardcoding the text. Behavior is unchanged —
  the asserts still pass.
  - Out of scope here: the two *other* bails with a similar prefix
    (`crates/cli/vault-push/src/run.rs:732`, `crates/vault/server/src/config.rs:145`)
    are separate texts and separate contracts in other groups' crates; they
    are not touched.
- **csv error types (finding 10).** `parse_utc_offset`, `DateRange::parse`,
  and its sibling `DateRange::parse_optional_tz` switch from
  `Result<_, String>` to `anyhow::Result` (adding `anyhow` as a dependency —
  a workspace dependency, not a version bump). No message text changes.
  `message-vault-io-core/src/pipeline.rs` has two wrappers that call these
  and return `Result<DateRange, String>` publicly; they keep their public
  signatures and adapt their `map_err` formatting trivially.

## Workstream 4: consolidation

Findings 11, 12, 13.

- **Shared attachment metadata (finding 11).** New struct in `message-ir`:
  `pub struct AttachmentMeta` with the four common fields taken verbatim from
  `IrAttachment` — `path`, `original_name`, `mime_type`, `digest_sha256`.
  - `IrAttachment` composes it (a `meta` field) plus its extras
    (`is_sticker`, `transcription`, `sticker_effect`).
  - `csv::AttachmentCell` and `mail::MailAttachment` compose the same struct
    plus their layer-specific extras.
  - ir-format's hand-written field-by-field mapping (`write.rs:258-270` and
    the mirrored readers) is replaced by `From` impls on `AttachmentMeta`.
  - Both writers build output manually (not via serde derives — verified at
    plan time); serialization keys and output bytes stay identical regardless.
  - Composition (a `meta` field) is chosen over flattening; consumer edits
    are compiler-guided.
- **Test fixtures (finding 12).** `message-ir` gains a `testutil` feature
  exposing one `sample_document()` builder. `message-ir-format` and
  `message-reexport` dev-depend on it
  (`message-ir = { workspace = true, features = ["testutil"] }`) and delete
  their local `tiny_doc`/`sample_doc` copies. The exporters' `convert_smoke`
  scaffolding is a separate finding and stays with the Exporters group.
- **mms_enc split (finding 13).** The ~30 `decode_*` unit decoders move to a
  private submodule (`mod decoders`), leaving PDU assembly in `mms_enc.rs`.
  Crate-private move; public API unchanged.

## Global constraints

- **Behavior-preserving.** Byte-identical serialized output (JSON, CSV, EML,
  XML), identical error text everywhere, identical HTTP behavior. Public Rust
  API changes are allowed within the workspace (all consumers are workspace
  members; there are no external consumers) when compiler-guided.
- **Green after every task.** `cargo test --workspace` (67 targets), fmt,
  check, and `cargo clippy --workspace -- -D warnings` clean.
- **Docs gate.** `cargo doc --no-deps` emits zero warnings for all 11 lib
  crates.
- **No generated artifacts change.** `openapi.json` and the CLI reference
  pages are untouched (no utoipa or clap changes), so the committed-dump
  tests stay green.
- **Doc style.** `docs/src/content/docs/vault/developer/rustdoc-style.md`
  governs all doc text.
- **Surface.** Each crate's `lib.rs` re-exports remain the curated public
  API; the gate documents that surface.

## Non-goals

Exporters, CLI tools, Tauri host, and GUI changes (other groups' findings);
dependency version bumps; a workspace-wide lints table; new workspace
members (the fixture builder lives inside `message-ir` behind a feature);
publishing configuration changes.

## Testing and verification

- Behavior pins that must stay green untouched: ir-format round-trip
  reader/writer tests, the exporters' `convert_smoke` output assertions, the
  server import tests asserting the unsafe-attachment-path error, and the
  existing sbr/media/contacts suites.
- Full-suite evidence at the end: `cargo test --workspace`, fmt/check/clippy,
  and per-crate `cargo doc --no-deps` runs, plus the committed-dump tests.

## Process

Same cycle as the server group: this spec → PR; implementation plan
(`docs/superpowers/plans/2026-08-23-libs-followup.md`) → PR; then
subagent-driven development in a fresh worktree — a fresh implementer per
task, a spec-and-quality task review after each, fix rounds, and a
whole-branch review on the strongest model — ending in a PR.
