# Import Failures and Schema Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A bad import file comes back as a 400 with a sentence naming the problem instead of a bare 500, `POST /v1/import` states its real contract, and every document agrees the JSONL schema version is 4.

**Architecture:** The import pipeline keeps returning `anyhow::Error`, but the two failures a person can act on (wrong schema version, a line that is not message-ir JSON) become one typed value, `ImportFailure`, raised where the file is parsed. The HTTP handler classifies the error at the seam by downcasting: an `ImportFailure` is a 400 with its own sentence; anything else stays a 500 with the cause on stderr. The contact mutation handler gets the mirror fix: a database error becomes a 500 instead of a 400.

**Tech Stack:** Rust 2024, Axum, sqlx (`AnyConnection`), anyhow, utoipa; tests through `crate::test_support` over real HTTP; `cargo run -p message-vault-server -- dump-openapi` and `cd web && npm run gen:api` regenerate the OpenAPI document and the web's types.

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`, section "Import failures". This is pull request 1 of the eight in that spec's "Delivery" section.

## Global Constraints

- The IR schema version is `4` (`crates/libs/ir/src/lib.rs:29`). No file is upgraded from 3; it is refused by name.
- Error bodies keep today's shape `{ok: false, error}` in this pull request. The envelope changes in pull request 2 (ADR-0005), not here.
- No `ok`, list-key, or paging changes here. Only failure classification, the `source` contract, the banner, and documents.
- Run from the repo root: `cargo fmt --all -- --check`, `cargo test -p message-vault-server`, and `./scripts/check-pr.sh` before the final commit.
- Commit messages: conventional commit subject, plain-English body explaining what and why, ending with the two attribution lines this session uses.
- The server crate avoids new dependencies. `ImportFailure` implements `std::error::Error` by hand; no `thiserror`.

---

## File map

| File | Responsibility after this plan |
| --- | --- |
| `crates/vault/server/src/import/failure.rs` (create) | The `ImportFailure` type: the two person-actionable reasons an import stops, their sentences, and the downcast helper. |
| `crates/vault/server/src/import/mod.rs` | Exposes `failure`, classifies the import result in `run_import_path`, defaults `source` in `ImportQuery`, validates it once. |
| `crates/vault/server/src/models.rs` | `parse_ir_lines` raises `ImportFailure` instead of free-text `bail!`. |
| `crates/vault/server/src/contacts_api.rs` | `contact_mutate_handler` maps a database error to 500 and everything else to 400 through one function. |
| `crates/vault/server/src/server.rs` | The startup banner prints the real `POST /v1/import` contract. |
| `crates/vault/server/src/test_support.rs` | Gains `post_raw` for a body that is not JSON, and stops naming a file that does not exist. |
| `crates/vault/server/src/conversations_api.rs:135` | Doc comment says `in:#<id>`. |
| `CLAUDE.md`, `AGENTS.md`, `docs/src/content/docs/vault/developer/message-transfer.md` | Say schema version 4. |
| `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts` | Regenerated because a schema description changed. |

---

### Task 1: `ImportFailure`, raised by the parser

**Files:**
- Create: `crates/vault/server/src/import/failure.rs`
- Modify: `crates/vault/server/src/import/mod.rs:35-40` (module list)
- Modify: `crates/vault/server/src/models.rs:141-180` (`parse_ir_lines`)
- Test: `crates/vault/server/src/import/failure.rs` (unit tests inside the file)

**Interfaces:**
- Produces: `crate::import::ImportFailure` with variants `SchemaVersion { found: u32, expected: u32, line: usize }` and `Parse { line: usize, detail: String }`, `impl Display`, `impl std::error::Error`, and `ImportFailure::in_error(&anyhow::Error) -> Option<&ImportFailure>`.
- Produces: `models::parse_ir_lines` now returns an `anyhow::Error` whose root cause is an `ImportFailure` for the two cases above.

- [ ] **Step 1: Write the failing tests**

Create `crates/vault/server/src/import/failure.rs` with only the tests and a `use` line, so the file compiles once the type exists:

