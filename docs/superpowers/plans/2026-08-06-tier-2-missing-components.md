# Tier 2 Plan — Missing Planned Components

**Date:** 2026-08-06
**Spec:** `docs/superpowers/specs/2026-08-06-unified-gui-design.md`
**Gap analysis:** `docs/superpowers/specs/2026-08-06-unified-gui-gap-analysis.md`
**Prerequisite:** Tier 1 plan (`docs/superpowers/plans/2026-08-06-tier-1-finish-plans.md`) merged — this plan assumes saved groups, offline tools, real import/export, and Docker integration are in place.

## Goal

Implement the 9 "missing planned components" from the gap analysis Tier 2 list, plus the backend endpoints they depend on: search with operator autocomplete, advanced search form, import history screen, find bar with match highlighting and next/prev arrows, participant chips in the message header, export popover with three scope options, conversation checkboxes for ad-hoc export selection, import contacts conflict review UI, and the Settings storage + account sections (change password, delete account).

## Architecture

Three backend prerequisites discovered while writing this plan were missing entirely and are included as Tasks 1–4:

1. **Conversation scoping is broken** — `MessageView` sends `conversation:<id>` but the parser has no such operator (the token falls through to FTS text matching, so the message view matches nothing), and even the existing `in:<id>` operator is parsed but never applied to the export SQL. Every message-view feature (find bar, pagination totals, chips) depends on this.
2. **No contacts API** — `ContactList` and `ContactDrawer` call `GET /v1/export/contacts` and `GET /v1/export/contacts/{id}` which 404. The participant-chip and conflict-review features need them too.
3. **No change-password / delete-account endpoints** — the Settings Account section needs them.
4. **No conversation count** — Settings storage stats need `conversations` on the existing count endpoint.

The plan alternates backend tasks (message-vault-rs) and frontend tasks (message-vault-io/web). Frontend features consume the exact response shapes the backend tasks define — do the backend tasks first, in order.

**Tech Stack:** Rust (axum, rusqlite) · React 19 + TypeScript (Vite) · Tauri v2 (one command extension for conflict review)

## Global Constraints

- Frontend changes go in `message-vault-io/web/src/`; backend changes go in `message-vault-rs/src/`; the one Tauri command change goes in `message-vault-io/src-tauri/src/commands/contacts.rs`
- Saved groups stay localStorage-backed (server persistence is follow-up work)
- Existing public contracts must not break: `LeftPanel` props, `apiClient` (get/post), `useAuth().logout`, the `Conversation`/`Participant` types, the Tauri `contacts_info` command name
- New API responses must satisfy the shapes `ContactList` and `ContactDrawer` already expect (they were written against the intended API)
- Commit after every step that compiles; one commit per logical change (see per-task commits)

## File Structure

### message-vault-rs

| File | Action |
|------|--------|
| `src/search_query.rs` | Modify — add `conversation` operator (Task 1) |
| `src/export_api.rs` | Modify — apply `in_conversation` filter (Task 1); `contact_id` on participants (Task 2); `conversations` count (Task 4) |
| `src/contacts_api.rs` | **Create** — contact list/detail queries (Task 2) |
| `src/server.rs` | Modify — register contacts + auth routes, add handlers (Tasks 2–3) |
| `src/main.rs` | Modify — `mod contacts_api;` (Task 2) |
| `src/db/account_profile.rs` | Modify — `update_password_hash`, `delete_account` (Task 3) |
| `src/auth.rs` | Modify — change-password + delete-account handlers (Task 3) |

### message-vault-io

| File | Action |
|------|--------|
| `web/src/lib/types.ts` | Modify — realign `Message` to the API shape, add `contact_id` to `Participant` (Task 5) |
| `web/src/components/MessageBubble.tsx` | Modify — real API fields + highlight prop (Task 5) |
| `web/src/screens/MessageView.tsx` | Modify — real count endpoint, find bar highlighting + next/prev (Task 5); chips (Task 6) |
| `web/src/components/AppLayout.tsx` | Modify — checked ids, export scope setter, import-history view, chip callback (Tasks 6, 9, 10, 11) |
| `web/src/components/GlobalSearch.tsx` | **Create** — operator autocomplete (Task 7) |
| `web/src/components/LeftPanel.tsx` | Modify — GlobalSearch + advanced form + export popover (Tasks 7, 8, 10) |
| `web/src/components/AdvancedSearchForm.tsx` | **Create** — Messages/Contacts tabs (Task 8) |
| `web/src/components/ConversationRow.tsx` | Modify — checkbox (Task 9) |
| `web/src/screens/ConversationList.tsx` | Modify — checked ids pass-through (Task 9) |
| `web/src/screens/ImportHistoryScreen.tsx` | **Create** — chronological import list (Task 11) |
| `web/src/screens/SettingsScreen.tsx` | Modify — history link (Task 11); storage + account sections (Task 13) |
| `web/src/screens/ImportScreen.tsx` | Modify — history button (Task 11); conflict review (Task 12) |
| `web/src/components/ContactReviewTable.tsx` | **Create** — side-by-side conflict table (Task 12) |
| `web/src/lib/tauri.ts` | Modify — `cards` on `ContactsInfo` (Task 12) |
| `src-tauri/src/commands/contacts.rs` | Modify — return full card list (Task 12) |

## Known upstream gaps (flagged, not fixed here)

- `GET /v1/export/conversations` does not exist — `ConversationList` currently 404s. Tier 1 must add it. Until then the conversation list is empty; all Tier 2 message-view work consumes the messages API, which is unaffected. When Tier 1 builds the endpoint, `Conversation.id` must serialize as the numeric `conversations.id` (a string) so `in:<id>` scoping works.
- `is:trash` is a parser no-op and no trash endpoints exist — Tier 3 territory.
- `AppLayout` holds `exportScope` state without a setter and passes `selectedCount={0}` — Task 10 fixes.

---

# Task 1 — Backend: real conversation scoping on `/v1/export/messages`

**Goal:** Make `in:<id>` (and the new `conversation:<id>` alias) actually filter messages to one conversation. Without this, `MessageView` matches nothing (its `conversation:<id>` token falls through to FTS text matching) and no message-view feature works.

**Files:** `src/search_query.rs`, `src/export_api.rs`

### Step 1: Add the `conversation` operator to the parser

In `message-vault-rs/src/search_query.rs`, `parse_operator` (around line 652) — add `| "conversation"` to the operator match arm. Find this block:

```rust
        | "context" | "sort" | "last-contact" | "first-contact" | "first" | "last" | "phone" => {
            Some((op, value))
        }
```

Replace it with:

```rust
        | "context" | "sort" | "last-contact" | "first-contact" | "first" | "last" | "phone"
        | "conversation" => {
            Some((op, value))
        }
```

Then in the big `match op.as_str()` block, right after the `"in"` arm (which sets `out.in_conversation` unless the value is `trash`), add an alias arm:

```rust
                "in" => {
                    if !value.eq_ignore_ascii_case("trash") {
                        out.in_conversation = Some(value.to_string());
                    }
                }
                "conversation" => out.in_conversation = Some(value.to_string()),
```

### Step 2: Apply `in_conversation` in the export filters

In `message-vault-rs/src/export_api.rs`, in `build_message_filters` (around line 444), the `where_parts` start with `"c.account_id = ?"`. Right after that block, add:

```rust
    if let Some(conv) = &parsed.in_conversation {
        match conv.parse::<i64>() {
            Ok(id) => {
                where_parts.push("c.id = ?".into());
                params.push(id.into());
            }
            Err(_) => {
                where_parts.push("c.chat_identifier = ?".into());
                params.push(conv.clone().into());
            }
        }
    }
```

### Step 3: Add a regression test

Append to the bottom of `src/export_api.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::schema::ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES ('a1', 'alice', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (id, account_id, chat_identifier, service, conversation_type, source_file)
             VALUES (1, 'a1', '+1555', 'sms', 'individual', 'backup-a.jsonl'),
                    (2, 'a1', '+1666', 'sms', 'individual', 'backup-a.jsonl')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, account_id, source, timestamp, is_from_me, sort_order, body)
             VALUES (1, 1, 'a1', 'sms', '2020-01-01T00:00:00Z', 0, 0, 'hello one'),
                    (2, 2, 'a1', 'sms', '2020-01-02T00:00:00Z', 0, 0, 'hello two')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn conversation_filter_scopes_messages() {
        let conn = setup();
        let opts = |q: &str| ExportPageOpts {
            account_id: "a1",
            query: q,
            limit: 100,
            offset: None,
            cursor: None,
            source_override: None,
        };

        let res = export_messages(&conn, opts("in:1")).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 1);

        let res = export_messages(&conn, opts("conversation:2")).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, 2);

        // No criteria → everything (unchanged behavior).
        let res = export_messages(&conn, opts("")).unwrap();
        assert_eq!(res.messages.len(), 2);
    }
}
```

### Step 4: Build and test

```bash
cd /home/mbeisser/repo/message-vault-rs && cargo build && cargo test -p message-vault-rs export_api
```

Expected: builds cleanly, all three assertions pass.

### Step 5: Commit

```bash
cd /home/mbeisser/repo/message-vault-rs
git add src/search_query.rs src/export_api.rs
git commit -m "fix(api): scope /v1/export/messages to a single conversation

Add 'conversation' as an alias for the parsed-but-unapplied 'in'
operator, and apply in_conversation in build_message_filters.
MessageView previously sent conversation:<id>, which the parser
ignored (token fell through to FTS), so message views matched
nothing. Adds a regression test covering in:, conversation:, and
the unfiltered case.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 2 — Backend: contacts API + `contact_id` on participants

**Goal:** `GET /v1/export/contacts` and `GET /v1/export/contacts/{id}` (currently 404 — `ContactList`/`ContactDrawer` are broken), plus `contact_id` on every participant in the messages API so message headers can open the contact drawer.

**Files:** `src/contacts_api.rs` (create), `src/export_api.rs`, `src/server.rs`, `src/main.rs`

### Step 1: Create `src/contacts_api.rs`

Complete file:

```rust
//! Read-only contact query used by `GET /v1/export/contacts`
//! and `GET /v1/export/contacts/{id}`.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::export_api::ExportQueryError;

