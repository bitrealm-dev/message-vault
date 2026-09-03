# An Import Names the Contact Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a backup knows a person's name, the import puts that name on the
Contact, and one query in one module decides the name shown for a participant
everywhere.

**Architecture:** A new `db/participant_names` module owns the single naming
query and the single `Participant` type that both the conversation list and
Export return. `ensure_contact_for_handle` takes the backup's name and puts it
on a Contact that has none. The three mechanisms that existed because imported
Contacts were nameless — `contact_handles.name_alias`, `ContactNameMode`, and
the web's "Use name aliases" toggle — are deleted, and the address-book load
adopts an imported Contact instead of creating a nameless duplicate beside it.

**Tech Stack:** Rust (Axum, sqlx over SQLite and Postgres, utoipa), TypeScript
(React 19, TanStack Query, Vitest, Biome).

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`,
section "Names (ADR-0006)". The binding decision is
`docs/adr/0006-an-import-names-the-contact.md`. This is pull request 3 of the
eight in `docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`.

## Global Constraints

- **The naming rule, verbatim from the spec.** One query, in one module:
  `name = COALESCE(NULLIF(trim(c.preferred_name), ''), NULLIF(trim(p.name_alias), ''), h.raw)`
  joined as
  `participants p LEFT JOIN contact_handles ch ON ch.handle_id = p.handle_id LEFT JOIN contacts c ON c.id = ch.contact_id`.
  `p.contact_id` is **not** consulted for naming.
- **ADR-0006.** A Contact created by an import carries the backup's name.
  An existing nameless Contact whose `origin` is `import` gets the name. A
  Contact that has any name is untouched — first backup wins. A name the person
  types, or loads from an address book, replaces an imported name.
- **ADR-0005.** Every route answers in the one shape. After **every** server
  change regenerate both generated artifacts:
  ```bash
  cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
  (cd web && npm run gen:api)
  ```
  `crates/vault/server/src/openapi.rs` has a test that fails on drift, and
  `scripts/check-generated-api-types.sh` fails when
  `web/src/lib/vaultApi.types.ts` does not match the document.
- **ADR-0002.** One way to fetch data in `web/`: TanStack Query over the route
  functions in `web/src/lib/vaultApi.ts`. Do not add a cache, a change event, or
  a fetching hook.
- **Export is the download button, never the path a screen reads by.** This
  plan does not add a screen that reads through `/v1/export/*`. Moving the
  message screen off Export is pull request 4, not this one.
- **A schema change means bumping `SCHEMA_VERSION`.** Editing any file under
  `schema/sql/` requires incrementing `SCHEMA_VERSION` in
  `crates/vault/server/src/db/schema.rs`. Every vault is then rebuilt empty and
  re-imported; that is the intended behaviour, not a problem to design around.
- **Every column in `schema/sql/*.sql` carries a comment.**
  `scripts/check-sql-column-comments.mjs` enforces it.
- **Verification.** `./scripts/check-pr.sh` passes on the head commit before the
  pull request opens.

## File Structure

**Created**

- `crates/vault/server/src/db/participant_names.rs` — the one naming query and
  the one `Participant` type both read routes return.

**Modified — server**

- `crates/vault/server/src/db/mod.rs` — register the new module.
- `crates/vault/server/src/conversations_api.rs` — delete
  `ConversationParticipant` and its `load_participants`; return `Participant`.
- `crates/vault/server/src/export_api.rs` — delete `ExportParticipant` and its
  `load_participants`; return `Participant`.
- `crates/vault/server/src/import/contact_name.rs` — `ensure_contact_for_handle`
  takes the backup name; `ContactNameMode`, `apply_contact_name_mode`,
  `seed_contact_handle_alias`, and `contact_preferred_name` are deleted.
- `crates/vault/server/src/import/staging.rs` — the participant loop passes the
  backup name and drops the name-mode merge.
- `crates/vault/server/src/import/mod.rs` — `contact_name_mode` leaves
  `ImportOptions`, the query struct, and the OpenAPI parameter list.
- `crates/vault/server/src/import_cli.rs` — drops the `contact_name_mode` field.
- `crates/vault/server/src/contacts_api.rs` — `ContactHandleInfo.name_alias` and
  its query column go.
- `crates/vault/server/src/db/contacts.rs` — the address-book load adopts an
  imported Contact that already owns one of the card's phones.
- `crates/vault/server/src/db/schema.rs` — `SCHEMA_VERSION` bumped.
- `schema/sql/contacts.sql` — `contact_handles.name_alias` removed.

**Modified — clients**

- `crates/libs/vault-push/src/http.rs`, `crates/libs/vault-push/src/run.rs`,
  `src-tauri/src/commands/push.rs` — the `contact_name_mode` parameter goes.

**Modified — web**

- `web/src/components/contactDrawer/` — `ContactDrawerHandles.tsx`,
  `HandleTableRow.tsx`, `handleTableLogic.tsx`, `contactDrawerTypes.ts` lose the
  Alias column.
- `web/src/components/ConversationRow.tsx`,
  `web/src/components/messages/chatBubbleShared.tsx`, `SmsBubble.tsx`,
  `ImessageBubble.tsx`, `web/src/screens/MessageView.tsx` — read `p.name`.
- `web/src/screens/settings/AppearanceSection.tsx` — the toggle goes.

**Deleted — web**

- `web/src/lib/nameAliases.ts`, `web/src/lib/useNameAliases.ts` and their tests.

**Regenerated**

- `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts`.

---

### Task 1: The one naming query

**Files:**
- Create: `crates/vault/server/src/db/participant_names.rs`
- Modify: `crates/vault/server/src/db/mod.rs`

**Interfaces:**
- Consumes: `crate::db::sql::group_rows_by_id` (already exists; chunks the
  `IN (…)` list and groups rows by the first column).
- Produces: `crate::db::participant_names::Participant` with public fields
  `name: String`, `handle: String`, `service: String`,
  `contact_id: Option<i64>`; and
  `pub async fn load_for_conversations(conn: &mut AnyConnection, conversation_ids: &[i64]) -> Result<HashMap<i64, Vec<Participant>>, sqlx::Error>`.
  Tasks 2 and 3 depend on both names exactly as written.

`name` is a `String`, not an `Option<String>`: the `COALESCE` ends in `h.raw`,
which the `JOIN handles` guarantees is present, so there is no case where the
query has nothing to show. `contact_id` comes from `contact_handles`, never from
`participants.contact_id` — ADR-0006 says a handle counts as a Contact's the
moment it is on the Contact.

- [ ] **Step 1: Write the failing test**

Create `crates/vault/server/src/db/participant_names.rs` containing only the
test module for now, so the test names the API before it exists:

```rust
//! The one query that decides the name shown for a participant.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    /// Insert an account, one conversation, and one participant on `handle`
    /// whose backup name is `name_alias`. Returns (conversation_id, handle_id).
    async fn seed(
        conn: &mut sqlx::AnyConnection,
        handle: &str,
        name_alias: Option<&str>,
    ) -> (i64, i64) {
        schema::ensure_vault_schema(conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, $2, $2, 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let conversation_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversations
                 (account_id, chat_handle_id, conversation_type, source_file)
             VALUES ($1, $2, 'individual', 'c.jsonl') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES ($1, $2, $3)",
        )
        .bind(conversation_id)
        .bind(handle_id)
        .bind(name_alias)
        .execute(&mut *conn)
        .await
        .unwrap();
        (conversation_id, handle_id)
    }

    async fn link(conn: &mut sqlx::AnyConnection, handle_id: i64, preferred_name: &str) -> i64 {
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(preferred_name)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        contact_id
    }

    #[tokio::test]
    async fn contact_name_wins_over_the_backup_name() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550100", Some("Bobby")).await;
        let contact_id = link(&mut conn, handle_id, "Robert Smith").await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "Robert Smith");
        assert_eq!(p.handle, "+15555550100");
        assert_eq!(p.service, "imessage");
        assert_eq!(p.contact_id, Some(contact_id));
    }

    #[tokio::test]
    async fn backup_name_shows_when_the_contact_has_none() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550200", Some("Bobby")).await;
        link(&mut conn, handle_id, "   ").await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        assert_eq!(loaded[&conversation_id][0].name, "Bobby");
    }

    #[tokio::test]
    async fn the_handle_shows_when_nothing_names_the_person() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, _handle_id) = seed(&mut conn, "+15555550300", None).await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "+15555550300");
        assert_eq!(p.contact_id, None);
    }

    /// `participants.contact_id` is not consulted: only the link in
    /// `contact_handles` names someone, so naming a Contact renames them in
    /// every conversation at once.
    #[tokio::test]
    async fn a_participant_contact_id_does_not_name_anyone() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550400", Some("Bobby")).await;
        let stranger: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Wrong') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("UPDATE participants SET contact_id = $1 WHERE handle_id = $2")
            .bind(stranger)
            .bind(handle_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "Bobby");
        assert_eq!(p.contact_id, None);
    }
}
```

Register the module by adding one line to
`crates/vault/server/src/db/mod.rs`, in alphabetical order between
`pub mod handles;` and `pub mod permissions;`:

```rust
pub mod participant_names;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p message-vault-server participant_names`
Expected: FAIL — `cannot find function 'load_for_conversations' in this scope`
and `cannot find type 'Participant'`.

- [ ] **Step 3: Write the module**

Put this above the `#[cfg(test)] mod tests` block in the same file:

```rust
//! The one query that decides the name shown for a participant.
//!
//! ADR-0006: the Contact's name, else what that backup called them in that
//! conversation, else the handle. Every route that names a participant calls
//! [`load_for_conversations`], so one person cannot show two names on one
//! screen. `participants.contact_id` is deliberately not consulted — a handle
//! counts as a Contact's the moment it is on the Contact, which is what makes
//! naming someone rename them everywhere at once.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::{AnyConnection, Row};

use crate::db::sql::group_rows_by_id;

/// One participant of a conversation, named by the rule above.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Participant {
    /// What to show for this person. Never empty: the rule ends at the handle.
    pub name: String,
    /// Raw handle value (phone, email, or username).
    pub handle: String,
    /// Platform service, e.g. `imessage`.
    pub service: String,
    /// Linked vault contact id, when the handle is on a Contact. Matches the
    /// `id` every other contact shape uses, so a caller can compare the two
    /// without converting either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<i64>,
}

/// Participants of each conversation in `conversation_ids`, ordered by
/// participant id within a conversation.
///
/// # Errors
///
/// Returns a database error when the query fails.
pub async fn load_for_conversations(
    conn: &mut AnyConnection,
    conversation_ids: &[i64],
) -> Result<HashMap<i64, Vec<Participant>>, sqlx::Error> {
    group_rows_by_id(
        conn,
        conversation_ids,
        |placeholders| {
            format!(
                "SELECT p.conversation_id,
                        COALESCE(NULLIF(trim(c.preferred_name), ''),
                                 NULLIF(trim(p.name_alias), ''),
                                 h.raw) AS name,
                        h.raw AS handle,
                        COALESCE(NULLIF(trim(h.service), ''), h.handle_type) AS service,
                        ch.contact_id
                 FROM participants p
                 JOIN handles h ON h.id = p.handle_id
                 JOIN conversations conv ON conv.id = p.conversation_id
                 LEFT JOIN contact_handles ch
                   ON ch.handle_id = p.handle_id AND ch.account_id = conv.account_id
                 LEFT JOIN contacts c
                   ON c.id = ch.contact_id AND c.account_id = conv.account_id
                 WHERE p.conversation_id IN ({placeholders})
                 ORDER BY p.conversation_id, p.id"
            )
        },
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                Participant {
                    name: row.try_get(1)?,
                    handle: row.try_get(2)?,
                    service: row
                        .try_get::<String, _>(3)
                        .unwrap_or_else(|_| "unknown".into()),
                    contact_id: row.try_get(4)?,
                },
            ))
        },
    )
    .await
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p message-vault-server participant_names`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/db/participant_names.rs crates/vault/server/src/db/mod.rs
git commit -m "feat(vault-server): one query decides a participant's name"
```

---

### Task 2: Both read routes return the one participant type

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs` (delete
  `ConversationParticipant` around line 100 and `load_participants` around
  line 332)
- Modify: `crates/vault/server/src/export_api.rs` (delete `ExportParticipant`
  around line 118 and `load_participants` around line 433)
- Modify: `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts`
  (regenerated, not hand-edited)
- Modify: `web/src/lib/types.ts`

**Interfaces:**
- Consumes: `crate::db::participant_names::{Participant, load_for_conversations}`
  from Task 1.
- Produces: the OpenAPI schema name `Participant` replaces both
  `ConversationParticipant` and `ExportParticipant`. Tasks 7 and 8 rely on
  `web/src/lib/types.ts` exporting `Participant` and `MessageParticipant` as
  that one generated type, whose fields are `name: string`,
  `handle: string`, `service: string`, `contact_id?: number`.

The Export participant loses `handle_type` and `preferred_name`. `service`
replaces `handle_type`, which is what the conversation list already returned and
what the web already renders; `preferred_name` is gone because `name` is now the
answer rather than one of three ingredients the caller had to combine.

- [ ] **Step 1: Delete the two duplicate types and loaders**

In `crates/vault/server/src/conversations_api.rs`, delete the whole
`ConversationParticipant` struct (its doc comment through its closing brace) and
the whole `load_participants` function. Add the import:

```rust
use crate::db::participant_names::{Participant, load_for_conversations};
```

Replace every `ConversationParticipant` in the file with `Participant`,
including the `participants: Vec<ConversationParticipant>` field on
`ConversationSummary` and the `name_alias: None` literal near line 319 (that
literal is part of a struct expression that now has no `name_alias` field —
delete the line). Replace the `load_participants(conn, &ids).await?` call with:

```rust
    let participants = load_for_conversations(conn, &ids).await?;
```

In `crates/vault/server/src/export_api.rs`, delete the `ExportParticipant`
struct and its `load_participants` function, add the same import, change
`ExportConversation.participants` to `Vec<Participant>`, and call
`load_for_conversations` in place of the deleted loader.

utoipa collects nested schemas from the handler annotations rather than from an
explicit list — `crates/vault/server/src/openapi.rs` registers only
`crate::search::ListKind` — so deriving `ToSchema` on `Participant` (Task 1) is
all the registration there is. Nothing in `openapi.rs` needs editing; Step 5
proves it by checking the regenerated document.

- [ ] **Step 2: Build and fix the call sites the compiler names**

Run: `cargo build -p message-vault-server`
Expected: errors listing every remaining use of the deleted types and of the
removed `name_alias` / `preferred_name` / `handle_type` fields. Fix each by
reading `p.name` instead. Repeat until the build is clean.

- [ ] **Step 3: Fix the server tests**

Run: `cargo test -p message-vault-server 2>&1 | tail -40`

The conversation-list tests around `conversations_api.rs:1220-1360` assert on
`p.name_alias`. The rule they were written for is gone, so rewrite them as
assertions on `p.name`. Replace the four tests named
`list_conversations_*_alias*`, `*keeps_residue*`, and
`list_conversations_matches_contact_preferred_name` with these three:

```rust
    #[tokio::test]
    async fn list_conversations_shows_the_contact_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam Preferred')
             RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "SELECT id FROM handles WHERE account_id = $1 AND raw = '+15555550200'",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = list_conversations_page(&mut conn, &account).await;
        let p = find_participant(&page, "+15555550200");
        assert_eq!(p.name, "Sam Preferred");
        assert_eq!(p.contact_id, Some(contact_id));
    }

    #[tokio::test]
    async fn list_conversations_falls_back_to_the_backup_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // setup() records the backup name 'Sam' on +15555550200 and links no
        // contact, so the backup's name is what there is to show.
        let page = list_conversations_page(&mut conn, &account).await;
        let p = find_participant(&page, "+15555550200");
        assert_eq!(p.name, "Sam");
        assert_eq!(p.contact_id, None);
    }

    #[tokio::test]
    async fn list_conversations_falls_back_to_the_handle() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE participants SET name_alias = NULL")
            .execute(&mut *conn)
            .await
            .unwrap();
        let page = list_conversations_page(&mut conn, &account).await;
        let p = find_participant(&page, "+15555550200");
        assert_eq!(p.name, "+15555550200");
    }
```

Those three use two helpers. Add them to the same `mod tests`, adapting
`list_conversations_page` to whatever the file's existing tests already call to
fetch a page (reuse that call rather than inventing a second one):

```rust
    fn find_participant<'a>(
        page: &'a crate::paging::Page<ConversationSummary>,
        handle: &str,
    ) -> &'a Participant {
        page.items
            .iter()
            .flat_map(|c| c.participants.iter())
            .find(|p| p.handle == handle)
            .expect("participant is in the page")
    }
```

- [ ] **Step 4: Run the server tests**

Run: `cargo test -p message-vault-server`
Expected: PASS.

- [ ] **Step 5: Regenerate the API document and the web types**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
```

Then confirm the document has one participant schema and not two:

```bash
grep -c '"ConversationParticipant"\|"ExportParticipant"' docs/src/assets/openapi.json
```
Expected: `0`.

- [ ] **Step 6: Point the web type aliases at the one schema**

In `web/src/lib/types.ts`, lines 13 and 19:

```ts
export type Participant = Schema["Participant"];

/** One participant on a message the Export routes return. */
export type MessageParticipant = Schema["Participant"];
```

- [ ] **Step 7: Verify the web still type-checks**

Run: `cd web && npm run build`
Expected: type errors in the components that read `p.name_alias`,
`p.preferred_name`, and `p.handle_type`. Leave them failing — Task 8 fixes
them. Record the list; do not patch them here.

Run instead, to confirm the server side is complete:
`cargo test -p message-vault-server && ./scripts/check-generated-api-types.sh`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server/src conversations_api.rs export_api.rs \
  docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts web/src/lib/types.ts
git commit -m "refactor(api): the conversation list and Export return one participant type"
```

