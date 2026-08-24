# CLI tools follow-up design — 2026-08-23

Product Rust audit follow-up, group 4 of 5: the 12 CLI-tools findings from
`docs/superpowers/reports/2026-08-23-rust-audit.md`. Scope: `vault-push`,
`vault-pull`, `dump-cli-docs`, `demo-seed`, plus two new crates in `crates/libs`
(`journal` and `vault-http`). Groups 1–3 (server, libs, exporters) are merged;
group 5 (Tauri host, 7 findings) follows in its own cycle.

## Goal

Extract the duplicated JSONL journal and the vault HTTP client bits into two
focused libs crates; replace substring-based retry classification with a typed
classifier (all 4xx permanent); wire demo-seed's dead config fields into the
generator; document the `dump-cli-docs` public surface and the small pub-API
doc gaps; and make the help text honest (`--mode`, demo-seed `about`) — with
byte-identical CLI help everywhere except the one sanctioned `--mode` rewrite,
and a byte-identical demo dataset.

## The 12 findings

| # | Sev | Category | Finding | Anchor |
|---|---|---|---|---|
| 1 | medium | duplication | JSONL journal module implemented twice (vault-push and vault-pull) | `crates/cli/vault-push/src/journal.rs:131` |
| 2 | low | duplication | `truncate()` and `HttpSession::new` boilerplate duplicated across the two CLI crates | `crates/cli/vault-push/src/http.rs:174` |
| 3 | low | duplication | `truncate()` and `HttpSession::new()` duplicated between vault-push and vault-pull (vault-pull already depends on `vault_push::authenticate`) | `crates/cli/vault-pull/src/http.rs:395` |
| 4 | medium | docs | dump-cli-docs is a library crate with a fully undocumented public API | `crates/cli/dump-cli-docs/src/lib.rs:3` |
| 5 | low | docs | `pub fn clap_command()` has no doc comment in either CLI crate | `crates/cli/vault-push/src/cli.rs:92` |
| 6 | low | docs | `DEFAULT_PAGE_LIMIT` const lacks a doc while its sibling const has one | `crates/cli/vault-pull/src/run.rs:22` |
| 7 | low | docs | `--mode` help uses invented term 'resume-safe' and hides the destructive replace | `crates/cli/vault-push/src/cli.rs:34` |
| 8 | low | docs | CLI `about` line claims iMessage-only while the tool generates three backup sources | `crates/vault/demo-seed/src/main.rs:11` |
| 9 | low | dead code | Dead config fields look live: `first_last` and `labels.names` are parsed but never read | `crates/vault/demo-seed/src/config.rs:55` |
| 10 | low | bug | `truncate()` slices at a byte offset and can panic on a UTF-8 boundary | `crates/cli/vault-push/src/http.rs:174` |
| 11 | low | design | Retry classification relies on substring-matching formatted error text | `crates/cli/vault-push/src/http.rs:800` |
| 12 | low | hygiene | `sha2` dependency is unused in vault-pull and pins a different major than vault-push | `crates/cli/vault-pull/Cargo.toml:27` |

## Adjudicated decisions

Three decisions were made with the user on 2026-08-23 before this spec was
written:

1. **Two focused crates.** The shared code becomes `crates/libs/journal`
   (generic JSONL journal mechanics) and `crates/libs/vault-http` (client
   builder, `truncate`, retry machinery, `AuthError`/`AuthInfo`) — not one
   grab-bag `cli-shared` crate, and not vault-push-hosted re-exports.
