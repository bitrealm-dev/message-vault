# Vault Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the approved vault authentication, storage, import, query, and demo-generation failures while preserving the explicitly excluded read-only, passwordless, and schema-recreation policies.

**Architecture:** Apply four independently testable batches. Account changes use SQLite transactions; assets and generated bundles use temporary paths plus atomic replacement; imports map staging rows by account-scoped identity; query endpoints share validated parsing and joined SQL.

**Tech Stack:** Rust 2024, Axum 0.8, rusqlite 0.40, tempfile, Chrono, Cargo test/Clippy.

## Global Constraints

- Do not enforce `accounts.read_only`.
- Do not remove passwordless local registration or empty-password login.
- Do not restore in-place database schema migrations.
- Do not commit changes unless the user separately requests a commit.
- Write each regression test before its production change and run it once in the failing state.
- Preserve existing public JSON response fields.

## File map

- `crates/vault/server/src/server.rs`: authentication router construction and HTTP query validation.
- `crates/vault/server/src/auth.rs`: transactional password changes.
- `crates/vault/server/src/profile.rs`: transactional profile updates.
- `crates/vault/server/src/db/api_tokens.rs`: account-wide API-token revocation.
- `crates/vault/server/src/assets.rs`: verified atomic blob installation.
- `crates/vault/server/src/import.rs`: append child mapping and import-scoped message IDs.
- `crates/vault/server/src/export_api.rs`: consistent count joins and boolean query SQL.
- `crates/vault/server/src/search_query.rs`: parsed boolean expression support.
- `crates/vault/server/src/contacts_api.rs`: trashed-contact mutation and query limits.
- `crates/vault/server/src/conversations_api.rs`: query limits.
- `crates/vault/demo-seed/src/config.rs`: fixed reference timestamp.
- `crates/vault/demo-seed/src/conversations.rs`: timestamps from configuration.
- `crates/vault/demo-seed/src/lib.rs`: generate into temporary output and replace after success.
- `crates/vault/demo-seed/demo_seed.toml`: committed reference timestamp.
- `crates/vault/server/src/reset_demo.rs`: preserve current demo data on failure.

---

### Task 1: Authentication mode and atomic account updates

**Files:**
- Modify and test: `crates/vault/server/src/server.rs`
- Modify and test: `crates/vault/server/src/auth.rs`
- Modify and test: `crates/vault/server/src/profile.rs`
- Modify and test: `crates/vault/server/src/db/api_tokens.rs`

**Interfaces:**
- Produce `fn auth_public_router(mode: AuthMode) -> Router<AppState>` or an equivalent testable router builder.
- Produce `pub fn delete_all_api_tokens(conn: &Connection, account_id: &str) -> Result<u64>`.
- Keep all existing HTTP request and response types.

- [ ] **Step 1: Add failing Hanko route tests**

Build the public authentication router for each explicit `AuthMode`. Assert local mode does not return 404 for `POST /v1/auth/register` and `POST /v1/auth/login`; assert Hanko mode returns 404 for both while retaining `/v1/auth/hanko/session`.

Run:

```bash
cargo test -p message-vault-server hanko_router_excludes_local_auth_routes -- --nocapture
```

Expected before implementation: failure because the router always includes local routes.

- [ ] **Step 2: Build authentication routes from explicit mode**

Move the route selection into a helper. In local mode, register local and Hanko routes. In Hanko mode, register only Hanko session exchange. Apply the existing 32 KiB body limit to the resulting router. `run` passes `AuthMode::from_env()`.

- [ ] **Step 3: Add failing password transaction tests**

Create an account with a password, session token, and named API token. Exercise a small synchronous helper used by `change_password_handler`. Verify success changes the password, rotates the session, and removes every named API token. Add an injected SQL failure or transaction rollback test proving the prior password/session/tokens remain unchanged.

Run:

```bash
cargo test -p message-vault-server change_password_transaction -- --nocapture
```

- [ ] **Step 4: Implement transactional password change**

Add `delete_all_api_tokens`. Open a mutable connection, start one transaction, verify the current password, update its hash, revoke named API tokens, rotate the session, then commit. Return the plaintext replacement session only after commit succeeds.

- [ ] **Step 5: Add failing profile rollback test**

Submit a preferred name followed by an unsupported handle service. Assert the operation returns an error and the preferred name remains unchanged.

Run:

```bash
cargo test -p message-vault-server profile_update_rolls_back -- --nocapture
```

- [ ] **Step 6: Implement transactional profile updates**

Change `apply_profile_update` to accept a transaction-compatible connection reference. Start a transaction in the blocking handler, apply every mutation, commit, then load the response.

- [ ] **Step 7: Verify Task 1**

```bash
cargo test -p message-vault-server auth:: db::api_tokens:: profile:: server:: -- --nocapture
```