---

### Task 3: An import names the Contact

**Files:**
- Modify: `crates/vault/server/src/import/contact_name.rs:52` and its tests
- Modify: `crates/vault/server/src/import/staging.rs:501,545-556`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `ensure_contact_for_handle(tx, account_id, handle_id, backup_name: Option<&str>, stats)`.
  The extra `backup_name` parameter sits before `stats`. Task 6 relies on the
  behaviour this task establishes: a Contact an import created has
  `origin = 'import'` and may carry a name.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/vault/server/src/import/contact_name.rs`:

```rust
    #[tokio::test]
    async fn an_import_creates_the_contact_with_the_backup_name() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550700', '+15555550700', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        let contact_id =
            ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("Ada"), &mut stats)
                .await
                .unwrap();

        let (name, origin): (String, String) = sqlx::query_as(
            "SELECT preferred_name, origin FROM contacts WHERE id = $1",
        )
        .bind(contact_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(name, "Ada");
        assert_eq!(origin, "import");
    }

    #[tokio::test]
    async fn a_later_backup_names_a_contact_an_earlier_one_left_nameless() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550800', '+15555550800', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        let first =
            ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, None, &mut stats)
                .await
                .unwrap();
        let second =
            ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("Ada"), &mut stats)
                .await
                .unwrap();
        assert_eq!(first, second, "the same handle keeps the same contact");

        let name: String =
            sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
                .bind(first)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(name, "Ada");
    }

    #[tokio::test]
    async fn a_second_spelling_does_not_rename_anyone() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550900', '+15555550900', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        let contact_id = ensure_contact_for_handle(
            &mut conn,
            TEST_ACCOUNT,
            handle_id,
            Some("Ada Lovelace"),
            &mut stats,
        )
        .await
        .unwrap();
        ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("ada l"), &mut stats)
            .await
            .unwrap();

        let name: String =
            sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
                .bind(contact_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(name, "Ada Lovelace", "first backup wins");
    }

    /// A name the person typed carries `origin = 'user'` and outranks any
    /// backup, however many imports later run.
    #[tokio::test]
    async fn an_import_does_not_overwrite_a_name_the_person_typed() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555551000', '+15555551000', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let contact_id = crate::db::contacts::create_contact(
            &mut conn,
            TEST_ACCOUNT,
            "",
            crate::db::contacts::Origin::User,
        )
        .await
        .unwrap();
        crate::db::contacts::link_handle_to_contact(
            &mut conn,
            TEST_ACCOUNT,
            handle_id,
            contact_id,
            crate::db::contacts::Origin::User,
        )
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("Ada"), &mut stats)
            .await
            .unwrap();

        let name: String =
            sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
                .bind(contact_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(name, "", "the person's contact is not the import's to name");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server contact_name`
Expected: FAIL — `this function takes 4 arguments but 5 arguments were supplied`.

- [ ] **Step 3: Change `ensure_contact_for_handle`**

Replace the function and its doc comment in
`crates/vault/server/src/import/contact_name.rs` with:

```rust
/// The contact that owns `handle_id`, creating one when nothing owns it yet.
///
/// Every participant an import meets becomes a contact. ADR-0006: a backup is
/// an address book the person already curated, so the name it supplies goes on
/// the contact — on creation, or later if an earlier backup left the contact
/// nameless. A contact that already has a name is untouched, because the same
/// number arrives spelled differently across backups and the first spelling is
/// as good as the second. A contact the person made or an address book loaded
/// is never renamed by an import.
pub(super) async fn ensure_contact_for_handle(
    tx: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
    backup_name: Option<&str>,
    stats: &mut ImportStats,
) -> Result<i64> {
    let name = nonempty_str(backup_name).unwrap_or("");
    if let Some(existing) = ensure_sibling_contact_link(tx, account_id, handle_id).await? {
        if !name.is_empty() {
            name_nameless_import_contact(tx, account_id, existing, name).await?;
        }
        return Ok(existing);
    }
    let contact_id =
        contacts::create_contact(tx, account_id, name, contacts::Origin::Import).await?;
    contacts::link_handle_to_contact(
        tx,
        account_id,
        handle_id,
        contact_id,
        contacts::Origin::Import,
    )
    .await?;
    stats.contacts_created += 1;
    Ok(contact_id)
}

/// Put the backup's name on a contact an earlier import left nameless.
///
/// The `origin = 'import'` clause is what keeps a typed or address-book name
/// safe, and the empty-name clause is what makes the first backup win.
async fn name_nameless_import_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
    name: &str,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE contacts
         SET preferred_name = $1
         WHERE account_id = $2 AND id = $3
           AND origin = 'import'
           AND trim(preferred_name) = ''",
    )
    .bind(name)
    .bind(account_id)
    .bind(contact_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if updated > 0 {
        contacts::touch_contact(conn, account_id, contact_id).await?;
    }
    Ok(())
}
```

- [ ] **Step 4: Update the two call sites**

In `crates/vault/server/src/import/staging.rs` around line 501, the chat handle
has no name of its own at that point — the participant loop below carries the
names — so pass `None`:

```rust
    if !cached {
        let _ = ensure_contact_for_handle(tx, &stmts.account_id, chat_handle_id, None, &mut stats)
            .await?;
    }