```rust
//! The reasons an import stops that the person who sent the file can act on.
//!
//! Everything else an import returns is an internal failure: the person
//! cannot fix it by changing the file, so the HTTP interface reports it as a
//! 500 and keeps the cause on stderr.

use std::fmt;

#[cfg(test)]
mod tests {
    use super::ImportFailure;

    #[test]
    fn schema_version_names_both_versions_and_the_line() {
        let f = ImportFailure::SchemaVersion {
            found: 3,
            expected: 4,
            line: 1,
        };
        assert_eq!(
            f.to_string(),
            "This file is schema version 3; the vault reads version 4 (line 1)."
        );
    }

    #[test]
    fn parse_names_the_line_and_the_detail() {
        let f = ImportFailure::Parse {
            line: 12,
            detail: "expected value at line 1 column 1".into(),
        };
        assert_eq!(
            f.to_string(),
            "Could not read line 12 of the file: expected value at line 1 column 1."
        );
    }

    #[test]
    fn in_error_finds_the_failure_under_anyhow_context() {
        use anyhow::Context;
        let root: anyhow::Error = ImportFailure::Parse {
            line: 2,
            detail: "boom".into(),
        }
        .into();
        let wrapped = root
            .context("failed to parse message-ir JSONL in /tmp/x.jsonl")
            .context("import failed");
        let found = ImportFailure::in_error(&wrapped).expect("failure survives context");
        assert_eq!(
            *found,
            ImportFailure::Parse {
                line: 2,
                detail: "boom".into()
            }
        );
    }

    #[test]
    fn in_error_is_none_for_other_errors() {
        let err = anyhow::anyhow!("disk full");
        assert!(ImportFailure::in_error(&err).is_none());
    }
}
```

Register the module in `crates/vault/server/src/import/mod.rs` next to the others:

```rust
pub mod contact_name;
pub mod failure;
pub mod promote;
pub mod staging;

pub use contact_name::ContactNameMode;
pub use failure::ImportFailure;
pub use staging::is_orphaned_export;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server import::failure`
Expected: compile error, `cannot find type ImportFailure`.

- [ ] **Step 3: Write the type**

Add above the `#[cfg(test)]` block in `failure.rs`:

```rust
/// A reason an import stopped that the sender can fix by changing the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportFailure {
    /// The conversation header's `schema_version` is not the one this vault
    /// reads. Nothing is upgraded: the sender re-exports with current tools.
    SchemaVersion {
        found: u32,
        expected: u32,
        line: usize,
    },
    /// A line is not the message-ir JSON the vault expects: not JSON at all,
    /// a header or message with the wrong fields, or a message before any
    /// header.
    Parse { line: usize, detail: String },
}

impl fmt::Display for ImportFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaVersion {
                found,
                expected,
                line,
            } => write!(
                f,
                "This file is schema version {found}; the vault reads version {expected} (line {line})."
            ),
            Self::Parse { line, detail } => {
                write!(f, "Could not read line {line} of the file: {detail}.")
            }
        }
    }
}

impl std::error::Error for ImportFailure {}

impl ImportFailure {
    /// The person-actionable failure inside `err`, if there is one.
    ///
    /// The import pipeline wraps errors in `anyhow` context on the way up;
    /// `downcast_ref` looks through every layer of context, so the parser can
    /// raise this type and the HTTP handler can find it without the layers in
    /// between knowing about it.
    pub fn in_error(err: &anyhow::Error) -> Option<&ImportFailure> {
        err.downcast_ref::<ImportFailure>()
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server import::failure`
Expected: 4 passed.

- [ ] **Step 5: Write the failing parser tests**

Append to the `mod tests` block at the bottom of `crates/vault/server/src/models.rs` (there is an existing test module; if there is none, create `#[cfg(test)] mod tests { use super::*; ... }`):

