# Exporters follow-up implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 19 Exporters findings from the product Rust audit — hoist the duplicated exporter helpers (`run()` skeleton, `main()` driver, convert preamble, attachment naming/copy, mechanical helpers) into `message-vault-io-core` and the shared lib crates, wire imessage-ir's silently dropped media flags, document and gate the core surface, and split the four oversized emit.rs files — with byte-identical serialized output and byte-identical CLI help everywhere.

**Architecture:** `message-vault-io-core` becomes the shared home: `ExportReport::bump`, `run_pipeline`, `run_cli`, `ExporterConfig::prepare_outputs`, attachment naming/copy helpers, a feature-gated `cli` module with the shared `clap_command` scaffolding, and a feature-gated `testutil` module for the smoke-test scaffolding. `libs/media` gains `file_sha256` and `mime_for_ext`; `libs/csv` gains `col()`/`field()`. Each exporter keeps only source-specific conversion and flags. The four emit.rs files split into document-building / parsing / attachment modules.

**Tech Stack:** Rust workspace, clap (core `cli` feature), anyhow, rayon (sms-backup-plus), serde_json.

## Global Constraints

From `docs/superpowers/specs/2026-08-23-exporters-followup-design.md` — every task's requirements implicitly include this section:

- **Behavior-preserving** except the sanctioned media-flags wiring (Task 11): byte-identical serialized output (CSV, EML, JSON, XML), byte-identical CLI help text, identical error text everywhere.
- **Green after every task.** `cargo test --workspace` (67 targets), `cargo fmt --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` all clean after every task commit.
- **Docs gates.** `cargo doc --no-deps -p message-vault-io-core -p media -p message-csv` emit zero warnings from the tasks that gate them onward.
- **Generated artifacts.** `openapi.json` untouched. CLI reference pages byte-identical unless a task's own clap-visible change alters rendered help — in that case regenerate in the same task: `cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference`. The committed-dump tests must stay green. Any task touching core's `cli.rs` doc comments MUST run `cargo test -p dump-cli-docs committed_cli_pages_match_dump`.
- **Doc style.** `docs/src/content/docs/vault/developer/rustdoc-style.md` governs all doc text. No `#[allow(missing_docs)]`.
- **No new crates, no dependency version bumps.** `testutil`/`cli` modules live inside `message-vault-io-core` behind features. Feature additions on existing deps are allowed.
- **Line anchors.** Audit line numbers are context only; find items by name — the compiler and `cargo doc` are authoritative.

---

### Task 1: core documentation, quality fixes, and gate

Findings 13 (ExporterConfig fields), 14 (Form fields), 16 (`message-media`), 17 (`` [`crate`]-style ``), plus the approved scope addition: `#![warn(missing_docs)]` on `message-vault-io-core` and the full sweep the gate surfaces.

**Files:**
- Modify: `crates/core/message-vault-io-core/src/lib.rs`, `crates/core/message-vault-io-core/src/config.rs`, `crates/core/message-vault-io-core/src/exporters.rs`, `crates/core/message-vault-io-core/src/pipeline.rs`, `crates/core/message-vault-io-core/src/process.rs`, `crates/core/message-vault-io-core/src/cli.rs`
- Possibly regenerate (Step 4): `docs/src/content/docs/vault/developer/reference/*.md`

**Interfaces:**
- Produces: a documented, gated `message-vault-io-core`; corrected `--help`-adjacent doc text on `CommonCli.output`.
- Consumes: nothing new.

- [ ] **Step 1: Add the gate and document ExporterConfig**

Insert `#![warn(missing_docs)]` in `crates/core/message-vault-io-core/src/lib.rs` after the `//!` intro (before `#[cfg(feature = "cli")] mod cli;`).

In `crates/core/message-vault-io-core/src/config.rs`, add field docs to `ExporterConfig` (lines ~101-119) at the bare fields — first sentence states what the field is, later sentences point at the CLI flag:

- `output`: `/// Output directory the export is written to (packaging plus \`attachments/\`).\n/// Set from the CLI \`--output\` flag.`
- `date_range`: `/// Optional \`[start, end)\` message window (\`YYYY-MM-DD\`, local midnight).\n/// Set from the CLI \`--start-date\` / \`--end-date\` flags.`
- `contacts`: `/// Optional contacts file used to resolve phone numbers to names.\n/// Set from the CLI \`--contacts\` / \`--vcf\` flags.`
- `obfuscate`: `/// Fake-name rewrite settings; \`None\`-equivalent when disabled.\n/// Set from the CLI \`--obfuscate\` / \`--obfuscate-seed\` flags.`
- `cancel`: `/// Shared cancel flag for in-process jobs; CLI runs leave it unset.`
- `source`: `/// Exporter-specific options; exactly one variant is set per run.`

- [ ] **Step 2: Document the 31 bare Form fields**

In `crates/core/message-vault-io-core/src/exporters.rs` (Form, lines ~352-388), add one doc line per bare field (the GUI wire contract — describe what the field means, and which exporter(s) use it where the field is source-specific):

- `input`: `/// Primary input path (source backup file or directory).`
- `output`: `/// Output directory for the export.`
- `contacts`: `/// Contacts file path (CSV or VCF) for phone→name resolution.`
- `contacts_kind`: `/// How the contacts file is parsed.`
- `owner_phones`: `/// Comma-separated owner phone numbers (marks outgoing messages).`
- `owner_emails`: `/// Comma-separated owner email addresses (marks outgoing messages).`
- `name_mapping`: `/// Optional incorrect-name mapping file path.`
- `timezone`: `/// Optional fixed UTC offset (e.g. \`UTC-05:00\`) for naive timestamps.`
- `obfuscate`: `/// Whether to rewrite output with stable fake identities.`
- `obfuscate_seed`: `/// Optional hex seed for reproducible obfuscation.`
- `advanced`: `/// Whether the advanced section of the GUI form is shown.`
- `db_path`: `/// iMessage chat database path (Apple sources).`
- `attachment_root`: `/// Apple backup attachment root directory.`
- `start_date`: `/// Start-date filter (\`YYYY-MM-DD\`).`
- `end_date`: `/// End-date filter (\`YYYY-MM-DD\`, exclusive).`
- `conversation_filter`: `/// iMessage conversation filter (chat id).`
- `apple_contacts`: `/// macOS AddressBook path (Apple sources).`
- `backup_password`: `/// Apple backup decryption password (never written to \`export.ini\`).`
- `attachment_media`: `/// Attachment handling choice for the export.`
- `media_max_resolution`: `/// Compress-only long-edge cap (720p/1080p/4k).`
- `media_max_fps`: `/// Compress-only max frame rate.`
- `media_min_size`: `/// Compress-only minimum video size (e.g. \`20M\`).`
- `media_skip_efficient`: `/// Compress-only: skip already-efficient HEVC videos.`
- `apple_platform`: `/// iPhone vs Mac backup layout.`
- `whatsapp_platform`: `/// Android vs iOS WhatsApp layout.`
- `whatsapp_key`: `/// WhatsApp backup encryption key (never written to \`export.ini\`).`
- `whatsapp_backup`: `/// WhatsApp backup file path.`
- `whatsapp_wa`: `/// WhatsApp Web session/wa path.`
- `whatsapp_media`: `/// WhatsApp media folder path.`
- `whatsapp_db`: `/// WhatsApp message database path.`
- `whatsapp_business`: `/// Whether the backup is a WhatsApp Business backup.`

- [ ] **Step 3: Fix the two doc-quality targets and the sweep the gate names**

In `crates/core/message-vault-io-core/src/exporters.rs`:
- `AttachmentMedia::media_mode` (line ~262): replace `/// Matching \`message-media\` mode used by FormatSink.` with `/// The \`media::MediaMode\` this GUI choice maps to (the same mode the\n/// \`--media-mode\` CLI flag selects).`
- `OutputFormat::Json` variant doc (config.rs ~26): the doc links `docs/maintainers/architecture/message-ir.md` which does not exist — replace the parenthetical with `(default; see <https://bitrealm.io/vault/developer/architecture/common-message/>)`.
- `OutputFormat::is_sbr_xml` (config.rs ~84): the [`message_ir_format::FormatSink`] intra-doc link targets a crate this crate does not depend on — reword to plain text: `/// True when export writes a single SyncTech \`smses.xml\` (the FormatSink XML path).`

