# Contact Threads service + identity search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Row Threads opens conversation search for that platform service + identity; Summary Threads opens all of the contact’s identities; right-align count columns; rename footer label to Summary.

**Architecture:** Add optional `service:` to conversation list parsing/SQL (applied only with `handle:`). Pass platform service from the contact drawer browse path; Summary uses `contact:<id>` with no handle preference. UI: Summary label + right-aligned count columns.

**Tech Stack:** Rust (`conversations_api`), React/TypeScript (`web/src/components`)

## Global Constraints

- Platform service values: `phone` | `whatsapp` (case-insensitive).
- Lone `service:` without `handle:` is ignored.
- Direct / Group message counts stay non-links.
- Footer label: **Summary** (not Total).

## File map

| File | Role |
|------|------|
| `crates/vault/server/src/conversations_api.rs` | Parse `service:`, filter SQL, tests |
| `web/src/components/AppLayout.tsx` | Build `handle:… service:…` / `contact:…` browse queries |
| `web/src/components/ContactDrawer.tsx` | Pass `service` through browse callback |
| `web/src/components/contactDrawer/ContactDrawerHandles.tsx` | Row/Summary Threads links, label, right-align |
| `web/src/components/contactDrawer/contactDrawerTypes.ts` | BrowseFn type if needed |

---

### Task 1: Conversation list `service:` + handle filter

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs`

**Interfaces:**
- Produces: `ConversationListQuery.service: Option<String>`; `list_conversations` applies service only when `handle` is set

- [ ] **Step 1: Write failing test** `list_conversations_filters_by_handle_and_service`

Create two conversations for the same raw `+15555550200` — one `phone`, one `whatsapp` — each with a message. Assert:
- `handle:+15555550200` → total 2
- `handle:+15555550200 service:phone` → 1 (phone only)
- `handle:+15555550200 service:whatsapp` → 1 (whatsapp only)
- `service:whatsapp` alone → same as empty (ignore lone service; at least not empty incorrectly — assert equals unfiltered count for that account’s setup, or total unchanged vs `""` in a dedicated fixture)

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test -p message-vault-server list_conversations_filters_by_handle_and_service -- --nocapture
```

- [ ] **Step 3: Implement parse + SQL**

Add `service: Option<String>` to `ConversationListQuery`. Parse `service:` like `handle:` (accept `phone`/`whatsapp`). When both handle and service set:

```sql
(hc.raw = ? AND lower(hc.service) = lower(?) OR EXISTS (
  SELECT 1 FROM participants p
  JOIN handles ph ON ph.id = p.handle_id
  WHERE p.conversation_id = c.id AND ph.raw = ? AND lower(ph.service) = lower(?)
))
```

When only handle: keep existing SQL.

- [ ] **Step 4: Run test — expect PASS**

- [ ] **Step 5: Commit** `feat(vault): filter conversation list by handle service`

---

### Task 2: Browse query builders + drawer wiring

**Files:**
- Modify: `web/src/components/AppLayout.tsx`
- Modify: `web/src/components/ContactDrawer.tsx`
- Modify: `web/src/components/contactDrawer/ContactDrawerHandles.tsx`
- Modify: `web/src/components/contactDrawer/contactDrawerTypes.ts` (BrowseFn)

**Interfaces:**
- Consumes: Task 1 `service:` token
- Produces: browse args `{ kind, handle?, service? }`; Summary → `contact:<id>` for `q` and `f`

- [ ] **Step 1: Update `contactBrowseQuery` / `visibleBrowseQuery`**

When `handle` + `service`: append ` service:<platform>` (use inferred platform id).  
When no handle: always `contact:<id>` (do **not** prefer first handle for visible query).

- [ ] **Step 2: Wire drawer**

Row Threads: `onBrowse({ kind: "all", handle: h.handle, service: inferService(...) })`  
Summary Threads: `CountCell` with `onBrowse({ kind: "all" })` when count > 0  
Label: **Summary**  
Right-align Threads / Direct / Group headers and cells (`text-right`)

- [ ] **Step 3: Manual sanity** (dev server) — Albert Jones style dual platform

- [ ] **Step 4: Commit** `feat(contacts): scope Threads browse by service + identity`

---

## Spec coverage

| Spec item | Task |
|-----------|------|
| Row `handle:` + `service:` | 1 + 2 |
| Summary `contact:` | 2 |
| Ignore lone `service:` | 1 |
| Summary label | 2 |
| Right-align counts | 2 |
| Direct/Group non-links | 2 (unchanged) |