```

Replace lines 545–556 (from `let contact_id =` through the `INSERT_PARTICIPANT`
execute) with:

```rust
        let backup_name = nonempty_str(name_alias.as_deref()).map(str::to_string);
        let contact_id = ensure_contact_for_handle(
            tx,
            &stmts.account_id,
            handle_id,
            backup_name.as_deref(),
            &mut stats,
        )
        .await?;
        // `participants.name_alias` keeps what this backup called them in this
        // conversation. It is the second clause of the naming rule, never the
        // first.
        sqlx::query(INSERT_PARTICIPANT)
            .bind(conversation_id)
            .bind(handle_id)
            .bind(Some(contact_id))
            .bind(backup_name)
            .execute(&mut *tx)
            .await?;
        stats.participants += 1;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p message-vault-server contact_name`
Expected: PASS.

Run: `cargo test -p message-vault-server`
Expected: the `contact_name_mode_*` tests in `import/mod.rs` fail, because the
merge they exercise no longer runs. Task 4 deletes them. Everything else passes.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/import/contact_name.rs crates/vault/server/src/import/staging.rs
git commit -m "feat(import): a backup's name goes on the Contact"
```

---

### Task 4: `contact_name_mode` is deleted

**Files:**
- Modify: `crates/vault/server/src/import/contact_name.rs` (delete
  `ContactNameMode`, `apply_contact_name_mode`, `contact_preferred_name`, and
  their tests)