In `crates/core/message-vault-io-core/src/pipeline.rs`:
- `RunResult` (line ~52): replace the doc with `/// Result of a successful exporter \`run\`: human-readable log lines.` and add a field doc: `messages`: `/// Human-readable log lines (summary lines plus mid-run notes).`

Now run `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` and document every remaining item the gate names, using the style guide. Expect the list to include (write one-liners matching each item's semantics): `SourceConfig`'s 7 undocumented variants (GoSmsPro, SmsBackupRestore, SmsBackupPlus, OpenExtract, Imazing, Apple, Whatsapp), `Exporter`'s 8 variants, `ContactsKind`'s 3 variants, `AttachmentMedia`'s 4 variants, `WhatsappPlatform`'s 2 variants, `ApplePlatform`'s 3 variants, `ProcessEvent`'s 4 variants, and the bare consts `WHATSAPP_PLATFORMS`, `ATTACHMENT_MEDIA`, `MAX_RESOLUTIONS`, `APPLE_PLATFORMS` (e.g. `/// The WhatsApp platforms in GUI dropdown order.` / `/// The attachment-media choices in GUI dropdown order.` / `/// The video resolution choices for compress mode.` / `/// The Apple platform choices in GUI dropdown order.`).

In `crates/core/message-vault-io-core/src/cli.rs`:
- `CommonCli.output` doc ends `"packaging + attachments/"` — the trailing slash is a typo; change to `packaging + \`attachments/\`` (backticked, no trailing-slash ambiguity). This is clap-visible text — Step 4 checks the committed pages.

- [ ] **Step 4: Verify — including the committed CLI pages**

Run: `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` — expect zero lines (iterate until clean).
Run: `cargo test -p message-vault-io-core` — all pass.
Run: `cargo test -p dump-cli-docs committed_cli_pages_match_dump`
Expected: PASS if the `--output` help rendering is unchanged; if it FAILS **because of this task's `CommonCli.output` doc change**, regenerate in this same task:
`cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference` and add the changed page(s) to the commit.
Run: `cargo clippy -p message-vault-io-core -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/message-vault-io-core/src/lib.rs crates/core/message-vault-io-core/src/config.rs crates/core/message-vault-io-core/src/exporters.rs crates/core/message-vault-io-core/src/pipeline.rs crates/core/message-vault-io-core/src/process.rs crates/core/message-vault-io-core/src/cli.rs
git commit -m "docs(core): document the exporter config and form surfaces, gate the crate"
```

---

### Task 2: ExportReport::bump and its five adoptions

Findings 8 and the bump half of finding 1 (five byte-identical 3-line copies).

**Files:**
- Modify: `crates/core/message-vault-io-core/src/pipeline.rs`, `crates/exporters/go-sms-pro-exporter/src/emit.rs`, `crates/exporters/openextract-exporter/src/emit.rs`, `crates/exporters/whatsapp-exporter/src/emit.rs`, `crates/exporters/imazing-exporter/src/emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/emit.rs`

**Interfaces:**
- Produces: `message_vault_io_core::ExportReport::bump(key, by)`.
- Consumes: nothing new.

- [ ] **Step 1: Add the method to core**

In `crates/core/message-vault-io-core/src/pipeline.rs`, add to `impl ExportReport` (after `summary_lines`):

```rust
/// Bump a per-exporter extension counter in the `extra` map.
pub fn bump(&mut self, key: &str, by: u64) {
    *self.extra.entry(key.to_string()).or_insert(0) += by;
}
```

- [ ] **Step 2: Delete the five copies and re-point call sites**

In each of the five exporter emit.rs files, delete the private `fn bump(...)` (3 lines) and replace every call `bump(report, "...", n)` with `report.bump("...", n)` — the compiler names the call sites. The per-exporter `count(...)` test helpers (imazing, sms-backup-plus) stay as-is.

- [ ] **Step 3: Verify**

Run: `cargo test -p message-vault-io-core -p go-sms-pro-exporter -p openextract-exporter -p whatsapp-exporter -p imazing-exporter -p sms-backup-plus-exporter` — all pass.
Run: `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` — zero lines.
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/message-vault-io-core/src/pipeline.rs crates/exporters/go-sms-pro-exporter/src/emit.rs crates/exporters/openextract-exporter/src/emit.rs crates/exporters/whatsapp-exporter/src/emit.rs crates/exporters/imazing-exporter/src/emit.rs crates/exporters/sms-backup-plus-exporter/src/emit.rs
git commit -m "refactor(exporters): share ExportReport::bump across five exporters"
```

---

### Task 3: csv col()/field() and media file_sha256 / mime_for_ext

Findings 9 (64KB hasher ×3), 10 (col/field ×2), 12 (mime tables).

**Files:**
- Modify: `crates/libs/csv/src/lib.rs`, `crates/libs/media/src/lib.rs`, `crates/exporters/openextract-exporter/src/parse.rs`, `crates/exporters/imazing-exporter/src/parse.rs`, `crates/exporters/whatsapp-exporter/src/emit.rs`, `crates/exporters/imazing-exporter/src/attachments.rs`, `crates/libs/ir-format/src/export_transforms.rs`, `crates/exporters/go-sms-pro-exporter/src/emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/assets.rs`

**Interfaces:**
- Produces: `message_csv::{col, field}`; `media::{file_sha256, mime_for_ext}` (all documented — the csv/media gates are already on).
- Constraint: hash outputs and mime results byte-identical per call site (the shared mime table holds only the 6 common arms; each exporter keeps its extras via `or(match ...)`).

- [ ] **Step 1: col()/field() into message-csv**

In `crates/libs/csv/Cargo.toml` `[dependencies]`, add `csv = "1.4.0"` (the version ir-format uses — required for the `csv::StringRecord` parameter).

In `crates/libs/csv/src/lib.rs`, add (exact bodies, new docs):

```rust
/// Index of a required CSV header column.
///
/// # Errors
///
/// Returns an error naming the missing column and the headers found.
pub fn col(headers: &[String], name: &str) -> anyhow::Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .with_context(|| format!("missing column {name:?} (have {headers:?})"))
}

