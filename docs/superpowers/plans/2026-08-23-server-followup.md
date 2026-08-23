# Server Crate Follow-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 14 server-crate findings from the rust audit: rewrite HTTP/rustdoc
documentation (the two high findings first), move handlers out of the `server.rs`
monolith, split `import.rs`, share the thread-tag/contact-group CRUD, and clean up
errors and the public surface — all behavior-preserving.

**Architecture:** Three sequenced workstreams. Workstream 1 (Tasks 1–5) rewrites
every handler doc, `ToSchema` doc, and module intro to the committed style guide,
adds the `missing_docs` gate, and regenerates `openapi.json`. Workstream 2
(Tasks 6–17) moves each handler group into its domain module with its utoipa
registration and tests in the same task, splits `import.rs` into
`staging`/`promote`/`contact_name`, and factors one named-membership CRUD helper.
Workstream 3 (Tasks 18–22) types the API-token label error, swaps `libc::flock`
for `fs2`, curates the `lib.rs` surface, removes the dead import field, and lands
the CHANGELOG entry with full verification.

**Tech Stack:** Rust, Axum 0.8, utoipa/utoipa-axum (OpenApiRouter), rusqlite,
anyhow, fs2. No new dependencies.

## Global Constraints

- **Behavior-preserving.** Status codes, JSON bodies, and error strings do not
  change. Every existing server test must pass on the final branch.
- **Green after every task.** `cargo fmt --check`, clippy clean, and
  `cargo test -p message-vault-server` pass after each task; no mid-project
  broken states.
- **Doc standard.** The committed rustdoc style guide
  (`docs/src/content/docs/vault/developer/rustdoc-style.md`) governs every
  comment written here: first sentence states what the item is; no route
  echoes; no `# Errors` headings in handler docs; every public item documented;
  examples for non-obvious behavior; no filler or jargon.
- **Sequencing.** Tasks run in order (Workstream 1 → 2 → 3). Do not reorder or
  combine tasks.
- **openapi.json stays in sync.** Any change to a handler doc comment or a
  `ToSchema` struct doc changes the OpenAPI dump, and the
  `committed_openapi_matches_dump` test compares it against the committed
  `docs/src/assets/openapi.json`. Regenerate it in the same task (and commit it
  in the same commit) with:
  `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`
- **Scope.** Changes only under `crates/vault/server/**`,
  `docs/src/assets/openapi.json`, and `CHANGELOG.md`. No version bumps, no
  `web/` or `src-tauri/` changes, no new dependencies.
- **Git.** Work on a branch (never `main`); never create or push `v*` tags.
  Commit messages in plain English, one commit per task unless a task says
  otherwise.
- **No `#[allow(missing_docs)]` anywhere.**

---

### Task 1: Module intros for undocumented modules

**Files:**
- Modify: `crates/vault/server/src/import.rs:1`
- Modify: `crates/vault/server/src/assets.rs:1`
- Modify: `crates/vault/server/src/jsonl.rs:1`
- Modify: `crates/vault/server/src/config.rs:1`
- Modify: `crates/vault/server/src/server.rs:1`
- Modify: `crates/vault/server/src/db/account_profile.rs:1`
- Modify: `crates/vault/server/src/db/contacts.rs:1`
- Modify: `crates/vault/server/src/db/schema.rs:1`

**Interfaces:**
- Consumes: nothing.
- Produces: `//!` intros that later tasks rely on (the `import.rs` intro already
  names the `staging`/`promote`/`contact_name` stages that Task 12 creates).

- [ ] **Step 1: Insert the intros**

Insert each `//!` block as the first lines of the file, before the `use`
statements. Use the text exactly as given.

`crates/vault/server/src/import.rs`:

```rust
//! Import message-ir JSONL into the vault.
//!
//! The pipeline runs in three stages: `staging` parses JSONL files and writes
//! staging rows, `promote` copies staging rows into the production tables, and
//! `contact_name` links handles to vault contacts and merges display names.
//! The HTTP handlers for `POST /v1/import` and the `/v1/imports` session
//! routes live at the end of this module.
```

`crates/vault/server/src/assets.rs`:

```rust
//! Content-addressed asset storage under each account's `assets/` directory.
//!
//! Files are stored by SHA-256 fingerprint (`aa/aaaa…ext`) and every reuse
//! re-checks the bytes against the claimed fingerprint. The HTTP handlers for
//! `HEAD` / `GET` / `PUT /v1/assets/{sha256}` and the multipart upload routes
//! also live here; multipart staging itself is in `asset_uploads`.
```

`crates/vault/server/src/jsonl.rs`:

```rust
//! Read message-ir JSONL files (one JSON object per line) into import records.
```

`crates/vault/server/src/config.rs`:

```rust
//! Config file model ([`Config`]) plus path/source validation and the
//! environment-driven settings ([`AuthMode`], [`GuestDemoSettings`]).
```

`crates/vault/server/src/server.rs`:

```rust
//! Router assembly, shared state, auth resolution, and HTTP plumbing.
//!
//! Domain handlers live in their own modules: `auth` (login and session),
//! `profile` (account settings), `contacts_api`, `conversations_api`,
//! `export_api` (messages and counts), `import` (JSONL ingest and import
//! sessions), and `assets` (asset bytes and multipart uploads). This module
//! keeps the pieces they share: [`AppState`], [`ApiError`], Bearer token
//! resolution, body-streaming helpers, and `http_app`, which assembles the
//! router.
```

`crates/vault/server/src/db/account_profile.rs`:

```rust
//! Account rows, profile fields, guest status, and message deletion.
```

`crates/vault/server/src/db/contacts.rs`:

```rust
//! Address book loading (VCF or vCard CSV) and contact/group/handle links.
```

`crates/vault/server/src/db/schema.rs`:

```rust
//! Schema management for the vault and accounts databases.
//!
//! Serve and import open their SQLite connections through `open_configured`
//! (shared pragmas) and ensure the schema with `ensure_vault_schema` /
//! `ensure_accounts_schema`. DDL lives in the SQL files embedded at compile
//! time; the functions here apply and evolve it.
```

- [ ] **Step 2: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0, all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/vault/server/src/import.rs crates/vault/server/src/assets.rs \
  crates/vault/server/src/jsonl.rs crates/vault/server/src/config.rs \
  crates/vault/server/src/server.rs crates/vault/server/src/db/account_profile.rs \
  crates/vault/server/src/db/contacts.rs crates/vault/server/src/db/schema.rs
git commit -m "docs(server): add module intros to undocumented modules"
```

---

### Task 2: Rewrite route-echo and `# Errors` handler docs (api_tokens_api, profile, auth)

**Files:**
- Modify: `crates/vault/server/src/api_tokens_api.rs` (4 handler doc comments)
- Modify: `crates/vault/server/src/profile.rs` (3 handler doc comments)
- Modify: `crates/vault/server/src/auth.rs` (7 handler doc comments)
- Modify: `docs/src/assets/openapi.json` (regenerated)

**Interfaces:**
- Consumes: Task 1 intros.
- Produces: plain-prose summaries for the 14 routes the audit flagged
  (ECHOED-ROUTE-SUMMARY / ERRORS-SECTION-IN-DESCRIPTION); the regenerated dump
  that the stale-spec test compares.

- [ ] **Step 1: Replace the four api_tokens_api.rs handler docs**

In `crates/vault/server/src/api_tokens_api.rs`, replace each existing handler
doc comment (the `///` block directly above the `#[utoipa::path(...)]`) with the
matching replacement. Delete every line of the old comment including the
`/// \`GET /v1/…\`` echo, the blank `///`, and the `/// # Errors` section.

1. Above `pub async fn list_api_tokens_handler`:

```rust
/// List the account's named API tokens with their scopes and masked secrets.
```

2. Above `pub async fn create_api_token_handler`:

```rust
/// Create a named API token. Returns the plaintext secret once, at creation;
/// it is never returned again.
```

3. Above `pub async fn delete_api_token_handler`:

```rust
/// Delete one named API token. Requests using it start failing on the next call.
```

4. Above `pub async fn rename_api_token_handler`:

```rust
/// Rename one named API token. The label is trimmed before storing.
```

- [ ] **Step 2: Replace the three profile.rs handler docs**

In `crates/vault/server/src/profile.rs`:

1. Above `pub async fn account_profile_handler`:

```rust
/// Load the signed-in account's profile: username, display name, linked
/// handles, and demo/guest flags.
```

2. Above `pub async fn account_profile_update_handler`:

```rust
/// Update the account's display name and linked handles, then return the
/// reloaded profile.
```

3. Above `pub async fn delete_messages_handler`:

```rust
/// Delete every conversation, message, and attachment for the account.
/// Contacts and the account login survive.
```

- [ ] **Step 3: Replace the seven auth.rs handler docs**

In `crates/vault/server/src/auth.rs`:

1. Above `pub async fn register_handler`:

```rust
/// Create a local vault account and return its session token.
```

2. Above `pub async fn login_handler`:

```rust
/// Verify a local username and password and return a session token.
```

3. Above `pub async fn hanko_session_handler`:

```rust
/// Verify a Hanko session JSON Web Token and exchange it for a vault session
/// token.
```

4. Above `pub async fn try_demo_handler`:

```rust
/// Open a sample account session: the shared demo account self-hosted, or a
/// private guest copy when the hosted pool is enabled.
```

5. Above `pub async fn logout_handler`:

```rust
/// Revoke the presented session token. Guest account data is deleted with the
/// session.
```

6. Above `pub async fn change_password_handler`:

```rust
/// Verify the current password, store the new one, revoke API tokens, and
/// issue a fresh session token.
```

7. Above `pub async fn delete_account_handler`:

```rust
/// Permanently delete the account and its data directory.
```

- [ ] **Step 4: Regenerate the committed OpenAPI document**

Run:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

Expected: exit 0; `docs/src/assets/openapi.json` changes on disk.

- [ ] **Step 5: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0 — including `committed_openapi_matches_dump`, which now
compares the regenerated file.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/api_tokens_api.rs crates/vault/server/src/profile.rs \
  crates/vault/server/src/auth.rs docs/src/assets/openapi.json
git commit -m "docs(server): rewrite route-echo handler docs as plain prose"
```

---

### Task 3: Add summaries to the 36 undocumented server.rs routes

**Files:**
- Modify: `crates/vault/server/src/server.rs` (36 handler doc comments)
- Modify: `docs/src/assets/openapi.json` (regenerated)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces: a `///` summary above every `#[utoipa::path(...)]` in `server.rs`
  that lacks one, and the rewritten `account_storage_handler` doc.

- [ ] **Step 1: Add each summary above its handler**

In `crates/vault/server/src/server.rs`, add the given `///` block immediately
above each `#[utoipa::path(...)]` attribute named below. Handlers not listed
here already have an acceptable summary; leave them alone except
`account_storage_handler`, which is replaced. For the two asset handlers that
already have summaries but no description, add the second paragraph.

Exact text, in file order:

- `health`:

```rust
/// Report process liveness.
```

- `auth_check`:

```rust
/// Check the Bearer token and return the account it resolves to, its username,
/// and its import sources.
```

- `account_storage_handler` — replace the existing doc comment:

```rust
/// Attachment storage usage for the account: total bytes, count, and the 100
/// largest files.
```

- `contacts_list_handler`:

```rust
/// Page through the account's contacts (id, name, handles, groups).
```

- `contact_summaries_handler`:

```rust
/// First/last message dates and counts for a list of contact ids.
```

- `contact_detail_handler`:

```rust
/// Full contact view: per-handle services, message stats, and group
/// memberships.
```

- `contact_mutate_handler`:

```rust
/// Rename a contact or change its linked handles.
```

- `contact_groups_list_handler`:

```rust
/// List the account's contact groups (A–Z, reserved names hidden).
```

- `contact_groups_create_handler`:

```rust
/// Create a contact group and return the updated list.
```

- `contact_groups_rename_handler`:

```rust
/// Rename a contact group and return the updated list.
```

- `contact_groups_delete_handler`:

```rust
/// Delete a contact group and return the updated list.
```

- `contact_groups_members_handler`:

```rust
/// Contact ids that belong to a named group.
```

- `contact_groups_membership_handler`:

```rust
/// Add or remove contacts in a group.
```

- `thread_tags_list_handler`:

```rust
/// List the account's thread tags (A–Z, reserved names hidden).
```

- `thread_tags_create_handler`:

```rust
/// Create a thread tag and return the updated list.
```

- `thread_tags_rename_handler`:

```rust
/// Rename a thread tag and return the updated list.
```

- `thread_tags_delete_handler`:

```rust
/// Delete a thread tag and return the updated list.
```

- `thread_tags_members_handler`:

```rust
/// Conversation ids that carry a named tag.
```

- `thread_tags_membership_handler`:

```rust
/// Add or remove a tag on conversations.
```

- `conversations_list_handler`:

```rust
/// Page through conversations (newest first) with participants, message
/// counts, and tags.
```

- `conversation_sources_handler`:

```rust
/// Per-backup message counts for one conversation (the Sources panel).
```

- `imports_list_handler`:

```rust
/// List past import sessions for the account with their stats.
```

- `imports_create_handler`:

```rust
/// Start an import session and return its id (see POST /v1/import and
/// complete).
```

- `imports_get_handler`:

```rust
/// Status, timings, and issues for one import session.
```

- `imports_complete_handler`:

```rust
/// Record the outcome of an import session started with POST /v1/imports.
```

- `import_handler`:

```rust
/// Import one message-ir JSONL body (raw or multipart) into the vault.
```

- `export_messages_handler`:

```rust
/// Export messages matching a search query (message mode; cursor paging).
```

- `export_messages_count_handler`:

```rust
/// Count messages, conversations, and attachment fingerprints matching a
/// query.
```

- `asset_head_handler` — add a description after the existing summary:

```rust
/// Probe whether a content-addressed asset is already stored (no body).
///
/// Clients may skip sending bytes when the asset exists.
```

- `asset_get_handler` — add a description after the existing summary:

```rust
/// Download a previously stored content-addressed asset (read-only).
///
/// The body streams the stored bytes; the URL is the SHA-256 fingerprint.
```

- `asset_put_handler`:

```rust
/// Store one asset body under its SHA-256 fingerprint.
```

- `asset_upload_start_handler`:

```rust
/// Start a chunked (multipart) asset upload and get the part size.
```

- `asset_upload_part_handler`:

```rust
/// Write one part of a chunked asset upload.
```

- `asset_upload_complete_handler`:

```rust
/// Assemble the uploaded parts, verify the SHA-256 fingerprint, and install
/// the asset.
```

- `asset_upload_abort_handler`:

```rust
/// Abort and delete a chunked asset upload's staging files.
```

- [ ] **Step 2: Regenerate the committed OpenAPI document**

Run:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

- [ ] **Step 3: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add crates/vault/server/src/server.rs docs/src/assets/openapi.json
git commit -m "docs(server): add summaries for every undocumented route"
```

---

### Task 4: One-line docs for every `ToSchema` struct

**Files:**
- Modify: `crates/vault/server/src/api_tokens_api.rs`
- Modify: `crates/vault/server/src/auth.rs`
- Modify: `crates/vault/server/src/profile.rs`
- Modify: `crates/vault/server/src/contacts_api.rs`
- Modify: `crates/vault/server/src/conversations_api.rs`
- Modify: `crates/vault/server/src/export_api.rs`
- Modify: `crates/vault/server/src/server.rs`
- Modify: `crates/vault/server/src/import.rs`
- Modify: `docs/src/assets/openapi.json` (regenerated)

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces: a one-line `///` doc on every struct that derives
  `utoipa::ToSchema`, which becomes the component description in the HTTP
  catalog.

- [ ] **Step 1: Add the doc lines**

For each struct listed, add the `///` line directly above the
`#[derive(...)]` block that contains `utoipa::ToSchema`. Structs that already
have a doc comment are not listed — leave them alone.

`api_tokens_api.rs`:

```rust
/// One named API token as shown in Settings: label, scopes, and masked secret.
```
above `ApiTokenItem`; and:

```rust
/// The account's named API tokens.
```
above `ListApiTokensResponse`; `/// Body for creating a token: label, scopes, optional expiry.` above
`CreateApiTokenRequest`; `/// The created token, including its plaintext secret (returned once).` above
`CreateApiTokenResponse`; `/// Deletion acknowledgement.` above `DeleteApiTokenResponse`;
`/// Body for renaming a token.` above `RenameApiTokenRequest`; `/// The renamed token's id and stored label.` above
`RenameApiTokenResponse`.

`auth.rs`:

```rust
/// Body for local account registration.
```
above `RegisterRequest`; `/// Username and password.` above `LoginRequest`;
`/// A raw Hanko session JWT from the client's onSessionCreated callback.` above
`HankoSessionRequest`; `/// Session token plus the account id and username it belongs to.` above
`AuthTokenResponse`; `/// Current and new password.` above `ChangePasswordRequest`;
`/// Fresh session token issued after the password change.` above
`ChangePasswordResponse`; `/// Confirmation flag and the current password when one is set.` above
`DeleteAccountRequest`; `/// Deletion acknowledgement.` above `DeleteAccountResponse`;
`/// Revocation acknowledgement.` above `LogoutResponse`.

`profile.rs`:

```rust
/// The signed-in account's profile.
```
above `AccountProfileResponse`; `/// One handle to link or unlink, with its platform service.` above
`ProfileHandleInput`; `/// Display name and handle changes.` above
`AccountProfileUpdateRequest`; `/// Confirmation flag for deleting all messages.` above
`DeleteMessagesRequest`; `/// Counts of deleted conversations and attachment rows.` above
`DeleteMessagesResponse`.

`contacts_api.rs`:

```rust
/// One page of the contact list.
```
above `ContactListPage`; `/// Contact row for the list: name, handles, groups.` above
`ContactSummary`; `/// One handle on a contact with service and message stats.` above
`ContactHandleInfo`; `/// A handle value plus optional platform service.` above
`ContactHandlePayload`; `/// The previous and new handle values for a link change.` above
`ContactUpdateHandlePayload`; `/// The handle to unlink.` above
`ContactRemoveHandlePayload`; `/// Full contact view: every handle with stats, plus totals across them.` above
`ContactDetail`.

`conversations_api.rs`:

```rust
/// One page of the conversation list.
```
above `ConversationListPage`; `/// One participant with display name and handle.` above
`ConversationParticipant`; `/// Conversation row for the list: participants, counts, tags.` above
`ConversationSummary`; `/// One backup source with message counts and share.` above
`ConversationSourceInfo`; `/// Per-source counts for one conversation.` above
`ConversationSourcesPage`.

`export_api.rs`:

```rust
/// One page of exported messages.
```
above `ExportMessagesResponse`; `/// Match counts for an export query.` above
`ExportCountResponse`; `/// One exported message.` above `ExportMessage`;
`/// The conversation a message belongs to.` above `ExportConversation`;
`/// One participant of an exported conversation.` above `ExportParticipant`;
`/// One attachment of an exported message.` above `ExportAttachment`;
`/// One tapback reaction on an exported message.` above `ExportTapback`.

`server.rs` (these structs are `pub(crate)`; the docs feed the catalog):

```rust
/// Import result: stats plus optional dedupe counts.
```
above `ImportResponse`; `/// Cross-source dedupe outcome.` above `DedupeResponse`;
`/// API error envelope returned for non-200 responses.` above `ErrorBody`;
`/// Sign-in mode and Hanko URL so clients can render the right login form.` above
`AuthModeResponse`; `/// Token check result: account, username, sources.` above
`AuthCheckResponse`; `/// Source, mode, tool, and optional account for a new import session.` above
`CreateImportBody`; `/// The new import session id.` above `CreateImportResponse`;
`/// Final stats and issues for a finished import session.` above `CompleteImportBody`;
`/// One parse/convert/upload issue from the import.` above `CompleteImportIssueBody`;
`/// Stored session status after completion.` above `CompleteImportResponse`;
`/// Attachment usage and the largest files.` above `AccountStorageResponse`;
`/// Past import sessions.` above `ImportsListResponse`; `/// One stored import issue.` above
`ImportDetailIssueResponse`; `/// Full import session record.` above `ImportDetailResponse`;
`/// Stored asset fingerprint and path.` above `AssetPutResponse`;
`/// Total bytes and optional MIME type for a chunked upload.` above
`AssetUploadStartBody`; `/// Upload id and part size, or the already-stored asset.` above
`AssetUploadStartResponse`; `/// Bytes written for one part.` above
`AssetUploadPartResponse`; `/// Abort acknowledgement.` above `AssetUploadAbortResponse`;
`/// A group name.` above `ContactGroupNameBody`; `/// Old and new group names.` above
`ContactGroupRenameBody`; `/// Contact ids, group name, and enable flag.` above
`ContactGroupMembershipBody`; `/// The account's group names.` above
`ContactGroupsListResponse`; `/// The affected group plus the updated list.` above
`ContactGroupNamedListResponse`; `/// The updated list after deletion.` above
`ContactGroupDeleteResponse`; `/// Contact ids in the named group.` above
`ContactGroupMembersResponse`; `/// Number of memberships changed.` above
`MembershipChangedResponse`; `/// A tag name.` above `ThreadTagNameBody`;
`/// Old and new tag names.` above `ThreadTagRenameBody`;
`/// Conversation ids, tag name, and enable flag.` above
`ThreadTagMembershipBody`; `/// The account's tag names.` above
`ThreadTagsListResponse`; `/// The affected tag plus the updated list.` above
`ThreadTagNamedListResponse`; `/// The updated list after deletion.` above
`ThreadTagDeleteResponse`; `/// Conversation ids carrying the named tag.` above
`ThreadTagMembersResponse`.

`import.rs`:

```rust
/// Counters for one import run (staging and promote results).
```
above `ImportStats`.

- [ ] **Step 2: Regenerate the committed OpenAPI document**

Run:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
```

- [ ] **Step 3: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add crates/vault/server/src/api_tokens_api.rs crates/vault/server/src/auth.rs \
  crates/vault/server/src/profile.rs crates/vault/server/src/contacts_api.rs \
  crates/vault/server/src/conversations_api.rs crates/vault/server/src/export_api.rs \
  crates/vault/server/src/server.rs crates/vault/server/src/import.rs \
  docs/src/assets/openapi.json
git commit -m "docs(server): document every ToSchema struct"
```

---

### Task 5: Add the `missing_docs` gate and document the remaining public items

**Files:**
- Modify: `crates/vault/server/src/lib.rs` (add the lint)
- Modify: every file named by a compiler warning in Step 2

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces: a crate that compiles with zero `missing_docs` warnings and has no
  `#[allow(missing_docs)]`.

- [ ] **Step 1: Add the lint**

In `crates/vault/server/src/lib.rs`, change the first line to:

```rust
//! HTTP API and SQLite storage for browsing imported messages.
#![warn(missing_docs)]
```

- [ ] **Step 2: Collect the warnings**

Run: `cargo check -p message-vault-server 2>&1 | grep "missing documentation" | sort -u`

Expected: a list of `file:line` warnings. The known starting inventory (from
the audit heuristic) is:

```
api_tokens_api.rs: ApiTokenItem, ListApiTokensResponse, CreateApiTokenRequest, CreateApiTokenResponse, DeleteApiTokenResponse, RenameApiTokenRequest, RenameApiTokenResponse
assets.rs: AssetStats, StoredAsset
asset_uploads.rs: UploadLimits, UploadManifest, StartUpload
auth.rs: RegisterRequest, LoginRequest, HankoSessionRequest, AuthTokenResponse, ChangePasswordRequest, ChangePasswordResponse, DeleteAccountRequest, DeleteAccountResponse, LogoutResponse
cli.rs: Cli, Commands, clap_command, run
config.rs: Config, ServerConfig, PathsConfig, Config::load, AuthMode, AuthMode::from_env, GuestDemoSettings, GuestDemoSettings::from_env, GuestDemoSettings::disabled
contact_groups_api.rs: GroupError
contacts_api.rs: DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT, ContactListPage, ContactSummary, ContactHandleInfo, ContactHandlePayload, ContactUpdateHandlePayload, ContactRemoveHandlePayload, ContactMutationBody, ContactDetail, ContactSummariesBody, ContactSelectionSummary, ContactSummariesPage
conversations_api.rs: DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT, ConversationListPage, ConversationParticipant, ConversationSummary, ConversationSourceInfo, ConversationSourcesPage
db/account_profile.rs: AccountProfile, is_demo_account, DeletedMessagesStats, guest_status, is_guest_account, insert_guest_account, set_guest_status
db/api_tokens.rs: ApiTokenScopes, ApiTokenScopes::as_str, ApiTokenRow, ApiTokenAuth
db/contacts.rs: ContactLoadStats
db/mod.rs: the seven `pub mod` lines
db/session_tokens.rs: generate_session_token, account_has_session_token, rotate_account_session_token
db/vault_imports.rs: VaultImportRow, VaultImportRow::mode, CompleteImportArgs, CompleteImportArgs::succeeded, CompleteImportArgs::failed, ImportIssueInput, ImportIssueRow, ImportDetail, ImportLookupError, ImportSummary, ImportSummary::mode, list_imports_for_account, TopAttachment
dedupe.rs: DedupeStats
export_api.rs: DEFAULT_EXPORT_LIMIT, MAX_EXPORT_LIMIT, ExportPageOpts, ExportCountOpts, ExportMessagesResponse, ExportCountResponse, ExportMessage, ExportConversation, ExportParticipant, ExportAttachment, ExportTapback, ExportQueryError, ExportQueryError::bad
guest_pool.rs: GuestPoolState, GuestPoolState::new
import_cli.rs: CliImportOptions, CliImportOptions::mode, CliImportStats
import_media.rs: MediaMode, MediaMode::parse, MediaMode::as_str, ResolvedMedia
import.rs: ImportMode, ContactNameMode, ImportMode::as_str, ImportOptions, ImportOptions::mode, FixedImportArgs, FixedImportArgs::mode, ImportStats, ImportStats::mode, ImportExportArgs, ImportExportArgs::mode, ImportSchemaMode, import_jsonl_files
media_tools.rs: JPEG_MIN_BYTES, MP3_MIN_BYTES, MP4_MIN_BYTES, MediaKind, ext_of, kind_of, path_str, tool_on_path, run_ffmpeg
models.rs: ExportRecord, ConversationRecord, ParticipantRecord, MessageRecord, AttachmentRecord, TapbackRecord
openapi.rs: API_TITLE, ApiDoc, SpecAuth, openapi_router
process_assets.rs: ProcessAssetsOptions, ProcessAssetsStats
profile.rs: AccountProfileResponse, ProfileHandleInput, AccountProfileUpdateRequest, DeleteMessagesRequest, DeleteMessagesResponse
reset_demo.rs: ResetDemoStats
search_query.rs: MAX_SEARCH_TEXT_TERMS, DateBounds, DateBounds::is_empty, SearchMode, SearchMode::as_str, ConversationTypeFilter, CountComparator, CountComparator::as_str, CountComparison, GroupBy, SortOrder, FtsNode, ParsedSearchQuery, ParsedSearchQuery::mode, FtsParseError, has_metadata_text_criteria, has_search_criteria
server.rs: AuthCapability, AuthIdentity, AppState, ApiError, AuthModeResponse::mode
thread_tags_api.rs: TagError, tags_for_conversation
```

Note: items already fixed by Tasks 2–4 will not appear in the compiler list —
that is expected; the compiler output is authoritative over this inventory.

- [ ] **Step 3: Document each warning**

For every item the compiler lists, add a doc comment following the style guide:
first sentence states what the item is or does; name exact values for
constants; explain the why for non-obvious choices (e.g. `MAX_LIST_OFFSET`'s
"Cap expensive OFFSET skips" rationale); no `#[allow(missing_docs)]`. Examples
of the expected quality, to copy the tone:

```rust
/// Largest allowed page size for a contact list request.
pub const MAX_LIST_LIMIT: usize = 500;
```

```rust
/// Parse `replace` or `append`; anything else is an error.
pub fn parse(s: &str) -> Result<Self> { ... }
```

For `db/mod.rs`'s seven `pub mod` lines: every `db/` submodule has a `//!`
intro after Task 1, so the compiler will not ask for these; if it does anyway,
add a one-line `///` above each.

For `cli.rs`'s `Cli` and `Commands`: document the struct as the CLI entry
("Command-line entry point parsed from argv.") and the enum as the subcommands
("One subcommand per CLI operation: import, serve, and maintenance."). The
clap-derived `///` docs on fields already cover the fields.

- [ ] **Step 4: Verify zero warnings**

Run: `cargo check -p message-vault-server`
Expected: no `missing documentation` warnings in the output, and no other
warnings introduced.

- [ ] **Step 5: Run the full gate**

Run: `cargo fmt --check && cargo test -p message-vault-server && cargo clippy -p message-vault-server --all-targets -- -D warnings`
Expected: exit 0 for all three.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src
git commit -m "docs(server): gate on missing_docs and document the public surface"
```

---

### Task 6: Shared pagination limits

**Files:**
- Create: `crates/vault/server/src/page_limits.rs`
- Modify: `crates/vault/server/src/lib.rs` (register the module)
- Modify: `crates/vault/server/src/contacts_api.rs:16-19`
- Modify: `crates/vault/server/src/conversations_api.rs:12-15`
- Modify: `crates/vault/server/src/export_api.rs:12-15`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces: `crate::page_limits` constants, re-exported from the three API
  modules under their existing public names so no call site changes.

- [ ] **Step 1: Create `page_limits.rs`**

Create `crates/vault/server/src/page_limits.rs` with exactly:

```rust
//! Pagination limits shared by the list and export endpoints.

/// Default page size for contact and conversation lists.
pub const DEFAULT_LIST_LIMIT: usize = 40;
/// Largest allowed page size for contact lists.
pub const MAX_LIST_LIMIT: usize = 500;
/// Largest allowed page size for conversation lists.
pub const MAX_CONVERSATION_LIST_LIMIT: usize = 100;
/// Cap on expensive OFFSET skips for contact and conversation lists.
pub const MAX_LIST_OFFSET: usize = 50_000;
/// Default page size for message export.
pub const DEFAULT_EXPORT_LIMIT: usize = 100;
/// Largest allowed page size for message export.
pub const MAX_EXPORT_LIMIT: usize = 500;
/// Cap on expensive OFFSET skips for message export (prefer cursor paging).
pub const MAX_EXPORT_OFFSET: usize = 50_000;
```

- [ ] **Step 2: Register the module**

In `crates/vault/server/src/lib.rs`, after `pub mod operation_lock;` add:

```rust
pub(crate) mod page_limits;
```

- [ ] **Step 3: Replace the three duplicated groups**

In `contacts_api.rs`, delete:

```rust
pub const DEFAULT_LIST_LIMIT: usize = 40;
pub const MAX_LIST_LIMIT: usize = 500;
/// Cap expensive OFFSET skips on contact list pages.
pub const MAX_LIST_OFFSET: usize = 50_000;
```

and add in its place:

```rust
pub use crate::page_limits::{DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT, MAX_LIST_OFFSET};
```

In `conversations_api.rs`, delete:

```rust
pub const DEFAULT_LIST_LIMIT: usize = 40;
pub const MAX_LIST_LIMIT: usize = 100;
/// Cap expensive OFFSET skips on conversation list pages.
pub const MAX_LIST_OFFSET: usize = 50_000;
```

and add in its place:

```rust
pub use crate::page_limits::{
    DEFAULT_LIST_LIMIT, MAX_CONVERSATION_LIST_LIMIT as MAX_LIST_LIMIT, MAX_LIST_OFFSET,
};
```

In `export_api.rs`, delete:

```rust
pub const DEFAULT_EXPORT_LIMIT: usize = 100;
pub const MAX_EXPORT_LIMIT: usize = 500;
/// Cap expensive OFFSET skips (prefer cursor pagination for deep pages).
pub const MAX_EXPORT_OFFSET: usize = 50_000;
```

and add in its place:

```rust
pub use crate::page_limits::{DEFAULT_EXPORT_LIMIT, MAX_EXPORT_LIMIT, MAX_EXPORT_OFFSET};
```

- [ ] **Step 4: Verify no duplicates remain**

Run: `rg "pub const (DEFAULT_LIST_LIMIT|MAX_LIST_LIMIT|MAX_LIST_OFFSET|DEFAULT_EXPORT_LIMIT|MAX_EXPORT_LIMIT|MAX_EXPORT_OFFSET)" crates/vault/server/src`
Expected: only `page_limits.rs` matches.

- [ ] **Step 5: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0 (the values are unchanged, so all limit-related tests pass).

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/page_limits.rs crates/vault/server/src/lib.rs \
  crates/vault/server/src/contacts_api.rs crates/vault/server/src/conversations_api.rs \
  crates/vault/server/src/export_api.rs
git commit -m "refactor(server): share pagination limits in one module"
```

---

### Task 7: Move auth-mode and auth-check handlers into `auth.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/auth.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:78-79`

**Interfaces:**
- Consumes: Task 6.
- Produces: `crate::auth::auth_mode_handler`, `crate::auth::auth_check`,
  `crate::auth::AuthModeResponse`, `crate::auth::AuthCheckQuery`,
  `crate::auth::AuthCheckResponse` (all `pub`).

- [ ] **Step 1: Move the items from `server.rs` into `auth.rs`**

Move these items verbatim (bodies unchanged) from `server.rs` into `auth.rs`,
placing them just above the `// ----- Handlers -----` banner comment:

- `AuthModeResponse` (struct + doc added in Task 4)
- `pub(crate) async fn auth_mode_handler`
- `AuthCheckQuery`
- `AuthCheckResponse`
- `pub(crate) async fn auth_check`
- `async fn list_account_sources`
- `async fn lookup_or_resolve_query`
- `async fn load_username`

Adjust imports in `auth.rs`:

- change `use axum::extract::State;` to `use axum::extract::{Query, State};`
- change `use crate::server::{ApiError, AppState, JoinBlocking};` to
  `use crate::server::{ApiError, AppState, JoinBlocking, resolve_auth, with_configured_db};`
- add `use crate::config::AuthMode;` (next to the existing
  `use crate::config::Config;`)

In `server.rs`, delete the moved items. Keep `resolve_account_ref_async`
(used by `resolve_import_account`). Remove any imports that become unused
(`cargo check` will list them; remove each).

Note: `auth_check`'s helpers call `crate::server::with_configured_db` — it is
already `pub(crate)`; no change needed.

- [ ] **Step 2: Repoint the utoipa registrations**

In `crates/vault/server/src/openapi.rs`, change:

```rust
        .routes(routes!(crate::server::auth_mode_handler))
        .routes(routes!(crate::server::auth_check))
```

to:

```rust
        .routes(routes!(crate::auth::auth_mode_handler))
        .routes(routes!(crate::auth::auth_check))
```

- [ ] **Step 3: Fix the server.rs test**

In `server.rs`'s `tests` module, the test `auth_mode_includes_try_demo_flag`
calls `auth_mode_handler(...)` unqualified. Change the call to
`crate::auth::auth_mode_handler(State(state))`. It stays in `server.rs` tests
(its fixture lives there).

- [ ] **Step 4: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/auth.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move auth-mode and auth-check handlers into auth"
```

---

### Task 8: Move account-storage handler into `profile.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/profile.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:86`

**Interfaces:**
- Consumes: Task 7.
- Produces: `crate::profile::account_storage_handler`,
  `crate::profile::AccountStorageResponse`.

- [ ] **Step 1: Widen `with_locked_conn` and move the items**

In `server.rs`, change `async fn with_locked_conn` to
`pub(crate) async fn with_locked_conn` (Tasks 9, 10, and 13 use it too).

Move `AccountStorageResponse` and `pub(crate) async fn account_storage_handler`
verbatim from `server.rs` into `profile.rs`, directly after
`delete_messages_handler` (before the `#[cfg(test)]` block).

Adjust `profile.rs` imports:

- change `use crate::server::{ApiError, AppState, JoinBlocking, require_full_access, resolve_auth};`
  to `use crate::server::{ApiError, AppState, JoinBlocking, require_full_access, resolve_auth, with_locked_conn};`
- add `use std::sync::Arc;` if the compiler asks (the handler clones
  `Arc::clone(&state.db)`).

In `server.rs`, delete the moved items and any imports that become unused.

- [ ] **Step 2: Repoint the registration**

In `openapi.rs`, change `.routes(routes!(crate::server::account_storage_handler))`
to `.routes(routes!(crate::profile::account_storage_handler))`.

