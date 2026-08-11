# Import progress summary and stage timings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Live Import Messages step counts, Summary with errors/skips and stage/total timings, one `vault_imports` session that owns message `import_id` links, reopenable from Storage history.

**Architecture:** Extend `vault_imports` with timing + `summary_json` columns and add `vault_import_issues`. GUI creates the session at import start, passes `import_id` into vault-push (no second session), completes with timings/issues. Tauri emits structured `extract:progress` / issue events so the Import screen can drive steps without treating free-text logs as the primary UI.

**Tech Stack:** SQLite DDL, Rust (`message-vault-server`, `vault-push`, Tauri commands), React/TypeScript (`web/`)

**Spec:** `docs/superpowers/specs/2026-08-11-import-progress-summary-design.md`

## Global Constraints

- Live `done/total` stays in memory / events — not polled from SQLite.
- Persist errors/skips only (no full success file list).
- Timing source of truth: `vault_imports.parse_ms`, `convert_ms`, `upload_ms`, `duration_ms` columns (not JSON-only).
- GUI owns start + enriched complete when `import_id` is supplied to push; CLI push without `import_id` keeps today’s start/complete behavior.
- Do not store push chunk `total time=` as `duration_ms`.
- Additive schema: `ALTER TABLE … ADD COLUMN` when missing + new issues table (no full vault wipe required).
- Exporter / vault-push CLIs must keep working without the GUI.

## File map

| File | Role |
|------|------|
| `schema/sql/accounts.sql` | `vault_imports` timing columns; `vault_import_issues` DDL |
| `scripts/sync-vault-schema.mjs` / `web-next/src/lib/vaultSchema.generated.ts` | Regenerate embedded DDL |
| `crates/vault/server/src/db/schema.rs` | Ensure new columns exist on existing DBs |
| `crates/vault/server/src/db/vault_imports.rs` | Complete/list/get + insert issues |
| `crates/vault/server/src/server.rs` | Complete body, `GET /v1/imports/{id}`, list duration |
| `crates/cli/vault-push/src/run.rs` (+ `http.rs` if needed) | Optional `import_id` on config; skip start/complete when set |
| `crates/cli/vault-push/tests/push_mock.rs` | Mock: reuse import_id |
| `src-tauri/src/commands/push.rs` | Pass `import_id`; emit structured progress |
| `src-tauri/src/commands/extract.rs` | Emit `extract:progress` from parse logs / events |
| `web/src/lib/tauri.ts` | `PushConfig.import_id`; progress listeners |
| `web/src/screens/ImportScreen.tsx` | Live steps, timings, Summary, session ownership |
| `web/src/components/import/ImportSummaryPanel.tsx` (new) | Shared Summary UI |
| `web/src/screens/settings/StorageSection.tsx` | Clickable history → detail |

---

### Task 1: Schema + vault_imports DB layer

**Files:**
- Modify: `schema/sql/accounts.sql`
- Modify: `crates/vault/server/src/db/schema.rs`
- Modify: `crates/vault/server/src/db/vault_imports.rs`
- Regenerate: `node scripts/sync-vault-schema.mjs`

**Interfaces:**
- Produces:
  - Columns on `vault_imports`: `duration_ms`, `parse_ms`, `convert_ms`, `upload_ms`, `summary_json` (all nullable)
  - Table `vault_import_issues (id, import_id, kind, step, item, reason, created_at)`
  - `CompleteImportArgs` gains optional timings + `summary_json` + `issues: Vec<ImportIssueInput>`
  - `get_import_detail(conn, account_id, import_id) -> ImportDetail` (row + issues)
  - `list_imports` / `ImportSummary` include `duration_ms: Option<i64>`

- [ ] **Step 1: Write failing unit tests** in `vault_imports.rs` `#[cfg(test)]`

Use an in-memory DB with `ensure_accounts_schema` (and minimal accounts row). Cover:

1. `complete_import_persists_timings_and_issues`
2. `get_import_detail_returns_issues`
3. `list_imports_includes_duration_ms`

Sketch for (1):

