# Exporter review fixes 1–8 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Close the must-fix and should-fix exporter review findings (priority order 1–8) so converters refuse destructive input/output overlap, confine WhatsApp media roots, write iMessage attachments atomically, keep group and message IDs stable, fix WhatsApp phone/path metadata, improve SMS Backup+ cancel/memory/dedupe, and finish remaining consistency gaps with tests.

**Architecture:** Each task touches one exporter (or a small pair) and copies an existing hardened pattern from another exporter. No new crates. Shared helpers stay local to each crate unless a pattern already lives in `message-vault-io-core`.

**Tech Stack:** Rust workspace exporters under `crates/exporters/*`, `message-ir`, `message-ir-format::FormatSink`, `tempfile`, `cargo test -p <crate>`.

## Global Constraints

- Follow `.cursor/skills/communication-style/SKILL.md` in commit messages and docs (no “we/us/our”, no review shorthand).
- Prefer TDD: failing test first, then minimal fix.
- Do not change vault-push / unrelated crates.
- Do not kill `wtsexporter` mid-process in this plan.
- Commit after each task on the feature branch.

## File map

| Area | Files |
|------|--------|
| OpenExtract guard | `openextract-exporter/src/emit.rs`, `tests/convert_smoke.rs` |
| SMS Backup+ guard | `sms-backup-plus-exporter/src/emit.rs`, tests under that crate |
| WhatsApp roots | `whatsapp-exporter/src/run.rs`, `emit.rs` (tests) |
| iMessage atomic write | `imessage-ir-exporter/src/emit.rs` |
| GO SMS group id | `go-sms-pro-exporter/src/emit.rs` |
| iMazing GUID | `imazing-exporter/src/emit.rs` |
| WhatsApp E.164 / no-copy | `whatsapp-exporter/src/jid.rs`, `emit.rs` |
| SMS Backup+ cancel/chunk/dedupe | `sms-backup-plus-exporter/src/{emit,identity}.rs` |
| Task 8 leftovers | imessage `emit.rs`/`options`/`attachments`, openextract `emit.rs`, go-sms `emit.rs` |

---

### Task 1: OpenExtract + SMS Backup+ input/output identity guard

**Files:**
- Modify: `crates/exporters/openextract-exporter/src/emit.rs` (`convert_export`)
- Modify: `crates/exporters/openextract-exporter/tests/convert_smoke.rs` (or add test module)
- Modify: `crates/exporters/sms-backup-plus-exporter/src/emit.rs` (entry that calls `open_prepared`)
- Test: add/extend smoke or unit tests in both crates

**Interfaces:**
- Consumes: `FormatSink::open_prepared` (must not run when paths overlap)
- Produces: early `bail!` with message containing `must not be the same as, or contain`

- [x] **Step 1: Write failing OpenExtract test**

Mirror go-sms `output_equals_input_bails_before_cleaning`: call convert with fixture input as both input and output; expect error; assert a fixture CSV still exists.

- [x] **Step 2: Run OpenExtract test — expect FAIL**

```bash
cargo test -p openextract-exporter output_equals_input -- --nocapture
```

- [x] **Step 3: Implement OpenExtract guard before `open_prepared`**

```rust
fs::create_dir_all(output)?;
let input = fs::canonicalize(input).with_context(|| format!("resolve {}", input.display()))?;
let output = fs::canonicalize(output).with_context(|| format!("resolve {}", output.display()))?;
if output == input || input.starts_with(&output) {
    bail!(
        "output {} must not be the same as, or contain, the input {}",
        output.display(),
        input.display()
    );
}
```

Then call `FormatSink::open_prepared(&output, ...)`.

- [x] **Step 4: Same pattern for SMS Backup+** before `open_prepared`, for every input root: refuse if any canonical input equals output or starts with output (file inputs: compare parent or the file path when output is that file’s parent incorrectly — match go-sms semantics: output must not be same as / contain input). For a list of inputs, canonicalize each; if any `input == output || input.starts_with(output)`, bail.

- [x] **Step 5: Tests pass**

```bash
cargo test -p openextract-exporter
cargo test -p sms-backup-plus-exporter
```

