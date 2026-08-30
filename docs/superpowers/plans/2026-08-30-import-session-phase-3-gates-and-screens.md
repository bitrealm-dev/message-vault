# Import Session Phase 3 — The Gates and the Screens Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An import stops and asks before it spends an hour converting media, and stops again before anything enters the vault — and the screens say which stage the user is looking at instead of calling every one of them "Import Messages".

**Architecture:** Converting and compressing move out of the exporter run and become a second pass over the staging folder that patches the conversation files it already wrote. The desktop gets that split for free by asking the exporter for `copy` whenever the user chose Convert or Compress, so no shared crate changes behaviour for CLI or library callers. With the expensive work separated, two approval gates fit between the pieces: Gate 1 after the folder is staged and before the media pass, Gate 2 after it and before the push. Both are new screens reading a summary recomputed from the folder on disk.

**Tech Stack:** Rust (`media`, `message-ir-format`, `message-vault-io-core`, Tauri commands, Axum server, sqlx over SQLite and Postgres), React 19 + TypeScript (Vitest + Testing Library), Biome.

**Spec:** `docs/superpowers/specs/2026-08-29-import-session-design.md` — this phase implements sequencing step 3 and the transcode half of step 4: decisions 8–20, 27–30, 39, 43–46, and the gate rows of 36. Phase 1 (decisions 21, 22, 40, 41) is merged as `56b0bb56`; Phase 2 (decisions 1–5, 35, 37) as `db2d50fb`.

**Branch:** `claude/import-session-phase-3`, cut from `main` at `db2d50fb`.

## What this phase can and cannot deliver

The spec's sequencing puts both gates in step 3 and the prepare restructure in step 4, but Gate 1's position — after write, before the media step — is a pipeline state that does not exist until conversion is separated from the attachment-copy pass. Confirmed by reading the code: every exporter calls `run_attachment_jobs`, which calls `apply_convert_or_compress` internally (`crates/core/message-vault-io-core/src/attachment_jobs.rs:126-137`), and only afterwards writes any conversation file (`crates/exporters/imazing-exporter/src/emit.rs:318-348` is representative). So this phase takes the transcode half of step 4 as well. That was ruled on explicitly and is the reason this plan is larger than Phase 2's.

**What this phase does NOT take from step 4:** the writer queue and parallel writers (decisions 23–25), disk headroom checks (32), worker counts by phase (33), `persist_clone`'s unique temp suffix (34), and `open_prepared`'s resume mode (42). All of those serve resuming `write`, which stays a single unsplit unit here. A task that leaves `write` unresumable is correct.

What ships:

- Under Convert or Compress: the import stops after staging, shows exact counts and a per-file size forecast, and converts nothing until the user says go. Stops again afterwards with the delta, and uploads nothing until the user says go.
- Under Copy as-is or Skip: one gate, after staging and before upload, with exact numbers and no forecast — there is no media step to forecast.
- Conversion is resumable. It commits per file through a rename, so an interrupted run re-does one file and no more.
- Every import screen names the stage it is on.
- Declining at either gate closes the session and deletes the staging folder.
- The final outcome is diffed against what the user approved at the last gate, so an expected omission reads as expected.

## Global Constraints

- **The word "transcode" never appears in user-facing copy** (decision 18). It is a stage name and a module name only. On screen the user sees **Convert** or **Compress** according to the media mode, and those are two different jobs: convert changes the format, compress changes the format *and* targets a smaller size.
- **Product copy states what the product can do; it does not warn, alarm, or hedge about consequences.** Decision 17's Gate 2 copy is written for the finished product and is not to be softened into a warning about irreversibility.
- **iMessage and SMS/MMS are both labelled "Text Message"** in user-facing copy, never by transport.
- **The database is authoritative; the filesystem holds work products** (decision 1). Progress *within* a stage is recomputed from the folder, never stored (decision 4). The gate summary is recomputed on resume, never read back from `summary_json` (decision 39) — `summary_json` records what the user approved, which is a different question, and is what decision 15 diffs against.
- Stage strings, exactly: `parse`, `write`, `awaiting_gate_1`, `transcode`, `awaiting_gate_2`, `pushing`. All six already exist server-side and in `web/src/lib/importSession.ts`; this phase is the first to write the middle four.
- Status strings, exactly: `running`, `completed`, `completed_with_issues`, `failed`, `cancelled`.
- **`missing_reason` is the closed set from Phase 1**, exactly: `file_missing`, `too_large`, `not_copied`, `convert_failed: <detail>`, `unknown: <raw>`. Do not invent a sixth. A file that crosses the size limit during the media pass becomes `too_large` (decision 45).
- **No schema change.** `vault_imports` already carries every column this phase needs. Do not touch `SCHEMA_VERSION`, `schema/sql/*.sql`, or `crates/vault/server/src/db/schema.rs`. A task that appears to need a column is wrong — re-read decision 39.
- Version lockstep files are not touched by this plan: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `web/package.json`, `crates/vault/server/Cargo.toml`. No version bump.
- `docs/src/assets/openapi.json` has a committed-dump gate (`committed_openapi_matches_dump`). Any change to a `utoipa::ToSchema` type or a routed handler must regenerate it in the same commit: `cargo run -q -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`.
- `cargo fmt --all -- --check` and `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` must pass. Biome must pass (`cd web && npm run lint`); imports sorted, unused bindings prefixed `_`, real fixes over `biome-ignore`.
- `src-tauri/` is not a workspace member. Build it with `--manifest-path src-tauri/Cargo.toml`.
- Tests use committed fixtures in `tests/fixtures/`; never real message data. Tests that need ffmpeg must skip cleanly when it is absent — see `crates/libs/media/src/process.rs` for the existing pattern.
- Never commit to `main`. Do not push, tag, or open a PR unless asked. Do not merge.
- Literal code below was written against `main` at `db2d50fb`. Where a snippet and the compiler disagree, the compiler is authoritative — keep the intent, fix the syntax.
- Commit after every task.

## Rulings made while planning

These resolve conflicts between the spec and the code as it stands. Each is binding on the tasks below.

**The split is a call-site choice, not a change to shared behaviour.** Decision 26 says writers do not transcode. Implemented literally — removing `apply_convert_or_compress` from `run_attachment_jobs` — every CLI and library caller that asks for Convert would silently stage unconverted files. Instead the desktop asks the exporter for `copy` whenever the user chose `convert` or `compress`, then runs the media pass itself over the staging folder. `run_attachment_jobs` is untouched and every existing caller keeps today's behaviour. Cost if wrong: the desktop and the CLI take different routes to the same output, which Phase 4 unifies when the writer queue lands.

**The size limit the forecast predicts is the desktop's 50 MiB.** Three values exist: `crates/vault/server/src/config.rs:68` defaults to 512 MiB, `crates/cli/vault-push/src/run.rs:76` likewise, and `src-tauri/src/commands/push.rs:136` hardcodes `50 * 1024 * 1024`. No endpoint tells the client the server's configured value. A forecast must predict what the push will actually do, so it uses the desktop's number — promoted from a literal to one named constant both the push command and the forecast read. Exposing the server's real value is a follow-up for issue #230, not this plan. Cost if wrong: on a vault configured below 50 MiB, a file the forecast called fine is refused at upload and reported as an issue — the same outcome as today.

**Gate screens are new components; `ImportSummaryPanel` is not widened.** It has a second call site outside the live import flow (`web/src/screens/settings/storage/ImportDetailPanel.tsx:96`, historical imports read back from the server). Widening its props to carry gate data would couple the history view to a shape it has no source for. Cost if wrong: some presentational duplication between the gate screens and the summary panel.

**The media pass reuses the `extract:*` event channel.** `awaitTauriJob` is one-shot per job (`web/src/lib/tauri.ts:207-234`) and the media pass is a separate job that runs after extract has finished, so the same channel carries it without collision. A parallel `media:*` vocabulary would double the wiring for no gain. `ImportProgressEvent["step"]` gains a `"media"` member. Cost if wrong: a second job's events would be ambiguous if the two ever overlapped, which the gate between them prevents by construction.

**"Preparing messages" folds into "Copy to staging".** Decision 8 names four steps: Read backup, Copy to staging, Convert (or Compress) media, Upload to vault. The pipeline emits a `prepare` progress step today (writing conversation files) that decision 8 does not name. Both `attachments` and `prepare` map to the Copy to staging step — from the user's side, staging is one thing. Cost if wrong: the staging step's detail line changes verb partway through, which is what it already does.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/libs/media/src/process.rs` | Explicit file list; keep-smaller guard for same-format re-encodes | 1 |
| `crates/libs/media/src/probe.rs` | **new** — public single-file probe: kind, dimensions, fps, codec | 2 |
| `crates/libs/media/src/estimate.rs` | **new** — size estimate and the five-state classifier | 2 |
| `crates/libs/ir-format/src/transcode.rs` | **new** — the media pass over a staged folder, patching JSONL | 3 |
| `crates/libs/ir-format/src/staging_summary.rs` | **new** — recompute a summary from a staged folder | 4 |
| `src-tauri/src/commands/staging.rs` | **new** — `summarize_staging` and `transcode_staging` commands | 5 |
| `src-tauri/src/commands/push.rs` | `asset_max_bytes` reads the shared constant | 5 |
| `crates/vault/server/src/contacts_api.rs` | `POST /v1/contacts/match` | 6 |
| `crates/vault/server/src/import/mod.rs` | A stage change may carry the approved plan | 6 |
| `src-tauri/src/commands/paths.rs` | `delete_staging`, root-guarded | 5 |
| `web/src/lib/types.ts` | `ImportProgressEvent["step"]` gains `"media"` | 7 |
| `web/src/lib/tauri.ts` | `invokeSummarizeStaging`, `invokeTranscodeStaging` | 7 |
| `web/src/screens/import/importProgressState.ts` | Step list by media mode; `stepIndexFor` covers `"media"` | 7 |
| `web/src/screens/import/GateOneScreen.tsx` | **new** — review what was copied | 8 |
| `web/src/screens/import/gateForecast.ts` | **new** — forecast rows and their copy | 8 |
| `web/src/screens/import/GateTwoScreen.tsx` | **new** — ready to upload, delta first | 9 |
| `web/src/screens/import/gateDelta.ts` | **new** — Gate 1 approval against Gate 2 measurement | 9 |
| `web/src/screens/import/useImportJob.ts` | Gate phases, stage writes, decline is terminal | 10 |
| `web/src/screens/ImportScreen.tsx` | Renders the gate screens; wires approve and decline | 10 |
| `web/src/screens/import/importOutcome.ts` | Outcome diffed against the approved plan | 11 |
| `web/src/screens/import/resumeDecision.ts` | Gate stages become resumable outcomes | 12 |
| `web/src/screens/import/ResumeImportPanel.tsx` | Copy for resuming at a gate | 12 |

---

### Task 1: The media crate transcodes one file and lets the caller commit it

Two changes to the media crate, both prerequisites for running conversion as a separate resumable pass over a folder that already holds conversation files.

Decision 28 fixes the commit order: transcode to `<derivative>.in_progress`, patch the conversation file, rename `.in_progress` to the final name, delete the original. The final name never exists until the conversation file already points at it. That gives the two invariants the whole of resume rests on — a file under its final derivative name is fully patched, and an original still on disk means work remains. Today `replace_original` renames the derivative into place and *then* deletes the original, so the final name exists before anything is patched. The pass therefore needs a per-file entry point where it owns the commit.

Decision 44: `replace_original` keeps whatever ffmpeg produced with no comparison. Where the conversion changes format that is correct and must stay — the user picked Convert or Compress because they want the target format, and handing back a smaller HEIC gives them a file the browser cannot display. The guard applies only to same-format re-encodes, where no format benefit is bought and a larger output is pure loss. There are exactly two: a JPEG over 500 KB re-encoded to JPEG, and an MP3 over 100 KB re-encoded to MP3, both in `compress` mode.

**Ruling carried into this task:** decision 46 asks for `process_attachments_dir` to take an explicit file list, so that conversion can be scoped to a known set instead of walking the folder. This task serves that purpose at a finer grain — the caller drives one file at a time, which is strictly more scoped than a list, and is the only shape that also gives the caller the commit point decision 28 requires. `process_attachments_dir` keeps its current signature and behaviour for the CLI and library callers that use it.

**Files:**
- Modify: `crates/libs/media/src/process.rs`
- Modify: `crates/libs/media/src/lib.rs:18` (re-exports)
- Test: `crates/libs/media/src/process.rs` (the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces, all called only by Task 3:
  - `media::derivative_name(src: &Path, mode: MediaMode) -> Option<String>` — the file name the media step would produce, or `None` when it leaves the file alone.
  - `media::TranscodeOutcome` — `Skipped` or `Produced`.
  - `media::transcode_file(src: &Path, dest: &Path, mode: MediaMode, compress: &CompressOptions) -> anyhow::Result<TranscodeOutcome>` — writes the derivative to exactly `dest` and never touches `src`.

- [ ] **Step 1: Write the failing test for the keep-smaller guard**

Add to the `tests` module in `crates/libs/media/src/process.rs`. Follow the existing ffmpeg-gated pattern in that module — copy how the neighbouring tests skip when the tools are absent.

```rust
#[test]
fn compress_keeps_the_original_jpeg_when_the_re_encode_is_not_smaller() {
    if !ffmpeg_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let attachments = dir.path().join("attachments");
    fs::create_dir_all(&attachments).unwrap();

    // A JPEG that is already tight for its pixel count: re-encoding at -q:v 5
    // produces a file no smaller than the source. Over 500 KB so the size gate
    // in process_one does not skip it outright.
    let jpeg = attachments.join("already-tight.jpg");
    write_incompressible_jpeg(&jpeg, 900 * 1024);
    let before = fs::read(&jpeg).unwrap();

    let (report, remap) =
        process_attachments_dir(dir.path(), MediaMode::Compress, &CompressOptions::default())
            .unwrap();

    assert_eq!(fs::read(&jpeg).unwrap(), before, "original bytes replaced");
    assert!(
        !remap.contains_key("attachments/already-tight.jpg"),
        "a kept file must not be remapped: a remap entry tells the caller to \
         recompute a digest that did not change"
    );
    assert_eq!(report.processed, 0);
    assert_eq!(report.skipped, 1);
}
```

`write_incompressible_jpeg` is a new test helper in the same module: write random RGB noise through ffmpeg at `-q:v 2`, sized so that re-encoding at `-q:v 5` is not smaller. If a reliable fixture cannot be produced from noise on the CI image, assert the guard by calling `is_smaller` directly instead — but keep the assertion that the original bytes survive.

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test -p media compress_keeps_the_original_jpeg -- --nocapture
```

Expected: FAIL — the original is replaced by the larger re-encode.

- [ ] **Step 3: Give the ffmpeg helpers a commit strategy**

In `crates/libs/media/src/process.rs`, add the strategy and the comparison next to `replace_original`:

```rust
/// Where a freshly produced derivative goes.
#[derive(Debug, Clone, Copy)]
enum Commit<'a> {
    /// Replace the original in place, deleting it. The directory pass's
    /// behaviour, unchanged.
    InPlace,
    /// Move the derivative to exactly this path and leave the original alone.
    ///
    /// The caller commits: it patches whatever points at the original, renames
    /// this file into its final name, and only then deletes the original
    /// (decision 28).
    To(&'a Path),
}

fn commit_produced(commit: Commit<'_>, original: &Path, produced: &Path) -> Result<PathBuf> {
    match commit {
        Commit::InPlace => replace_original(original, produced),
        Commit::To(dest) => {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::rename(produced, dest)
                .with_context(|| format!("rename {} to {}", produced.display(), dest.display()))?;
            Ok(dest.to_path_buf())
        }
    }
}

/// Is `produced` actually smaller than `original`?
///
/// Only meaningful for a same-format re-encode. Where the format changes the
/// user asked for the target format, and a smaller file in the source format
/// is not a substitute for it.
fn is_smaller(produced: &Path, original: &Path) -> Result<bool> {
    Ok(fs::metadata(produced)?.len() < fs::metadata(original)?.len())
}
```

Thread `commit: Commit<'_>` through the five producers — `convert_image`,
`convert_audio`, `convert_video`, `compress_video`, `try_remux_replace` — and
replace every `replace_original(path, &tmp)` call inside them with
`commit_produced(commit, path, &tmp)`. There are six such call sites; `git grep -n
replace_original crates/libs/media/src/process.rs` finds them all. `replace_original`
itself is unchanged.

`convert_image` and `convert_audio` also gain the guard and an `Option` return,
matching the shape `compress_video` already uses:

```rust
fn convert_image(
    path: &Path,
    compress: bool,
    keep_smaller: bool,
    commit: Commit<'_>,
) -> Result<Option<PathBuf>> {
    let tmp = temp_sibling(path, "jpg");
    let quality = if compress { "5" } else { "2" }; // ffmpeg -q:v (2 best … 31 worst for mjpeg)
    // `-frames:v 1 -update 1`: animated GIF/WebP must write a single still, not an
    // image2 sequence (otherwise ffmpeg leaves a partial tmp and exits non-zero).
    let args = vec![
        "-y".into(),
        "-i".into(),
        path_str(path),
        "-frames:v".into(),
        "1".into(),
        "-update".into(),
        "1".into(),
        "-q:v".into(),
        quality.into(),
        path_str(&tmp),
    ];
    with_temp_output(&tmp, || {
        run_ffmpeg(&args).with_context(|| format!("convert image {}", path.display()))?;
        if keep_smaller && !is_smaller(&tmp, path)? {
            let _ = fs::remove_file(&tmp);
            return Ok(None);
        }
        commit_produced(commit, path, &tmp).map(Some)
    })
}
```

`convert_audio` takes the same three-line change, with `mp3` as its tmp extension.

- [ ] **Step 4: Rework `process_one` around the strategy**

Split `process_one` into an inner function that answers "what did the media step
produce for this file" and the existing wrapper that turns that into a remap
entry. The wrapper's behaviour is unchanged.

```rust
/// Run the media step over one file, committing however `commit` says.
///
/// Returns the produced path, or `None` when the media step leaves this file
/// alone — either because the mode does not touch it, or because a same-format
/// re-encode came out no smaller (decision 44).
fn run_one(
    path: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    commit: Commit<'_>,
) -> Result<Option<PathBuf>> {
    let kind = classify(path).context("unknown media kind")?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match (kind, mode) {
        (Kind::Image, MediaMode::Convert) => {
            // Keep GIF as-is (animation); jpg already in target form.
            if matches!(ext.as_str(), "jpg" | "jpeg" | "gif") {
                return Ok(None);
            }
            convert_image(path, false, false, commit)
        }
        (Kind::Image, MediaMode::Compress) => {
            if ext == "gif" {
                return Ok(None);
            }
            let same_format = matches!(ext.as_str(), "jpg" | "jpeg");
            if same_format && fs::metadata(path)?.len() <= JPEG_COMPRESS_FLOOR {
                return Ok(None);
            }
            convert_image(path, true, same_format, commit)
        }
        (Kind::Audio, MediaMode::Convert) => {
            if ext == "mp3" {
                return Ok(None);
            }
            convert_audio(path, false, false, commit)
        }
        (Kind::Audio, MediaMode::Compress) => {
            let same_format = ext == "mp3";
            if same_format && fs::metadata(path)?.len() <= MP3_COMPRESS_FLOOR {
                return Ok(None);
            }
            convert_audio(path, true, same_format, commit)
        }
        (Kind::Video, MediaMode::Convert) => convert_video(path, commit).map(Some),
        (Kind::Video, MediaMode::Compress) => compress_video(path, compress, commit),
        (_, MediaMode::Clone | MediaMode::Disabled) => Ok(None),
    }
}

fn process_one(
    output_dir: &Path,
    path: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<Outcome> {
    let old_rel = rel_path(output_dir, path)?;
    match run_one(path, mode, compress, Commit::InPlace)? {
        Some(new_path) => changed(output_dir, &old_rel, &new_path),
        None => Ok(Outcome::Skipped),
    }
}
```

Lift the two size gates to named constants beside `MEDIA_PROGRESS_EVERY`, so
`derivative_name` in the next step reads the same numbers rather than repeating
the literals:

```rust
/// JPEGs at or under this size are left alone in compress mode: re-encoding
/// them buys nothing.
const JPEG_COMPRESS_FLOOR: u64 = 500 * 1024;
/// MP3s at or under this size are left alone in compress mode.
const MP3_COMPRESS_FLOOR: u64 = 100 * 1024;
```

- [ ] **Step 5: Run the crate suite**

```bash
cargo test -p media
```

Expected: PASS, including the new keep-smaller test and every pre-existing test
— the directory pass's behaviour is unchanged.

- [ ] **Step 6: Write the failing tests for the per-file entry point**

```rust
#[test]
fn transcode_file_writes_the_derivative_and_leaves_the_original_alone() {
    if !ffmpeg_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("photo.png");
    write_test_png(&src);
    let before = fs::read(&src).unwrap();

    let name = derivative_name(&src, MediaMode::Convert).expect("png is converted");
    assert_eq!(name, "photo.jpg");
    let dest = dir.path().join(format!("{name}.in_progress"));

    let outcome =
        transcode_file(&src, &dest, MediaMode::Convert, &CompressOptions::default()).unwrap();

    assert_eq!(outcome, TranscodeOutcome::Produced);
    assert!(dest.exists(), "derivative written where the caller asked");
    assert!(
        !dir.path().join("photo.jpg").exists(),
        "the final name must not exist until the caller renames it: a file \
         under its final name means fully patched"
    );
    assert_eq!(
        fs::read(&src).unwrap(),
        before,
        "the original is the caller's to delete, after it commits"
    );
}

#[test]
fn derivative_name_is_none_for_a_file_the_mode_leaves_alone() {
    let dir = tempfile::tempdir().unwrap();
    let gif = dir.path().join("loop.gif");
    fs::write(&gif, b"not really a gif").unwrap();
    assert_eq!(derivative_name(&gif, MediaMode::Convert), None);

    let jpeg = dir.path().join("photo.jpg");
    fs::write(&jpeg, b"not really a jpeg").unwrap();
    assert_eq!(derivative_name(&jpeg, MediaMode::Convert), None);

    let doc = dir.path().join("notes.pdf");
    fs::write(&doc, b"%PDF").unwrap();
    assert_eq!(derivative_name(&doc, MediaMode::Convert), None);
}

#[test]
fn derivative_name_matches_what_the_media_step_actually_produces() {
    if !ffmpeg_available() {
        return;
    }
    // The forecast and the patch both trust derivative_name. If it disagrees
    // with the pass, a conversation file points at a name nothing wrote.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("photo.png");
    write_test_png(&src);
    let name = derivative_name(&src, MediaMode::Convert).unwrap();
    let dest = dir.path().join("out").join(&name);
    transcode_file(&src, &dest, MediaMode::Convert, &CompressOptions::default()).unwrap();
    assert_eq!(dest.file_name().and_then(|n| n.to_str()), Some(name.as_str()));
}

#[test]
fn transcode_file_clears_scratch_beside_the_source_only() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("photo.png");
    write_test_png(&src);
    let own_scratch = dir.path().join("photo.msgmedia.tmp.jpg");
    fs::write(&own_scratch, b"leftover").unwrap();
    let other_scratch = dir.path().join("other.msgmedia.tmp.jpg");
    fs::write(&other_scratch, b"in flight").unwrap();
    let marker = dir.path().join("photo.jpg.in_progress");
    fs::write(&marker, b"a previous attempt").unwrap();

    // Clone mode returns before any ffmpeg work, which is enough to show what
    // the entry point sweeps.
    let _ = transcode_file(
        &src,
        &dir.path().join("photo.jpg.in_progress"),
        MediaMode::Clone,
        &CompressOptions::default(),
    );

    assert!(!own_scratch.exists(), "this file's own leftovers go");
    assert!(
        other_scratch.exists(),
        "another file's in-flight scratch must survive: a folder-wide sweep \
         destroys work that is still running"
    );
    assert!(
        marker.exists(),
        "the .in_progress marker is the resume signal and must survive the \
         scratch sweep (decision 30)"
    );
}
```

- [ ] **Step 7: Run them and watch them fail**

```bash
cargo test -p media transcode_file && cargo test -p media derivative_name
```

Expected: FAIL to compile — neither function exists.

- [ ] **Step 8: Add the per-file entry point**

```rust
/// What [`transcode_file`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscodeOutcome {
    /// Nothing was written: the mode does not touch this file, or a
    /// same-format re-encode came out no smaller.
    Skipped,
    /// A derivative was written to the destination the caller named.
    Produced,
}

/// File name the media step would produce for `src`, or `None` when it leaves
/// the file alone.
///
/// Reads the same decision tree as the pass itself, so the name a caller
/// patches into a conversation file is the name the pass writes.
#[must_use]
pub fn derivative_name(src: &Path, mode: MediaMode) -> Option<String> {
    let kind = classify(src)?;
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = src.file_stem().and_then(|s| s.to_str())?;
    let target = match (kind, mode) {
        (_, MediaMode::Clone | MediaMode::Disabled) => return None,
        (Kind::Image, MediaMode::Convert) => {
            if matches!(ext.as_str(), "jpg" | "jpeg" | "gif") {
                return None;
            }
            "jpg"
        }
        (Kind::Image, MediaMode::Compress) => {
            if ext == "gif" {
                return None;
            }
            if matches!(ext.as_str(), "jpg" | "jpeg")
                && fs::metadata(src).map(|m| m.len()).unwrap_or(0) <= JPEG_COMPRESS_FLOOR
            {
                return None;
            }
            "jpg"
        }
        (Kind::Audio, MediaMode::Convert) => {
            if ext == "mp3" {
                return None;
            }
            "mp3"
        }
        (Kind::Audio, MediaMode::Compress) => {
            if ext == "mp3" && fs::metadata(src).map(|m| m.len()).unwrap_or(0) <= MP3_COMPRESS_FLOOR
            {
                return None;
            }
            "mp3"
        }
        (Kind::Video, _) => "mp4",
    };
    Some(format!("{stem}.{target}"))
}

/// Transcode `src` and write the derivative to exactly `dest`.
///
/// `src` is never modified or deleted: committing is the caller's, because it
/// has to patch whatever points at the original first (decision 28). Scratch
/// left beside `src` by an interrupted run is cleared; scratch belonging to
/// other files, and any `.in_progress` marker, is left alone.
///
/// # Errors
///
/// Returns an error when ffmpeg/ffprobe are missing or fail, or IO fails.
pub fn transcode_file(
    src: &Path,
    dest: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
) -> Result<TranscodeOutcome> {
    if matches!(mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok(TranscodeOutcome::Skipped);
    }
    require_ffmpeg()?;
    remove_temps_beside(src);
    match run_one(src, mode, compress, Commit::To(dest))? {
        Some(_) => Ok(TranscodeOutcome::Produced),
        None => Ok(TranscodeOutcome::Skipped),
    }
}
```

Add the scoped scratch sweep beside `remove_msgmedia_temps`. The folder-wide
sweep is wrong for a per-file pass: the folder can hold scratch belonging to
work still in flight, and the `.in_progress` marker is the resume signal.

```rust
/// Delete ffmpeg scratch left beside `path` by an earlier interrupted run.
///
/// Scoped to this file's own siblings, and matched on the same
/// `.msgmedia.tmp.` marker `remove_msgmedia_temps` uses — which is why a
/// `.in_progress` file survives it (decision 30).
fn remove_temps_beside(path: &Path) {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str()))
    else {
        return;
    };
    let prefix = format!("{stem}.");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let candidate = entry.path();
        if is_msgmedia_temp(&candidate)
            && candidate
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        {
            let _ = fs::remove_file(&candidate);
        }
    }
}
```

Export from `crates/libs/media/src/lib.rs:18`:

```rust
pub use process::{
    MediaReport, TranscodeOutcome, derivative_name, process_attachments_dir,
    process_attachments_dir_with_log, transcode_file,
};
```

`classify` and `Kind` are already private to `process.rs`; `derivative_name`
lives in that module so it reads them directly. Task 2's `estimate.rs` needs
them too — make both `pub(crate)` in the same commit.

- [ ] **Step 9: Run the crate suite and the linter**

```bash
cargo test -p media && cargo clippy -p media --all-targets -- -D warnings && cargo fmt --all -- --check
```

Expected: PASS with no new warnings.

- [ ] **Step 10: Commit**

```bash
git add crates/libs/media/src/process.rs crates/libs/media/src/lib.rs
git commit -m "feat(media): transcode one file to a caller-named path, and keep the smaller same-format re-encode"
```

---

### Task 2: Forecast what the media step will do to a file's size

Gate 1 reports its counts from the folder, so they are exact — except one thing, which is a guess: what the media step will do to each file's size. Decision 12 is explicit that the guess must run in both directions, because conversion is not a size reduction. In `convert` mode any image that is not already JPEG or GIF is written to JPEG at `-q:v 2`, and HEIC is roughly half the size of an equivalent JPEG, so HEIC grows. Video prefers a remux, but when that fails it re-encodes, and HEVC to H.264 typically grows. An iPhone backup is mostly HEIC and HEVC, so this is the common path, not a corner.

Decision 13 adds that files near the limit are classified whether or not they are over it, because the media step can push a file across. Five states, and probing every media file may be too slow — so probe files within a band around the limit and treat the rest by size alone.

**Files:**
- Create: `crates/libs/media/src/probe.rs`
- Create: `crates/libs/media/src/estimate.rs`
- Modify: `crates/libs/media/src/lib.rs` (module declarations and re-exports)
- Test: inside both new files

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `media::MediaProbe { codec: String, width: u32, height: u32, fps: Option<f32> }` and `media::probe_media(path: &Path) -> anyhow::Result<MediaProbe>`.
  - `media::SizeVerdict` — the five states, `Serialize`/`Deserialize` with `#[serde(rename_all = "snake_case")]`.
  - `media::classify_probed(size_bytes: u64, probe: Option<&MediaProbe>, ext: &str, mode: MediaMode, compress: &CompressOptions, limit_bytes: u64) -> SizeVerdict`.
  - `media::estimate_bytes(...)` — public so the screen can show the estimate itself, not only the verdict.
  Task 4 calls `classify_probed` per attachment; Task 5 serializes `SizeVerdict` across the Tauri boundary; Task 8 renders it.

- [ ] **Step 1: Write the failing probe test**