```rust
#[test]
fn complete_import_persists_timings_and_issues() {
    let conn = test_conn_with_account("acc1");
    let id = start_import(&conn, "acc1", "ios", "append", Some("message-vault-io")).unwrap();
    complete_import(
        &conn,
        "acc1",
        id,
        &CompleteImportArgs {
            ok: true,
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: Some(48_000),
            parse_ms: Some(18_000),
            convert_ms: Some(22_000),
            upload_ms: Some(8_000),
            summary_json: Some(r#"{"parse":{"messages":10}}"#.into()),
            issues: vec![ImportIssueInput {
                kind: "skip".into(),
                step: "convert".into(),
                item: "photo.heic".into(),
                reason: "convert failed".into(),
            }],
        },
    )
    .unwrap();
    let detail = get_import_detail(&conn, "acc1", id).unwrap();
    assert_eq!(detail.row.duration_ms, Some(48_000));
    assert_eq!(detail.row.parse_ms, Some(18_000));
    assert_eq!(detail.issues.len(), 1);
    assert_eq!(detail.issues[0].kind, "skip");
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cargo test -p message-vault-server complete_import_persists_timings_and_issues -- --nocapture
```

Expected: compile/link failure or missing fields/columns.

- [ ] **Step 3: Implement DDL + migrate + DB functions**

In `accounts.sql`, extend `vault_imports` CREATE TABLE with the five new columns, and add:

```sql
CREATE TABLE IF NOT EXISTS vault_import_issues (
    id INTEGER PRIMARY KEY,
    import_id INTEGER NOT NULL REFERENCES vault_imports(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    step TEXT NOT NULL,
    item TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_vault_import_issues_import
    ON vault_import_issues(import_id);
```

In `schema.rs` `ensure_accounts_schema`: after `ACCOUNTS_DDL`, call helpers that `ALTER TABLE vault_imports ADD COLUMN …` when `column_exists` is false (extend `column_exists` to allow `"vault_imports"`). Create issues table via DDL batch if missing.

Extend `VaultImportRow`, `CompleteImportArgs`, `complete_import` UPDATE, SELECTs, and add `get_import_detail` + `insert_issues`.

- [ ] **Step 4: Regenerate schema bundle**

```bash
node scripts/sync-vault-schema.mjs
```

- [ ] **Step 5: Run tests — expect PASS**

```bash
cargo test -p message-vault-server --lib db::vault_imports -- --nocapture
```

- [ ] **Step 6: Commit**

```bash
git add schema/sql/accounts.sql crates/vault/server/src/db/schema.rs \
  crates/vault/server/src/db/vault_imports.rs web-next/src/lib/vaultSchema.generated.ts
git commit -m "$(cat <<'EOF'
feat(vault): persist import timings and issue rows

Store stage/total durations and error/skip rows on vault import
sessions so history can diagnose slow or failed imports.
EOF
)"
```

---

### Task 2: HTTP API — complete payload + get-by-id

**Files:**
- Modify: `crates/vault/server/src/server.rs`
- Test: add handler/integration tests if the crate already has HTTP tests for imports; otherwise unit-test via DB layer already done and add a focused axum test only if one exists nearby

**Interfaces:**
- Consumes: Task 1 `CompleteImportArgs` / `get_import_detail`
- Produces:
  - `CompleteImportBody` fields: `duration_ms`, `parse_ms`, `convert_ms`, `upload_ms`, `summary` (serde_json::Value or String), `issues: Vec<{kind,step,item,reason}>`
  - `GET /v1/imports/{id}` → JSON detail with timings + summary + issues
  - List response may include `duration_ms`

- [ ] **Step 1: Extend `CompleteImportBody` and wire `complete_import`**

Map body fields into `CompleteImportArgs`. Serialize `summary` to `summary_json` text (`serde_json::to_string`). Reject unknown import with 404 (existing).

- [ ] **Step 2: Add `imports_get_handler`**

Route: `.route("/v1/imports/{id}", get(imports_get_handler))` (keep `POST …/complete` as today). Auth same as list/complete. Return:

```json
{
  "id": 1,
  "source": "ios",
  "status": "completed",
  "started_at": "...",
  "finished_at": "...",
  "message_count": 10,
  "attachment_count": 2,
  "bytes_uploaded": 100,
  "duration_ms": 48000,
  "parse_ms": 18000,
  "convert_ms": 22000,
  "upload_ms": 8000,
  "summary": { },
  "issues": [ { "kind": "skip", "step": "convert", "item": "…", "reason": "…" } ]
}
```

- [ ] **Step 3: Include `duration_ms` on list summaries**

Update `ImportSummary` serde + list SELECT.

- [ ] **Step 4: Manual/API smoke (or existing test harness)**

```bash
cargo test -p message-vault-server --lib -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(vault): import complete timings and get-by-id API"
```

---