- Modify: `crates/vault/server/src/import/mod.rs:40,116,159,558-563,1400,1589,1664,2466,2585,2645,2712-2930`
- Modify: `crates/vault/server/src/import_cli.rs:174`
- Modify: `crates/libs/vault-push/src/http.rs:70,426,439`
- Modify: `crates/libs/vault-push/src/run.rs:135,2566,2596,2615,2630`
- Modify: `src-tauri/src/commands/push.rs:94,117-118,153`
- Modify: `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts`

**Interfaces:**
- Consumes: Task 3's `ensure_contact_for_handle`, which is now the only thing
  that decides an imported name.
- Produces: `POST /v1/import` no longer accepts a `contact_name_mode` query
  parameter. Nothing later in this plan depends on it.

The one rule from ADR-0006 replaces all three modes, so the parameter has
nothing left to select. It goes from the server, from `vault-push`, and from the
Tauri push command in the same commit — a parameter the server rejects is worse
than one that is gone.

- [ ] **Step 1: Delete the server side**

In `crates/vault/server/src/import/contact_name.rs`, delete the
`ContactNameMode` enum with its `impl`, the `apply_contact_name_mode` function,
the `contact_preferred_name` function, and the `apply_contact_name_mode_unit`
test. Drop the now-unused `anyhow::bail` from the `use` line if the compiler
says so.

