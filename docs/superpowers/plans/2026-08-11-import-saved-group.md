# Auto-save Import as Saved Group Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a GUI Import that wrote messages, auto-create a Saved Group whose query lists every conversation that received messages from that import session.

**Architecture:** Add conversation-list token `import:<id>` (SQL `EXISTS` on `messages.import_id`). On Import finish, `addGroup` with name `Import {source} {YYYY-MM-DD}` (suffix ` 2`, ` 3`, … on collision) and query `import:{sessionId}`. Notify LeftPanel via a custom event so the new row appears without reload. Stay on the Import done screen.

**Tech Stack:** Rust (`message-vault-server` / `conversations_api`), React/TypeScript (`web/src`), `localStorage` Saved Groups

**Spec:** `docs/superpowers/specs/2026-08-11-import-saved-group-design.md`

## Global Constraints

- Filter: conversations with ≥1 message where `messages.import_id` equals the session id.
- Query shape: `import:<importSessionId>` (positive integer).
- Group name: `Import {source} {YYYY-MM-DD}`; collisions append ` 2`, ` 3`, …
- Create only when `importSessionId` is set and `messages_inserted > 0` (missing counts as 0).
- Stay on Import; do not auto-run search.
- No schema migration; Saved Groups stay in `localStorage`.
- CLI `vault-push` alone does not create Saved Groups.

## File map

| File | Role |
|------|------|
| `crates/vault/server/src/conversations_api.rs` | Parse `import:`, SQL filter, unit tests |
| `crates/vault/server/src/db/vault_imports.rs` | `start_import` used by tests only |
| `web/src/lib/savedGroups.ts` | Naming helper, change event, `addGroup`/`removeGroup` notify |
| `web/src/lib/savedGroups.test.ts` | Unit tests for naming + save gate |
| `web/src/components/LeftPanel.tsx` | Re-list groups on change event |
| `web/src/screens/ImportScreen.tsx` | Call save helper after import finish |
| `web/src/screens/ConversationList.tsx` | Treat `import:` as structured filter (no debounce) |

---