Create `crates/libs/media/src/probe.rs` with its test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_rational_frame_rate() {
        assert_eq!(parse_frame_rate("30000/1001"), Some(29.97003));
        assert_eq!(parse_frame_rate("30/1"), Some(30.0));
    }

    #[test]
    fn a_still_image_has_no_frame_rate() {
        // ffprobe reports 0/0 for a still: there is no rate, and dividing by
        // the denominator would be a panic on some inputs and a lie on others.
        assert_eq!(parse_frame_rate("0/0"), None);
        assert_eq!(parse_frame_rate(""), None);
        assert_eq!(parse_frame_rate("N/A"), None);
    }

    #[test]
    fn reads_one_csv_line_into_a_probe() {
        let probe = parse_probe_line("hevc,3840,2160,30000/1001").unwrap();
        assert_eq!(probe.codec, "hevc");
        assert_eq!(probe.width, 3840);
        assert_eq!(probe.height, 2160);
        assert_eq!(probe.fps, Some(29.97003));
    }

    #[test]
    fn a_short_csv_line_is_an_error_not_a_default() {
        // A defaulted probe reads as a 0x0 file, which the estimate would
        // divide by. Fail loudly instead.
        assert!(parse_probe_line("hevc,3840").is_err());
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test -p media --lib probe
```

Expected: FAIL to compile — the module does not exist.

- [ ] **Step 3: Write the probe**

`crates/libs/media/src/probe.rs`. The existing crate-private `probe_video` in
`tools.rs:273` stays where it is and keeps its callers; this is a public
superset that also reports frame rate, which the estimate needs and
`probe_video` does not carry.

```rust
//! Read one media file's shape with ffprobe, for the size forecast.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::tools::ffprobe_command;

/// What ffprobe reports about one media file's first video stream.
///
/// Stills have a stream too, with no frame rate.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaProbe {
    /// Codec as ffprobe spells it, lowercased: `hevc`, `h264`, `mjpeg`, `png`.
    pub codec: String,
    /// Pixel width. Zero only when ffprobe reported no dimensions.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Frames per second, `None` for stills.
    pub fps: Option<f32>,
}

impl MediaProbe {
    /// Total pixels in one frame, as a float so ratios do not truncate.
    #[must_use]
    pub fn pixels(&self) -> f64 {
        f64::from(self.width) * f64::from(self.height)
    }
}

/// Ask ffprobe about `path`.
///
/// # Errors
///
/// Returns an error when ffprobe is missing, exits non-zero, or reports a
/// line this cannot read.
pub fn probe_media(path: &Path) -> Result<MediaProbe> {
    let mut cmd: Command = ffprobe_command();
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=codec_name,width,height,avg_frame_rate",
        "-of",
        "csv=p=0",
    ]);
    cmd.arg(path);
    let out = cmd
        .output()
        .with_context(|| format!("run ffprobe on {}", path.display()))?;
    if !out.status.success() {
        return Err(anyhow!(
            "ffprobe failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow!("ffprobe reported no stream for {}", path.display()))?;
    parse_probe_line(line.trim())
}

/// Read one `codec,width,height,avg_frame_rate` line.
fn parse_probe_line(line: &str) -> Result<MediaProbe> {
    let mut parts = line.split(',');
    let codec = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
    let width = parts.next().ok_or_else(|| anyhow!("no width in {line:?}"))?;
    let height = parts.next().ok_or_else(|| anyhow!("no height in {line:?}"))?;
    let rate = parts.next().unwrap_or_default();
    Ok(MediaProbe {
        codec,
        width: width.trim().parse().unwrap_or(0),
        height: height.trim().parse().unwrap_or(0),
        fps: parse_frame_rate(rate.trim()),
    })
}

/// ffprobe writes frame rates as a rational: `30000/1001`, or `0/0` for a still.
fn parse_frame_rate(raw: &str) -> Option<f32> {
    let (num, den) = raw.split_once('/')?;
    let num: f32 = num.trim().parse().ok()?;
    let den: f32 = den.trim().parse().ok()?;
    if num <= 0.0 || den <= 0.0 {
        return None;
    }
    Some(num / den)
}
```

If `tools.rs` has no `ffprobe_command` helper, add one there mirroring however
`probe_video` builds its command today, and make `probe_video` use it too — one
place decides where ffprobe lives.

- [ ] **Step 4: Run the probe tests**

```bash
cargo test -p media --lib probe
```

Expected: PASS.

- [ ] **Step 5: Write the failing estimate tests**

Create `crates/libs/media/src/estimate.rs` with its test module. These cases are
the decision, not illustrations — keep every one.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: u64 = 50 * 1024 * 1024;

    fn probe(codec: &str, width: u32, height: u32, fps: Option<f32>) -> MediaProbe {
        MediaProbe { codec: codec.into(), width, height, fps }
    }

    #[test]
    fn a_small_file_is_fine_without_probing() {
        // Under the probe band: no ffprobe call, decided on size alone.
        assert_eq!(
            classify_probed(1024, None, "heic", MediaMode::Convert, &CompressOptions::default(), LIMIT),
            SizeVerdict::FitsAsIs
        );
    }

    #[test]
    fn heic_under_the_limit_may_grow_past_it() {
        // Decision 12's headline case: HEIC is about half an equivalent JPEG,
        // so converting grows it. 30 MB in, over 50 MB out.
        let p = probe("hevc", 4032, 3024, None);
        assert_eq!(
            classify_probed(30 * 1024 * 1024, Some(&p), "heic", MediaMode::Convert, &CompressOptions::default(), LIMIT),
            SizeVerdict::MayGrow
        );
    }

    #[test]
    fn a_huge_video_compressed_down_is_likely_to_fit() {
        // 4K30 at 400 MB, compressed to 1080p30: the pixel ratio alone is
        // about a quarter, and it lands comfortably under 80% of the limit.
        let p = probe("hevc", 3840, 2160, Some(30.0));
        assert_eq!(
            classify_probed(400 * 1024 * 1024, Some(&p), "mov", MediaMode::Compress, &CompressOptions::default(), LIMIT),
            SizeVerdict::LikelyFits
        );
    }

    #[test]
    fn a_video_that_stays_over_the_limit_says_so() {
        let p = probe("h264", 1920, 1080, Some(30.0));
        assert_eq!(
            classify_probed(900 * 1024 * 1024, Some(&p), "mp4", MediaMode::Compress, &CompressOptions::default(), LIMIT),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn an_estimate_just_under_the_limit_still_reads_as_too_big() {
        // The 80% margin: a near miss must not read as a promise.
        let p = probe("h264", 1920, 1080, Some(30.0));
        let size = 60 * 1024 * 1024;
        let estimate = estimate_bytes(size, Some(&p), "mp4", MediaMode::Compress, &CompressOptions::default());
        assert!(estimate < LIMIT, "test needs an estimate under the limit");
        assert!(estimate > (LIMIT as f64 * PROBABLY_FITS_MARGIN) as u64);
        assert_eq!(
            classify_probed(size, Some(&p), "mp4", MediaMode::Compress, &CompressOptions::default(), LIMIT),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn a_file_the_media_step_cannot_touch_says_so() {
        assert_eq!(
            classify_probed(80 * 1024 * 1024, None, "pdf", MediaMode::Convert, &CompressOptions::default(), LIMIT),
            SizeVerdict::CannotProcess
        );
    }

    #[test]
    fn gif_is_never_processed_so_it_is_judged_on_its_own_size() {
        // process_one skips GIF in both modes. Its size will not change.
        assert_eq!(
            classify_probed(80 * 1024 * 1024, None, "gif", MediaMode::Convert, &CompressOptions::default(), LIMIT),
            SizeVerdict::ProbablyTooBig
        );
        assert_eq!(
            classify_probed(1024, None, "gif", MediaMode::Convert, &CompressOptions::default(), LIMIT),
            SizeVerdict::FitsAsIs
        );
    }

    #[test]
    fn the_estimate_is_not_capped_at_the_original_size() {
        // Decision 12 says so in as many words. A cap would erase MayGrow.
        let p = probe("hevc", 4032, 3024, None);
        let size = 10 * 1024 * 1024;
        assert!(
            estimate_bytes(size, Some(&p), "heic", MediaMode::Convert, &CompressOptions::default()) > size
        );
    }

    #[test]
    fn a_file_in_the_band_is_worth_probing_and_a_small_one_is_not() {
        assert!(!needs_probe(1024, LIMIT));
        assert!(needs_probe(30 * 1024 * 1024, LIMIT));
        assert!(needs_probe(900 * 1024 * 1024, LIMIT));
    }
}
```

- [ ] **Step 6: Run them and watch them fail**

```bash
cargo test -p media --lib estimate
```

Expected: FAIL to compile.

- [ ] **Step 7: Write the estimate**

`crates/libs/media/src/estimate.rs`:

```rust
//! Forecast what the media step will do to a staged file's size.
//!
//! Every number here is an estimate and the screen says so. The point is not
//! precision: it is telling the difference between a file that will comfortably
//! fit, one that will not, and one that is fine now and will not be afterwards.

use crate::{CompressOptions, MediaMode, MediaProbe};

/// Files smaller than this fraction of the limit are not probed.
///
/// The largest growth factor in [`format_factor`] is well under 2.5×, so a file
/// this far below the limit cannot cross it, and probing every thumbnail in a
/// backup costs more than the answer is worth (decision 13).
const PROBE_BAND_FLOOR: f64 = 0.4;

/// An over-limit file whose estimate lands above this fraction of the limit
/// reads as probably still too big rather than likely to fit.
///
/// The margin is what stops a near miss from reading as a promise (decision 13).
pub const PROBABLY_FITS_MARGIN: f64 = 0.8;

/// How a staged attachment is expected to land against the size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeVerdict {
    /// Under the limit now, and expected to stay under.
    FitsAsIs,
    /// Over the limit now, expected to come under after the media step.
    LikelyFits,
    /// Under the limit now, expected to cross it during the media step.
    MayGrow,
    /// Over the limit now, and expected to stay over.
    ProbablyTooBig,
    /// The media step does not handle this kind of file, so its size is fixed.
    CannotProcess,
}

/// Is this file worth an ffprobe call?
#[must_use]
pub fn needs_probe(size_bytes: u64, limit_bytes: u64) -> bool {
    size_bytes as f64 >= limit_bytes as f64 * PROBE_BAND_FLOOR
}

/// Estimated size after the media step, in bytes. Never capped at the original.
#[must_use]
pub fn estimate_bytes(
    size_bytes: u64,
    probe: Option<&MediaProbe>,
    ext: &str,
    mode: MediaMode,
    compress: &CompressOptions,
) -> u64 {
    let factor = format_factor(ext, probe, mode);
    let scale = match (probe, mode) {
        // Only compress scales video. convert_video re-encodes at the source
        // resolution, so its size change is entirely the format's doing.
        (Some(p), MediaMode::Compress) if p.fps.is_some() => {
            pixel_ratio(p, compress) * fps_ratio(p, compress)
        }
        _ => 1.0,
    };
    (size_bytes as f64 * scale * factor).round() as u64
}

/// Classify one file, probing it first when it is close enough to matter.
#[must_use]
pub fn classify_probed(
    size_bytes: u64,
    probe: Option<&MediaProbe>,
    ext: &str,
    mode: MediaMode,
    compress: &CompressOptions,
    limit_bytes: u64,
) -> SizeVerdict {
    if !processable(ext) {
        return size_only(size_bytes, limit_bytes, SizeVerdict::CannotProcess);
    }
    if untouched_by(ext, mode) {
        return size_only(size_bytes, limit_bytes, SizeVerdict::ProbablyTooBig);
    }
    if !needs_probe(size_bytes, limit_bytes) {
        return SizeVerdict::FitsAsIs;
    }
    let estimate = estimate_bytes(size_bytes, probe, ext, mode, compress);
    if size_bytes <= limit_bytes {
        return if estimate > limit_bytes {
            SizeVerdict::MayGrow
        } else {
            SizeVerdict::FitsAsIs
        };
    }
    if (estimate as f64) <= limit_bytes as f64 * PROBABLY_FITS_MARGIN {
        SizeVerdict::LikelyFits
    } else {
        SizeVerdict::ProbablyTooBig
    }
}

/// A file whose size the media step will not change is judged on that size.
fn size_only(size_bytes: u64, limit_bytes: u64, over: SizeVerdict) -> SizeVerdict {
    if size_bytes <= limit_bytes {
        SizeVerdict::FitsAsIs
    } else {
        over
    }
}
```

`processable(ext)` mirrors `process::classify` — the same three extension lists,
and `false` for anything else. Do not duplicate the lists: make `classify` and
its `Kind` visible to this module (`pub(crate)`) and call it, so a new extension
added to the media pass cannot be missed by the forecast.

`untouched_by(ext, mode)` mirrors the early returns in `process_one`: GIF in
either mode, JPEG in `Convert`, MP3 in `Convert`. It must not include the
500 KB / 100 KB size gates — those files are under the limit anyway, and
duplicating a threshold is how the two drift apart.

`pixel_ratio` and `fps_ratio` clamp at 1.0, because compression never scales a
file up:

```rust
fn pixel_ratio(probe: &MediaProbe, compress: &CompressOptions) -> f64 {
    let source_long = f64::from(probe.width.max(probe.height));
    if source_long <= 0.0 {
        return 1.0;
    }
    let target_long = f64::from(compress.max_resolution.max_long_edge());
    let ratio = (target_long / source_long).min(1.0);
    ratio * ratio
}

fn fps_ratio(probe: &MediaProbe, compress: &CompressOptions) -> f64 {
    let Some(source) = probe.fps.filter(|f| *f > 0.0) else {
        return 1.0;
    };
    let target = if compress.max_fps > 0.0 { compress.max_fps } else { 30.0 };
    f64::from(target / source).min(1.0)
}
```

`format_factor` is the calibration table. These are estimates, and the screen
says so throughout; they are grouped in one place so they can be adjusted from
real imports without hunting through the logic:

```rust
/// Size change from the format alone, holding pixels and frame rate fixed.
///
/// Above 1.0 means the target format is bulkier than the source — the case
/// decision 12 exists to catch, and the common one on an iPhone backup.
fn format_factor(ext: &str, probe: Option<&MediaProbe>, mode: MediaMode) -> f64 {
    let compressing = matches!(mode, MediaMode::Compress);
    match ext {
        // Apple stills. HEIC is roughly half an equivalent JPEG, so it grows.
        "heic" | "heif" => 1.8,
        // Lossless and near-lossless stills re-encoded to JPEG.
        "png" | "tif" | "tiff" | "bmp" => 1.3,
        "webp" => 1.2,
        // Already JPEG: only compress touches it, at -q:v 5.
        "jpg" | "jpeg" => 0.7,
        // Already MP3: only compress touches it, at 96k mono.
        "mp3" => 0.6,
        // Anything else to MP3.
        "m4a" | "aac" | "caf" | "amr" | "wav" | "ogg" | "opus" => 0.8,
        // Video: the codec decides, not the container.
        _ => match probe.map(|p| p.codec.as_str()) {
            Some("hevc" | "vp9" | "av1") => 1.4,
            Some(_) if compressing => 0.7,
            _ => 1.0,
        },
    }
}
```

Declare both modules and re-export in `crates/libs/media/src/lib.rs`:

```rust
mod estimate;
mod probe;

pub use estimate::{PROBABLY_FITS_MARGIN, SizeVerdict, classify_probed, estimate_bytes, needs_probe};
pub use probe::{MediaProbe, probe_media};
```

`serde` is already a dependency of this crate; if it is not, add it with the
`derive` feature to `crates/libs/media/Cargo.toml`.

- [ ] **Step 8: Run the tests**

```bash
cargo test -p media && cargo clippy -p media --all-targets -- -D warnings
```

Expected: PASS with no new clippy warnings. If a calibration constant makes a
test fail, the test case is the requirement — adjust the constant, not the case.

- [ ] **Step 9: Commit**

```bash
git add crates/libs/media/src/probe.rs crates/libs/media/src/estimate.rs crates/libs/media/src/lib.rs crates/libs/media/Cargo.toml
git commit -m "feat(media): forecast a file's size after the media step"
```

---

### Task 3: Convert the staged folder as a separate, resumable pass

This is the task the phase exists for. Conversion becomes a second pass over a staging folder that already holds conversation files, so there is a moment between staging and converting where the import can stop and ask.

Decision 27: the pass patches the conversation files afterward, updating four fields per attachment — path, `digest_sha256`, `size_bytes`, and mime. The digest matters because the vault dedupes assets by sha256.

Decision 28: it commits per file, through a rename. For each attachment — transcode to `<derivative>.in_progress`, patch the conversation file, rename `.in_progress` to the final name, delete the original. The final name never exists until the conversation file already points at it. Two invariants fall out, and they are the whole of resume: a file under its final derivative name is fully patched, and an original still on disk means work remains. There is no state to classify and no progress to record. Reversing the order — deleting an original before its conversation file commits — leaves conversation files pointing at bytes that no longer exist.

Decision 28 also settles the interrupted file: a resumed run always re-transcodes it rather than adopting the `.in_progress` bytes. A crash during the write leaves a truncated file, and nothing distinguishes a complete `.in_progress` from a partial one without hashing it. The cost is one file.

Decision 29: the patch reads the file on disk and never replays a captured remap. ffmpeg output is not guaranteed byte-identical across runs, so a re-transcoded file can carry a different sha256, and writing a stale digest would corrupt silently.

Decision 45: a file that crosses the size limit during the media step is skipped, not reverted. It becomes `too_large`, the message keeps its text and a placeholder. Falling back to the original would store a file in the format the user asked to be rid of, which is worse than storing nothing.

**Accepted cost, stated so it is not mistaken for a defect:** where the keep-smaller guard from Task 1 keeps the original, no on-disk signal records that the file was considered, so a resumed run re-tries it. That applies only to same-format re-encodes in `compress` mode — JPEGs over 500 KB and MP3s over 100 KB that did not shrink. Recording it would mean a progress record, which decision 4 rules out.

**Files:**
- Create: `crates/libs/ir-format/src/transcode.rs`
- Modify: `crates/libs/ir-format/src/lib.rs` (module and re-exports)
- Modify: `crates/libs/ir-format/src/write.rs` (public exact-path JSONL writer)
- Modify: `crates/core/message-vault-io-core/src/attachment_jobs.rs` (`mime_for_rel` becomes `pub`)
- Test: `crates/libs/ir-format/src/transcode.rs`

**Interfaces:**
- Consumes: `media::derivative_name`, `media::transcode_file`, `media::TranscodeOutcome` from Task 1.
- Produces: `message_ir_format::{TranscodeOptions, TranscodeProgress, TranscodeReport, transcode_staged}`. Task 5 is the only caller.

- [ ] **Step 1: Add the exact-path JSONL writer**

`write_conversation_jsonl` derives its output path from `doc.filename_stem()`. A patch must be written back to the file it was read from, not to wherever the stem happens to point — otherwise a document whose stem no longer matches its filename silently forks into a second file.

In `crates/libs/ir-format/src/write.rs`, add the public function and make the existing one delegate:

```rust
/// Write `doc` as JSON Lines to exactly `path`, atomically.
///
/// Unlike the export writers this does not derive the file name from the
/// document: a caller patching a file it already read must write back to the
/// same path. The write goes through a `.tmp` sibling and a rename, so a
/// reader never sees a half-written conversation.
///
/// # Errors
///
/// Returns an error when the file cannot be created, serialized, or renamed.
pub fn write_conversation_jsonl_to(path: &Path, doc: &ConversationDocument) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut tmp = path.to_path_buf();
    tmp.set_extension("jsonl.tmp");
    {
        let file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        let mut file = BufWriter::new(file);
        let header = ConversationHeader::from_document(doc);
        serde_json::to_writer(&mut file, &header).context("serialize JSONL header")?;
        file.write_all(b"\n")?;
        for msg in &doc.messages {
            serde_json::to_writer(&mut file, msg).context("serialize JSONL message")?;
            file.write_all(b"\n")?;
        }
        file.flush()
            .with_context(|| format!("flush {}", tmp.display()))?;
    }
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(())
}

/// First JSON Lines line: schema, export, and conversation metadata (no messages).
fn write_conversation_jsonl(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    let path = output_dir.join(format!("{}.jsonl", doc.filename_stem()));
    write_conversation_jsonl_to(&path, doc)?;
    Ok(path)
}
```

Export it from `crates/libs/ir-format/src/lib.rs`:

```rust
pub use write::{CSV_HEADERS, document_to_mail_messages, write_conversation_jsonl_to};
```

Run `cargo test -p message-ir-format` — the existing export tests cover the
delegating path, so a regression in the shared write logic shows up there.

- [ ] **Step 2: Write the failing tests for the pass**

Create `crates/libs/ir-format/src/transcode.rs` with its test module. These cases are the invariants, not illustrations.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A staging folder holding one conversation and one attachment.
    ///
    /// Writes `attachments/<name>` with `bytes`, and one `.jsonl` whose single
    /// message has non-empty text and one attachment pointing at
    /// `attachments/<name>`. Build the document with `ConversationDocument`
    /// directly and write it with `write_conversation_jsonl_to`, so the fixture
    /// and the code under test agree on the on-disk shape.
    ///
    /// Returns (staging dir, conversation file path, attachment path).
    fn staged_one(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf)

    fn options(mode: MediaMode, limit: u64) -> TranscodeOptions {
        TranscodeOptions {
            mode,
            compress: CompressOptions::default(),
            asset_max_bytes: limit,
        }
    }

    #[test]
    fn a_converted_attachment_is_patched_before_its_final_name_exists() {
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let report = transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), None, &mut |_| {})
            .unwrap();

        assert_eq!(report.converted, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        assert_eq!(att.path.as_deref(), Some("attachments/photo.jpg"));
        assert!(!original.exists(), "original deleted after the patch committed");
        assert!(dir.path().join("attachments/photo.jpg").exists());
        assert!(
            std::fs::read_dir(dir.path().join("attachments"))
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".in_progress")),
            "no marker survives a completed file"
        );
    }

    #[test]
    fn the_digest_and_size_are_recomputed_from_the_derivative() {
        if !ffmpeg_available() {
            return;
        }
        // Decision 29: ffmpeg output is not byte-identical across runs, so a
        // replayed digest would be a silent corruption — the vault dedupes
        // assets by sha256.
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), None, &mut |_| {}).unwrap();

        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let att = &doc.messages[0].attachments[0];
        let derivative = dir.path().join("attachments/photo.jpg");
        let on_disk = std::fs::read(&derivative).unwrap();
        assert_eq!(att.digest_sha256.as_deref(), Some(hex_sha256(&on_disk).as_str()));
        assert_eq!(att.size_bytes, Some(on_disk.len() as u64));
        assert_eq!(att.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn an_interrupted_file_is_re_transcoded_not_adopted() {
        if !ffmpeg_available() {
            return;
        }
        // Decision 28: nothing distinguishes a complete .in_progress from a
        // truncated one without hashing it, so the marker's bytes are never used.
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        let marker = dir.path().join("attachments/photo.jpg.in_progress");
        std::fs::write(&marker, b"truncated garbage from a killed run").unwrap();

        transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), None, &mut |_| {}).unwrap();

        let derivative = dir.path().join("attachments/photo.jpg");
        assert_ne!(
            std::fs::read(&derivative).unwrap(),
            b"truncated garbage from a killed run".to_vec(),
            "the marker's bytes must never be adopted"
        );
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(doc.messages[0].attachments[0].path.as_deref(), Some("attachments/photo.jpg"));
    }

    #[test]
    fn an_already_converted_attachment_is_left_alone_on_a_second_run() {
        if !ffmpeg_available() {
            return;
        }
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), None, &mut |_| {}).unwrap();
        let after_first = std::fs::read(dir.path().join("attachments/photo.jpg")).unwrap();

        let second = transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), None, &mut |_| {})
            .unwrap();

        assert_eq!(second.converted, 0, "resume must not redo finished work");
        assert_eq!(
            std::fs::read(dir.path().join("attachments/photo.jpg")).unwrap(),
            after_first
        );
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(doc.messages[0].attachments[0].path.as_deref(), Some("attachments/photo.jpg"));
    }

    #[test]
    fn a_derivative_over_the_limit_becomes_too_large_and_keeps_the_message() {
        if !ffmpeg_available() {
            return;
        }
        // Decision 45: skipped, not reverted. Falling back to the original
        // would store the format the user asked to be rid of.
        let (dir, jsonl, original) = staged_one("photo.png", &test_png_bytes());
        let report =
            transcode_staged(dir.path(), &options(MediaMode::Convert, 1), None, &mut |_| {}).unwrap();

        assert_eq!(report.too_large, 1);
        assert_eq!(report.converted, 0);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let msg = &doc.messages[0];
        assert!(!msg.text.is_empty(), "the message keeps its text");
        let att = &msg.attachments[0];
        assert_eq!(att.missing_reason.as_deref(), Some("too_large"));
        assert_eq!(att.path, None, "nothing to upload");
        assert!(!original.exists(), "the original is not kept as a fallback");
        assert!(!dir.path().join("attachments/photo.jpg").exists());
    }

    #[test]
    fn a_conversion_failure_becomes_a_per_item_reason_carrying_the_detail() {
        let (dir, jsonl, original) = staged_one("broken.png", b"not a png at all");
        let report =
            transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), None, &mut |_| {});

        // ffmpeg failing on one file is an issue, never a failed pass.
        let report = report.unwrap();
        assert_eq!(report.failed, 1);
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        let reason = doc.messages[0].attachments[0].missing_reason.clone().unwrap();
        assert!(
            reason.starts_with("convert_failed: "),
            "reason must stay inside the closed set: {reason}"
        );
        assert!(reason.len() > "convert_failed: ".len(), "the detail must survive");
        assert!(original.exists(), "a file that failed to convert is still there");
    }

    #[test]
    fn cancelling_stops_the_pass_without_corrupting_the_folder() {
        let (dir, jsonl, _) = staged_one("photo.png", &test_png_bytes());
        let cancel = CancelFlag::default();
        cancel.store(true, std::sync::atomic::Ordering::SeqCst);

        let err = transcode_staged(dir.path(), &options(MediaMode::Convert, u64::MAX), Some(&cancel), &mut |_| {});

        assert!(err.is_err());
        let doc = read_conversation_jsonl(&jsonl).unwrap();
        assert_eq!(
            doc.messages[0].attachments[0].path.as_deref(),
            Some("attachments/photo.png"),
            "an untouched attachment still points at its original"
        );
    }

    #[test]
    fn progress_counts_the_work_it_actually_has() {
        let (dir, _, _) = staged_one("notes.pdf", b"%PDF-1.4");
        let mut seen = Vec::new();
        let report = transcode_staged(
            dir.path(),
            &options(MediaMode::Convert, u64::MAX),
            None,
            &mut |p| seen.push((p.done, p.total)),
        )
        .unwrap();
        // A file the media step does not handle is not work.
        assert_eq!(report.converted, 0);
        assert!(seen.iter().all(|(_, total)| *total == 0));
    }
}
```

- [ ] **Step 3: Run them and watch them fail**

```bash
cargo test -p message-ir-format transcode
```

Expected: FAIL to compile — the module does not exist.

- [ ] **Step 4: Write the pass**

`crates/libs/ir-format/src/transcode.rs`.

**Why this crate and not `message-vault-io-core`:** the pass needs both the IR
JSONL reader/writer and the media crate. `message-ir-format` already depends on
`media`, `message-ir` and `message-vault-io-core`
(`crates/libs/ir-format/Cargo.toml`), so everything is in reach. Putting it in
`message-vault-io-core` instead would mean core depending on `ir-format`, which
already depends on core — a cycle that will not compile. `ir-format` uses
`anyhow` throughout, so this module does too, and `src-tauri` maps to `String`
at its edge exactly as it already does for `process_attachments_dir`.

```rust
//! Convert or compress a staged folder, patching the conversation files it wrote.
//!
//! This runs after the staging folder is complete and before anything is
//! uploaded, so the import can stop and ask between the two. It commits one
//! attachment at a time through a rename, which is what makes it resumable
//! with no progress record: a file under its final derivative name is fully
//! patched, and an original still on disk means work remains.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use media::{CompressOptions, MediaMode, TranscodeOutcome};
use message_ir::ConversationDocument;
use message_vault_io_core::{CancelFlag, check_cancel, mime_for_rel};