If Cargo accepts only one name filter, run the four filters separately.

---

### Task 2: Atomic assets and import identity mapping

**Files:**
- Modify and test: `crates/vault/server/src/assets.rs`
- Modify and test: `crates/vault/server/src/import.rs`

**Interfaces:**
- `store_verified` keeps its existing signature and return value.
- Append imports retain existing message fields and add only missing attachment/tapback children.
- Message mapping must be scoped by staging/account/source/GUID identity, not global row IDs.

- [ ] **Step 1: Add failing corrupt-destination tests**

Pre-create `<assets>/<prefix>/<sha>.ext` containing bytes that do not hash to the filename. Call `store_verified` with valid source bytes and assert the destination contains the valid source afterward. Add a concurrent test where two installers target the same SHA and both return a valid final file.

Run:

```bash
cargo test -p message-vault-server assets::tests::store_verified_replaces_corrupt_destination -- --nocapture
```

- [ ] **Step 2: Implement verified atomic installation**

Hash the source before checking deduplication. Hash any existing destination before reporting it present. Write/copy to a `NamedTempFile` or unique create-new temporary file inside the destination shard, flush and `sync_all`, then persist/rename atomically. If another writer wins, verify the winner before returning `already_present`.

- [ ] **Step 3: Add failing append-child regression test**

Import a message with a non-empty GUID and no children. Append the same GUID with one attachment and one tapback. Assert the original body remains unchanged and both missing children exist once. Repeat the append and assert child rows are not duplicated.

Run:

```bash
cargo test -p message-vault-server append_existing_guid_adds_missing_children -- --nocapture
```

- [ ] **Step 4: Replace global message-ID mapping**

Populate `_promote_msg_map` directly from staging and production rows using `(account_id, source, guid)` for non-empty GUIDs. Include skipped existing rows. For empty GUID inserts, capture only rows belonging to the current account/chunk, using `RETURNING`, an import-specific marker, or another account-scoped mapping that cannot include a concurrent writer.

Insert attachments with a `NOT EXISTS` predicate over stable child identity fields. Insert tapbacks with a `NOT EXISTS` predicate over message ID, part index, kind, emoji, direction, and sender.

- [ ] **Step 5: Add unrelated-writer mapping test**

Within the promotion transaction test setup, insert an unrelated account message between the relevant IDs or invoke the mapping helper with unrelated higher IDs. Assert only current-account production IDs appear in `_promote_msg_map`.

Run:

```bash
cargo test -p message-vault-server promote_message_map_ignores_other_accounts -- --nocapture
```

- [ ] **Step 6: Verify Task 2**

```bash
cargo test -p message-vault-server assets::tests -- --nocapture
cargo test -p message-vault-server import::tests -- --nocapture
```

---

### Task 3: Export and contact query correctness

**Files:**
- Modify and test: `crates/vault/server/src/export_api.rs`
- Modify and test: `crates/vault/server/src/search_query.rs`
- Modify and test: `crates/vault/server/src/contacts_api.rs`
- Modify and test: `crates/vault/server/src/conversations_api.rs`
- Modify and test HTTP mapping in `crates/vault/server/src/server.rs` only if needed

**Interfaces:**
- Keep existing query syntax.
- Return `ExportQueryError::BadRequest` for unsupported or oversized queries.
- Reuse one exported query validation function for byte and term limits.

- [ ] **Step 1: Add failing filtered-count test**

Seed sender and chat handles. Call `export_message_count` with `from:alice` and an `in:`/chat-handle query. Assert message, conversation, and attachment counts return successfully.

Run:

```bash
cargo test -p message-vault-server export_message_count_supports_handle_filters -- --nocapture
```

- [ ] **Step 2: Use the shared joined source in every count query**

Build the conversation count from `messages_from_sql()` or the exact equivalent join set used by message export. Keep bound parameter order unchanged.

- [ ] **Step 3: Add failing boolean query tests**

Seed one message containing `foo`, one containing `bar`, and one containing both. Assert `foo OR bar` returns all three and `foo AND bar` returns only the third. Add a negation assertion if the parsed AST supports it.

Run:

```bash
cargo test -p message-vault-server export_boolean_query_preserves_or -- --nocapture
```

- [ ] **Step 4: Compile the parsed expression without flattening OR**

Add a recursive compiler for `FtsExpr` that writes parenthesized SQL and bound parameters for leaf, phrase, AND, OR, and NOT nodes. Use the same expression for body FTS and metadata matching. If a node cannot be represented consistently, return `BadRequest` instead of flattening it.

- [ ] **Step 5: Add failing trashed-contact mutation test**

Create a contact whose only linked handle is trashed. Call `mutate_contact` and assert it behaves as not found and changes no rows.