2. **Retry: typed, all 4xx permanent.** The typed classifier adopts the
   documented intent ("Permanent errors (4xx auth, 413, malformed input)
   should not be retried"). Small deltas: currently-unknown 4xx statuses
   (400/405/422…) and 429 stop being retried. Catalogued in Behavior deltas.
3. **demo-seed: wire the fields in.** `contacts.first_last` and
   `labels.names` become live config (not deleted). The shipped
   `demo_seed.toml` values exactly match today's hard-coded numbers and
   strings, so the generated dataset stays identical.

## Crate layout and dependency edges

```
vault-push ──► journal, vault-http, message-vault-io-core, message-ir, message-ir-format
vault-pull ──► journal, vault-http, vault-push (authenticate), message-vault-io-core, message-ir
dump-cli-docs ► unchanged (still depends on vault-push/vault-pull with the cli feature)
demo-seed ───► unchanged
```

Hard constraints:

- `journal` and `vault-http` must not depend on `vault-push`, `vault-pull`, or
  each other (no cycles; `vault-http` carries `AuthError`, so
  `vault-pull → vault-http` replaces its old `vault-pull → vault-push`
  auth-type edge).
- Both new crates are `#![warn(missing_docs)]`-gated like the other libs
  crates, and all new doc text follows the rustdoc style guide
  (`docs/src/content/docs/vault/developer/rustdoc-style.md`).
- Existing public paths stay reachable via re-exports (see Curated surfaces).

## New crate: `journal` (crates/libs/journal)

Generic JSON Lines state-journal mechanics. The event enums, state types,
filename consts, and the semantic folding stay in their own crates; only the
file mechanics are shared.

Proposed surface (exact shapes are finalized in the implementation plan with
compile checks):

```rust
//! JSON Lines state journals: append-only logs rewritten by sorted compaction.

/// Append one serialized event as a single JSON Lines row and flush it.
/// Serializes to a buffer first so a mid-line failure cannot tear a row.
pub fn append<E: serde::Serialize>(path: &Path, event: &E) -> anyhow::Result<()>;

/// Parse every event from a journal file. A missing file yields an empty
/// list. Each corrupt line is reported to `on_corrupt(line_number, parse_error)`
/// and skipped — the caller decides whether to warn (push) or stay silent (pull).
pub fn load_events<E: serde::de::DeserializeOwned>(
    path: &Path,
    on_corrupt: &mut dyn FnMut(usize, &serde_json::Error),
) -> anyhow::Result<Vec<E>>;

/// Rewrite the journal from a list of events: write a `jsonl.tmp` file, flush,
/// and rename over the original. Serialization and the rewrite run under one
/// process-wide write lock shared with [`append`].
pub fn rewrite<E: serde::Serialize>(path: &Path, events: &[E]) -> anyhow::Result<()>;
```

Details pinned from today's code:

- The static `JOURNAL_WRITE_LOCK` moves into this crate (one lock, not two).
  Push already locks append+compact; pull's append/compact gain the lock —
  pull is single-threaded today, so nothing observable changes.
- Error context strings move verbatim (e.g. `open journal for append {path}`,
  `read journal line {n}`), so CLI error output is unchanged.
- The tmp file name stays `path.with_extension("jsonl.tmp")` on both sides.
- Push's `compact` keeps its preserve-other-targets semantics by composing
  `load_events` (filter by url/username) + `rewrite`. Pull's `compact`
  (rebuild from state, drop everything else) composes state-to-events +
  `rewrite`. Both keep their existing sort orders.
- Push's `append` keeps `create_dir_all(parent)`; pull's does too (both do
  today).

The push and pull `journal.rs` modules remain, now thin wrappers: consts,
event enums, state types, `journal_path`, and the semantic load/compact
functions. `vault_pull::journal` stays a pub module with the same items;
`vault_push::journal` stays private with the same lib.rs re-exports.

## New crate: `vault-http` (crates/libs/vault-http)

Shared vault HTTP client utilities and the typed retry machinery.

Proposed surface:

```rust
//! Blocking HTTP client helpers and retry classification for the vault CLI
//! crates.

/// Build the shared reqwest client (connection pool, 16 idle hosts per host).
pub fn build_client() -> anyhow::Result<reqwest::blocking::Client>;

/// Copy `s`, cutting it to at most `max` bytes on a char boundary and adding
/// an ellipsis when truncated.
pub fn truncate(s: &str, max: usize) -> String;

/// Whether an error is likely to succeed on retry.
pub enum RetryKind { Transient, Permanent }

/// Classify an error for [`with_retries`].
pub fn classify_retry(error: &anyhow::Error) -> RetryKind;

/// Run `op` again on transient failures, with exponential backoff and jitter,
/// up to `max_retries` extra tries.
pub fn with_retries<T, F>(max_retries: u32, op: F) -> anyhow::Result<T>;

/// Marker context attached at payload-too-large bail sites so the classifier
/// can recognize them without parsing message text.
pub struct PayloadTooLarge;

pub enum AuthError { /* moved verbatim from vault-push::auth_error */ }
pub struct AuthInfo { /* moved verbatim from vault-push::http */ }
```

Details:

- `build_client` is the byte-identical `Client::builder().pool_max_idle_per_host(16)`
  block (context `build HTTP client`), used by both `HttpSession::new`
  constructors.
- `truncate` slices on `s.floor_char_boundary(max)` instead of `s[..max]`
  (finding 10). Same doc text, same ellipsis format.
- `AuthError` moves verbatim from `vault-push/src/auth_error.rs` (variants,
  `kind()`, `user_message()`, `detail()`, `Display`, and its tests);
  `AuthInfo` moves verbatim from `vault-push/src/http.rs` with its serde
  derives. The auth-flow helpers that stay push-specific
  (`classify_auth_http_status`, `classify_unauthorized`, `auth_check`,
  `authenticate`, `HttpSession` itself) remain in vault-push and now construct
  `vault_http::AuthError` values.
- `with_retries` keeps today's exact backoff/jitter schedule (`rand_factor`
  moves with it). It is only called by vault-push; vault-pull gains no retries.

