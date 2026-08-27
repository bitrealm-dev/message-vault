# Import Session Source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Desktop Import opens the vault session with the JSONL chat-kind slug (`imessage`, `whatsapp`) instead of the Platform method id, so conversation uploads stop failing `source mismatch`.

**Architecture:** A pure helper, `vaultSourceForMethod`, maps iMessage and WhatsApp method ids to the existing vault source constants. `useImportJob` uses that helper only on `POST /v1/imports`. Extract, staging, remembered paths, and saved-group names keep `form.source`. Exporters and `vault-push` stay unchanged.

**Tech Stack:** TypeScript in `web/`, Vitest. No Playwright. No Rust changes.

**Spec:** `docs/superpowers/specs/2026-08-27-import-session-source-design.md`

## Global Constraints

- Keep `imessage` and `whatsapp` as the vault source. Do not write method ids into `messages.source` or asset folders.
- Map only when creating the session (`POST /v1/imports`). Extract (`invokeExtract`), staging directories, remembered paths, and saved-group names keep the method id.
- Mapping: `imessage-ios` / `imessage-macos` / `imessage-jailbreak` → `imessage`; `whatsapp-android` / `whatsapp-ios` → `whatsapp`; anything else, including `sms-backup-restore` and unknown strings, is returned unchanged.
- Do not change exporter `EXPORT_SOURCE`, `vault-push` `detect_source()`, or the vault match check.
- Do not mix this with Import Errors grouping ([issue 202](https://github.com/bitrealm-io/message-vault/issues/202)).
- Import is desktop-only. Prove this with Vitest, not Playwright against Vite.
- Prefer a real fix over `biome-ignore`. Prefix unused bindings with `_`.
- Never commit to `main`. Work on `fix/import-session-source`.
- Product version files stay at the current lockstep value. Do not bump versions.
- Do not commit `docs/package.json` or `docs/package-lock.json` if they are dirty from an unrelated install.
- Do not implement in the main checkout if it is still on `fix/import-progress-steps` or another unrelated branch.

## File map

| File | Responsibility |
|---|---|
| `web/src/lib/vaultSource.ts` | Pure helper: method id → vault session source |
| `web/src/lib/vaultSource.test.ts` | Mapping table and session-create body tests |
| `web/src/screens/import/useImportJob.ts` | Session create uses the helper; extract, staging, and saved groups stay on `form.source` |
| `CHANGELOG.md` | Unreleased Fixed note dated 2026-08-27 |

Out of scope files: exporter crates, `crates/cli/**` (`vault-push`), `crates/vault/server/src/db/vault_imports.rs`, `web/src/lib/savedGroups.ts`, Playwright specs.

---

### Task 0: Branch and record the plan

**Files:**
- Create: this plan at `docs/superpowers/plans/2026-08-27-import-session-source.md`
- Existing: `docs/superpowers/specs/2026-08-27-import-session-source-design.md` on branch `docs/import-session-source`

**Interfaces:**
- Consumes: locked spec on disk
- Produces: git branch `fix/import-session-source` with spec + plan committed

- [ ] **Step 1: Confirm or create the isolated branch**

The spec commit lives on `docs/import-session-source` (`e5d99270`). The main checkout may still be on an unrelated branch. Do not commit this work there.

If a worktree for the spec already exists:

```bash
cd /home/mbeisser/repo/message-vault
git fetch
git worktree list
```

Create the implementation branch from the spec branch:

```bash
git worktree add -b fix/import-session-source \
  .worktrees/fix/import-session-source \
  docs/import-session-source
cd /home/mbeisser/repo/message-vault/.worktrees/fix/import-session-source
git branch --show-current
```

If `fix/import-session-source` already exists and is based on `docs/import-session-source`, use that worktree instead of creating another.

Expected: `git branch --show-current` prints `fix/import-session-source`.

- [ ] **Step 2: Commit this plan** (skip if `git status` already shows it committed)

Copy the plan into the worktree if it is only in the main checkout, then:

```bash
git add docs/superpowers/plans/2026-08-27-import-session-source.md
git commit -m "$(cat <<'EOF'
docs: add import session source plan

The spec locks mapping method ids to the JSONL chat-kind slug
only when creating the vault import session.

Related to #203
EOF
)"
```

Do not stage `docs/package.json` or `docs/package-lock.json`.

---

### Task 1: `vaultSourceForMethod` helper

**Files:**
- Create: `web/src/lib/vaultSource.ts`
- Test: `web/src/lib/vaultSource.test.ts`

**Interfaces:**
- Consumes:
  - `isImessageMethod` and `IMESSAGE_SOURCE_ID` (`"imessage"`) from `web/src/lib/imessageImport.ts`
  - `isWhatsappMethod` and `WHATSAPP_SOURCE_ID` (`"whatsapp"`) from `web/src/lib/whatsappImport.ts`
  - `IMESSAGE_METHODS` and `WHATSAPP_METHODS` in tests, so every current method id is covered if a new id is added
- Produces:

```ts
export function vaultSourceForMethod(source: string): string
```

If `isImessageMethod(source)` is true, return `IMESSAGE_SOURCE_ID`. If `isWhatsappMethod(source)` is true, return `WHATSAPP_SOURCE_ID`. Otherwise return `source` unchanged. Do not throw. Do not trim or lowercase.

- [ ] **Step 1: Write the failing tests**

Create `web/src/lib/vaultSource.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { IMESSAGE_METHODS, IMESSAGE_SOURCE_ID } from "./imessageImport";
import { vaultSourceForMethod } from "./vaultSource";
import { WHATSAPP_METHODS, WHATSAPP_SOURCE_ID } from "./whatsappImport";

describe("vaultSourceForMethod", () => {
  it("maps each iMessage method id to imessage", () => {
    expect(IMESSAGE_METHODS.map((m) => m.id)).toEqual([
      "imessage-macos",
      "imessage-ios",
      "imessage-jailbreak",
    ]);
    for (const method of IMESSAGE_METHODS) {
      expect(vaultSourceForMethod(method.id)).toBe(IMESSAGE_SOURCE_ID);
    }
  });

  it("maps each WhatsApp method id to whatsapp", () => {
    expect(WHATSAPP_METHODS.map((m) => m.id)).toEqual([
      "whatsapp-android",
      "whatsapp-ios",
    ]);
    for (const method of WHATSAPP_METHODS) {
      expect(vaultSourceForMethod(method.id)).toBe(WHATSAPP_SOURCE_ID);
    }
  });

  it("leaves sms-backup-restore unchanged", () => {
    expect(vaultSourceForMethod("sms-backup-restore")).toBe("sms-backup-restore");
  });

  it("returns an unknown string unchanged", () => {
    expect(vaultSourceForMethod("not-a-real-source")).toBe("not-a-real-source");
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

```bash
cd /home/mbeisser/repo/message-vault/.worktrees/fix/import-session-source/web
npm test -- src/lib/vaultSource.test.ts
```

If `web/node_modules` is missing in the worktree, run `npm ci` in that `web/` directory first.

Expected: FAIL because `./vaultSource` cannot be resolved (or `vaultSourceForMethod` is not exported).

- [ ] **Step 3: Write the minimal helper**

Create `web/src/lib/vaultSource.ts`:

```ts
import { IMESSAGE_SOURCE_ID, isImessageMethod } from "./imessageImport";
import { WHATSAPP_SOURCE_ID, isWhatsappMethod } from "./whatsappImport";

/** Vault session / messages.source slug for a desktop Import method id. */
export function vaultSourceForMethod(source: string): string {
  if (isImessageMethod(source)) {
    return IMESSAGE_SOURCE_ID;
  }
  if (isWhatsappMethod(source)) {
    return WHATSAPP_SOURCE_ID;
  }
  return source;
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

```bash
cd /home/mbeisser/repo/message-vault/.worktrees/fix/import-session-source/web
npm test -- src/lib/vaultSource.test.ts
```

Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/vaultSource.ts web/src/lib/vaultSource.test.ts
git commit -m "$(cat <<'EOF'
feat(web): map import method ids to vault source

Desktop Import method ids name a backup layout. The vault session
needs the JSONL chat-kind slug so Mac and iPhone stay one bucket.

Related to #203
EOF
)"
```

---

### Task 2: Open the session with the mapped source

**Files:**
- Modify: `web/src/lib/vaultSource.ts` (add `importSessionCreateBody`)
- Modify: `web/src/lib/vaultSource.test.ts` (session-create assertions)
- Modify: `web/src/screens/import/useImportJob.ts` (session create only)
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `vaultSourceForMethod(source: string): string` from Task 1
- Produces:

```ts
export function importSessionCreateBody(formSource: string): {
  source: string;
  tool: "message-vault-io";
  mode: "append";
}
```

`useImportJob` posts that object to `/v1/imports`. `invokeExtract`, `resolveImportStagingDir`, and `saveImportSavedGroup` still receive `form.source`.

- [ ] **Step 1: Write the failing session-create tests**

Append to `web/src/lib/vaultSource.test.ts` (keep the existing `vaultSourceForMethod` describe block):

```ts
describe("importSessionCreateBody", () => {
  it("sends imessage when the form method is imessage-ios", () => {
    expect(importSessionCreateBody("imessage-ios")).toEqual({
      source: "imessage",
      tool: "message-vault-io",
      mode: "append",
    });
  });

  it("sends whatsapp when the form method is whatsapp-android", () => {
    expect(importSessionCreateBody("whatsapp-android")).toEqual({
      source: "whatsapp",
      tool: "message-vault-io",
      mode: "append",
    });
  });

  it("sends sms-backup-restore unchanged", () => {
    expect(importSessionCreateBody("sms-backup-restore").source).toBe(
      "sms-backup-restore",
    );
  });
});
```

Add `importSessionCreateBody` to the import from `./vaultSource`.

There is no existing import-job test that posts `/v1/imports`. These tests are the session-create coverage. Do not mount `useImportJob`.

- [ ] **Step 2: Run the tests and confirm they fail**

```bash
cd /home/mbeisser/repo/message-vault/.worktrees/fix/import-session-source/web
npm test -- src/lib/vaultSource.test.ts
```

Expected: FAIL because `importSessionCreateBody` is not exported.

- [ ] **Step 3: Add the session body helper and wire the job hook**

Append to `web/src/lib/vaultSource.ts`:

```ts
/** Body for POST /v1/imports. Maps method ids; leaves other sources as-is. */
export function importSessionCreateBody(formSource: string): {
  source: string;
  tool: "message-vault-io";
  mode: "append";
} {
  return {
    source: vaultSourceForMethod(formSource),
    tool: "message-vault-io",
    mode: "append",
  };
}
```

In `web/src/screens/import/useImportJob.ts`, add this import next to the other `../../lib/` imports:

```ts
import { importSessionCreateBody } from "../../lib/vaultSource";
```

Replace only the session-create POST. Today it is:

```ts
      const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
        source: form.source,
        tool: "message-vault-io",
        mode: "append",
      });