### Task 1: Conversation list `import:` filter

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs`

**Interfaces:**
- Produces: `ConversationListQuery.import_id: Option<i64>`; `list_conversations` applies EXISTS filter when set

- [ ] **Step 1: Write failing test** `list_conversations_filters_by_import_id`

In `conversations_api.rs` tests module, add a test that:

1. Uses `schema::ensure_vault_schema` + account like `setup()`.
2. Creates two import sessions via `crate::db::vault_imports::start_import(&conn, &account, "imessage-ios", "append", Some("test"))`.
3. Creates two conversations (ids `1` and `2`) with distinct handles/participants (mirror existing multi-conversation tests).
4. Inserts a message on conv `1` with `import_id = import_a`, and a message on conv `2` with `import_id = import_b` (include `import_id` in the INSERT column list).
5. Asserts:
   - `import:{import_a}` → total 1, conversation id `"1"`
   - `import:{import_b}` → total 1, conversation id `"2"`
   - `import:999999` → total 0
   - `import:not-a-number` → same total as `""` for that fixture (invalid token ignored; if the fixture has 2 conversations with messages, total 2)

```rust
#[test]
fn list_conversations_filters_by_import_id() {
    // ... fixture as above ...
    let a = list_conversations(
        &conn,
        &account,
        &format!("import:{import_a}"),
        DEFAULT_LIST_LIMIT,
        0,
    )
    .unwrap();
    assert_eq!(a.total, 1);
    assert_eq!(a.conversations[0].id, "1");

    let b = list_conversations(
        &conn,
        &account,
        &format!("import:{import_b}"),
        DEFAULT_LIST_LIMIT,
        0,
    )
    .unwrap();
    assert_eq!(b.total, 1);
    assert_eq!(b.conversations[0].id, "2");

    let missing = list_conversations(&conn, &account, "import:999999", DEFAULT_LIST_LIMIT, 0)
        .unwrap();
    assert_eq!(missing.total, 0);

    let junk = list_conversations(&conn, &account, "import:not-a-number", DEFAULT_LIST_LIMIT, 0)
        .unwrap();
    let all = list_conversations(&conn, &account, "", DEFAULT_LIST_LIMIT, 0).unwrap();
    assert_eq!(junk.total, all.total);
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test -p message-vault-server list_conversations_filters_by_import_id -- --nocapture
```

Expected: FAIL (no `import:` support / wrong totals).

- [ ] **Step 3: Implement parse + SQL**

1. Add to `ConversationListQuery`:

```rust
import_id: Option<i64>,
```

2. In `parse_conversation_list_query`, before the generic `contact:` / text branch, handle `import:`:

```rust
} else if let Some(rest) = lower.strip_prefix("import:") {
    if let Ok(id) = rest.trim().parse::<i64>() {
        if id > 0 {
            out.import_id = Some(id);
        }
    }
    // invalid / non-positive: ignore token (do not push to text_parts)
} else if let Some((_, id_part)) = token.split_once(':') {
    // existing contact: branch ...
```

Update the doc comment on `parse_conversation_list_query` / `list_conversations` to mention `import:<id>`.

3. After building other `where_parts`, when `parsed.import_id` is `Some(id)`:

```rust
if let Some(import_id) = parsed.import_id {
    where_parts.push(
        "EXISTS (
           SELECT 1 FROM messages m
           WHERE m.conversation_id = c.id
             AND m.account_id = c.account_id
             AND m.import_id = ?N
         )"
        .into(),
    );
    params.push(import_id.into());
}
```

Use the same `?N` indexing pattern already used for other dynamic params in this function (do not invent a second param style).

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test -p message-vault-server list_conversations_filters_by_import_id -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/conversations_api.rs
git commit -m "$(cat <<'EOF'
feat(vault): filter conversation list by import session

Add import:<id> so Saved Groups can list threads touched by a GUI import run.
EOF
)"
```

---

### Task 2: Saved Groups helpers + LeftPanel refresh

**Files:**
- Modify: `web/src/lib/savedGroups.ts`
- Create: `web/src/lib/savedGroups.test.ts`
- Modify: `web/src/components/LeftPanel.tsx`

**Interfaces:**
- Produces:
  - `SAVED_GROUPS_CHANGED_EVENT = "mv-saved-groups-changed"`
  - `uniqueImportGroupName(source: string, dateYmd: string, existingNames: string[]): string`
  - `shouldSaveImportGroup(importSessionId: number | null | undefined, messagesInserted: number | null | undefined): boolean`
  - `saveImportSavedGroup(args: { importSessionId: number; source: string; messagesInserted: number | null | undefined; now?: Date }): SavedGroup | null`
  - `addGroup` / `removeGroup` dispatch the change event after writing storage

- [ ] **Step 1: Write failing tests**

Create `web/src/lib/savedGroups.test.ts` using the same `localStorage` mock pattern as `web/src/lib/contactRecentSearches.test.ts`:

```ts
import assert from "node:assert/strict";
import { describe, it, beforeEach } from "node:test";
import {
  addGroup,
  listGroups,
  uniqueImportGroupName,
  shouldSaveImportGroup,
  saveImportSavedGroup,
  SAVED_GROUPS_CHANGED_EVENT,
} from "./savedGroups.ts";

// mock localStorage in beforeEach (copy pattern from contactRecentSearches.test.ts)

describe("uniqueImportGroupName", () => {
  it("uses base name when free", () => {
    assert.equal(
      uniqueImportGroupName("imessage-ios", "2026-08-11", []),
      "Import imessage-ios 2026-08-11",
    );
  });

  it("appends 2, 3 for collisions", () => {
    const names = ["Import imessage-ios 2026-08-11"];
    assert.equal(
      uniqueImportGroupName("imessage-ios", "2026-08-11", names),
      "Import imessage-ios 2026-08-11 2",
    );
    names.push("Import imessage-ios 2026-08-11 2");
    assert.equal(
      uniqueImportGroupName("imessage-ios", "2026-08-11", names),
      "Import imessage-ios 2026-08-11 3",
    );
  });
});

describe("shouldSaveImportGroup", () => {
  it("requires session id and messages_inserted > 0", () => {
    assert.equal(shouldSaveImportGroup(42, 1), true);
    assert.equal(shouldSaveImportGroup(42, 0), false);
    assert.equal(shouldSaveImportGroup(42, undefined), false);
    assert.equal(shouldSaveImportGroup(null, 5), false);
  });
});

describe("saveImportSavedGroup", () => {
  it("writes group with import: query and notifies", () => {
    let notified = 0;
    const onChange = () => {
      notified += 1;
    };
    window.addEventListener(SAVED_GROUPS_CHANGED_EVENT, onChange);
    const g = saveImportSavedGroup({
      importSessionId: 7,
      source: "imessage-ios",
      messagesInserted: 3,
      now: new Date("2026-08-11T15:00:00"),
    });
    window.removeEventListener(SAVED_GROUPS_CHANGED_EVENT, onChange);
    assert.ok(g);
    assert.equal(g!.name, "Import imessage-ios 2026-08-11");
    assert.equal(g!.query, "import:7");
    assert.equal(listGroups().length, 1);
    assert.equal(notified, 1);
  });

  it("skips when no messages inserted", () => {
    assert.equal(
      saveImportSavedGroup({
        importSessionId: 7,
        source: "imessage-ios",
        messagesInserted: 0,
      }),
      null,
    );
    assert.equal(listGroups().length, 0);
  });
});
```

Note: under Node, use `globalThis` for `addEventListener` if `window` is missing — either polyfill `globalThis.window = globalThis as unknown as Window` in `beforeEach`, or dispatch via a small internal `notifySavedGroupsChanged()` that uses `globalThis.dispatchEvent` when available. Prefer exporting the event name and calling `globalThis.dispatchEvent?.(new Event(...))` from `addGroup` so tests can listen on `globalThis`.

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd web && npx --yes tsx --test src/lib/savedGroups.test.ts
```

Expected: FAIL (exports missing).

- [ ] **Step 3: Implement helpers in `savedGroups.ts`**

```ts
export const SAVED_GROUPS_CHANGED_EVENT = "mv-saved-groups-changed";

function notifySavedGroupsChanged(): void {
  try {
    globalThis.dispatchEvent?.(new Event(SAVED_GROUPS_CHANGED_EVENT));
  } catch {
    // ignore (non-DOM / restricted)
  }
}

export function uniqueImportGroupName(
  source: string,
  dateYmd: string,
  existingNames: string[],
): string {
  const base = `Import ${source} ${dateYmd}`;
  const taken = new Set(existingNames);
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base} ${n}`)) n += 1;
  return `${base} ${n}`;
}

export function shouldSaveImportGroup(
  importSessionId: number | null | undefined,
  messagesInserted: number | null | undefined,
): boolean {
  return (
    importSessionId != null &&
    importSessionId > 0 &&
    (messagesInserted ?? 0) > 0
  );
}

function localYmd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function saveImportSavedGroup(args: {
  importSessionId: number;
  source: string;
  messagesInserted: number | null | undefined;
  now?: Date;
}): SavedGroup | null {
  if (!shouldSaveImportGroup(args.importSessionId, args.messagesInserted)) {
    return null;
  }
  const dateYmd = localYmd(args.now ?? new Date());
  const name = uniqueImportGroupName(
    args.source,
    dateYmd,
    listGroups().map((g) => g.name),
  );
  return addGroup(name, `import:${args.importSessionId}`);
}
```