### Retry classification contract

`classify_retry` walks the error in this order and returns `Permanent` for the
first match:

| Error source | Condition | RetryKind |
|---|---|---|
| context marker | `PayloadTooLarge` context present anywhere in the chain | Permanent |
| `vault_http::AuthError` | `InvalidKey`, `Forbidden`, `ApiNotFound`, `Rejected`, `RateLimited` | Permanent |
| `vault_http::AuthError` | `HttpStatus { status, .. }` with `400 <= status < 500` | Permanent |
| `vault_http::AuthError` | any other variant (`Network`, `Timeout`, `ReadResponse`, `WrongHostHtml`, `ServerError`, `Client`, `BadJson`, `InvalidUrl`, `HttpsRequired`, `MissingAccountId`, non-4xx `HttpStatus`) | Transient |
| `reqwest::Error` | `error.status()` is `Some(s)` with `400 <= s < 500` | Permanent |
| `reqwest::Error` | no status, or 5xx | Transient |
| `std::io::Error` | `ErrorKind::NotFound` | Permanent |
| `std::io::Error` | any other kind | Transient |
| anything unrecognized | — | Transient (today's default) |

- The 413 bail sites in vault-push (`looks_like_payload_too_large` /
  `payload_too_large_message` callers) gain `.context(vault_http::PayloadTooLarge)`
  so classification is typed instead of string-matched.
- `with_retries`'s call sites, signature, retry counts, and backoff bounds are
  unchanged. Only the classifier is replaced.
- Today's auth-string cases map cleanly: `Forbidden` covers 403 and
  "username does not match" bodies; `Rejected` covers "invalid vault key"
  server text; 401 lands in `HttpStatus` (4xx → Permanent); 404 lands in
  `ApiNotFound`/`HttpStatus` (Permanent); 413 is the marker.

## demo-seed wiring (findings 8, 9)

- `ContactsConfig::first_last`: drop `#[allow(dead_code)]` and the stale
  comment. `sample_name_shape` becomes a three-way partition:
  `roll < first_only` → first-only; `roll < first_only + first_middle_last` →
  first-middle-last; `roll < first_only + first_middle_last + first_last` →
  first-last; anything at or past the sum falls through to first-last
  (defensive; unreachable with the shipped values). The shipped
  `first_last = 0.96` plus `first_only = 0.02` and `first_middle_last = 0.02`
  sum to 1.0, which is exactly today's implicit third branch — same RNG
  stream, same names.
- `LabelsConfig::names`: drop `#[allow(dead_code)]`. Group labels are read by
  position: `family` roll → `names[0]`, `work` → `names[1]`, `college` →
  `names[2]`, inactive contact → `names[3]`. `SeedConfig::load` validates
  `names.len() == 4` with a clear error naming the required shape. The shipped
  `names = ["Family", "Work", "College", "Inactive"]` equals today's
  hard-coded strings — same output.
- `about` becomes `"Generate the demo message dataset (iMessage, SMS Backup &
  Restore, WhatsApp) for Message Vault"`. demo-seed is not in `PAGE_SPECS`, so
  no committed CLI page changes.
- Dataset identity: wiring these fields adds no random draws and reorders
  none, so with the shipped `demo_seed.toml` the generated dataset is
  byte-identical. Unit tests pin the name-shape partition and the label
  mapping; a validation-error test pins the 4-names requirement.

## Docs and help-text fixes (findings 4–7, 12)

- **dump-cli-docs**: add a `//!` crate intro, `#![warn(missing_docs)]`, and
  doc comments for `PageSpec` (struct + all four fields), `PAGE_SPECS`,
  `render_page`, `command_for` (# Errors), `page_markdown` (# Errors), and
  `write_pages` (# Errors). The crate stays a library consumed by its
  `main.rs` and tests — no API shape changes.
- **`clap_command` docs** (both crates): one line each, e.g. `/// The clap
  \`Command\` for embedding --help output into the docs pages and GUI.`
- **`DEFAULT_PAGE_LIMIT`**: `/// Default page size for GET
  /v1/export/messages`.
- **`--mode` help** (finding 7): rewrite the doc string to
  `"append: add to existing data (safe to re-run); replace: delete existing
  messages for this source, then import"`.
- **`sha2`**: remove from vault-pull's dependencies (unused; vault-push keeps
  `sha2 0.11`).
- **Sanctioned page regeneration**: the `--mode` rewrite changes
  `docs/src/content/docs/vault/developer/reference/cli/vault-push.md`.
  Regenerate once with
  `cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference`.
  `committed_cli_pages_match_dump` pins all 11 pages; only `vault-push.md`
  should differ.

## Curated surfaces

- **journal** exports exactly `append`, `load_events`, `rewrite` — nothing
  else pub.
- **vault-http** exports `build_client`, `truncate`, `RetryKind`,
  `classify_retry`, `with_retries`, `PayloadTooLarge`, `AuthError`, `AuthInfo`
  — nothing else pub.
- **vault-push** lib.rs keeps every existing export path. `AuthError` and
  `AuthInfo` become `pub use vault_http::{AuthError, AuthInfo};` so
  `vault_push::AuthError` etc. still compile everywhere (Tauri, GUI,
  dump-cli-docs, vault-pull).
- **vault-pull** replaces `pub use vault_push::{AuthError, AuthInfo,
  authenticate};` with `pub use vault_http::{AuthError, AuthInfo};` and keeps
  `pub use vault_push::authenticate;` (authenticate stays a push function).
- **dump-cli-docs** keeps the same items, now documented.

## Testing and verification

- **journal**: port push's three tests (legacy+batch load, compact preserves
  other targets, append contention) and pull's three tests (load, filter by
  url/username, compact sorts) to the crate against a local test event type;
  add corrupt-line callback and rewrite tests. Push/pull keep thin wrapper
  tests where semantics live there (e.g. multi-target preservation).
