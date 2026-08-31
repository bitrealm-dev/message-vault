# iMessage Backup Identity Check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Before an iMessage import starts, read the addresses the backup's device sent from, compare them to the account's profile, and stop (Continue/Cancel) when nothing matches — with the list always shown on Gate 1 and inline "Add to profile" actions.

**Architecture:** A public `backup_identities` probe in `imessage-ir-exporter` (opens the source through the same `DataSource` as a real run, before any parsing), exposed as a Tauri command. The web app calls it on Start, compares client-side against `GET /v1/account/profile` (phones **and** emails, fail-open), and stores the cleaned list on the import session (`vault_imports.source_identities`) so a resumed Gate 1 can show it without re-reading the backup.

**Tech Stack:** Rust (rusqlite, plist, Tauri v2), Axum + sqlx, React 19 + TypeScript (Vitest).

**Spec:** `docs/superpowers/specs/2026-08-31-imessage-backup-identity-check-design.md`

## Global Constraints

- Product copy states facts; it never warns, alarms, or hedges. No acknowledgment checkboxes.
- The six import stages (`parse`, `write`, `awaiting_gate_1`, `transcode`, `awaiting_gate_2`, `pushing`) are unchanged; the identity check runs before the session is created.
- Schema change = edit `schema/sql/*.sql` **and** bump `SCHEMA_VERSION` in `crates/vault/server/src/db/schema.rs` (5 → 6). No migration exists before the first stable release; the bump rebuilds every vault empty, and that is intended.
- OpenAPI-visible changes must regenerate `docs/src/assets/openapi.json` in the same commit: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json` (the workspace gate `committed_openapi_matches_dump` fails otherwise).
- Tests use fixtures built in the test itself or under `tests/fixtures/`; never commit personal backups or real message data. All literal test data below is invented (`+15550001111`, `owner@example.com`).
- `src-tauri/` is not a workspace member: build/format it with `--manifest-path src-tauri/Cargo.toml`.
- Never commit to `main`; work on this branch. Never create or push tags.
- Biome gates `web/`: prefix unused bindings with `_`; prefer real fixes over `biome-ignore`.
- The Rust code in Tasks 1–2 was compile-checked and its tests run green before this plan was written. The TypeScript snippets were written against the current sources but not type-checked — where wiring details differ, the compiler and existing tests are authoritative; keep names and types exactly as the Interfaces blocks state.

---

### Task 1: Exporter identity probe

**Files:**
- Create: `crates/exporters/imessage-ir-exporter/src/identity.rs`
- Modify: `crates/exporters/imessage-ir-exporter/src/lib.rs` (module + re-export)

**Interfaces:**
- Consumes: crate-private `DataSource::from`, `MailOptions`, `AttachmentEmbed` (existing).
- Produces: `pub fn backup_identities(db_path: &Path, ios: bool, backup_password: Option<&str>) -> anyhow::Result<Vec<String>>` and `pub fn ios_backup_phone_number(backup_root: &Path) -> Option<String>`, both re-exported from the crate root. Task 2 calls `imessage_ir_exporter::backup_identities`.

- [ ] **Step 1: Write the failing tests (new module, tests first)**

Create `crates/exporters/imessage-ir-exporter/src/identity.rs` containing only the test module below plus stub-free `use` lines — or, simpler and equally valid: write the whole file from Step 3 but with the four function bodies as `todo!()`, run tests, see them fail. Either way the tests are these, verbatim:

```rust
#[cfg(test)]
mod tests {
    use super::{backup_identities, clean_identity, identity_key, ios_backup_phone_number};
    use rusqlite::Connection;

    #[test]
    fn clean_identity_strips_prefixes_and_drops_empties() {
        assert_eq!(
            clean_identity("P:+15550001111"),
            Some("+15550001111".to_string())
        );
        assert_eq!(
            clean_identity("E:owner@example.com"),
            Some("owner@example.com".to_string())
        );
        assert_eq!(
            clean_identity("tel:+15550001111"),
            Some("+15550001111".to_string())
        );
        assert_eq!(clean_identity("E:"), None);
        assert_eq!(clean_identity("  "), None);
    }

    #[test]
    fn identity_key_normalizes_phones_and_emails() {
        assert_eq!(identity_key("+1 (555) 000-1111"), "5550001111");
        assert_eq!(identity_key("5550001111"), "5550001111");
        assert_eq!(identity_key("Owner@Example.com"), "owner@example.com");
    }

    #[test]
    fn info_plist_phone_number_reads_string() {
        let dir = tempfile::tempdir().unwrap();
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Phone Number</key>
  <string>+1 (555) 000-1111</string>
</dict>
</plist>
"#;
        std::fs::write(dir.path().join("Info.plist"), body).unwrap();
        assert_eq!(
            ios_backup_phone_number(dir.path()),
            Some("+1 (555) 000-1111".to_string())
        );

        let missing = tempfile::tempdir().unwrap();
        assert_eq!(ios_backup_phone_number(missing.path()), None);
    }

    #[test]
    fn backup_identities_cleans_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chat.db");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            "CREATE TABLE chat (ROWID INTEGER PRIMARY KEY, account_login TEXT);
             CREATE TABLE message (ROWID INTEGER PRIMARY KEY, destination_caller_id TEXT);
             INSERT INTO chat (account_login) VALUES
                 ('P:+15550001111'), ('E:'), ('E:Owner@Example.com');
             INSERT INTO message (destination_caller_id) VALUES
                 ('+15550001111'), ('tel:+15550001111'), ('owner@example.com'), (NULL);",
        )
        .unwrap();
        drop(db);

        let identities = backup_identities(&db_path, false, None).unwrap();
        assert_eq!(
            identities,
            vec!["+15550001111".to_string(), "Owner@Example.com".to_string()]
        );
    }

    #[test]
    fn backup_identities_survives_missing_columns() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chat.db");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch("CREATE TABLE chat (ROWID INTEGER PRIMARY KEY);")
            .unwrap();
        drop(db);

        let identities = backup_identities(&db_path, false, None).unwrap();
        assert!(identities.is_empty());
    }
}
```

Note what the fixtures pin: the bare `E:` prefix (28% of chats on a real backup carry it) must clean to nothing; `P:+15550001111`, `+15550001111`, and `tel:+15550001111` are one identity; `E:Owner@Example.com` and `owner@example.com` are one identity keeping the first-seen display form; a `chat` table with no `account_login` column degrades to fewer signals, not an error.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p imessage-ir-exporter identity`
Expected: FAIL (unresolved names / `todo!()` panics).

