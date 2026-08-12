# Vault Review Fixes

**Date:** 2026-08-12  
**Status:** approved  
**Scope:** Remaining Must fix and Should fix findings from the 2026-08-12 review of `crates/vault/`.

## Problem

The vault server and demo generator have several independent failure modes. Interrupted asset writes can leave corrupt files at trusted paths. Some import and export queries lose data or fail for valid filters. Account updates can partially succeed. Demo generation and reset remove working data before replacement succeeds.

This change fixes those findings without changing the intentionally supported passwordless local-account behavior, read-only account behavior, or policy that existing vault databases are recreated after schema changes.

## Goals

1. Install assets atomically and never trust an incomplete destination file.
2. Disable local registration and login routes when Hanko authentication is selected.
3. Make filtered export counts use the same joins as message export.
4. Replace generated demo files only after the complete replacement succeeds.
5. Preserve attachments and tapbacks supplied for existing message GUIDs during append imports.
6. Map imported message IDs without relying on global SQLite insertion order.
7. Apply password and profile changes in transactions.
8. Revoke named API tokens when a password changes.
9. Make export boolean expressions, contact handling, and query limits behave consistently.
10. Preserve the current demo account and bundle when generation or import fails.
11. Make demo output reproducible from its configuration and random seed.

## Non-goals

- Enforcing the `accounts.read_only` flag on write routes.
- Removing passwordless local registration or empty-password login.
- Restoring in-place database column migrations.
- Reorganizing the vault server into new modules.
- Changing response shapes except where an existing request must return a clear HTTP 400 or 404 error.

## Architecture

### Authentication and account updates

Router construction checks `AuthMode`. Local register and login routes exist only in local mode. Hanko session exchange and the mode endpoint remain available in Hanko mode.

Password changes run the password update, session rotation, and named API-token revocation in one SQLite transaction. Profile updates run every name and handle mutation in one transaction and load the response after commit.

### Asset installation

Verified bytes are written to a temporary file in the destination shard. The file is flushed and persisted before an atomic rename to the final content-addressed path. A concurrent writer that wins the rename is accepted only after the winning file is verified.

Existing destination files are hashed before they are reported as present. A mismatch is treated as corruption and replaced without exposing a partially-written final file.

### Import promotion

Append promotion maps each staging message to a production message using account ID, source, and non-empty GUID. The map includes both newly inserted rows and rows skipped by `INSERT OR IGNORE`. Attachments and tapbacks for an existing GUID are inserted only when the corresponding child row is not already present.

Messages with empty GUIDs continue to be inserted each time. Newly inserted rows are mapped with import-scoped identity rather than every global message ID greater than a previously observed maximum.

### Export and contact queries

Message, conversation, and attachment count queries use the same joined source clause, so every filter alias exists in every count query.

Boolean free-text expressions retain their parsed `AND`, `OR`, and negation structure when converted to SQL. If an expression cannot be represented safely by the export query, the request returns HTTP 400 rather than silently changing its meaning.

Contact and conversation list queries use the same maximum query byte and term limits as export. Contact mutation checks exclude trashed contacts. Existing HTTP response formats remain unchanged.

### Demo generation and reset

Demo generation writes all JSONL files into temporary sibling directories. It replaces the active files only after every output succeeds. Obsolete files are removed only after the replacement set is ready.

The seed configuration contains a fixed RFC3339 reference timestamp. All generated timestamps derive from that value, making output byte-stable for a fixed seed and configuration.

Demo reset validates and imports into recoverable temporary state before removing the current demo account. If generation, import, deduplication, or asset processing fails, the previous account data and generated bundle remain available.

## Error handling

- Failed asset installation removes temporary files and leaves the last verified final file untouched.
- Failed account updates roll back all related SQL changes.
- Oversized or unsupported search expressions return HTTP 400.
- Local login and registration return HTTP 404 in Hanko mode because those routes are absent.
- Failed demo generation or reset preserves the previous usable data and reports the original error with context.
- Append import does not overwrite existing message body or timestamp fields while adding missing child records.

## Testing

Every behavior change follows a red-green test cycle:

1. Add a focused regression test.
2. Run it and confirm it fails for the reported reason.
3. Implement the smallest production change.
4. Run the focused test and the containing crate suite.

Required regression coverage:

- truncated and concurrent asset destination files;
- Hanko router excludes local register and login;
- filtered message count with sender and chat-handle aliases;
- failed demo generation preserves existing JSONL files;
- append import adds missing attachments and tapbacks to an existing GUID;
- import message mapping ignores unrelated concurrently inserted rows;
- failed password/profile sub-operations roll back earlier changes;
- password change revokes named API tokens;
- export `OR` keeps union semantics;
- trashed contact mutation returns not found;
- oversized contact and conversation queries return HTTP 400;
- fixed seed and reference timestamp produce identical demo output;
- failed demo reset preserves the previous demo account.

Final verification:

```bash
cargo fmt --all -- --check
cargo clippy -p message-vault-server -p demo-seed --all-targets -- -D warnings
cargo test -p message-vault-server -p demo-seed
cargo test --workspace
```

## Success criteria

- Every listed regression test passes.
- No existing vault test changes its expected behavior unless this design explicitly changes it.
- Asset and demo replacement paths do not expose partial output.
- Account updates either complete fully or leave prior state unchanged.
- Export filters produce consistent results across list and count endpoints.
- The explicitly excluded read-only, passwordless, and schema-recreation policies remain unchanged.