In `crates/vault/server/src/import/mod.rs`:
- delete `pub use contact_name::ContactNameMode;` (line 40)
- delete the `pub contact_name_mode: ContactNameMode,` field from
  `ImportOptions` (line 116) and its initializer (line 159)
- delete the `#[serde(default = "default_contact_name_mode")] contact_name_mode: String,`
  field and the `default_contact_name_mode` function (lines 558–563)
- delete the `("contact_name_mode" = Option<String>, Query)` entry from the
  utoipa `params(...)` list (line 1400)
- delete the `let contact_name_mode = …parse…` binding (line 1589) and the
  `opts.contact_name_mode = contact_name_mode;` assignment (line 1664)
- delete the four `contact_name_mode_*` tests (lines 2712–2930) and the
  `contact_name_mode` fields in the test fixtures at lines 2466, 2585, 2645

In `crates/vault/server/src/import_cli.rs`, delete line 174
(`contact_name_mode: import::ContactNameMode::default(),`).

- [ ] **Step 2: Delete the client side**

In `crates/libs/vault-push/src/http.rs`, delete the
`pub contact_name_mode: &'a str,` field (line 70), the destructuring at line
426, and the `("contact_name_mode", contact_name_mode.to_string()),` query pair
(line 439).

In `crates/libs/vault-push/src/run.rs`, delete `pub contact_name_mode: String,`
(line 135), and the three places that thread it through (lines 2566, 2596,
2615, 2630).

In `src-tauri/src/commands/push.rs`, delete
`pub contact_name_mode: Option<String>,` (line 94), the
`let contact_name_mode = args.contact_name_mode…` binding (lines 117–118), and
the field at line 153.

- [ ] **Step 3: Build everything and fix what the compiler names**

```bash
cargo build --workspace
cargo build --manifest-path src-tauri/Cargo.toml
```
Expected: clean. Fix any remaining reference the compiler reports.

Confirm nothing is left:

```bash
grep -rn 'contact_name_mode\|ContactNameMode' crates/ src-tauri/src web/src docs/
```
Expected: no output.

- [ ] **Step 4: Run the tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Regenerate the API document and the web types**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
./scripts/check-generated-api-types.sh
```
Expected: the script passes and the diff on `openapi.json` shows the parameter
removed from `POST /v1/import`.

- [ ] **Step 6: Commit**

```bash
git add crates/ src-tauri/src/commands/push.rs docs/src/assets/openapi.json \
  web/src/lib/vaultApi.types.ts
git commit -m "refactor(import): delete contact_name_mode; one naming rule replaces three modes"
```

---

### Task 5: `contact_handles.name_alias` is deleted

**Files:**
- Modify: `schema/sql/contacts.sql:58`
- Modify: `crates/vault/server/src/db/schema.rs:46`
- Modify: `crates/vault/server/src/import/contact_name.rs` (delete
  `seed_contact_handle_alias` and `seed_contact_handle_alias_unit_first_wins`)
- Modify: `crates/vault/server/src/import/staging.rs:24-25,549`
- Modify: `crates/vault/server/src/contacts_api.rs:52,372,391,406,416`
- Modify: `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts`

**Interfaces:**
- Consumes: Task 2 (Export no longer reads `ch.name_alias`) and Task 4.
- Produces: `ContactHandleInfo` without `name_alias`. Task 7 removes the column
  the web drew from it.

`contact_handles.name_alias` existed to hold the backup's name where nothing
else could. The Contact now holds it, so the column is a second answer to a
question that has one. Dropping a column is a schema change, which means a
`SCHEMA_VERSION` bump and a rebuild of every vault — the intended behaviour
before a stable release.

- [ ] **Step 1: Remove the column from the schema**

In `schema/sql/contacts.sql`, delete these two lines from the
`contact_handles` table:

```sql
    -- Name the source gave for this handle (may differ from preferred_name).
    name_alias TEXT,
```

In `crates/vault/server/src/db/schema.rs`, bump the constant:

```rust
pub const SCHEMA_VERSION: i64 = 8;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server 2>&1 | tail -30`
Expected: FAIL — `no such column: ch.name_alias` from the contact-drawer query
and from `seed_contact_handle_alias`.

- [ ] **Step 3: Delete the writer**

In `crates/vault/server/src/import/contact_name.rs`, delete
`seed_contact_handle_alias` with its doc comment and the
`seed_contact_handle_alias_unit_first_wins` test.

In `crates/vault/server/src/import/staging.rs`, drop
`seed_contact_handle_alias` from the `use super::contact_name::{…}` list at
lines 24–25, and delete line 549 with its comment:

```rust
        // Seed contact identity alias from the import display name (first wins).
        seed_contact_handle_alias(tx, &stmts.account_id, handle_id, name_alias.as_deref()).await?;
```

- [ ] **Step 4: Delete the reader**

In `crates/vault/server/src/contacts_api.rs`, delete the `name_alias` field and
its doc comment from `ContactHandleInfo` (lines 50–52):

```rust
    /// Per-service alias from the address book, when linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_alias: Option<String>,