Update `addGroup` / `removeGroup` to call `notifySavedGroupsChanged()` after a successful `localStorage.setItem`.

Wrap `localStorage` writes in the existing try/catch style if not already — failures must not throw out of Import.

- [ ] **Step 4: Wire LeftPanel**

In `LeftPanel.tsx`:

```ts
import { useEffect, useState } from "react";
import {
  listGroups,
  addGroup,
  removeGroup,
  SAVED_GROUPS_CHANGED_EVENT,
} from "../lib/savedGroups";

// inside component:
useEffect(() => {
  const refresh = () => setGroups(listGroups());
  globalThis.addEventListener(SAVED_GROUPS_CHANGED_EVENT, refresh);
  return () => globalThis.removeEventListener(SAVED_GROUPS_CHANGED_EVENT, refresh);
}, []);
```

Keep the existing local `setGroups(listGroups())` after manual add/remove.

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd web && npx --yes tsx --test src/lib/savedGroups.test.ts
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/savedGroups.ts web/src/lib/savedGroups.test.ts web/src/components/LeftPanel.tsx
git commit -m "$(cat <<'EOF'
feat(web): import saved-group helpers and sidebar refresh

Add unique Import naming, change events, and LeftPanel re-list on updates.
EOF
)"
```

---

### Task 3: ImportScreen save + ConversationList debounce

**Files:**
- Modify: `web/src/screens/ImportScreen.tsx`
- Modify: `web/src/screens/ConversationList.tsx`

**Interfaces:**
- Consumes: `saveImportSavedGroup` from Task 2
- Produces: Saved Group created at end of import `finally` when gate passes; `import:` queries apply without debounce

- [ ] **Step 1: Call save after import finish**

In `ImportScreen.tsx` `finally` block, after session complete / before or after `setSummaryView`, add:

```ts
import { saveImportSavedGroup } from "../lib/savedGroups";

// inside finally, after pushReport / importSessionId are known:
if (importSessionId != null) {
  saveImportSavedGroup({
    importSessionId,
    source,
    messagesInserted: pushReport?.messages_inserted,
  });
}
```

Do **not** navigate or call `onSearch`. Stay on done phase as today.

- [ ] **Step 2: Structured-filter debounce for `import:`**

In `ConversationList.tsx`, extend the immediate-apply regex:

```ts
if (/\b(contact:|handle:|import:|is:direct|is:group|is:trash|participants:)\b/i.test(query)) {
```

- [ ] **Step 3: Typecheck**

```bash
cd web && npx tsc --noEmit
```

Expected: exit 0

- [ ] **Step 4: Manual check**

1. Run Import that inserts ≥1 message → Saved Groups shows `Import {source} {today}` without reload.
2. Click it → conversation list shows only that run’s threads; URL/search shows `import:{id}`.
3. Run a second same-source import same day → name ends with ` 2`.
4. Import with 0 inserts → no new group.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/ImportScreen.tsx web/src/screens/ConversationList.tsx
git commit -m "$(cat <<'EOF'
feat(import): auto-save Finished Import as Saved Group

Create import:<id> groups after non-empty GUI imports and skip debounce for that filter.
EOF
)"
```

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| `import:<id>` conversation filter | Task 1 |
| EXISTS on `messages.import_id` | Task 1 |
| Auto `addGroup` when messages written | Tasks 2–3 |
| Name `Import {source} {date}` + ` 2`/` 3` | Task 2 |
| Stay on Import | Task 3 |
| LeftPanel refresh without reload | Task 2 |
| Zero inserts / missing session → no group | Task 2–3 |
| Server + frontend unit tests | Tasks 1–2 |
| No schema migration / CLI out of scope | honored |

## Self-review notes

- No placeholders left.
- `saveImportSavedGroup` is the single ImportScreen entry point so naming + gate stay unit-tested.
- ConversationList `import:` debounce is required so the list does not flash unfiltered results while typing/applying the saved query.