#[derive(Debug, Serialize)]
pub struct ContactSummary {
    pub id: i64,
    pub name: String,
    pub handle_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContactHandleInfo {
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    pub message_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ContactDetail {
    pub id: i64,
    pub name: String,
    pub handles: Vec<ContactHandleInfo>,
    pub direct_conversations: u64,
    pub group_conversations: u64,
    pub total_messages: u64,
}

/// A contact is linked to a conversation when one of its handles is either
/// the conversation's `chat_identifier` or a participant handle in it.
fn involves_contact_sql() -> &'static str {
    "EXISTS (
       SELECT 1 FROM contact_handles ch
       WHERE ch.account_id = c.account_id
         AND ch.contact_id = ?
         AND (
           ch.handle = c.chat_identifier
           OR EXISTS (
             SELECT 1 FROM participants p
             WHERE p.conversation_id = c.id AND p.handle = ch.handle
           )
         )
     )"
}

/// Flat list of contacts: id, display name, handle count, last message date.
pub fn list_contacts(
    conn: &Connection,
    account_id: &str,
) -> Result<Vec<ContactSummary>, ExportQueryError> {
    let mut stmt = conn
        .prepare(
            "SELECT ct.id,
                    COALESCE(NULLIF(trim(ct.preferred_name), ''), NULLIF(trim(ct.preferred_handle), ''), '(unknown)') AS name,
                    (SELECT COUNT(*) FROM contact_handles ch WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id) AS handle_count,
                    (SELECT MAX(m.timestamp)
                     FROM messages m
                     JOIN conversations c ON c.id = m.conversation_id
                     WHERE c.account_id = ct.account_id
                       AND m.duplicate_of IS NULL
                       AND EXISTS (
                         SELECT 1 FROM contact_handles ch2
                         WHERE ch2.account_id = c.account_id AND ch2.contact_id = ct.id
                           AND (
                             ch2.handle = c.chat_identifier
                             OR EXISTS (
                               SELECT 1 FROM participants p
                               WHERE p.conversation_id = c.id AND p.handle = ch2.handle
                             )
                           )
                       )) AS last_message_at
             FROM contacts ct
             WHERE ct.account_id = ?1
             ORDER BY name COLLATE NOCASE",
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map([account_id], |row| {
            Ok(ContactSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                handle_count: row.get::<_, i64>(2)?.max(0) as u64,
                last_message_at: row.get(3)?,
            })
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    Ok(rows)
}

/// Full contact view: per-handle service + date range + direct message count,
/// plus conversation and total-message stats across all the contact's handles.
pub fn get_contact_detail(
    conn: &Connection,
    account_id: &str,
    contact_id: i64,
) -> Result<Option<ContactDetail>, ExportQueryError> {
    let name: Option<String> = conn
        .query_row(
            "SELECT COALESCE(NULLIF(trim(preferred_name), ''), NULLIF(trim(preferred_handle), ''), '(unknown)')
             FROM contacts WHERE id = ?1 AND account_id = ?2",
            rusqlite::params![contact_id, account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let Some(name) = name else {
        return Ok(None);
    };

    // One row per handle. Date range covers direct + group conversations;
    // message count is direct-messages only (group stats are not attributed).
    // COUNT(DISTINCT ...) guards against a conversation matching two handles
    // of the same contact (chat_identifier + participant).
    let mut stmt = conn
        .prepare(
            "SELECT ch.handle,
                    (SELECT c2.service FROM conversations c2
                     WHERE c2.account_id = ch.account_id
                       AND (c2.chat_identifier = ch.handle
                            OR EXISTS (
                              SELECT 1 FROM participants p2
                              WHERE p2.conversation_id = c2.id AND p2.handle = ch.handle
                            ))
                     ORDER BY c2.id DESC LIMIT 1) AS service,
                    MIN(m.timestamp) AS first_ts,
                    MAX(m.timestamp) AS last_ts,
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN m.id END)
             FROM contact_handles ch
             LEFT JOIN conversations c ON c.account_id = ch.account_id
               AND (c.chat_identifier = ch.handle
                    OR EXISTS (
                      SELECT 1 FROM participants p
                      WHERE p.conversation_id = c.id AND p.handle = ch.handle
                    ))
             LEFT JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
             WHERE ch.account_id = ?1 AND ch.contact_id = ?2
             GROUP BY ch.handle
             ORDER BY ch.handle",
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let mut handles = Vec::new();
    let rows = stmt
        .query_map(rusqlite::params![account_id, contact_id], |row| {
            Ok(ContactHandleInfo {
                handle: row.get(0)?,
                service: row.get(1)?,
                start_date: row.get(2)?,
                end_date: row.get(3)?,
                message_count: row.get::<_, i64>(4)?.max(0) as u64,
            })
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    for row in rows {
        handles.push(row.map_err(|e| ExportQueryError::Internal(e.to_string()))?);
    }

    // Conversation + message stats across all handles of this contact.
    let mut stats_stmt = conn
        .prepare(&format!(
            "SELECT COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN c.id END),
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'group' THEN c.id END),
                    COALESCE(SUM(mc.m_count), 0)
             FROM conversations c
             LEFT JOIN (
               SELECT conversation_id, COUNT(*) AS m_count
               FROM messages
               WHERE account_id = ?1 AND duplicate_of IS NULL
               GROUP BY conversation_id
             ) mc ON mc.conversation_id = c.id
             WHERE c.account_id = ?1
               AND {involves_contact_sql()}
               AND NOT EXISTS (
                 SELECT 1 FROM trashed_conversations tc
                 WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
               )
               AND NOT EXISTS (
                 SELECT 1 FROM trashed_handles th
                 WHERE th.account_id = c.account_id AND th.handle = c.chat_identifier
               )",
            involves_contact_sql = involves_contact_sql(),
        ))
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
    let (direct, groups, total): (Option<i64>, Option<i64>, Option<i64>) = stats_stmt
        .query_row(rusqlite::params![account_id, contact_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;

    Ok(Some(ContactDetail {
        id: contact_id,
        name,
        handles,
        direct_conversations: direct.unwrap_or(0).max(0) as u64,
        group_conversations: groups.unwrap_or(0).max(0) as u64,
        total_messages: total.unwrap_or(0).max(0) as u64,
    }))
}
```

### Step 2: Register the module

In `message-vault-rs/src/main.rs`, find `mod export_api;` and add next to it:

```rust
mod contacts_api;
```

### Step 3: Add `contact_id` to export participants

In `src/export_api.rs`, extend the struct:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ExportParticipant {
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<i64>,
}
```

Then in `load_participants` (around line 743), replace the SQL and row mapping. The query becomes:

```rust
        let sql = format!(
            "SELECT p.conversation_id, p.handle, p.name_hint, ch.contact_id
             FROM participants p
             JOIN conversations c ON c.id = p.conversation_id
             LEFT JOIN contact_handles ch
               ON ch.account_id = c.account_id AND ch.handle = p.handle
             WHERE p.conversation_id IN ({placeholders})
             ORDER BY p.conversation_id, p.id"
        );
```

and the closure becomes:

```rust
            .query_map(params_from_iter(chunk.iter().copied()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    ExportParticipant {
                        handle: row.get(1)?,
                        name_hint: row.get(2)?,
                        contact_id: row.get(3)?,
                    },
                ))
            })
```

(`contact_handles` has PRIMARY KEY `(account_id, handle)`, so the LEFT JOIN cannot duplicate participant rows.)

### Step 4: Register routes and handlers in `server.rs`

In `src/server.rs`, after the `/v1/export/messages` route line, add:

```rust
        .route("/v1/export/contacts", get(contacts_list_handler))
        .route(
            "/v1/export/contacts/{id}",
            get(contact_detail_handler),
        )
```

Add the handlers near `imports_list_handler` (the `resolve_auth` + `spawn_blocking` + `db.lock()` pattern is already imported/used in this file):

```rust
async fn contacts_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let db = Arc::clone(&state.db);
    let contacts = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database mutex poisoned"))?;
        crate::contacts_api::list_contacts(&conn, &auth.account_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("contacts list task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "contacts": contacts })))
}

async fn contact_detail_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(contact_id): AxumPath<i64>,
) -> Result<Json<crate::contacts_api::ContactDetail>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let db = Arc::clone(&state.db);
    let detail = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database mutex poisoned"))?;
        crate::contacts_api::get_contact_detail(&conn, &auth.account_id, contact_id)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("contact detail task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    match detail {
        Some(d) => Ok(Json(d)),
        None => Err(ApiError::NotFound("contact not found".into())),
    }
}
```

Note: `AxumPath` and `HeaderMap` are already imported in server.rs.

### Step 5: Build

```bash
cd /home/mbeisser/repo/message-vault-rs && cargo build
```

Expected: compiles cleanly.

### Step 6: Commit

```bash
cd /home/mbeisser/repo/message-vault-rs
git add src/contacts_api.rs src/main.rs src/export_api.rs src/server.rs
git commit -m "feat(api): add contacts list/detail endpoints + contact_id on participants

GET /v1/export/contacts returns id, name, handle_count, last_message_at.
GET /v1/export/contacts/{id} returns per-handle service, date range, and
direct-message counts plus direct/group conversation and total-message
stats. Fixes the 404s ContactList and ContactDrawer have been hitting.
load_participants now LEFT JOINs contact_handles so each message carries
contact_id for clickable participant chips.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 3 — Backend: change-password + delete-account endpoints

**Goal:** `POST /v1/auth/change-password` (verify current, set new argon2id hash) and `POST /v1/auth/delete-account` (permanent, cascades via `ON DELETE CASCADE`). Settings Account section consumes both.

**Files:** `src/db/account_profile.rs`, `src/auth.rs`, `src/server.rs`

### Step 1: DB functions

In `src/db/account_profile.rs`, after `load_password_hash`, add:

```rust
/// Replace the argon2 password hash for an account.
pub fn update_password_hash(
    conn: &Connection,
    account_id: &str,
    password_hash: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET password_hash = ?2 WHERE id = ?1",
        params![account_id, password_hash],
    )
    .with_context(|| format!("update password hash for {account_id}"))?;
    Ok(())
}

/// Permanently delete an account. All dependent rows are removed by
/// ON DELETE CASCADE (messages, conversations, contacts, vault_imports,
/// account_phones/emails/api_tokens).
pub fn delete_account(conn: &Connection, account_id: &str) -> Result<()> {
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])
        .with_context(|| format!("delete account {account_id}"))?;
    Ok(())
}
```

### Step 2: Handlers in `auth.rs`

Add to the imports at the top of `src/auth.rs`:

```rust
use axum::http::HeaderMap;
```

Add request/response types near the other request types:

```rust
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    pub confirm: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteAccountResponse {
    pub ok: bool,
}
```

Add the handlers after `hanko_session_handler`:

```rust
/// `POST /v1/auth/change-password` — verify the current password, set a new one.
pub async fn change_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiError> {
    let new_password = req.new_password.trim();
    if new_password.len() < 8 {
        return Err(ApiError::BadRequest(
            "new password must be at least 8 characters".into(),
        ));
    }
    let auth = crate::server::resolve_auth(&headers, &state).await?;
    let account_id = auth.account_id;
    let current_password = req.current_password.clone();
    let db = state.cfg.paths.db.clone();
    let new_hash = hash_password(new_password)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    tokio::task::spawn_blocking(move || -> Result<()> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        let current_hash = account_profile::load_password_hash(&conn, &account_id)?;
        if !passwords_match(current_hash.as_deref(), &current_password) {
            bail!("current password is incorrect");
        }
        account_profile::update_password_hash(&conn, &account_id, &new_hash)?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("change password task: {e}")))?
    .map_err(|e| {
        if e.to_string().contains("current password is incorrect") {
            // 400, not 401 — a wrong current password is a form error,
            // not an expired session (which would trigger a frontend logout).
            ApiError::BadRequest(e.to_string())
        } else {
            ApiError::Internal(e.to_string())
        }
    })?;

    Ok(Json(ChangePasswordResponse { ok: true }))
}

/// `POST /v1/auth/delete-account` — permanently delete the account.
pub async fn delete_account_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteAccountRequest>,
) -> Result<Json<DeleteAccountResponse>, ApiError> {
    if !req.confirm {
        return Err(ApiError::BadRequest("confirmation flag must be true".into()));
    }
    let auth = crate::server::resolve_auth(&headers, &state).await?;
    let account_id = auth.account_id;
    let db = state.cfg.paths.db.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;
        account_profile::delete_account(&conn, &account_id)?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("delete account task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(DeleteAccountResponse { ok: true }))
}
```

### Step 3: Register routes

In `src/server.rs`, after the `/v1/auth/check` route line, add:

```rust
        .route(
            "/v1/auth/change-password",
            post(crate::auth::change_password_handler),
        )
        .route(
            "/v1/auth/delete-account",
            post(crate::auth::delete_account_handler),
        )
```

### Step 4: Build

```bash
cd /home/mbeisser/repo/message-vault-rs && cargo build
```

Expected: compiles cleanly.

### Step 5: Commit

```bash
cd /home/mbeisser/repo/message-vault-rs
git add src/db/account_profile.rs src/auth.rs src/server.rs
git commit -m "feat(api): add change-password and delete-account endpoints

POST /v1/auth/change-password verifies the current argon2 hash before
writing the new one. POST /v1/auth/delete-account requires confirm:true
and relies on ON DELETE CASCADE to remove all account data.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 4 — Backend: `conversations` count on `/v1/export/messages/count`

**Goal:** The Settings storage section shows conversation, message, and attachment counts from one call.

**File:** `src/export_api.rs`

### Step 1: Extend the response and query

In `src/export_api.rs`, add `conversations` to `ExportCountResponse`:

```rust
#[derive(Debug, Serialize)]
pub struct ExportCountResponse {
    pub ok: bool,
    pub query: String,
    pub messages: u64,
    /// Distinct conversations with at least one matching message.
    pub conversations: u64,
    /// Unique attachment digests among matching messages.
    pub attachments: u64,
    /// Sum of known `size_bytes` for those unique digests (unknown sizes omitted).
    pub total_bytes: u64,
}
```

In `export_message_count`, after the `messages` count query (before the attachment query), add:

```rust
    let conv_sql = format!(
        "SELECT COUNT(DISTINCT c.id)
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE {where_sql}{dedupe}",
        where_sql = filters.where_sql,
        dedupe = filters.dedupe_sql,
    );
    let conversations: i64 = conn
        .query_row(
            &conv_sql,
            params_from_iter(filters.params.iter().cloned()),
            |row| row.get(0),
        )
        .map_err(|e| ExportQueryError::Internal(e.to_string()))?;
```

and add `conversations: conversations.max(0) as u64,` to the response construction.

### Step 2: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-rs && cargo build
cd /home/mbeisser/repo/message-vault-rs
git add src/export_api.rs
git commit -m "feat(api): include conversation count in export message count

Settings storage stats now get conversations, messages, attachments,
and total bytes from a single /v1/export/messages/count call.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 5 — Frontend: real message API types, pagination total, find bar with highlighting + next/prev

**Goal:** Fix `MessageView` (it reads a nonexistent `res.total` and types the API response as a shape the backend never sends) and implement the find bar: highlight all matches on the visible page, navigate with next/prev arrows, and show a match counter. The page total comes from `/v1/export/messages/count`.

**Files:** `web/src/lib/types.ts`, `web/src/components/MessageBubble.tsx`, `web/src/screens/MessageView.tsx`

### Step 1: Realign `Message` to the API shape

In `web/src/lib/types.ts`, replace the `Participant` and `Message`/`Attachment`/`PaginatedMessages` block (lines 1–43) with:

```typescript
export interface Participant {
  name: string | null;
  handle: string;
  service: string;
  contact_id: string | null;
}

export interface Conversation {
  id: string;
  participants: Participant[];
  message_count: number;
  last_message_at: string;
  date_range_start: string | null;
  date_range_end: string | null;
  service: string;
  is_group: boolean;
  label: string | null;
}

export interface MessageParticipant {
  handle: string;
  name_hint: string | null;
  contact_id: string | null;
}

export interface MessageConversation {
  id: string;
  chat_identifier: string;
  service: string | null;
  conversation_type: string;
  group_title: string | null;
  participants: MessageParticipant[];
}

export interface MessageAttachment {
  path: string | null;
  original_name: string | null;
  mime_type: string | null;
  sha256: string | null;
  is_sticker: boolean;
  transcription: string | null;
}

export interface MessageTapback {
  part_index: number;
  kind: string;
  emoji: string | null;
  is_from_me: boolean;
  sender: string | null;
}

export interface Message {
  id: string;
  source: string;
  guid: string | null;
  timestamp: string;
  timestamp_utc: string | null;
  is_from_me: boolean;
  sender: string | null;
  subject: string | null;
  text: string | null;
  conversation: MessageConversation;
  attachments: MessageAttachment[];
  tapbacks: MessageTapback[];
}
```

Keep `ExtractConfig` and `ExtractErrorEvent` as-is. `PaginatedMessages` is removed — nothing else uses it (verified: only `types.ts` defines it).

### Step 2: Rewrite `MessageBubble.tsx`

Full file replacement:

```typescript
import type { ReactNode } from "react";
import type { Message } from "../lib/types";

function highlightText(text: string, term: string): ReactNode[] {
  const t = term.trim().toLowerCase();
  if (!t) return [text];
  const out: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (true) {
    const idx = rest.toLowerCase().indexOf(t);
    if (idx === -1) {
      out.push(rest);
      break;
    }
    if (idx > 0) out.push(rest.slice(0, idx));
    out.push(
      <mark key={key++} style={{ background: "#fde68a", borderRadius: "2px", padding: "0 1px" }}>
        {rest.slice(idx, idx + t.length)}
      </mark>,
    );
    rest = rest.slice(idx + t.length);
  }
  return out;
}

function senderName(m: Message): string {
  if (m.is_from_me) return "Me";
  if (m.sender) {
    const p = m.conversation.participants.find((x) => x.handle === m.sender);
    return p?.name_hint || m.sender;
  }
  const p = m.conversation.participants[0];
  return p ? p.name_hint || p.handle : "Unknown";
}

export default function MessageBubble({
  message,
  highlight,
  isActive,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
}) {
  const time = new Date(message.timestamp).toLocaleString([], {
    month: "short", day: "numeric", year: "numeric",
    hour: "numeric", minute: "2-digit",
  });
  const mine = message.is_from_me;

  return (
    <div id={`msg-${message.id}`} style={{
      padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6",
      background: isActive ? "#fef9c3" : "transparent",
    }}>
      <div style={{
        display: "flex", gap: "0.5rem", marginBottom: "0.25rem",
        justifyContent: mine ? "flex-end" : "flex-start",
      }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#374151" }}>
          {senderName(message)}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
      </div>
      <div style={{
        fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5,
        whiteSpace: "pre-wrap", textAlign: mine ? "right" : "left",
      }}>
        {highlight ? highlightText(message.text || "", highlight) : message.text}
      </div>
    </div>
  );
}
```

### Step 3: Rewrite `MessageView.tsx`

Full file replacement:

```typescript
import { useState, useEffect, useCallback } from "react";
import { apiClient } from "../lib/api";
import type { Conversation, Message } from "../lib/types";
import MessageBubble from "../components/MessageBubble";
import PaginationBar from "../components/PaginationBar";

const PAGE_SIZE = 50;

export default function MessageView({ conversation }: { conversation: Conversation }) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [findTerm, setFindTerm] = useState("");
  const [loading, setLoading] = useState(false);
  const [matchIds, setMatchIds] = useState<string[]>([]);
  const [activeIndex, setActiveIndex] = useState(-1);

  const scoped = (term: string) =>
    `in:${conversation.id}${term.trim() ? ` ${term.trim()}` : ""}`;

  const fetchPage = useCallback(
    async (newOffset: number, searchTerm?: string) => {
      setLoading(true);
      try {
        const q = scoped(searchTerm || "");
        const countRes = await apiClient.get<{ ok: boolean; messages: number }>(
          `/v1/export/messages/count?q=${encodeURIComponent(q)}`,
        );
        setTotal(countRes.messages);
        const res = await apiClient.get<{ ok: boolean; messages: Message[] }>(
          `/v1/export/messages?q=${encodeURIComponent(q)}&offset=${newOffset}&limit=${PAGE_SIZE}`,
        );
        setMessages(res.messages);
        setOffset(newOffset);
      } catch {
        setMessages([]);
        setTotal(0);
      } finally {
        setLoading(false);
      }
    },
    [conversation.id],
  );

  useEffect(() => {
    fetchPage(0);
  }, [fetchPage]);

  const handleSearch = () => {
    fetchPage(0, findTerm);
  };

  // Recompute visible matches whenever the page or find term changes.
  useEffect(() => {
    const term = findTerm.trim().toLowerCase();
    if (!term) {
      setMatchIds([]);
      setActiveIndex(-1);
      return;
    }
    const ids = messages
      .filter((m) => (m.text || "").toLowerCase().includes(term))
      .map((m) => m.id);
    setMatchIds(ids);
    setActiveIndex((prev) =>
      prev === -1 || prev >= ids.length ? (ids.length ? 0 : -1) : prev,
    );
  }, [messages, findTerm]);

  // Scroll the active match into view.
  useEffect(() => {
    if (activeIndex < 0 || activeIndex >= matchIds.length) return;
    document
      .getElementById(`msg-${matchIds[activeIndex]}`)
      ?.scrollIntoView({ block: "center" });
  }, [activeIndex, matchIds]);

  const nextMatch = () => {
    if (!matchIds.length) return;
    setActiveIndex((activeIndex + 1) % matchIds.length);
  };
  const prevMatch = () => {
    if (!matchIds.length) return;
    setActiveIndex((activeIndex - 1 + matchIds.length) % matchIds.length);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Header */}
      <div style={{
        padding: "0.75rem 1.5rem", borderBottom: "1px solid #e5e7eb",
        background: "#fafafa",
      }}>
        <div style={{ fontSize: "1rem", fontWeight: 600, marginBottom: "0.25rem" }}>
          {conversation.label ||
            (conversation.is_group
              ? `${conversation.participants.length} participants`
              : conversation.participants[0]?.name || conversation.participants[0]?.handle)}
        </div>
        <div style={{ display: "flex", gap: "1rem", fontSize: "0.75rem", color: "#6b7280", flexWrap: "wrap" }}>
          <span>{conversation.service}</span>
          {conversation.date_range_start && conversation.date_range_end && (
            <span>
              {new Date(conversation.date_range_start).toLocaleDateString([], { month: "short", year: "numeric" })} –{" "}
              {new Date(conversation.date_range_end).toLocaleDateString([], { month: "short", year: "numeric" })}
            </span>
          )}
          <span>{conversation.message_count} messages</span>
        </div>
      </div>

      {/* Find bar */}
      <div style={{
        padding: "0.375rem 1.5rem", borderBottom: "1px solid #e5e7eb",
        display: "flex", gap: "0.5rem", alignItems: "center",
      }}>
        <input
          type="text"
          value={findTerm}
          onChange={(e) => setFindTerm(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          placeholder="Find in conversation…"
          style={{
            flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.813rem",
            border: "1px solid #d1d5db", borderRadius: "4px",
          }}
        />
        <button onClick={handleSearch} style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
          Find
        </button>
        {matchIds.length > 0 && (
          <>
            <button onClick={prevMatch} title="Previous match"
              style={{ padding: "0.25rem 0.5rem", fontSize: "0.813rem" }}>
              ↑
            </button>
            <button onClick={nextMatch} title="Next match"
              style={{ padding: "0.25rem 0.5rem", fontSize: "0.813rem" }}>
              ↓
            </button>
            <span style={{ fontSize: "0.75rem", color: "#6b7280", whiteSpace: "nowrap" }}>
              {activeIndex + 1} of {matchIds.length} on this page
            </span>
          </>
        )}
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflow: "auto" }}>
        {loading ? (
          <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>
        ) : (
          messages.map((m) => (
            <MessageBubble
              key={m.id}
              message={m}
              highlight={findTerm.trim() || undefined}
              isActive={m.id === matchIds[activeIndex]}
            />
          ))
        )}
      </div>

      {/* Pagination */}
      <PaginationBar
        offset={offset}
        limit={PAGE_SIZE}
        total={total}
        onPrev={() => fetchPage(Math.max(0, offset - PAGE_SIZE))}
        onNext={() => fetchPage(offset + PAGE_SIZE)}
      />
    </div>
  );
}
```

### Step 4: Build

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
```

Expected: compiles cleanly. (Needs Task 1 merged — the `in:<id>` query is what scopes the page.)

### Step 5: Commit

```bash
cd /home/mbeisser/repo/message-vault-io
git add web/src/lib/types.ts web/src/components/MessageBubble.tsx web/src/screens/MessageView.tsx
git commit -m "fix(web): real message API shapes, correct totals, find bar highlighting

MessageView typed the export response as a shape the API never sends
(res.total, camelCase Message) — the page total is now fetched from
/v1/export/messages/count and Message matches the wire format
(timestamp/text/sender, nested conversation with participants).
Find bar highlights every match on the visible page, with up/down
arrows cycling matches and a 'N of M on this page' counter; the
active match is scrolled into view and tinted.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 6 — Frontend: participant chips in the message header

**Goal:** Clickable participant chips in the message header (spec: "participant chips (clickable — opens profile drawer)"). Chips come from the loaded page's message data (`messages[0].conversation.participants`), which carries `contact_id` from Task 2. Chips without a `contact_id` render non-clickable.

**Files:** `web/src/screens/MessageView.tsx`, `web/src/components/AppLayout.tsx`

### Step 1: Add the chips + callback prop to `MessageView.tsx`

Change the component signature to:

```typescript
export default function MessageView({
  conversation,
  onOpenContact,
}: {
  conversation: Conversation;
  onOpenContact?: (contactId: string) => void;
}) {
```

Inside the component, before the `return`, derive header participants:

```typescript
  const headerParticipants = messages[0]?.conversation.participants || [];
```

In the header block, after the metadata row (the div with `{conversation.service}` / date range / message count), insert:

```typescript
        {headerParticipants.length > 0 && (
          <div style={{ display: "flex", gap: "0.375rem", flexWrap: "wrap", marginTop: "0.375rem" }}>
            {headerParticipants.map((p, i) => {
              const label = p.name_hint || p.handle;
              return p.contact_id ? (
                <button
                  key={i}
                  onClick={() => onOpenContact?.(p.contact_id!)}
                  title={`Open contact for ${label}`}
                  style={{
                    fontSize: "0.75rem", padding: "0.125rem 0.5rem", borderRadius: "999px",
                    border: "1px solid #d1d5db", background: "#fff",
                    color: "#2563eb", cursor: "pointer",
                  }}
                >
                  {label}
                </button>
              ) : (
                <span
                  key={i}
                  style={{
                    fontSize: "0.75rem", padding: "0.125rem 0.5rem", borderRadius: "999px",
                    border: "1px solid #e5e7eb", background: "#f9fafb", color: "#6b7280",
                  }}
                >
                  {label}
                </span>
              );
            })}
          </div>
        )}
```

### Step 2: Wire the callback in `AppLayout.tsx`

Change the `MessageView` usage in `AppLayout.tsx` (the `case "conversations":` branch) to:

```typescript
        return selectedConversation ? (
          <MessageView
            conversation={selectedConversation}
            onOpenContact={(contactId) => setSelectedContactId(contactId)}
          />
        ) : (
```

### Step 3: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/screens/MessageView.tsx web/src/components/AppLayout.tsx
git commit -m "feat(web): clickable participant chips in message header

Chips render from the loaded page's conversation participants.
Chips with a contact_id open the contact drawer; unmatched handles
render as inert pills.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 7 — Frontend: GlobalSearch with operator autocomplete

**Goal:** Replace the plain search input in `LeftPanel` with a component that autocompletes operators (`from:`, `to:`, `with:`, `within:`, `label:`, `handle:`, `has:`, `after:`, `before:`, `source:`, `subject:`, `is:`) and — after an operator colon — contact names from `GET /v1/export/contacts`. Keyboard: ↓/↑ to move, Enter to accept or run the search, Escape to close.

**Files:** `web/src/components/GlobalSearch.tsx` (create), `web/src/components/LeftPanel.tsx`

### Step 1: Create `GlobalSearch.tsx`

```typescript
import { useEffect, useRef, useState } from "react";
import { apiClient } from "../lib/api";

const OPERATORS = [
  "from:", "to:", "with:", "within:", "label:", "handle:",
  "has:", "after:", "before:", "source:", "subject:", "is:",
];

interface ContactName {
  id: string;
  name: string;
}

export default function GlobalSearch({
  value,
  onChange,
  onSubmit,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (q: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const [contacts, setContacts] = useState<ContactName[]>([]);

  useEffect(() => {
    apiClient
      .get<{ contacts: ContactName[] }>("/v1/export/contacts")
      .then((res) => setContacts(res.contacts))
      .catch(() => setContacts([]));
  }, []);

  const lastToken = value.split(/\s+/).pop() || "";
  const colonIdx = lastToken.indexOf(":");
  const completingValue = colonIdx !== -1;
  const opPrefix = completingValue ? lastToken.slice(0, colonIdx + 1) : "";
  const valuePart = completingValue ? lastToken.slice(colonIdx + 1) : "";

  const suggestions: string[] = completingValue
    ? contacts
        .map((c) => c.name)
        .filter((n) => n.toLowerCase().includes(valuePart.toLowerCase()))
        .slice(0, 6)
    : OPERATORS.filter((op) => op.startsWith(lastToken.toLowerCase())).slice(0, 6);

  const applySuggestion = (s: string) => {
    const tokens = value.split(/\s+/);
    tokens.pop();
    const next = completingValue
      ? tokens.concat(`${opPrefix}"${s}"`).join(" ")
      : tokens.concat(`${s} `).join(" ");
    onChange(next);
    setOpen(false);
  };

  return (
    <div style={{ position: "relative" }}>
      <input
        type="search"
        value={value}
        onChange={(e) => { onChange(e.target.value); setOpen(true); setHighlight(0); }}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            if (open && suggestions.length > 0) {
              applySuggestion(suggestions[Math.min(highlight, suggestions.length - 1)]);
            } else {
              onSubmit(value);
            }
          } else if (e.key === "ArrowDown") {
            e.preventDefault();
            setOpen(true);
            setHighlight((h) => (suggestions.length ? (h + 1) % suggestions.length : 0));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setHighlight((h) => (suggestions.length ? (h - 1 + suggestions.length) % suggestions.length : 0));
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
        placeholder="Search vault"
        style={{
          width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.813rem",
          border: "1px solid #d1d5db", borderRadius: "4px", boxSizing: "border-box",
        }}
      />
      {open && suggestions.length > 0 && (
        <div style={{
          position: "absolute", top: "100%", left: 0, right: 0, zIndex: 60,
          background: "#fff", border: "1px solid #e5e7eb", borderRadius: "4px",
          boxShadow: "0 4px 12px rgba(0,0,0,0.1)", marginTop: "2px", overflow: "hidden",
        }}>
          {suggestions.map((s, i) => (
            <button
              key={s}
              onMouseDown={(e) => { e.preventDefault(); applySuggestion(s); }}
              onMouseEnter={() => setHighlight(i)}
              style={{
                display: "block", width: "100%", textAlign: "left", border: "none",
                background: i === highlight ? "#e5e7eb" : "#fff",
                padding: "0.375rem 0.5rem", fontSize: "0.813rem", cursor: "pointer",
              }}
            >
              {completingValue ? `${opPrefix}"${s}"` : s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

### Step 2: Swap into `LeftPanel.tsx`

Add the import:

```typescript
import GlobalSearch from "./GlobalSearch";
```

Replace the global-search block (the `<div style={{ padding: "0.75rem" }}>` containing the plain `<input>`) with:

```typescript
      {/* Global search */}
      <div style={{ padding: "0.75rem" }}>
        <GlobalSearch value={searchQuery} onChange={onSearchChange} onSubmit={onSearch} />
      </div>
```

(Search-on-Enter and query population from saved groups both flow through the existing `onSearchChange`/`onSearch` props — unchanged.)

### Step 3: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/components/GlobalSearch.tsx web/src/components/LeftPanel.tsx
git commit -m "feat(web): GlobalSearch with operator and contact autocomplete

Replaces the plain search input. Suggests operators while typing a
token, then contact names after an operator colon (quoted on insert).
Arrow keys navigate, Enter accepts or runs the search.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 8 — Frontend: AdvancedSearchForm

**Goal:** The spec's advanced search form with Messages and Contacts tabs, composing a backend query string. Toggle from the search bar area in `LeftPanel`.

**Files:** `web/src/components/AdvancedSearchForm.tsx` (create), `web/src/components/LeftPanel.tsx`

### Step 1: Create `AdvancedSearchForm.tsx`

```typescript
import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { apiClient } from "../lib/api";

export default function AdvancedSearchForm({
  onApply,
  onClose,
}: {
  onApply: (query: string) => void;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"messages" | "contacts">("messages");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [withPerson, setWithPerson] = useState("");
  const [hasWords, setHasWords] = useState("");
  const [notWords, setNotWords] = useState("");
  const [subject, setSubject] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [msgType, setMsgType] = useState<"all" | "direct" | "group">("all");
  const [source, setSource] = useState("");
  const [sources, setSources] = useState<string[]>([]);
  const [handle, setHandle] = useState("");
  const [firstMsgDate, setFirstMsgDate] = useState("");
  const [lastMsgDate, setLastMsgDate] = useState("");
  const [msgCount, setMsgCount] = useState("");
  const [groupCount, setGroupCount] = useState("");

  useEffect(() => {
    apiClient
      .get<{ ok: boolean; sources: string[] }>("/v1/auth/check")
      .then((res) => setSources(res.sources))
      .catch(() => setSources([]));
  }, []);

  const inputStyle: CSSProperties = {
    width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.813rem",
    border: "1px solid #d1d5db", borderRadius: "4px", boxSizing: "border-box",
  };
  const labelStyle: CSSProperties = {
    fontSize: "0.688rem", fontWeight: 600, color: "#6b7280",
    textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: "0.25rem",
    display: "block",
  };

  const buildQuery = (): string => {
    const parts: string[] = [];
    const push = (s: string) => { if (s.trim()) parts.push(s.trim()); };
    if (tab === "messages") {
      if (from) push(`from:"${from}"`);
      if (to) push(`to:"${to}"`);
      if (withPerson) push(`with:"${withPerson}"`);
      if (hasWords) push(hasWords.trim());
      if (notWords) push(notWords.trim().split(/\s+/).map((w) => `-${w}`).join(" "));
      if (subject) push(`subject:"${subject}"`);
      if (dateFrom) push(`after:${dateFrom}`);
      if (dateTo) push(`before:${dateTo}`);
      if (msgType === "direct") push("is:direct");
      if (msgType === "group") push("is:group");
      if (source) push(`source:${source}`);
    } else {
      if (handle) push(`handle:"${handle}"`);
      if (firstMsgDate) push(`first-contact:${firstMsgDate}`);
      if (lastMsgDate) push(`last-contact:${lastMsgDate}`);
      if (msgCount) push(`message-count:${msgCount}`);
      if (groupCount) push(`group-count:${groupCount}`);
      push("search:contacts");
    }
    return parts.join(" ");
  };

  return (
    <div style={{
      background: "#fff", border: "1px solid #e5e7eb", borderRadius: "6px",
      boxShadow: "0 4px 12px rgba(0,0,0,0.1)", padding: "0.75rem", zIndex: 60,
    }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.75rem" }}>
        <button
          onClick={() => setTab("messages")}
          style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem", fontWeight: tab === "messages" ? 600 : 400 }}
        >
          Messages
        </button>
        <button
          onClick={() => setTab("contacts")}
          style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem", fontWeight: tab === "contacts" ? 600 : 400 }}
        >
          Contacts
        </button>
        <span style={{ flex: 1 }} />
        <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1rem", cursor: "pointer" }}>×</button>
      </div>

      {tab === "messages" ? (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div>
            <label style={labelStyle}>From</label>
            <input style={inputStyle} value={from} onChange={(e) => setFrom(e.target.value)} placeholder="Name or handle" />
          </div>
          <div>
            <label style={labelStyle}>To</label>
            <input style={inputStyle} value={to} onChange={(e) => setTo(e.target.value)} placeholder="Name or handle" />
          </div>
          <div>
            <label style={labelStyle}>With person</label>
            <input style={inputStyle} value={withPerson} onChange={(e) => setWithPerson(e.target.value)} placeholder="Name or handle" />
          </div>
          <div>
            <label style={labelStyle}>Has words</label>
            <input style={inputStyle} value={hasWords} onChange={(e) => setHasWords(e.target.value)} placeholder="vacation beach" />
          </div>
          <div>
            <label style={labelStyle}>Doesn't have words</label>
            <input style={inputStyle} value={notWords} onChange={(e) => setNotWords(e.target.value)} placeholder="work meeting" />
          </div>
          <div>
            <label style={labelStyle}>Subject</label>
            <input style={inputStyle} value={subject} onChange={(e) => setSubject(e.target.value)} />
          </div>
          <div>
            <label style={labelStyle}>Date from</label>
            <input type="date" style={inputStyle} value={dateFrom} onChange={(e) => setDateFrom(e.target.value)} />
          </div>
          <div>
            <label style={labelStyle}>Date to</label>
            <input type="date" style={inputStyle} value={dateTo} onChange={(e) => setDateTo(e.target.value)} />
          </div>
          <div>
            <label style={labelStyle}>Message type</label>
            <select style={inputStyle} value={msgType} onChange={(e) => setMsgType(e.target.value as "all" | "direct" | "group")}>
              <option value="all">All</option>
              <option value="direct">Direct</option>
              <option value="group">Group</option>
            </select>
          </div>
          <div>
            <label style={labelStyle}>Source</label>
            <select style={inputStyle} value={source} onChange={(e) => setSource(e.target.value)}>
              <option value="">Any</option>
              {sources.map((s) => <option key={s} value={s}>{s}</option>)}
            </select>
          </div>
        </div>
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div>
            <label style={labelStyle}>Handle</label>
            <input style={inputStyle} value={handle} onChange={(e) => setHandle(e.target.value)} placeholder="bob#1234" />
          </div>
          <div>
            <label style={labelStyle}>First message date from</label>
            <input type="date" style={inputStyle} value={firstMsgDate} onChange={(e) => setFirstMsgDate(e.target.value)} />
          </div>
          <div>
            <label style={labelStyle}>Last message date to</label>
            <input type="date" style={inputStyle} value={lastMsgDate} onChange={(e) => setLastMsgDate(e.target.value)} />
          </div>
          <div>
            <label style={labelStyle}>Message count</label>
            <input type="number" style={inputStyle} value={msgCount} onChange={(e) => setMsgCount(e.target.value)} placeholder="e.g. 1000" />
          </div>
          <div>
            <label style={labelStyle}>Group conversation count</label>
            <input type="number" style={inputStyle} value={groupCount} onChange={(e) => setGroupCount(e.target.value)} placeholder="e.g. 3" />
          </div>
        </div>
      )}

      <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end", marginTop: "0.75rem" }}>
        <button onClick={onClose} style={{ padding: "0.375rem 0.75rem", fontSize: "0.813rem" }}>
          Cancel
        </button>
        <button
          onClick={() => onApply(buildQuery())}
          style={{ padding: "0.375rem 1rem", fontSize: "0.813rem", fontWeight: 600 }}
        >
          Apply
        </button>
      </div>
    </div>
  );
}
```

Note: the file uses `CSSProperties` from the type import at the top — there is no `React` default import.

### Step 2: Wire the toggle into `LeftPanel.tsx`

**Target block:** the global-search block introduced by Task 7 (the `<div style={{ padding: "0.75rem" }}>` containing `<GlobalSearch .../>`) — not the original plain-input block (already replaced).

Add the import:

```typescript
import AdvancedSearchForm from "./AdvancedSearchForm";
```

Add state next to the existing `groups` state:

```typescript
  const [showAdvanced, setShowAdvanced] = useState(false);
```

Replace the global-search block with:

```typescript
      {/* Global search */}
      <div style={{ padding: "0.75rem", paddingBottom: "0.25rem" }}>
        <GlobalSearch value={searchQuery} onChange={onSearchChange} onSubmit={onSearch} />
        <button
          onClick={() => setShowAdvanced(!showAdvanced)}
          style={{
            fontSize: "0.688rem", border: "none", background: "none",
            color: "#2563eb", cursor: "pointer", padding: 0, marginTop: "0.25rem",
          }}
        >
          {showAdvanced ? "Hide advanced search" : "Advanced search"}
        </button>
      </div>
      {showAdvanced && (
        <div style={{ padding: "0 0.75rem 0.5rem" }}>
          <AdvancedSearchForm
            onApply={(q) => { if (q) onSearch(q); setShowAdvanced(false); }}
            onClose={() => setShowAdvanced(false)}
          />
        </div>
      )}
```

### Step 3: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/components/AdvancedSearchForm.tsx web/src/components/LeftPanel.tsx
git commit -m "feat(web): advanced search form with Messages/Contacts tabs

Composes backend query strings (from/to/with/has/subject/after/before/
is:direct|group/source on Messages; handle/first-contact/last-contact/
message-count/group-count + search:contacts on Contacts). Sources list
comes from /v1/auth/check.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 9 — Frontend: conversation checkboxes

**Goal:** Checkboxes on conversation rows for ad-hoc export selection (spec: "Ad-hoc selection: the user selects specific conversations from the list"). Selection state lives in `AppLayout` so both the rows and the export popover can read it.

**Files:** `web/src/components/ConversationRow.tsx`, `web/src/screens/ConversationList.tsx`, `web/src/components/AppLayout.tsx`

### Step 1: `ConversationRow.tsx`

Change the component signature and wrap the row in a flex container with a checkbox:

```typescript
export default function ConversationRow({
  conversation,
  isSelected,
  onClick,
  checked,
  onToggle,
}: {
  conversation: Conversation;
  isSelected: boolean;
  onClick: () => void;
  checked?: boolean;
  onToggle?: () => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", borderBottom: "1px solid #f3f4f6" }}>
      <button
        onClick={onClick}
        style={{
          display: "block", flex: 1, textAlign: "left", border: "none",
          background: isSelected ? "#e5e7eb" : "transparent",
          padding: "0.5rem 0.75rem", cursor: "pointer",
        }}
      >
        <div style={{
          display: "flex", justifyContent: "space-between", alignItems: "baseline",
          marginBottom: "2px",
        }}>
          <span style={{
            fontSize: "0.875rem", fontWeight: 500, color: "#1f2937",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            flex: 1, marginRight: "0.5rem",
          }}>
            {displayName(conversation)}
          </span>
          <span style={{ fontSize: "0.75rem", color: "#9ca3af", flexShrink: 0 }}>
            {formatDate(conversation.last_message_at)}
          </span>
        </div>
        <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
          {subtitle(conversation)}
        </div>
      </button>
      {onToggle && (
        <input
          type="checkbox"
          checked={!!checked}
          onChange={(e) => {
            e.stopPropagation();
            onToggle();
          }}
          onClick={(e) => e.stopPropagation()}
          style={{ margin: "0 0.5rem", flexShrink: 0 }}
        />
      )}
    </div>
  );
}
```

(The row was previously a bare `<button>` with `borderBottom` in its own style — the border moves to the wrapper.)

### Step 2: `ConversationList.tsx`

Change the signature and row usage:

```typescript
export default function ConversationList({
  selectedId,
  onSelect,
  query,
  checkedIds,
  onToggle,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
  checkedIds?: Set<string>;
  onToggle?: (id: string) => void;
}) {
```

and the row:

```typescript
        <ConversationRow
          key={c.id}
          conversation={c}
          isSelected={c.id === selectedId}
          onClick={() => onSelect(c)}
          checked={checkedIds?.has(c.id)}
          onToggle={onToggle ? () => onToggle(c.id) : undefined}
        />
```

### Step 3: `AppLayout.tsx`

Add state next to `exportScope`:

```typescript
  const [checkedIds, setCheckedIds] = useState<Set<string>>(new Set());
```

Pass to the list (in `leftContent`):

```typescript
      <ConversationList
        selectedId={selectedConversation?.id || null}
        onSelect={(c) => { setSelectedConversation(c); setActiveView("conversations"); }}
        query={activeView === "trash" ? "is:trash" : searchQuery}
        checkedIds={checkedIds}
        onToggle={(id) => setCheckedIds((prev) => {
          const next = new Set(prev);
          if (next.has(id)) next.delete(id); else next.add(id);
          return next;
        })}
      />
```

### Step 4: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/components/ConversationRow.tsx web/src/screens/ConversationList.tsx web/src/components/AppLayout.tsx
git commit -m "feat(web): conversation checkboxes for ad-hoc export selection

Checkbox on each row toggles a Set<string> owned by AppLayout.
Checked count feeds the export popover (Task 10).

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 10 — Frontend: export popover with three scope options

**Goal:** Spec export behavior: the Export button opens a popover with "Export entire vault" (always), "Export current view" (enabled when a search/saved group is active), and "Export selected" (enabled when conversations are checked). The most likely option is pre-selected. Selecting one navigates to the export view with that scope.

**Files:** `web/src/components/LeftPanel.tsx`, `web/src/components/AppLayout.tsx`

### Step 1: `LeftPanel.tsx`

Add props and popover state:

```typescript
export default function LeftPanel({
  activeView,
  onNavigate,
  searchQuery,
  onSearchChange,
  onSearch,
  conversationList,
  selectedCount = 0,
  hasActiveQuery = false,
  onExport,
}: {
  activeView: string;
  onNavigate: (view: string) => void;
  searchQuery: string;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
  conversationList?: ReactNode;
  selectedCount?: number;
  hasActiveQuery?: boolean;
  onExport: (scope: "all" | "current-view" | "selected") => void;
}) {
```

Add state:

```typescript
  const [exportOpen, setExportOpen] = useState(false);
```

Replace the Export button block (the `isTauri()` section) with:

```typescript
      {isTauri() && (
        <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
          <button
            onClick={() => onNavigate("import")}
            style={{
              width: "100%", padding: "0.5rem", marginBottom: "0.375rem",
              fontSize: "0.875rem", fontWeight: 600,
            }}
          >
            Import
          </button>
          <div style={{ position: "relative" }}>
            <button
              onClick={() => setExportOpen(!exportOpen)}
              style={{ width: "100%", padding: "0.5rem", fontSize: "0.875rem" }}
            >
              Export
            </button>
            {exportOpen && (
              <div style={{
                position: "absolute", bottom: "100%", left: 0, right: 0,
                marginBottom: "4px", background: "#fff", border: "1px solid #e5e7eb",
                borderRadius: "6px", boxShadow: "0 4px 12px rgba(0,0,0,0.1)",
                padding: "0.25rem", zIndex: 50,
              }}>
                <button
                  onClick={() => { onExport("all"); setExportOpen(false); }}
                  style={popoverItem(false)}
                >
                  Export entire vault
                </button>
                <button
                  disabled={!hasActiveQuery}
                  onClick={() => { onExport("current-view"); setExportOpen(false); }}
                  style={popoverItem(!hasActiveQuery)}
                >
                  Export current view
                </button>
                <button
                  disabled={selectedCount === 0}
                  onClick={() => { onExport("selected"); setExportOpen(false); }}
                  style={popoverItem(selectedCount === 0)}
                >
                  Export selected{selectedCount > 0 ? ` (${selectedCount})` : ""}
                </button>
              </div>
            )}
          </div>
        </div>
      )}
```

Add the `popoverItem` helper near `linkStyle`:

```typescript
  const popoverItem = (disabled: boolean) => ({
    display: "block", width: "100%", textAlign: "left" as const,
    border: "none", background: "transparent", padding: "0.375rem 0.5rem",
    fontSize: "0.813rem", cursor: disabled ? "default" as const : "pointer" as const,
    color: disabled ? "#9ca3af" : "#1f2937",
  });
```

### Step 2: `AppLayout.tsx`

Give `exportScope` a setter and wire `onExport`:

```typescript
  const [exportScope, setExportScope] = useState<"all" | "current-view" | "selected">("all");
```

Change the `export` case to use the real count:

```typescript
      case "export": return <ExportScreen scope={exportScope} selectedCount={checkedIds.size} />;
```

Pass the new props to `LeftPanel`:

```typescript
      <LeftPanel
        activeView={activeView}
        onNavigate={setActiveView}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={(q) => { setSearchQuery(q); setActiveView("conversations"); }}
        conversationList={leftContent}
        selectedCount={checkedIds.size}
        hasActiveQuery={searchQuery.trim().length > 0}
        onExport={(scope) => { setExportScope(scope); setActiveView("export"); }}
      />
```

### Step 3: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/components/LeftPanel.tsx web/src/components/AppLayout.tsx
git commit -m "feat(web): export popover with three scope options

Export button now opens a popover: entire vault (always), current
view (when a search is active), or selected (when conversations are
checked). Selecting navigates to the export screen with the scope.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 11 — Frontend: import history screen

**Goal:** Chronological list of past imports (spec: "Accessible from the Import button dropdown or the Settings screen… helps answer 'did I already import that backup?'"). Backed by the existing `GET /v1/imports` (`{ imports: ImportSummary[] }`).

**Files:** `web/src/screens/ImportHistoryScreen.tsx` (create), `web/src/components/AppLayout.tsx`, `web/src/screens/SettingsScreen.tsx`, `web/src/screens/ImportScreen.tsx`

### Step 1: Create `ImportHistoryScreen.tsx`

```typescript
import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface ImportRecord {
  id: number;
  source: string;
  tool: string | null;
  mode: string;
  status: string;
  started_at: string;
  finished_at: string | null;
  message_count: number;
  attachment_count: number;
  bytes_uploaded: number;
}

function formatBytes(n: number): string {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${n} B`;
}

export default function ImportHistoryScreen() {
  const [imports, setImports] = useState<ImportRecord[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    apiClient
      .get<{ imports: ImportRecord[] }>("/v1/imports")
      .then((res) => setImports(res.imports))
      .catch(() => setImports([]))
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return <div style={{ padding: "1.5rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>;
  }

  return (
    <div style={{ padding: "1.5rem", maxWidth: "800px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Import History</h2>
      {imports.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "#9ca3af" }}>No imports yet.</div>
      ) : (
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.813rem" }}>
          <thead>
            <tr style={{ textAlign: "left", color: "#6b7280" }}>
              <th style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb" }}>Date</th>
              <th style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb" }}>Source</th>
              <th style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb" }}>Messages</th>
              <th style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb" }}>Attachments</th>
              <th style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb" }}>Size</th>
              <th style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb" }}>Status</th>
            </tr>
          </thead>
          <tbody>
            {imports.map((r) => (
              <tr key={r.id}>
                <td style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6", whiteSpace: "nowrap" }}>
                  {new Date(r.started_at).toLocaleString([], {
                    month: "short", day: "numeric", year: "numeric",
                    hour: "numeric", minute: "2-digit",
                  })}
                </td>
                <td style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6" }}>
                  {r.source}{r.tool ? ` (${r.tool})` : ""}
                </td>
                <td style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6" }}>
                  {r.message_count.toLocaleString()}
                </td>
                <td style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6" }}>
                  {r.attachment_count.toLocaleString()}
                </td>
                <td style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6" }}>
                  {formatBytes(r.bytes_uploaded)}
                </td>
                <td style={{ padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6" }}>{r.status}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
```

### Step 2: Add the view to `AppLayout.tsx`

Add the import:

```typescript
import ImportHistoryScreen from "../screens/ImportHistoryScreen";
```

Add a case in `mainContent`:

```typescript
      case "import-history": return <ImportHistoryScreen />;
```

### Step 3: Link from Settings and Import screens

`SettingsScreen.tsx` — add an optional `onNavigate` prop:

```typescript
export default function SettingsScreen({
  onNavigate,
}: {
  onNavigate?: (view: string) => void;
}) {
```

Add under the "Vault Connection" heading:

```typescript
      <button
        onClick={() => onNavigate?.("import-history")}
        style={{
          fontSize: "0.813rem", border: "none", background: "none",
          color: "#2563eb", cursor: "pointer", padding: 0, marginBottom: "1rem",
        }}
      >
        Import history
      </button>
```

`ImportScreen.tsx` — add the same optional prop and a header-row button:

```typescript
export default function ImportScreen({
  onNavigate,
}: {
  onNavigate?: (view: string) => void;
}) {
```

Replace the `<h2>` with a header row:

```typescript
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1.5rem" }}>
        <h2 style={{ margin: 0 }}>Import to Vault</h2>
        <button
          onClick={() => onNavigate?.("import-history")}
          style={{
            fontSize: "0.813rem", border: "none", background: "none",
            color: "#2563eb", cursor: "pointer", padding: 0,
          }}
        >
          Import history
        </button>
      </div>
```

`AppLayout.tsx` — pass the callback to both:

```typescript
      case "import": return <ImportScreen onNavigate={setActiveView} />;
      case "settings": return <SettingsScreen onNavigate={setActiveView} />;
```

### Step 4: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/screens/ImportHistoryScreen.tsx web/src/components/AppLayout.tsx web/src/screens/SettingsScreen.tsx web/src/screens/ImportScreen.tsx
git commit -m "feat(web): import history screen

Chronological table of past imports (date, source, tool, messages,
attachments, size, status) from GET /v1/imports. Reachable from
Settings and from a header button on the Import screen.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 12 — Frontend + Tauri: import contacts conflict review

**Goal:** Spec's step 4 of the import flow — when a contacts file is provided, parse it and show a side-by-side comparison (file name | vault name | handle | Use file / Use vault / Edit) before importing. The Tauri `contacts_info` command currently returns only `count`/`format`/`preview` (10 names) — extend it to return the full card list (name + first phone/email) so real review is possible.

**Dependency note:** Tier 1 Task 3 rewrites `ImportScreen` to do a real extract+push. This task assumes `contactsPath` state (already present in today's file) survives; if the merged Tier 1 file differs, apply only the *additions* below to the new structure.

**Files:** `src-tauri/src/commands/contacts.rs`, `web/src/lib/tauri.ts`, `web/src/components/ContactReviewTable.tsx` (create), `web/src/screens/ImportScreen.tsx`

### Step 1: Extend the Tauri command

In `src-tauri/src/commands/contacts.rs`, add a card type and extend `ContactsInfo`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ContactCardInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactsInfo {
    pub count: usize,
    pub format: String,
    pub preview: Vec<String>,
    pub cards: Vec<ContactCardInfo>,
}
```

Add a small helper above `contacts_info`:

```rust
fn card_name(fn_raw: &str, given: &str, family: &str) -> String {
    if !fn_raw.is_empty() {
        return fn_raw.clone();
    }
    let name = format!("{given} {family}").trim().to_string();
    if name.is_empty() { "(unknown)".to_string() } else { name }
}
```

In the `Vcf` arm, build the full list (the existing `preview` computation stays):

```rust
            let cards = parse_vcf(&path).map_err(|e| e.to_string())?;
            let preview: Vec<String> = cards
                .iter()
                .take(10)
                .map(|c| card_name(&c.fn_raw, &c.n_given, &c.n_family))
                .collect();
            let all: Vec<ContactCardInfo> = cards
                .iter()
                .map(|c| ContactCardInfo {
                    name: card_name(&c.fn_raw, &c.n_given, &c.n_family),
                    handle: c.phones.first().cloned().or_else(|| c.email.clone()),
                })
                .collect();
            Ok(ContactsInfo {
                count: cards.len(),
                format: "vcf".to_string(),
                preview,
                cards: all,
            })
```

In the `VcardCsv` arm, same treatment (`ContactCsvRow` exposes `phones: Vec<String>`):

```rust
            let rows = read_vcard_csv_rows(&path).map_err(|e| e.to_string())?;
            let preview: Vec<String> = rows
                .iter()
                .take(10)
                .map(|r| {
                    let name = format!("{} {} {}", r.first, r.middle, r.last)
                        .trim()
                        .to_string();
                    if name.is_empty() { "(unknown)".to_string() } else { name }
                })
                .collect();
            let all: Vec<ContactCardInfo> = rows
                .iter()
                .map(|r| {
                    let name = format!("{} {} {}", r.first, r.middle, r.last)
                        .trim()
                        .to_string();
                    ContactCardInfo {
                        name: if name.is_empty() { "(unknown)".to_string() } else { name },
                        handle: r.phones.first().cloned(),
                    }
                })
                .collect();
            Ok(ContactsInfo {
                count: rows.len(),
                format: "csv".to_string(),
                preview,
                cards: all,
            })
```

The old inline `fn_raw`-based preview mapping in the `Vcf` arm is replaced by the `card_name` helper.

### Step 2: Update the TS type in `lib/tauri.ts`

Replace the `ContactsInfo` interface (lines 79–83):

```typescript
export interface ContactCardInfo {
  name: string;
  handle: string | null;
}

export interface ContactsInfo {
  count: number;
  format: string;
  preview: string[];
  cards: ContactCardInfo[];
}
```

### Step 3: Create `ContactReviewTable.tsx`

```typescript
import type { CSSProperties } from "react";

export interface ReviewRow {
  id: string;
  fileName: string;
  vaultName: string | null;
  handle: string | null;
  action: "file" | "vault" | "edit" | null;
}

const th: CSSProperties = {
  padding: "0.375rem 0.5rem", borderBottom: "1px solid #e5e7eb",
  textAlign: "left", fontSize: "0.75rem", color: "#6b7280",
};
const td: CSSProperties = {
  padding: "0.375rem 0.5rem", borderBottom: "1px solid #f3f4f6",
};

function actionStyle(active: boolean, disabled: boolean): CSSProperties {
  return {
    fontSize: "0.75rem", padding: "0.25rem 0.5rem", cursor: disabled ? "default" : "pointer",
    border: "1px solid #d1d5db", borderRadius: "4px", background: active ? "#2563eb" : "#fff",
    color: active ? "#fff" : "#1f2937", opacity: disabled ? 0.4 : 1,
  };
}

export default function ContactReviewTable({
  rows,
  onAction,
}: {
  rows: ReviewRow[];
  onAction: (id: string, action: "file" | "vault" | "edit") => void;
}) {
  return (
    <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.813rem", marginTop: "1rem" }}>
      <thead>
        <tr>
          <th style={th}>Contact file name</th>
          <th style={th}>Vault name</th>
          <th style={th}>Handle</th>
          <th style={th}>Action</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.id}>
            <td style={td}>{r.fileName}</td>
            <td style={td}>{r.vaultName ?? "—"}</td>
            <td style={td}>{r.handle ?? "(none)"}</td>
            <td style={td}>
              <div style={{ display: "flex", gap: "0.375rem" }}>
                <button
                  disabled={r.action === "file"}
                  onClick={() => onAction(r.id, "file")}
                  style={actionStyle(r.action === "file", r.action === "file")}
                >
                  Use file
                </button>
                <button
                  disabled={r.action === "vault"}
                  onClick={() => onAction(r.id, "vault")}
                  style={actionStyle(r.action === "vault", r.action === "vault")}
                >
                  Use vault
                </button>
                <button
                  disabled={r.action === "edit"}
                  onClick={() => onAction(r.id, "edit")}
                  style={actionStyle(r.action === "edit", r.action === "edit")}
                >
                  Edit
                </button>
              </div>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

Note: the file uses `CSSProperties` from the type import at the top — there is no `React` default import.

### Step 4: Wire into `ImportScreen.tsx`

Add imports:

```typescript
import { invokeContactsInfo, type ContactCardInfo } from "../lib/tauri";
import ContactReviewTable, { type ReviewRow } from "../components/ContactReviewTable";
```

Add state:

```typescript
  const [reviewRows, setReviewRows] = useState<ReviewRow[] | null>(null);
  const [reviewing, setReviewing] = useState(false);
```

Add the review handler and matching logic:

```typescript
  const reviewContacts = async () => {
    if (!contactsPath) return;
    setReviewing(true);
    try {
      const info = await invokeContactsInfo(contactsPath);
      const vaultRes = await apiClient.get<{ contacts: { id: string; name: string; handle_count: number }[] }>(
        "/v1/export/contacts",
      ).catch(() => ({ contacts: [] }));
      const vaultByHandle = new Map<string, string>();
      for (const c of vaultRes.contacts) vaultByHandle.set(c.name.toLowerCase(), c.name);

      const rows: ReviewRow[] = info.cards.map((card: ContactCardInfo, i: number) => {
        const match = card.handle
          ? vaultRes.contacts.find(
              (vc) => vc.name.toLowerCase() === card.handle!.toLowerCase(),
            )
          : undefined;
        return {
          id: String(i),
          fileName: card.name,
          vaultName: match ? match.name : null,
          handle: card.handle,
          action: null as "file" | "vault" | "edit" | null,
        };
      });
      setReviewRows(rows);
    } catch {
      setReviewRows([]);
    } finally {
      setReviewing(false);
    }
  };
```

`apiClient` needs importing: `import { apiClient } from "../lib/api";`.

Render the review between the contacts picker and the Import button (inside the `!running && !done` block, after the contacts `FormRow`):

```typescript
          <div style={{ marginTop: "0.5rem" }}>
            <button
              onClick={reviewContacts}
              disabled={!contactsPath || reviewing}
              style={{ fontSize: "0.813rem", padding: "0.375rem 0.75rem" }}
            >
              {reviewing ? "Reviewing…" : "Review contacts"}
            </button>
          </div>
          {reviewRows && (
            <ContactReviewTable
              rows={reviewRows}
              onAction={(id, action) =>
                setReviewRows((rows) =>
                  rows ? rows.map((r) => (r.id === id ? { ...r, action } : r)) : rows,
                )
              }
            />
          )}
```

Decisions are kept in `reviewRows` state for the import pipeline (Tier 1's real push) to consume.

### Step 5: Build both sides and commit

```bash
cd /home/mbeisser/repo/message-vault-io/src-tauri && cargo check
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add src-tauri/src/commands/contacts.rs web/src/lib/tauri.ts web/src/components/ContactReviewTable.tsx web/src/screens/ImportScreen.tsx
git commit -m "feat(import): contacts conflict review with side-by-side table

contacts_info now returns every parsed card (name + first phone/email)
instead of only a 10-name preview. ImportScreen gains a Review contacts
button that matches file cards against vault contacts by handle and
renders Use file / Use vault / Edit actions; decisions are held in
state for the import pipeline.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

# Task 13 — Frontend: Settings storage + account sections

**Goal:** Spec Settings sections: **Storage** — conversation count, message count, attachment storage used (from `GET /v1/export/messages/count?q=` with no query — includes `conversations` from Task 4); **Account** — change password and delete account (with confirmation), consuming Task 3's endpoints.

**File:** `web/src/screens/SettingsScreen.tsx`

### Step 1: Add state, effects, and handlers

Add imports:

```typescript
import { apiClient } from "../lib/api";
import { useAuth } from "../lib/auth";
```

Add state and handlers inside the component:

```typescript
  const { logout } = useAuth();
  const [storage, setStorage] = useState<{
    conversations: number; messages: number; attachments: number; total_bytes: number;
  } | null>(null);
  const [pwCurrent, setPwCurrent] = useState("");
  const [pwNew, setPwNew] = useState("");
  const [pwConfirm, setPwConfirm] = useState("");
  const [pwMsg, setPwMsg] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    apiClient
      .get<{ ok: boolean; conversations: number; messages: number; attachments: number; total_bytes: number }>(
        "/v1/export/messages/count?q=",
      )
      .then((res) => setStorage({
        conversations: res.conversations,
        messages: res.messages,
        attachments: res.attachments,
        total_bytes: res.total_bytes,
      }))
      .catch(() => setStorage(null));
  }, []);

  const formatBytes = (n: number): string => {
    if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
    if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
    if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
    return `${n} B`;
  };

  const changePassword = async () => {
    setPwMsg(null);
    if (pwNew.length < 8) {
      setPwMsg({ ok: false, text: "New password must be at least 8 characters." });
      return;
    }
    if (pwNew !== pwConfirm) {
      setPwMsg({ ok: false, text: "Passwords do not match." });
      return;
    }
    try {
      await apiClient.post("/v1/auth/change-password", {
        current_password: pwCurrent,
        new_password: pwNew,
      });
      setPwCurrent(""); setPwNew(""); setPwConfirm("");
      setPwMsg({ ok: true, text: "Password updated." });
    } catch (e) {
      setPwMsg({ ok: false, text: String(e) });
    }
  };

  const deleteAccount = async () => {
    if (!window.confirm("Delete this account and ALL vault data? This cannot be undone.")) return;
    try {
      await apiClient.post("/v1/auth/delete-account", { confirm: true });
      logout();
    } catch (e) {
      window.alert(`Delete failed: ${e}`);
    }
  };
```

### Step 2: Add the Storage section

Insert between the Appearance section and the Save button row:

```typescript
      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Storage</h3>
      {storage ? (
        <div style={{ display: "flex", gap: "1.5rem", fontSize: "0.875rem" }}>
          <div>
            <div style={{ fontWeight: 600 }}>{storage.conversations.toLocaleString()}</div>
            <div style={{ fontSize: "0.75rem", color: "#9ca3af" }}>Conversations</div>
          </div>
          <div>
            <div style={{ fontWeight: 600 }}>{storage.messages.toLocaleString()}</div>
            <div style={{ fontSize: "0.75rem", color: "#9ca3af" }}>Messages</div>
          </div>
          <div>
            <div style={{ fontWeight: 600 }}>{storage.attachments.toLocaleString()}</div>
            <div style={{ fontSize: "0.75rem", color: "#9ca3af" }}>Attachments</div>
          </div>
          <div>
            <div style={{ fontWeight: 600 }}>{formatBytes(storage.total_bytes)}</div>
            <div style={{ fontSize: "0.75rem", color: "#9ca3af" }}>Attachment storage</div>
          </div>
        </div>
      ) : (
        <div style={{ fontSize: "0.813rem", color: "#9ca3af" }}>
          Storage stats unavailable — check the vault connection.
        </div>
      )}

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Account</h3>
      <FormRow label="Current password">
        <input type="password" value={pwCurrent}
          onChange={(e) => setPwCurrent(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>
      <FormRow label="New password">
        <input type="password" value={pwNew}
          onChange={(e) => setPwNew(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>
      <FormRow label="Confirm new password">
        <input type="password" value={pwConfirm}
          onChange={(e) => setPwConfirm(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>
      <div style={{ marginTop: "0.5rem" }}>
        <button onClick={changePassword} style={{ padding: "0.375rem 1rem", fontSize: "0.875rem" }}>
          Change password
        </button>
        {pwMsg && (
          <span style={{
            marginLeft: "0.75rem", fontSize: "0.813rem",
            color: pwMsg.ok ? "#16a34a" : "#dc2626",
          }}>
            {pwMsg.text}
          </span>
        )}
      </div>
      <div style={{ marginTop: "1.5rem", borderTop: "1px solid #e5e7eb", paddingTop: "1rem" }}>
        <button
          onClick={deleteAccount}
          style={{ padding: "0.5rem 1rem", fontSize: "0.875rem", background: "#dc2626", color: "#fff", border: "none", borderRadius: "4px", cursor: "pointer" }}
        >
          Delete account
        </button>
        <span style={{ marginLeft: "0.75rem", fontSize: "0.75rem", color: "#9ca3af" }}>
          Permanently deletes the account and all vault data.
        </span>
      </div>
```

### Step 3: Build and commit

```bash
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-io
git add web/src/screens/SettingsScreen.tsx
git commit -m "feat(web): Settings storage stats and account section

Storage section shows conversations/messages/attachments/total bytes
from /v1/export/messages/count. Account section adds change-password
(current + new + confirm) and delete-account with confirmation,
logging out after a successful delete.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Definition of done

- All 13 tasks merged (each committed, build green).
- `MessageView` shows real totals, scoped to the selected conversation; find bar highlights and navigates matches on the visible page.
- Participant chips open the contact drawer; `ContactList`/`ContactDrawer` load real data.
- Global search autocompletes operators and contact names; advanced form composes valid backend queries on both tabs.
- Conversation checkboxes + export popover scope selection drive `ExportScreen`.
- Import history screen lists past imports; conflict review table appears when a contacts file is picked.
- Settings shows storage stats and supports change-password / delete-account.
- No spec-only placeholders remain in the Tier 2 feature set.