```

In the handles query, delete the `NULLIF(trim(ch.name_alias), '') AS name_alias,`
select column (line 372) and drop `ch.name_alias` from the `GROUP BY` (line
391). The row tuple `ContactHandleRow` loses one element, so remove `name_alias`
from the destructuring pattern and from the `ContactHandleInfo` construction
(lines 406 and 416), and shift the remaining tuple positions by one.

- [ ] **Step 5: Run the tests**

```bash
cargo test -p message-vault-server
grep -rn 'name_alias' crates/vault/server/src/db/ crates/vault/server/src/contacts_api.rs
```
Expected: tests PASS; the grep returns only
`crates/vault/server/src/db/participant_names.rs` (`p.name_alias` in the naming
query) and test fixtures that insert into `participants`.

- [ ] **Step 6: Confirm the schema fixture and generated artifacts**

```bash
node scripts/sync-vault-schema.mjs
node scripts/check-sql-column-comments.mjs
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
```
Expected: all four succeed; `openapi.json` loses `name_alias` from
`ContactHandleInfo`.

- [ ] **Step 7: Commit**

```bash
git add schema/sql/contacts.sql crates/vault/server/src web-next docs/src/assets/openapi.json \
  web/src/lib/vaultApi.types.ts tests/fixtures/schema
git commit -m "refactor(vault-server): delete contact_handles.name_alias (schema 8)"
```

---

### Task 6: An address book renames an imported Contact

**Files:**
- Modify: `crates/vault/server/src/db/contacts.rs:488-560` (`insert_contact_drafts`)
- Modify: `crates/vault/server/src/contacts_api.rs:2419` (extend the existing
  `loading_an_address_book_replaces_only_its_own_rows` test's neighbourhood)

**Interfaces:**
- Consumes: Task 3 — an imported Contact now carries a name and
  `origin = 'import'`.
- Produces: nothing later depends on it.

Before Task 3 an imported Contact was nameless, so a book card for the same
phone created a second, handle-less Contact and nobody noticed. Now the imported
Contact has the backup's name, and leaving the book's card beside it would show
"Bobby" on the thread while "Robert Smith" sat in Unknown with no identity.
ADR-0006 says an address-book name replaces an imported one, so the load adopts
the imported Contact instead of creating a duplicate.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vault/server/src/contacts_api.rs`:

```rust
    #[tokio::test]
    async fn an_address_book_renames_a_contact_an_import_named() {
        let (pool, dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        // What an import leaves behind: a contact named by the backup, holding
        // the phone, marked as the import's.
        let discovered =
            insert_contact_with_handle(&mut conn, &account, "Bobby", "+15551234567").await;
        sqlx::query("UPDATE contacts SET origin = 'import' WHERE id = $1")
            .bind(discovered)
            .execute(&mut *conn)
            .await
            .unwrap();

        let book = dir.path().join("book.vcf");
        std::fs::write(
            &book,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Robert Smith\nN:Smith;Robert;;;\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        contacts::load_contacts_if_needed(&mut conn, Some(&book), true, &account)
            .await
            .unwrap();

        let names: Vec<String> = sqlx::query_scalar(
            "SELECT preferred_name FROM contacts WHERE account_id = $1 ORDER BY preferred_name",
        )
        .bind(&account)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            names,
            vec!["Robert Smith".to_string()],
            "the book renames the imported contact instead of making a second one: {names:?}"
        );

        let name: String = sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
            .bind(discovered)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(name, "Robert Smith");

        // The identity stays the import's, so a later book that drops the card
        // does not take the person's messages' contact with it.
        let origin: String = sqlx::query_scalar("SELECT origin FROM contacts WHERE id = $1")
            .bind(discovered)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(origin, "import");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p message-vault-server an_address_book_renames`
Expected: FAIL — the assertion reports two names,
`["Bobby", "Robert Smith"]`.

- [ ] **Step 3: Adopt the imported contact**

In `crates/vault/server/src/db/contacts.rs`, inside `insert_contact_drafts`,
replace the body of the `for draft in drafts` loop's opening — the block that
inserts the contact row — with a lookup first. Add this helper above
`insert_contact_drafts`:

```rust
/// The contact an import already built for one of this card's phones, if any.
///
/// A card and an imported contact that share a phone are the same person, so
/// the book renames that contact rather than standing a second one beside it.
/// The identity stays the import's — `origin` is left alone — because the
/// messages are what proved the person exists, and a later book that drops the
/// card must not take them with it.
async fn imported_contact_for_draft(
    conn: &mut AnyConnection,
    account_id: &str,
    phones: &[(String, Option<String>)],
) -> Result<Option<i64>> {
    for (phone, _note) in phones {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT ch.contact_id
             FROM contact_handles ch
             JOIN handles h ON h.id = ch.handle_id
             JOIN contacts c ON c.id = ch.contact_id
             WHERE ch.account_id = $1
               AND h.normalized = $2
               AND h.handle_type = 'phone'
               AND c.origin = 'import'
             LIMIT 1",
        )
        .bind(account_id)
        .bind(phone)
        .fetch_optional(&mut *conn)
        .await?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}
```

Then, in the loop, replace:

```rust
        let preferred_name = draft.preferred_name.as_deref().unwrap_or("");
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name, origin)
             VALUES ($1, $2, 'address_book') RETURNING id",
        )
        .bind(account_id)
        .bind(preferred_name)
        .fetch_one(&mut *tx)
        .await?;
        stats.contacts += 1;
```

with:

```rust
        let preferred_name = draft.preferred_name.as_deref().unwrap_or("");
        let contact_id = match imported_contact_for_draft(&mut tx, account_id, &draft.phones)
            .await?
        {
            Some(existing) => {
                sqlx::query(
                    "UPDATE contacts SET preferred_name = $1 WHERE account_id = $2 AND id = $3",
                )
                .bind(preferred_name)
                .bind(account_id)
                .bind(existing)
                .execute(&mut *tx)
                .await?;
                touch_contact(&mut tx, account_id, existing).await?;
                existing
            }
            None => {
                let created: i64 = sqlx::query_scalar(
                    "INSERT INTO contacts (account_id, preferred_name, origin)
                     VALUES ($1, $2, 'address_book') RETURNING id",
                )
                .bind(account_id)
                .bind(preferred_name)
                .fetch_one(&mut *tx)
                .await?;
                created
            }
        };
        stats.contacts += 1;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p message-vault-server contacts`