use crate::read_json::read_conversation_jsonl;
use crate::write::write_conversation_jsonl_to;

/// Suffix on a derivative that is written but not yet committed.
///
/// Named so it survives the media crate's ffmpeg-scratch sweep, which matches
/// `.msgmedia.tmp.` — deleting this file would delete the resume signal.
const IN_PROGRESS_SUFFIX: &str = ".in_progress";

/// What the media pass should do.
#[derive(Debug, Clone)]
pub struct TranscodeOptions {
    /// Convert or Compress. Clone and Disabled make the pass a no-op.
    pub mode: MediaMode,
    /// Video targets, from the import form.
    pub compress: CompressOptions,
    /// A derivative larger than this is dropped rather than uploaded.
    pub asset_max_bytes: u64,
}

/// How far the pass has got.
#[derive(Debug, Clone, Copy)]
pub struct TranscodeProgress {
    /// Files finished, however they finished.
    pub done: usize,
    /// Files the pass found work for.
    pub total: usize,
}

/// What the pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscodeReport {
    /// Attachments replaced by a derivative.
    pub converted: usize,
    /// Attachments the media step left alone.
    pub skipped: usize,
    /// Derivatives that came out over the size limit and were dropped.
    pub too_large: usize,
    /// Attachments ffmpeg could not process.
    pub failed: usize,
    /// Total bytes of the originals the pass replaced.
    pub bytes_before: u64,
    /// Total bytes of the derivatives it wrote.
    pub bytes_after: u64,
}