Run:

```bash
cargo test -p message-vault-server mutate_contact_rejects_trashed_contact -- --nocapture
```

- [ ] **Step 6: Exclude trashed contacts from mutation**

Make `contact_exists` apply `NOT_TRASHED_CONTACT_SQL` with the same account scoping as detail retrieval.

- [ ] **Step 7: Add failing query-limit tests**

Pass a query exceeding 2,048 bytes to contact and conversation list parsing. Assert both return `BadRequest`. Add a query with more terms than the export limit and assert the same.

Run:

```bash
cargo test -p message-vault-server list_queries_enforce_search_limits -- --nocapture
```

- [ ] **Step 8: Share query validation**

Move the existing export byte/term checks into `search_query.rs` and call them before contact, conversation, and export parsing. Do not change list pagination limits.

- [ ] **Step 9: Verify Task 3**

```bash
cargo test -p message-vault-server export_api::tests -- --nocapture
cargo test -p message-vault-server contacts_api::tests -- --nocapture
cargo test -p message-vault-server conversations_api::tests -- --nocapture
cargo test -p message-vault-server search_query::tests -- --nocapture
```

---

### Task 4: Recoverable and reproducible demo generation

**Files:**
- Modify and test: `crates/vault/demo-seed/src/config.rs`
- Modify and test: `crates/vault/demo-seed/src/conversations.rs`
- Modify and test: `crates/vault/demo-seed/src/lib.rs`
- Modify: `crates/vault/demo-seed/demo_seed.toml`
- Modify and test: `crates/vault/server/src/reset_demo.rs`

**Interfaces:**
- Add `reference_time: chrono::DateTime<chrono::Utc>` or an RFC3339 string parsed during config loading.
- Keep `generate(&SeedConfig) -> Result<GenStats>` and `generate_to` signatures.
- Keep `run_reset_demo` command behavior and result type.

- [ ] **Step 1: Add failing reference-time test**

Load the committed config twice and assert the parsed reference instant is fixed. Call the timestamp generator twice with identically seeded RNGs and assert identical output.

Run:

```bash
cargo test -p demo-seed fixed_reference_time_makes_timestamps_reproducible -- --nocapture
```

- [ ] **Step 2: Add and use configured reference time**

Add an RFC3339 value such as `reference_time = "2026-08-01T12:00:00Z"` to `demo_seed.toml`. Parse it once in `SeedConfig::load` or expose a validated accessor. Pass it into every burst timestamp calculation instead of calling `Utc::now()`.

- [ ] **Step 3: Add failing generation-preservation test**

Extract a small replacement helper that receives active and prepared output roots. Seed the active root with a sentinel JSONL file, induce a preparation failure, and assert the sentinel remains byte-for-byte unchanged.

Run:

```bash
cargo test -p demo-seed failed_generation_preserves_existing_bundle -- --nocapture
```

- [ ] **Step 4: Generate into a temporary sibling**

Refactor the current generation body to write to an explicitly supplied root. `generate` creates a temporary sibling directory, writes and validates every file there, then replaces generated staging/config/README paths. Do not call `clear_jsonl` against the active output before preparation succeeds.

- [ ] **Step 5: Add failing reset rollback test**

Create a temporary configured vault with an existing demo account/message. Invoke an extracted reset transaction/helper with an invalid prepared bundle. Assert the original account/message and account data directory remain.

Run:

```bash
cargo test -p message-vault-server failed_reset_preserves_existing_demo_account -- --nocapture
```

- [ ] **Step 6: Make reset recoverable**

Before wiping, validate all three source trees and prepare a backup of the demo account database state and account data directory. Prefer importing into a temporary database copied from the active database, then atomically replace the database and demo account directory after import, dedupe, and asset processing succeed. If full database replacement is unsafe while serving, use an attached temporary database or an explicit backup/restore guard. Any failure restores the prior database and directory before returning.

- [ ] **Step 7: Verify Task 4**

```bash
cargo test -p demo-seed -- --nocapture
cargo test -p message-vault-server reset_demo::tests -- --nocapture
```

---

### Task 5: Full verification

**Files:**
- Inspect all modified files and the approved design.

- [ ] **Step 1: Format**

```bash
cargo fmt --all -- --check
```

- [ ] **Step 2: Lint**

```bash
cargo clippy -p message-vault-server -p demo-seed --all-targets -- -D warnings
```

- [ ] **Step 3: Run focused suites**

```bash
cargo test -p message-vault-server -p demo-seed
```

- [ ] **Step 4: Run workspace suite**

```bash
cargo test --workspace
```

- [ ] **Step 5: Inspect final changes**

Confirm the diff contains no enforcement of read-only accounts, no removal of passwordless login, no restored schema migration code, and no unrelated changes.
