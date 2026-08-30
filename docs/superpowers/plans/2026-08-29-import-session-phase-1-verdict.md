# Import Session Phase 1 — Verdict and Reason Vocabulary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An import's outcome is computed from the push report — three outcomes with a zero floor — and every missing-attachment reason is a member of a closed set that survives to the screen.

**Architecture:** A pure `importOutcome` function in the web app reads the `PushFinishedReport` that `useImportJob` already receives and ignores today, producing `completed` / `completed_with_issues` / `failed`. The server's `/v1/imports/{id}/complete` accepts that status alongside the existing `ok` flag, and the vault-push CLI derives the same status for its self-completed sessions. Independently, two library fixes land: a read error on one attachment stops killing the whole extract, and the missing-reason vocabulary collapses to the closed set from spec decision 41.

**Tech Stack:** Rust (Axum server, vault-push CLI, message-vault-io-core, imessage-ir-exporter), React 19 + TypeScript (Vitest), sqlx over SQLite/Postgres.

**Spec:** `docs/superpowers/specs/2026-08-29-import-session-design.md` — this plan implements sequencing step 1 (decisions 21, 22, 40, 41). Phases 2–4 (session record, gates, prepare restructure) get their own plans once this one lands.

**Why this is first:** it fixes a live bug — the 2026-08-27 iPhone run failed all 681 conversations, inserted nothing, and was recorded `completed`, because `useImportJob.ts:383` sets `importCompleted = true` the moment `invokePush` returns and push runs with `continue_on_error: true`, so it returns a report instead of throwing. Nothing reads the report.

## Global Constraints

- Never commit to `main`; work stays on this branch. Never create or push `v*` tags. Do not merge PRs unless asked.
- Version lockstep files are **not touched** by this plan (no version bump).
- Status strings, exactly: `completed`, `completed_with_issues`, `failed` (stored), plus the pre-existing `running` and `cancelled`/`canceled` values, which this plan does not change.
- Missing-reason strings, exactly (spec decision 41): `file_missing`, `too_large`, `not_copied`, `convert_failed: <detail>`, `unknown: <raw>`. Legacy values `skipped` and `embed_disabled` are already stored in user vaults — display keeps recognizing them forever; writers stop producing them.
- The word "transcode" never appears in user-facing copy (spec decision 18).
- Rust: `cargo fmt --all -- --check` must pass; run `./scripts/lint-all.sh` before finishing. Web: Biome (`cd web && npm run lint`), unused bindings prefixed `_`, real fixes over `biome-ignore`.
- Tests use committed fixtures only; never real message data.
- `docs/src/assets/openapi.json` has a committed-dump gate: any change to a `utoipa::ToSchema` type must regenerate it in the same commit (`cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`), or `committed_openapi_matches_dump` fails.
- Literal code below was written against the tree at `5f85ee08`. Where a snippet and the compiler disagree, the compiler is authoritative — keep the intent, fix the syntax.
- Commit after every task.

---

### Task 1: `importOutcome` — the verdict comes from the push report

A pure function turning the push report + whether the job threw into the three-way outcome. Nothing is wired yet; that is Task 3.

**Files:**
- Create: `web/src/screens/import/importOutcome.ts`
- Modify: `web/src/lib/tauri.ts:57` (export the `PushFinishedReport` interface)
- Test: `web/src/screens/import/importOutcome.test.ts` (create)

**Interfaces:**
- Consumes: `PushFinishedReport` from `web/src/lib/tauri.ts` (currently a non-exported `interface PushFinishedReport` — this task adds `export`).
- Produces: `type ImportOutcome = "completed" | "completed_with_issues" | "failed"` and `importOutcome(args: { report: PushFinishedReport | undefined; threw: boolean; issues: readonly { kind: string }[] }): ImportOutcome`, both exported. Task 3 calls it; Task 4 mirrors the same rules in Rust.

The rules, from spec decisions 21–22:

1. `failed` when the job threw (interrupt, cancel, network death) or there is no report at all.
2. `failed` when nothing landed: `conversations_total > 0` and `conversations_ok === 0` and `conversations_skipped === 0`. The skipped guard keeps a re-push where every conversation dedupes to a no-op from reading as a failure.
3. `completed_with_issues` when anything item-level went wrong: `conversations_failed > 0`, `messages_failed > 0`, or any recorded issue. (Phase 3's approval contract will later exempt user-approved skips; until gates exist, a skip is worth surfacing.)
4. `completed` otherwise.

- [ ] **Step 1: Export the report type**

In `web/src/lib/tauri.ts`, change line 57 from `interface PushFinishedReport {` to `export interface PushFinishedReport {`.

- [ ] **Step 2: Write the failing test**

Create `web/src/screens/import/importOutcome.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { PushFinishedReport } from "../../lib/tauri";
import { importOutcome } from "./importOutcome";

function report(overrides: Partial<PushFinishedReport> = {}): PushFinishedReport {
  return {
    ok: true,
    messages: 100,
    messages_attempted: 100,
    messages_inserted: 100,
    messages_deduped: 0,
    messages_failed: 0,
    assets_uploaded: 5,
    assets_bytes: 1_000,
    conversations_ok: 10,
    conversations_total: 10,
    conversations_failed: 0,
    conversations_skipped: 0,
    results: [],
    ...overrides,
  };
}

describe("importOutcome", () => {
  it("is completed for a clean run", () => {
    expect(importOutcome({ report: report(), threw: false, issues: [] })).toBe("completed");
  });

  it("is failed when the job threw, whatever the report says", () => {
    expect(importOutcome({ report: report(), threw: true, issues: [] })).toBe("failed");
  });

  it("is failed when there is no report at all", () => {
    expect(importOutcome({ report: undefined, threw: false, issues: [] })).toBe("failed");
  });

  it("is failed when every conversation failed and nothing landed (2026-08-27 shape)", () => {
    const r = report({
      ok: false,
      conversations_total: 681,
      conversations_ok: 0,
      conversations_failed: 681,
      conversations_skipped: 0,
      messages_inserted: 0,
      messages_failed: 8_000,
    });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("failed");
  });

  it("is completed when a re-push dedupes everything to skips", () => {
    const r = report({
      conversations_total: 10,
      conversations_ok: 0,
      conversations_failed: 0,
      conversations_skipped: 10,
      messages_inserted: 0,
    });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("completed");
  });

  it("is completed_with_issues when some conversations failed but others landed", () => {
    const r = report({ ok: false, conversations_ok: 8, conversations_failed: 2 });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("completed_with_issues");
  });

  it("is completed_with_issues when messages failed inside ok conversations", () => {
    const r = report({ messages_failed: 3 });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("completed_with_issues");
  });

  it("is completed_with_issues when the run recorded an issue", () => {
    expect(
      importOutcome({ report: report(), threw: false, issues: [{ kind: "skip" }] }),
    ).toBe("completed_with_issues");
  });
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run from `web/`: `npx vitest run src/screens/import/importOutcome.test.ts`
Expected: FAIL — `importOutcome.ts` does not exist.

- [ ] **Step 4: Write the implementation**

Create `web/src/screens/import/importOutcome.ts`:

```ts
import type { PushFinishedReport } from "../../lib/tauri";

export type ImportOutcome = "completed" | "completed_with_issues" | "failed";

/**
 * Three-way verdict for a finished import, read from the push report rather
 * than from whether the push call returned (spec decisions 21–22).
 *
 * `failed` has a zero floor: interrupted, threw, or nothing landed at all.
 * A re-push where every conversation dedupes to a skip is a no-op, not a
 * failure. Item-level problems make it `completed_with_issues`.
 */
export function importOutcome(args: {
  report: PushFinishedReport | undefined;
  threw: boolean;
  issues: readonly { kind: string }[];
}): ImportOutcome {
  const { report, threw, issues } = args;
  if (threw || !report) return "failed";
  const nothingLanded =
    report.conversations_total > 0 &&
    report.conversations_ok === 0 &&
    report.conversations_skipped === 0;
  if (nothingLanded) return "failed";
  if (report.conversations_failed > 0 || report.messages_failed > 0 || issues.length > 0) {
    return "completed_with_issues";
  }
  return "completed";
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run from `web/`: `npx vitest run src/screens/import/importOutcome.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 6: Lint and commit**

```bash
cd web && npm run lint
git add web/src/screens/import/importOutcome.ts web/src/screens/import/importOutcome.test.ts web/src/lib/tauri.ts
git commit -m "feat(web): compute the import verdict from the push report"
```

---

### Task 2: The server stores the three-way status

`POST /v1/imports/{id}/complete` today collapses everything to `completed` / `failed` via the `ok` boolean. This task lets the body carry an explicit `status`, validated against the closed set, with `ok` kept for older clients.

**Files:**
- Modify: `crates/vault/server/src/import/mod.rs` (`CompleteImportBody` around line 595, `imports_complete_handler` around line 828, add `validate_import_status`)
- Modify: `crates/vault/server/src/db/vault_imports.rs` (`CompleteImportArgs` around line 56, `complete_import` around line 315)
- Modify: `crates/vault/server/src/server.rs` (existing test-body literals gain the new field; one new test)
- Modify: `docs/src/assets/openapi.json` (regenerated)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `CompleteImportBody.status: Option<String>` (accepted values exactly `"completed"`, `"completed_with_issues"`, `"failed"`); `crate::db::vault_imports::CompleteImportArgs.status: Option<String>`. When `status` is absent, behavior is unchanged: `ok: true` → `completed`, `ok: false` → `failed`. Tasks 3 and 4 send the field.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `crates/vault/server/src/server.rs`, next to `imports_complete_and_detail_surface_timings_and_issues`, add:

```rust
    #[tokio::test]
    async fn imports_complete_stores_completed_with_issues_status() {
        let (_tmp, state, token, import_id) = test_state().await;
        let body = CompleteImportBody {
            ok: true,
            status: Some("completed_with_issues".into()),
            message_count: Some(10),
            attachment_count: Some(2),
            bytes_uploaded: Some(100),
            duration_ms: None,
            parse_ms: None,
            attachments_ms: None,
            prepare_ms: None,
            upload_ms: None,
            summary: None,
            issues: Vec::new(),
        };
        let response = imports_complete_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(body),
        )
        .await
        .unwrap();
        assert_eq!(response.0.status, "completed_with_issues");
    }

    #[tokio::test]
    async fn imports_complete_rejects_unknown_status() {
        let (_tmp, state, token, import_id) = test_state().await;
        let body = CompleteImportBody {
            ok: true,
            status: Some("victorious".into()),
            message_count: None,
            attachment_count: None,
            bytes_uploaded: None,
            duration_ms: None,
            parse_ms: None,
            attachments_ms: None,
            prepare_ms: None,
            upload_ms: None,
            summary: None,
            issues: Vec::new(),
        };
        let err = imports_complete_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(body),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));

        // The session is untouched.
        let mut conn = state.db.acquire().await.unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM vault_imports WHERE id = $1")
            .bind(import_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(status, "running");
    }
