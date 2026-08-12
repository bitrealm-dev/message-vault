# Auto-save Import as Saved Group

**Date:** 2026-08-11  
**Status:** Approved for planning  
**Scope:** GUI Import finish + Saved Groups sidebar + conversation list query (`web/`, `crates/vault/server`)

## Problem

After a GUI Import finishes, there is no one-click way to browse only the conversations that received messages from that run. Users must remember which threads were touched or dig through Import History. Saved Groups already store a name plus a search query in the left panel, but nothing creates a group for an import session automatically, and the conversation list has no filter for `messages.import_id`.

## Goals

- When a GUI Import ends with at least one message written, create a Saved Group under **Saved Groups**.
- Clicking that group lists every non-trashed conversation that has at least one message stamped with that import session id.
- Stay on the Import done screen; do not auto-run the search.

## Non-goals

- Creating saved groups from CLI `vault-push` alone.
- An Import History “save group” button (possible follow-up).
- Server-side persistence of Saved Groups (they remain `localStorage`).
- Schema migrations (`messages.import_id` already exists).
- Changing message-body FTS search operators (this is conversation-list `q` only).

## Decisions

| Topic | Choice |
|--------|--------|
| Filter meaning | Conversations with ≥1 message where `messages.import_id` equals the session id |
| Query shape | `import:<importSessionId>` |
| Group name | `Import {source} {YYYY-MM-DD}` (form source + local calendar date at create time) |
| Name collisions | First keeps the base name; later same-day same-source groups append ` 2`, ` 3`, … |
| When to create | Any end state with `messages_inserted > 0` (including partial/failed runs that still wrote messages) |
| Zero inserts | Do not create a group |
| Post-create UX | Add to Saved Groups only; stay on Import |
| Approach | First-class conversation-list token + existing `addGroup(name, query)` |

## Approach

Add an `import:<id>` token to the conversation list query parser (same family as `contact:` / `handle:`). Filter with an `EXISTS` on `messages` for that `import_id`. On Import finish, if the run has a session id and wrote messages, call `addGroup` with the name rules above and query `import:{id}`, then notify the left panel to re-read groups from storage.

## Behavior

| Event | Condition | Result |
|--------|-----------|--------|
| Import finishes | `importSessionId` set and `messages_inserted > 0` | Create Saved Group; stay on Import done |
| Import finishes | zero messages inserted, or no session id | No Saved Group |
| Click Saved Group | any | Set search `q` to the stored query and run conversation list (unchanged click path) |
| Click `import:` for unknown/deleted session | — | Empty conversation list |

### Name uniqueness

Base name: `Import {source} {YYYY-MM-DD}`.

Among existing Saved Group names:

- If base is free → use base.
- Else use `Import {source} {YYYY-MM-DD} {n}` for the smallest integer `n ≥ 2` not already taken.

Examples for source `imessage-ios` on 2026-08-11:

1. `Import imessage-ios 2026-08-11`
2. `Import imessage-ios 2026-08-11 2`
3. `Import imessage-ios 2026-08-11 3`

## Server

In `conversations_api` conversation list parsing:

- Accept `import:<id>` where `id` is a positive integer.
- Non-numeric / invalid tokens are ignored (same style as bad `contact:`).
- When present, require:

  ```sql
  EXISTS (
    SELECT 1 FROM messages m
    WHERE m.conversation_id = c.id
      AND m.account_id = c.account_id
      AND m.import_id = ?
  )
  ```

- Keep existing trash exclusion and other structured tokens combinable as today.

Add unit tests: conversations with messages from import A appear for `import:A`; conversations only touched by import B do not; empty/unknown id yields empty page.

## UI

- **`ImportScreen`:** After push and best-effort session complete, if `importSessionId` is set and `messages_inserted > 0` (missing counts as 0), compute unique name, `addGroup(name, \`import:${id}\`)`, emit groups-changed notification.
- **`savedGroups.ts`:** Keep `{ id, name, query }` shape. Add a small “groups changed” notifier (custom event or equivalent) that `addGroup` / `removeGroup` fire so callers outside LeftPanel can refresh the list. Export a helper for unique import group naming if that keeps ImportScreen thin.
- **`LeftPanel`:** Subscribe to the notifier and re-`listGroups()`. Click behavior unchanged.

## Out of scope / follow-ups

- Button on Import History to (re)create a Saved Group for an old session.
- Prompting for a custom name at import end.
- Auto-navigating to the conversation list after import.

## Testing

- Server: conversation list filter tests for `import:<id>` as above.
- Frontend: unit coverage for unique name selection and “create only when `messages_inserted > 0`”.
- Manual: run an import that inserts messages → Saved Groups shows the new entry without reload → click it → only that run’s threads appear; run a second same-source same-day import → name ends with ` 2`.