### Task 3: vault-push optional `import_id` (session reuse)

**Files:**
- Modify: `crates/cli/vault-push/src/run.rs` (`VaultPushConfig`, `setup_run` / finish)
- Modify: `crates/cli/vault-push/tests/push_mock.rs`
- Modify: `src-tauri/src/commands/push.rs`
- Modify: `web/src/lib/tauri.ts` (`PushConfig.import_id?: number`)

**Interfaces:**
- Produces: `VaultPushConfig.import_id: Option<i64>`
  - `Some(id)` → do **not** `POST /v1/imports`; do **not** complete; pass `import_id` on every `POST /v1/import`
  - `None` → today’s start + complete behavior

- [ ] **Step 1: Failing mock test**

In `push_mock.rs`, add a case that:
1. Does **not** expect `POST /v1/imports` when config has `import_id: Some(99)`
2. Expects `POST /v1/import?…&import_id=99`
3. Does **not** expect `POST /v1/imports/99/complete`

- [ ] **Step 2: Run — expect FAIL**

```bash
cargo test -p vault-push --test push_mock -- --nocapture
```

- [ ] **Step 3: Implement config + setup/finish branches**

```rust
// VaultPushConfig
pub import_id: Option<i64>,
```

In setup: if `cfg.import_id.is_some()`, use it; else call `start_import` as today.  
In finish: if `cfg.import_id.is_some()`, skip `complete_import` (caller owns it); else complete as today.

Default `import_id: None` everywhere CLI / existing Tauri call sites construct `VaultPushConfig`.

- [ ] **Step 4: Wire Tauri + TS**

`push` command takes `import_id: Option<i64>`; `invokePush` passes `importId: config.import_id ?? null`.

- [ ] **Step 5: Run mock tests — PASS**

```bash
cargo test -p vault-push --test push_mock -- --nocapture
```

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(vault-push): reuse pre-created import session id"
```

---

### Task 4: Structured progress events (push + extract → web)

**Files:**
- Modify: `src-tauri/src/commands/push.rs`
- Modify: `src-tauri/src/commands/extract.rs`
- Modify: `web/src/lib/tauri.ts`
- Optionally: `crates/core/message-vault-io-core/src/process.rs` if adding `ProcessEvent::Progress` (preferred over brittle log parsing)

**Interfaces:**
- Produces Tauri event `extract:progress` payload:

```ts
type ProgressPayload = {
  step: "parse" | "convert" | "upload";
  done: number;
  total: number;
  status?: string; // e.g. "skipped" | "copied"
};
```

Optional `extract:issue`: `{ kind: "error"|"skip"; step; item; reason }`

- Consumes: existing `ProgressEvent::FileStart { index, total, file }` for upload; parse log lines like `N/M` from iMessage emit **or** new `ProcessEvent::Progress`

- [ ] **Step 1: Emit upload progress from push command**

On `FileStart { index, total, .. }` (1-based index preferred — match existing):

```rust
let _ = app_handle.emit("extract:progress", serde_json::json!({
    "step": "upload",
    "done": index.saturating_sub(1),
    "total": total,
}));
```

On `FileDone` with non-ok status, emit `extract:issue`. Keep `extract:log` for optional details.

- [ ] **Step 2: Emit parse progress from extract**

Minimal viable path: in the extract progress callback, when a log line matches message progress (e.g. contains `messages` and `/\d+\/\d+/`), emit `extract:progress` with `step: "parse"`. Better path if cheap: add `ProcessEvent::Progress { step, done, total }` in core and have `imessage-ir-exporter` emit it at the existing 1_000-message cadence.

For convert: if extract cannot distinguish convert, emit once at extract start/end with `status: "included_in_extract"` and leave `convert_ms` null in the client (allowed by spec).

- [ ] **Step 3: Extend `awaitTauriJob` / listeners**

```ts
export type ImportProgressEvent = {
  step: "parse" | "convert" | "upload";
  done: number;
  total: number;
  status?: string;
};