/// Trimmed value of one CSV cell (empty string when missing).
pub fn field(rec: &csv::StringRecord, idx: usize) -> String {
    rec.get(idx).unwrap_or("").trim().to_string()
}
```

In `openextract-exporter/src/parse.rs` and `imazing-exporter/src/parse.rs`: delete the local `col`/`field` copies and add `use message_csv::{col, field};` (keep existing call sites unchanged — they resolve through the import).

- [ ] **Step 2: file_sha256 into libs/media**

In `crates/libs/media/src/lib.rs`, add:

```rust
/// Stream a file through SHA-256 in 64 KB chunks (no full read into memory).
///
/// Returns 64 lowercase hex digits.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
pub fn file_sha256(path: &std::path::Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        use std::io::Read;
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
```

Check `crates/libs/media/Cargo.toml` already depends on `sha2` and `hex` (it does — its process code hashes media); if not, add them at the workspace-pinned versions. Add `pub use` of the fn in lib.rs's existing re-export style.

Replace the three copies:
- whatsapp emit.rs `file_sha256` (lines ~440-454): delete the local fn, calls become `media::file_sha256(...)` (the local fn's doc + `BufReader` layer disappear; hashing is identical).
- imazing attachments.rs `stream_sha256` (lines ~266-280): delete; calls become `media::file_sha256(...)`.
- ir-format export_transforms.rs `hash_file_sha256` (lines ~203-217): delete; calls become `media::file_sha256(...)` (ir-format already depends on media).

- [ ] **Step 3: mime_for_ext into libs/media (common arms only)**

In `crates/libs/media/src/lib.rs`, add:

```rust
/// MIME type for a common media file extension, if known.
///
/// Exporters that recognize extra extensions chain their own match after
/// this table (e.g. go-sms-pro's `.wav`, sms-backup-plus's `.webp`).
pub fn mime_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".gif" => Some("image/gif"),
        ".mp4" => Some("video/mp4"),
        ".3gp" => Some("video/3gpp"),
        ".amr" => Some("audio/amr"),
        _ => None,
    }
}
```

Adoptions (behavior byte-identical — each exporter's result for every extension is unchanged):
- go-sms-pro emit.rs: delete the local `mime_for_ext` (lines ~76-87); at its call sites use `media::mime_for_ext(ext).or(match ext { ".wav" => Some("audio/wav"), _ => None })` (or an equivalent local helper with that exact body).
- sms-backup-plus assets.rs: delete the local table (lines ~84-97); call sites use `media::mime_for_ext(ext).or(match ext { ".webp" => Some("image/webp"), ".mp3" => Some("audio/mpeg"), ".m4a" => Some("audio/mp4"), _ => None })`.

- [ ] **Step 4: Verify**

Run: `cargo test -p message-csv -p media -p openextract-exporter -p imazing-exporter -p whatsapp-exporter -p go-sms-pro-exporter -p sms-backup-plus-exporter -p message-ir-format` — all pass (ir-format's round-trip tests and the exporters' smoke tests pin the hashed/typed output).
Run: `cargo doc --no-deps -p message-csv -p media 2>&1 | grep -E "warning|error"` — zero lines.
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/libs/csv/src/lib.rs crates/libs/media/src/lib.rs crates/exporters/openextract-exporter/src/parse.rs crates/exporters/imazing-exporter/src/parse.rs crates/exporters/whatsapp-exporter/src/emit.rs crates/exporters/imazing-exporter/src/attachments.rs crates/libs/ir-format/src/export_transforms.rs crates/exporters/go-sms-pro-exporter/src/emit.rs crates/exporters/sms-backup-plus-exporter/src/assets.rs
git commit -m "refactor(exporters): share col/field, file_sha256, and mime_for_ext"
```

---

### Task 4: shared clap_command scaffolding

Findings 7 and 11 (clap_command + binary-name test duplicated in all 7 cli.rs files).

**Files:**
- Modify: `crates/core/message-vault-io-core/src/cli.rs`, `crates/core/message-vault-io-core/src/lib.rs`, all 7 exporters' `src/cli.rs` and `Cargo.toml`