- [ ] **Step 3: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/profile.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move account-storage handler into profile"
```

---

### Task 9: Move contact handlers into `contacts_api.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/contacts_api.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:93-96`

**Interfaces:**
- Consumes: Tasks 7–8.
- Produces: `crate::contacts_api::{contacts_list_handler,
  contact_summaries_handler, contact_detail_handler, contact_mutate_handler}`.
- `crate::server::ListPageQuery` stays in `server.rs` (conversations uses it
  too, Task 10).

- [ ] **Step 1: Move the four handlers**

Move verbatim from `server.rs` into `contacts_api.rs`, directly before the
`#[cfg(test)]` block: `contacts_list_handler`, `contact_summaries_handler`,
`contact_detail_handler`, `contact_mutate_handler`. Their `#[utoipa::path]`
attributes and Task 3 doc comments move with them.

Adjust `contacts_api.rs` imports — add:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::HeaderMap;

use crate::server::{ApiError, AppState, JoinBlocking, require_full_access, resolve_auth, with_locked_conn};
```

and keep the existing `rusqlite::Connection` import (the moved
`contact_mutate_handler` body opens the locked connection itself).

The moved code references `ListPageQuery` — change those to
`crate::server::ListPageQuery` (two handlers) — and
`crate::contacts_api::list_contacts` / `MAX_LIST_LIMIT` / `ContactListPage`
can drop their `crate::contacts_api::` prefix now that they are in the module
(the plan leaves that cleanup optional; if kept, the code still compiles).

In `server.rs`, delete the four handlers and remove now-unused imports
(`cargo check` lists them). `ListPageQuery` and
`crate::contacts_api::{DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT}` references in
`server.rs` go away with the handlers.

- [ ] **Step 2: Repoint the registrations**

In `openapi.rs`, change the four lines to:

```rust
        .routes(routes!(crate::contacts_api::contacts_list_handler))
        .routes(routes!(crate::contacts_api::contact_summaries_handler))
        .routes(routes!(crate::contacts_api::contact_detail_handler))
        .routes(routes!(crate::contacts_api::contact_mutate_handler))
```

- [ ] **Step 3: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/contacts_api.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move contact handlers into contacts_api"
```

---

### Task 10: Move conversation handlers into `conversations_api.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/conversations_api.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:109-110`

**Interfaces:**
- Consumes: Task 9.
- Produces: `crate::conversations_api::{conversations_list_handler,
  conversation_sources_handler}`.

- [ ] **Step 1: Move the two handlers**

Move `conversations_list_handler` and `conversation_sources_handler` verbatim
(attributes and docs included) from `server.rs` into `conversations_api.rs`,
directly before the `#[cfg(test)]` block.

Adjust `conversations_api.rs` imports — add:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::HeaderMap;

use crate::server::{ApiError, AppState, JoinBlocking, require_full_access, resolve_auth, with_locked_conn};
```

Reference `crate::server::ListPageQuery` for the query type, and drop the
`crate::conversations_api::` prefixes inside the moved bodies (optional, but
do it — the code reads cleaner). In `server.rs`, delete the moved handlers and
unused imports.

- [ ] **Step 2: Repoint the registrations**

In `openapi.rs`, change the two lines to:

```rust
        .routes(routes!(crate::conversations_api::conversations_list_handler))
        .routes(routes!(crate::conversations_api::conversation_sources_handler))
```

- [ ] **Step 3: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/conversations_api.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move conversation handlers into conversations_api"
```

---

### Task 11: Move export handlers into `export_api.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/export_api.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:91-92`

**Interfaces:**
- Consumes: Task 10.
- Produces: `crate::export_api::{export_messages_handler,
  export_messages_count_handler, ExportMessagesQuery, ExportMessagesCountQuery}`;
  `crate::server::{with_configured_db_map, resolve_import_account}` become
  `pub(crate)`.

- [ ] **Step 1: Widen two server.rs helpers to `pub(crate)`**

In `server.rs`, change `async fn with_configured_db_map` to
`pub(crate) async fn with_configured_db_map` and `async fn resolve_import_account`
to `pub(crate) async fn resolve_import_account` (later tasks use them too).

- [ ] **Step 2: Move the items**

Move `ExportMessagesQuery`, `ExportMessagesCountQuery`,
`export_messages_count_handler`, and `export_messages_handler` verbatim from
`server.rs` into `export_api.rs`, directly before the `#[cfg(test)]` block.

Adjust `export_api.rs` imports — add:

```rust
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::{ApiError, AppState, resolve_auth, require_export_access, resolve_import_account, with_configured_db_map};
```

The handlers reference `DEFAULT_EXPORT_LIMIT` — now in scope via the Task 6
`pub use`. In `server.rs`, delete the moved items; remove the now-unused
`use crate::export_api::{...}` import line at the top of `server.rs` if it
becomes unused (`ExportQueryError` is still needed for the `From` impl — keep
whatever `cargo check` says is still used).

- [ ] **Step 3: Repoint the registrations**

In `openapi.rs`, change the two lines to:

```rust
        .routes(routes!(crate::export_api::export_messages_handler))
        .routes(routes!(crate::export_api::export_messages_count_handler))
```

- [ ] **Step 4: Fix the server.rs test**

In `server.rs`'s tests, `guest_cannot_create_imports_but_can_export_messages`
calls `export_messages_handler(...)` and builds `ExportMessagesQuery {...}`.
Change the call to `crate::export_api::export_messages_handler(...)` and the
struct to `crate::export_api::ExportMessagesQuery {...}`.

- [ ] **Step 5: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/export_api.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move export handlers into export_api"
```

---

### Task 12: Split `import.rs` into `staging`, `promote`, and `contact_name`

**Files:**
- Create: `crates/vault/server/src/import/staging.rs`
- Create: `crates/vault/server/src/import/promote.rs`
- Create: `crates/vault/server/src/import/contact_name.rs`
- Modify: `crates/vault/server/src/import/mod.rs` (was `import.rs`; remove moved
  code; add `mod` declarations and re-exports)

**Interfaces:**
- Consumes: Tasks 1–11.
- Produces: `crate::import::{staging, promote, contact_name}` submodules;
  `crate::import::ContactNameMode`, `crate::import::apply_contact_name_mode`,
  and `crate::import::is_orphaned_export` keep their existing paths via
  re-exports, so `server.rs` and `import_cli.rs` need no changes.

- [ ] **Step 1: Create the directory layout**

```bash
mkdir -p crates/vault/server/src/import
git mv crates/vault/server/src/import.rs crates/vault/server/src/import/mod.rs
```

(`lib.rs` already declares `pub mod import;` and needs no change — a
`import/mod.rs` file satisfies that declaration.)

- [ ] **Step 2: Create `contact_name.rs`**

Create `crates/vault/server/src/import/contact_name.rs`. Header:

```rust
//! Contact linking and display-name merging during import.

use anyhow::Result;
use message_ir::HandleType;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::contacts;
use crate::db::handles::{infer_handle_type_from_shape as infer_handle_type, upsert_handle_row};
use super::ImportStats;
```

Move these items verbatim from `import/mod.rs` into this file, in order:
`ContactNameMode` (enum + `impl ContactNameMode::parse`),
`resolve_incoming_sender_handle`, `ensure_sibling_contact_link`,
`seed_contact_handle_alias`, `contact_preferred_name`, `trim_nonempty`,
`apply_contact_name_mode`.

Visibility changes on the moved items:

- `ContactNameMode` and `apply_contact_name_mode`: keep `pub` (re-exported
  from the parent below).
- `resolve_incoming_sender_handle`: `pub(super) fn` (used by `staging`).
- `ensure_sibling_contact_link`: `pub(super) fn` (used by `staging`).
- `seed_contact_handle_alias`: `pub(super) fn` (used by `staging`).
- `contact_preferred_name`: private.
- `trim_nonempty`: `pub(super) fn` (used by `import/mod.rs` tests).

`resolve_incoming_sender_handle` takes `tx: &Transaction<'_>` — add
`use rusqlite::Transaction;` to the imports above.

- [ ] **Step 3: Create `staging.rs`**

Create `crates/vault/server/src/import/staging.rs`. Header:

```rust
//! Stage message-ir JSONL rows into the temporary import tables.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use message_ir::HandleService;
use rusqlite::{Transaction, params};

use crate::assets::{self, AssetStats, StoredAsset};
use crate::config::{PathsConfig, validate_source_id};
use crate::db::handles::{infer_handle_type_from_shape as infer_handle_type, upsert_handle_row};
use crate::import_media::{self, MediaMode};
use crate::jsonl;
use crate::models::{AttachmentRecord, ExportRecord, MessageRecord, clean_body};

use super::contact_name::{
    apply_contact_name_mode, contact_preferred_name, ensure_sibling_contact_link,
    resolve_incoming_sender_handle, seed_contact_handle_alias,
};
use super::{ImportOptions, ImportStats, PreparedAttachment};
```

Move these items verbatim from `import/mod.rs`, in order:
`PreparedAttachment` (struct), `nonempty_rel`, `nonempty_str`,
`stored_size_bytes`, `try_store_converted`, `store_claimed_or_path`,
`prepare_attachments`, `StagingInserts` (+ `impl`), `ConversationHeader`
(type), `resolve_conversation_source`, `assets_dir_for_source`,
`is_orphaned_export`, `import_file_to_staging`, `ImportConversationArgs`,
`import_conversation_to_staging`.

Visibility changes:

- `is_orphaned_export`: keep `pub` (re-exported from the parent below).
- `import_file_to_staging`: `pub(super) fn` (called by
  `import_jsonl_files_on_conn`).
- everything else private as it is today.

- [ ] **Step 4: Create `promote.rs`**

Create `crates/vault/server/src/import/promote.rs`. Header:

```rust
//! Copy staged import rows into the production tables.

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;

use anyhow::{Result, bail};
use rusqlite::{Transaction, params, params_from_iter};

use crate::db::sql::{SQLITE_IN_CHUNK, pair_placeholders};
use crate::db::schema;

use super::ImportMode;
```

Move these items verbatim from `import/mod.rs`, in order: `PromoteStats`,
`promote_append`, `PROMOTE_MESSAGE_BATCH`, `PROMOTE_INDEX_DROP_MIN_STAGING`,
`promote_log`, `promote_phase_done`,
`should_drop_messages_secondary_indexes`, `promote_messages_chunked`,
`promote_messages_replace_chunked`, `promote_messages_append_chunked`,
`zip_new_message_ids`, `fill_promote_msg_map`.

Visibility changes:

- `promote_append`: `pub(super) fn` (called by `import_jsonl_files_on_conn`).
- `zip_new_message_ids`: private (its test moves here, see Step 6).
- everything else private.

- [ ] **Step 5: Trim `import/mod.rs`**

In `import/mod.rs`:

- delete every item moved in Steps 2–4;
- add at the top, after the `//!` intro:

```rust
pub mod contact_name;
pub mod promote;
pub mod staging;

pub use contact_name::{apply_contact_name_mode, ContactNameMode};
pub use staging::is_orphaned_export;
```

- in `import_jsonl_files_on_conn`, change `import_file_to_staging(...)` to
  `staging::import_file_to_staging(...)` and `promote_append(...)` to
  `promote::promote_append(...)`;
- fix the now-unused imports (`cargo check` lists them; remove each). The
  file should end up importing only what it still uses (`io::Write` stays for
  the `flush` calls, `Instant` stays for progress timing, `jsonl`/`models`
  imports move away, `assets` moves away, `import_media` moves away).

- [ ] **Step 6: Move the tests that belong to submodules**

- Move `apply_contact_name_mode_unit`, `seed_contact_handle_alias_unit_first_wins`,
  and `sibling_contact_link_bumps_last_modified_only_on_insert` into a
  `#[cfg(test)] mod tests` at the bottom of `contact_name.rs`. Give it
  `use super::*;` plus whatever the tests already used (`rusqlite::params`,
  `crate::db::schema`, `crate::db::account_profile`, `TEST_ACCOUNT` — move the
  `const TEST_ACCOUNT` line into this tests module).
- Move `promote_message_map_ignores_other_accounts` into a `#[cfg(test)] mod
  tests` at the bottom of `promote.rs` (`use super::*;` + `TEST_ACCOUNT`).
- The remaining tests in `import/mod.rs` stay; they call
  `crate::import::import_jsonl_files`, which still exists.