- [ ] **Step 3: Write the implementation**

The non-test part of `identity.rs`, verbatim (this exact code compiled, passed the five tests, and was clean under `cargo clippy -p imessage-ir-exporter` and rustfmt when checked on 2026-08-31):

```rust
//! Read which addresses a backup's device sent from, before any parsing.
//!
//! The Import screen's identity check calls [`backup_identities`] right after
//! the user starts an iMessage import, before the import session is created.
//! It opens the source through the same [`DataSource`] the real run uses, so
//! every method (Mac `chat.db`, iPhone backup folder, jailbreak `sms.db`) and
//! both encryption states go through one code path.

use std::{collections::HashSet, fs::File, path::Path};

use imessage_database::util::{platform::Platform, query_context::QueryContext};
use message_ir_format::ExportTransforms;
use message_vault_io_core::OutputFormat;
use rusqlite::Connection;

use crate::{
    data_source::DataSource,
    options::{AttachmentEmbed, MailOptions},
};

/// `Info.plist` → `Phone Number` from an iOS backup folder.
///
/// Returns `None` when the file is missing or cannot be parsed. `Info.plist`
/// is plaintext even in an encrypted backup.
pub fn ios_backup_phone_number(backup_root: &Path) -> Option<String> {
    let file = File::open(backup_root.join("Info.plist")).ok()?;
    let value = plist::Value::from_reader(file).ok()?;
    let dict = value.as_dictionary()?;
    match dict.get("Phone Number") {
        Some(plist::Value::String(number)) => Some(number.clone()),
        _ => None,
    }
}

/// Addresses the backup's device sent from: the union of
/// `chat.account_login`, `message.destination_caller_id`, and (for iOS
/// backups) `Info.plist` → `Phone Number`, cleaned and deduplicated.
///
/// Each per-column query falls back to an empty list when the table or
/// column is missing, so an unusual schema degrades to fewer signals rather
/// than an error.
///
/// # Errors
///
/// Returns an error when the source cannot be opened: missing database,
/// missing or wrong backup password, not an iPhone backup.
pub fn backup_identities(
    db_path: &Path,
    ios: bool,
    backup_password: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    let options = MailOptions {
        db_path: db_path.to_path_buf(),
        attachment_root: None,
        export_path: std::path::PathBuf::new(),
        query_context: QueryContext::default(),
        use_caller_id: true,
        platform: if ios { Platform::iOS } else { Platform::macOS },
        conversation_filter: None,
        cleartext_password: backup_password.map(str::to_string),
        contacts_path: None,
        attachment_embed: AttachmentEmbed::Disabled,
        transforms: ExportTransforms::default(),
        output_format: OutputFormat::Jsonl,
        log: None,
        cancel: None,
        resume: false,
    };
    let data_source = DataSource::from(&options)?;

    let mut raw = distinct_texts(data_source.db(), "SELECT DISTINCT account_login FROM chat");
    raw.extend(distinct_texts(
        data_source.db(),
        "SELECT DISTINCT destination_caller_id FROM message",
    ));
    if ios {
        raw.extend(ios_backup_phone_number(db_path));
    }

    let mut seen = HashSet::new();
    let mut identities = Vec::new();
    for value in raw {
        let Some(cleaned) = clean_identity(&value) else {
            continue;
        };
        let key = identity_key(&cleaned);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        identities.push(cleaned);
    }
    Ok(identities)
}

/// One column's distinct values; empty on any query error (older schemas).
fn distinct_texts(db: &Connection, sql: &str) -> Vec<String> {
    let Ok(mut stmt) = db.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, Option<String>>(0)) else {
        return Vec::new();
    };
    rows.flatten().flatten().collect()
}

/// Strip the `P:` / `E:` / `tel:` prefix and drop what is then empty.
///
/// Real backups hold `account_login` rows that are the bare prefix `E:` with
/// nothing after it, so the emptiness test must run on the remainder.
fn clean_identity(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("P:")
        .or_else(|| trimmed.strip_prefix("E:"))
        .or_else(|| trimmed.strip_prefix("tel:"))
        .unwrap_or(trimmed)
        .trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

/// Deduplication key: emails lowercased, phones as US national digits
/// (matching `toUsNationalDigits` in the web app and the vault's
/// `sanitize_number`).
fn identity_key(value: &str) -> String {
    if value.contains('@') {
        return value.to_ascii_lowercase();
    }
    let mut digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits.remove(0);
    }
    digits
}
```

Wire it into `crates/exporters/imessage-ir-exporter/src/lib.rs` — add the module in alphabetical order and the re-export **after** the `error` re-export (rustfmt enforces this ordering):

```rust
mod fields;
mod identity;
```

```rust
pub use backup::ios_backup_encrypted_flag;
pub use error::ENCRYPTED_BACKUP_PASSWORD_REQUIRED;
pub use identity::{backup_identities, ios_backup_phone_number};
```

Known accepted cost (do not "fix" it): `DataSource::from` also builds the contacts index, and for an encrypted iOS backup decrypts the contacts database — wasted for a probe, kept for the one-code-path rule the spec sets.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p imessage-ir-exporter identity`
Expected: 5 passed. Then `cargo clippy -p imessage-ir-exporter 2>&1 | grep identity.rs` — expected: no output (the three pre-existing warnings in `emit.rs`/`attachments_emit.rs` are not yours to fix).

- [ ] **Step 5: Commit**

```bash
git add crates/exporters/imessage-ir-exporter/src/identity.rs crates/exporters/imessage-ir-exporter/src/lib.rs
git commit -m "feat(imessage): read a backup's sender identities before parsing"
```

---

### Task 2: Tauri command and web wrapper

**Files:**
- Modify: `src-tauri/src/commands/paths.rs` (new command above `home_dir`, next to `ios_backup_encrypted`)
- Modify: `src-tauri/src/main.rs` (register in `generate_handler!`)
- Modify: `web/src/lib/tauri.ts` (wrapper next to `invokeIosBackupEncrypted`)

**Interfaces:**
- Consumes: `imessage_ir_exporter::backup_identities` (Task 1).
- Produces: Tauri command `imessage_backup_identities(path, ios, backup_password)` → `Vec<String>`; TS `invokeImessageBackupIdentities(args: { path: string; ios: boolean; backupPassword: string }): Promise<string[]>` (Task 7 calls it).

- [ ] **Step 1: Add the command to `paths.rs`**

Insert this, verbatim, directly above the `/// Ask this process for the current user's home directory.` doc comment (this exact code passed `cargo check --manifest-path src-tauri/Cargo.toml` with no new warnings and rustfmt clean on 2026-08-31; `use std::path::Path` is already imported at the top of the file):