```

Change it to:

```ts
      const importSession = await apiClient.post<{ id: number }>(
        "/v1/imports",
        importSessionCreateBody(form.source),
      );
```

Do not change these call sites. They must still use `form.source`:

- `resolveImportStagingDir(form.backupPath, form.source)`
- `invokeExtract({ source: form.source, ... })`
- `saveImportSavedGroup({ source: form.source, ... })`

Add this bullet under `## [Unreleased]` → `### Fixed` in `CHANGELOG.md`:

```md
- 2026-08-27: Desktop Import opens the vault session as `imessage` or `whatsapp` instead of the Platform method id (`imessage-ios`, `whatsapp-android`, …), so conversation uploads no longer fail with a source mismatch.
```

- [ ] **Step 4: Run the related tests and confirm they pass**

```bash
cd /home/mbeisser/repo/message-vault/.worktrees/fix/import-session-source/web
npm test -- src/lib/vaultSource.test.ts src/lib/imessageImport.test.ts src/lib/whatsappImport.test.ts src/lib/savedGroups.test.ts src/screens/import/ImportFormFields.test.tsx
```

Expected: PASS. Saved-group tests still use method ids such as `imessage-ios`. Form tests still treat Platform as the method id.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/vaultSource.ts web/src/lib/vaultSource.test.ts \
  web/src/screens/import/useImportJob.ts CHANGELOG.md
git commit -m "$(cat <<'EOF'
fix(import): open session with JSONL source

The desktop job posted the Platform method id on /v1/imports.
vault-push sends export.source from the conversation header, and
the vault requires those strings to match.

Related to #203
EOF
)"
```

---

## Spec coverage

| Spec requirement | Task |
|---|---|
| Map iMessage method ids to `imessage` | Task 1 |
| Map WhatsApp method ids to `whatsapp` | Task 1 |
| Leave `sms-backup-restore` and unknown strings unchanged | Task 1 |
| Use the helper only on `POST /v1/imports` | Task 2 |
| Extract, staging, remembered paths, saved groups keep method id | Task 2 (do not touch those call sites) |
| Session-create test: `imessage-ios` → body `source: imessage` | Task 2 |
| Do not change exporters, `vault-push`, or the vault match check | File map out of scope |
| Vitest only, no Playwright | Global constraints |
| No new user-facing error copy | No UI copy task |