/// Convert or compress every original still staged under `staging_dir`.
///
/// Safe to call again after an interruption: it re-reads the folder and does
/// whatever is left.
///
/// # Errors
///
/// Returns an error when the folder cannot be read, a conversation file cannot
/// be parsed or written, or the pass is cancelled. A single attachment ffmpeg
/// cannot process is an item-level issue recorded in the report, never an error.
pub fn transcode_staged(
    staging_dir: &Path,
    options: &TranscodeOptions,
    cancel: Option<&CancelFlag>,
    on_progress: &mut dyn FnMut(TranscodeProgress),
) -> Result<TranscodeReport> {
    if matches!(options.mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok(TranscodeReport::default());
    }
    let files = conversation_files(staging_dir)?;
    // Counting up front costs a second parse of each conversation file and
    // buys an honest progress total. Decision 31 accepts the re-read.
    let total = count_remaining(staging_dir, &files, options.mode)?;
    on_progress(TranscodeProgress { done: 0, total });

    let mut report = TranscodeReport::default();
    let mut done = 0usize;
    for jsonl in &files {
        check_cancel(cancel)?;
        let mut doc = read_conversation_jsonl(jsonl)?;
        let work = pending_in(staging_dir, &doc, options.mode);
        for (msg_idx, att_idx, src) in work {
            check_cancel(cancel)?;
            apply_one(staging_dir, jsonl, &mut doc, msg_idx, att_idx, &src, options, &mut report)?;
            done += 1;
            on_progress(TranscodeProgress { done, total });
        }
    }
    Ok(report)
}
```

`conversation_files` lists `*.jsonl` in `staging_dir`, sorted, non-recursive —
the sink writes them flat (`FormatSink::open_prepared`).

`pending_in` returns `(message index, attachment index, absolute source path)`
for every attachment whose file is still on disk under its recorded path and
for which `media::derivative_name` returns `Some`. An attachment whose file is
gone has already been committed; an attachment `derivative_name` declines is not
work. It resolves paths as `staging_dir.join(rel)` after rejecting any relative
path that escapes the folder — reuse `message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX`
or the same check `attachment_jobs.rs` applies.

`count_remaining` is `pending_in` summed over every file, and nothing else.

`apply_one` is the commit protocol, and its ordering is the task:

```rust
/// Transcode one attachment and commit it, in the order decision 28 fixes:
/// derivative written, conversation file patched, derivative renamed into its
/// final name, original deleted. Reversing any pair leaves the folder lying
/// about itself.
#[allow(clippy::too_many_arguments)]
fn apply_one(
    staging_dir: &Path,
    jsonl: &Path,
    doc: &mut ConversationDocument,
    msg_idx: usize,
    att_idx: usize,
    src: &Path,
    options: &TranscodeOptions,
    report: &mut TranscodeReport,
) -> Result<()> {
    let Some(name) = media::derivative_name(src, options.mode) else {
        report.skipped += 1;
        return Ok(());
    };
    let attachments_dir = staging_dir.join("attachments");
    let final_path = attachments_dir.join(&name);
    let marker = attachments_dir.join(format!("{name}{IN_PROGRESS_SUFFIX}"));
    let original_len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);

    match media::transcode_file(src, &marker, options.mode, &options.compress) {
        Err(err) => {
            // The closed reason set, with the detail that only this reason
            // carries. Phase 1 made the display side keep it.
            let detail = err.to_string();
            set_missing(doc, msg_idx, att_idx, &format!("convert_failed: {detail}"), None);
            write_conversation_jsonl_to(jsonl, doc)?;
            report.failed += 1;
            Ok(())
        }
        Ok(TranscodeOutcome::Skipped) => {
            report.skipped += 1;
            Ok(())
        }
        Ok(TranscodeOutcome::Produced) => {
            let produced_len = std::fs::metadata(&marker)
                .with_context(|| format!("stat {}", marker.display()))?
                .len();
            if produced_len > options.asset_max_bytes {
                // Decision 45: skipped, not reverted.
                set_missing(doc, msg_idx, att_idx, "too_large", Some(produced_len));
                write_conversation_jsonl_to(jsonl, doc)?;
                let _ = std::fs::remove_file(&marker);
                let _ = std::fs::remove_file(src);
                report.too_large += 1;
                return Ok(());
            }
            // Decision 29: read the file on disk. A replayed digest can be
            // stale, and the vault dedupes assets by sha256.
            let digest = media::file_sha256(&marker)?;
            {
                let att = attachment_at(doc, msg_idx, att_idx)?;
                att.path = Some(format!("attachments/{name}"));
                att.digest_sha256 = Some(digest);
                att.size_bytes = Some(produced_len);
                att.mime_type = mime_for_rel(&format!("attachments/{name}"));
                att.missing_reason = None;
            }
            write_conversation_jsonl_to(jsonl, doc)?;
            std::fs::rename(&marker, &final_path)
                .with_context(|| format!("commit {}", final_path.display()))?;
            let _ = std::fs::remove_file(src);
            report.converted += 1;
            report.bytes_before += original_len;
            report.bytes_after += produced_len;
            Ok(())
        }
    }
}
```

`set_missing` clears `path` and `digest_sha256`, sets `missing_reason`, and sets
`size_bytes` to the value given. `attachment_at` indexes the document and
returns an error rather than panicking on an index the caller built —
the indices come from `pending_in` against the same document, so a failure here
is a bug, not an input.

Make `mime_for_rel` `pub` in `message-vault-io-core`'s `attachment_jobs.rs` and
re-export it from that crate's `lib.rs`, rather than writing a second
extension-to-mime table that can drift from the first.

Declare and re-export in `crates/libs/ir-format/src/lib.rs`:

```rust
mod transcode;

pub use transcode::{TranscodeOptions, TranscodeProgress, TranscodeReport, transcode_staged};
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p message-ir-format
```

Expected: PASS. The ffmpeg-gated cases skip cleanly where the tools are absent;
`a_conversion_failure_becomes_a_per_item_reason_carrying_the_detail`,
`cancelling_stops_the_pass_without_corrupting_the_folder` and
`progress_counts_the_work_it_actually_has` need no ffmpeg and must pass everywhere.

- [ ] **Step 6: Check the whole workspace still builds**

```bash
cargo build --workspace && cargo test --workspace
```

Expected: PASS. Nothing outside this task calls the new pass yet, and
`process_attachments_dir` is untouched, so every existing exporter behaves
exactly as before.

- [ ] **Step 7: Commit**

```bash
git add crates/libs/ir-format/src/transcode.rs crates/libs/ir-format/src/write.rs crates/libs/ir-format/src/lib.rs crates/core/message-vault-io-core/src/attachment_jobs.rs crates/core/message-vault-io-core/src/lib.rs
git commit -m "feat(core): convert a staged folder as a separate resumable pass"
```

---

### Task 4: Read a staged folder back into an exact summary

Gate 1's numbers are measured, not remembered. Decision 39: the summary is recomputed from the folder on every visit, never read back from `summary_json` — the folder is the truth, and `summary_json` records what the user approved, which is a different question. That is also what makes resuming at a gate work: reopening the session recomputes rather than restoring.

Decision 11 fixes what it reports: conversations, messages, contacts (with how many match no contact in the vault), attachments, bytes copied, and a breakdown of files by size verdict. Everything except the verdicts is exact. The contact matching is not done here — the vault answers that, and Task 6 builds the endpoint — so this returns the distinct identifiers and lets the caller ask.

**Files:**
- Create: `crates/libs/ir-format/src/staging_summary.rs`
- Modify: `crates/libs/ir-format/src/lib.rs`
- Test: `crates/libs/ir-format/src/staging_summary.rs`

**Interfaces:**
- Consumes: `media::{classify_probed, estimate_bytes, needs_probe, probe_media, SizeVerdict}` from Task 2.
- Produces: `message_ir_format::{StagingSummary, AttachmentForecast, SummaryProgress, summarize_staging}`. Task 5 is the only caller; the types cross the Tauri boundary, so every one of them derives `Serialize` with `#[serde(rename_all = "camelCase")]` to match how `PathStat` already crosses.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_conversations_messages_and_distinct_contacts() {
        let dir = staged_fixture(); // two conversations, one shared participant
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.conversations, 2);
        assert_eq!(summary.messages, 5);
        assert_eq!(
            summary.contact_identifiers,
            vec!["+15550100".to_string(), "+15550101".to_string()],
            "sorted and de-duplicated across conversations"
        );
    }

    #[test]
    fn attachment_bytes_are_measured_on_disk_not_read_from_the_document() {
        // size_bytes in the document is what the writer recorded. The folder is
        // the truth, and a resumed run must not trust a stale field.
        let dir = staged_fixture();
        let attachment = dir.path().join("attachments/photo.png");
        std::fs::write(&attachment, vec![7u8; 4096]).unwrap();
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.attachment_bytes, 4096);
    }

    #[test]
    fn an_attachment_that_is_already_missing_is_counted_but_not_forecast() {
        let dir = staged_fixture_with_missing_reason("not_copied");
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.attachments, 1);
        assert_eq!(summary.attachment_bytes, 0);
        assert!(summary.forecasts.is_empty(), "nothing to forecast about a file that is not there");
    }

    #[test]
    fn only_files_worth_reporting_get_a_forecast_row() {
        // Every attachment is classified; a row is returned only where the
        // verdict is something other than "fits as-is", because that is the
        // whole content of the report. The counts cover the rest.
        let dir = staged_fixture_with_sizes(&[("small.png", 1024), ("huge.png", 900 * 1024 * 1024)]);
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.verdict_counts.fits_as_is, 1);
        assert_eq!(summary.verdict_counts.probably_too_big, 1);
        assert_eq!(summary.forecasts.len(), 1);
        assert_eq!(summary.forecasts[0].name, "huge.png");
    }

    #[test]
    fn copy_and_skip_modes_forecast_nothing_because_nothing_will_change() {
        // There is no media step under these modes, so every file is judged on
        // the size it already has and no probing happens at all.
        let dir = staged_fixture_with_sizes(&[("huge.png", 900 * 1024 * 1024)]);
        let mut options = summary_options();
        options.mode = MediaMode::Clone;
        let summary = summarize_staging(dir.path(), &options, &mut |_| {}).unwrap();
        assert_eq!(summary.verdict_counts.probably_too_big, 1);
        assert_eq!(summary.forecasts[0].estimate_bytes, summary.forecasts[0].size_bytes);
    }

    #[test]
    fn a_folder_with_no_conversation_files_is_an_empty_summary_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let summary = summarize_staging(dir.path(), &summary_options(), &mut |_| {}).unwrap();
        assert_eq!(summary.conversations, 0);
        assert_eq!(summary.messages, 0);
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p message-ir-format staging_summary
```

Expected: FAIL to compile.

- [ ] **Step 3: Write the summary reader**

```rust
//! Recompute what a staged folder holds, for the approval gates.
//!
//! Everything here is measured from the folder. The one estimate is what the
//! media step will do to a file's size, and it is labelled as an estimate all
//! the way to the screen.

/// One attachment the user should see before approving.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentForecast {
    /// Relative path inside the staging folder.
    pub path: String,
    /// File name, for the screen.
    pub name: String,
    /// Bytes on disk now.
    pub size_bytes: u64,
    /// Bytes expected after the media step. Equal to `size_bytes` when there
    /// is no media step.
    pub estimate_bytes: u64,
    /// How it is expected to land against the limit.
    pub verdict: SizeVerdict,
}

/// How many attachments landed in each verdict.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictCounts {
    pub fits_as_is: usize,
    pub likely_fits: usize,
    pub may_grow: usize,
    pub probably_too_big: usize,
    pub cannot_process: usize,
}

/// What a staged folder holds.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagingSummary {
    pub conversations: usize,
    pub messages: u64,
    /// Distinct participant identifiers, sorted. The vault decides which of
    /// these it already knows.
    pub contact_identifiers: Vec<String>,
    /// Attachments referenced by the documents, including ones already marked
    /// missing.
    pub attachments: usize,
    /// Bytes on disk under `attachments/` for the files that are actually there.
    pub attachment_bytes: u64,
    pub verdict_counts: VerdictCounts,
    /// One row per attachment whose verdict is not `fits_as_is`.
    pub forecasts: Vec<AttachmentForecast>,
}
```

`summarize_staging(staging_dir, options, on_progress)` walks the same `*.jsonl`
list Task 3 walks, and for each document:

- `conversations += 1`, `messages += doc.messages.len()`.
- collects `doc.conversation.participants` identifiers into a `BTreeSet`.
- for each attachment: `attachments += 1`. If it has no `path`, or the file is
  not on disk, it contributes no bytes and gets no forecast — an attachment
  already carrying a `missing_reason` is settled, and forecasting it would
  invite the user to approve something that cannot happen.
- otherwise reads the file's length from disk, probes it when
  `media::needs_probe(len, limit)` and the mode has a media step, and calls
  `media::classify_probed`.

Under `Clone` and `Disabled` there is no media step: skip probing entirely,
set `estimate_bytes = size_bytes`, and classify on size alone. That is not an
optimization — probing would produce an estimate for work that will not run.

`on_progress` reports `SummaryProgress { done, total }` over attachments, so a
folder with tens of thousands of files does not look frozen. Emit at the same
cadence the media crate uses (`MEDIA_PROGRESS_EVERY`, every 100) plus a final
call.

The probe is best-effort: an ffprobe failure on one file means classifying it
without a probe, not failing the summary. A gate that cannot render because one
file is unreadable is worse than a gate with one rougher estimate.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p message-ir-format && cargo clippy -p message-ir-format --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/libs/ir-format/src/staging_summary.rs crates/libs/ir-format/src/lib.rs
git commit -m "feat(core): recompute a staged folder's summary for the approval gates"
```

---

### Task 5: Expose the pass and the summary to the desktop app

Two new Tauri commands, and the size limit stops being a magic number in one file.

**Files:**
- Create: `src-tauri/src/commands/staging.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (command registration)
- Modify: `src-tauri/src/commands/push.rs` (read the shared constant)
- Modify: `src-tauri/src/commands/extract.rs` (ask the exporter for `copy` under convert/compress)
- Test: `src-tauri/src/commands/staging.rs`, `src-tauri/src/commands/extract.rs`

**Interfaces:**
- Consumes: `message_ir_format::{summarize_staging, transcode_staged}` and their option/report types from Tasks 3 and 4.
- Produces: Tauri commands `summarize_staging(staging_dir, attachment_media, media_max_resolution, media_max_fps, media_min_size)` returning `StagingSummary`, `transcode_staging(...)` which spawns a job and reports through the existing `extract:*` events, and `delete_staging(staging_dir)` for the decline path. Also `pub const ASSET_MAX_BYTES: u64`. Task 7 wraps all three.

- [ ] **Step 1: Write the failing test for the exporter media mode**

The desktop gets the write/convert split by asking the exporter for `copy`
whenever the user chose `convert` or `compress`, then running the media pass
itself. Nothing in the shared crates changes, so no CLI or library caller is
affected.

In `src-tauri/src/commands/extract.rs`'s test module:

```rust
#[test]
fn convert_and_compress_stage_originals_and_defer_the_media_step() {
    // The desktop runs conversion as its own pass so a gate can sit in front
    // of it. Asking the exporter to convert would spend the time before the
    // user has approved anything.
    for chosen in [AttachmentMedia::Convert, AttachmentMedia::Compress] {
        let mut options = test_options(vec!["+15550100".into()]);
        options.attachment_media = chosen;
        let config = build_exporter_config("imessage-ios", "/backup", "/out", &options).unwrap();
        assert_eq!(
            exporter_media_mode(&config),
            MediaMode::Clone,
            "{chosen:?} must stage originals"
        );
    }
}

#[test]
fn copy_and_skip_reach_the_exporter_unchanged() {
    let mut options = test_options(vec!["+15550100".into()]);
    options.attachment_media = AttachmentMedia::Skip;
    let config = build_exporter_config("imessage-ios", "/backup", "/out", &options).unwrap();
    assert_eq!(exporter_media_mode(&config), MediaMode::Disabled);
}
```

`exporter_media_mode` is a small test helper reading the media mode back out of
the built `ExporterConfig`; follow how the existing tests in that module reach
into the config (`jailbreak_uses_macos_platform_and_attachment_root` is the
pattern).

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml convert_and_compress_stage_originals
```

Expected: FAIL — the exporter is still asked to convert.

- [ ] **Step 3: Defer the media step in `build_exporter_config`**

In `build_exporter_config`, map the requested media mode down for the exporter
only, leaving `options.attachment_media` untouched for the caller that runs the
pass afterwards:

```rust
/// Media mode the exporter is asked for.
///
/// Convert and Compress become Clone: the desktop stages originals, shows the
/// first gate, and runs the media pass itself, so the expensive work happens
/// after the user has approved it rather than before. Copy and Skip have no
/// media step and reach the exporter unchanged.
fn exporter_media_mode(chosen: AttachmentMedia) -> MediaMode {
    match chosen {
        AttachmentMedia::Convert | AttachmentMedia::Compress => MediaMode::Clone,
        other => other.media_mode(),
    }
}
```

Replace the `mode: options.attachment_media.media_mode()` at
`src-tauri/src/commands/extract.rs:482` with
`mode: exporter_media_mode(options.attachment_media)`, and do the same anywhere
else in that function the mode reaches an exporter's transforms. The `compress`
options built at `:455-462` are still forwarded — they are what the media pass
will use — but they no longer take effect during extract.

- [ ] **Step 4: Add the shared size limit**

In `src-tauri/src/commands/push.rs`, replace the literal at line 136:

```rust
/// Largest attachment the desktop app will upload.
///
/// The vault's own `asset_max_bytes` defaults higher and is not exposed to
/// clients, so this is the number the app can actually promise. The size
/// forecast at the first gate predicts against this same constant — a forecast
/// against a different limit than the upload uses would be worse than none.
pub const ASSET_MAX_BYTES: u64 = 50 * 1024 * 1024;
```

and use it for `asset_max_bytes`.

- [ ] **Step 5: Write the two commands**

`src-tauri/src/commands/staging.rs`:

- `summarize_staging` is a plain `async` command that returns
  `Result<StagingSummary, String>`. It builds `CompressOptions` from the same
  form fields `extract` parses (reuse `parse_max_resolution` and the fps/min-size
  parsing rather than re-deriving them), calls
  `message_ir_format::summarize_staging`, and emits progress on
  `extract:progress` with `step: "prepare"` so a long summary of a huge folder
  shows movement on the step the user is already looking at.
- `transcode_staging` follows the `extract` command's shape exactly: reset the
  cancel flag through `reset_and_clone_cancel`, then `spawn_job`, emitting
  `extract:log` lines, `extract:progress` with `step: "media"`, and a final
  `extract:finished` payload carrying the `TranscodeReport`. Per-item failures
  from the report are emitted as `extract:issue` with `kind: "skip"` and
  `step: "media"`, so they land in the same issues list every other step feeds.

- `delete_staging` removes a staging folder, for the decline path. Decision 16
  makes declining terminal: the session closes and the folder is deleted. It
  MUST refuse any path outside the configured staging root — reuse the guard
  `src-tauri/src/commands/paths.rs:113` already applies before opening a path,
  and fail loudly rather than silently doing nothing, so a bug here cannot turn
  into a recursive delete somewhere else on the disk.

Register all three in `src-tauri/src/commands/mod.rs` and in the `invoke_handler`
list wherever `extract` and `push` are registered.

- [ ] **Step 6: Test the delete guard before anything else can call it**

```rust
#[test]
fn delete_staging_refuses_a_path_outside_the_staging_root() {
    // This command deletes a directory tree. The only thing standing between
    // a path bug and someone's home folder is this check.
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("keep-me");
    std::fs::create_dir_all(&victim).unwrap();

    let err = delete_staging_dir(root.path(), &victim).unwrap_err();

    assert!(err.contains("staging"), "the refusal should say why: {err}");
    assert!(victim.exists());
}

#[test]
fn delete_staging_removes_a_folder_inside_the_root() {
    let root = tempfile::tempdir().unwrap();
    let staged = root.path().join("staging-run-1");
    std::fs::create_dir_all(staged.join("attachments")).unwrap();
    std::fs::write(staged.join("a.jsonl"), b"{}").unwrap();

    delete_staging_dir(root.path(), &staged).unwrap();

    assert!(!staged.exists());
}

#[test]
fn delete_staging_is_quiet_about_a_folder_that_is_already_gone() {
    // The decline path may run after a crash that already removed it.
    let root = tempfile::tempdir().unwrap();
    assert!(delete_staging_dir(root.path(), &root.path().join("never-existed")).is_ok());
}
```

- [ ] **Step 7: Verify**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src
git commit -m "feat(desktop): stage originals, then summarize and convert as their own steps"
```

---

### Task 6: Ask the vault which contacts it knows, and record a gate approval

Decision 11 has Gate 1 report contacts "with how many match no contact in the vault". No endpoint answers that today: `contacts_api.rs` can look up a handle for a *known* contact id (`find_contact_handle_id`), and can list or search contacts, but nothing takes a batch of raw identifiers and says which are new. Contact-name resolution during import runs server-side inside the import pipeline, too late for a gate.

The number matters because it is the one line on Gate 1 that tells the user whether this import is bringing in people they already have or a fresh set — a good signal that they picked the wrong backup.

**Files:**
- Modify: `crates/vault/server/src/contacts_api.rs`
- Modify: `crates/vault/server/src/import/mod.rs` (`StageImportBody` gains the approved plan)
- Modify: `crates/vault/server/src/db/vault_imports.rs` (`set_import_stage` writes `summary_json` when given one)
- Modify: `web/src/lib/importSession.ts` (`setImportStage` takes the plan)
- Modify: `crates/vault/server/src/openapi.rs` (route and schema registration)
- Modify: `docs/src/assets/openapi.json` (regenerated, same commit)
- Test: `crates/vault/server/src/contacts_api.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `POST /v1/contacts/match`, request `{ "identifiers": ["+15550100", "sam@example.com"] }`, response `{ "unknown": ["sam@example.com"] }` — Task 8 is the only caller. Also `setImportStage(id, stage, approvedPlan?)`, which Task 10 uses to record what was approved.

- [ ] **Step 1: Write the failing tests**

Follow the existing handler tests in this file for the account and auth
scaffolding — `register_via_api` plus the router-level request the neighbouring
tests use.

```rust
#[tokio::test]
async fn contact_match_reports_only_the_identifiers_the_vault_does_not_have() {
    let (app, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
    let body = json!({ "identifiers": ["+15550100", "+15550999"] });
    let response = post_json(&app, "/v1/contacts/match", &token, body).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.json::<Value>().await["unknown"], json!(["+15550999"]));
}

#[tokio::test]
async fn contact_match_ignores_blank_identifiers_and_de_duplicates() {
    let (app, token, _account) = contacts_fixture_with_handles(&[]).await;
    let body = json!({ "identifiers": ["+15550999", "  ", "+15550999", ""] });
    let response = post_json(&app, "/v1/contacts/match", &token, body).await;
    assert_eq!(response.json::<Value>().await["unknown"], json!(["+15550999"]));
}

#[tokio::test]
async fn contact_match_does_not_count_a_trashed_contact_as_known() {
    // A trashed contact is not in the user's vault as far as every other
    // screen is concerned, and saying "you already have this person" about
    // someone they deleted would be a lie.
    let (app, token, _account) = contacts_fixture_with_trashed_handle("+15550100").await;
    let body = json!({ "identifiers": ["+15550100"] });
    let response = post_json(&app, "/v1/contacts/match", &token, body).await;
    assert_eq!(response.json::<Value>().await["unknown"], json!(["+15550100"]));
}

#[tokio::test]
async fn contact_match_is_scoped_to_the_calling_account() {
    let (app, token, _mine) = contacts_fixture_with_handles(&[]).await;
    let _other = account_with_handle("+15550100").await;
    let body = json!({ "identifiers": ["+15550100"] });
    let response = post_json(&app, "/v1/contacts/match", &token, body).await;
    assert_eq!(response.json::<Value>().await["unknown"], json!(["+15550100"]));
}

#[tokio::test]
async fn contact_match_rejects_an_oversized_batch() {
    let (app, token, _account) = contacts_fixture_with_handles(&[]).await;
    let identifiers: Vec<String> = (0..MAX_MATCH_IDENTIFIERS + 1).map(|i| format!("+1555{i:06}")).collect();
    let response = post_json(&app, "/v1/contacts/match", &token, json!({ "identifiers": identifiers })).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p message-vault-server contact_match
```

Expected: FAIL — the route does not exist.

- [ ] **Step 3: Write the handler**

```rust
/// Most identifiers one request may ask about.
///
/// A staged folder can reference thousands of participants; the client
/// batches. The cap keeps one request's SQL bounded.
pub(crate) const MAX_MATCH_IDENTIFIERS: usize = 500;

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct ContactMatchBody {
    /// Raw identifiers — phone numbers, emails — as they appear in an export.
    identifiers: Vec<String>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub(crate) struct ContactMatchResponse {
    /// The subset this account has no contact for, in the order given, blanks
    /// dropped and duplicates collapsed.
    unknown: Vec<String>,
}
```

The handler trims each identifier, drops blanks, de-duplicates while preserving
first-seen order, rejects a batch over `MAX_MATCH_IDENTIFIERS` with 400, and
runs one query per batch against `contact_handles` joined to `contacts`, scoped
to the account and excluding trashed contacts with `NOT_TRASHED_CONTACT_SQL` —
the same predicate every other contacts query in this file uses.

Match on the same normalized form the import pipeline uses. `normalize_handle`
(or whatever `crates/vault/server/src/import/contact_name.rs` applies before
comparing) is the authority: matching on the raw string would report a contact
as unknown because the export wrote `+1 555 0100` and the vault stored
`+15550100`. Reuse that function rather than writing a second normalizer — if
it is private to the import module, make it `pub(crate)`.

Register the route beside the other contacts routes in `openapi.rs`, with both
new types in the schema list.

- [ ] **Step 4: Let a stage change carry the approved plan**

Decision 2 says `summary_json` carries the approved plan, and decision 15 diffs
the outcome against it. Nothing can write it mid-session today: `complete_import`
writes it at the end, and `POST /v1/imports/{id}/stage` takes only a stage. So an
approval that survives a reload has nowhere to live.

Give `StageImportBody` an optional `summary`, written to `summary_json` when
present and leaving the column untouched when absent:

```rust
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct StageImportBody {
    /// New stage for this session.
    stage: String,
    /// What the user approved at the gate they just passed, when they passed one.
    ///
    /// Recorded here rather than at completion so an approval survives a
    /// reload: the summary shown at a gate is recomputed from the folder, but
    /// what was approved is a different question and only the session remembers it.
    #[serde(default)]
    summary: Option<serde_json::Value>,
}
```

Test it both ways — a stage change carrying a summary stores it, and one without
leaves an existing `summary_json` alone rather than nulling it:

```rust
#[tokio::test]
async fn a_stage_change_without_a_summary_does_not_erase_the_stored_one() {
    // Most stage changes carry nothing. Treating absent as null would throw
    // away the plan the outcome is judged against.
    let (app, token, import_id) = session_with_summary(json!({"approved": true})).await;
    post_json(&app, &format!("/v1/imports/{import_id}/stage"), &token, json!({"stage": "pushing"})).await;
    assert_eq!(stored_summary(import_id).await, Some(json!({"approved": true})));
}
```

`web/src/lib/importSession.ts`'s `setImportStage` gains an optional second
argument for it.

- [ ] **Step 5: Run the tests and regenerate the dump**

```bash
cargo test -p message-vault-server contact_match
cargo run -q -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
cargo test -p message-vault-server committed_openapi_matches_dump
```

Expected: all PASS. The dump must be regenerated in this commit, never hand-edited.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/contacts_api.rs crates/vault/server/src/import/mod.rs crates/vault/server/src/db/vault_imports.rs crates/vault/server/src/openapi.rs docs/src/assets/openapi.json web/src/lib/importSession.ts
git commit -m "feat(vault): report unknown contact identifiers, and record a gate approval"
```

---

### Task 7: Teach the web client about the media step

The bridge work the gate screens sit on: the two new commands, a fourth progress step, and the step list that changes shape with the media mode.

Decision 8: the progress display shows four steps — Read backup, Copy to staging, Convert (or Compress) media, Upload to vault. Under `copy` and `skip` the media step is absent and there are three. Decision 18: the word "transcode" never reaches the screen.

Decision 20 also lands here. Every state is titled "Import Messages" today — the form's heading (`ImportFormFields.tsx:335`) and the progress view's (`ImportProgressView.tsx:34`) are literally the same string, which tells the user nothing about where they are. The progress heading becomes the stage it is on: **Reading your backup**, **Copying to staging**, **Converting media** (or **Compressing media**), **Uploading to your vault**. The form keeps "Import Messages" — that one is accurate.

**Watch out:** `stepIndexFor` (`web/src/screens/import/useImportJob.ts:83-88`) ends in `return 3`, so any progress step it does not recognise silently lands on Upload. Adding a `"media"` event without teaching it the mapping would draw conversion progress on the upload bar.

**Files:**
- Modify: `web/src/lib/types.ts` (`ImportProgressEvent["step"]`, `ImportIssueEvent["step"]`)
- Modify: `web/src/lib/tauri.ts` (two invoke wrappers and their config types)
- Modify: `web/src/screens/import/importProgressState.ts` (the step list, the index mapping, the heading)
- Modify: `web/src/screens/import/useImportJob.ts` (use the moved mapping)
- Modify: `web/src/screens/import/ImportProgressView.tsx` (heading names the stage)
- Test: `web/src/screens/import/importProgressState.test.ts`, `ImportProgressView.test.tsx`

**Interfaces:**
- Consumes: the Tauri commands from Task 5.
- Produces:
  - `invokeSummarizeStaging(config): Promise<StagingSummary>` and `invokeTranscodeStaging(config): Promise<void>` in `web/src/lib/tauri.ts`, with `StagingSummary`, `AttachmentForecast`, `VerdictCounts` and `SizeVerdict` mirroring Task 4's serde shapes in camelCase.
  - `stepsFor(mode: AttachmentMediaMode): ImportStep[]`, `stepIndexFor(step, mode): number`, and `progressHeading(steps: ImportStep[], phase: ImportPhase): string` from `importProgressState.ts`.
  Tasks 8–12 consume all of it.

- [ ] **Step 1: Write the failing tests**

```ts
describe("stepsFor", () => {
  it("shows a media step under convert and compress", () => {
    expect(stepsFor("convert").map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Convert media",
      "Upload to vault",
    ]);
    expect(stepsFor("compress")[2].label).toBe("Compress media");
  });

  it("has no media step under copy or skip", () => {
    // There is no media step in these modes, so a greyed-out row would be
    // promising work that will never run.
    expect(stepsFor("copy").map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Upload to vault",
    ]);
    expect(stepsFor("skip").map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Upload to vault",
    ]);
  });

  it("never says transcode", () => {
    for (const mode of ["copy", "convert", "compress", "skip"] as const) {
      for (const step of stepsFor(mode)) {
        expect(step.label.toLowerCase()).not.toContain("transcode");
      }
    }
  });
});