```rust
    #[test]
    fn parse_ir_lines_refuses_schema_3_as_a_failure() {
        let header = r#"{"schema_version":3,"export":{"source":"whatsapp","tool":"t","owner_handle":"+1","owner_display_name":"Me"},"conversation":{"chat_identifier":"+2","conversation_type":"individual","participants":[]}}"#;
        let err = parse_ir_lines([header]).unwrap_err();
        let failure = crate::import::ImportFailure::in_error(&err).expect("typed failure");
        assert_eq!(
            *failure,
            crate::import::ImportFailure::SchemaVersion {
                found: 3,
                expected: message_ir::SCHEMA_VERSION,
                line: 1
            }
        );
    }

    #[test]
    fn parse_ir_lines_reports_a_non_json_line_as_a_failure() {
        let err = parse_ir_lines(["this is not json"]).unwrap_err();
        let failure = crate::import::ImportFailure::in_error(&err).expect("typed failure");
        match failure {
            crate::import::ImportFailure::Parse { line, .. } => assert_eq!(*line, 1),
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn parse_ir_lines_reports_a_message_before_any_header_as_a_failure() {
        let err = parse_ir_lines([r#"{"guid":"m1"}"#]).unwrap_err();
        let failure = crate::import::ImportFailure::in_error(&err).expect("typed failure");
        match failure {
            crate::import::ImportFailure::Parse { line, detail } => {
                assert_eq!(*line, 1);
                assert!(detail.contains("before the conversation header"), "{detail}");
            }
            other => panic!("expected Parse, got {other:?}"),
        }
    }
```

- [ ] **Step 6: Run the parser tests to verify they fail**

Run: `cargo test -p message-vault-server models::tests::parse_ir_lines`
Expected: 3 failed, each on `expect("typed failure")`.

- [ ] **Step 7: Raise `ImportFailure` in `parse_ir_lines`**

Replace the body of `parse_ir_lines` in `crates/vault/server/src/models.rs` (the function at line 141) with:

```rust
pub fn parse_ir_lines(
    lines: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<Vec<ExportRecord>> {
    use crate::import::ImportFailure;

    let mut out = Vec::new();
    let mut saw_header = false;
    for (i, line) in lines.into_iter().enumerate() {
        let line = line.as_ref().trim();
        if line.is_empty() {
            continue;
        }
        let line_no = i + 1;
        let value: Value = serde_json::from_str(line).map_err(|e| ImportFailure::Parse {
            line: line_no,
            detail: e.to_string(),
        })?;
        if is_ir_header(&value) {
            let header: ConversationHeader =
                serde_json::from_value(value).map_err(|e| ImportFailure::Parse {
                    line: line_no,
                    detail: format!("the conversation header is not valid: {e}"),
                })?;
            if header.schema_version != SCHEMA_VERSION {
                return Err(ImportFailure::SchemaVersion {
                    found: header.schema_version,
                    expected: SCHEMA_VERSION,
                    line: line_no,
                }
                .into());
            }
            out.push(ExportRecord::Conversation(conversation_from_ir(&header)));
            saw_header = true;
        } else {
            if !saw_header {
                return Err(ImportFailure::Parse {
                    line: line_no,
                    detail: "a message appears before the conversation header".into(),
                }
                .into());
            }
            let msg: IrMessage = serde_json::from_value(value).map_err(|e| ImportFailure::Parse {
                line: line_no,
                detail: format!("the message is not valid: {e}"),
            })?;
            out.push(ExportRecord::Message(message_from_ir(&msg)?));
        }
    }
    if out.is_empty() {
        return Err(ImportFailure::Parse {
            line: 1,
            detail: "the file has no conversation header".into(),
        }
        .into());
    }
    Ok(out)
}
```

Then fix the import line at the top of `models.rs`: `use anyhow::{Context, Result, bail};` becomes `use anyhow::{Context, Result};` if `bail` and `Context` are no longer used elsewhere in the file. Run `cargo build -p message-vault-server` and remove whichever of the two the compiler reports as unused.

- [ ] **Step 8: Run the parser tests and the whole crate**

Run: `cargo test -p message-vault-server models::tests::parse_ir_lines`
Expected: 3 passed.