```

Match the surrounding tests for imports and helpers (`test_state`, `auth_headers`, `ApiError` are already in scope there; mirror `imports_complete_rejects_invalid_issue_kind_before_db_write` for the error-shape assertion if `matches!` on `ApiError` needs adjusting).

- [ ] **Step 2: Run to verify they fail to compile**

Run: `cargo test -p message-vault-server imports_complete`
Expected: compile error — `CompleteImportBody` has no field `status`.

- [ ] **Step 3: Add the field, the validator, and the plumbing**

In `crates/vault/server/src/import/mod.rs`, add to `CompleteImportBody` (after `ok`):

```rust
    /// Explicit session outcome; overrides `ok` when present.
    /// One of `completed`, `completed_with_issues`, `failed`.
    #[serde(default)]
    pub(crate) status: Option<String>,
```

Add next to `validate_complete_import_issues`:

```rust
fn validate_import_status(status: Option<&str>) -> Result<(), ApiError> {
    match status {
        None | Some("completed") | Some("completed_with_issues") | Some("failed") => Ok(()),
        Some(other) => Err(ApiError::BadRequest(format!(
            "invalid import status '{other}'; expected 'completed', 'completed_with_issues', or 'failed'"
        ))),
    }
}
```

In `imports_complete_handler`, after the `validate_complete_import_issues(&body.issues)?;` line, add:

```rust
    validate_import_status(body.status.as_deref())?;
```

and in the `CompleteImportArgs` construction add `status: body.status,` after `ok: body.ok,`.

In `crates/vault/server/src/db/vault_imports.rs`:

- Add to `CompleteImportArgs` (after `ok`): 

```rust
    /// Explicit outcome status; falls back to `ok` when `None`.
    pub status: Option<String>,