describe("stepIndexFor", () => {
  it("puts writing conversation files on the staging step", () => {
    // "prepare" is the pipeline's name for writing conversation files. From
    // the user's side that is part of staging, not a step of its own.
    expect(stepIndexFor("attachments", "convert")).toBe(1);
    expect(stepIndexFor("prepare", "convert")).toBe(1);
  });

  it("maps the media step to its own row, and upload after it", () => {
    expect(stepIndexFor("media", "convert")).toBe(2);
    expect(stepIndexFor("upload", "convert")).toBe(3);
  });

  it("shifts upload down when there is no media step", () => {
    expect(stepIndexFor("upload", "copy")).toBe(2);
  });

  it("never lands an unmapped step on upload by accident", () => {
    // The old mapping ended in `return 3`, so a step nobody had wired drew
    // its progress on the upload bar.
    expect(stepIndexFor("media", "copy")).toBe(-1);
  });
});
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && npx vitest run src/screens/import/importProgressState.test.ts
```

Expected: FAIL — neither function exists in that module.

- [ ] **Step 3: Write the failing heading tests**

```ts
describe("progressHeading", () => {
  it("names the stage the import is actually on", () => {
    const steps = stepsFor("convert");
    steps[0].status = "done";
    steps[1].status = "active";
    expect(progressHeading(steps, "progress")).toBe("Copying to staging");
    steps[1].status = "done";
    steps[2].status = "active";
    expect(progressHeading(steps, "progress")).toBe("Converting media");
  });

  it("says compressing when that is the job", () => {
    const steps = stepsFor("compress");
    steps[2].status = "active";
    expect(progressHeading(steps, "progress")).toBe("Compressing media");
  });

  it("never says transcode", () => {
    // Decision 18: it is a stage name, and the user never sees it.
    const steps = stepsFor("convert");
    for (let i = 0; i < steps.length; i += 1) {
      const marked = steps.map((s, j) => ({ ...s, status: j === i ? "active" : "pending" }));
      expect(progressHeading(marked, "progress").toLowerCase()).not.toContain("transcode");
    }
  });

  it("titles the finished screen by its outcome, not by a step", () => {
    expect(progressHeading(stepsFor("convert"), "done")).toBe("Import finished");
  });

  it("falls back to the first step rather than an empty heading", () => {
    // Nothing active yet, one render frame before the first event arrives.
    expect(progressHeading(stepsFor("convert"), "progress")).toBe("Reading your backup");
  });
});
```

- [ ] **Step 4: Write the step model**

Move the step list and the index mapping into `importProgressState.ts`, where
the other progress-shape logic already lives, and have `useImportJob.ts` import
them. `attachmentStepCopy` keeps its job of naming the staging step's detail;
the media step's label comes from the mode:

```ts
/** Media step label, or null when this mode has no media step. */
function mediaStepLabel(mode: AttachmentMediaMode): string | null {
  if (mode === "convert") return "Convert media";
  if (mode === "compress") return "Compress media";
  return null;
}
```

`stepIndexFor` returns `-1` for a step that has no row in this mode, and every
caller must treat `-1` as "no row to update" rather than indexing with it.

`progressHeading` reads the first `active` step, falling back to the first step
when none is active yet, and returns a fixed heading for `phase === "done"`. The
headings are the present-tense form of the step labels, so the two are edited
together and cannot drift into telling different stories. Replace the static
`<h1>Import Messages</h1>` at `ImportProgressView.tsx:34` with it.

- [ ] **Step 5: Add the two bridge wrappers**

In `web/src/lib/tauri.ts`, following `invokeExtract`'s shape exactly — the same
camelCase remap and nullish-coalescing. `invokeSummarizeStaging` returns the
summary directly (it is a plain command, not a job); `invokeTranscodeStaging`
returns `Promise<void>` and reports through the `extract:*` events like every
other long job, so `runTauriJob` drives it unchanged.

Add `"media"` to both `ImportProgressEvent["step"]` and `ImportIssueEvent["step"]`
in `web/src/lib/types.ts`, and mirror Task 4's serialized types.

- [ ] **Step 6: Verify**

```bash
cd web && npx tsc --noEmit && npm test && npm run lint
```

Expected: PASS. `tsc` is the point of this step — widening the step union makes
every exhaustive switch over it a compile error, and those are the call sites
that need the new case.

- [ ] **Step 7: Commit**

```bash
git add web/src/lib/types.ts web/src/lib/tauri.ts web/src/screens/import/importProgressState.ts web/src/screens/import/importProgressState.test.ts web/src/screens/import/useImportJob.ts web/src/screens/import/ImportProgressView.tsx web/src/screens/import/ImportProgressView.test.tsx
git commit -m "feat(web): give the media step its own row, and name the stage on screen"
```

---

### Task 8: Gate 1 — review what was copied

The first gate approves **spending time**. Converting or compressing every media file is the most expensive work in the pipeline, and nobody should pay for it before seeing that the import is worth running (decision 9). Under `copy` and `skip` there is no media step, so this is the only gate and approving it starts the upload (decision 6).

Decision 11: the counts are read from the staging folder and are exact — conversations, messages, contacts with how many match no contact in the vault, attachments, bytes copied. Only the per-file verdict is a guess, because the media step has not run.

Decision 19: the estimate column stays in both modes, and `convert` needs it more than `compress` does. Converting changes size as a side effect, usually upward for Apple formats, so a file comfortably under the limit before the media step can be over it afterwards — the forecast a user needs in `convert` mode is the one about files that are currently fine. Only the wording differs: "after converting" against "after compressing".

Decision 20: the heading names the stage. This one is **Review what was copied**.

**Files:**
- Create: `web/src/screens/import/gateForecast.ts` — verdict copy and grouping, pure
- Create: `web/src/screens/import/gateForecast.test.ts`
- Create: `web/src/screens/import/GateOneScreen.tsx`
- Create: `web/src/screens/import/GateOneScreen.test.tsx`

**Interfaces:**
- Consumes: `StagingSummary`, `AttachmentForecast`, `VerdictCounts`, `SizeVerdict` from Task 7; `POST /v1/contacts/match` from Task 6.
- Produces: `GateOneScreen({ summary, unknownContacts, mode, onApprove, onDecline, busy })`. Task 10 renders it.

- [ ] **Step 1: Write the failing copy tests**

`gateForecast.test.ts`. The copy is the deliverable here, so the assertions are
on exact strings.

```ts
describe("verdictCopy", () => {
  it("says which job the estimate is about", () => {
    expect(verdictCopy("likely_fits", "convert").label).toBe("Likely to fit after converting");
    expect(verdictCopy("likely_fits", "compress").label).toBe("Likely to fit after compressing");
  });

  it("warns that a file which fits today may not afterwards", () => {
    // Decision 12's whole point: conversion is not a size reduction, and an
    // iPhone backup is mostly formats that grow.
    expect(verdictCopy("may_grow", "convert").label).toBe("May grow past the limit");
  });

  it("names the two settled states plainly", () => {
    expect(verdictCopy("fits_as_is", "convert").label).toBe("Fits as-is");
    expect(verdictCopy("probably_too_big", "convert").label).toBe("Probably still too big");
  });

  it("explains a file the media step cannot touch", () => {
    expect(verdictCopy("cannot_process", "convert").label).toBe(
      "Cannot be converted — not audio or video",
    );
  });

  it("never says transcode", () => {
    const modes = ["convert", "compress"] as const;
    const verdicts = [
      "fits_as_is",
      "likely_fits",
      "may_grow",
      "probably_too_big",
      "cannot_process",
    ] as const;
    for (const mode of modes) {
      for (const verdict of verdicts) {
        expect(verdictCopy(verdict, mode).label.toLowerCase()).not.toContain("transcode");
      }
    }
  });
});