- [ ] **Step 7: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0; the import tests pass unchanged (they exercise behavior,
not module layout).

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server/src/import
git commit -m "refactor(server): split import into staging, promote, and contact_name"
```

---

### Task 13: Move import HTTP handlers from `server.rs` into `import`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code; widen
  helpers to `pub(crate)`)
- Modify: `crates/vault/server/src/import/mod.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:111-115`

**Interfaces:**
- Consumes: Task 12.
- Produces: `crate::import::{imports_list_handler, imports_create_handler,
  imports_get_handler, imports_complete_handler, import_handler,
  ImportResponse, DedupeResponse, ...}` and `pub(crate)` server helpers
  `resolve_import_account` (done in Task 11), `content_type_base`,
  `is_jsonl_content_type`, `is_multipart_content_type`, `upload_content_type`,
  `safe_rel_path`, `stream_body_to_file`, `stream_field_to_file`,
  `create_dest_file`, `lock_conn`, `lock_import_conn`.

- [ ] **Step 1: Widen the shared helpers in `server.rs`**

Change these `server.rs` functions from private to `pub(crate)`:
`content_type_base`, `is_jsonl_content_type`, `is_multipart_content_type`,
`upload_content_type`, `safe_rel_path`, `stream_body_to_file`,
`stream_field_to_file`, `create_dest_file`, `lock_conn`, `lock_import_conn`.
(`resolve_import_account` and `with_configured_db_map` were widened in Task 11.)

- [ ] **Step 2: Move the handler group**

Move these items verbatim from `server.rs` into `import/mod.rs`, placed after
`import_jsonl_files_on_conn` and before the `#[cfg(test)]` block:
`ImportQuery`, `default_contact_name_mode`, `default_import_mode`,
`ImportResponse`, `DedupeResponse`, `CreateImportBody`, `CreateImportResponse`,
`CompleteImportBody`, `default_true`, `CompleteImportIssueBody`,
`validate_complete_import_issues`, `CompleteImportResponse`, `ListImportsQuery`,
`ImportsListResponse`, `ImportDetailIssueResponse`, `ImportDetailResponse`,
`imports_list_handler`, `imports_create_handler`, `imports_get_handler`,
`imports_complete_handler`, `parse_summary_json`, `import_detail_response`,
`import_handler`, `import_multipart`, `run_import_path`.

Adjust `import/mod.rs` imports — add:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::HeaderMap;

use crate::db::account_profile;
use crate::dedupe;
use crate::server::{
    ApiError, AppState, JoinBlocking, create_dest_file, content_type_base,
    is_jsonl_content_type, is_multipart_content_type, lock_conn, lock_import_conn,
    reject_if_guest_account, require_import_access, resolve_auth, resolve_import_account,
    safe_rel_path, stream_body_to_file, stream_field_to_file, with_configured_db_map,
    with_locked_conn,
};
```

Inside the moved bodies, `stream_body_to_file` and the other helpers are now
in scope directly (the `crate::server::` prefix is optional). `schema`,
`vault_imports`, `validate_source_id`, `ImportMode` are already imported by
`import/mod.rs`. In `server.rs`, delete the moved items; `futures_util` and
`tokio::io::AsyncWriteExt` imports may become unused — remove what
`cargo check` flags. `ErrorBody` stays in `server.rs` (Task 4 doc included).

- [ ] **Step 3: Repoint the registrations**

In `openapi.rs`, change the five lines to:

```rust
        .routes(routes!(crate::import::imports_list_handler))
        .routes(routes!(crate::import::imports_create_handler))
        .routes(routes!(crate::import::imports_get_handler))
        .routes(routes!(crate::import::imports_complete_handler))
        .routes(routes!(crate::import::import_handler))
```

- [ ] **Step 4: Fix the server.rs tests**

Five tests in `server.rs`'s tests module call the moved handlers directly.
Add at the top of the tests module (after `use super::*;`):

```rust
    use crate::import::{
        CompleteImportBody, CreateImportBody, imports_complete_handler, imports_create_handler,
        imports_get_handler,
    };