```

- The `succeeded` and `failed` constructors need no edit: both end in `..Default::default()`, and the new `Option<String>` defaults to `None`.
- In `complete_import`, replace 

```rust
    let status = if args.ok { "completed" } else { "failed" };
```

with

```rust
    let status = args
        .status
        .as_deref()
        .unwrap_or(if args.ok { "completed" } else { "failed" });
```

- [ ] **Step 4: Fix the existing struct literals**

Every `CompleteImportBody { … }` literal in `crates/vault/server/src/server.rs` tests and every `CompleteImportArgs { … }` literal anywhere in the server crate now needs `status: None,` (or the test's explicit value). Find them:

```bash
grep -rn "CompleteImportBody {\|CompleteImportArgs {" crates/vault/server/src/
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p message-vault-server imports_complete`
Expected: PASS, including the two new tests and the pre-existing ones.

- [ ] **Step 6: Regenerate the OpenAPI dump**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
cargo test -p message-vault-server committed_openapi_matches_dump
```

Expected: the dump test passes with the regenerated file.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all -- --check
git add crates/vault/server/src docs/src/assets/openapi.json
git commit -m "feat(server): imports complete accepts a three-way status"
```

---

### Task 3: The web sends the verdict and the screens show it

Wire `importOutcome` into `useImportJob`, add `completed_with_issues` to the summary status union, and give it a completion line and a history mapping.

**Files:**
- Modify: `web/src/screens/import/useImportJob.ts` (the `startImport` try/catch/finally, lines ~236–470)
- Modify: `web/src/components/import/ImportSummaryPanel.tsx:12` (status union), `:53-60` (`completionTextFor`)
- Modify: `web/src/screens/settings/storage/storageUtils.ts:80-91` (`toSummaryStatus`)
- Test: `web/src/components/import/ImportSummaryPanel.test.tsx`, `web/src/screens/settings/storage/storageUtils.test.ts` (extend if present; create the assertion in the existing test file that covers `toImportSummaryView` — check `ls web/src/screens/settings/storage/*.test.*`)

**Interfaces:**
- Consumes: `importOutcome`, `ImportOutcome` from Task 1; server `status` field from Task 2.
- Produces: `ImportSummaryView["status"]` union gains `"completed_with_issues"`. `completionTextFor("completed_with_issues")` returns `"Import completed with issues"`. The `/complete` body now carries `status` and `ok: status !== "failed"`.

- [ ] **Step 1: Write the failing tests**

In `web/src/components/import/ImportSummaryPanel.test.tsx`, add (mirroring the existing `completionTextFor` coverage; if none exists, add a small describe block):

```ts
import { completionTextFor } from "./ImportSummaryPanel";

describe("completionTextFor", () => {
  it("names the with-issues outcome", () => {
    expect(completionTextFor("completed_with_issues")).toBe("Import completed with issues");
  });
});
```

In the storage utils test file (create `web/src/screens/settings/storage/storageUtils.test.ts` if none exists), assert the mapping:

```ts
import { describe, expect, it } from "vitest";
import { toImportSummaryView } from "./storageUtils";

describe("toImportSummaryView", () => {
  it("passes completed_with_issues through", () => {
    const detail = {
      id: 1,
      source: "imessage",
      tool: null,
      mode: "append",
      status: "completed_with_issues",
      started_at: "2026-08-29T00:00:00Z",
      finished_at: "2026-08-29T00:10:00Z",
      message_count: 10,
      attachment_count: 2,
      bytes_uploaded: 100,
      duration_ms: 1000,
      parse_ms: null,
      attachments_ms: null,
      prepare_ms: null,
      upload_ms: null,
      summary: null,
      issues: [],
    };
    // Cast through the response type the file already imports.
    expect(toImportSummaryView(detail as never).status).toBe("completed_with_issues");
  });
});
```

Adjust the `detail` literal to the real `ImportDetailResponse` shape the module imports — the compiler tells you the missing fields; the assertion is the point.

- [ ] **Step 2: Run to verify they fail**

Run from `web/`: `npx vitest run src/components/import/ImportSummaryPanel.test.tsx src/screens/settings/storage/storageUtils.test.ts`
Expected: FAIL — the union rejects the string / `toSummaryStatus` maps it to `"failed"`.

- [ ] **Step 3: Widen the union and the mappings**

In `ImportSummaryPanel.tsx`:

```ts
  status: "completed" | "completed_with_issues" | "failed" | "canceled" | "running";
```

and in `completionTextFor`, after the `completed` line:

```ts
  if (status === "completed_with_issues") return "Import completed with issues";
```

`historySteps` treats the new status like `completed` (all steps done) — its `running` / `failed` branches already do the right thing by not matching; verify no other `summary.status ===` comparison in the file needs a case.

In `storageUtils.ts` `toSummaryStatus`, add before `default`:

```ts
    case "completed_with_issues":
      return "completed_with_issues";
```

- [ ] **Step 4: Wire the verdict into `useImportJob`**

In `web/src/screens/import/useImportJob.ts`:

1. Add the import: `import { importOutcome } from "./importOutcome";`
2. Replace the declaration `let importCompleted = false;` with `let threw = false;`
3. Delete the line `importCompleted = true;` (immediately after `uploadMs = performance.now() - uploadStartedAt;`).
4. In the `catch` block, first line: `threw = true;`
5. In the `finally` block, replace:

```ts
      const pushReport = pushResult?.report;
      const finalSummary: ImportSummaryView = {
        status: importCompleted ? "completed" : "failed",
```

with:

```ts
      const pushReport = pushResult?.report;
      const outcome = importOutcome({ report: pushReport, threw, issues: issuesRef.current });
      const finalSummary: ImportSummaryView = {
        status: outcome,
```

6. In the `/v1/imports/{id}/complete` body, replace `ok: importCompleted,` with:

```ts
            ok: outcome !== "failed",
            status: outcome,
```

- [ ] **Step 5: Run the web suite**

Run from `web/`: `npm test`
Expected: PASS. Pay attention to any test that asserted the old always-completed behavior — if one exists, it was asserting the bug; update it to the report-driven expectation and say so in the commit message.

- [ ] **Step 6: Lint and commit**

```bash
cd web && npm run lint
git add web/src
git commit -m "fix(web): a failed push no longer reports a completed import"
```

---

### Task 4: The CLI derives the same verdict for self-completed sessions

When vault-push starts its own session (`cfg.import_id.is_none()`), it completes it with `ok: report.ok` — which is `false` whenever any conversation failed, so one bad conversation among hundreds records the whole run `failed`, and an aborted run with partial progress is indistinguishable from an item failure. Mirror the Task 1 rules in Rust and send the status.

**Files:**
- Modify: `crates/cli/vault-push/src/run.rs` (new `outcome_status` fn; the `complete_import` call around line 1075; test module)
- Modify: `crates/cli/vault-push/src/http.rs` (`CompleteImportArgs` at line 100; `complete_import` body at line 696)

**Interfaces:**
- Consumes: server `status` field from Task 2 (older servers ignore unknown JSON fields — the body change is backward-compatible).
- Produces: `pub fn outcome_status(report: &PushReport, aborted: bool) -> &'static str` in `run.rs`; `CompleteImportArgs.status: &'a str` in `http.rs`. Phase 2's plan will reuse `outcome_status` unchanged.

- [ ] **Step 1: Write the failing test**

In the `tests` module of `crates/cli/vault-push/src/run.rs` (there is one near the bottom — a `PushReport` literal already appears around line 3040; mirror its field list exactly), add:

```rust
    fn sample_report() -> PushReport {
        PushReport {
            ok: true,
            account: "acct".into(),
            username: "user".into(),
            mode: "append".into(),
            started_at: "2026-08-29T00:00:00Z".into(),
            finished_at: "2026-08-29T00:01:00Z".into(),
            elapsed_ms: 60_000,
            conversations_total: 10,
            conversations_ok: 10,
            conversations_failed: 0,
            conversations_skipped: 0,
            messages_attempted: 100,
            messages_inserted: 90,
            messages_deduped: 10,
            messages_failed: 0,
            messages: 100,
            assets_uploaded: 5,
            assets_skipped: 0,
            assets_bytes: 1_000,
            results: Vec::new(),
        }
    }

    #[test]
    fn outcome_status_matches_the_spec_verdicts() {
        // Clean run.
        assert_eq!(outcome_status(&sample_report(), false), "completed");

        // Aborted is failed regardless of counts.
        assert_eq!(outcome_status(&sample_report(), true), "failed");

        // Nothing landed at all: the zero floor.
        let mut nothing = sample_report();
        nothing.ok = false;
        nothing.conversations_ok = 0;
        nothing.conversations_failed = 10;
        assert_eq!(outcome_status(&nothing, false), "failed");

        // A skip-only re-push is a no-op, not a failure.
        let mut skips = sample_report();
        skips.conversations_ok = 0;
        skips.conversations_skipped = 10;
        assert_eq!(outcome_status(&skips, false), "completed");

        // Item-level failures beside successes.
        let mut partial = sample_report();
        partial.ok = false;
        partial.conversations_ok = 8;
        partial.conversations_failed = 2;
        assert_eq!(outcome_status(&partial, false), "completed_with_issues");

        // Message failures inside ok conversations.
        let mut msgs = sample_report();
        msgs.messages_failed = 3;
        assert_eq!(outcome_status(&msgs, false), "completed_with_issues");
    }
```

If `PushReport` has fields the literal above misses, add them — the compiler is authoritative; the assertions are the contract.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vault-push outcome_status`
Expected: compile error — `outcome_status` not found.

- [ ] **Step 3: Implement**

In `crates/cli/vault-push/src/run.rs`, near `format_push_summary` (line ~300):

```rust
/// Three-way session status for `/v1/imports/{id}/complete` (import-session
/// spec, decisions 21–22). `failed` has a zero floor: aborted, or nothing
/// landed at all. A skip-only re-push is a no-op, not a failure. Item-level
/// failures beside successes are `completed_with_issues`.
pub fn outcome_status(report: &PushReport, aborted: bool) -> &'static str {
    let nothing_landed = report.conversations_total > 0
        && report.conversations_ok == 0
        && report.conversations_skipped == 0;
    if aborted || nothing_landed {
        return "failed";
    }
    if report.conversations_failed > 0 || report.messages_failed > 0 {
        return "completed_with_issues";
    }
    "completed"
}
```

In `crates/cli/vault-push/src/http.rs`, add to `CompleteImportArgs`:

```rust
    /// Session outcome: `completed`, `completed_with_issues`, or `failed`.
    pub status: &'a str,
```

In `HttpSession::complete_import`, add `status` to the destructuring `let CompleteImportArgs { … }` and to the body:

```rust
        let body = serde_json::json!({
            "ok": ok,
            "status": status,
            "message_count": message_count,
            "attachment_count": attachment_count,
            "bytes_uploaded": bytes_uploaded,
        });
```

At the call site in `run.rs` (~line 1075), add to the `CompleteImportArgs` literal:

```rust
            status: outcome_status(&report, aborted),
```

(`aborted` is in scope — it feeds `report.ok` two dozen lines up. If the borrow checker objects to `&report` there, compute `let status = outcome_status(&report, aborted);` before the literal.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p vault-push`
Expected: PASS, including the new test and every existing one (a second `CompleteImportArgs` literal may exist in tests — the compiler will list them).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all -- --check
git add crates/cli/vault-push/src
git commit -m "feat(vault-push): self-completed sessions carry the three-way verdict"
```

---

### Task 5: A read error on one attachment stops killing the extract

`run_attachment_jobs` treats a missing attachment as per-item (`file_missing`, continue) but a *read error* as fatal to the entire run via `let loaded = load(i)?;` — one unreadable file kills a multi-hour extract (spec decision 40). Treat a load error exactly like a missing file, while still letting a cancel raised inside the loader abort.

**Files:**
- Modify: `crates/core/message-vault-io-core/src/attachment_jobs.rs:86` (the `load(i)?` line)
- Test: same file, `tests` module

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: no signature change. New behavior: `load(i)` returning `Err` (other than `"canceled"`) marks that attachment `file_missing` and continues.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `attachment_jobs.rs`, next to `missing_source_is_file_missing_and_continues`:

```rust
    #[test]
    fn read_error_marks_file_missing_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let mut b = empty_att("b.jpg");
        {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut a,
                    timestamp_unix_ms: 0,
                    size_hint: None,
                },
                AttachmentJob {
                    attachment: &mut b,
                    timestamp_unix_ms: 0,
                    size_hint: Some(4),
                },
            ];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                MediaMode::Clone,
                &CompressOptions::default(),
                |i| {
                    if i == 0 {
                        Err("permission denied".into())
                    } else {
                        Ok(Some(b"data".to_vec()))
                    }
                },
                |_| {},
                None,
            )
            .unwrap();
        }
        assert_eq!(a.missing_reason.as_deref(), Some("file_missing"));
        assert!(b.path.is_some());
    }

    #[test]
    fn canceled_error_from_the_loader_still_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let err = {
            let mut jobs = [AttachmentJob {
                attachment: &mut a,
                timestamp_unix_ms: 0,
                size_hint: Some(1),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                MediaMode::Clone,
                &CompressOptions::default(),
                |_| Err("canceled".into()),
                |_| {},
                None,
            )
            .unwrap_err()
        };
        assert_eq!(err, "canceled");
    }
```

- [ ] **Step 2: Run to verify the first fails**

Run: `cargo test -p message-vault-io-core read_error_marks`
Expected: FAIL — the run returns `Err("permission denied")` instead of `Ok`.

- [ ] **Step 3: Implement**

Replace `let loaded = load(i)?;` with:

```rust
        let loaded = match load(i) {
            Ok(loaded) => loaded,
            // A cancel raised inside the loader still stops the run.
            Err(err) if err == "canceled" => return Err(err),
            // One unreadable source is that attachment's problem, not the
            // run's. Fall through to the missing-file handling below.
            Err(_) => None,
        };
```

The existing `match loaded { Some(bytes) if !bytes.is_empty() => …, _ => file_missing + continue }` does the rest.

- [ ] **Step 4: Run the crate tests**

Run: `cargo test -p message-vault-io-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/message-vault-io-core/src/attachment_jobs.rs
git commit -m "fix(core): one unreadable attachment no longer kills the extract"
```

---

### Task 6: `not_copied` replaces the two spellings of "Do not copy"

`skipped` (shared exporters) and `embed_disabled` (iMessage) are one condition written two ways (spec decision 41). Writers converge on `not_copied`; already-stored data keeps its old strings and Task 7 keeps displaying them.

**Files:**
- Modify: `crates/core/message-vault-io-core/src/attachment_jobs.rs:64` (the `MediaMode::Disabled` branch) and its test at line ~340
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs:395` (the `AttachmentEmbed::Disabled` arm) and its test at line ~1142
- Modify: `crates/cli/vault-push/src/run.rs:1792` (comment only — it names the two old spellings)

**Interfaces:**
- Consumes: nothing.
- Produces: the stored reason string `not_copied`. Task 7 displays it; Phase 3's Gate copy ("Not copied, by your import setting") reads it.

- [ ] **Step 1: Update the two tests first**

In `attachment_jobs.rs` test `disabled_skips_without_loading`, change:

```rust
        assert_eq!(att.missing_reason.as_deref(), Some("not_copied"));
```

In `emit.rs` (test around line 1142), change:

```rust
        assert_eq!(
            ir_disabled.attachments[0].missing_reason.as_deref(),
            Some("not_copied")
        );
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p message-vault-io-core disabled_skips && cargo test -p imessage-ir-exporter`
Expected: both assertions FAIL against the old strings.

- [ ] **Step 3: Change the writers**

`attachment_jobs.rs:64`: `Some("skipped".into())` → `Some("not_copied".into())`.

`emit.rs:395`: `Some("embed_disabled".to_string())` → `Some("not_copied".to_string())`.

`run.rs:1792` comment: change `("skipped" / "embed_disabled")` to `("not_copied"; older exports say "skipped" or "embed_disabled")`.

- [ ] **Step 4: Sweep for other writers**

```bash
grep -rn '"skipped"\|"embed_disabled"' crates/ --include=*.rs
```

Remaining hits must be: the vault-push *conversation-status* string `"skipped"` in `run.rs` (a different vocabulary — conversations, not attachment reasons; leave it), test fixtures, and the comment just updated. Anything else writing an attachment `missing_reason` gets the same rename.

- [ ] **Step 5: Run the workspace tests and commit**

Run: `cargo test --workspace`
Expected: PASS.

```bash
cargo fmt --all -- --check
git add crates
git commit -m "refactor: one reason string for attachments not copied by setting"
```

---

### Task 7: The display side becomes a closed set with an explicit unknown

`missingAttachmentLabel.ts` recognizes four reasons and flattens everything else — including `convert_failed: <detail>`, the only reason carrying a real explanation — to a bare "missing" (spec decision 41).

**Files:**
- Modify: `web/src/lib/missingAttachmentLabel.ts` (the `missingWhy` helper)
- Test: `web/src/lib/missingAttachmentLabel.test.ts`

**Interfaces:**
- Consumes: `not_copied` from Task 6; legacy `skipped` / `embed_disabled` stay recognized (they are stored in user vaults).
- Produces: chip wording per reason. Phase 3's Gate 2 tables reuse this vocabulary.

- [ ] **Step 1: Write the failing tests**

Add to `missingAttachmentLabel.test.ts` (using the existing `att` helper):

```ts
  it("labels not_copied and both legacy spellings as skipped", () => {
    for (const reason of ["not_copied", "skipped", "embed_disabled"]) {
      expect(
        missingAttachmentChipLabel(att({ original_name: "a.jpg", missing_reason: reason })),
      ).toBe("a.jpg (skipped)");
    }
  });

  it("keeps the ffmpeg detail from a convert_failed reason", () => {
    expect(
      missingAttachmentChipLabel(
        att({ original_name: "clip.mov", missing_reason: "convert_failed: no video stream" }),
      ),
    ).toBe("clip.mov (could not be converted — no video stream)");
  });

  it("shows an explicit unknown reason instead of swallowing it", () => {
    expect(
      missingAttachmentChipLabel(
        att({ original_name: "a.bin", missing_reason: "unknown: gremlins" }),
      ),
    ).toBe("a.bin (could not be imported — gremlins)");
  });

  it("keeps an unrecognized raw reason visible", () => {
    expect(
      missingAttachmentChipLabel(att({ original_name: "a.bin", missing_reason: "weird_reason" })),
    ).toBe("a.bin (missing — weird_reason)");
  });

  it("treats no_path and null as plain missing", () => {
    expect(missingAttachmentChipLabel(att({ original_name: "a.bin", missing_reason: "no_path" }))).toBe(
      "a.bin (missing)",
    );
    expect(missingAttachmentChipLabel(att({ original_name: "a.bin" }))).toBe("a.bin (missing)");
  });
```

- [ ] **Step 2: Run to verify they fail**

Run from `web/`: `npx vitest run src/lib/missingAttachmentLabel.test.ts`
Expected: FAIL — new reasons flatten to `(missing)`.

- [ ] **Step 3: Implement**

Replace `missingWhy` in `missingAttachmentLabel.ts`:

```ts
/** Short reason shown in parentheses on a missing-attachment chip. */
function missingWhy(reason: string | null | undefined): string {
  if (!reason || reason === "no_path") return "missing";
  if (reason === "too_large") return "missing — too large";
  if (reason === "file_missing") return "missing — file not found";
  // Chosen on import ("Do not copy"), so the file is absent by request, not
  // lost. Writers say "not_copied"; older exports stored "skipped" (shared
  // exporters) or "embed_disabled" (iMessage).
  if (reason === "not_copied" || reason === "skipped" || reason === "embed_disabled") {
    return "skipped";
  }
  if (reason.startsWith("convert_failed: ")) {
    return `could not be converted — ${reason.slice("convert_failed: ".length)}`;
  }
  if (reason.startsWith("unknown: ")) {
    return `could not be imported — ${reason.slice("unknown: ".length)}`;
  }
  // Keep an unrecognized reason visible and reportable, never uniform.
  return `missing — ${reason}`;
}
```

- [ ] **Step 4: Run the test and the suite**

Run from `web/`: `npx vitest run src/lib/missingAttachmentLabel.test.ts && npm test`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cd web && npm run lint
git add web/src/lib/missingAttachmentLabel.ts web/src/lib/missingAttachmentLabel.test.ts
git commit -m "feat(web): missing-attachment reasons survive to the chip"
```

---

### Final verification

- [ ] Run the full gate: `./scripts/check-pr.sh` (rustfmt both trees, workspace build + test, src-tauri build, Biome, Vitest, docs check).
- [ ] Sanity-check the live bug is dead: in `useImportJob.ts`, `status` comes from `importOutcome` and nothing sets `completed` from the mere return of `invokePush`.
- [ ] Confirm no user-facing string says "transcode".