**Interfaces:**
- Produces: `message_vault_io_core::cli::clap_command::<C>()` and the `message_vault_io_core::clap_command_uses_binary_name_test!` macro (both behind core's existing `cli` feature).
- Constraint: every binary's rendered `--help` is byte-identical (the factory calls `C::command()` exactly as today). The committed CLI pages must stay green.

- [ ] **Step 1: Add the factory and the test macro to core**

In `crates/core/message-vault-io-core/src/cli.rs`, add:

```rust
/// The clap `Command` for an exporter binary (for embedding `--help` output
/// into GUI docs).
pub fn clap_command<C: clap::CommandFactory>() -> clap::Command {
    C::command()
}

/// Declare the standard test that a crate's `clap_command()` reports its
/// binary name.
///
/// Usage: `message_vault_io_core::clap_command_uses_binary_name_test!("go-sms-pro-exporter");`
#[macro_export]
macro_rules! clap_command_uses_binary_name_test {
    ($bin:literal) => {
        #[cfg(test)]
        mod clap_command_tests {
            #[test]
            fn clap_command_uses_binary_name() {
                let cmd = crate::cli::clap_command();
                assert_eq!(cmd.get_name(), $bin);
            }
        }
    };
}
```

In `crates/core/message-vault-io-core/src/lib.rs`, add `pub use cli::clap_command;` to the existing `#[cfg(feature = "cli")] pub use cli::CommonCli;` block (adjusting the block to re-export both).

- [ ] **Step 2: Adopt in all 7 exporters**

For each exporter (go-sms-pro, imazing, openextract, whatsapp, sms-backup-restore, sms-backup-plus, imessage-ir):
1. In `src/cli.rs`, replace the `pub fn clap_command() -> Command { Cli::command() }` body with `message_vault_io_core::cli::clap_command::<Cli>()` (keep the fn's signature and any doc).
2. Replace the entire local `#[cfg(test)] mod clap_command_tests { ... }` with the macro invocation, e.g. `message_vault_io_core::clap_command_uses_binary_name_test!("go-sms-pro-exporter");` (using each crate's own asserted binary name from the table: go-sms-pro-exporter, imazing-exporter, openextract-exporter, whatsapp-exporter, sms-backup-restore-exporter, sms-backup-plus-exporter, imessage-ir-exporter).
3. In `Cargo.toml`, change the `message-vault-io-core` dependency to enable the feature: `message-vault-io-core = { path = "../../core/message-vault-io-core", features = ["cli"] }` (keep any other existing options on that dep line).

- [ ] **Step 3: Verify — including the committed CLI pages**

Run: `cargo test -p go-sms-pro-exporter -p imazing-exporter -p openextract-exporter -p whatsapp-exporter -p sms-backup-restore-exporter -p sms-backup-plus-exporter -p imessage-ir-exporter` — all pass (each `clap_command_uses_binary_name` test still asserts its own name).
Run: `cargo test -p dump-cli-docs committed_cli_pages_match_dump`
Expected: PASS — the rendered help is unchanged by construction. If it fails for any reason, stop and report BLOCKED (do not regenerate — a failure here means help text changed unexpectedly).
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/message-vault-io-core/src/cli.rs crates/core/message-vault-io-core/src/lib.rs crates/exporters/*/src/cli.rs crates/exporters/*/Cargo.toml
git commit -m "refactor(exporters): share clap_command scaffolding from core"
```

---

### Task 5: shared prepare_outputs preamble

Finding 3 (convert_export output-overlap preamble in 4 exporters).

**Files:**
- Modify: `crates/core/message-vault-io-core/src/pipeline.rs`, `crates/exporters/go-sms-pro-exporter/src/emit.rs`, `crates/exporters/imazing-exporter/src/emit.rs`, `crates/exporters/openextract-exporter/src/emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/emit.rs`

**Interfaces:**
- Produces: `message_vault_io_core::prepare_outputs(inputs: &[PathBuf], output: &Path) -> Result<(Vec<PathBuf>, PathBuf)>` (a free fn — the spec named it `ExporterConfig::prepare_outputs`, but every call site is inside `convert_export` where inputs/outputs are function args, not config fields; same contract).
- Constraint: the overlap bail text is byte-identical (`output {} must not be the same as, or contain, the input {}`). One accepted delta: go-sms-pro's bare `create_dir_all(...)?` gains the `with_context(|| format!("create {}", ...))` wrapper the other three already use — flagged for the PR notes; the failure mode is a disk-error path, and the message becomes more informative, matching 3 of 4 exporters.

- [ ] **Step 1: Add the helper to core**

In `crates/core/message-vault-io-core/src/pipeline.rs`, add:

```rust
/// Create and canonicalize the output directory, canonicalize every input,
/// and bail when the output is the same as, or contains, an input.
///
/// Returns the canonicalized `(inputs, output)` paths.
///
/// # Errors
///
/// Returns an error when the output directory cannot be created, a path
/// cannot be resolved, or the output overlaps an input.
pub fn prepare_outputs(
    inputs: &[std::path::PathBuf],
    output: &std::path::Path,
) -> anyhow::Result<(Vec<std::path::PathBuf>, std::path::PathBuf)> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    let output = fs::canonicalize(output)
        .with_context(|| format!("resolve {}", output.display()))?;
    let mut resolved = Vec::with_capacity(inputs.len());
    for input in inputs {
        let input =
            fs::canonicalize(input).with_context(|| format!("resolve {}", input.display()))?;
        if output == input || input.starts_with(&output) {
            bail!(
                "output {} must not be the same as, or contain, the input {}",
                output.display(),
                input.display()
            );
        }
        resolved.push(input);
    }
    Ok((resolved, output))
}
```

(Use the existing `fs`/`bail` imports in pipeline.rs — adjust the `use` lines only if the compiler requires.)

- [ ] **Step 2: Replace the four preambles**

In each exporter's `convert_export`, replace the create_dir_all + canonicalize + overlap-check block with a call to `prepare_outputs` and use the returned canonical paths for the rest of the function:
- go-sms-pro emit.rs (lines ~754-769): `let (inputs, output_dir) = prepare_outputs(&[input_dir.to_path_buf()], output_dir)?; let input_dir = &inputs[0];` — the fn body after the preamble continues with `output_dir` (now the canonicalized value) and `input_dir` (canonicalized); adapt the remaining uses compiler-guided, keeping identical values.
- imazing emit.rs (~97-109): same shape with its `input`/`output` names.
- openextract emit.rs (~69-83): same.
- sms-backup-plus emit.rs (~735-753): `let (inputs, output_dir) = prepare_outputs(&config.inputs... , output_dir)?;` — its loop over `inputs` is replaced by iterating the returned canonical `inputs` (the fn's later code consumes the same values).

Delete the now-redundant per-exporter comments with the moved blocks.

- [ ] **Step 3: Verify**

Run: `cargo test -p go-sms-pro-exporter -p imazing-exporter -p openextract-exporter -p sms-backup-plus-exporter -p message-vault-io-core` — all pass.
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/message-vault-io-core/src/pipeline.rs crates/exporters/go-sms-pro-exporter/src/emit.rs crates/exporters/imazing-exporter/src/emit.rs crates/exporters/openextract-exporter/src/emit.rs crates/exporters/sms-backup-plus-exporter/src/emit.rs
git commit -m "refactor(exporters): share the convert_export output-overlap preamble"
```

---

### Task 6: shared attachment naming and copy-if-missing helpers

Finding 2 (content-addressed naming + skip-if-exists copy in 4 exporters).

**Files:**
- Create: `crates/core/message-vault-io-core/src/attachments.rs`
- Modify: `crates/core/message-vault-io-core/src/lib.rs`, `crates/exporters/imazing-exporter/src/attachments.rs`, `crates/exporters/imessage-ir-exporter/src/emit.rs`, `crates/exporters/go-sms-pro-exporter/src/emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/assets.rs`, `crates/exporters/sms-backup-plus-exporter/src/emit.rs`

**Interfaces:**
- Produces: `message_vault_io_core::attachments::{digest_prefix, attachment_dest_name, write_if_missing, copy_if_missing}` (new public module, documented — core's gate is on).
- Constraint: every emitted filename and every write/rename decision is byte-identical per exporter. imessage-ir's size-checked tmp+rename `persist_attachment` keeps its bespoke logic (different semantics) and adopts only `digest_prefix`/`attachment_dest_name`.

- [ ] **Step 1: Create the core attachments module**

Check `crates/core/message-vault-io-core/Cargo.toml` for a direct `chrono` dependency; if absent, add `chrono = "0.4.44"` (the workspace-pinned version) to `[dependencies]`.

Create `crates/core/message-vault-io-core/src/attachments.rs`:

```rust
//! Shared content-addressed attachment naming and idempotent file writes.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use std::path::Path;

/// First 16 hex digits of a SHA-256 digest (content-addressed path prefix).
pub fn digest_prefix(digest_hex: &str) -> &str {
    &digest_hex[..16.min(digest_hex.len())]
}

fn date_prefix(timestamp_secs: i64) -> String {
    Local
        .timestamp_opt(timestamp_secs, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| timestamp_secs.to_string())
}

/// Content-addressed attachment filename: `{local-date}-{digest16}{ext}`.
pub fn attachment_dest_name(timestamp_secs: i64, digest_hex: &str, ext: &str) -> String {
    format!("{}-{}{}", date_prefix(timestamp_secs), digest_prefix(digest_hex), ext)
}

/// Write `bytes` to `path` only when the file does not exist.
///
/// Returns `true` when the file was written.
///
/// # Errors
///
/// Returns an error when the write fails.
pub fn write_if_missing(path: &Path, bytes: &[u8]) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Copy `src` to `dest` only when `dest` does not exist.
///
/// Returns `true` when the copy happened.
///
/// # Errors
///
/// Returns an error when the copy fails.
pub fn copy_if_missing(src: &Path, dest: &Path) -> Result<bool> {
    if dest.exists() {
        return Ok(false);
    }
    std::fs::copy(src, dest).with_context(|| format!("copy {} to {}", src.display(), dest.display()))?;
    Ok(true)
}
```

In `crates/core/message-vault-io-core/src/lib.rs`: add `mod attachments;` and `pub use attachments::{attachment_dest_name, copy_if_missing, digest_prefix, write_if_missing};`.

- [ ] **Step 2: Adopt in the four exporters**

- **imazing** attachments.rs (lines ~243-262): replace the inline `digest_prefix`/`date_prefix`/`name`/`exists-copy` block with `let digest_prefix = digest_prefix(&digest_hex);` … `let name = attachment_dest_name(message_secs, &digest_hex, &ext);` … `if copy_if_missing(&src, &dest)? { *attachments_saved += 1; }` — same ext computation, same date fallback, same counter.
- **imessage-ir** emit.rs: `attachment_dest_name` (lines ~448-466) becomes a thin wrapper: compute `secs`/`ext` exactly as today, then return `message_vault_io_core::attachments::attachment_dest_name(secs, digest_hex, &ext)` (delete the local date_prefix/digest_prefix code). `persist_attachment` keeps its size-check + tmp/rename exactly as-is, using the wrapper's name.
- **go-sms-pro** emit.rs (lines ~238-253): use `digest_prefix(&digest_hex)` for its `{date}-I_{ts}_{digest16}_{idx}{ext}` name (name format unchanged), and replace the `if !path.exists() { fs::write(...)?; report.attachments_saved += 1; }` with `if write_if_missing(&path, &att.data)? { report.attachments_saved += 1; }`.
- **sms-backup-plus** assets.rs (lines ~151-162): use `digest_prefix(&digest_hex)` in its unchanged name format. In emit.rs `write_attachments` (lines ~65-87), keep the `copy_attachments` gate and the per-failure `report.errors` + `continue` behavior; replace the inner exists-check/write with `match write_if_missing(&path, &blob.data) { Ok(true) => {} Ok(false) => {} Err(err) => { report.errors.push(format!("{}", err)); continue; } }` — record the same error text `format!("{}", err)` that `fs::write` would have produced via `?`.

- [ ] **Step 3: Verify**

Run: `cargo test -p imazing-exporter -p imessage-ir-exporter -p go-sms-pro-exporter -p sms-backup-plus-exporter -p message-vault-io-core` — all pass (attachment-naming and copy behavior are pinned by the exporter tests).
Run: `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` — zero lines.
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/message-vault-io-core/src/attachments.rs crates/core/message-vault-io-core/src/lib.rs crates/exporters/imazing-exporter/src/attachments.rs crates/exporters/imessage-ir-exporter/src/emit.rs crates/exporters/go-sms-pro-exporter/src/emit.rs crates/exporters/sms-backup-plus-exporter/src/assets.rs crates/exporters/sms-backup-plus-exporter/src/emit.rs
git commit -m "refactor(exporters): share attachment naming and copy-if-missing helpers"
```

---

### Task 7: share prepare_conversation and pending_to_document skeletons

Finding 1's remaining half (prepare_conversation ×5, pending_to_document ×5).

**Files:**
- Modify: `crates/core/message-vault-io-core/src/pipeline.rs`, `crates/exporters/go-sms-pro-exporter/src/emit.rs`, `crates/exporters/openextract-exporter/src/emit.rs`, `crates/exporters/whatsapp-exporter/src/emit.rs`, `crates/exporters/imazing-exporter/src/emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/emit.rs`

**Interfaces:**
- Produces: `message_vault_io_core::{prune_and_finish_conversation, export_meta}`.
- Constraint: identical per-exporter behavior — go-sms-pro keeps its dedupe-first order, whatsapp keeps its `key_id` tie-break sort and ms→secs conversion, the other three keep their sort_by_key.

- [ ] **Step 1: Add the shared conversation tail**

In `crates/core/message-vault-io-core/src/pipeline.rs`, add:

```rust
/// Drop messages with unrepresentable timestamps and finalize a pending
/// conversation. Returns whether any message remains.
///
/// `to_secs` converts a message sort key to Unix seconds (exporters that
/// store milliseconds pass `|k| k / 1000`).
pub fn prune_and_finish_conversation(
    convo: &mut PendingConversation,
    report: &mut ExportReport,
    to_secs: impl Fn(i64) -> i64,
) -> bool {
    convo.messages.retain(|m| {
        if message_csv::format_local_ts(to_secs(m.sort_key)).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    convo.has_attachments = convo.messages.iter().any(|m| !m.attachments.is_empty());
    !convo.messages.is_empty()
}
```

(Imports: `message_ir::PendingConversation`, `message_csv::format_local_ts` — core already depends on both.)

- [ ] **Step 2: Adopt in the five prepare_conversation copies**

- **openextract / imazing / sms-backup-plus** (identical bodies): keep the empty-check early return and the `convo.messages.sort_by_key(|m| m.sort_key);` line; replace the retain/has_attachments/final-bool tail with `message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k)`.
- **whatsapp**: keep its `key_id` tie-break sort; replace the tail with `message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k / 1000)`.
- **go-sms-pro**: keep `dedupe_messages(&mut convo.messages);` first (no sort — preserve that order); replace the tail with `message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k)`.

- [ ] **Step 3: Share the ExportMeta construction in pending_to_document**

Add to core `pipeline.rs`:

```rust
/// Standard export metadata from a pending conversation's provenance.
pub fn export_meta(source: &str, tool: &str, tool_version: &str, owner: &message_ir::ExportMeta) -> message_ir::ExportMeta {
    message_ir::ExportMeta {
        source: source.to_string(),
        tool: tool.to_string(),
        tool_version: tool_version.to_string(),
        owner_handle: owner.owner_handle.clone(),
        owner_display_name: owner.owner_display_name.clone(),
    }
}
```

Then in each exporter's `pending_to_document`, read its current `ExportMeta { ... }` construction (all five build `source: EXPORT_SOURCE`, `tool: EXPORT_TOOL`, `tool_version: EXPORT_TOOL_VERSION` plus owner fields; two use `owner_sender`). Replace the construction with `let export = message_vault_io_core::export_meta(EXPORT_SOURCE, EXPORT_TOOL, EXPORT_TOOL_VERSION, &owner_meta);` — where `owner_meta` is whatever the site currently derives the owner fields from (e.g. a locally built `ExportMeta` from `owner_sender(owner_export)` — read the code; the implementer preserves the exact owner values the site produced before). If a site's owner derivation does not fit the helper (per-exporter differences), keep that site's construction local and note it in the report — the helper exists for the common shape; the compiler and tests are authoritative for value identity.

- [ ] **Step 4: Verify**

Run: `cargo test -p go-sms-pro-exporter -p openextract-exporter -p whatsapp-exporter -p imazing-exporter -p sms-backup-plus-exporter -p message-vault-io-core` — all pass (per-exporter unit tests pin sort/dedupe/drop behavior and document shapes).
Run: `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` — zero lines.
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/message-vault-io-core/src/pipeline.rs crates/exporters/go-sms-pro-exporter/src/emit.rs crates/exporters/openextract-exporter/src/emit.rs crates/exporters/whatsapp-exporter/src/emit.rs crates/exporters/imazing-exporter/src/emit.rs crates/exporters/sms-backup-plus-exporter/src/emit.rs
git commit -m "refactor(exporters): share conversation pruning and export metadata"
```

---

### Task 8: shared run_pipeline and finish_run

Finding 4 (run() skeleton in 5 exporters).

**Files:**
- Modify: `crates/core/message-vault-io-core/src/pipeline.rs`, `crates/exporters/go-sms-pro-exporter/src/run.rs`, `crates/exporters/openextract-exporter/src/run.rs`, `crates/exporters/sms-backup-restore-exporter/src/run.rs`, `crates/exporters/imazing-exporter/src/run.rs`, `crates/exporters/whatsapp-exporter/src/run.rs`

**Interfaces:**
- Produces: `message_vault_io_core::{run_pipeline, finish_run}`.
- Constraint: identical log lines, error texts (including `media processing failed for all candidate files`), and summary lines per exporter.

- [ ] **Step 1: Add run_pipeline and finish_run to core**

In `crates/core/message-vault-io-core/src/pipeline.rs`, add:

```rust
/// The shared exporter run skeleton: cancel check, contacts resolution,
/// transforms, conversion, media-failure bail, and result assembly.
///
/// `load_contacts` resolves the contacts book (exporters with custom
/// loading pass their own closure); `convert` runs the source-specific
/// conversion and returns the report and finished sink.
///
/// # Errors
///
/// Returns an error when the user cancels, contacts cannot be loaded,
/// conversion fails, or media processing fails for every candidate file.
pub fn run_pipeline(
    config: &ExporterConfig,
    load_contacts: impl FnOnce(&ExporterConfig, &dyn Fn(&str)) -> anyhow::Result<ContactsBook>,
    convert: impl FnOnce(&ContactsBook, message_ir_format::ExportTransforms) -> anyhow::Result<(
        ExportReport,
        message_ir_format::FormatSinkResult,
    )>,
) -> anyhow::Result<RunResult> {
    check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let log_fn = |line: &str| config.emit_log(line);
    let contacts = load_contacts(config, &log_fn)?;
    let mut transforms = message_ir_format::ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert(&contacts, transforms)?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());
    report.summary_lines(&config.output, &mut messages);
    Ok(RunResult { messages })
}

/// The run tail shared by exporters whose middle diverges (WhatsApp):
/// media-failure bail plus log-line and summary assembly.
///
/// # Errors
///
/// Returns an error when media processing fails for every candidate file.
pub fn finish_run(
    config: &ExporterConfig,
    report: &ExportReport,
    sink: &message_ir_format::FormatSinkResult,
    needs_tools: bool,
) -> anyhow::Result<RunResult> {
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && needs_tools {
        anyhow::bail!("media processing failed for all candidate files");
    }
    let mut messages = sink.log_lines();
    report.summary_lines(&config.output, &mut messages);
    Ok(RunResult { messages })
}
```

(Core does NOT yet depend on `message-ir-format` — add `message-ir-format = { path = "../../libs/ir-format" }` to core's `[dependencies]`. Import paths: use the existing `use` style.)

- [ ] **Step 2: Adopt in the four near-identical run()s**

- **go-sms-pro** run.rs: keep the source-variant check (`bail!("go-sms-pro-exporter requires SourceConfig::GoSmsPro")`) and `require_input`; the rest of the body becomes:

```rust
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let source = match &config.source {
        SourceConfig::GoSmsPro(s) => s,
        _ => unreachable!(),
    };
    message_vault_io_core::run_pipeline(
        config,
        |config, log_fn| {
            let (contacts_path, vcf) = config.contacts_csv_vcf();
            resolve_contacts_cli(contacts_path, vcf, Some(log_fn)).map(|(b, _)| b)
        },
        |contacts, transforms| {
            convert_export(ConvertExportArgs {
                input_dir: input,
                output_dir: &config.output,
                owner_phones: &source.owner_phones,
                contacts,
                date_range: &config.date_range,
                transforms,
                output_format: config.output_format,
                cancel: config.cancel.as_ref(),
            })
        },
    )
```

(Adapt the source destructure so `source` lives long enough for the closure; preserve the exact bail text in the guard.)

- **openextract**: same pattern with its `OpenExtract` guard text, its `book` arg name, and its ConvertExportArgs (no owner_phones).
- **sms-backup-restore**: same pattern with its guard text and its args (owner_phones from its bound `source`).
- **imazing**: keep its guard text and its custom contacts loading (the CSV-or-VCF match with its `bail!("contacts config must be CSV or VCF, not both")` and the neither-case warning push) inside the `load_contacts` closure; its `convert` closure passes its own args (input, book, timezone, …).

- [ ] **Step 3: Adopt the tail in whatsapp**

In whatsapp run.rs, keep its entire middle (platform mapping, input resolution, wtsexporter execution, convert_json call) exactly as-is; replace only the final block (media-failure bail + `messages.extend(sink.log_lines())` + `report.summary_lines(...)` + `Ok(RunResult { messages })`) with:

```rust
    message_vault_io_core::finish_run(config, &report, &sink, needs_media_tools)
```

using the `needs_media_tools` value its current bail already checks (read the current code — the flag was captured before the tail).

- [ ] **Step 4: Verify**

Run: `cargo test -p go-sms-pro-exporter -p openextract-exporter -p sms-backup-restore-exporter -p imazing-exporter -p whatsapp-exporter -p message-vault-io-core` — all pass.
Run: `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` — zero lines.
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/message-vault-io-core/src/pipeline.rs crates/exporters/go-sms-pro-exporter/src/run.rs crates/exporters/openextract-exporter/src/run.rs crates/exporters/sms-backup-restore-exporter/src/run.rs crates/exporters/imazing-exporter/src/run.rs crates/exporters/whatsapp-exporter/src/run.rs
git commit -m "refactor(exporters): share the run pipeline skeleton"
```

---

### Task 9: shared run_cli driver

Finding 5 (main() driver in 6 of 7 binaries).

**Files:**
- Modify: `crates/core/message-vault-io-core/src/cli.rs`, `crates/exporters/go-sms-pro-exporter/src/main.rs`, `crates/exporters/imazing-exporter/src/main.rs`, `crates/exporters/openextract-exporter/src/main.rs`, `crates/exporters/sms-backup-restore-exporter/src/main.rs`, `crates/exporters/sms-backup-plus-exporter/src/main.rs`, `crates/exporters/whatsapp-exporter/src/main.rs`

**Interfaces:**
- Produces: `message_vault_io_core::cli::run_cli` (behind the `cli` feature).
- Constraint: identical CLI behavior and stdout/stderr split per binary; imessage-ir's divergent main is untouched by this task.

- [ ] **Step 1: Add run_cli to core**

In `crates/core/message-vault-io-core/src/cli.rs`, add:

```rust
/// The shared exporter main: parse the common CLI flags, build the source
/// config, run, and print the result with the standard stdout/stderr split.
///
/// `parse_dates` supplies the exporter's date parsing (local or
/// timezone-aware); `build` builds the exporter's `ExporterConfig` from the
/// parsed common values; `run` is the exporter's run function.
///
/// # Errors
///
/// Returns an error when a flag value cannot be parsed or the run fails.
pub fn run_cli(
    common: &CommonCli,
    parse_dates: impl FnOnce(&CommonCli) -> Result<DateRange, String>,
    build: impl FnOnce(DateRange, OutputFormat, CompressOptions) -> ExporterConfig,
    run: impl FnOnce(&ExporterConfig) -> anyhow::Result<RunResult>,
) -> anyhow::Result<()> {
    let date_range = parse_dates(common).map_err(anyhow::Error::msg)?;
    let output_format = OutputFormat::parse(&common.format).map_err(anyhow::Error::msg)?;
    let compress = compress_options_from_cli(
        common.media_max_resolution,
        common.media_max_fps,
        &common.media_min_size,
        common.media_skip_efficient,
    )?;
    let result = run(&build(date_range, output_format, compress))?;
    print_result(&result);
    Ok(())
}
```

(Imports: `CompressOptions`/`OutputFormat`/`RunResult` from the crate's own modules, `media::compress_options_from_cli` — core's cli.rs already imports what CommonCli needs; the compiler is authoritative.)

- [ ] **Step 2: Adopt in the six mains**

Each `main()` keeps `Cli::parse()` and becomes a single `message_vault_io_core::run_cli(...)` call whose closures carry exactly what the current body computed:
- **go-sms-pro**: `parse_dates` = the current `parse_date_range(common.start_date.as_deref(), common.end_date.as_deref())` (as a closure over `common` — note `parse_date_range` is a free fn; pass `|c| parse_date_range(c.start_date.as_deref(), c.end_date.as_deref())`); `build` = the current `ExporterConfig { inputs: vec![cli.input], ..., source: SourceConfig::GoSmsPro(...) }` literal; `run` = `go_sms_pro_exporter::run`.
- **imazing**: `parse_dates` uses `parse_date_range_tz` with `cli.timezone.as_deref()`; `build` sets `timezone: cli.timezone.clone()` and `SourceConfig::Imazing(ImazingConfig {})`.
- **openextract / sms-backup-restore**: same pattern with their config literals.
- **whatsapp**: `build` sets `inputs: cli.input.into_iter().collect()` and its 8-field `WhatsappConfig`.
- **sms-backup-plus**: the whole body is already inside `match cli.command { Commands::Convert { input, owner_phones, owner_emails, name_mapping, common } => { ... } }` — replace the body of that arm with the `run_cli` call; the build closure captures the destructured fields (`input`, `owner_phones`, …) and sets `SmsBackupPlusConfig { owner_phones, owner_emails, name_mapping, verbose: cli.verbose, include_summary: !cli.no_summary }`.

Remove now-unused imports the compiler flags (e.g. `compress_options_from_cli`, `OutputFormat`, `parse_date_range`) from each main.rs.

- [ ] **Step 3: Verify — including the committed CLI pages**

Run: `cargo test -p go-sms-pro-exporter -p imazing-exporter -p openextract-exporter -p sms-backup-restore-exporter -p sms-backup-plus-exporter -p whatsapp-exporter` — all pass.
Run: `cargo test -p dump-cli-docs committed_cli_pages_match_dump` — PASS (no clap-visible change in this task; if it fails, stop and report BLOCKED).
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/message-vault-io-core/src/cli.rs crates/exporters/go-sms-pro-exporter/src/main.rs crates/exporters/imazing-exporter/src/main.rs crates/exporters/openextract-exporter/src/main.rs crates/exporters/sms-backup-restore-exporter/src/main.rs crates/exporters/sms-backup-plus-exporter/src/main.rs crates/exporters/whatsapp-exporter/src/main.rs
git commit -m "refactor(exporters): share the CLI main driver"
```

---

### Task 10: shared convert_smoke scaffolding

Finding 6 (convert_smoke test scaffolding duplicated across exporters).

**Files:**
- Create: `crates/core/message-vault-io-core/src/testutil.rs`
- Modify: `crates/core/message-vault-io-core/Cargo.toml`, `crates/core/message-vault-io-core/src/lib.rs`, `crates/exporters/go-sms-pro-exporter/tests/convert_smoke.rs`, `crates/exporters/sms-backup-restore-exporter/tests/convert_smoke.rs`, `crates/exporters/sms-backup-plus-exporter/tests/convert_smoke.rs`, and the other exporters' `tests/convert_smoke.rs` files where they use `empty_contacts` (imazing, openextract, whatsapp, imessage-ir — check each; the pattern is `fn empty_contacts(dir: &tempfile::TempDir) -> ContactsBook` writing a header CSV), plus each exporter's `Cargo.toml` dev-dependencies

**Interfaces:**
- Produces: `message_vault_io_core::testutil::{empty_contacts, csv_files, assert_csv_header}` behind a `testutil` feature (core gains `testutil = ["dep:tempfile"]` with an optional `tempfile` dep at the workspace-pinned version).
- Constraint: each smoke test asserts exactly what it asserts today (header substrings and body substrings passed as parameters); the per-exporter `convert` wrapper stays local (its args differ per crate).

- [ ] **Step 1: Create the testutil module**

Create `crates/core/message-vault-io-core/src/testutil.rs`:

```rust
//! Shared scaffolding for exporter `convert_smoke` tests (behind `testutil`).

use contacts::ContactsBook;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// An empty contacts CSV book in a temp dir (header-only, no rows).
pub fn empty_contacts(dir: &tempfile::TempDir) -> ContactsBook {
    let path = dir.path().join("contacts.csv");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(f, "First Name,Last Name,Mobile Phone").unwrap();
    ContactsBook::load_vcard_csv(&path).unwrap()
}

/// Sorted `.csv` paths under `root` (the smoke-test file collection block).
pub fn csv_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .collect();
    files.sort();
    files
}

/// Assert that the first CSV under `root` has every `contains` header
/// column, none of the `not_contains` columns, and that the file body
/// contains `body_contains`; also assert no `.meta.json` files remain.
pub fn assert_csv_header(
    root: &Path,
    contains: &[&str],
    not_contains: &[&str],
    body_contains: &str,
) {
    let files = csv_files(root);
    assert!(!files.is_empty(), "expected at least one .csv");
    let json_count = fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("json")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".meta.json"))
        })
        .count();
    assert_eq!(json_count, 0);
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut fs::File::open(&files[0]).unwrap(), &mut contents).unwrap();
    let header = contents.lines().next().unwrap();
    for col in contains {
        assert!(header.contains(col), "header missing {col:?}");
    }
    for col in not_contains {
        assert!(!header.contains(col), "header unexpectedly has {col:?}");
    }
    assert!(contents.contains(body_contains));
}
```

Wire it: in core's `Cargo.toml` add `tempfile = { version = "3", optional = true }` and `contacts = { path = "../../libs/contacts", optional = true }` to `[dependencies]` (match the tempfile version core's dev-dependencies already use — read the file; `contacts` is required for `ContactsBook`) and `testutil = ["dep:tempfile", "dep:contacts"]` to `[features]`; in `lib.rs` add `#[cfg(feature = "testutil")] pub mod testutil;`.

- [ ] **Step 2: Adopt in the smoke tests**

For each exporter's `tests/convert_smoke.rs` that has an `empty_contacts` copy: delete the local fn and call `message_vault_io_core::testutil::empty_contacts(dir)`; add `message-vault-io-core = { path = "../../core/message-vault-io-core", features = ["testutil"] }` to that exporter's `[dev-dependencies]` (keeping any existing core dev-dep entry's other options).

- **sms-backup-restore** convert_smoke.rs: replace its csv-count/`.meta.json` block and header-assert block with `assert_csv_header(tmp.path(), &["chat_identifier", "export_source", "export_tool", "export_tool_version", "message_kind", "timestamp_unix_ms", "source_fields_json", "owner_handle", "participants_json", "subject"], &["date_ms", "contact_name", "xml_fields_json"], "sms-backup-restore")` — the exact current substrings, same order.
- **sms-backup-plus** convert_smoke.rs: same with its lists (`["chat_identifier", "attachments_json", "export_source", "export_tool", "export_tool_version", "timestamp_unix_ms", "android_type", "source_fields_json", "owner_handle", "participants_json", "read_receipt", "tapbacks_json"]`, `["date_ms", "contact_name", "xml_fields_json"]`, `"sms-backup-plus"`).
- **go-sms-pro** convert_smoke.rs: replace its shorter header block with `assert_csv_header(tmp.path(), &["chat_identifier", "direction", "attachments_json"], &["export_schema"], "go-sms-pro")` (matching its current asserts — read the current block and mirror exactly).
- Other exporters that only duplicate `empty_contacts`: swap in the shared fn; leave their other assertions as-is.

- [ ] **Step 3: Verify**

Run: `cargo test -p go-sms-pro-exporter -p sms-backup-restore-exporter -p sms-backup-plus-exporter -p imazing-exporter -p openextract-exporter -p whatsapp-exporter -p imessage-ir-exporter` — all pass.
Run: `cargo doc --no-deps -p message-vault-io-core 2>&1 | grep -E "warning|error"` — zero lines (testutil docs included).
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/message-vault-io-core/src/testutil.rs crates/core/message-vault-io-core/Cargo.toml crates/core/message-vault-io-core/src/lib.rs crates/exporters/*/tests/convert_smoke.rs crates/exporters/*/Cargo.toml
git commit -m "refactor(exporters): share convert_smoke scaffolding via core testutil"
```

---

### Task 11: wire imessage-ir's media flags

Finding 18 (silently dropped `--media-mode` and the other media flags). User-approved ruling: wire the flags.

**Files:**
- Modify: `crates/exporters/imessage-ir-exporter/src/main.rs`, `crates/exporters/imessage-ir-exporter/src/cli.rs` (check whether its `Cli` already flattens `CommonCli` — it does; no change expected there)

**Interfaces:**
- Consumes: `media::compress_options_from_cli` (same call as go-sms-pro's main).
- Constraint: default behavior unchanged (`--media-mode clone` default ⇒ same output); passing convert/compress now actually converts.

- [ ] **Step 1: Build MediaConfig from the common flags**

In `crates/exporters/imessage-ir-exporter/src/main.rs`, replace `media: MediaConfig::default(),` (line ~38) with:

```rust
        media: MediaConfig {
            mode: common.media_mode,
            compress: media::compress_options_from_cli(
                common.media_max_resolution,
                common.media_max_fps,
                &common.media_min_size,
                common.media_skip_efficient,
            )?,
        },
```

Add the `use media::compress_options_from_cli;` import (or use the full path as above and drop the import need).

- [ ] **Step 2: Add a wiring test**

In `crates/exporters/imessage-ir-exporter/src/main.rs`'s crate (or a `#[cfg(test)]` unit test in the lib if main.rs has no test slot — put the test in `src/cli.rs`'s test module), add:

```rust
#[test]
fn media_flags_reach_the_config() {
    use clap::Parser;
    let cli = Cli::parse_from(["imessage-ir-exporter", "--media-mode", "convert", "--media-max-resolution", "720p"]);
    let config = build_config_from_cli(&cli); // the exact build expression from main, extracted
    assert_eq!(config.media.mode, MediaMode::Convert);
    assert_eq!(config.media.compress.max_resolution, MaxResolution::P720);
}
```

To make this testable, extract the config-building expression from `main` into a small `pub(crate) fn build_config_from_cli(cli: &Cli) -> anyhow::Result<ExporterConfig>` used by both `main` and the test (a pure refactor of the same expression — the test asserts the wiring, no ffmpeg needed). If `parse_from` panics on missing required args, pass the minimal required args the Cli declares (read its `#[arg]` attributes and supply dummy values).

- [ ] **Step 3: Verify**

Run: `cargo test -p imessage-ir-exporter` — all pass including the new wiring test.
Run: `cargo test -p dump-cli-docs committed_cli_pages_match_dump` — PASS (no clap-visible change).
Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 4: Commit**

```bash
git add crates/exporters/imessage-ir-exporter/src/main.rs crates/exporters/imessage-ir-exporter/src/cli.rs
git commit -m "fix(imessage-ir): wire the shared media flags into MediaConfig"
```

---

### Task 12: imazing helper docs

Finding 15 (imazing emit.rs private helpers lack docs).

**Files:**
- Modify: `crates/exporters/imazing-exporter/src/emit.rs`

- [ ] **Step 1: Add the docs**

In `crates/exporters/imazing-exporter/src/emit.rs`, add doc comments (above each fn):
- `resolve_tz` (line ~422): `/// Parse a timezone string into local time or a fixed UTC offset.`
- `parse_message_date` (line ~432): `/// Parse an iMazing date string into \`(unix_secs, date_ms)\`; DST-ambiguous\n/// times resolve to the earliest occurrence.`
- `is_outgoing` (line ~457): `/// True for rows the exporter treats as sent (\`outgoing\`/\`sent\` types).`
- `resolve_chat_identifier` (line ~497): replace its existing `/// Returns (chat_identifier, contact_name, unresolved_phone).` with `/// Resolve a session into \`(chat_identifier, contact_name, unresolved_phone)\`.\n///\n/// The third value is \`true\` only when the chat id could not be resolved\n/// and callers should record the raw phone as unresolved.`

(Read each fn first; adjust the one-liners to match the actual match arms — the texts above are required content, and if a detail differs, keep the required claims accurate.)

- [ ] **Step 2: Verify**

Run: `cargo test -p imazing-exporter` — all pass. `cargo fmt --check` — clean (docs only; clippy optional).

- [ ] **Step 3: Commit**

```bash
git add crates/exporters/imazing-exporter/src/emit.rs
git commit -m "docs(imazing): document the private emit helpers"
```

---

### Task 13: split imazing-exporter emit.rs

Finding 19 (1 of 4). Move the parsing cluster and the thin attachment mappers out of `emit.rs`.

**Files:**
- Create: `crates/exporters/imazing-exporter/src/parse_emit.rs` (new name to avoid colliding with the existing `crate::parse` module — use `parse_emit.rs`), `crates/exporters/imazing-exporter/src/attachments_emit.rs`
- Modify: `crates/exporters/imazing-exporter/src/emit.rs`, `crates/exporters/imazing-exporter/src/lib.rs`

**Moves (verbatim, visibility `pub(super)` where the compiler requires):**
- → `parse_emit.rs`: `TransportFamily::from_kind`, `collect_peer_info`, `resolve_tz`, `parse_message_date`, `is_outgoing`, `is_notification`, `phones_in_text`, `resolve_chat_identifier`, `resolve_sender`, plus types `PeerInfo`, `TzMode`.
- → `attachments_emit.rs`: `attachment_guid_materials`, `pending_attachment_to_ir`.
- Stays in `emit.rs`: `convert_export`, `prepare_conversation`, `pending_to_document`, `first_contact_name`, `handle_type_for`, `imazing_peers`, `imazing_packaging_stem_suffix`, `TransportFamily` (+ `key_prefix`), `bump`, `count`, `ConvertExportArgs`, the consts, and the test module.

Declare the two new modules in `lib.rs` (`mod parse_emit; mod attachments_emit;`). Add `use super::...` imports in each new file as the compiler names them. Behavior-neutral move — no body changes, no signature changes.

**Verify:** `cargo test -p imazing-exporter` (all pass, including the 13 emit tests via `use super::*`), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`. **Commit:** `refactor(imazing): split emit.rs parsing and attachment helpers into modules`.

---

### Task 14: split imessage-ir-exporter emit.rs

Finding 19 (2 of 4). Move the attachment cluster out of `emit.rs`.

**Files:**
- Create: `crates/exporters/imessage-ir-exporter/src/attachments_emit.rs`
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs`, `crates/exporters/imessage-ir-exporter/src/lib.rs`

**Moves (verbatim, `pub(super)` where required):** `attachment_dest_name`, `persist_attachment`, `try_handwriting_svg`, `remap_part_attachment_indices`, `collect_mail_parts_and_attachments`.
**Stays:** everything else, including the test module.

Declare `mod attachments_emit;` in `lib.rs` (next to the existing `attachments` module). Behavior-neutral move.

**Verify:** `cargo test -p imessage-ir-exporter` (all pass), clippy `-D warnings`, fmt. **Commit:** `refactor(imessage-ir): split emit.rs attachment handling into a module`.

---

### Task 15: split go-sms-pro-exporter emit.rs

Finding 19 (3 of 4). Move the chat-id cluster and the attachment cluster out of `emit.rs`.

**Files:**
- Create: `crates/exporters/go-sms-pro-exporter/src/chat_id.rs`, `crates/exporters/go-sms-pro-exporter/src/attachments_emit.rs`
- Modify: `crates/exporters/go-sms-pro-exporter/src/emit.rs`, `crates/exporters/go-sms-pro-exporter/src/lib.rs`

**Moves (verbatim, `pub(super)` where required):**
- → `chat_id.rs`: `guarded_phone`, `join_guarded_phones`, `group_id_slug`, `chat_id_individual`, `chat_id_group`.
- → `attachments_emit.rs`: `mime_for_ext` (the Task 3 wrapper form), `save_pdu_attachments`, `pending_attachment_to_ir`.
- **Stays:** `convert_export`, `ensure_convo`, `add_xml_messages`, `add_pdu_message`, `dedupe_base_key`, `dedupe_messages`, `prepare_conversation`, `pending_to_document`, `enrich_pending_names`, `display_names_for_handles`, `first_contact_name`, `is_empty_pdu`, `pdu_basename`, `bump`, `push_skip_detail`, the skipped-CSV writers, `SkipDetails`, the skip-detail structs, the consts, `ConvertExportArgs`, and the test module.

Declare the modules in `lib.rs`. Behavior-neutral move.

**Verify:** `cargo test -p go-sms-pro-exporter` (all pass, incl. the chat-id tests), clippy `-D warnings`, fmt. **Commit:** `refactor(go-sms-pro): split emit.rs chat-id and attachment helpers into modules`.

---

### Task 16: split sms-backup-plus-exporter emit.rs

Finding 19 (4 of 4). Move the parse pipeline and the attachment cluster out of `emit.rs`.

**Files:**
- Create: `crates/exporters/sms-backup-plus-exporter/src/parse_emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/attachments_emit.rs`
- Modify: `crates/exporters/sms-backup-plus-exporter/src/emit.rs`, `crates/exporters/sms-backup-plus-exporter/src/lib.rs`

**Moves (verbatim, `pub(super)` where required):**
- → `parse_emit.rs`: `parse_one_eml`, `ParsedEmlKind`, `collect_eml_paths` (with its nested `in_skipped_dir`).
- → `attachments_emit.rs`: `write_attachments`, `merge_attachments`, `pending_attachment_to_ir`.
- **Stays:** `convert_export`, `ensure_convo`, `pending_from_parsed`, `add_message`, `should_replace_kept`, `prepare_conversation`, `pending_to_document`, `display_names_for_handles`, `first_contact_name`, `peer_handles_from_digits`, `relative_eml_path`, `is_eml_file`, `vlog`, `report_progress`, `bump`, `count`, `ConvertExportArgs`, the consts, and the test module.

Declare the modules in `lib.rs`. Behavior-neutral move.

**Verify:** `cargo test -p sms-backup-plus-exporter` (all pass, incl. the rayon parse path), clippy `-D warnings`, fmt. **Commit:** `refactor(sms-backup-plus): split emit.rs parse and attachment helpers into modules`.

---

### Task 17: CHANGELOG and final workspace verification

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add the CHANGELOG entry**

In `CHANGELOG.md`, under `[Unreleased]` → `### Changed`, add (matching the file's existing entry style):

```markdown
- **Exporters:** hoist the duplicated exporter pipeline, CLI driver, output
  preamble, attachment naming, and mechanical helpers into
  `message-vault-io-core` and the shared lib crates, document and gate the
  core config/form surfaces, split the four oversized emit.rs files, and
  wire imessage-ir's previously ignored media flags. CLI help text and
  exported output are unchanged; imessage-ir now honors `--media-mode`
  convert/compress when passed.
```

- [ ] **Step 2: Final verification**

Run each and confirm clean:
- `cargo test --workspace` — all 67 targets pass
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo doc --no-deps -p message-vault-io-core -p media -p message-csv 2>&1 | grep -E "warning|error"` — zero lines
- `cargo test -p message-vault-server committed_openapi_matches_dump` and `cargo test -p dump-cli-docs committed_cli_pages_match_dump` — both pass

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for exporters consolidation"
```