Expected: PASS, including the existing
`loading_an_address_book_replaces_only_its_own_rows`, which asserts an
import-discovered contact survives a book reload and that hand-built Contact
Groups are untouched. If that test now fails, the adoption is reaching contacts
whose phones the book does not list — narrow the lookup, do not change the test.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/db/contacts.rs crates/vault/server/src/contacts_api.rs
git commit -m "feat(contacts): an address book renames the contact an import made"
```

---

### Task 7: The contact drawer drops the Alias column

**Files:**
- Modify: `web/src/components/contactDrawer/ContactDrawerHandles.tsx:69,91,230-238`
- Modify: `web/src/components/contactDrawer/HandleTableRow.tsx:26`
- Modify: `web/src/components/contactDrawer/handleTableLogic.tsx:55-56`
- Modify: `web/src/components/contactDrawer/contactDrawerTypes.ts:85-127`
- Modify: the drawer's `*.test.tsx` files beside those sources

**Interfaces:**
- Consumes: Task 5 — `ContactHandleInfo` no longer carries `name_alias`, so
  `web/src/lib/vaultApi.types.ts` has already lost the field and the web build
  fails until this task lands.
- Produces: nothing later depends on it.

The drawer showed the Contact's name in its header and a read-only Alias column
in the identities table. With one name per person the column has nothing to say.

- [ ] **Step 1: Run the type check to see the failures**

Run: `cd web && npx tsc --noEmit`
Expected: errors on `h.name_alias` in `ContactDrawerHandles.tsx`,
`HandleTableRow.tsx`, `handleTableLogic.tsx`, and `contactDrawerTypes.ts`.

- [ ] **Step 2: Remove the column**

In `ContactDrawerHandles.tsx`:
- delete `const aliasMin = headerLabelMinWidth("Alias");` (line 69)
- delete the `aliasTexts` binding (line 91)
- delete `alias: ColumnSize;` from the returned type and the
  `alias: { width: …, min: aliasMin },` entry from the returned object
- delete the whole `<SortableColumn id="name_alias" …>Alias</SortableColumn>`
  element (lines 230–238)

In `HandleTableRow.tsx`, delete `const alias = h.name_alias?.trim() || "";`
(line 26) and the `<Cell>` that renders it.

In `handleTableLogic.tsx`, delete the `case "name_alias":` arm (lines 55–56).

In `contactDrawerTypes.ts`, delete `name_alias?: string | null;` (line 86), the
`name_alias: null,` literal (line 127), and drop `p.name_alias?.trim() ||` from
the two expressions at lines 92 and 106 so they read:

```ts
    (p.name ?? p.preferred_name)?.trim() || p.handle.trim() || "Contact"
```

```ts
    Boolean((p.name ?? p.preferred_name)?.trim()),
```

- [ ] **Step 3: Fix the drawer tests**

Run: `cd web && npx vitest run src/components/contactDrawer`
Expected: failures where a test builds a handle fixture with `name_alias` or
asserts on an "Alias" column header. Delete the field from the fixtures and the
assertions about that column. Do not add a replacement assertion — the column is
gone, and the header row test that enumerates columns should enumerate the seven
that remain.

- [ ] **Step 4: Run the type check and the drawer tests**

```bash
cd web && npx tsc --noEmit && npx vitest run src/components/contactDrawer
```
Expected: `tsc` reports only the errors Task 8 owns (`ConversationRow.tsx`,
`chatBubbleShared.tsx`, `MessageView.tsx`, `AppearanceSection.tsx`); the drawer
tests PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/contactDrawer
git commit -m "refactor(web): the contact drawer shows one name per person"
```

---

### Task 8: Participant labels come from the server

**Files:**
- Delete: `web/src/lib/nameAliases.ts`, `web/src/lib/useNameAliases.ts` and
  their `*.test.ts` files
- Modify: `web/src/components/ConversationRow.tsx:3,6,49-58,64,68,80-97,139,156,198`
- Modify: `web/src/components/messages/chatBubbleShared.tsx:3,5,32-59,197,209`
- Modify: `web/src/components/messages/SmsBubble.tsx:1,20,34`
- Modify: `web/src/components/messages/ImessageBubble.tsx:19`
- Modify: `web/src/screens/MessageView.tsx:79-105`
- Modify: `web/src/screens/settings/AppearanceSection.tsx`

**Interfaces:**
- Consumes: Task 2 — `Participant.name` is a non-optional `string` that is
  already the answer.
- Produces: nothing later depends on it.

`personDisplayLabel` combined three fields because the server handed over three.
It now hands over one, so the client-side rule, the browser-storage preference
behind it, and the Appearance toggle that set it all go. This is also what
ADR-0002 asks for: no second store of state the vault already owns.

- [ ] **Step 1: Delete the two modules**

```bash
cd web && git rm src/lib/nameAliases.ts src/lib/useNameAliases.ts
git rm -f src/lib/nameAliases.test.ts src/lib/useNameAliases.test.ts 2>/dev/null || true
```

(The `|| true` covers the case where a test file for one of them does not exist;
check with `ls src/lib/ | grep -i alias` and remove whatever is there.)

- [ ] **Step 2: Run the type check to enumerate the callers**

Run: `cd web && npx tsc --noEmit`
Expected: `Cannot find module '../lib/nameAliases'` and
`'../lib/useNameAliases'` from the six files listed above.

- [ ] **Step 3: Read `p.name` in `ConversationRow.tsx`**

Delete both imports (lines 3 and 6), delete `participantLabel` entirely, and
delete every `useAliases` binding and parameter. The three functions become:

```tsx
/** Comma-separated names; each name stays whole; at most two lines then ellipsis. */
function GroupNames({ conv }: { conv: Conversation }) {
  return (
    <span className="line-clamp-2 break-words leading-[1.35]">
      {conv.participants.map((p, i) => (
        <span key={p.handle}>
          {i > 0 ? ", " : null}
          <span className="whitespace-nowrap">{p.name}</span>
        </span>
      ))}
    </span>
  );
}

function titleContent(conv: Conversation): ReactNode {
  if (conv.label) return conv.label;
  if (!conv.is_group) {
    const p = conv.participants[0];
    if (!p) return "(unknown)";
    return p.name;
  }
  return <GroupNames conv={conv} />;
}

/** Plain-text form of the row title, for the checkbox's accessible name. */
function conversationTitleText(conv: Conversation): string {
  if (conv.label) return conv.label;
  if (!conv.is_group) {
    const p = conv.participants[0];
    return p ? p.name : "(unknown)";
  }
  return conv.participants.map((p) => p.name).join(", ");
}
```

Update the two call sites at lines 156 and 198 to `titleContent(conversation)`
and `conversationTitleText(conversation)`.

- [ ] **Step 4: Read `p.name` in the message bubbles**

In `chatBubbleShared.tsx`, delete both imports and replace `senderName` with:

```tsx
export function senderName(m: Message): string {
  if (m.is_from_me) return "Me";
  if (m.sender) {
    const p = m.conversation.participants.find((x) => x.handle === m.sender);
    return p ? p.name : m.sender;
  }
  const p = m.conversation.participants[0];
  return p ? p.name : "Unknown";
}
```

Delete the `useAliases` binding at line 197 and change line 209 to
`{senderName(message)}`.

In `SmsBubble.tsx`, delete the import (line 1) and the `useAliases` binding
(line 20), and change line 34 to `senderLabel={senderName(message)}`.

In `ImessageBubble.tsx`, delete the `useAliases` binding (line 19) and drop the
argument wherever it is passed on.

- [ ] **Step 5: Read `p.name` in `MessageView.tsx`**

Replace lines 78–105 with:

```tsx
  /** Prefer list-API participants; fall back to the loaded page's conversation header. */
  const displayParticipants = useMemo(() => {
    const source =
      conversation.participants.length > 0
        ? conversation.participants
        : messages[0]?.conversation.participants || [];
    return source.map((p) => ({
      label: p.name,
      contact_id: p.contact_id == null ? null : String(p.contact_id),
    }));
  }, [conversation.participants, messages]);
```

Both routes now return the same type, so the two branches that differed only in
which name fields they read collapse into one.

- [ ] **Step 6: Delete the Appearance toggle**

Replace `web/src/screens/settings/AppearanceSection.tsx` with:

```tsx
import ThemeSettings from "../../components/ThemeSettings";

export function AppearanceSection() {
  return (
    <div>
      <ThemeSettings />
    </div>
  );
}
```

- [ ] **Step 7: Fix the web tests**

Run: `cd web && npx vitest run`
Expected: failures in tests that set the alias preference, build participant
fixtures with `name_alias` / `preferred_name`, or assert on the "Use name
aliases" checkbox. Update each fixture to the one field (`name`) and delete the
toggle's tests — the preference no longer exists, so there is nothing left to
assert about it.

- [ ] **Step 8: Verify the web is clean**

```bash
cd web && npm run lint && npm test && npm run build
grep -rn 'nameAlias\|name_alias\|personDisplayLabel\|useNameAliases' src/
```
Expected: all three commands PASS; the grep returns nothing.

- [ ] **Step 9: Commit**

```bash
git add web/src
git commit -m "refactor(web): the server names a participant, so the client stops guessing"
```

---

### Task 9: The roadmap records the merge

**Files:**
- Modify: `docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`
- Modify: `CLAUDE.md` if the naming rule is described there

**Interfaces:**
- Consumes: Tasks 1–8.
- Produces: the roadmap's Status table names the next pull request.

This is step 6 of "How a pull request is delivered" and is committed on main
after the squash-merge, so it lands as its own follow-up commit rather than
inside the pull request.

- [ ] **Step 1: Run the full check**

Run: `./scripts/check-pr.sh`
Expected: `All pre-PR checks passed.`

- [ ] **Step 2: Confirm the roadmap's Done-when list**

```bash
grep -rln 'name_alias\|contact_name_mode\|ContactNameMode' crates/vault/server/src crates/libs/vault-push/src src-tauri/src
grep -rn 'seed_contact_handle_alias\|ensure_contact_for_handle' crates/vault/server/src
```
Expected: the first grep returns only
`crates/vault/server/src/db/participant_names.rs` and the import and test files
that write `participants.name_alias`; the second returns only
`import/contact_name.rs` and `import/staging.rs`, with no
`seed_contact_handle_alias`.

- [ ] **Step 3: Open the pull request**

```bash
git push -u origin HEAD
gh pr create --base main \
  --title "feat: an import names the Contact (ADR-0006)" \
  --body "$(cat <<'BODY'
Pull request 3 of the HTTP interface repair.

A backup is an address book the person already curated. Refusing its names made
every import a naming chore and filled Unknown with people the vault could have
named. Now the import puts the backup's name on the Contact it creates, and on a
Contact an earlier import left nameless. A name the person types, or loads from
an address book, still wins.

One `db/participant_names` module owns the naming query, and the conversation
list and Export return the one `Participant` type, so a person cannot show two
names on one screen. Three mechanisms that existed only because imported
Contacts were nameless are gone: `contact_handles.name_alias` (schema 8),
`contact_name_mode` on import, and the web's "Use name aliases" toggle.

Design: docs/superpowers/specs/2026-09-03-http-interface-repair-design.md
Decision: docs/adr/0006-an-import-names-the-contact.md

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_01AGey7vJswYhgjdYKGg4hrb
BODY
)"
gh pr checks --watch
```

- [ ] **Step 4: After the squash-merge, update the roadmap on main**

In the Status table, set row 3's State to `merged, #<number>` and row 4's to
`**next**`. Add this plan to the "Plans so far" line. Commit on a branch, not on
main.

---

## Self-Review

**Spec coverage.** Every sentence of the spec's "Names (ADR-0006)" section maps
to a task: `ensure_contact_for_handle` naming the Contact → Task 3;
`seed_contact_handle_alias` and `contact_handles.name_alias` deleted → Task 5;
`ContactNameMode` and `contact_name_mode` deleted from the server, `vault-push`,
and `src-tauri` → Task 4; `participants.name_alias` kept → Task 3 (the
`INSERT_PARTICIPANT` bind stays); one `participant_names` module with the single
query → Task 1; `p.contact_id` not consulted → Task 1, with a test named for it;
`ConversationParticipant` and `ExportParticipant` becoming one type → Task 2;
the drawer's Alias column → Task 7; the address-book rule → Task 6.

**One thing the spec assumes that the code did not do.** The spec says the
address-book load "overwrites an `origin = import` name with the book's name."
It does not today: `insert_contact_drafts` always creates a new
`address_book` contact, and the `ON CONFLICT DO NOTHING` on `contact_handles`
leaves it with no identity when an import already owns the phone. Before this
plan that produced a harmless-looking nameless pair; after Task 3 it would show
the backup's name on the thread while the book's name sat in Unknown. Task 6
makes the load adopt the imported Contact, which is what ADR-0006 promises.

**Deliberately out of scope.** A participant the source named with no address
(`participants.handle_id IS NULL`) is excluded from the conversation list today,
because the loader inner-joins `handles`. Task 1 keeps that join and that
behaviour. Fixing it means deciding what `handle` and `service` mean for someone
with no address, which belongs with the conversation read routes in pull
request 4, not here.