Run: `cargo test -p message-vault-server`
Expected: all pass. If a test elsewhere asserted the old free-text wording (`unsupported schema_version`, `missing conversation header`), update its expected string to the new sentence from `Display`.

- [ ] **Step 9: Commit**

```bash
git add crates/vault/server/src/import/failure.rs crates/vault/server/src/import/mod.rs crates/vault/server/src/models.rs
git commit -m "feat(vault-server): name the two import failures a person can fix

The JSONL parser used to stop with a free-text message for a wrong schema
version or a line that was not message-ir JSON. Nothing above it could
tell those apart from a disk error, so the HTTP route reported all of
them as 'internal server error'.

This adds one type, ImportFailure, for exactly those two cases. The
parser raises it, and it survives the anyhow context the pipeline adds
on the way up, so a caller at the edge can find it with one downcast.
The sentences are written for the person who sent the file: which
version the file has, which the vault reads, and which line.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SbrfkTMUYjNCGvAgDkdyfw"
```

---

### Task 2: The HTTP route reports a bad file as a 400

**Files:**
- Modify: `crates/vault/server/src/test_support.rs` (add `post_raw`; fix the header comment at lines 7-8)
- Modify: `crates/vault/server/src/import/mod.rs:1673` (`let stats = import_result?;` in `run_import_path`)
- Test: `crates/vault/server/src/import/mod.rs` `mod tests` (HTTP tests)

**Interfaces:**
- Consumes: `crate::import::ImportFailure::in_error` from Task 1.
- Produces: `crate::test_support::post_raw(state, path, token, content_type, body) -> (StatusCode, String)`; later plans use it for any non-JSON body.

- [ ] **Step 1: Add the raw-body helper and fix the stale comment**

In `crates/vault/server/src/test_support.rs`, change lines 7-8 of the header comment from:

```rust
//! HTTP, for tests in `auth.rs`, and (per the authorization-model plan)
//! `accounts_api.rs` and related modules.
```

to:

```rust
//! HTTP, for tests in `auth.rs`, `admin_api.rs`, `api_tokens_api.rs`, and
//! any route whose contract is worth checking end to end.
```

Then append after `delete_json`:

```rust
/// POST a body that is not JSON (JSONL, plain text, an empty body) with a
/// Bearer token and an explicit Content-Type, returning the status and the
/// response text. For routes whose contract is the raw body, such as
/// `POST /v1/import`.
pub async fn post_raw(
    state: &AppState,
    path: &str,
    token: &str,
    content_type: &str,
    body: impl Into<reqwest::Body>,
) -> (StatusCode, String) {
    let app = http_app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://{address}{path}"))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(body)
        .send()
        .await
        .unwrap();
    server.abort();
    let status = response.status();
    let text = response.text().await.unwrap();
    (status, text)
}
```

- [ ] **Step 2: Write the failing HTTP tests**

Append to the `mod tests` block at the bottom of `crates/vault/server/src/import/mod.rs`:

```rust
    /// A registered account may import with its session token: `can_import`
    /// is on by default, which `server.rs`'s `can_import = 0` test relies on
    /// to prove the opposite case.
    async fn importer() -> (crate::server::AppState, crate::test_support::TestVault, String) {
        let vault = crate::test_support::test_vault().await;
        let account =
            crate::test_support::register_via_api(&vault.state, "importer", "hunter2hunter2")
                .await;
        let state = vault.state.clone();
        (state, vault, account.token)
    }

    #[tokio::test]
    async fn http_import_of_a_schema_3_file_is_a_400_naming_both_versions() {
        let (state, _vault, token) = importer().await;
        let body = concat!(
            r#"{"schema_version":3,"export":{"source":"whatsapp","tool":"t","owner_handle":"+15550000001","owner_display_name":"Me"},"#,
            r#""conversation":{"chat_identifier":"+15550000002","conversation_type":"individual","participants":[{"handle":"+15550000002","display_name":"Sam"}]}}"#,
            "\n",
        );
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/import?source=whatsapp",
            &token,
            "application/jsonl",
            body,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{text}");
        let err: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            err["error"],
            "This file is schema version 3; the vault reads version 4 (line 1)."
        );
    }

    #[tokio::test]
    async fn http_import_of_a_line_that_is_not_json_is_a_400_naming_the_line() {
        let (state, _vault, token) = importer().await;
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/import?source=whatsapp",
            &token,
            "application/jsonl",
            "this is not json\n",
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{text}");
        let err: serde_json::Value = serde_json::from_str(&text).unwrap();
        let message = err["error"].as_str().unwrap();
        assert!(
            message.starts_with("Could not read line 1 of the file:"),
            "{message}"
        );
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server import::tests::http_import_of`
Expected: 2 failed; both report status `500 Internal Server Error` with body `{"ok":false,"error":"internal server error"}`.