- [x] **Step 6: Commit**

```bash
git add crates/exporters/openextract-exporter crates/exporters/sms-backup-plus-exporter
git commit -m "$(cat <<'EOF'
fix(exporters): refuse output that overlaps OpenExtract or SMS Backup+ input

Cleaning export artifacts before read was deleting source CSVs when
output pointed at the backup folder. Match the GO SMS / iMazing guard.
EOF
)"
```

---

### Task 2: WhatsApp media-root allowlist (drop CWD)

**Files:**
- Modify: `crates/exporters/whatsapp-exporter/src/run.rs`
- Modify: `crates/exporters/whatsapp-exporter/src/emit.rs` (unit tests for `resolve_media_file` / roots)

**Interfaces:**
- Consumes: `media_search_roots` passed into `convert_json`
- Produces: roots = work dir + backup input + optional absolute media only (no `env::current_dir()`)

- [x] **Step 1: Failing test**

Add a unit test that builds allowed roots without CWD and asserts a file that only lives under a fake CWD-like path is rejected by `path_within_any` / `resolve_media_file`.

- [x] **Step 2: Remove CWD pushes in `run.rs`**

`--json` path: roots = input (if any) + json parent (if any).  
wtsexporter path: roots = `work.path()` + `input` only.  
If `source.media` is absolute, it is already covered via `media_base` / allowed roots in emit — ensure absolute `media` is passed through as today.

- [x] **Step 3: Tests pass**

```bash
cargo test -p whatsapp-exporter
```

- [x] **Step 4: Commit**

```bash
git commit -m "$(cat <<'EOF'
fix(whatsapp-exporter): stop treating CWD as an allowed media root

Media copy already rejects paths outside the allowlist. Including the
process working directory let crafted JSON copy arbitrary readable files
under that tree into attachments/.
EOF
)"
```

---

### Task 3: iMessage atomic attachment writes

**Files:**
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs` (`persist_attachment`)

**Interfaces:**
- Produces: final dest only appears after successful write+rename; hash bytes once

- [x] **Step 1: Failing unit test**

Test that writing via the new helper leaves no truncated final file if the temp write is the only complete artifact (or assert dest is created via rename: write dest.tmp then rename; existing incomplete dest without matching length is replaced).

Concrete approach:

```rust
#[test]
fn persist_attachment_uses_temp_then_rename() {
    // After persist, dest exists and digest matches bytes.
    // If dest exists with wrong length, rewrite.
}
```

- [x] **Step 2: Implement**

```rust
fn persist_attachment(...) -> Result<(String, String, u64), RuntimeError> {
    let digest_hex = hex::encode(Sha256::digest(bytes));
    let name = attachment_dest_name_from_digest(timestamp_unix_ms, &digest_hex, original_name);
    let dest = attachments_dir.join(&name);
    let byte_len = bytes.len() as u64;
    let needs_write = match fs::metadata(&dest) {
        Ok(meta) => meta.len() != byte_len,
        Err(_) => true,
    };
    if needs_write {
        let tmp = attachments_dir.join(format!("{name}.tmp"));
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &dest)?;
    }
    Ok((format!("attachments/{name}"), digest_hex, byte_len))
}
```

Refactor `attachment_dest_name` to accept precomputed digest or compute once.

- [x] **Step 3: Tests**

```bash
cargo test -p imessage-ir-exporter
```

- [x] **Step 4: Commit**

```bash
git commit -m "$(cat <<'EOF'
fix(imessage-ir-exporter): write attachments via temp file then rename

A crash mid-write left a short file that later runs treated as complete.
Match the SMS Backup & Restore staging pattern.
EOF
)"
```

---

### Task 4: GO SMS Pro group ID length-prefix

**Files:**
- Modify: `crates/exporters/go-sms-pro-exporter/src/emit.rs` (`chat_id_group`)

- [x] **Step 1: Failing unit test**

```rust
#[test]
fn group_chat_ids_do_not_collide_on_digit_boundaries() {
    let (a, _) = chat_id_group(&[ "12".into(), "34".into() ], ...);
    let (b, _) = chat_id_group(&[ "123".into(), "4".into() ], ...);
    assert_ne!(a, b);
}
```

(Adapt to actual `chat_id_group` signature.)

- [x] **Step 2: Implement length-prefix like SMS Backup+**

```rust
let slug = others
    .iter()
    .map(|d| format!("{}:{}", d.len(), d))
    .collect::<Vec<_>>()
    .join("_");