```rust
/// Addresses an iMessage backup's device sent from, for the Import
/// identity check.
///
/// Runs on a blocking-pool thread: for an encrypted backup, answering this
/// decrypts `chat.db` to a temp file.
#[tauri::command]
pub async fn imessage_backup_identities(
    path: String,
    ios: bool,
    backup_password: Option<String>,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let password = backup_password
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty());
        imessage_ir_exporter::backup_identities(Path::new(path.trim()), ios, password)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
```

- [ ] **Step 2: Register it in `main.rs`**

In the `generate_handler!` list, after `commands::paths::ios_backup_encrypted,` add:

```rust
            commands::paths::imessage_backup_identities,
```

- [ ] **Step 3: Build to verify**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: success; no warnings pointing at `paths.rs` or `main.rs`.

- [ ] **Step 4: Add the web wrapper**

In `web/src/lib/tauri.ts`, directly below `invokeIosBackupEncrypted` (Tauri v2 maps camelCase invoke keys to snake_case Rust parameters, same as `invokeExtract` does):

```ts
/**
 * Addresses an iMessage backup's device sent from. The desktop opens the
 * source the same way the extractor will, so a source it cannot read fails
 * here with the message the extractor would give.
 */
export async function invokeImessageBackupIdentities(args: {
  path: string;
  ios: boolean;
  backupPassword: string;
}): Promise<string[]> {
  return invoke("imessage_backup_identities", {
    path: args.path,
    ios: args.ios,
    backupPassword: args.backupPassword.trim() === "" ? null : args.backupPassword,
  });
}
```