- **vault-http**: `truncate` unit tests (empty, under-limit, exact-boundary,
  multibyte char split at `max`, non-ASCII input); `classify_retry` unit tests
  per table row using constructed `AuthError` values, marker contexts, and
  `std::io::Error`s — reqwest-status rows via an httpmock integration test
  (vault-push already dev-depends on httpmock); `with_retries` tests for
  retry-then-succeed, give-up-on-permanent, and exhaustion.
- **push/pull**: existing test suites unchanged and passing; `run.rs` and
  `project.rs` untouched.
- **demo-seed**: existing tests plus the new validation/partition/mapping
  tests; the server's demo-seed consumers unaffected.
- **Full pass**: `./scripts/check-pr.sh` (format, license, cargo-deny,
  workspace build+test, src-tauri check, web lint/test/audit/build, docs
  check/build) and `committed_cli_pages_match_dump` — with exactly one
  regenerated page, `cli/vault-push.md`.

## Out of scope

- The Tauri host group (7 findings): cancel wiring into push, the
  `env::set_var` SAFETY rewrite, IPC DTO docs, and the src-tauri CI job — next
  cycle.
- Structural work in `vault-push/src/run.rs` (2,880 lines), the
  `tests/push_mock.rs` scaffolding pattern, and the gui/server/tauri
  exporter-wrapper triad — flagged by the audit as unevidenced; not part of
  this group.
- Adding retries to vault-pull; changing journal formats or filenames.

## Behavior deltas (complete catalog)

| # | Change | Why it is acceptable |
|---|---|---|
| 1 | Unknown 4xx statuses (400/405/422…) and 429 are no longer retried | Adjudicated ("typed, all 4xx permanent"); matches the classifier's documented intent; at most one extra attempt today |
| 2 | Substring rewording can no longer change retry behavior | The point of finding 11 |
| 3 | `truncate` no longer panics when `max` splits a multibyte char | Finding 10 bug fix; output otherwise identical |
| 4 | vault-pull journal append/rewrite runs under a write lock | Pull is single-threaded today; no observable change |
| 5 | `--mode` help text rewritten | Finding 7, sanctioned; one CLI page regenerated |
| 6 | demo-seed `about` text rewritten | Finding 8, sanctioned; no committed pages affected |
| 7 | Hand-edited `demo_seed.toml` files with fewer/more than 4 label names now fail to load | New validation; the shipped file is unchanged |

Everything else — journal file formats, filenames, event schemas, retry
counts and backoff, CLI output, and the demo dataset — stays byte-identical.