```

Keep the >180 digest truncation.

- [x] **Step 3: Tests + commit**

```bash
cargo test -p go-sms-pro-exporter
git commit -m "$(cat <<'EOF'
fix(go-sms-pro-exporter): length-prefix group chat id peer digits

Joining raw digit lists with underscores made [12,34] and [123,4]
share one conversation id. Match SMS Backup+ length prefixes.
EOF
)"
```

---

### Task 5: iMazing GUID digests

**Files:**
- Modify: `crates/exporters/imazing-exporter/src/emit.rs` (~713)

- [x] **Step 1: Failing test** (unit or emit test)

Same message metadata with `digest_sha256` set vs only `rel_path` differing must produce the same GUID when digests match; GUID must follow digest not path.

- [x] **Step 2: Implement**

```rust
let digests: Vec<String> = msg
    .attachments
    .iter()
    .map(|a| {
        a.digest_sha256
            .clone()
            .unwrap_or_else(|| a.rel_path.clone())
    })
    .collect();
```

Sort digests before `stable_guid` if order is unstable.

- [x] **Step 3: Tests + commit**

```bash
cargo test -p imazing-exporter
git commit -m "$(cat <<'EOF'
fix(imazing-exporter): build message GUIDs from attachment digests

Using relative paths made IDs change when a later run found and copied
a previously missing file. Prefer content digests when present.
EOF
)"
```

---

### Task 6: WhatsApp E.164 + no-copy path hygiene

**Files:**
- Modify: `crates/exporters/whatsapp-exporter/src/jid.rs`
- Modify: `crates/exporters/whatsapp-exporter/src/emit.rs` (no-copy branch ~201–211)

- [x] **Step 1: Failing jid tests**

```rust
assert_eq!(jid_to_e164("447911123456@s.whatsapp.net").as_deref(), Some("+447911123456"));
// US still +1…
assert_eq!(jid_to_e164("15555550122@s.whatsapp.net").as_deref(), Some("+15555550122"));
```

- [x] **Step 2: Implement `jid_to_e164`**

After `sanitize_number(local)`:
- If `normalize_guarded(&digits, Usa)` yields a value starting with `+`, use it.
- Else if digits length is in 8..=15 and does not start with `0`, use `format!("+{digits}")`.
- Else use `normalize_guarded` result / `None` as today’s guarded policy for ambiguous cases.

Do not invent `+0…` for trunk-zero locals.

- [x] **Step 3: No-copy branch**

When `copy_attachments` is false and `src` is present:

```rust
vec![PendingAttachment {
    rel_path: String::new(), // or omit path — PendingAttachment may require String; use empty and leave IR path None at emit
    content_type: ...,
    extension: ...,
    digest_sha256: None,
    name_hint: name,
}]
```

Ensure document conversion sets `IrAttachment.path` to `None` when rel_path empty, puts original hint in `source.fields` if needed (`media_path` key). Prefer minimal change: `digest_sha256: None`, and store only the basename in `name_hint` / leave `rel_path` empty so packaging does not treat it as export-relative.

- [x] **Step 4: Tests + commit**

```bash
cargo test -p whatsapp-exporter
git commit -m "$(cat <<'EOF'
fix(whatsapp-exporter): E.164 JIDs and honest no-copy attachment metadata