- [ ] **Step 4: Classify the import result at the seam**

In `crates/vault/server/src/import/mod.rs`, inside `run_import_path`, the line `let stats = import_result?;` (around line 1673) becomes:

```rust
    let stats = import_result.map_err(classify_import_error)?;
```

Add this function just above `async fn run_import_path`:

```rust
/// Turn an import's error into the HTTP failure a caller should see.
///
/// The two failures a sender can fix by changing the file travel up the
/// pipeline as `ImportFailure` and become a 400 with their own sentence.
/// Everything else (a disk or database error, a bug) is a 500: the message
/// goes to stderr and the client sees "internal server error".
fn classify_import_error(err: anyhow::Error) -> ApiError {
    match ImportFailure::in_error(&err) {
        Some(failure) => ApiError::BadRequest(failure.to_string()),
        None => ApiError::Internal(format!("{err:#}")),
    }
}
```

Both HTTP paths (`application/jsonl` at line 1470 and multipart at line 1546) call `run_import_path`, so one change covers both.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server import::tests::http_import_of`
Expected: 2 passed.

Run: `cargo test -p message-vault-server`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/test_support.rs crates/vault/server/src/import/mod.rs
git commit -m "fix(vault-server): answer a bad import file with a 400 that says what is wrong

POST /v1/import returned 'internal server error' for every failure,
including a file in an old schema version or a line that was not JSON.
The real reason was printed to the server's stderr, where the person
who sent the file could not see it.

The route now looks for the typed ImportFailure at the edge and returns
a 400 with its sentence. Anything else is still a 500, because the
sender cannot fix a disk or database error by changing the file.

The test helpers gain post_raw for routes whose contract is the raw
body, and stop naming a file that no longer exists.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SbrfkTMUYjNCGvAgDkdyfw"
```

---

### Task 3: `source` is checked once, and its absence is a JSON 400

**Files:**
- Modify: `crates/vault/server/src/import/mod.rs:540` (`ImportQuery.source`), `:1448` and `:1579` (the two `validate_source_id` calls)
- Modify: `crates/vault/server/src/server.rs:549-553` (banner)
- Test: `crates/vault/server/src/import/mod.rs` `mod tests`

**Interfaces:**
- Consumes: `post_raw` from Task 2.
- Produces: nothing new. `POST /v1/import` without `source` returns `400 {"ok":false,"error":"query param source is required"}`.

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `crates/vault/server/src/import/mod.rs`:

```rust
    #[tokio::test]
    async fn http_import_without_source_is_a_json_400() {
        let (state, _vault, token) = importer().await;
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/import",
            &token,
            "application/jsonl",
            "{}\n",
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{text}");
        let err: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("expected a JSON error body, got: {text}"));
        assert_eq!(err["error"], "query param source is required");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p message-vault-server import::tests::http_import_without_source`
Expected: FAIL on `expected a JSON error body`; the body is Axum's plain text `Failed to deserialize query string: missing field \`source\``.

- [ ] **Step 3: Default `source` and validate it once**

In `ImportQuery` (line 540) change:

```rust
    source: String,
```

to:

```rust
    /// Source slug the import registers its data under. Required; checked in
    /// the handler so a missing value is the JSON 400 every other failure is.
    #[serde(default)]
    source: String,
```

In `run_import_path` (line 1579) delete the second check:

```rust
    validate_source_id(&query.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
```

The one in `import_handler` (line 1448) stays. Confirm with `grep -n validate_source_id crates/vault/server/src/import/mod.rs` that the handler is now the only call on the HTTP path.

- [ ] **Step 4: Fix the banner**

In `crates/vault/server/src/server.rs`, replace the five `eprintln!` lines that start at `POST /v1/import?source=` with:

```rust
    eprintln!("  POST /v1/import?source=<slug>&mode=append|replace&dedupe=false&import_id=&account=");
    eprintln!("       source= required: a short name such as whatsapp or imessage");
    eprintln!("       account= optional (must match token); derived from Bearer when omitted");
    eprintln!("       Content-Type: application/jsonl  (body only; assets by sha256)");
    eprintln!("       Content-Type: multipart/form-data   (field jsonl + file parts; remote push)");
    eprintln!("       A file the vault cannot read is a 400 that names the line; export routes are read-only");
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server import::tests::http_import`
Expected: 3 passed.

Run: `cargo test -p message-vault-server`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/import/mod.rs crates/vault/server/src/server.rs
git commit -m "fix(vault-server): say plainly that POST /v1/import needs a source