Run: `cd web && npm run lint`
Expected: clean (the wrapper is exercised by Task 7's tests; it has no logic of its own to unit-test).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/paths.rs src-tauri/src/main.rs web/src/lib/tauri.ts
git commit -m "feat(desktop): expose the backup identity probe as a command"
```

---

### Task 3: `source_identities` on the import session

**Files:**
- Modify: `schema/sql/accounts.sql` and `schema/sql/pg_accounts.sql` (new column on `vault_imports`)
- Modify: `crates/vault/server/src/db/schema.rs` (`SCHEMA_VERSION` 5 → 6)
- Modify: `crates/vault/server/src/db/vault_imports.rs` (`VaultImportRow`, `StartImportArgs`, insert, `VAULT_IMPORT_COLUMNS`, `vault_import_from_row`, tests)
- Modify: `crates/vault/server/src/import/mod.rs` (`CreateImportBody`, `imports_create_handler`, `ActiveImportSession`, `imports_active_handler`)
- Modify: every other `StartImportArgs { … }` literal (`server.rs`, `conversations_api.rs`, `import_cli.rs`, tests) — the compiler lists them all
- Modify: `docs/src/assets/openapi.json` (regenerated)

**Interfaces:**
- Consumes: nothing new.
- Produces: `POST /v1/imports` accepts optional `source_identities: string[]`; `GET /v1/imports/active` returns `source_identities` (JSON array or null) on the session. Task 7's client code sends and reads exactly those field names.

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)] mod tests` of `crates/vault/server/src/import/mod.rs` — wherever the existing create/active round-trip tests live (server.rs's tests also construct `CreateImportBody`; follow whichever module holds `a_stored_form_snapshot_drops_credentials` and mirror its setup exactly, including `test_state()` and discarding the pre-opened session):

```rust
    /// The identity list a client read from the backup rides on the session
    /// so a resumed Gate 1 can show it without re-reading the backup.
    #[tokio::test]
    async fn imports_create_stores_source_identities() {
        let (_tmp, state, token, import_id) = test_state().await;
        let _ = imports_discard_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
        )
        .await
        .unwrap();

        let body = CreateImportBody {
            source: "imessage".into(),
            mode: "append".into(),
            tool: None,
            account: None,
            stage: None,
            staging_dir: None,
            device_id: None,
            form: None,
            source_fingerprint: None,
            source_identities: Some(serde_json::json!(["+15550001111", "owner@example.com"])),
        };
        let _ = imports_create_handler(State(state.clone()), auth_headers(&token), Json(body))
            .await
            .unwrap();

        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        let session = active.0.session.expect("a live session is reported");
        assert_eq!(
            session.source_identities,
            serde_json::json!(["+15550001111", "owner@example.com"])
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p message-vault-server imports_create_stores_source_identities`
Expected: FAIL to compile — `CreateImportBody` has no field `source_identities`.

- [ ] **Step 3: Implement**

`schema/sql/accounts.sql` — append to the `vault_imports` column list, after `source_fingerprint TEXT` (add the comma to the line above):

```sql
    -- Addresses the backup's device sent from (JSON array), read by the
    -- client before parsing. Lets a resumed Gate 1 show the identity list
    -- without re-reading the backup.
    source_identities TEXT
```

Make the same edit to `vault_imports` in `schema/sql/pg_accounts.sql`. Bump `crates/vault/server/src/db/schema.rs`:

```rust
pub const SCHEMA_VERSION: i64 = 6;
```

`crates/vault/server/src/db/vault_imports.rs` — four mechanical edits that must stay in step:

1. `VaultImportRow` gains, after `source_fingerprint`:

```rust
    /// Addresses the backup's device sent from (JSON array).
    pub source_identities: Option<String>,
```

2. `StartImportArgs` gains, after `source_fingerprint`:

```rust
    /// Backup device identity list as JSON.
    pub source_identities: Option<&'a str>,
```

3. `start_import`'s INSERT gains the column and an `$11` bind:

```rust
        INSERT INTO vault_imports (
            account_id, source, tool, mode, status, started_at,
            message_count, attachment_count, bytes_uploaded,
            stage, staging_dir, device_id, form_json, source_fingerprint,
            source_identities
        ) VALUES ($1, $2, $3, $4, 'running', $5, 0, 0, 0, $6, $7, $8, $9, $10, $11)
        RETURNING id
```

with `.bind(args.source_identities)` after `.bind(args.source_fingerprint)`.

4. `VAULT_IMPORT_COLUMNS` gains `, source_identities` at the end, and `vault_import_from_row` gains `source_identities: row.try_get(22)?,` after the `source_fingerprint` line.

The compiler then lists every other `StartImportArgs { … }` literal (in `server.rs`, `conversations_api.rs`, `import_cli.rs`, this file's tests). Add `source_identities: None,` to each — only `imports_create_handler` ever passes a value.

`crates/vault/server/src/import/mod.rs`:

- `CreateImportBody` gains, after `source_fingerprint`:

```rust
    /// Addresses the backup's device sent from, when the client read them.
    #[serde(default)]
    pub(crate) source_identities: Option<serde_json::Value>,
```

- In `imports_create_handler`, next to the `fingerprint_json` line:

```rust
    let identities_json =
        optional_json_string(body.source_identities.as_ref(), "source_identities")?;
```

and thread `source_identities: identities_json.as_deref(),` into its `StartImportArgs`.

- `ActiveImportSession` gains, after `source_fingerprint`:

```rust
    /// Addresses the backup's device sent from (JSON array), or null.
    pub(crate) source_identities: serde_json::Value,
```

- `imports_active_handler`'s mapping gains, after its `source_fingerprint` line:

```rust
            source_identities: parse_summary_json(row.source_identities),
```

The compiler also flags every `CreateImportBody { … }` test literal — add `source_identities: None,` to each.

- [ ] **Step 4: Run tests, regenerate OpenAPI**

Run: `cargo test -p message-vault-server`
Expected: all pass, including the new one. Then:

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`
Run: `cargo test -p message-vault-server committed_openapi_matches_dump`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add schema/sql/accounts.sql schema/sql/pg_accounts.sql crates/vault/server/src docs/src/assets/openapi.json
git commit -m "feat(vault): record the backup's identity list on the import session"
```

---

### Task 4: Web identity logic

**Files:**
- Create: `web/src/lib/backupIdentity.ts`
- Test: `web/src/lib/backupIdentity.test.ts`

**Interfaces:**
- Consumes: `phonesMatch` from `web/src/lib/phoneTokens.ts`.
- Produces (Tasks 5–8 import these exact names):
  - `type IdentityService = "phone" | "email"`
  - `identityService(value: string): IdentityService`
  - `identityOnProfile(value: string, profile: { phones: string[]; emails: string[] }): boolean`
  - `needsIdentityStop(identities: string[], profile: { phones: string[]; emails: string[] } | null): boolean`
  - `parseSourceIdentities(value: unknown): string[] | null`

- [ ] **Step 1: Write the failing tests**

`web/src/lib/backupIdentity.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  identityOnProfile,
  identityService,
  needsIdentityStop,
  parseSourceIdentities,
} from "./backupIdentity";

const profile = { phones: ["+1 (555) 000-1111"], emails: ["Owner@Example.com"] };

describe("identityService", () => {
  it("calls anything with an @ an email and the rest a phone", () => {
    expect(identityService("owner@example.com")).toBe("email");
    expect(identityService("+15550001111")).toBe("phone");
  });
});

describe("identityOnProfile", () => {
  it("matches phones by digits despite formatting", () => {
    expect(identityOnProfile("5550001111", profile)).toBe(true);
    expect(identityOnProfile("+15559999999", profile)).toBe(false);
  });

  it("matches emails case-insensitively", () => {
    expect(identityOnProfile("owner@example.com", profile)).toBe(true);
    expect(identityOnProfile("other@example.com", profile)).toBe(false);
  });
});

describe("needsIdentityStop", () => {
  it("stops when nothing matches, including an empty profile", () => {
    expect(needsIdentityStop(["+15559999999"], profile)).toBe(true);
    expect(needsIdentityStop(["+15550001111"], { phones: [], emails: [] })).toBe(true);
  });

  it("does not stop on any overlap", () => {
    expect(needsIdentityStop(["+15559999999", "owner@example.com"], profile)).toBe(false);
  });

  it("fails open: no identities read, or no profile loaded", () => {
    expect(needsIdentityStop([], profile)).toBe(false);
    expect(needsIdentityStop(["+15559999999"], null)).toBe(false);
  });
});

describe("parseSourceIdentities", () => {
  it("keeps only an array of strings", () => {
    expect(parseSourceIdentities(["a", "b"])).toEqual(["a", "b"]);
    expect(parseSourceIdentities(["a", 5])).toBeNull();
    expect(parseSourceIdentities(null)).toBeNull();
    expect(parseSourceIdentities("a")).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/lib/backupIdentity.test.ts`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement**

`web/src/lib/backupIdentity.ts`:

```ts
import { phonesMatch } from "./phoneTokens";

/** Which kind of address a backup identity is, for display and for the profile endpoint. */
export type IdentityService = "phone" | "email";

/** Anything with an `@` is an email; everything else is a phone. */
export function identityService(value: string): IdentityService {
  return value.includes("@") ? "email" : "phone";
}

/** Whether one backup identity is on the account's profile. */
export function identityOnProfile(
  value: string,
  profile: { phones: string[]; emails: string[] },
): boolean {
  if (identityService(value) === "email") {
    const needle = value.trim().toLowerCase();
    return profile.emails.some((email) => email.trim().toLowerCase() === needle);
  }
  return profile.phones.some((phone) => phonesMatch(value, phone));
}

/**
 * Whether Import should stop before creating the session: identities were
 * read and none is on the profile. Fails open — no identities read, or no
 * profile loaded (fetch failed), never blocks an import.
 */
export function needsIdentityStop(
  identities: string[],
  profile: { phones: string[]; emails: string[] } | null,
): boolean {
  if (identities.length === 0 || profile === null) return false;
  return !identities.some((identity) => identityOnProfile(identity, profile));
}

/** The session's stored identity list, or null when absent or malformed. */
export function parseSourceIdentities(value: unknown): string[] | null {
  if (!Array.isArray(value)) return null;
  return value.every((item) => typeof item === "string") ? (value as string[]) : null;
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd web && npx vitest run src/lib/backupIdentity.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/backupIdentity.ts web/src/lib/backupIdentity.test.ts
git commit -m "feat(web): backup identity classification and profile matching"
```

---

### Task 5: Identity list component

**Files:**
- Create: `web/src/screens/import/BackupIdentityList.tsx`
- Test: `web/src/screens/import/BackupIdentityList.test.tsx`

**Interfaces:**
- Consumes: `identityOnProfile`, `identityService`, `IdentityService` (Task 4).
- Produces: `default export function BackupIdentityList(props: { identities: string[]; profile: { phones: string[]; emails: string[] } | null; onAdd: (value: string, service: IdentityService) => Promise<void>; busy?: boolean })`. Tasks 6 and 8 render it.

- [ ] **Step 1: Write the failing tests**

`web/src/screens/import/BackupIdentityList.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import BackupIdentityList from "./BackupIdentityList";

const profile = { phones: ["+15550001111"], emails: [] };

describe("BackupIdentityList", () => {
  it("marks matched addresses and offers to add unmatched ones", () => {
    render(
      <BackupIdentityList
        identities={["+15550001111", "owner@example.com"]}
        profile={profile}
        onAdd={vi.fn()}
      />,
    );
    expect(screen.getByText("+15550001111")).toBeInTheDocument();
    expect(screen.getByText("On your profile")).toBeInTheDocument();
    expect(screen.getByText("owner@example.com")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add to profile" })).toBeInTheDocument();
  });

  it("sends the value and its service to onAdd", async () => {
    const onAdd = vi.fn().mockResolvedValue(undefined);
    render(
      <BackupIdentityList identities={["owner@example.com"]} profile={profile} onAdd={onAdd} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Add to profile" }));
    expect(onAdd).toHaveBeenCalledWith("owner@example.com", "email");
  });

  it("states the fact when the backup records no identities", () => {
    render(<BackupIdentityList identities={[]} profile={profile} onAdd={vi.fn()} />);
    expect(
      screen.getByText("This backup doesn't record which account it came from."),
    ).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/screens/import/BackupIdentityList.test.tsx`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement**

`web/src/screens/import/BackupIdentityList.tsx` (match the Tailwind idiom of `GateOneScreen.tsx` — `border-border`, `text-muted`, `text-text`, `rounded-lg`; adjust class details to sit well next to the Gate 1 table, but keep every string of copy exactly as written here):

```tsx
import Button from "../../components/Button";
import { type IdentityService, identityOnProfile, identityService } from "../../lib/backupIdentity";

/**
 * The addresses a backup's device sent from, each marked as on the
 * account's profile or not, with an inline add for the ones that are not.
 * Renders on the identity stop and as a section on Gate 1.
 */
export default function BackupIdentityList({
  identities,
  profile,
  onAdd,
  busy,
}: {
  identities: string[];
  /** Null while the profile is loading or its fetch failed — marks and
   * add buttons need it, so both wait on it rather than guessing. */
  profile: { phones: string[]; emails: string[] } | null;
  onAdd: (value: string, service: IdentityService) => Promise<void>;
  busy?: boolean;
}) {
  if (identities.length === 0) {
    return (
      <p className="m-0 text-[0.813rem] text-muted">
        This backup doesn't record which account it came from.
      </p>
    );
  }

  return (
    <ul className="m-0 flex list-none flex-col gap-2 p-0">
      {identities.map((identity) => {
        const matched = profile != null && identityOnProfile(identity, profile);
        return (
          <li
            key={identity}
            className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"
          >
            <span className="text-[0.875rem] text-text">{identity}</span>
            {matched ? (
              <span className="text-[0.813rem] text-muted">On your profile</span>
            ) : (
              <span className="flex items-center gap-2">
                <span className="text-[0.813rem] text-muted">Not on your profile</span>
                {profile != null && (
                  <Button
                    variant="ghost"
                    onClick={() => void onAdd(identity, identityService(identity))}
                    disabled={busy}
                  >
                    Add to profile
                  </Button>
                )}
              </span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
```

(If `Button` requires a `size` prop or its `variant` values differ, follow the component's actual API — check `web/src/components/Button.tsx`.)

- [ ] **Step 4: Run to verify they pass**

Run: `cd web && npx vitest run src/screens/import/BackupIdentityList.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/import/BackupIdentityList.tsx web/src/screens/import/BackupIdentityList.test.tsx
git commit -m "feat(web): backup identity list with profile marks and inline add"
```

---

### Task 6: The identity stop screen

**Files:**
- Create: `web/src/screens/import/BackupIdentityStopScreen.tsx`
- Test: `web/src/screens/import/BackupIdentityStopScreen.test.tsx`

**Interfaces:**
- Consumes: `BackupIdentityList` (Task 5), `identityOnProfile` (Task 4).
- Produces: `default export function BackupIdentityStopScreen(props: { identities: string[]; profile: { phones: string[]; emails: string[] } | null; onAdd: (value: string, service: IdentityService) => Promise<void>; onContinue: () => void; onCancel: () => void; busy?: boolean })`. Task 8 renders it for the `identity_stop` phase.

- [ ] **Step 1: Write the failing tests**

`web/src/screens/import/BackupIdentityStopScreen.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import BackupIdentityStopScreen from "./BackupIdentityStopScreen";

const noMatch = { phones: ["+15559999999"], emails: [] };

describe("BackupIdentityStopScreen", () => {
  it("states the mismatch and offers Continue and Cancel — no checkbox", () => {
    render(
      <BackupIdentityStopScreen
        identities={["+15550001111"]}
        profile={noMatch}
        onAdd={vi.fn()}
        onContinue={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(
      screen.getByText("None of the addresses this backup sent from are on your profile."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue import" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });

  it("restates the fact once an address matches (after an add)", () => {
    render(
      <BackupIdentityStopScreen
        identities={["+15550001111"]}
        profile={{ phones: ["+15550001111"], emails: [] }}
        onAdd={vi.fn()}
        onContinue={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(
      screen.getByText("An address this backup sent from is on your profile."),
    ).toBeInTheDocument();
  });

  it("wires the two buttons", async () => {
    const onContinue = vi.fn();
    const onCancel = vi.fn();
    render(
      <BackupIdentityStopScreen
        identities={["+15550001111"]}
        profile={noMatch}
        onAdd={vi.fn()}
        onContinue={onContinue}
        onCancel={onCancel}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Continue import" }));
    expect(onContinue).toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/screens/import/BackupIdentityStopScreen.test.tsx`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement**

`web/src/screens/import/BackupIdentityStopScreen.tsx`:

```tsx
import Button from "../../components/Button";
import { type IdentityService, identityOnProfile } from "../../lib/backupIdentity";
import BackupIdentityList from "./BackupIdentityList";

/**
 * Shown before the import session is created, when a probe of the backup
 * found identities and none is on the account's profile. A mismatch has no
 * mechanical consequence — attribution runs on the importing account and
 * Apple's own from-me flag either way — so the list is the information and
 * the decision is the click: Continue, or Cancel back to the form. Adding
 * an address re-runs the comparison live (the marks and heading derive
 * from the profile prop), so claiming the device's address resolves the
 * mismatch in place.
 */
export default function BackupIdentityStopScreen({
  identities,
  profile,
  onAdd,
  onContinue,
  onCancel,
  busy,
}: {
  identities: string[];
  profile: { phones: string[]; emails: string[] } | null;
  onAdd: (value: string, service: IdentityService) => Promise<void>;
  onContinue: () => void;
  onCancel: () => void;
  busy?: boolean;
}) {
  const matched =
    profile != null && identities.some((identity) => identityOnProfile(identity, profile));

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">
        {matched
          ? "An address this backup sent from is on your profile."
          : "None of the addresses this backup sent from are on your profile."}
      </h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">
        These are the addresses the backup's device sent messages from.
      </p>

      <BackupIdentityList identities={identities} profile={profile} onAdd={onAdd} busy={busy} />

      <div className="mt-5 flex items-center gap-3">
        <Button variant="primary" size="wide" onClick={onContinue} disabled={busy}>
          Continue import
        </Button>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
      </div>
    </>
  );
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd web && npx vitest run src/screens/import/BackupIdentityStopScreen.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/import/BackupIdentityStopScreen.tsx web/src/screens/import/BackupIdentityStopScreen.test.tsx
git commit -m "feat(web): identity stop screen with Continue and Cancel"
```

---

### Task 7: Probe and stop flow in useImportJob

**Files:**
- Modify: `web/src/screens/import/importProgressState.ts` (phase union)
- Modify: `web/src/lib/importSession.ts` (session type)
- Modify: `web/src/screens/import/useImportJob.ts`
- Test: `web/src/screens/import/useImportJob.test.tsx`

**Interfaces:**
- Consumes: `invokeImessageBackupIdentities` (Task 2), `needsIdentityStop` / `parseSourceIdentities` (Task 4), `loadAccountProfile` from `web/src/lib/useAccountProfile.ts`, `isImessageMethod` (already imported in the hook).
- Produces: `ImportPhase` gains `"identity_stop"`; `ActiveImportSession` gains `source_identities: unknown`; the hook returns three new members Task 8 wires up: `sourceIdentities: string[] | null`, `continueAfterIdentityStop(): Promise<void>`, `cancelIdentityStop(): void`.

- [ ] **Step 1: Write the failing tests**

Append to `useImportJob.test.tsx`, following its existing conventions (module mocks at the top, `renderHook`, `act`). Two new module mocks are needed at the top of the file with the others:

```ts
const invokeImessageBackupIdentitiesMock = vi.fn();
const loadAccountProfileMock = vi.fn();
```

Add `invokeImessageBackupIdentities: (...args: unknown[]) => invokeImessageBackupIdentitiesMock(...args),` inside the existing `vi.mock("../../lib/tauri", …)` factory, and a new mock block:

```ts
vi.mock("../../lib/useAccountProfile", () => ({
  loadAccountProfile: (...args: unknown[]) => loadAccountProfileMock(...args),
}));
```

Then the tests (adapt the `startImport` form-values literal from an existing iMessage test in this file — the fields are the `ImportJobFormValues` the file already constructs; only `source: "imessage-ios"`, `backupPath`, and `backupPassword` matter here):

```ts
describe("identity check", () => {
  it("stops at identity_stop when nothing the backup sent from is on the profile", async () => {
    invokeImessageBackupIdentitiesMock.mockResolvedValue(["+15550001111"]);
    loadAccountProfileMock.mockResolvedValue({ phones: ["+15559999999"], emails: [] });
    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(imessageForm());
    });
    expect(result.current.phase).toBe("identity_stop");
    expect(result.current.sourceIdentities).toEqual(["+15550001111"]);
    // Nothing was created: no session POST, no extract.
    expect(postMock).not.toHaveBeenCalled();
    expect(invokeExtractMock).not.toHaveBeenCalled();
  });

  it("continueAfterIdentityStop proceeds and sends the identities on the session", async () => {
    invokeImessageBackupIdentitiesMock.mockResolvedValue(["+15550001111"]);
    loadAccountProfileMock.mockResolvedValue({ phones: ["+15559999999"], emails: [] });
    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(imessageForm());
    });
    await act(async () => {
      await result.current.continueAfterIdentityStop();
    });
    expect(postMock).toHaveBeenCalledWith(
      "/v1/imports",
      expect.objectContaining({ source_identities: ["+15550001111"] }),
    );
  });

  it("cancelIdentityStop returns to the form with nothing created", async () => {
    invokeImessageBackupIdentitiesMock.mockResolvedValue(["+15550001111"]);
    loadAccountProfileMock.mockResolvedValue({ phones: [], emails: [] });
    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(imessageForm());
    });
    act(() => {
      result.current.cancelIdentityStop();
    });
    expect(result.current.phase).toBe("form");
    expect(postMock).not.toHaveBeenCalled();
  });

  it("proceeds without a stop when an identity matches, sending the list", async () => {
    invokeImessageBackupIdentitiesMock.mockResolvedValue(["+15550001111"]);
    loadAccountProfileMock.mockResolvedValue({ phones: ["+1 555 000 1111"], emails: [] });
    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(imessageForm());
    });
    expect(result.current.phase).not.toBe("identity_stop");
    expect(postMock).toHaveBeenCalledWith(
      "/v1/imports",
      expect.objectContaining({ source_identities: ["+15550001111"] }),
    );
  });

  it("fails open when the probe errors", async () => {
    invokeImessageBackupIdentitiesMock.mockRejectedValue(new Error("locked"));
    loadAccountProfileMock.mockResolvedValue({ phones: [], emails: [] });
    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(imessageForm());
    });
    expect(result.current.phase).not.toBe("identity_stop");
  });

  it("does not probe non-iMessage sources", async () => {
    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(sbrForm());
    });
    expect(invokeImessageBackupIdentitiesMock).not.toHaveBeenCalled();
  });
});
```

`imessageForm()` / `sbrForm()`: small helpers returning the same `ImportJobFormValues` literal the file's existing tests pass to `startImport`, with `source` set accordingly. If the file already has such a helper, use it.

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/screens/import/useImportJob.test.tsx`
Expected: the new describe block fails (`sourceIdentities`/`continueAfterIdentityStop` undefined); every pre-existing test still passes.

- [ ] **Step 3: Implement**

1. `web/src/screens/import/importProgressState.ts`:

```ts
export type ImportPhase = "form" | "identity_stop" | "progress" | "gate_1" | "gate_2" | "done";
```

(`progressHeading` guards on `"done"` and otherwise reads steps, so no further change there.)

2. `web/src/lib/importSession.ts` — `ActiveImportSession` gains, after `source_fingerprint`:

```ts
  /** Addresses the backup's device sent from (JSON array), or null. */
  source_identities: unknown;
```

3. `web/src/screens/import/useImportJob.ts`:

- Imports: `invokeImessageBackupIdentities` from `../../lib/tauri`; `needsIdentityStop`, `parseSourceIdentities` from `../../lib/backupIdentity`; `loadAccountProfile` from `../../lib/useAccountProfile`.
- New state and ref near the other phase state:

```ts
  const [sourceIdentities, setSourceIdentities] = useState<string[] | null>(null);
  /** The submitted form, parked while the identity stop is showing. */
  const pendingIdentityFormRef = useRef<ImportJobFormValues | null>(null);
```

- Rename the existing `startImport` to `runImport` and give it an identities parameter:

```ts
  async function runImport(
    form: ImportJobFormValues,
    identities: string[] | null,
    resume?: ResumePush,
    resumeWrite?: ResumeWrite,
  ): Promise<void> {
```

Inside it, the `/v1/imports` create body gains one field, after `source_fingerprint`:

```ts
          source_identities: identities,
```

- The new `startImport` keeps the old public signature and runs the check first:

```ts
  /**
   * Start an import. For a fresh iMessage start this first reads which
   * addresses the backup's device sent from and compares them to the
   * profile; when nothing matches, it parks the form and stops at
   * `identity_stop` — before any session exists, so Cancel has nothing to
   * clean up. The probe fails open: a source it cannot read will fail in
   * the extractor moments later with the proper error.
   */
  async function startImport(
    form: ImportJobFormValues,
    resume?: ResumePush,
    resumeWrite?: ResumeWrite,
  ): Promise<void> {
    if (!isTauri()) return;
    let identities: string[] | null = null;
    if (!resume && !resumeWrite && isImessageMethod(form.source)) {
      identities = await invokeImessageBackupIdentities({
        path: form.backupPath,
        ios: form.source === "imessage-ios",
        backupPassword: form.backupPassword,
      }).catch(() => []);
      setSourceIdentities(identities);
      const profile = await loadAccountProfile();
      if (needsIdentityStop(identities, profile)) {
        pendingIdentityFormRef.current = form;
        setPhase("identity_stop");
        return;
      }
    } else {
      setSourceIdentities(null);
    }
    await runImport(form, identities, resume, resumeWrite);
  }

  /** Continue past the identity stop with the parked form. */
  async function continueAfterIdentityStop(): Promise<void> {
    const form = pendingIdentityFormRef.current;
    if (!form) return;
    pendingIdentityFormRef.current = null;
    await runImport(form, sourceIdentities);
  }

  /** Leave the identity stop; nothing was created, so only the phase moves. */
  function cancelIdentityStop(): void {
    pendingIdentityFormRef.current = null;
    returnToForm();
  }
```

(One subtlety: `runImport`'s early `if (!isTauri()) return;` line stays where it is — harmless duplication; and `continueAfterIdentityStop` reads `sourceIdentities` from state, which was set before the stop rendered.)

- `resumeAtGate` hydrates the list from the session, next to its other `set…` calls:

```ts
    setSourceIdentities(parseSourceIdentities(session.source_identities));
```

- The hook's return object gains:

```ts
    sourceIdentities,
    continueAfterIdentityStop,
    cancelIdentityStop,
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd web && npx vitest run src/screens/import/useImportJob.test.tsx`
Expected: all pass, old and new. Then `cd web && npm test` — nothing else broke (`resumeDecision`, `ImportScreen`, …).

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/import/importProgressState.ts web/src/lib/importSession.ts web/src/screens/import/useImportJob.ts web/src/screens/import/useImportJob.test.tsx
git commit -m "feat(web): probe backup identities on start and stop on a mismatch"
```

---

### Task 8: ImportScreen and Gate 1 wiring

**Files:**
- Modify: `web/src/screens/ImportScreen.tsx`
- Modify: `web/src/screens/import/GateOneScreen.tsx`
- Test: `web/src/screens/import/GateOneScreen.test.tsx`, `web/src/screens/ImportScreen.test.tsx`

**Interfaces:**
- Consumes: hook members from Task 7; `BackupIdentityStopScreen` (Task 6); `BackupIdentityList` (Task 5); `useAccountProfile` and `AccountProfile` (existing).
- Produces: `GateOneScreen` gains one optional prop: `identityPanel?: ReactNode` (stays presentational — the connected list is composed in ImportScreen).

- [ ] **Step 1: Write the failing tests**

`GateOneScreen.test.tsx` — one new test in its existing describe, using the file's existing `summary`/props helpers:

```tsx
  it("renders the identity panel it is given", () => {
    renderGateOne({ identityPanel: <div data-testid="identity-panel" /> });
    expect(screen.getByTestId("identity-panel")).toBeInTheDocument();
  });
```

(`renderGateOne` here stands for however the file builds props today — extend its helper or inline the extra prop the same way neighboring tests do.)

`ImportScreen.test.tsx` — the screen test file mocks `useImportJob`; extend the mocked return with `sourceIdentities`, `continueAfterIdentityStop`, `cancelIdentityStop`, and add:

```tsx
  it("shows the identity stop screen for the identity_stop phase", () => {
    mockImportJob({ phase: "identity_stop", sourceIdentities: ["+15550001111"] });
    renderImportScreen();
    expect(
      screen.getByText("None of the addresses this backup sent from are on your profile."),
    ).toBeInTheDocument();
  });
```

(Again: match the file's actual mock/render helpers; the assertion strings are the contract.)

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/screens/import/GateOneScreen.test.tsx src/screens/ImportScreen.test.tsx`
Expected: new tests FAIL; existing ones pass.

- [ ] **Step 3: Implement**

1. `GateOneScreen.tsx` — add to the props type and render it between the counts table and the media-forecast section:

```tsx
  /** The backup's identity list, composed by the caller (null-safe: omit to hide). */
  identityPanel?: ReactNode;
```

```tsx
      {identityPanel ? (
        <section className="mt-5">
          <h2 className="m-0 text-base font-semibold">Addresses this backup sent from</h2>
          <div className="mt-3">{identityPanel}</div>
        </section>
      ) : null}
```

(`import type { ReactNode } from "react";` at the top.)

2. `ImportScreen.tsx`:

- Pull the new hook members out of `useImportJob()` alongside the existing ones: `sourceIdentities`, `continueAfterIdentityStop`, `cancelIdentityStop`.
- Get the shared profile (the screen already imports `loadAccountProfile`; the hook variant carries `setProfile` for updating after an add):

```tsx
import { useAccountProfile } from "../lib/useAccountProfile";
import type { AccountProfile } from "../lib/account";
import type { IdentityService } from "../lib/backupIdentity";
import BackupIdentityList from "./import/BackupIdentityList";
import BackupIdentityStopScreen from "./import/BackupIdentityStopScreen";
```

```tsx
  const { profile, setProfile } = useAccountProfile();
  const identityProfile = profile ? { phones: profile.phones, emails: profile.emails } : null;

  /** Link one backup address onto the profile; the marks re-derive from the
   * updated profile, so a claimed address resolves a mismatch in place. */
  const addIdentityToProfile = async (value: string, service: IdentityService) => {
    const updated = await apiClient.post<AccountProfile>("/v1/account/profile", {
      handles: [{ handle: value, service }],
    });
    setProfile(updated);
  };
```

- Render the stop phase with the other phase blocks:

```tsx
      {phase === "identity_stop" && sourceIdentities && (
        <BackupIdentityStopScreen
          identities={sourceIdentities}
          profile={identityProfile}
          onAdd={addIdentityToProfile}
          onContinue={() => void continueAfterIdentityStop()}
          onCancel={cancelIdentityStop}
          busy={running}
        />
      )}
```

- Pass the identity panel into Gate 1 (only when a list exists — non-iMessage imports have `sourceIdentities === null` and show nothing):

```tsx
          identityPanel={
            sourceIdentities != null ? (
              <BackupIdentityList
                identities={sourceIdentities}
                profile={identityProfile}
                onAdd={addIdentityToProfile}
                busy={running}
              />
            ) : undefined
          }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd web && npm test && npm run lint`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/ImportScreen.tsx web/src/screens/import/GateOneScreen.tsx web/src/screens/import/GateOneScreen.test.tsx web/src/screens/ImportScreen.test.tsx
git commit -m "feat(web): identity stop phase and Gate 1 identity section"
```

---

### Task 9: Full verification

**Files:** none new.

- [ ] **Step 1: The whole gate**

Run: `./scripts/check-pr.sh`
Expected: passes end to end (rustfmt both trees, workspace build + test, src-tauri build, Biome, Vitest, docs check, Clippy).

- [ ] **Step 2: Browser check with the Playwright MCP**

Start `./scripts/run-vault-dev.sh --reset-demo` and `cd web && npm run dev`, then against `http://127.0.0.1:5173` (sign in `demo`, empty password):

- Gate flows that don't involve iMessage still work (the browser build has no Tauri, so `startImport` returns early — this checks nothing regressed in Import's form/resume rendering).
- Render `BackupIdentityStopScreen` and the Gate 1 identity section via the Vitest-covered paths; the full desktop probe (`cargo tauri dev` with a real backup folder) is a desktop-only check — run it if a safe fixture backup is at hand, otherwise note it as manually untested in the PR body.

- [ ] **Step 3: Commit anything the gate fixed, then stop**

Do not open a PR, merge, or tag unless asked.

---

## Self-review notes (done at planning time)

- Spec coverage: probe signals + cleaning (Task 1), pre-parse placement outside the stage machine (Tasks 1–2, 7), phones-and-emails comparison + fail-open (Task 4), stop with Continue/Cancel and no checkbox (Tasks 6–7), list always shown with marks and inline add via the existing profile endpoint (Tasks 5, 8), no-signal line (Task 5), recording on the session + resumed Gate 1 (Tasks 3, 7, 8). Non-goals (CLI, Settings → Storage, docs) are deliberately absent.
- The Rust code in Tasks 1 and 2 was compiled, tested (5/5), clippy- and fmt-checked in the working tree before being written into this plan, then reverted. TypeScript snippets follow the current sources; the type-checker and existing tests adjudicate wiring details, with names and copy strings fixed by this plan.
- Copy strings appear in exactly two places each (component + its test) and must stay byte-identical between them.