International JIDs were left without a leading plus, breaking matching
against other exporters. Path-string digests and host paths are not
content digests when media is not copied.
EOF
)"
```

---

### Task 7: SMS Backup+ cancel, chunked parse, MMS dedupe

**Files:**
- Modify: `crates/exporters/sms-backup-plus-exporter/src/emit.rs`
- Modify: `crates/exporters/sms-backup-plus-exporter/src/identity.rs`

- [x] **Step 1: Failing identity test**

Two messages same chat/second/direction/empty text, digests `aaa` vs `bbb` → different `cover_identity` (or new helper used as map key).

- [x] **Step 2: Extend `cover_identity`**

When attachment digests (sorted, non-empty) exist, append `|digest1,digest2` to the key. Text-only / no-attachment messages keep today’s key so archive↔flat text SMS still collapse.

- [x] **Step 3: Cancel inside parallel work**

In `par_iter` map closure, first line:

```rust
if message_vault_io_core::check_cancel(cancel).is_err() {
    return ParsedEmlKind::Cancelled; // or Err collected
}
```

Propagate cancel after `collect` so the function returns the cancelled error promptly. Define how `ParsedEmlKind` represents cancel, or use `map` → `Result` and find first cancel.

- [x] **Step 4: Chunk EML paths**

Process `eml_paths` in chunks (e.g. 256 or 512 files): `par_iter` each chunk, merge into conversations, drop chunk outcomes before next chunk. Keeps peak attachment payloads bounded.

- [x] **Step 5: Tests + commit**

```bash
cargo test -p sms-backup-plus-exporter
git commit -m "$(cat <<'EOF'
fix(sms-backup-plus-exporter): cancel during parse, chunk work, digest-aware dedupe

Parallel parse ignored cancel and held every attachment in memory.
Same-second empty-caption MMS with different digests also collapsed.
EOF
)"
```

---

### Task 8: Remaining should-fix + tests

**Files:**
- `imessage-ir-exporter/src/emit.rs` — use `FormatSink::open_prepared` (or `clean_previous_ir_output` before open); set `missing_reason` when bytes absent
- `openextract-exporter/src/emit.rs` — dedupe rows
- `go-sms-pro-exporter/src/emit.rs` — treat `others.len() >= 2` as group even if `!parsed.is_group`
- Tests for each

**Scope for missing_reason values** (string constants matching vault usage where possible):

- `embed_disabled` when `AttachmentEmbed::Disabled`
- `file_missing` when embed on but bytes empty / not loaded
- leave `None` when bytes present

**iMessage open_prepared:** After collecting conversations, call `FormatSink::open_prepared` instead of `FormatSink::open` so prior artifacts and stale `attachments/` are cleaned consistently with WA/SBR. Keep any existing validation for non-export directories if still required by `options::validate_export_path` — adjust so prepared open does not conflict (validate then open_prepared, or relax validate when directory only has prior IR artifacts).

**OpenExtract dedupe:** Track `HashSet` of `(chat_id, secs, is_from_me, text)` per conversation (or global) and skip duplicates.

**GO SMS:** If `others.len() >= 2`, use `chat_id_group` path regardless of `parsed.is_group`.

- [x] **Step 1–N:** Test → implement → `cargo test` for each sub-fix
- [x] **Step final: Commit**

```bash
git commit -m "$(cat <<'EOF'
fix(exporters): align iMessage prep and missing_reason, OpenExtract dedupe, GO SMS groups

iMessage now cleans prior export output like the other converters and
records why attachment bytes are absent. OpenExtract drops duplicate
rows. Multi-peer GO SMS PDU traffic uses a group chat id.
EOF
)"
```

---

### Task 9: Workspace verification

- [x] Run:

```bash
cargo test -p openextract-exporter -p sms-backup-plus-exporter -p whatsapp-exporter \
  -p imessage-ir-exporter -p go-sms-pro-exporter -p imazing-exporter -p sms-backup-restore-exporter
```

- [x] Fix any fallout from API signature changes
- [x] Mark plan checkboxes done in this file as tasks complete

---

## Spec coverage self-check

| Design goal | Task |
|-------------|------|
| Input/output refuse | 1 |
| WhatsApp no CWD root | 2 |
| iMessage atomic write | 3 |
| GO SMS group id | 4 |
| iMazing GUID digests | 5 |
| WhatsApp E.164 + no-copy | 6 |
| SMS Backup+ cancel/chunk/dedupe | 7 |
| Remaining should-fix | 8 |
| Verification | 9 |