describe("forecastGroups", () => {
  it("drops the states with nothing in them", () => {
    // A row reading "0 files may grow past the limit" is noise on a screen
    // whose job is to be read quickly.
    const groups = forecastGroups({ fitsAsIs: 12, likelyFits: 0, mayGrow: 2, probablyTooBig: 0, cannotProcess: 0 }, "convert");
    expect(groups.map((g) => g.verdict)).toEqual(["fits_as_is", "may_grow"]);
  });

  it("puts the states that need attention first", () => {
    const groups = forecastGroups({ fitsAsIs: 12, likelyFits: 3, mayGrow: 2, probablyTooBig: 1, cannotProcess: 1 }, "convert");
    expect(groups[0].verdict).toBe("probably_too_big");
    expect(groups.at(-1)?.verdict).toBe("fits_as_is");
  });
});
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && npx vitest run src/screens/import/gateForecast.test.ts
```

Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the copy module**

`gateForecast.ts` holds the verdict wording, the ordering, and nothing else, so
the screen stays presentational and the copy is testable without rendering.

```ts
/** The job the media step is doing, in the user's words (decisions 18, 19). */
export function mediaJobVerb(mode: AttachmentMediaMode): "converting" | "compressing" | null {
  if (mode === "convert") return "converting";
  if (mode === "compress") return "compressing";
  return null;
}
```

`verdictCopy(verdict, mode)` returns `{ label, hint }` where `hint` is the
one-line explanation shown under the group, and the label uses `mediaJobVerb`
so the two modes never drift apart. Order for `forecastGroups`:
`probably_too_big`, `may_grow`, `cannot_process`, `likely_fits`, `fits_as_is` —
what needs attention first, what is fine last.

- [ ] **Step 4: Write the failing screen tests**

`GateOneScreen.test.tsx`:

```tsx
it("names the stage in its heading", () => {
  render(<GateOneScreen {...props()} />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Review what was copied");
});

it("shows the measured counts", () => {
  render(<GateOneScreen {...props({ summary: { conversations: 12, messages: 4310, attachments: 88, attachmentBytes: 1024 * 1024 * 512 } })} />);
  expect(screen.getByText("12")).toBeInTheDocument();
  expect(screen.getByText("4,310")).toBeInTheDocument();
});

it("says how many contacts are new to the vault", () => {
  render(<GateOneScreen {...props({ unknownContacts: 7 })} />);
  expect(screen.getByText(/7 new to your vault/)).toBeInTheDocument();
});

it("says the size numbers are estimates", () => {
  // The screen says throughout that these are estimates (decision 13).
  render(<GateOneScreen {...props({ mode: "convert" })} />);
  expect(screen.getByText(/estimate/i)).toBeInTheDocument();
});

it("offers to start the media step under convert", () => {
  render(<GateOneScreen {...props({ mode: "convert" })} />);
  expect(screen.getByRole("button", { name: "Convert media" })).toBeInTheDocument();
});

it("offers to upload directly under copy, because there is no media step", () => {
  render(<GateOneScreen {...props({ mode: "copy" })} />);
  expect(screen.getByRole("button", { name: "Upload to vault" })).toBeInTheDocument();
  expect(screen.queryByText(/estimate/i)).not.toBeInTheDocument();
});

it("does not act twice on a double click", () => {
  const onApprove = vi.fn();
  render(<GateOneScreen {...props({ onApprove, busy: true })} />);
  fireEvent.click(screen.getByRole("button", { name: /Convert media|Upload to vault/ }));
  expect(onApprove).not.toHaveBeenCalled();
});

it("offers to cancel the import", () => {
  render(<GateOneScreen {...props()} />);
  expect(screen.getByRole("button", { name: "Cancel this import" })).toBeInTheDocument();
});
```

`props()` is a local factory returning a complete valid prop set with sensible
defaults, overridable per test — follow how `ResumeImportPanel.test.tsx` builds
its cases.

- [ ] **Step 5: Write the screen**

Presentational: props in, callbacks out, no fetching and no session knowledge.
Structure — heading, the measured counts as a small table, the forecast groups
(omitted entirely when `mediaJobVerb(mode)` is null, because there is nothing
to forecast), then the two buttons. The primary button's label comes from the
mode: `Convert media`, `Compress media`, or `Upload to vault` when this gate is
the only one. Secondary is `Cancel this import`, ghost variant, matching
`ResumeImportPanel`'s button pair.

Numbers get `toLocaleString()` and byte totals go through the existing byte
formatter the import screens already use — do not add a second one.

- [ ] **Step 6: Verify**

```bash
cd web && npx vitest run src/screens/import/ && npx tsc --noEmit && npm run lint
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src/screens/import/gateForecast.ts web/src/screens/import/gateForecast.test.ts web/src/screens/import/GateOneScreen.tsx web/src/screens/import/GateOneScreen.test.tsx
git commit -m "feat(web): review what was copied before spending time on media"
```

---

### Task 9: Gate 2 — ready to upload

The second gate approves **what lands in the vault**. Decision 14: it leads with the delta, not a fresh summary. The question it answers is where Gate 1 was wrong — how many files we said would fit did, how many we wrote off came in under after all, and what failed that nobody flagged. The final upload state follows underneath.

Decision 17 fixes the standing copy, and it is written for the finished product. Do not hedge it into a warning about irreversibility:

> Messages are always uploaded. A skipped attachment leaves a placeholder in the conversation, and the message text is kept. Imported conversations can later be removed from your vault in the messages area.

Decision 20: the heading is **Ready to upload**.

This gate exists only under `convert` and `compress`. Under `copy` and `skip` there is no media step, Gate 1's numbers were already exact, and approving there starts the upload.

**Files:**
- Create: `web/src/screens/import/gateDelta.ts`, `gateDelta.test.ts`
- Create: `web/src/screens/import/GateTwoScreen.tsx`, `GateTwoScreen.test.tsx`

**Interfaces:**
- Consumes: two `StagingSummary` values — the one shown at Gate 1 and the one recomputed after the media pass.
- Produces: `gateDelta(approved: StagingSummary, actual: StagingSummary): GateDelta` and `GateTwoScreen({ delta, actual, mode, onApprove, onDecline, busy })`. Task 10 renders it; Task 11 reads the same `gateDelta`.

- [ ] **Step 1: Write the failing delta tests**

```ts
describe("gateDelta", () => {
  it("counts the forecasts that came true", () => {
    const delta = gateDelta(
      summary({ likelyFits: 4 }),
      summary({ fitsAsIs: 4 }),
    );
    expect(delta.forecastHeld).toBe(4);
  });

  it("counts files written off that came in under after all", () => {
    // Good news, and worth saying: the user approved on the assumption these
    // were lost.
    const delta = gateDelta(summary({ probablyTooBig: 3 }), summary({ fitsAsIs: 3 }));
    expect(delta.betterThanForecast).toBe(3);
  });

  it("counts files that crossed the limit nobody flagged", () => {
    const delta = gateDelta(summary({ fitsAsIs: 10 }), summary({ fitsAsIs: 9, tooLarge: 1 }));
    expect(delta.worseThanForecast).toBe(1);
  });

  it("is empty when the forecast was exactly right", () => {
    const same = summary({ fitsAsIs: 10 });
    expect(gateDelta(same, same).hasChanges).toBe(false);
  });
});
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && npx vitest run src/screens/import/gateDelta.test.ts
```

Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the delta**

`gateDelta.ts` compares the two summaries per attachment path, not per count —
counts alone cannot tell "3 fell out and 3 different ones came in" from "nothing
changed". `GateDelta` carries `forecastHeld`, `betterThanForecast`,
`worseThanForecast`, `failed`, `hasChanges`, and the affected file names for
each bucket so the screen can list them.

Decision 45 gives `worseThanForecast` its own row: a file that was under the
limit and is now over. Match it by looking for `missing_reason === "too_large"`
in the recomputed summary against a `fits_as_is` or `may_grow` verdict in the
approved one.

- [ ] **Step 4: Write the failing screen tests**

```tsx
it("names the stage in its heading", () => {
  render(<GateTwoScreen {...props()} />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Ready to upload");
});

it("leads with the delta, not a fresh summary", () => {
  // Decision 14: where was Gate 1 wrong. The final state follows underneath.
  render(<GateTwoScreen {...props({ delta: { worseThanForecast: 2, hasChanges: true } })} />);
  const headings = screen.getAllByRole("heading");
  expect(headings[1]).toHaveTextContent(/what changed/i);
});

it("says so plainly when the forecast held", () => {
  render(<GateTwoScreen {...props({ delta: { hasChanges: false } })} />);
  expect(screen.getByText(/came out as expected/i)).toBeInTheDocument();
});

it("carries the standing copy about what an import does", () => {
  render(<GateTwoScreen {...props()} />);
  expect(
    screen.getByText(
      "Messages are always uploaded. A skipped attachment leaves a placeholder in the conversation, and the message text is kept. Imported conversations can later be removed from your vault in the messages area.",
    ),
  ).toBeInTheDocument();
});

it("offers to upload and to cancel", () => {
  render(<GateTwoScreen {...props()} />);
  expect(screen.getByRole("button", { name: "Upload to vault" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel this import" })).toBeInTheDocument();
});
```

- [ ] **Step 5: Write the screen**

Heading, then the delta section, then the final counts, then the standing copy,
then the buttons. The delta section renders "Everything came out as expected"
when `hasChanges` is false rather than an empty region — a blank space where a
comparison should be reads as a bug.

- [ ] **Step 6: Verify**

```bash
cd web && npx vitest run src/screens/import/ && npx tsc --noEmit && npm run lint
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src/screens/import/gateDelta.ts web/src/screens/import/gateDelta.test.ts web/src/screens/import/GateTwoScreen.tsx web/src/screens/import/GateTwoScreen.test.tsx
git commit -m "feat(web): show what changed before uploading to the vault"
```

---

### Task 10: Wire the gates into the import flow

The flow the two screens sit in. Decision 10: parse and write run unasked — clicking Import starts them immediately, because abandoning before Gate 1 costs a folder deletion and there is nothing to protect the user from yet. Decision 16: declining at either gate is terminal — the session closes, the staging folder is deleted, and the next Import opens a clean form. A user who changes their mind pays for the work again; keeping staging for a re-push with different settings would leave large folders on disk with no owner.

The stage column follows the run for the first time: `parse` on create, `write` once extract starts, `awaiting_gate_1` when the summary is ready, `transcode` while the media pass runs, `awaiting_gate_2` after it, `pushing` on the last approval.

**Files:**
- Modify: `web/src/screens/import/useImportJob.ts`
- Modify: `web/src/screens/ImportScreen.tsx`
- Modify: `web/src/screens/import/useImportJob.test.tsx`, `web/src/screens/ImportScreen.test.tsx`

**Interfaces:**
- Consumes: everything from Tasks 7, 8 and 9, plus `invokeDeleteStaging` and the stage endpoint's approved-plan field from Tasks 5 and 6.
- Produces: `ImportPhase` gains `"gate_1"` and `"gate_2"`; the hook returns `gateSummary`, `gateDelta`, `approveGate`, `declineGate`.

- [ ] **Step 1: Write the failing hook tests**

Follow `useImportJob.test.tsx`'s existing mocking of `../../lib/tauri` — its
factory must gain the two new commands or every test in the file fails on an
undefined import.

```tsx
it("stops at the first gate instead of uploading", async () => {
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  expect(result.current.phase).toBe("gate_1");
  expect(invokePush).not.toHaveBeenCalled();
  expect(invokeTranscodeStaging).not.toHaveBeenCalled();
});

it("asks the exporter to stage originals under convert", async () => {
  // The desktop runs the media pass itself, after the gate.
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  expect(invokeExtract).toHaveBeenCalledWith(
    expect.objectContaining({ attachment_media: "copy" }),
  );
});

it("records the stage as it goes", async () => {
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  expect(setImportStage).toHaveBeenCalledWith(1, "write");
  expect(setImportStage).toHaveBeenCalledWith(1, "awaiting_gate_1");
});

it("runs the media pass then stops at the second gate", async () => {
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  await act(() => result.current.approveGate());
  expect(invokeTranscodeStaging).toHaveBeenCalled();
  expect(result.current.phase).toBe("gate_2");
  expect(invokePush).not.toHaveBeenCalled();
});

it("uploads straight from the first gate under copy, because there is no second one", async () => {
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "copy" })));
  await act(() => result.current.approveGate());
  expect(invokeTranscodeStaging).not.toHaveBeenCalled();
  expect(invokePush).toHaveBeenCalled();
});

it("recomputes the summary after the media pass rather than adjusting the old one", async () => {
  // Decision 39: the folder is the truth.
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  invokeSummarizeStaging.mockClear();
  await act(() => result.current.approveGate());
  expect(invokeSummarizeStaging).toHaveBeenCalledTimes(1);
});

it("declining closes the session and deletes the folder", async () => {
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  await act(() => result.current.declineGate());
  expect(discardImportSession).toHaveBeenCalledWith(1);
  expect(invokeDeleteStaging).toHaveBeenCalledWith("/staging/run-1");
  expect(result.current.phase).toBe("form");
});

it("deletes the folder even when discarding the session fails", async () => {
  // Either half failing must not leave the other undone: a live session with
  // no folder blocks the next import, and a folder with no session is litter
  // nothing will ever clean up.
  discardImportSession.mockRejectedValueOnce(new Error("offline"));
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  await act(() => result.current.declineGate());
  expect(invokeDeleteStaging).toHaveBeenCalled();
});

it("does not run the media pass twice on a double click", async () => {
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  await act(() => {
    void result.current.approveGate();
    void result.current.approveGate();
  });
  expect(invokeTranscodeStaging).toHaveBeenCalledTimes(1);
});

it("a failed media pass is a failed import, not a silent skip to upload", async () => {
  invokeTranscodeStaging.mockRejectedValueOnce(new Error("ffmpeg missing"));
  const { result } = renderHook(() => useImportJob());
  await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
  await act(() => result.current.approveGate());
  expect(invokePush).not.toHaveBeenCalled();
  expect(result.current.phase).toBe("done");
  expect(result.current.summaryView?.status).toBe("failed");
});
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && npx vitest run src/screens/import/useImportJob.test.tsx
```

Expected: FAIL — the hook still pushes straight after extract.

- [ ] **Step 3: Split `startImport` at the gate**

`startImport` currently runs staging resolve → session create → extract →
`/complete` → push → verdict in one function. Cut it after extract:

- `startImport` ends by writing stage `awaiting_gate_1`, calling
  `invokeSummarizeStaging`, storing the summary, and setting `phase: "gate_1"`.
  It no longer pushes and no longer completes the session.
- `approveGate()` branches on the phase. From `gate_1` with a media step it
  writes stage `transcode`, runs `invokeTranscodeStaging`, recomputes the
  summary, writes `awaiting_gate_2`, stores both summaries, and sets
  `phase: "gate_2"`. From `gate_1` without a media step, or from `gate_2`, it
  writes `pushing` and runs the push and verdict block exactly as it does today.
- `declineGate()` discards the session and deletes the staging folder, then
  returns to the form. Both halves run regardless of the other's outcome — wrap
  each independently, not in one `try`.

The approved plan is persisted at the moment of approval, not at completion:
the stage call that moves to `pushing` carries the summary the user was looking
at, so decision 15's diff survives a reload. That is what `summary_json` is for
(decision 2), and it is a different question from the recomputed summary
(decision 39).

Guard both callbacks with the same in-flight ref the resume actions already use
in `ImportScreen.tsx`, so a double click does one thing.

The resume short-circuit added in Phase 2 (`startImport(form, resume)`) keeps
working unchanged: it lands directly in the push block, which is still the push
block.

- [ ] **Step 4: Render the gates**

In `ImportScreen.tsx`, add two branches to the render switch, beside the
existing form / resume / progress ones. Both are `<GateOneScreen>` and
`<GateTwoScreen>` with the hook's values and callbacks passed straight through.
Fetch the unknown-contact count once, when entering `gate_1`, from
`POST /v1/contacts/match` with the summary's identifiers — batched at
`MAX_MATCH_IDENTIFIERS` per request. A failed lookup renders the contact row
without the "new to your vault" clause rather than blocking the gate.

- [ ] **Step 5: Verify**

```bash
cd web && npx vitest run && npx tsc --noEmit && npm run lint
```

Expected: PASS. Widening `ImportPhase` makes every switch over it a compile
error until each one handles the two new values — that is the point.

- [ ] **Step 6: Commit**

```bash
git add web/src/screens/import/useImportJob.ts web/src/screens/import/useImportJob.test.tsx web/src/screens/ImportScreen.tsx web/src/screens/ImportScreen.test.tsx
git commit -m "feat(web): stop at both gates before spending time and before uploading"
```

---

### Task 11: Judge the outcome against what the user approved

Decision 15: the outcome is diffed against Gate 2's approval, not Gate 1's. Gate 1 approved spending time; only Gate 2 gated what enters the vault. A skip approved at Gate 2 is an expected omission. A skip nobody forecast is an error even if there is only one. This is what makes "12 attachments too big" a normal import rather than a failure.

Phase 1 already established the three-way verdict and reads it from the push report (decisions 21, 22). This task gives it the second input.

**Files:**
- Modify: `web/src/screens/import/importOutcome.ts`, `importOutcome.test.ts`
- Modify: `web/src/screens/import/useImportJob.ts` (pass the approved plan in)

**Interfaces:**
- Consumes: `gateDelta` from Task 9, the approved summary from Task 10.
- Produces: `importOutcome({ report, threw, issues, approved })` — `approved` is optional, and its absence must behave exactly as today so a resumed push with no stored plan still reports something sensible.

- [ ] **Step 1: Write the failing tests**

```ts
it("an approved omission is not an issue", () => {
  // The user saw "12 attachments too big" at the gate and said go. Reporting
  // that back as a problem makes a normal import look like a failure.
  const outcome = importOutcome({
    report: okReport({ conversationsOk: 10, messagesInserted: 500 }),
    threw: false,
    issues: tooLargeIssues(12),
    approved: approvedPlan({ tooLarge: 12 }),
  });
  expect(outcome).toBe("completed");
});

it("one omission nobody forecast is an issue", () => {
  const outcome = importOutcome({
    report: okReport({ conversationsOk: 10, messagesInserted: 500 }),
    threw: false,
    issues: tooLargeIssues(1),
    approved: approvedPlan({ tooLarge: 0 }),
  });
  expect(outcome).toBe("completed_with_issues");
});

it("zero conversations is a failure however clean the issue list is", () => {
  // Decision 21's floor, unchanged by this task.
  const outcome = importOutcome({
    report: okReport({ conversationsOk: 0, messagesInserted: 0 }),
    threw: false,
    issues: [],
    approved: approvedPlan({ tooLarge: 0 }),
  });
  expect(outcome).toBe("failed");
});

it("behaves exactly as before when there is no approved plan", () => {
  const args = { report: okReport({ conversationsOk: 10, messagesInserted: 500 }), threw: false, issues: tooLargeIssues(3) };
  expect(importOutcome(args)).toBe("completed_with_issues");
  expect(importOutcome({ ...args, approved: undefined })).toBe("completed_with_issues");
});
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && npx vitest run src/screens/import/importOutcome.test.ts
```

Expected: FAIL — `approved` is not a parameter.

- [ ] **Step 3: Add the comparison**

An issue is expected when the approved plan already accounted for that file:
matched by path, with the same reason. Everything else counts toward
`completed_with_issues` as it does today. The zero-conversation floor is checked
first and is not affected by the plan.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npx vitest run && npx tsc --noEmit && npm run lint
git add web/src/screens/import/importOutcome.ts web/src/screens/import/importOutcome.test.ts web/src/screens/import/useImportJob.ts
git commit -m "feat(web): judge an import against the plan the user approved"
```

---

### Task 12: Resume at a gate

Decision 36's remaining rows. A session that died at either gate goes back to the summary, recomputed from the folder. One that died in the media pass re-runs it over every original still on disk — Task 3 already makes that safe, so resuming is a matter of routing there. Decision 37 still holds: no timeout ever reclaims a session, and the only way out is an explicit discard.

**Files:**
- Modify: `web/src/screens/import/resumeDecision.ts`, `resumeDecision.test.ts`
- Modify: `web/src/screens/import/ResumeImportPanel.tsx`, `ResumeImportPanel.test.tsx`
- Modify: `web/src/screens/ImportScreen.tsx` (route the new kinds)

**Interfaces:**
- Consumes: the stage vocabulary from Phase 2, the gate flow from Task 10.
- Produces: `ResumeDecision["kind"]` gains `"resume_gate"` and `"resume_media"`.

- [ ] **Step 1: Write the failing tests**

```ts
it("sends a session waiting at a gate back to its gate", () => {
  expect(resumeDecisionFor(session({ stage: "awaiting_gate_1" }), stagedFolder(), thisDevice).kind)
    .toBe("resume_gate");
  expect(resumeDecisionFor(session({ stage: "awaiting_gate_2" }), stagedFolder(), thisDevice).kind)
    .toBe("resume_gate");
});

it("sends a session that died converting back to the media pass", () => {
  expect(resumeDecisionFor(session({ stage: "transcode" }), stagedFolder(), thisDevice).kind)
    .toBe("resume_media");
});

it("still offers discard only when the folder is gone at a gate", () => {
  // Decision 36: after approval, discard only. There is nothing to recompute
  // a summary from.
  expect(resumeDecisionFor(session({ stage: "awaiting_gate_2" }), missingFolder(), thisDevice).kind)
    .toBe("folder_missing");
});

it("leaves the stages Phase 2 already routed alone", () => {
  expect(resumeDecisionFor(session({ stage: "pushing" }), stagedFolder(), thisDevice).kind)
    .toBe("resume_push");
  expect(resumeDecisionFor(session({ stage: "parse" }), stagedFolder(), thisDevice).kind)
    .toBe("restart");
});
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && npx vitest run src/screens/import/resumeDecision.test.ts
```

Expected: FAIL — both stages currently fall through to `restart`.

- [ ] **Step 3: Route the two stages**

`resumeDecisionFor` gains the two cases ahead of its `restart` fallback. Both
require a present staging folder; without one they keep falling through to
`folder_missing`, which is decision 36's "after approval: discard only".

Copy for the panel, following the existing entries' voice:

```ts
resume_gate: {
  heading: () => "Pick up where you left off",
  body: () =>
    "Your messages are staged. Opening the import again shows you the same summary, read fresh from the folder.",
  primary: { label: "Show me the summary", action: "resume" },
  secondary: { label: "Discard this import", action: "discard" },
},
resume_media: {
  heading: () => "Finish preparing your media",
  body: () =>
    "The media step did not finish. Carrying on picks up the files it had not reached yet.",
  primary: { label: "Carry on", action: "resume" },
  secondary: { label: "Discard this import", action: "discard" },
},
```

In `ImportScreen.tsx`, `resume_gate` recomputes the summary and lands on the
gate matching the recorded stage; `resume_media` re-runs the media pass and then
continues to Gate 2, which is what `approveGate` already does from `transcode`.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npx vitest run && npx tsc --noEmit && npm run lint
git add web/src/screens/import/resumeDecision.ts web/src/screens/import/resumeDecision.test.ts web/src/screens/import/ResumeImportPanel.tsx web/src/screens/import/ResumeImportPanel.test.tsx web/src/screens/ImportScreen.tsx
git commit -m "feat(web): resume an import that stopped at a gate or mid-conversion"
```

---

## Finishing the branch

```bash
./scripts/check-pr.sh
```

Everything must be green. The Postgres-gated tests do not run locally and CI's
`test-postgres` job is what covers them — this phase changes no schema, so that
job is a regression check rather than a gate on new work.