Leaving source= off the import request produced a plain-text error from
the framework instead of the JSON error the handler was written to
send, because the field was required by the query parser before the
handler ran. The value is now defaulted and checked in the handler, once
rather than twice, so a missing source is the same JSON 400 as any other
bad input. The startup banner now prints that source is required.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SbrfkTMUYjNCGvAgDkdyfw"
```

---

### Task 4: A database error while editing a contact is a 500, not a 400

**Files:**
- Modify: `crates/vault/server/src/contacts_api.rs:1217-1229` (`contact_mutate_handler`)
- Test: `crates/vault/server/src/contacts_api.rs` `mod tests`

**Interfaces:**
- Produces: `contacts_api::classify_mutation_error(anyhow::Error) -> ApiError` (private to the file).

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `crates/vault/server/src/contacts_api.rs`:

```rust
    #[test]
    fn a_database_error_while_editing_a_contact_is_internal() {
        let err = anyhow::Error::from(sqlx::Error::PoolClosed).context("update contact");
        match super::classify_mutation_error(err) {
            crate::server::ApiError::Internal(_) => {}
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn a_validation_error_while_editing_a_contact_is_bad_request() {
        let err = anyhow::anyhow!("handle already belongs to another contact");
        match super::classify_mutation_error(err) {
            crate::server::ApiError::BadRequest(m) => {
                assert_eq!(m, "handle already belongs to another contact");
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server contacts_api::tests::a_database_error`
Expected: compile error, `cannot find function classify_mutation_error`.

- [ ] **Step 3: Add the classifier and use it**

Above `contact_mutate_handler` in `crates/vault/server/src/contacts_api.rs`:

```rust
/// Turn a contact edit's error into the HTTP failure a caller should see.
///
/// `mutate_contact` returns `anyhow` so that its validation messages ("handle
/// already belongs to another contact") reach the person. A database error
/// is not something the person can fix by changing the request, so it is a
/// 500 with the cause on stderr rather than a 400 wearing sqlx's words.
fn classify_mutation_error(err: anyhow::Error) -> ApiError {
    if err.downcast_ref::<sqlx::Error>().is_some() {
        ApiError::Internal(format!("{err:#}"))
    } else {
        ApiError::BadRequest(err.to_string())
    }
}
```

In `contact_mutate_handler` change:

```rust
        Err(e) => Err(ApiError::BadRequest(e.to_string())),
```

to:

```rust
        Err(e) => Err(classify_mutation_error(e)),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server contacts_api::tests`
Expected: all pass, including the two new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/contacts_api.rs
git commit -m "fix(vault-server): stop reporting a database error as a bad contact edit

Every error from editing a contact became a 400, including database
failures, so a broken connection read as if the person had sent a bad
request. Database errors are now a 500 with the cause on stderr, and
only the edit's own validation messages come back as a 400.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SbrfkTMUYjNCGvAgDkdyfw"
```

---

### Task 5: Every document says schema version 4, and the generated files follow

**Files:**
- Modify: `CLAUDE.md:20,25`
- Modify: `AGENTS.md:298`
- Modify: `docs/src/content/docs/vault/developer/message-transfer.md:38,44`
- Modify: `crates/vault/server/src/conversations_api.rs:135`
- Regenerate: `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts`

**Interfaces:** none.

- [ ] **Step 1: Fix the four documents**

`CLAUDE.md` line 20: `→ ConversationDocument (schema_version 3) written as JSONL` becomes `→ ConversationDocument (schema_version 4) written as JSONL`.

`CLAUDE.md` line 25: `` `schema_version` is `3` and independent of the product version. `` becomes `` `schema_version` is `4` and independent of the product version. A version-3 file is refused by name, never upgraded. ``

`AGENTS.md` line 298: `| JSONL schema    | `schema_version: 3` | ...` becomes `| JSONL schema    | `schema_version: 4` | Shared chat file format. Independent of the product version. Version 3 is refused, never upgraded. |`

`docs/src/content/docs/vault/developer/message-transfer.md` line 38: `(`schema_version` 3)` becomes `(`schema_version` 4)`; line 44: `{"schema_version":3,` becomes `{"schema_version":4,`.

- [ ] **Step 2: Fix the schema description and regenerate**

`crates/vault/server/src/conversations_api.rs` line 135:

```rust
    /// Numeric `conversations.id`, serialized as a string for `in:<id>` queries.
```

becomes:

```rust
    /// Numeric `conversations.id`, serialized as a string; search for it as `in:#<id>`.
```

Run:

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
cd web && npm run gen:api && cd ..
git diff --stat docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
```

Expected: both files change on exactly the one description line.

- [ ] **Step 3: Confirm nothing else still says 3**

Run: `grep -rn 'schema_version.*3\b\|schema version 3' CLAUDE.md AGENTS.md docs/src/content | grep -v 'refused\|Version 3 is\|version 3;'`
Expected: no output.

- [ ] **Step 4: Run the full check**

Run: `./scripts/check-pr.sh`
Expected: passes end to end (rustfmt, workspace build and tests, Tauri build, Biome, Vitest, docs). The OpenAPI drift test in `crates/vault/server/src/openapi.rs` passes because the JSON was regenerated.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md AGENTS.md docs/src/content/docs/vault/developer/message-transfer.md crates/vault/server/src/conversations_api.rs docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "docs: say the JSONL schema is version 4 everywhere

The shared file format moved to version 4 in #286, but CLAUDE.md,
AGENTS.md, and the transfer page still said 3, and the conversation
id's description still showed the old search spelling. All four now
match the code, and the generated OpenAPI document and web types follow.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SbrfkTMUYjNCGvAgDkdyfw"
```

---

## Self-review

**Spec coverage** (spec section "Import failures"): typed failure with statuses — Tasks 1, 2 (the spec's `MissingAsset` and `AccountMismatch` rows are not implemented: a missing attachment is counted in `assets_missing` and the import succeeds today, and the account mismatch is already a 403 from `resolve_import_account`; the spec's table is adjusted to say so). `ImportQuery.source` reaches the handler — Task 3. `validate_source_id` once — Task 3. `contact_mutate_handler` — Task 4. Banner — Task 3. Three documents, `test_support.rs:7`, `conversations_api.rs:135` — Tasks 2 and 5.

**Placeholders:** none; every step has its code or its exact command.

**Type consistency:** `ImportFailure::in_error` is the name in Task 1 and in Tasks 2 and 3's tests; `post_raw` returns `(StatusCode, String)` in Task 2 and is consumed that way in Task 3; `classify_import_error` and `classify_mutation_error` are each defined once and used once.