// onExtractEvents({ onProgress?, onIssue?, onLog?, onFinished, onError })
```

- [ ] **Step 4: Smoke build**

```bash
cargo check -p message-vault-io # or tauri package name used by src-tauri
cd web && npx tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(tauri): emit structured import progress events"
```

---

### Task 5: ImportScreen — live steps, timings, single session, Summary

**Files:**
- Create: `web/src/components/import/ImportSummaryPanel.tsx`
- Modify: `web/src/screens/ImportScreen.tsx`

**Interfaces:**
- Consumes: Task 2 complete API; Task 3 `import_id`; Task 4 progress events
- Produces UI state matching the approved mockups (live counts → Summary)

- [ ] **Step 1: Shared Summary panel**

Props:

```ts
type ImportIssue = { kind: string; step: string; item: string; reason: string };
type ImportSummaryView = {
  status: "completed" | "failed";
  parseMessages?: number;
  convertDetail?: string; // "9840 files · 3 skipped" or status text
  uploadFiles?: number;
  parseMs?: number | null;
  convertMs?: number | null;
  uploadMs?: number | null;
  durationMs: number;
  issues: ImportIssue[];
};
```

Render step list + timing line (`Parse Xm · Convert Ym · Upload Zm · Total …`) + Errors & skips list.

- [ ] **Step 2: Rewrite `startImport` flow**

Order:

1. `t0 = performance.now()`; reset steps/issues; `setPhase("progress")`
2. `POST /v1/imports` → `importSession.id` **before** extract
3. Run extract via `awaitTauriJob` with `onProgress` updating Parse/Convert steps; record `parseMs` / `convertMs` from wall clocks around phases (extract wall → `parseMs` if convert not separate; `convertMs` null or 0 per Task 4)
4. Run push with `import_id: Number(importSession.id)` and `onProgress` for Upload; record `uploadMs`
5. `durationMs = performance.now() - t0`
6. `POST /v1/imports/{id}/complete` with `ok`, counts from push summary / report if available, timings, `summary`, `issues`
7. `setPhase("done")` and show `ImportSummaryPanel`

On failure: still attempt complete with `ok: false`, partial timings, and issues; mark steps error.

Do **not** call push without `import_id`. Remove reliance on vague “Extraction complete” as the only detail.

- [ ] **Step 3: Live elapsed clock**

`setInterval` / `requestAnimationFrame` while `running` updating header Elapsed from `t0`. Clear on finish.

- [ ] **Step 4: Typecheck**

```bash
cd web && npx tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(import): live step progress and summary with timings"
```

---

### Task 6: Storage Import history detail

**Files:**
- Modify: `web/src/screens/settings/StorageSection.tsx`
- Reuse: `ImportSummaryPanel`

**Interfaces:**
- Consumes: `GET /v1/imports/{id}`

- [ ] **Step 1: Make history rows clickable**

On click, fetch detail, show a detail panel/modal below the table (or replace table temporarily with Back + Summary). Map API timings/issues into `ImportSummaryView`.

Optional: show `duration_ms` column in the table when present.

- [ ] **Step 2: Typecheck**

```bash
cd web && npx tsc --noEmit
```

- [ ] **Step 3: Manual check list (document in commit body)**

- One Import Messages run → one history row
- Row detail timings match Summary
- Messages for that run have `import_id` = that session (SQL or Storage counts)

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(settings): reopen import summary from history"
```

---

### Task 7: Spec status + docs touch (light)

**Files:**
- Modify: `docs/superpowers/specs/2026-08-11-import-progress-summary-design.md` (status already Approved — confirm)
- Optionally one sentence in `docs/maintainers/gui.md` under Import if that section documents the old log-centric UI

- [ ] **Step 1: Align maintainer note** only if Import is documented there; skip if not.

- [ ] **Step 2: Final verification**

```bash
cargo test -p message-vault-server --lib db::vault_imports
cargo test -p vault-push --test push_mock
cd web && npx tsc --noEmit
```

- [ ] **Step 3: Commit** only if docs changed.

---

## Self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| Live step `done/total` | 4, 5 |
| Summary totals + errors/skips | 5, 1 |
| Stage + total timings in UI | 5 |
| Timings columns in DB | 1, 2 |
| `vault_import_issues` | 1, 2 |
| Single session; messages linked | 3, 5 |
| GUI owns complete when import_id set | 3, 5 |
| CLI push unchanged without import_id | 3 |
| History reopen Summary | 6 |
| No full success file list | 1, 5 |
| No live DB polling | 4, 5 |
| Ignore push chunk total time | 5 (client wall clock) |

---

## Execution notes

- Prefer measuring stage walls on the client around `awaitTauriJob` boundaries; push `elapsed_ms` is upload-only and must not replace `duration_ms`.
- If convert cannot be separated from extract in v1, store `convert_ms: null` and put a short note in `summary_json` / convert step detail — do not invent fake counters.