```

Then:

- in `guest_cannot_create_imports_but_can_export_messages`, change the
  `export_messages_handler(` call to
  `crate::export_api::export_messages_handler(` and `Query(ExportMessagesQuery {`
  to `Query(crate::export_api::ExportMessagesQuery {`;
- `guest_cannot_complete_imports`, `imports_complete_and_detail_surface_timings_and_issues`,
  `imports_complete_rejects_invalid_issue_kind_before_db_write`, and
  `imports_get_handler_returns_not_found_for_missing_import` then resolve the
  handler names from the import above.

- [ ] **Step 5: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0. (`openapi.json` does not change — no doc text changed.)

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/import/mod.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move import handlers into import"
```

---

### Task 14: Move asset handlers into `assets.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code; widen
  `read_body_limited` / `discard_body`)
- Modify: `crates/vault/server/src/assets.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:116-122`

**Interfaces:**
- Consumes: Task 13.
- Produces: `crate::assets::{asset_head_handler, asset_get_handler,
  asset_put_handler, asset_upload_start_handler, asset_upload_part_handler,
  asset_upload_complete_handler, asset_upload_abort_handler}`.

- [ ] **Step 1: Widen two helpers in `server.rs`**

Change `async fn read_body_limited` to `pub(crate) async fn read_body_limited`
and `async fn discard_body` to `pub(crate) async fn discard_body`.

- [ ] **Step 2: Move the handler group**

Move verbatim from `server.rs` into `assets.rs`, directly before the
`#[cfg(test)]` block: `AssetPutQuery`, `AssetPutResponse` (+ its `impl`
block with `stored`), `AssetAccess`, `resolve_asset_lookup`,
`asset_head_handler`, `asset_get_handler`, `asset_put_handler`,
`AssetUploadStartBody`, `AssetUploadStartResponse`, `AssetUploadPartResponse`,
`AssetUploadAbortResponse`, `asset_upload_start_handler`,
`asset_upload_part_handler`, `asset_upload_complete_handler`,
`asset_upload_abort_handler`.

Adjust `assets.rs` imports — add:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;

use crate::asset_uploads;
use crate::config::validate_source_id;
use crate::server::{
    ApiError, AppState, discard_body, read_body_limited, reject_if_guest_account,
    require_export_access, require_import_access, require_import_or_export_access,
    resolve_auth, resolve_import_account, stream_body_to_file,
};
```

Note the naming collision: `assets.rs` already has private fns
`hash_file` etc. — none collide with the moved names. `resolve_asset_lookup`
calls `assets::lookup_by_sha256` / `lookup_by_sha256_unverified` — drop the
`assets::` prefix inside the moved bodies (same module now).

In `server.rs`, delete the moved items and the now-unused imports
(`futures_util`, `AsyncWriteExt`, `Multipart`, `FromRequest` may go — remove
exactly what `cargo check` flags).

- [ ] **Step 3: Repoint the registrations**

In `openapi.rs`, change the seven lines to:

```rust
        .routes(routes!(crate::assets::asset_head_handler))
        .routes(routes!(crate::assets::asset_get_handler))
        .routes(routes!(crate::assets::asset_put_handler))
        .routes(routes!(crate::assets::asset_upload_start_handler))
        .routes(routes!(crate::assets::asset_upload_part_handler))
        .routes(routes!(crate::assets::asset_upload_complete_handler))
        .routes(routes!(crate::assets::asset_upload_abort_handler))
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/assets.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move asset handlers into assets"
```

---

### Task 15: Move contact-group handlers into `contact_groups_api.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/contact_groups_api.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:97-102`

**Interfaces:**
- Consumes: Tasks 13–14.
- Produces: `crate::contact_groups_api::{contact_groups_list_handler, …,
  contact_groups_membership_handler}`.
- `crate::server::MembershipChangedResponse` stays in `server.rs` (thread
  tags share it, Task 16).

- [ ] **Step 1: Move the handler group**

Move verbatim from `server.rs` into `contact_groups_api.rs`, directly before
the `#[cfg(test)]` block: `ContactGroupNameBody`, `ContactGroupRenameBody`,
`ContactGroupMembersQuery`, `ContactGroupMembershipBody`,
`ContactGroupsListResponse`, `ContactGroupNamedListResponse`,
`ContactGroupDeleteResponse`, `ContactGroupMembersResponse`,
`map_group_error`, `with_group_conn`, `contact_groups_list_handler`,
`contact_groups_create_handler`, `contact_groups_rename_handler`,
`contact_groups_delete_handler`, `contact_groups_members_handler`,
`contact_groups_membership_handler`.

Adjust `contact_groups_api.rs` imports — add:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::{
    ApiError, AppState, JoinBlocking, MembershipChangedResponse, require_full_access,
    resolve_auth,
};
```

(`lock_conn` is already `pub(crate)` from Task 13 — `with_group_conn` uses
`crate::server::lock_conn`; add it to the import list.)

The moved bodies call `crate::contact_groups_api::list_groups` etc. — drop the
`crate::contact_groups_api::` prefixes (same module now). In `server.rs`,
delete the moved items and unused imports.

- [ ] **Step 2: Repoint the registrations**

In `openapi.rs`, change the six lines to:

```rust
        .routes(routes!(crate::contact_groups_api::contact_groups_list_handler))
        .routes(routes!(crate::contact_groups_api::contact_groups_create_handler))
        .routes(routes!(crate::contact_groups_api::contact_groups_rename_handler))
        .routes(routes!(crate::contact_groups_api::contact_groups_delete_handler))
        .routes(routes!(crate::contact_groups_api::contact_groups_members_handler))
        .routes(routes!(crate::contact_groups_api::contact_groups_membership_handler))
```

- [ ] **Step 3: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/contact_groups_api.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move contact-group handlers into contact_groups_api"
```

---

### Task 16: Move thread-tag handlers into `thread_tags_api.rs`

**Files:**
- Modify: `crates/vault/server/src/server.rs` (remove moved code)
- Modify: `crates/vault/server/src/thread_tags_api.rs` (receive moved code)
- Modify: `crates/vault/server/src/openapi.rs:103-108`

**Interfaces:**
- Consumes: Task 15.
- Produces: `crate::thread_tags_api::{thread_tags_list_handler, …,
  thread_tags_membership_handler}`.

- [ ] **Step 1: Move the handler group**

Move verbatim from `server.rs` into `thread_tags_api.rs`, directly before the
`#[cfg(test)]` block: `ThreadTagNameBody`, `ThreadTagRenameBody`,
`ThreadTagMembersQuery`, `ThreadTagMembershipBody`, `ThreadTagsListResponse`,
`ThreadTagNamedListResponse`, `ThreadTagDeleteResponse`,
`ThreadTagMembersResponse`, `map_tag_error`, `with_tag_conn`,
`thread_tags_list_handler`, `thread_tags_create_handler`,
`thread_tags_rename_handler`, `thread_tags_delete_handler`,
`thread_tags_members_handler`, `thread_tags_membership_handler`.

Adjust `thread_tags_api.rs` imports — add:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::{
    ApiError, AppState, JoinBlocking, MembershipChangedResponse, lock_conn,
    require_full_access, resolve_auth,
};
```

Drop the `crate::thread_tags_api::` prefixes inside the moved bodies. In
`server.rs`, delete the moved items and unused imports.

- [ ] **Step 2: Repoint the registrations**

In `openapi.rs`, change the six lines to:

```rust
        .routes(routes!(crate::thread_tags_api::thread_tags_list_handler))
        .routes(routes!(crate::thread_tags_api::thread_tags_create_handler))
        .routes(routes!(crate::thread_tags_api::thread_tags_rename_handler))
        .routes(routes!(crate::thread_tags_api::thread_tags_delete_handler))
        .routes(routes!(crate::thread_tags_api::thread_tags_members_handler))
        .routes(routes!(crate::thread_tags_api::thread_tags_membership_handler))
```

- [ ] **Step 3: Verify and commit**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0.

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/thread_tags_api.rs \
  crates/vault/server/src/openapi.rs
git commit -m "refactor(server): move thread-tag handlers into thread_tags_api"
```

---

### Task 17: Shared named-membership CRUD helper

**Files:**
- Create: `crates/vault/server/src/named_membership.rs`
- Modify: `crates/vault/server/src/lib.rs` (register the module)
- Modify: `crates/vault/server/src/thread_tags_api.rs` (thin wrappers)
- Modify: `crates/vault/server/src/contact_groups_api.rs` (thin wrappers)

**Interfaces:**
- Consumes: Task 16.
- Produces: `crate::named_membership::{MembershipError, MembershipSpec,
  MAX_NAME_LEN, tag_spec, group_spec, is_reserved, list_names, create_name,
  rename_name, delete_name, list_member_ids, set_membership}`. The public
  function names and error messages of both API modules are unchanged.

- [ ] **Step 1: Create `named_membership.rs`**

Create `crates/vault/server/src/named_membership.rs` with exactly:

```rust
//! Shared CRUD for named membership sets (thread tags and contact groups).
//!
//! Both domains store a named set (rows in a names table) whose members are
//! conversation or contact ids. The operations are identical apart from table
//! and column names, reserved names, and one post-change hook, so this module
//! implements them once behind [`MembershipSpec`].

use anyhow::Result as AnyResult;
use rusqlite::{Connection, OptionalExtension, params};

/// Longest allowed name for either kind of set (characters).
pub const MAX_NAME_LEN: usize = 80;

/// Create / rename / delete / membership failures for a named set.
#[derive(Debug)]
pub enum MembershipError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl From<rusqlite::Error> for MembershipError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Table names, labels, reserved names, and messages for one named set.
///
/// `name_column` and `member_column` live on the membership table;
/// `member_table` is the table members must exist in. All values are compile
/// time constants, so the SQL built from them is fixed at build time.
pub struct MembershipSpec {
    /// Names table (`conversation_tags` / `contact_groups`).
    pub table: &'static str,
    /// Membership table (`conversation_tag_members` / `contact_group_members`).
    pub members_table: &'static str,
    /// Column on the membership table that references the names table.
    pub name_column: &'static str,
    /// Column on the membership table that holds the member id.
    pub member_column: &'static str,
    /// Table members must exist in (`conversations` / `contacts`).
    pub member_table: &'static str,
    /// Singular label used in error messages (`"tag"` / `"group"`).
    pub label: &'static str,
    /// Member label used in error messages (`"conversation"` / `"contact"`).
    pub member_label: &'static str,
    /// Names that must not be created.
    pub reserved: &'static [&'static str],
    /// Reserved names with dedicated error messages (lowercase name, message).
    pub special_reserved: &'static [(&'static str, &'static str)],
    /// Extra work after a membership change (groups touch the contact row).
    pub on_change: Option<fn(&Connection, &str, i64) -> AnyResult<()>>,
}

/// Thread tags on conversations.
pub fn tag_spec() -> &'static MembershipSpec {
    static SPEC: MembershipSpec = MembershipSpec {
        table: "conversation_tags",
        members_table: "conversation_tag_members",
        name_column: "tag_id",
        member_column: "conversation_id",
        member_table: "conversations",
        label: "tag",
        member_label: "conversation",
        max_name_len: MAX_NAME_LEN,
        reserved: &[
            "home",
            "contacts",
            "threads",
            "thread",
            "all",
            "excluded",
            "unassigned",
            "trash",
            "tags",
            "tag",
            "no-tag",
            "no tag",
            "groups",
            "group",
            "labels",
            "label",
        ],
        special_reserved: &[],
        on_change: None,
    };
    &SPEC
}

/// Contact groups on contacts.
pub fn group_spec() -> &'static MembershipSpec {
    static SPEC: MembershipSpec = MembershipSpec {
        table: "contact_groups",
        members_table: "contact_group_members",
        name_column: "group_id",
        member_column: "contact_id",
        member_table: "contacts",
        label: "group",
        member_label: "contact",
        max_name_len: MAX_NAME_LEN,
        reserved: &[
            "home",
            "contacts",
            "all",
            "excluded",
            "no-messages",
            "no messages",
            "unassigned",
            "trash",
            "groups",
            "group",
            "group-chats",
            "group chats",
            "group-chats-2",
            "group chats 2",
            "group-messages",
            "group messages",
            "group-messages-2",
            "group messages 2",
            "no-label",
            "no-group",
            "no group",
            "labels",
            "label",
            "no label",
        ],
        special_reserved: &[
            ("contacts", "Contacts is a reserved group"),
            ("all", "All is a reserved group"),
            ("excluded", "Excluded is a reserved group"),
            ("unassigned", "Unassigned is a reserved group"),
            ("trash", "Trash is a reserved group"),
            ("no messages", "No messages is a reserved group"),
            ("no-messages", "No messages is a reserved group"),
            ("groups", "Group Messages is a reserved name"),
            ("group", "Group Messages is a reserved name"),
            ("group chats", "Group Messages is a reserved name"),
            ("group-chats", "Group Messages is a reserved name"),
            ("group chats 2", "Group Messages is a reserved name"),
            ("group-chats-2", "Group Messages is a reserved name"),
            ("group messages", "Group Messages is a reserved name"),
            ("group-messages", "Group Messages is a reserved name"),
            ("group messages 2", "Group Messages is a reserved name"),
            ("group-messages-2", "Group Messages is a reserved name"),
        ],
        on_change: Some(touch_member_owner),
    };
    &SPEC
}

fn touch_member_owner(conn: &Connection, account_id: &str, member_id: i64) -> AnyResult<()> {
    crate::db::contacts::touch_contact(conn, account_id, member_id)
}

fn find_id(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Option<i64>, MembershipError> {
    let sql = format!(
        "SELECT id FROM {table} WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE",
        table = spec.table
    );
    let id = conn
        .query_row(&sql, params![account_id, name], |row| row.get(0))
        .optional()?;
    Ok(id)
}

fn ensure_id(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<i64, MembershipError> {
    let name = normalize_name(spec, name)?;
    let sql = format!(
        "INSERT OR IGNORE INTO {table} (account_id, name) VALUES (?1, ?2)",
        table = spec.table
    );
    conn.execute(&sql, params![account_id, name])?;
    find_id(spec, conn, account_id, &name)?
        .ok_or_else(|| MembershipError::Internal(format!("failed to ensure {} {name}", spec.label)))
}

/// True when `name` is reserved and must not be created.
pub fn is_reserved(spec: &MembershipSpec, name: &str) -> bool {
    let key = name.trim().to_ascii_lowercase();
    spec.reserved.contains(&key.as_str())
}

fn reserved_error(spec: &MembershipSpec, name: &str) -> String {
    let key = name.trim().to_ascii_lowercase();
    for (reserved, message) in spec.special_reserved {
        if key == *reserved {
            return (*message).to_string();
        }
    }
    format!("\"{}\" is a reserved {}", name.trim(), spec.label)
}

fn normalize_name(spec: &MembershipSpec, name: &str) -> Result<String, MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    if trimmed.chars().count() > spec.max_name_len {
        return Err(MembershipError::BadRequest(format!(
            "name must be at most {} characters",
            spec.max_name_len
        )));
    }
    if is_reserved(spec, trimmed) {
        return Err(MembershipError::BadRequest(reserved_error(spec, trimmed)));
    }
    Ok(trimmed.to_string())
}

/// Names for this account, A–Z, excluding reserved leftovers.
pub fn list_names(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
) -> Result<Vec<String>, MembershipError> {
    let sql = format!(
        "SELECT name FROM {table} WHERE account_id = ?1 ORDER BY name COLLATE NOCASE",
        table = spec.table
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let name = row?;
        if !is_reserved(spec, &name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Create a name. Fails when the name is taken (ignoring case).
pub fn create_name(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<String, MembershipError> {
    let name = normalize_name(spec, name)?;
    if find_id(spec, conn, account_id, &name)?.is_some() {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "INSERT INTO {table} (account_id, name) VALUES (?1, ?2)",
        table = spec.table
    );
    conn.execute(&sql, params![account_id, name])?;
    Ok(name)
}

/// Rename a name. Allows a case-only change of the same name.
pub fn rename_name(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, MembershipError> {
    let old_name = from.trim();
    if old_name.is_empty() {
        return Err(MembershipError::BadRequest("from and to required".into()));
    }
    let new_name = normalize_name(spec, to)?;
    let Some(id) = find_id(spec, conn, account_id, old_name)? else {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    };
    if old_name.eq_ignore_ascii_case(&new_name) {
        if old_name == new_name {
            return Ok(new_name);
        }
    } else if let Some(other) = find_id(spec, conn, account_id, &new_name)?
        && other != id
    {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "UPDATE {table} SET name = ?1 WHERE id = ?2 AND account_id = ?3",
        table = spec.table
    );
    conn.execute(&sql, params![new_name, id, account_id])?;
    Ok(new_name)
}

/// Delete a name and its memberships.
pub fn delete_name(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<(), MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    let Some(id) = find_id(spec, conn, account_id, trimmed)? else {
        return Err(MembershipError::NotFound(format!(
            "{} not found",
            spec.label
        )));
    };
    let members_sql = format!(
        "DELETE FROM {mt} WHERE {nc} = ?1",
        mt = spec.members_table,
        nc = spec.name_column
    );
    conn.execute(&members_sql, params![id])?;
    let sql = format!(
        "DELETE FROM {table} WHERE id = ?1 AND account_id = ?2",
        table = spec.table
    );
    conn.execute(&sql, params![id, account_id])?;
    Ok(())
}

/// Member ids that currently belong to a named set (case-insensitive).
pub fn list_member_ids(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, MembershipError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MembershipError::BadRequest("name required".into()));
    }
    let sql = format!(
        "SELECT m.{mc}
         FROM {mt} m
         JOIN {table} n ON n.id = m.{nc}
         WHERE n.account_id = ?1 AND n.name = ?2 COLLATE NOCASE
         ORDER BY m.{mc}",
        mc = spec.member_column,
        mt = spec.members_table,
        table = spec.table,
        nc = spec.name_column,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id, trimmed], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn member_exists(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    member_id: i64,
) -> Result<bool, MembershipError> {
    let sql = format!(
        "SELECT id FROM {mt} WHERE id = ?1 AND account_id = ?2",
        mt = spec.member_table
    );
    let found: Option<i64> = conn
        .query_row(&sql, params![member_id, account_id], |row| row.get(0))
        .optional()?;
    Ok(found.is_some())
}

/// Add or remove one name for many members. Creates the name when enabling.
pub fn set_membership(
    spec: &MembershipSpec,
    conn: &Connection,
    account_id: &str,
    member_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, MembershipError> {
    let mut ids: Vec<i64> = member_ids.iter().copied().filter(|id| *id > 0).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(MembershipError::BadRequest(format!(
            "{} ids required",
            spec.member_label
        )));
    }
    let name_trimmed = name.trim();
    if name_trimmed.is_empty() {
        return Err(MembershipError::BadRequest(format!(
            "{} name required",
            spec.label
        )));
    }
    if is_reserved(spec, name_trimmed) {
        return Err(MembershipError::BadRequest(reserved_error(spec, name_trimmed)));
    }

    for id in &ids {
        if !member_exists(spec, conn, account_id, *id)? {
            return Err(MembershipError::NotFound(format!(
                "{} {id} not found",
                spec.member_label
            )));
        }
    }

    let name_row_id = if enable {
        ensure_id(spec, conn, account_id, name_trimmed)?
    } else {
        match find_id(spec, conn, account_id, name_trimmed)? {
            Some(id) => id,
            None => return Ok(0),
        }
    };

    let mut changed = 0u64;
    for id in ids {
        let n = if enable {
            let sql = format!(
                "INSERT OR IGNORE INTO {mt} ({mc}, {nc})
                 SELECT id, ?1 FROM {member_table} WHERE id = ?2 AND account_id = ?3",
                mt = spec.members_table,
                mc = spec.member_column,
                nc = spec.name_column,
                member_table = spec.member_table,
            );
            conn.execute(&sql, params![name_row_id, id, account_id])?
        } else {
            let sql = format!(
                "DELETE FROM {mt}
                 WHERE {mc} = ?1 AND {nc} = ?2
                   AND EXISTS (
                     SELECT 1 FROM {member_table}
                     WHERE {member_table}.id = {mt}.{mc}
                       AND {member_table}.account_id = ?3
                   )",
                mt = spec.members_table,
                mc = spec.member_column,
                nc = spec.name_column,
                member_table = spec.member_table,
            );
            conn.execute(&sql, params![id, name_row_id, account_id])?
        };
        if n > 0 {
            changed += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, id)
                    .map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    use crate::db::schema;

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        let account = "00000000-0000-4000-8000-0000000000d9".to_string();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![&account],
        )
        .unwrap();
        (conn, account)
    }

    #[test]
    fn reserved_names_rejected_with_exact_messages() {
        let (conn, account) = setup();
        let err = create_name(tag_spec(), &conn, &account, "Trash").unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "\"Trash\" is a reserved tag"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_name(group_spec(), &conn, &account, "Trash").unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => assert_eq!(msg, "Trash is a reserved group"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        let err = create_name(group_spec(), &conn, &account, "Group Chats").unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => {
                assert_eq!(msg, "Group Messages is a reserved name")
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn names_over_max_len_rejected() {
        let (conn, account) = setup();
        let long = "x".repeat(MAX_NAME_LEN + 1);
        let err = create_name(tag_spec(), &conn, &account, &long).unwrap_err();
        match err {
            MembershipError::BadRequest(msg) => {
                assert_eq!(msg, format!("name must be at most {MAX_NAME_LEN} characters"))
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn on_change_hook_runs_on_membership_change() {
        let (conn, account) = setup();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Ada')",
            params![&account],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE contacts SET last_modified = '2000-01-01 00:00:00' WHERE id = ?1",
            params![contact_id],
        )
        .unwrap();

        assert_eq!(
            set_membership(group_spec(), &conn, &account, &[contact_id], "Family", true).unwrap(),
            1
        );
        let after: String = conn
            .query_row(
                "SELECT last_modified FROM contacts WHERE id = ?1",
                params![contact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(after, "2000-01-01 00:00:00", "group change must touch the contact");
    }
}
```

Wait — one correction to `tag_spec`'s `reserved` list vs the original
`RESERVED_TAG_NAMES`: the original is exactly the 16 names listed; the spec
list above matches it. The original `RESERVED_GROUP_NAMES` is 24 names; the
spec list above matches it.

- [ ] **Step 2: Register the module**

In `crates/vault/server/src/lib.rs`, after `pub mod media_tools;` add:

```rust
pub(crate) mod named_membership;
```

- [ ] **Step 3: Rewrite `thread_tags_api.rs` as thin wrappers**

Replace the file's CRUD section. The file becomes:

```rust
//! Thread tags stored in `conversation_tags` / `conversation_tag_members`.

use std::collections::HashMap;

use anyhow::Result as AnyResult;
use rusqlite::{Connection, params, params_from_iter};

use crate::db::sql::{fold_in_id_chunks, in_placeholders};
use crate::named_membership::{self, MembershipError, MembershipSpec, tag_spec};

/// Create / rename / delete / membership failures.
pub type TagError = MembershipError;

/// Longest allowed tag name (characters).
pub use crate::named_membership::MAX_NAME_LEN as MAX_TAG_NAME_LEN;

/// True when `name` is reserved and must not be created.
pub fn is_reserved_tag_name(name: &str) -> bool {
    named_membership::is_reserved(tag_spec(), name)
}

/// Tag names for this account, A–Z, excluding reserved leftovers.
pub fn list_tags(conn: &Connection, account_id: &str) -> Result<Vec<String>, TagError> {
    named_membership::list_names(tag_spec(), conn, account_id)
}

/// Create a tag. Fails when the name is taken (ignoring case).
pub fn create_tag(conn: &Connection, account_id: &str, name: &str) -> Result<String, TagError> {
    named_membership::create_name(tag_spec(), conn, account_id, name)
}

/// Rename a tag. Allows a case-only change of the same name.
pub fn rename_tag(
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, TagError> {
    named_membership::rename_name(tag_spec(), conn, account_id, from, to)
}

/// Delete a tag and its memberships.
pub fn delete_tag(conn: &Connection, account_id: &str, name: &str) -> Result<(), TagError> {
    named_membership::delete_name(tag_spec(), conn, account_id, name)
}

/// Conversation ids that currently have a named tag (case-insensitive).
pub fn list_tag_member_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, TagError> {
    named_membership::list_member_ids(tag_spec(), conn, account_id, name)
}

/// Add or remove one tag for many conversations. Creates the tag when enabling.
pub fn set_conversations_tag_membership(
    conn: &Connection,
    account_id: &str,
    conversation_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, TagError> {
    named_membership::set_membership(tag_spec(), conn, account_id, conversation_ids, name, enable)
}

/// Tags on one conversation, A–Z.
#[cfg(test)]
pub(crate) fn tags_for_conversation(
    conn: &Connection,
    account_id: &str,
    conversation_id: i64,
) -> AnyResult<Vec<String>> {
    // body unchanged from today
}
```

In the concrete edit: keep `tags_for_conversation`'s body exactly as it is
today (only its visibility line changes from `#[cfg(test)] pub fn` to
`#[cfg(test)] pub(crate) fn`), keep `tags_for_conversations` unchanged, and
delete everything the wrappers replace: `MAX_TAG_NAME_LEN`,
`RESERVED_TAG_NAMES`, `TagError` + its `From` impl, `reserved_tag_error`,
`normalize_new_name`, `find_tag_id`, `ensure_tag_id`, `create_tag`,
`rename_tag`, `delete_tag`, `list_tag_member_ids`, `conversation_exists`,
`set_conversations_tag_membership`, and `list_tags`'s old body. Remove the
now-unused `OptionalExtension` import.

The tests module at the bottom stays as-is (it matches `TagError::…`
variants through the alias, which works).

- [ ] **Step 4: Rewrite `contact_groups_api.rs` as thin wrappers**

The file becomes:

```rust
//! Contact groups stored in `contact_groups` / `contact_group_members`.

use anyhow::Result as AnyResult;
use rusqlite::{Connection, params};

use crate::named_membership::{self, MembershipError, group_spec};

/// Create / rename / delete / membership failures.
pub type GroupError = MembershipError;

/// Longest allowed group name (characters).
pub use crate::named_membership::MAX_NAME_LEN as MAX_GROUP_NAME_LEN;

/// True when `name` is reserved and must not be created.
pub fn is_reserved_group_name(name: &str) -> bool {
    named_membership::is_reserved(group_spec(), name)
}

/// Group names for this account, A–Z, excluding reserved leftovers.
pub fn list_groups(conn: &Connection, account_id: &str) -> Result<Vec<String>, GroupError> {
    named_membership::list_names(group_spec(), conn, account_id)
}

/// Create a group. Fails when the name is taken (ignoring case).
pub fn create_group(conn: &Connection, account_id: &str, name: &str) -> Result<String, GroupError> {
    named_membership::create_name(group_spec(), conn, account_id, name)
}

/// Rename a group. Allows a case-only change of the same name.
pub fn rename_group(
    conn: &Connection,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<String, GroupError> {
    named_membership::rename_name(group_spec(), conn, account_id, from, to)
}

/// Delete a group and its memberships.
pub fn delete_group(conn: &Connection, account_id: &str, name: &str) -> Result<(), GroupError> {
    named_membership::delete_name(group_spec(), conn, account_id, name)
}

/// Contact ids that currently belong to a named group (case-insensitive).
pub fn list_group_member_ids(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> Result<Vec<i64>, GroupError> {
    named_membership::list_member_ids(group_spec(), conn, account_id, name)
}

/// Add or remove one group for many contacts. Creates the group when enabling.
pub fn set_contacts_group_membership(
    conn: &Connection,
    account_id: &str,
    contact_ids: &[i64],
    name: &str,
    enable: bool,
) -> Result<u64, GroupError> {
    named_membership::set_membership(group_spec(), conn, account_id, contact_ids, name, enable)
}
```

Keep `groups_for_contact` unchanged. Delete everything else except the tests
module: `MAX_GROUP_NAME_LEN`, `RESERVED_GROUP_NAMES`, `GroupError` + `From`
impl, `reserved_group_error`, `normalize_new_name`, `find_group_id`,
`ensure_group_id`, `create_group`, `rename_group`, `delete_group`,
`list_group_member_ids`, `contact_exists`, `set_contacts_group_membership`,
and `list_groups`'s old body. Remove the now-unused `OptionalExtension` and
`touch_contact` imports.

- [ ] **Step 5: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0 — every existing tag/group test passes unchanged (same
names, same messages, same behavior), plus the three new helper tests.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/named_membership.rs crates/vault/server/src/lib.rs \
  crates/vault/server/src/thread_tags_api.rs crates/vault/server/src/contact_groups_api.rs
git commit -m "refactor(server): share named-membership CRUD between tags and groups"
```

---

### Task 18: Typed API-token label validation

**Files:**
- Modify: `crates/vault/server/src/db/api_tokens.rs`
- Modify: `crates/vault/server/src/api_tokens_api.rs`
- Test: `crates/vault/server/src/api_tokens_api.rs` (new tests module)

**Interfaces:**
- Consumes: Task 17.
- Produces: `crate::db::api_tokens::{ApiTokenLabelError,
  ApiTokenMutationError}`; `create_api_token` and `update_api_token_label`
  return `Result<_, ApiTokenMutationError>`. Status codes and message strings
  are unchanged.

- [ ] **Step 1: Add the error types to `db/api_tokens.rs`**

Below the `ApiTokenAuth` struct, add:

```rust
/// Label validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTokenLabelError {
    /// The trimmed label is empty.
    Required,
    /// The label is longer than 120 characters.
    TooLong,
}

impl std::fmt::Display for ApiTokenLabelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required => write!(f, "label is required"),
            Self::TooLong => write!(f, "label must be at most 120 characters"),
        }
    }
}

impl std::error::Error for ApiTokenLabelError {}

/// Failures from creating or renaming an API token: a typed label error, or
/// any other database error.
#[derive(Debug)]
pub enum ApiTokenMutationError {
    InvalidLabel(ApiTokenLabelError),
    Other(anyhow::Error),
}

impl From<ApiTokenLabelError> for ApiTokenMutationError {
    fn from(e: ApiTokenLabelError) -> Self {
        Self::InvalidLabel(e)
    }
}

impl From<anyhow::Error> for ApiTokenMutationError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

impl From<rusqlite::Error> for ApiTokenMutationError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Other(anyhow::Error::new(e))
    }
}
```

- [ ] **Step 2: Change `validate_api_token_label`**

Replace the current function with:

```rust
fn validate_api_token_label(label: &str) -> Result<&str, ApiTokenLabelError> {
    let label = label.trim();
    if label.is_empty() {
        return Err(ApiTokenLabelError::Required);
    }
    if label.len() > 120 {
        return Err(ApiTokenLabelError::TooLong);
    }
    Ok(label)
}
```

- [ ] **Step 3: Change the two public signatures**

`create_api_token`'s return type becomes:

```rust
) -> Result<(
    String,
    String,
    ApiTokenScopes,
    String,
    Option<String>,
    String,
), ApiTokenMutationError> {
```

`update_api_token_label`'s return type becomes `Result<bool,
ApiTokenMutationError>`. Inside both bodies, the `validate_api_token_label(label)?`
line now converts through `From<ApiTokenLabelError>`, and the
`.with_context(...)` calls convert through `From<anyhow::Error>` — no other
body changes. Update their doc comments: replace the `# Errors` paragraph
with "Returns `ApiTokenMutationError::InvalidLabel` when the label is empty
or longer than 120 characters, and `Other` for database failures."

- [ ] **Step 4: Update `map_label_error` in `api_tokens_api.rs`**

Replace the current `map_label_error` with:

```rust
/// Label validation rejections are the caller's fault; anything else is a server error.
fn map_label_error(e: crate::db::api_tokens::ApiTokenMutationError) -> ApiError {
    use crate::db::api_tokens::ApiTokenMutationError;
    match e {
        ApiTokenMutationError::InvalidLabel(err) => ApiError::BadRequest(err.to_string()),
        ApiTokenMutationError::Other(err) => ApiError::Internal(err.to_string()),
    }
}
```

And change the two `spawn_blocking` closure return annotations in
`create_api_token_handler` and `rename_api_token_handler` from
`anyhow::Result<...>` to `Result<..., crate::db::api_tokens::ApiTokenMutationError>`
(keep the tuple types as they are).

- [ ] **Step 5: Add the tests**

Append to `api_tokens_api.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api_tokens::{ApiTokenLabelError, ApiTokenMutationError};

    #[test]
    fn label_errors_map_to_bad_request_with_the_same_message() {
        let err = map_label_error(ApiTokenMutationError::InvalidLabel(
            ApiTokenLabelError::Required,
        ));
        match err {
            ApiError::BadRequest(msg) => assert_eq!(msg, "label is required"),
            other => panic!("expected BadRequest, got {other:?}"),
        }

        let err = map_label_error(ApiTokenMutationError::InvalidLabel(
            ApiTokenLabelError::TooLong,
        ));
        match err {
            ApiError::BadRequest(msg) => assert_eq!(msg, "label must be at most 120 characters"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn other_errors_map_to_internal() {
        let err = map_label_error(ApiTokenMutationError::Other(anyhow::anyhow!("boom")));
        match err {
            ApiError::Internal(msg) => assert_eq!(msg, "boom"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
```

And add to `db/api_tokens.rs`'s tests module:

```rust
    #[test]
    fn label_validation_errors_are_typed() {
        let (conn, account_id) = setup();

        let err = create_api_token(&conn, &account_id, "  ", ApiTokenScopes::Both, None)
            .unwrap_err();
        match err {
            ApiTokenMutationError::InvalidLabel(label_err) => {
                assert_eq!(label_err.to_string(), "label is required");
            }
            other => panic!("expected InvalidLabel, got {other:?}"),
        }

        let err = create_api_token(
            &conn,
            &account_id,
            &"x".repeat(121),
            ApiTokenScopes::Both,
            None,
        )
        .unwrap_err();
        match err {
            ApiTokenMutationError::InvalidLabel(label_err) => {
                assert_eq!(label_err.to_string(), "label must be at most 120 characters");
            }
            other => panic!("expected InvalidLabel, got {other:?}"),
        }
    }
```

- [ ] **Step 6: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0. Existing tests that call `create_api_token(...).is_err()`
and `update_api_token_label(...).is_err()` still compile and pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vault/server/src/db/api_tokens.rs crates/vault/server/src/api_tokens_api.rs
git commit -m "refactor(server): type the API-token label validation error"
```

---

### Task 19: Replace `libc::flock` with `fs2::FileExt`

**Files:**
- Modify: `crates/vault/server/src/asset_uploads.rs` (the `lock_session` fn
  and imports)
- Test: `crates/vault/server/src/asset_uploads.rs` (new test)

**Interfaces:**
- Consumes: Task 18.
- Produces: `lock_session` using `fs2::FileExt::try_lock_exclusive`, matching
  `operation_lock.rs`. `libc` stays a dependency (`assets.rs` uses
  `libc::O_NOFOLLOW`).

- [ ] **Step 1: Swap the lock call**

In `asset_uploads.rs`, add `use fs2::FileExt;` near the top (after
`use anyhow::{Context, Result, bail};`), and replace the `lock_session`
function with:

```rust
fn lock_session(session: &Path) -> Result<ManifestLock> {
    let path = session.join("manifest.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("failed to lock {}", path.display()))?;
    Ok(ManifestLock { _file: file })
}
```

This removes the `#[cfg(unix)] { use std::os::unix::io::AsRawFd; ... }`
block. The only behavior change: a second concurrent locker now fails
immediately ("failed to lock …") instead of blocking — each part write holds
the lock for milliseconds, so no client path depends on blocking.

- [ ] **Step 2: Add the lock test**

Append to `asset_uploads.rs`'s tests module:

```rust
    #[test]
    fn manifest_lock_is_exclusive() {
        let dir = tempdir().unwrap();
        let sha = "c".repeat(64);
        let session = session_dir(dir.path(), &sha, "locktest01");
        fs::create_dir_all(&session).unwrap();

        let _held = lock_session(&session).unwrap();
        let err = lock_session(&session).unwrap_err();
        assert!(
            err.to_string().contains("failed to lock"),
            "expected lock failure, got: {err}"
        );
    }
```

- [ ] **Step 3: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0 (the multipart tests exercise the same lock path).

- [ ] **Step 4: Commit**

```bash
git add crates/vault/server/src/asset_uploads.rs
git commit -m "refactor(server): use fs2 for multipart manifest locks"
```

---

### Task 20: Curated `lib.rs` re-exports

**Files:**
- Modify: `crates/vault/server/src/lib.rs`

**Interfaces:**
- Consumes: Tasks 6–19 (every handler move).
- Produces: the final public surface: `cli`, `config`, `clap_command()`, and
  the re-exported server entry points and key types. All other modules are
  `pub(crate)`. External consumers unaffected: `main.rs` uses
  `message_vault_server::cli::Cli`, `dump-cli-docs` uses
  `message_vault_server::clap_command()`.

- [ ] **Step 1: Rewrite `lib.rs`**

Replace the module declarations and `clap_command` with exactly:

```rust
//! HTTP API and SQLite storage for browsing imported messages.
#![warn(missing_docs)]

pub mod cli;
pub mod config;

pub(crate) mod api_tokens_api;
pub(crate) mod asset_uploads;
pub(crate) mod assets;
pub(crate) mod auth;
pub(crate) mod contact_groups_api;
pub(crate) mod contacts_api;
pub(crate) mod conversations_api;
pub(crate) mod db;
pub(crate) mod dedupe;
pub(crate) mod export_api;
pub(crate) mod guest_clone;
pub(crate) mod guest_pool;
pub(crate) mod import;
pub(crate) mod import_cli;
pub(crate) mod import_media;
pub(crate) mod jsonl;
pub(crate) mod media_tools;
pub(crate) mod models;
pub(crate) mod named_membership;
pub(crate) mod openapi;
pub(crate) mod operation_lock;
pub(crate) mod page_limits;
pub(crate) mod process_assets;
pub(crate) mod profile;
pub(crate) mod reset_demo;
pub(crate) mod search_query;
pub(crate) mod server;
pub(crate) mod thread_tags_api;

pub use server::{ApiError, AppState, AuthCapability, AuthIdentity, ErrorBody, resolve_auth, run};

use clap::Command;

pub fn clap_command() -> Command {
    cli::clap_command()
}
```

- [ ] **Step 2: Fix anything the compiler flags**

Run: `cargo check -p message-vault-server`
Expected: at most unused-import warnings caused by the visibility change
(imports inside the crate still resolve the same way). Fix any error the
compiler reports; remove any import that became unused.

- [ ] **Step 3: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server && cargo check -p message-vault-server`
Expected: exit 0, no warnings from the library target.

- [ ] **Step 4: Commit**

```bash
git add crates/vault/server/src/lib.rs
git commit -m "refactor(server): expose a curated public surface from lib"
```

---

### Task 21: Remove the dead `ImportOptions.db_path` field

**Files:**
- Modify: `crates/vault/server/src/import/mod.rs`
- Modify: `crates/vault/server/src/import_cli.rs`
- Modify: `crates/vault/server/src/server.rs`

**Interfaces:**
- Consumes: Task 20.
- Produces: `ImportOptions` and `FixedImportArgs` without `db_path`;
  `import_jsonl_files(db_path, paths, opts)` as a `#[cfg(test)] pub(crate)`
  test helper; `import_export` and `import_cli` unchanged in behavior.

- [ ] **Step 1: Remove the field**

In `crates/vault/server/src/import/mod.rs`:

- delete from `ImportOptions`:

```rust
    /// Used by [`import_jsonl_files`] (CLI/tests). Warm HTTP path opens its own connection.
    #[allow(dead_code)]
    pub db_path: &'a Path,
```

- delete from `FixedImportArgs`:

```rust
    pub db_path: &'a Path,
```

- delete from `ImportOptions::fixed`:

```rust
            db_path: args.db_path,
```

- in `import_export`, delete from the `FixedImportArgs { ... }` literal:

```rust
            db_path: args.db_path,
```

- replace the whole `import_jsonl_files` function with:

```rust
/// Test helper: open a configured database and run one import.
///
/// Production paths use [`import_jsonl_files_on_conn`] on their own
/// connection (HTTP serve) or [`import_export`] (CLI directory import).
#[cfg(test)]
pub(crate) fn import_jsonl_files(
    db_path: &Path,
    paths: &[PathBuf],
    opts: &ImportOptions<'_>,
) -> Result<ImportStats> {
    validate_import_options(opts)?;

    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut conn = schema::open_configured(db_path)
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    println!("  sql:      opened {}", db_path.display());
    let _ = io::stdout().flush();
    import_jsonl_files_on_conn(&mut conn, paths, opts, ImportSchemaMode::Ensure)
}
```

- [ ] **Step 2: Update every call site**

In `crates/vault/server/src/import_cli.rs`, delete the line
`        db_path: &db_path,` from the `ImportOptions { ... }` literal in
`run` (around line 123).

In `crates/vault/server/src/server.rs`, in `run_import_path`, delete the line
`            db_path: &cfg.paths.db,` from the `FixedImportArgs { ... }`
literal.

In `crates/vault/server/src/import/mod.rs`'s tests module, every
`import_jsonl_files(` call gains `&db,` as its first argument, and every
`db_path: &db,` line in `FixedImportArgs` / `ImportOptions` literals is
deleted. The tests are: `append_skips_existing_guids_and_keeps_id_map`,
`append_existing_guid_adds_missing_children`,
`repeated_append_keeps_one_fts_posting_per_message`,
`deferred_fts_indexes_attachment_text_after_promote`,
`source_from_jsonl_stamps_export_source_and_assets`,
`media_none_skips_attachment_copy`, the four `contact_name_mode_*` tests,
`contact_handle_alias_seeds_first_wins`, `persists_missing_reason_with_null_sha256`,
`claimed_import_rejects_corrupt_existing_asset`,
`rejects_attachment_path_traversal`, and `failed_replace_keeps_existing_messages`.
(Use the compiler: `cargo test -p message-vault-server --no-run` will point at
each remaining site; fix them one by one.)

- [ ] **Step 3: Verify**

Run: `cargo fmt --check && cargo test -p message-vault-server`
Expected: exit 0; no `dead_code` warnings anywhere.

- [ ] **Step 4: Commit**

```bash
git add crates/vault/server/src/import/mod.rs crates/vault/server/src/import_cli.rs \
  crates/vault/server/src/server.rs
git commit -m "refactor(server): drop the dead ImportOptions.db_path field"
```

---

### Task 22: CHANGELOG entry and final verification

**Files:**
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: Tasks 1–21.
- Produces: the final branch state that passes the full project gate.

- [ ] **Step 1: Add the CHANGELOG entry**

In `CHANGELOG.md`, under `## [Unreleased]`, add a `### Changed` section (if
absent) with:

```markdown
### Changed

- Server crate cleanup: rustdoc and HTTP API descriptions rewritten, handlers
  moved out of `server.rs`, thread-tag and contact-group CRUD unified, and
  API-token label validation typed. No behavior change.
```

- [ ] **Step 2: Run the full workspace gate**

Run:

```bash
cargo fmt --all -- --check
./scripts/lint-all.sh
cargo build --workspace
cargo test --workspace
```

Expected: exit 0 for each (lint-all.sh runs Clippy across the workspace
except the Slint GUI, plus Biome; no web files changed).

- [ ] **Step 3: Smoke-test the dev server**

Run:

```bash
./scripts/run-vault-dev.sh --reset-demo
```

Wait for the server banner, then in a second shell:

```bash
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8080/v1/auth/mode
```

Expected: `ok` from `/health`; a JSON `{"mode":"local",...}` from
`/v1/auth/mode`. Stop the dev server (Ctrl-C in its shell).

- [ ] **Step 4: Verify the committed OpenAPI document is in sync**

Run: `cargo test -p message-vault-server committed_openapi_matches_dump`
Expected: PASS (guards against any doc edit in Tasks 6–21 that touched a
summary without regenerating).

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog entry for the server crate cleanup"
```

---

## Completion checklist

All 22 tasks complete means:

- Every handler registered in `openapi.rs` has a plain-prose summary; no
  route echoes and no `# Errors` headings remain in handler docs.
- Every `ToSchema` struct has a one-line doc; every module has a `//!`
  intro; the crate builds warning-clean with `#![warn(missing_docs)]`.
- `docs/src/assets/openapi.json` is regenerated and the stale-spec test
  passes.
- `server.rs` holds only router assembly, shared state, auth resolution, and
  plumbing; every handler lives in its domain module; `openapi.rs` points at
  the new locations; `server.rs` is under ~1,500 lines.
- `import.rs` is split into `staging` / `promote` / `contact_name`; the
  import HTTP handlers live in `import`.
- Thread tags and contact groups share `named_membership`; both API modules
  are thin wrappers; pagination limits are defined once.
- API-token label validation is typed (400 for label errors, 500 otherwise,
  same messages); `asset_uploads` uses `fs2`; `lib.rs` exposes the curated
  surface; the dead `db_path` field is gone.
- `CHANGELOG.md` has the `[Unreleased]` entry; the full workspace gate
  (fmt, clippy, build, test) and the dev-server smoke test pass.
