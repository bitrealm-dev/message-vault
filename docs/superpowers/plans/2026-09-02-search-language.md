# One Search Language Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the three per-list search parsers and their scattered SQL with one `search` module that parses the redesigned language and compiles it to SQL for the Contacts, Conversations, and Messages lists, then move the web app and the docs onto the new words.

**Architecture:** A new `crates/vault/server/src/search/` module owns the lexer, the parser, a static registry of the twenty-seven words, three per-list "bridges" that say what "this contact", "this conversation", and "this message" mean from each base row, and one emitter per word. `compile` is pure (no database, no clock) and answers with a WHERE fragment plus its parameters; every filter is a correlated subquery, so callers add no joins. The three list functions become callers, `search_query.rs` and `ExportQueryError` are deleted, and a new `GET /v1/search/fields` route serves the word list the web and the docs read from. The web replaces its operator-sniffing regexes with that list and writes the new spellings.

**Tech Stack:** Rust (Axum, sqlx Any over SQLite and Postgres, utoipa, chrono), React 19 + TypeScript, TanStack Query, Vitest, Astro Starlight docs.

**Spec:** `docs/superpowers/specs/2026-09-02-search-language-design.md`. Decision record: `docs/adr/0004-one-search-language-compiled-in-one-module.md`. Read both before Task 1; the word table in the spec is the source of truth for every emitter.

## Global Constraints

- Work on the `worktree-search-language-design` branch. Never commit to `main`. Never create or push tags.
- The language is exactly the spec's word table: twenty-seven words, the value rules, and the per-list applicability. No aliases, no extra words, no memory of earlier spellings anywhere in code, tests, messages, or docs. A word the language does not have is `UnknownWord`, the same as a typo.
- `compile` is pure: it takes `today` as an argument and never opens a connection, reads the clock, or reads the environment. The same request compiles to the same SQL byte for byte.
- Every fragment `compile` returns mentions exactly one base alias, fixed per list: `ct` for `contacts`, `c` for `conversations`, `m` for `messages`. Everything else is a correlated subquery that names its own tables. Callers add no joins and no `DISTINCT`.
- Account scope, `messages.duplicate_of IS NULL`, and the trash default live inside the fragment. Callers never write `account_id = ?` for the base row.
- Placeholders are `?`. Callers run `db::sql::renumber_placeholders` on the finished statement. `params()` are in the textual order of `where_sql()`.
- Query limits stay as today: at most 2048 bytes, 32 free-text terms, 64 expression nodes, nesting depth 32.
- Tests at the module's interface use invented data only (`Ana`, `Bo`, `Cy`, `Jane Doe`, `Sam`, `Family`, `Archive`, `+1555…`, `@example.com`). Never commit real message data.
- OpenAPI-visible changes regenerate `docs/src/assets/openapi.json` in the same commit: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`. The test `committed_openapi_matches_dump` in `crates/vault/server/src/openapi.rs` fails otherwise. After the JSON changes, regenerate `web/src/lib/vaultApi.types.ts` with `cd web && npm run gen:api` and commit it.
- Biome gates `web/`: prefix unused bindings with `_`; prefer a real fix over `biome-ignore`.
- Commit messages are conventional commits whose body says what changed and why in plain language.
- The Rust and TypeScript below were written against the current sources but not compiled before this plan was written. Where wiring details differ, the compiler and the existing tests are authoritative; keep the names and types the Interfaces blocks state.

## File structure

New, all under `crates/vault/server/src/search/`:

| File | One responsibility |
|---|---|
| `mod.rs` | The interface: `ListKind`, `CompileRequest`, `Filter`, `compile`, `describe`. Re-exports `QueryError`, `FieldDoc`. |
| `error.rs` | `QueryError` and `QueryErrorKind`, and its conversion to `ApiError`. |
| `lex.rs` | Query string to tokens with byte spans: words, phrases, `field:value`, parentheses, `or`/`and`/`not`, leading `-`. |
| `value.rs` | Typed values: dates as spans with comparisons and ranges, sizes, counts, ids, keywords. |
| `fields.rs` | The static registry: one `FieldSpec` per word, `lookup`, `describe`. |
| `parse.rs` | Tokens to an expression tree, resolving each `field:` against the registry for the requested list. |
| `bridge.rs` | `Sql` (text plus params) and `ListCtx`, the three per-list wrappers. |
| `emit.rs` | Defaults plus one emitter per word; the free-text emitter per list. |
| `fts.rs` | The full-text leaf for SQLite FTS5 and Postgres tsquery. |
| `tests.rs` | The seeded fixture and the interface tests. |

New elsewhere: `crates/vault/server/src/search_api.rs` (the fields route), `web/src/lib/searchFields.ts` (the hook and the token test).

Modified: `contacts_api.rs`, `conversations_api.rs`, `export_api.rs` (each loses its parser and filter SQL), `lib.rs`, `openapi.rs`, `crates/vault/server/tests/search_parity.rs`, the web files listed in Task 14, the docs search page.

Deleted: `crates/vault/server/src/search_query.rs`, `tests/fixtures/search/parse-cases.json` if nothing else reads it.

---

### Task 1: Module skeleton, error type, and lexer

**Files:**
- Create: `crates/vault/server/src/search/mod.rs`
- Create: `crates/vault/server/src/search/error.rs`
- Create: `crates/vault/server/src/search/lex.rs`
- Modify: `crates/vault/server/src/lib.rs` (add `pub(crate) mod search;` after `pub(crate) mod saved_searches_api;`)

**Interfaces:**
- Consumes: `crate::server::ApiError`.
- Produces:
  - `search::ListKind { Contacts, Conversations, Messages }` with `label(self) -> &'static str` ("Contacts", "Conversations", "Messages") and `base_alias(self) -> &'static str` ("ct", "c", "m"). Derives `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema`, serialized lowercase.
  - `search::error::QueryErrorKind { UnknownWord, WrongList, BadValue, EmptyValue, Unbalanced, TooLong, TooComplex }`.
  - `search::error::QueryError { kind, message: String, span: Range<usize>, field: Option<&'static str>, did_you_mean: Option<&'static str> }` with `QueryError::new(kind, span, message)` and `From<QueryError> for ApiError`.
  - `search::lex::{Token, TokenKind, tokenize}`; `Token { kind: TokenKind, span: Range<usize>, negated: bool }`; `TokenKind::{LParen, RParen, Or, And, Not, Field { word: String, value: String, quoted: bool }, Word { text: String, prefix: bool }, Phrase(String)}`; `tokenize(input: &str) -> Result<Vec<Token>, QueryError>`.
  - Constants in `lex.rs`: `MAX_QUERY_BYTES = 2048`, `MAX_TEXT_TERMS = 32`, `MAX_NODES = 64`, `MAX_DEPTH = 32`.

- [ ] **Step 1: Write the failing lexer tests**

Create `crates/vault/server/src/search/lex.rs` with only the tests at the bottom, so the file exists:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn words_phrases_and_prefixes() {
        assert_eq!(
            kinds(r#"hello "two words" avoc*"#),
            vec![
                TokenKind::Word { text: "hello".into(), prefix: false },
                TokenKind::Phrase("two words".into()),
                TokenKind::Word { text: "avoc".into(), prefix: true },
            ]
        );
    }

    #[test]
    fn fields_take_bare_and_quoted_values() {
        assert_eq!(
            kinds(r#"tag:Work group:"Book Club" date:2019..2021"#),
            vec![
                TokenKind::Field { word: "tag".into(), value: "Work".into(), quoted: false },
                TokenKind::Field { word: "group".into(), value: "Book Club".into(), quoted: true },
                TokenKind::Field { word: "date".into(), value: "2019..2021".into(), quoted: false },
            ]
        );
    }

    #[test]
    fn a_doubled_quote_is_a_literal_quote() {
        assert_eq!(
            kinds(r#"title:"say ""hi"" now""#),
            vec![TokenKind::Field { word: "title".into(), value: r#"say "hi" now"#.into(), quoted: true }]
        );
    }

    #[test]
    fn operators_and_negation() {
        let toks = tokenize("-tag:Work or (a and not b)").unwrap();
        assert!(toks[0].negated);
        assert!(matches!(toks[0].kind, TokenKind::Field { .. }));
        assert_eq!(toks[1].kind, TokenKind::Or);
        assert_eq!(toks[2].kind, TokenKind::LParen);
        assert_eq!(toks[4].kind, TokenKind::And);
        assert_eq!(toks[5].kind, TokenKind::Not);
        assert_eq!(toks[7].kind, TokenKind::RParen);
        // Operator words are case-insensitive.
        assert_eq!(kinds("OR")[0], TokenKind::Or);
    }

    #[test]
    fn spans_are_byte_ranges_into_the_input() {
        let toks = tokenize("hello tag:Work").unwrap();
        assert_eq!(toks[0].span, 0..5);
        assert_eq!(toks[1].span, 6..14);
        // A negated token's span starts at the minus.
        let toks = tokenize("a -b").unwrap();
        assert_eq!(toks[1].span, 2..4);
    }

    #[test]
    fn a_field_word_is_lowercase_letters_and_hyphens() {
        // `Re:` inside a phrase is text, and a token like `http://x` is a word,
        // not a field, because the part before the colon is not a field shape.
        assert_eq!(kinds("http://x")[0], TokenKind::Word { text: "http://x".into(), prefix: false });
        assert_eq!(
            kinds("First-Message:2019")[0],
            TokenKind::Field { word: "first-message".into(), value: "2019".into(), quoted: false }
        );
    }

    #[test]
    fn an_empty_field_value_is_kept_for_the_parser_to_reject() {
        assert_eq!(
            kinds("tag:")[0],
            TokenKind::Field { word: "tag".into(), value: String::new(), quoted: false }
        );
    }

    #[test]
    fn unterminated_quote_is_unbalanced() {
        let err = tokenize(r#"tag:"Book"#).unwrap_err();
        assert_eq!(err.kind, QueryErrorKind::Unbalanced);
        assert_eq!(err.span, 4..9);
    }

    #[test]
    fn too_long_is_refused_before_anything_else() {
        let long = "a".repeat(MAX_QUERY_BYTES + 1);
        assert_eq!(tokenize(&long).unwrap_err().kind, QueryErrorKind::TooLong);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cargo test -p message-vault-server search::lex`
Expected: compile error, `tokenize` and `TokenKind` not found.

- [ ] **Step 3: Write `mod.rs`, `error.rs`, and the lexer**

`crates/vault/server/src/search/mod.rs` (only what Task 1 needs; later tasks add to it):

```rust
//! The search language: one parser and one SQL compiler for the Contacts,
//! Conversations, and Messages lists. See
//! `docs/adr/0004-one-search-language-compiled-in-one-module.md`.

pub mod error;
pub(crate) mod lex;

pub use error::{QueryError, QueryErrorKind};

/// Which list a query is compiled for. Each list accepts its own subset of
/// the words, and every filter is expressed against that list's base row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ListKind {
    /// One row per contact; base alias `ct`.
    Contacts,
    /// One row per conversation; base alias `c`.
    Conversations,
    /// One row per message; base alias `m`.
    Messages,
}

impl ListKind {
    /// The list's name as a person reads it in an error message.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contacts => "Contacts",
            Self::Conversations => "Conversations",
            Self::Messages => "Messages",
        }
    }

    /// The one table alias a compiled fragment may mention.
    pub(crate) fn base_alias(self) -> &'static str {
        match self {
            Self::Contacts => "ct",
            Self::Conversations => "c",
            Self::Messages => "m",
        }
    }
}
```

`crates/vault/server/src/search/error.rs`:

```rust
//! What a query that cannot be compiled answers with.

use std::fmt;
use std::ops::Range;

use crate::server::ApiError;

/// Why a query was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryErrorKind {
    /// A `word:` the language does not have.
    UnknownWord,
    /// A word the language has, but not on the requested list.
    WrongList,
    /// A value the word does not understand.
    BadValue,
    /// A `word:` with nothing after the colon.
    EmptyValue,
    /// Parentheses or quotes that do not close, or an operator with no operand.
    Unbalanced,
    /// The query string is longer than the limit.
    TooLong,
    /// Too many terms or too much nesting.
    TooComplex,
}

/// A refused query: a user-facing sentence and where in the input it points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub kind: QueryErrorKind,
    /// The 400 body. Names the word and the list; never names a spelling the
    /// language does not have.
    pub message: String,
    /// Byte range in the input the message is about.
    pub span: Range<usize>,
    /// The word involved, when there is one.
    pub field: Option<&'static str>,
    /// A word on this list within a small edit distance, when there is one.
    pub did_you_mean: Option<&'static str>,
}

impl QueryError {
    pub(crate) fn new(kind: QueryErrorKind, span: Range<usize>, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), span, field: None, did_you_mean: None }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QueryError {}

impl From<QueryError> for ApiError {
    fn from(e: QueryError) -> Self {
        ApiError::BadRequest(e.message)
    }
}
```

`crates/vault/server/src/search/lex.rs` (above the tests from Step 1):

```rust
//! Query string to tokens. Every token carries the byte range it came from,
//! so an error can point at the exact text.

use std::ops::Range;

use super::error::{QueryError, QueryErrorKind};

/// Reject huge query strings before doing anything else.
pub(crate) const MAX_QUERY_BYTES: usize = 2_048;
/// Free-text terms and phrases allowed in one query.
pub(crate) const MAX_TEXT_TERMS: usize = 32;
/// Expression nodes allowed in one query.
pub(crate) const MAX_NODES: usize = 64;
/// Parenthesis and negation nesting allowed.
pub(crate) const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    LParen,
    RParen,
    Or,
    And,
    Not,
    /// `word:value`. `quoted` says the value came in quotes, so a comma in it
    /// is text rather than a list separator.
    Field { word: String, value: String, quoted: bool },
    /// A bare word; `prefix` when it ended in `*`.
    Word { text: String, prefix: bool },
    /// A quoted phrase.
    Phrase(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
    /// A leading `-` was attached to this token.
    pub negated: bool,
}

/// Read a quoted value starting just after the opening quote. `""` inside is
/// one literal quote. Returns the text and the index just past the closing
/// quote, or `None` when the quote never closes.
fn read_quoted(bytes: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                out.push(b'"');
                i += 2;
                continue;
            }
            return Some((String::from_utf8_lossy(&out).into_owned(), i + 1));
        }
        out.push(bytes[i]);
        i += 1;
    }
    None
}

fn is_field_word(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphabetic() || c == '-')
}

fn is_bare_end(b: u8) -> bool {
    b.is_ascii_whitespace() || b == b'(' || b == b')'
}

/// Tokenize `input`.
///
/// # Errors
///
/// `TooLong` past [`MAX_QUERY_BYTES`]; `Unbalanced` for a quote that never closes.
pub(crate) fn tokenize(input: &str) -> Result<Vec<Token>, QueryError> {
    if input.len() > MAX_QUERY_BYTES {
        return Err(QueryError::new(
            QueryErrorKind::TooLong,
            0..input.len(),
            format!("The search is longer than {MAX_QUERY_BYTES} characters."),
        ));
    }
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        if bytes[i] == b'(' || bytes[i] == b')' {
            let kind = if bytes[i] == b'(' { TokenKind::LParen } else { TokenKind::RParen };
            tokens.push(Token { kind, span: start..i + 1, negated: false });
            i += 1;
            continue;
        }
        let mut negated = false;
        if bytes[i] == b'-' && i + 1 < bytes.len() && !is_bare_end(bytes[i + 1]) {
            negated = true;
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'(' {
            // `-(a or b)`: the minus applies to the group.
            tokens.push(Token { kind: TokenKind::LParen, span: start..i + 1, negated });
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            let (text, next) = read_quoted(bytes, i + 1).ok_or_else(|| {
                QueryError::new(QueryErrorKind::Unbalanced, i..bytes.len(), "A quote never closes.")
            })?;
            tokens.push(Token { kind: TokenKind::Phrase(text), span: start..next, negated });
            i = next;
            continue;
        }
        // A bare run up to whitespace or a parenthesis, watching for `word:`.
        let mut j = i;
        while j < bytes.len() && !is_bare_end(bytes[j]) && bytes[j] != b':' {
            j += 1;
        }
        let head = &input[i..j];
        // `word:` is a field unless the value starts with `/`, so a pasted
        // URL such as `http://x` stays a word.
        let value_starts_with_slash = j + 1 < bytes.len() && bytes[j + 1] == b'/';
        if j < bytes.len() && bytes[j] == b':' && is_field_word(head) && !value_starts_with_slash {
            let word = head.to_ascii_lowercase();
            let mut k = j + 1;
            if k < bytes.len() && bytes[k] == b'"' {
                let (value, next) = read_quoted(bytes, k + 1).ok_or_else(|| {
                    QueryError::new(QueryErrorKind::Unbalanced, k..bytes.len(), "A quote never closes.")
                })?;
                tokens.push(Token {
                    kind: TokenKind::Field { word, value, quoted: true },
                    span: start..next,
                    negated,
                });
                i = next;
                continue;
            }
            while k < bytes.len() && !is_bare_end(bytes[k]) {
                k += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Field { word, value: input[j + 1..k].to_string(), quoted: false },
                span: start..k,
                negated,
            });
            i = k;
            continue;
        }
        // Not a field: take the whole bare run, colons included.
        while j < bytes.len() && !is_bare_end(bytes[j]) {
            j += 1;
        }
        let text = &input[i..j];
        let lower = text.to_ascii_lowercase();
        let kind = match lower.as_str() {
            "or" if !negated => TokenKind::Or,
            "and" if !negated => TokenKind::And,
            "not" if !negated => TokenKind::Not,
            _ => {
                let (text, prefix) = match text.strip_suffix('*') {
                    Some(t) if !t.is_empty() => (t.to_string(), true),
                    _ => (text.to_string(), false),
                };
                TokenKind::Word { text, prefix }
            }
        };
        tokens.push(Token { kind, span: start..j, negated });
        i = j;
    }
    Ok(tokens)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::lex`
Expected: 9 passed. Also run `cargo build -p message-vault-server` to be sure the unused-code warnings are only warnings (the crate has `#![warn(missing_docs)]`, not deny).

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/search crates/vault/server/src/lib.rs
git commit -m "feat(search): add the search module skeleton, error type, and lexer

The lexer turns a query string into tokens with byte spans, so a refusal
can point at the exact text. It knows words, quoted phrases, field:value
with bare or quoted values, parentheses, the operator words, and a
leading minus. Nothing uses it yet."
```

---

### Task 2: Typed values: dates, sizes, counts, ids, keywords

**Files:**
- Create: `crates/vault/server/src/search/value.rs`
- Modify: `crates/vault/server/src/search/mod.rs` (add `pub(crate) mod value;`)

**Interfaces:**
- Consumes: `chrono::NaiveDate`.
- Produces, all `pub(crate)` in `search::value`:
  - `enum Value { Text(String), Prefix(String), Id(i64), Keyword(&'static str), Choice(&'static str), Date(DateCmp), Count(Cmp<i64>), Size(Cmp<i64>) }`
  - `enum Cmp<T> { Eq(T), Gt(T), Gte(T), Lt(T), Lte(T), Range(T, T) }`
  - `struct DateSpan { start: NaiveDate, end: NaiveDate }` where `end` is exclusive.
  - `enum DateCmp { In(DateSpan), Gte(NaiveDate), Gt(NaiveDate), Lt(NaiveDate), Lte(NaiveDate) }` where every variant carries the bound already resolved to a day: `In(span)` means `start <= t < end`; `Gte(d)` means `t >= d`; `Gt(d)` means `t >= d` with `d` the span's end; `Lt(d)` means `t < d`; `Lte(d)` means `t < d` with `d` the span's end.
  - `fn parse_date_span(raw: &str, today: NaiveDate) -> Option<DateSpan>`
  - `fn parse_date(raw: &str, today: NaiveDate) -> Option<DateCmp>`
  - `fn parse_cmp<T: Copy>(raw: &str, scalar: impl Fn(&str) -> Option<T>) -> Option<Cmp<T>>`
  - `fn parse_size_bytes(raw: &str) -> Option<i64>`
  - `fn parse_count(raw: &str) -> Option<i64>`
  - `fn parse_id(raw: &str) -> Option<i64>` (accepts `#12` only)
  - `fn ymd(d: NaiveDate) -> String` (`YYYY-MM-DD`)

- [ ] **Step 1: Write the failing tests**

At the bottom of `crates/vault/server/src/search/value.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    const TODAY: fn() -> NaiveDate = || d(2026, 9, 2);

    #[test]
    fn a_partial_date_names_its_whole_span() {
        assert_eq!(parse_date_span("2019", TODAY()).unwrap(), DateSpan { start: d(2019, 1, 1), end: d(2020, 1, 1) });
        assert_eq!(parse_date_span("2024-02", TODAY()).unwrap(), DateSpan { start: d(2024, 2, 1), end: d(2024, 3, 1) });
        assert_eq!(parse_date_span("2024-02-29", TODAY()).unwrap(), DateSpan { start: d(2024, 2, 29), end: d(2024, 3, 1) });
        assert!(parse_date_span("2019-13", TODAY()).is_none());
        assert!(parse_date_span("2023-02-30", TODAY()).is_none());
    }

    #[test]
    fn relative_spans_end_tomorrow() {
        assert_eq!(parse_date_span("7d", TODAY()).unwrap(), DateSpan { start: d(2026, 8, 26), end: d(2026, 9, 3) });
        assert_eq!(parse_date_span("2w", TODAY()).unwrap().start, d(2026, 8, 19));
        assert_eq!(parse_date_span("3m", TODAY()).unwrap().start, d(2026, 6, 2));
        assert_eq!(parse_date_span("1y", TODAY()).unwrap().start, d(2025, 9, 2));
        assert_eq!(parse_date_span("today", TODAY()).unwrap(), DateSpan { start: d(2026, 9, 2), end: d(2026, 9, 3) });
        assert_eq!(parse_date_span("yesterday", TODAY()).unwrap(), DateSpan { start: d(2026, 9, 1), end: d(2026, 9, 2) });
        // A month shift lands on the last day when the month is shorter.
        assert_eq!(parse_date_span("1m", d(2026, 3, 31)).unwrap().start, d(2026, 2, 28));
        // More than ten years back is refused.
        assert!(parse_date_span("11y", TODAY()).is_none());
    }

    #[test]
    fn comparisons_resolve_against_the_span_edges() {
        assert_eq!(parse_date(">=2019", TODAY()).unwrap(), DateCmp::Gte(d(2019, 1, 1)));
        assert_eq!(parse_date(">2019", TODAY()).unwrap(), DateCmp::Gt(d(2020, 1, 1)));
        assert_eq!(parse_date("<2019", TODAY()).unwrap(), DateCmp::Lt(d(2019, 1, 1)));
        assert_eq!(parse_date("<=2019", TODAY()).unwrap(), DateCmp::Lte(d(2020, 1, 1)));
        assert_eq!(
            parse_date("2019..2021", TODAY()).unwrap(),
            DateCmp::In(DateSpan { start: d(2019, 1, 1), end: d(2022, 1, 1) })
        );
        assert_eq!(parse_date("<1m", TODAY()).unwrap(), DateCmp::Lt(d(2026, 8, 2)));
        assert!(parse_date("2021..2019", TODAY()).is_none());
        assert!(parse_date("", TODAY()).is_none());
    }

    #[test]
    fn sizes_are_1024_based() {
        assert_eq!(parse_size_bytes("500k").unwrap(), 512_000);
        assert_eq!(parse_size_bytes("1M").unwrap(), 1_048_576);
        assert_eq!(parse_size_bytes("2g").unwrap(), 2_147_483_648);
        assert_eq!(parse_size_bytes("12345").unwrap(), 12_345);
        assert_eq!(parse_size_bytes("1.5M").unwrap(), 1_572_864);
        assert!(parse_size_bytes("big").is_none());
    }

    #[test]
    fn comparisons_and_ranges_on_counts() {
        assert_eq!(parse_cmp(">3", parse_count).unwrap(), Cmp::Gt(3));
        assert_eq!(parse_cmp(">=3", parse_count).unwrap(), Cmp::Gte(3));
        assert_eq!(parse_cmp("<10", parse_count).unwrap(), Cmp::Lt(10));
        assert_eq!(parse_cmp("<=10", parse_count).unwrap(), Cmp::Lte(10));
        assert_eq!(parse_cmp("0", parse_count).unwrap(), Cmp::Eq(0));
        assert_eq!(parse_cmp("1..10", parse_count).unwrap(), Cmp::Range(1, 10));
        assert!(parse_cmp("10..1", parse_count).is_none());
        assert!(parse_cmp("many", parse_count).is_none());
    }

    #[test]
    fn ids_need_the_hash() {
        assert_eq!(parse_id("#12").unwrap(), 12);
        assert!(parse_id("12").is_none());
        assert!(parse_id("#").is_none());
        assert!(parse_id("#-1").is_none());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::value`
Expected: compile error, the functions do not exist.

- [ ] **Step 3: Write the value parsers**

Above the tests in `value.rs`:

```rust
//! Typed values a word can take. Dates are spans, so `date:2019` is the
//! year and `date:>2019` is after it ends; `today` is an input, never the clock.

use chrono::{Datelike, Days, NaiveDate};
// `Days` needs chrono 0.4.23 or later; if the workspace pins an older
// chrono, use `chrono::Duration::days(n as i64)` with `checked_sub_signed`.

/// Relative spans further back than this are refused.
const MAX_LOOKBACK_DAYS: u64 = 3_650;

/// A comparison or range on an ordered scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cmp<T> {
    Eq(T),
    Gt(T),
    Gte(T),
    Lt(T),
    Lte(T),
    /// Inclusive on both ends.
    Range(T, T),
}

/// The days a date value names: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DateSpan {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// A date filter with its bounds already resolved to calendar days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DateCmp {
    /// Inside the span.
    In(DateSpan),
    /// On or after this day.
    Gte(NaiveDate),
    /// On or after this day (the span's end, so "after the span").
    Gt(NaiveDate),
    /// Before this day.
    Lt(NaiveDate),
    /// Before this day (the span's end, so "up to the span's last day").
    Lte(NaiveDate),
}

/// One parsed value, typed by the word it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    /// `pre*` on a text word.
    Prefix(String),
    /// `#12`.
    Id(i64),
    /// A universal keyword: `none`, `any`, `me`, `unknown`, `last`.
    Keyword(&'static str),
    /// One of a word's fixed choices, already lower-cased and checked.
    Choice(&'static str),
    Date(DateCmp),
    Count(Cmp<i64>),
    Size(Cmp<i64>),
}

/// `YYYY-MM-DD`, the form timestamps are compared against as text.
pub(crate) fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn first_of_next_month(y: i32, m: u32) -> Option<NaiveDate> {
    if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) }
}

fn shift_months_back(today: NaiveDate, months: u32) -> Option<NaiveDate> {
    let total = i64::from(today.year()) * 12 + i64::from(today.month()) - 1 - i64::from(months);
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = u32::try_from(total.rem_euclid(12) + 1).ok()?;
    let last = first_of_next_month(year, month)?.pred_opt()?.day();
    NaiveDate::from_ymd_opt(year, month, today.day().min(last))
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// A bare date value as a span. Accepts `YYYY`, `YYYY-MM`, `YYYY-MM-DD`,
/// `today`, `yesterday`, and `Nd`/`Nw`/`Nm`/`Ny` (the last N units ending today).
pub(crate) fn parse_date_span(raw: &str, today: NaiveDate) -> Option<DateSpan> {
    let t = raw.trim().to_ascii_lowercase();
    let tomorrow = today.checked_add_days(Days::new(1))?;
    match t.as_str() {
        "" => return None,
        "today" => return Some(DateSpan { start: today, end: tomorrow }),
        "yesterday" => return Some(DateSpan { start: today.checked_sub_days(Days::new(1))?, end: today }),
        _ => {}
    }
    if let Some(unit) = t.chars().last()
        && matches!(unit, 'd' | 'w' | 'm' | 'y')
        && all_digits(&t[..t.len() - 1])
    {
        let n: u32 = t[..t.len() - 1].parse().ok()?;
        let days = match unit {
            'd' => u64::from(n),
            'w' => u64::from(n) * 7,
            'm' => u64::from(n) * 31,
            _ => u64::from(n) * 365,
        };
        if days > MAX_LOOKBACK_DAYS {
            return None;
        }
        let start = match unit {
            'd' => today.checked_sub_days(Days::new(u64::from(n)))?,
            'w' => today.checked_sub_days(Days::new(u64::from(n) * 7))?,
            'm' => shift_months_back(today, n)?,
            _ => shift_months_back(today, n * 12)?,
        };
        return Some(DateSpan { start, end: tomorrow });
    }
    let parts: Vec<&str> = t.split('-').collect();
    match parts.as_slice() {
        [y] if y.len() == 4 && all_digits(y) => {
            let y: i32 = y.parse().ok()?;
            Some(DateSpan { start: NaiveDate::from_ymd_opt(y, 1, 1)?, end: NaiveDate::from_ymd_opt(y + 1, 1, 1)? })
        }
        [y, m] if y.len() == 4 && all_digits(y) && m.len() == 2 && all_digits(m) => {
            let (y, m): (i32, u32) = (y.parse().ok()?, m.parse().ok()?);
            Some(DateSpan { start: NaiveDate::from_ymd_opt(y, m, 1)?, end: first_of_next_month(y, m)? })
        }
        [y, m, d] if y.len() == 4 && all_digits(y) && m.len() == 2 && all_digits(m) && d.len() == 2 && all_digits(d) => {
            let start = NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?;
            Some(DateSpan { start, end: start.checked_add_days(Days::new(1))? })
        }
        _ => None,
    }
}

/// A date value with its optional comparison or range.
pub(crate) fn parse_date(raw: &str, today: NaiveDate) -> Option<DateCmp> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if let Some((a, b)) = t.split_once("..") {
        let (a, b) = (parse_date_span(a, today)?, parse_date_span(b, today)?);
        if b.end <= a.start {
            return None;
        }
        return Some(DateCmp::In(DateSpan { start: a.start, end: b.end }));
    }
    if let Some(rest) = t.strip_prefix(">=") {
        return Some(DateCmp::Gte(parse_date_span(rest, today)?.start));
    }
    if let Some(rest) = t.strip_prefix("<=") {
        return Some(DateCmp::Lte(parse_date_span(rest, today)?.end));
    }
    if let Some(rest) = t.strip_prefix('>') {
        return Some(DateCmp::Gt(parse_date_span(rest, today)?.end));
    }
    if let Some(rest) = t.strip_prefix('<') {
        return Some(DateCmp::Lt(parse_date_span(rest, today)?.start));
    }
    Some(DateCmp::In(parse_date_span(t, today)?))
}

/// `>3`, `>=3`, `<10`, `<=10`, `1..10`, or a bare scalar meaning equals.
pub(crate) fn parse_cmp<T: Copy + PartialOrd>(
    raw: &str,
    scalar: impl Fn(&str) -> Option<T>,
) -> Option<Cmp<T>> {
    let t = raw.trim();
    if let Some((a, b)) = t.split_once("..") {
        let (a, b) = (scalar(a)?, scalar(b)?);
        return if a <= b { Some(Cmp::Range(a, b)) } else { None };
    }
    if let Some(rest) = t.strip_prefix(">=") {
        return Some(Cmp::Gte(scalar(rest)?));
    }
    if let Some(rest) = t.strip_prefix("<=") {
        return Some(Cmp::Lte(scalar(rest)?));
    }
    if let Some(rest) = t.strip_prefix('>') {
        return Some(Cmp::Gt(scalar(rest)?));
    }
    if let Some(rest) = t.strip_prefix('<') {
        return Some(Cmp::Lt(scalar(rest)?));
    }
    Some(Cmp::Eq(scalar(t.strip_prefix('=').unwrap_or(t))?))
}

/// `500k`, `1M`, `2G` (1024-based, case-insensitive), or bare bytes.
pub(crate) fn parse_size_bytes(raw: &str) -> Option<i64> {
    let t = raw.trim().to_ascii_lowercase();
    let end = t.bytes().position(|b| !(b.is_ascii_digit() || b == b'.')).unwrap_or(t.len());
    if end == 0 {
        return None;
    }
    let n: f64 = t[..end].parse().ok()?;
    let mult = match t[end..].trim().trim_end_matches('b') {
        "" => 1.0,
        "k" => 1024.0,
        "m" => 1024.0_f64.powi(2),
        "g" => 1024.0_f64.powi(3),
        _ => return None,
    };
    let bytes = (n * mult).round();
    if !bytes.is_finite() || bytes < 0.0 || bytes > i64::MAX as f64 {
        return None;
    }
    Some(bytes as i64)
}

/// A non-negative integer.
pub(crate) fn parse_count(raw: &str) -> Option<i64> {
    let t = raw.trim();
    if !all_digits(t) {
        return None;
    }
    t.parse().ok()
}

/// `#12`: the row with that id.
pub(crate) fn parse_id(raw: &str) -> Option<i64> {
    let digits = raw.trim().strip_prefix('#')?;
    if !all_digits(digits) {
        return None;
    }
    digits.parse().ok()
}
```

Add `pub(crate) mod value;` to `mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::value`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "feat(search): parse dates as spans, plus sizes, counts, and ids

A date value names the whole span it spells out, so date:2019 is the
year, date:>2019 is after it ends, and date:7d is the last week. Sizes
are 1024-based and counts take the same comparisons and ranges. Today's
date is an argument, so the parser never reads the clock."
```

---

### Task 3: The registry and the parser

**Files:**
- Create: `crates/vault/server/src/search/fields.rs`
- Create: `crates/vault/server/src/search/parse.rs`
- Modify: `crates/vault/server/src/search/mod.rs` (add the two modules; re-export `FieldDoc`, `ValueType`; add `pub fn describe`)
- Modify: `crates/vault/server/src/search/lex.rs` (derive `Clone` on `Token` if Task 1 did not)

**Interfaces:**
- Consumes: `lex::{Token, TokenKind, MAX_*}`, `value::*`, `ListKind`, `QueryError`.
- Produces:
  - `fields::ValueType { Text, Name, Person, Choice, Date, Count, Size, Flag }` (pub, `Serialize`, `ToSchema`, lowercase).
  - `fields::FieldSpec { word: &'static str, value_type: ValueType, lists: &'static [ListKind], values: &'static [&'static str], help: &'static str, example: &'static str }` (pub(crate)).
  - `fields::FIELDS: &[FieldSpec]` (pub(crate)), twenty-seven entries in the spec's order.
  - `fields::lookup(word: &str) -> Option<&'static FieldSpec>`, `fields::for_list(list) -> impl Iterator<Item = &'static FieldSpec>`, `fields::nearest(word: &str, list: ListKind) -> Option<&'static str>` (edit distance at most 2, ties to the first in registry order).
  - `fields::FieldDoc { word: String, value_type: ValueType, values: Vec<String>, help: String, example: String, lists: Vec<ListKind> }` (pub, `Serialize`, `ToSchema`) and `search::describe(list) -> Vec<FieldDoc>`.
  - `parse::Expr { And(Vec<Expr>), Or(Vec<Expr>), Not(Box<Expr>), Field(FieldTerm), Text(TextTerm) }` with `Expr::uses(&self, word) -> bool`; `parse::FieldTerm { spec: &'static FieldSpec, values: Vec<Value>, span: Range<usize> }`; `parse::TextTerm { Term { text: String, prefix: bool }, Phrase(String) }`; `parse::parse(list, tokens: &[Token], today) -> Result<Option<Expr>, QueryError>` (`None` for an empty query).

- [ ] **Step 1: Write the failing parser tests**

At the bottom of `crates/vault/server/src/search/parse.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::lex::tokenize;
    use crate::search::value::{Cmp, Value};
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()
    }

    fn parse_ok(list: ListKind, q: &str) -> Expr {
        parse(list, &tokenize(q).unwrap(), today()).unwrap().unwrap()
    }

    fn parse_err(list: ListKind, q: &str) -> QueryError {
        parse(list, &tokenize(q).unwrap(), today()).unwrap_err()
    }

    #[test]
    fn space_is_and_and_or_binds_loosest() {
        let e = parse_ok(ListKind::Messages, "a b or c");
        let Expr::Or(parts) = e else { panic!("expected or") };
        assert_eq!(parts.len(), 2);
        assert!(matches!(parts[0], Expr::And(_)));
        assert!(matches!(parts[1], Expr::Text(_)));
    }

    #[test]
    fn minus_and_not_negate_and_parentheses_group() {
        let e = parse_ok(ListKind::Conversations, "-tag:Work not (a or b)");
        let Expr::And(parts) = e else { panic!("expected and") };
        assert!(matches!(parts[0], Expr::Not(_)));
        assert!(matches!(parts[1], Expr::Not(_)));
    }

    #[test]
    fn a_field_resolves_against_the_registry_for_its_list() {
        let e = parse_ok(ListKind::Contacts, "groups:>5");
        let Expr::Field(term) = e else { panic!("expected field") };
        assert_eq!(term.spec.word, "groups");
        assert_eq!(term.values, vec![Value::Count(Cmp::Gt(5))]);
    }

    #[test]
    fn commas_mean_or_inside_a_field_unless_quoted() {
        let e = parse_ok(ListKind::Messages, "service:imessage,sms");
        let Expr::Field(term) = e else { panic!("expected field") };
        assert_eq!(term.values, vec![Value::Choice("imessage"), Value::Choice("sms")]);
        let e = parse_ok(ListKind::Contacts, r#"group:"A, B""#);
        let Expr::Field(term) = e else { panic!("expected field") };
        assert_eq!(term.values, vec![Value::Text("A, B".into())]);
    }

    #[test]
    fn keywords_and_ids_are_typed() {
        let e = parse_ok(ListKind::Contacts, "group:none");
        let Expr::Field(term) = e else { panic!() };
        assert_eq!(term.values, vec![Value::Keyword("none")]);
        let e = parse_ok(ListKind::Messages, "with:#42");
        let Expr::Field(term) = e else { panic!() };
        assert_eq!(term.values, vec![Value::Id(42)]);
        let e = parse_ok(ListKind::Messages, "from:me");
        let Expr::Field(term) = e else { panic!() };
        assert_eq!(term.values, vec![Value::Keyword("me")]);
        let e = parse_ok(ListKind::Messages, "filename:IMG_*");
        let Expr::Field(term) = e else { panic!() };
        assert_eq!(term.values, vec![Value::Prefix("IMG_".into())]);
    }

    #[test]
    fn unknown_word_is_refused_without_naming_anything_else() {
        let err = parse_err(ListKind::Messages, "people:Family");
        assert_eq!(err.kind, QueryErrorKind::UnknownWord);
        assert_eq!(err.message, "people: is not a search word.");
        assert_eq!(err.span, 0..13);
        assert_eq!(err.did_you_mean, None);
    }

    #[test]
    fn a_near_miss_gets_a_suggestion() {
        let err = parse_err(ListKind::Conversations, "paticipants:>2");
        assert_eq!(err.kind, QueryErrorKind::UnknownWord);
        assert_eq!(err.did_you_mean, Some("participants"));
        assert_eq!(err.message, "paticipants: is not a search word. Did you mean participants:?");
    }

    #[test]
    fn a_word_from_another_list_says_where_it_works() {
        let err = parse_err(ListKind::Contacts, "from:me");
        assert_eq!(err.kind, QueryErrorKind::WrongList);
        assert_eq!(err.field, Some("from"));
        assert_eq!(err.message, "from: is not a Contacts word. It works on Messages.");
        assert_eq!(err.span, 0..7);
        assert_eq!(err.did_you_mean, None);
    }

    #[test]
    fn empty_and_bad_values() {
        let err = parse_err(ListKind::Messages, "tag:");
        assert_eq!(err.kind, QueryErrorKind::EmptyValue);
        assert_eq!(err.message, "tag: needs a value, for example tag:Holiday or tag:none.");
        let err = parse_err(ListKind::Messages, "date:2019-13");
        assert_eq!(err.kind, QueryErrorKind::BadValue);
        assert_eq!(
            err.message,
            "date: does not understand 2019-13. Write a year, a month like 2024-05, a day, or a span like 7d, with >, >=, <, <=, or a..b."
        );
        let err = parse_err(ListKind::Messages, "kind:big");
        assert_eq!(err.message, "kind: does not understand big. Write one of: direct, group.");
    }

    #[test]
    fn unbalanced_shapes() {
        assert_eq!(parse_err(ListKind::Messages, "(a or b").kind, QueryErrorKind::Unbalanced);
        assert_eq!(parse_err(ListKind::Messages, "a or").kind, QueryErrorKind::Unbalanced);
        assert_eq!(parse_err(ListKind::Messages, "a)").kind, QueryErrorKind::Unbalanced);
    }

    #[test]
    fn empty_query_is_none() {
        assert!(parse(ListKind::Messages, &tokenize("   ").unwrap(), today()).unwrap().is_none());
    }

    #[test]
    fn describe_lists_only_that_lists_words() {
        let docs = crate::search::describe(ListKind::Contacts);
        assert!(docs.iter().any(|d| d.word == "groups"));
        assert!(!docs.iter().any(|d| d.word == "from"));
        assert_eq!(crate::search::describe(ListKind::Messages).iter().filter(|d| d.word == "date").count(), 1);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::parse`
Expected: compile error.

- [ ] **Step 3: Write the registry**

`crates/vault/server/src/search/fields.rs`:

```rust
//! The registry: every word the language has, once. `describe` and the
//! parser read this table, so a word cannot exist for one and not the other.

use super::ListKind;
use super::ListKind::{Contacts as C, Conversations as V, Messages as M};

/// What shape of value a word takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    /// Free text, matched as "contains"; `pre*` matches a prefix.
    Text,
    /// A named thing: a name, or `#id`.
    Name,
    /// A person: a name, a handle, `#id`, or `me` where allowed.
    Person,
    /// One of a fixed set of words.
    Choice,
    /// A day, month, year, or relative span, with comparisons and ranges.
    Date,
    /// A whole number, with comparisons and ranges.
    Count,
    /// A byte size, with comparisons and ranges.
    Size,
    /// `yes`, `no`, or `any`.
    Flag,
}

/// One word.
#[derive(Debug)]
pub(crate) struct FieldSpec {
    pub word: &'static str,
    pub value_type: ValueType,
    pub lists: &'static [ListKind],
    /// Keyword values this word accepts besides its value type: `none`,
    /// `any`, `me`, `unknown`, `last`, or the fixed choices of a Choice/Flag.
    pub values: &'static [&'static str],
    pub help: &'static str,
    pub example: &'static str,
}

const NONE_ANY: &[&str] = &["none", "any"];

/// The twenty-seven words, in the spec's order.
pub(crate) static FIELDS: &[FieldSpec] = &[
    FieldSpec { word: "body", value_type: ValueType::Text, lists: &[V, M], values: NONE_ANY, help: "message body only", example: "body:avocado" },
    FieldSpec { word: "subject", value_type: ValueType::Text, lists: &[V, M], values: NONE_ANY, help: "subject line only", example: "subject:dinner" },
    FieldSpec { word: "name", value_type: ValueType::Text, lists: &[C, V, M], values: NONE_ANY, help: "a person's name: this contact, or a participant", example: "name:jane" },
    FieldSpec { word: "title", value_type: ValueType::Text, lists: &[V, M], values: NONE_ANY, help: "the conversation's title", example: "title:\"book club\"" },
    FieldSpec { word: "handle", value_type: ValueType::Text, lists: &[C, V, M], values: NONE_ANY, help: "a phone number, email, or username", example: "handle:@gmail.com" },
    FieldSpec { word: "with", value_type: ValueType::Person, lists: &[V, M], values: &[], help: "this person is a participant", example: "with:jane" },
    FieldSpec { word: "from", value_type: ValueType::Person, lists: &[M], values: &["me"], help: "this person sent it", example: "from:me" },
    FieldSpec { word: "to", value_type: ValueType::Person, lists: &[M], values: &["me"], help: "it was sent to this person", example: "to:jane" },
    FieldSpec { word: "in", value_type: ValueType::Name, lists: &[M], values: &[], help: "this one conversation", example: "in:#19" },
    FieldSpec { word: "group", value_type: ValueType::Name, lists: &[C, V, M], values: &["none", "unknown"], help: "in this Contact Group: the contact itself, or a participant", example: "group:Family" },
    FieldSpec { word: "tag", value_type: ValueType::Name, lists: &[C, V, M], values: &["none"], help: "the conversation carries this Message Tag", example: "tag:Holiday" },
    FieldSpec { word: "kind", value_type: ValueType::Choice, lists: &[C, V, M], values: &["direct", "group"], help: "the conversation's shape", example: "kind:direct" },
    FieldSpec { word: "service", value_type: ValueType::Choice, lists: &[C, V, M], values: &["imessage", "sms", "mms", "rcs", "whatsapp"], help: "the transport that carried the message", example: "service:imessage" },
    FieldSpec { word: "source", value_type: ValueType::Choice, lists: &[V, M], values: &["imessage", "whatsapp", "sms"], help: "the backup family it was imported from", example: "source:whatsapp" },
    FieldSpec { word: "import", value_type: ValueType::Name, lists: &[V, M], values: &["last"], help: "brought in by this Import Run", example: "import:last" },
    FieldSpec { word: "date", value_type: ValueType::Date, lists: &[C, V, M], values: &[], help: "when a message was sent; on Contacts and Conversations, has a message then", example: "date:2019..2021" },
    FieldSpec { word: "first-message", value_type: ValueType::Date, lists: &[C, V, M], values: &[], help: "the date of the earliest message", example: "first-message:<2020" },
    FieldSpec { word: "last-message", value_type: ValueType::Date, lists: &[C, V, M], values: &[], help: "the date of the latest message", example: "last-message:<2022" },
    FieldSpec { word: "attachment", value_type: ValueType::Choice, lists: &[V, M], values: &["image", "video", "audio", "document", "pdf", "contact", "other", "any", "none"], help: "what is attached", example: "attachment:image" },
    FieldSpec { word: "filename", value_type: ValueType::Text, lists: &[V, M], values: &[], help: "an attachment's file name", example: "filename:IMG_*" },
    FieldSpec { word: "size", value_type: ValueType::Size, lists: &[V, M], values: &[], help: "an attachment's size", example: "size:>1M" },
    FieldSpec { word: "messages", value_type: ValueType::Count, lists: &[C, V], values: &[], help: "how many messages", example: "messages:>100" },
    FieldSpec { word: "conversations", value_type: ValueType::Count, lists: &[C], values: &[], help: "how many conversations", example: "conversations:0" },
    FieldSpec { word: "groups", value_type: ValueType::Count, lists: &[C], values: &[], help: "how many Contact Groups", example: "groups:>5" },
    FieldSpec { word: "participants", value_type: ValueType::Count, lists: &[V, M], values: &[], help: "how many people in the conversation", example: "participants:>2" },
    FieldSpec { word: "attachments", value_type: ValueType::Count, lists: &[M], values: &[], help: "how many attachments on the message", example: "attachments:>0" },
    FieldSpec { word: "trashed", value_type: ValueType::Flag, lists: &[C, V], values: &["yes", "no", "any"], help: "in the trash", example: "trashed:yes" },
];

/// The spec for a word, on any list.
pub(crate) fn lookup(word: &str) -> Option<&'static FieldSpec> {
    FIELDS.iter().find(|f| f.word == word)
}

/// The words one list accepts, in registry order.
pub(crate) fn for_list(list: ListKind) -> impl Iterator<Item = &'static FieldSpec> {
    FIELDS.iter().filter(move |f| f.lists.contains(&list))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// A word on `list` within two edits of `word`, for "did you mean".
pub(crate) fn nearest(word: &str, list: ListKind) -> Option<&'static str> {
    for_list(list)
        .map(|f| (edit_distance(word, f.word), f.word))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, w)| w)
}

/// One word as the web and the docs see it.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct FieldDoc {
    /// The spelling, without the colon.
    pub word: String,
    /// What shape of value it takes.
    pub value_type: ValueType,
    /// Keyword or fixed values the word accepts.
    pub values: Vec<String>,
    /// One line of help.
    pub help: String,
    /// One example, ready to type.
    pub example: String,
    /// Every list the word works on.
    pub lists: Vec<ListKind>,
}

/// The words for one list.
pub fn describe(list: ListKind) -> Vec<FieldDoc> {
    for_list(list)
        .map(|f| FieldDoc {
            word: f.word.to_string(),
            value_type: f.value_type,
            values: f.values.iter().map(|v| v.to_string()).collect(),
            help: f.help.to_string(),
            example: f.example.to_string(),
            lists: f.lists.to_vec(),
        })
        .collect()
}
```

- [ ] **Step 4: Write the parser**

`crates/vault/server/src/search/parse.rs` above the tests:

```rust
//! Tokens to an expression tree. Every `word:` is resolved against the
//! registry for the requested list here, so the emitters never see a word
//! they do not own.

use std::ops::Range;

use chrono::NaiveDate;

use super::ListKind;
use super::error::{QueryError, QueryErrorKind};
use super::fields::{self, FieldSpec, ValueType};
use super::lex::{MAX_DEPTH, MAX_NODES, MAX_TEXT_TERMS, Token, TokenKind};
use super::value::{self, Value};

/// A free-text leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextTerm {
    Term { text: String, prefix: bool },
    Phrase(String),
}

/// One `word:values`.
#[derive(Debug)]
pub(crate) struct FieldTerm {
    pub spec: &'static FieldSpec,
    /// Comma-separated values, meaning OR.
    pub values: Vec<Value>,
    pub span: Range<usize>,
}

/// The parsed query.
#[derive(Debug)]
pub(crate) enum Expr {
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Field(FieldTerm),
    Text(TextTerm),
}

impl Expr {
    /// True when any field term in the tree is `word`.
    pub(crate) fn uses(&self, word: &str) -> bool {
        match self {
            Self::And(v) | Self::Or(v) => v.iter().any(|e| e.uses(word)),
            Self::Not(e) => e.uses(word),
            Self::Field(t) => t.spec.word == word,
            Self::Text(_) => false,
        }
    }

    fn nodes(&self) -> usize {
        match self {
            Self::And(v) | Self::Or(v) => 1 + v.iter().map(Expr::nodes).sum::<usize>(),
            Self::Not(e) => 1 + e.nodes(),
            Self::Field(_) | Self::Text(_) => 1,
        }
    }

    fn text_terms(&self) -> usize {
        match self {
            Self::And(v) | Self::Or(v) => v.iter().map(Expr::text_terms).sum(),
            Self::Not(e) => e.text_terms(),
            Self::Field(_) => 0,
            Self::Text(_) => 1,
        }
    }
}

fn value_hint(spec: &FieldSpec) -> String {
    let choices = || spec.values.join(", ");
    match spec.value_type {
        ValueType::Date => "Write a year, a month like 2024-05, a day, or a span like 7d, with >, >=, <, <=, or a..b.".into(),
        ValueType::Count => "Write a number, with >, >=, <, <=, or a..b.".into(),
        ValueType::Size => "Write a size like 500k, 1M, or 2G, with >, >=, <, <=, or a..b.".into(),
        ValueType::Choice | ValueType::Flag => format!("Write one of: {}.", choices()),
        ValueType::Name if spec.values.is_empty() => "Write a name or #id.".into(),
        ValueType::Name => format!("Write a name, #id, or one of: {}.", choices()),
        ValueType::Person if spec.values.is_empty() => "Write a name, a handle, or #id.".into(),
        ValueType::Person => format!("Write a name, a handle, #id, or one of: {}.", choices()),
        ValueType::Text if spec.values.is_empty() => "Write some text, or pre* for a prefix.".into(),
        ValueType::Text => format!("Write some text, pre* for a prefix, or one of: {}.", choices()),
    }
}

fn parse_one_value(spec: &FieldSpec, raw: &str, quoted: bool, today: NaiveDate) -> Option<Value> {
    let lower = raw.trim().to_ascii_lowercase();
    if let Some(kw) = spec.values.iter().find(|v| **v == lower) {
        return Some(match spec.value_type {
            ValueType::Choice | ValueType::Flag => Value::Choice(kw),
            _ => Value::Keyword(kw),
        });
    }
    match spec.value_type {
        ValueType::Choice | ValueType::Flag => None,
        ValueType::Text => {
            if !quoted && let Some(p) = raw.strip_suffix('*') && !p.is_empty() {
                Some(Value::Prefix(p.to_string()))
            } else {
                Some(Value::Text(raw.trim().to_string()))
            }
        }
        ValueType::Name | ValueType::Person => {
            if !quoted && let Some(id) = value::parse_id(raw) {
                Some(Value::Id(id))
            } else {
                Some(Value::Text(raw.trim().to_string()))
            }
        }
        ValueType::Date => value::parse_date(raw, today).map(Value::Date),
        ValueType::Count => value::parse_cmp(raw, value::parse_count).map(Value::Count),
        ValueType::Size => value::parse_cmp(raw, value::parse_size_bytes).map(Value::Size),
    }
}

fn field_term(
    list: ListKind,
    word: &str,
    raw: &str,
    quoted: bool,
    span: Range<usize>,
    today: NaiveDate,
) -> Result<FieldTerm, QueryError> {
    let Some(spec) = fields::lookup(word) else {
        let mut err = QueryError::new(QueryErrorKind::UnknownWord, span, format!("{word}: is not a search word."));
        if let Some(near) = fields::nearest(word, list) {
            err.message.push_str(&format!(" Did you mean {near}:?"));
            err.did_you_mean = Some(near);
        }
        return Err(err);
    };
    if !spec.lists.contains(&list) {
        let works_on: Vec<&str> = spec.lists.iter().map(|l| l.label()).collect();
        let mut err = QueryError::new(
            QueryErrorKind::WrongList,
            span,
            format!("{word}: is not a {} word. It works on {}.", list.label(), works_on.join(" and ")),
        );
        err.field = Some(spec.word);
        if let Some(near) = fields::nearest(word, list).filter(|n| *n != spec.word) {
            err.message.push_str(&format!(" Did you mean {near}:?"));
            err.did_you_mean = Some(near);
        }
        return Err(err);
    }
    if raw.trim().is_empty() {
        let also = if spec.values.is_empty() { String::new() } else { format!(" or {word}:{}", spec.values[0]) };
        let mut err = QueryError::new(
            QueryErrorKind::EmptyValue,
            span,
            format!("{word}: needs a value, for example {}{also}.", spec.example),
        );
        err.field = Some(spec.word);
        return Err(err);
    }
    let pieces: Vec<&str> = if quoted {
        vec![raw]
    } else {
        raw.split(',').filter(|p| !p.trim().is_empty()).collect()
    };
    let mut values = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let Some(v) = parse_one_value(spec, piece, quoted, today) else {
            let mut err = QueryError::new(
                QueryErrorKind::BadValue,
                span.clone(),
                format!("{word}: does not understand {}. {}", piece.trim(), value_hint(spec)),
            );
            err.field = Some(spec.word);
            return Err(err);
        };
        values.push(v);
    }
    Ok(FieldTerm { spec, values, span })
}

struct Parser<'t> {
    list: ListKind,
    tokens: &'t [Token],
    i: usize,
    today: NaiveDate,
    end: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.i)
    }

    fn unbalanced(&self, span: Range<usize>, msg: &str) -> QueryError {
        QueryError::new(QueryErrorKind::Unbalanced, span, msg)
    }

    fn at_operand_end(&self) -> bool {
        matches!(
            self.peek().map(|t| &t.kind),
            None | Some(TokenKind::RParen) | Some(TokenKind::Or) | Some(TokenKind::And)
        )
    }

    fn parse_or(&mut self, depth: usize) -> Result<Expr, QueryError> {
        let mut parts = vec![self.parse_and(depth)?];
        while matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Or)) {
            let or_span = self.peek().unwrap().span.clone();
            self.i += 1;
            if self.at_operand_end() {
                return Err(self.unbalanced(or_span, "or needs something on both sides."));
            }
            parts.push(self.parse_and(depth)?);
        }
        Ok(if parts.len() == 1 { parts.remove(0) } else { Expr::Or(parts) })
    }

    fn parse_and(&mut self, depth: usize) -> Result<Expr, QueryError> {
        let mut parts = vec![self.parse_unary(depth)?];
        loop {
            match self.peek().map(|t| &t.kind) {
                None | Some(TokenKind::Or) | Some(TokenKind::RParen) => break,
                Some(TokenKind::And) => {
                    let and_span = self.peek().unwrap().span.clone();
                    self.i += 1;
                    if self.at_operand_end() {
                        return Err(self.unbalanced(and_span, "and needs something on both sides."));
                    }
                    parts.push(self.parse_unary(depth)?);
                }
                Some(_) => parts.push(self.parse_unary(depth)?),
            }
        }
        Ok(if parts.len() == 1 { parts.remove(0) } else { Expr::And(parts) })
    }

    fn parse_unary(&mut self, depth: usize) -> Result<Expr, QueryError> {
        if depth > MAX_DEPTH {
            return Err(QueryError::new(QueryErrorKind::TooComplex, 0..self.end, "The search nests too deeply."));
        }
        let Some(tok) = self.peek() else {
            return Err(self.unbalanced(self.end..self.end, "The search ends where a word was expected."));
        };
        if matches!(tok.kind, TokenKind::Not) {
            let span = tok.span.clone();
            self.i += 1;
            if self.at_operand_end() {
                return Err(self.unbalanced(span, "not needs something after it."));
            }
            return Ok(Expr::Not(Box::new(self.parse_unary(depth + 1)?)));
        }
        let negated = tok.negated;
        let inner = self.parse_primary(depth)?;
        Ok(if negated { Expr::Not(Box::new(inner)) } else { inner })
    }

    fn parse_primary(&mut self, depth: usize) -> Result<Expr, QueryError> {
        let tok = self.peek().expect("parse_unary checked for a token").clone();
        match tok.kind {
            TokenKind::LParen => {
                self.i += 1;
                if matches!(self.peek().map(|t| &t.kind), None | Some(TokenKind::RParen)) {
                    return Err(self.unbalanced(tok.span, "A parenthesis has nothing inside it."));
                }
                let inner = self.parse_or(depth + 1)?;
                match self.peek().map(|t| &t.kind) {
                    Some(TokenKind::RParen) => {
                        self.i += 1;
                        Ok(inner)
                    }
                    _ => Err(self.unbalanced(tok.span, "A parenthesis never closes.")),
                }
            }
            TokenKind::RParen => Err(self.unbalanced(tok.span, "A closing parenthesis has no opening one.")),
            TokenKind::Or | TokenKind::And => Err(self.unbalanced(tok.span, "or and and need something on both sides.")),
            TokenKind::Not => Err(self.unbalanced(tok.span, "not needs something after it.")),
            TokenKind::Field { word, value, quoted } => {
                self.i += 1;
                Ok(Expr::Field(field_term(self.list, &word, &value, quoted, tok.span, self.today)?))
            }
            TokenKind::Word { text, prefix } => {
                self.i += 1;
                Ok(Expr::Text(TextTerm::Term { text, prefix }))
            }
            TokenKind::Phrase(text) => {
                self.i += 1;
                Ok(Expr::Text(TextTerm::Phrase(text)))
            }
        }
    }
}

/// Parse tokens for `list`. `Ok(None)` is an empty query.
pub(crate) fn parse(list: ListKind, tokens: &[Token], today: NaiveDate) -> Result<Option<Expr>, QueryError> {
    if tokens.is_empty() {
        return Ok(None);
    }
    let end = tokens.last().map_or(0, |t| t.span.end);
    let mut p = Parser { list, tokens, i: 0, today, end };
    let expr = p.parse_or(0)?;
    if let Some(extra) = p.peek() {
        return Err(p.unbalanced(extra.span.clone(), "A closing parenthesis has no opening one."));
    }
    if expr.text_terms() > MAX_TEXT_TERMS {
        return Err(QueryError::new(QueryErrorKind::TooComplex, 0..end, format!("The search has more than {MAX_TEXT_TERMS} words.")));
    }
    if expr.nodes() > MAX_NODES {
        return Err(QueryError::new(QueryErrorKind::TooComplex, 0..end, "The search has too many parts."));
    }
    Ok(Some(expr))
}
```

In `mod.rs` add `pub(crate) mod fields; pub(crate) mod parse;`, `pub use fields::{FieldDoc, ValueType};`, and:

```rust
/// The words one list accepts, for the web's suggestions and the docs page.
pub fn describe(list: ListKind) -> Vec<FieldDoc> {
    fields::describe(list)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::`
Expected: all lexer, value, and parser tests pass (27 in total).

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "feat(search): the word registry and the parser

One static table holds every word: its spelling, value shape, the lists
it works on, its keyword values, one line of help, and an example. The
parser resolves each word:value against that table for the requested
list, so a word the language does not have is refused as unknown, a
word from another list says where it works, and a bad value says what
to write instead. describe() reads the same table."
```

---

### Task 4: Bridges, defaults, free text, and the first interface tests

**Files:**
- Create: `crates/vault/server/src/search/bridge.rs`
- Create: `crates/vault/server/src/search/fts.rs`
- Create: `crates/vault/server/src/search/emit.rs`
- Create: `crates/vault/server/src/search/tests.rs`
- Modify: `crates/vault/server/src/search/mod.rs` (add modules; add `CompileRequest`, `Filter`, `compile`)
- Modify: `crates/vault/server/src/db/sql.rs` (add `Clone` to the derive on `SqlParam`)

**Interfaces:**
- Consumes: `parse::{Expr, FieldTerm, TextTerm}`, `db::sql::SqlParam`, `db::dialect::like_ci`, `db::engine::DbEngine`.
- Produces:
  - `search::CompileRequest<'a> { list, query: &'a str, account_id: &'a str, engine: DbEngine, today: NaiveDate }`.
  - `search::Filter` with `where_sql(&self) -> &str` and `params(&self) -> &[SqlParam]`.
  - `search::compile(req) -> Result<Filter, QueryError>`.
  - `bridge::Sql { text: String, params: Vec<SqlParam> }` with `push(&str)`, `bind_text(impl Into<String>)`, `bind_int(i64)`, `like(engine, column, pattern)`.
  - `bridge::ListCtx { list, engine, account_id }` with `account_col() -> &'static str`, `conversation(out, inner)`, `message(out, inner)`, `contact(out, inner)`, `messages_link(alias) -> String`, `conversations_link(alias) -> String`.
  - `bridge::conversation_involves(conv_alias: &str, contact_expr: &str) -> String`.
  - `emit::compile(list, expr: Option<&Expr>, account_id, engine) -> Result<Filter, QueryError>`; `emit::emit_field` is a match on `term.spec.word` that Tasks 5 to 8 fill in. Until then an unhandled word answers `BadValue` reading `"{word}: is not built yet"`, and the coverage test in Task 9 fails. That is the intended build order.
  - `fts::leaf(out, engine, term: &TextTerm)`.
  - `tests::{ACCOUNT, OTHER_ACCOUNT, today, Fixture, seeded, run, sorted, err}` and the row helpers `handle`, `contact`, `conversation`, `message`, `Msg`, `attachment`, `group`, `tag`, all `pub(crate)` so later tasks' test modules use them.

- [ ] **Step 1: Write the fixture and the first interface tests**

`crates/vault/server/src/search/tests.rs`. The fixture is shared by every later task; it seeds one vault with everything the spec's test cases need.

```rust
//! Tests at the module's interface: seed a SQLite vault, compile a query
//! for a list, run it, and assert which ids come back.

use chrono::NaiveDate;
use sqlx::AnyConnection;

use super::{CompileRequest, ListKind, QueryError, compile};
use crate::db::dialect::engine_of;
use crate::db::engine::DbEngine;
use crate::db::sql::{bind_args, renumber_placeholders};

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-00000000aaaa";
pub(crate) const OTHER_ACCOUNT: &str = "00000000-0000-4000-8000-00000000bbbb";

pub(crate) fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()
}

/// Ids the fixture created, so a test can assert on them by name.
#[derive(Debug, Default)]
pub(crate) struct Fixture {
    // contacts
    pub ana: i64,
    pub bo: i64,
    pub cy: i64,
    pub jane: i64,
    pub sam: i64,
    pub nameless: i64,
    // handles
    pub me_handle: i64,
    pub ana_handle: i64,
    pub bo_handle: i64,
    pub jane_handle: i64,
    pub sam_handle: i64,
    pub nameless_handle: i64,
    // conversations
    pub ana_direct: i64,
    pub bo_direct: i64,
    pub jane_direct: i64,
    pub sam_direct: i64,
    pub archive_group: i64,
    pub big_group: i64,
    pub trashed_conv: i64,
    // messages
    pub ana_2018: i64,
    pub ana_2021: i64,
    pub bo_2023: i64,
    pub jane_avocado_from_me: i64,
    pub jane_guac_from_me: i64,
    pub jane_avocado_to_me: i64,
    pub sam_avocado_from_me: i64,
    pub jane_2018: i64,
    pub feb_big_jpeg: i64,
    pub feb_small_jpeg: i64,
    pub feb_pdf: i64,
    pub may_big_jpeg: i64,
    pub big_group_msg: i64,
    pub archive_msg: i64,
    pub trashed_msg: i64,
    // named sets
    pub family: i64,
    pub archive: i64,
}

pub(crate) async fn handle(conn: &mut AnyConnection, account: &str, raw: &str, service: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
         VALUES ($1, $2, $2, 'phone', $3) RETURNING id",
    )
    .bind(account)
    .bind(raw)
    .bind(service)
    .fetch_one(&mut *conn)
    .await
    .unwrap()
}

pub(crate) async fn contact(conn: &mut AnyConnection, account: &str, name: &str, handles: &[i64]) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(account)
    .bind(name)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    for h in handles {
        sqlx::query("INSERT INTO contact_handles (account_id, handle_id, contact_id) VALUES ($1, $2, $3)")
            .bind(account)
            .bind(h)
            .bind(id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    id
}

/// A conversation whose chat handle is `chat` and whose participants are the
/// given handles, each linked to its contact when one exists.
pub(crate) async fn conversation(
    conn: &mut AnyConnection,
    account: &str,
    chat: i64,
    kind: &str,
    title: Option<&str>,
    participants: &[i64],
) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, group_title, source_file)
         VALUES ($1, $2, $3, $4, 'seed.jsonl') RETURNING id",
    )
    .bind(account)
    .bind(chat)
    .bind(kind)
    .bind(title)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    for h in participants {
        let contact_id: Option<i64> = sqlx::query_scalar(
            "SELECT contact_id FROM contact_handles WHERE account_id = $1 AND handle_id = $2",
        )
        .bind(account)
        .bind(h)
        .fetch_optional(&mut *conn)
        .await
        .unwrap();
        sqlx::query("INSERT INTO participants (conversation_id, handle_id, contact_id) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(h)
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    id
}

pub(crate) struct Msg<'a> {
    pub conversation: i64,
    pub timestamp: &'a str,
    pub from_me: bool,
    pub sender: Option<i64>,
    pub body: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub source: &'a str,
    pub service: &'a str,
}

pub(crate) fn msg<'a>(conversation: i64, timestamp: &'a str, from_me: bool, sender: Option<i64>, body: &'a str) -> Msg<'a> {
    Msg { conversation, timestamp, from_me, sender, body: Some(body), subject: None, source: "imessage", service: "imessage" }
}

pub(crate) async fn message(conn: &mut AnyConnection, account: &str, m: Msg<'_>) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO messages (conversation_id, account_id, source, timestamp, is_from_me,
                               sender_handle_id, service, subject, body, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 0) RETURNING id",
    )
    .bind(m.conversation)
    .bind(account)
    .bind(m.source)
    .bind(m.timestamp)
    .bind(i64::from(m.from_me))
    .bind(m.sender)
    .bind(m.service)
    .bind(m.subject)
    .bind(m.body)
    .fetch_one(&mut *conn)
    .await
    .unwrap()
}

pub(crate) async fn attachment(conn: &mut AnyConnection, message: i64, name: &str, mime: &str, size: i64) {
    sqlx::query("INSERT INTO attachments (message_id, original_name, mime_type, size_bytes) VALUES ($1, $2, $3, $4)")
        .bind(message)
        .bind(name)
        .bind(mime)
        .bind(size)
        .execute(&mut *conn)
        .await
        .unwrap();
}

pub(crate) async fn group(conn: &mut AnyConnection, account: &str, name: &str, members: &[i64]) -> i64 {
    let id: i64 = sqlx::query_scalar("INSERT INTO contact_groups (account_id, name) VALUES ($1, $2) RETURNING id")
        .bind(account)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    for c in members {
        sqlx::query("INSERT INTO contact_group_members (contact_id, group_id) VALUES ($1, $2)")
            .bind(c)
            .bind(id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    id
}

pub(crate) async fn tag(conn: &mut AnyConnection, account: &str, name: &str, conversations: &[i64]) -> i64 {
    let id: i64 = sqlx::query_scalar("INSERT INTO message_tags (account_id, name) VALUES ($1, $2) RETURNING id")
        .bind(account)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    for c in conversations {
        sqlx::query("INSERT INTO message_tag_members (conversation_id, tag_id) VALUES ($1, $2)")
            .bind(c)
            .bind(id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    id
}

/// A vault with two accounts and every row the spec's cases need.
pub(crate) async fn seeded() -> (sqlx::AnyPool, tempfile::TempDir, Fixture) {
    let (pool, dir) = crate::db::engine::test_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    crate::db::schema::ensure_vault_schema(&mut conn).await.unwrap();
    for (id, name) in [(ACCOUNT, "alice"), (OTHER_ACCOUNT, "bob")] {
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, $2)")
            .bind(id)
            .bind(name)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    let a = ACCOUNT;
    let mut f = Fixture::default();

    f.me_handle = handle(&mut conn, a, "+15550000", "imessage").await;
    sqlx::query("INSERT INTO account_handles (account_id, handle_id) VALUES ($1, $2)")
        .bind(a)
        .bind(f.me_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
    f.ana_handle = handle(&mut conn, a, "+15550001", "imessage").await;
    f.bo_handle = handle(&mut conn, a, "+15550002", "sms").await;
    f.jane_handle = handle(&mut conn, a, "jane.doe@gmail.com", "imessage").await;
    f.sam_handle = handle(&mut conn, a, "sam@icloud.com", "imessage").await;
    f.nameless_handle = handle(&mut conn, a, "+15550009", "sms").await;
    let cy_handle = handle(&mut conn, a, "+15550003", "whatsapp").await;

    f.ana = contact(&mut conn, a, "Ana", &[f.ana_handle]).await;
    f.bo = contact(&mut conn, a, "Bo", &[f.bo_handle]).await;
    f.cy = contact(&mut conn, a, "Cy", &[cy_handle]).await;
    f.jane = contact(&mut conn, a, "Jane Doe", &[f.jane_handle]).await;
    f.sam = contact(&mut conn, a, "Sam", &[f.sam_handle]).await;
    f.nameless = contact(&mut conn, a, "", &[f.nameless_handle]).await;

    f.ana_direct = conversation(&mut conn, a, f.ana_handle, "individual", None, &[f.ana_handle]).await;
    f.bo_direct = conversation(&mut conn, a, f.bo_handle, "individual", None, &[f.bo_handle]).await;
    f.jane_direct = conversation(&mut conn, a, f.jane_handle, "individual", None, &[f.jane_handle]).await;
    f.sam_direct = conversation(&mut conn, a, f.sam_handle, "individual", None, &[f.sam_handle]).await;
    let archive_chat = handle(&mut conn, a, "chat100", "imessage").await;
    f.archive_group = conversation(&mut conn, a, archive_chat, "group", Some("Old Times"), &[f.ana_handle, f.bo_handle, f.sam_handle]).await;
    let big_chat = handle(&mut conn, a, "chat200", "imessage").await;
    f.big_group = conversation(&mut conn, a, big_chat, "group", Some("Book Club"), &[f.ana_handle, f.bo_handle, f.jane_handle, f.sam_handle]).await;
    let trashed_chat = handle(&mut conn, a, "chat300", "imessage").await;
    f.trashed_conv = conversation(&mut conn, a, trashed_chat, "group", Some("Gone"), &[f.ana_handle, f.bo_handle]).await;
    sqlx::query("INSERT INTO trashed_conversations (account_id, conversation_id) VALUES ($1, $2)")
        .bind(a)
        .bind(f.trashed_conv)
        .execute(&mut *conn)
        .await
        .unwrap();

    f.ana_2018 = message(&mut conn, a, msg(f.ana_direct, "2018-03-01T10:00:00Z", false, Some(f.ana_handle), "hello from ana")).await;
    f.ana_2021 = message(&mut conn, a, msg(f.ana_direct, "2021-05-01T10:00:00Z", true, None, "hi ana")).await;
    f.bo_2023 = message(&mut conn, a, Msg { service: "sms", ..msg(f.bo_direct, "2023-01-01T10:00:00Z", false, Some(f.bo_handle), "bo here") }).await;
    f.jane_avocado_from_me = message(&mut conn, a, msg(f.jane_direct, "2024-02-10T10:00:00Z", true, None, "want some avocado")).await;
    f.jane_guac_from_me = message(&mut conn, a, msg(f.jane_direct, "2024-02-11T10:00:00Z", true, None, "guacamole night at mine")).await;
    f.jane_avocado_to_me = message(&mut conn, a, msg(f.jane_direct, "2024-02-12T10:00:00Z", false, Some(f.jane_handle), "avocado toast?")).await;
    f.sam_avocado_from_me = message(&mut conn, a, msg(f.sam_direct, "2024-02-13T10:00:00Z", true, None, "avocado again")).await;
    f.jane_2018 = message(&mut conn, a, msg(f.jane_direct, "2018-06-01T10:00:00Z", false, Some(f.jane_handle), "first hello")).await;
    f.feb_big_jpeg = message(&mut conn, a, msg(f.jane_direct, "2024-02-20T10:00:00Z", false, Some(f.jane_handle), "photo")).await;
    attachment(&mut conn, f.feb_big_jpeg, "beach.jpg", "image/jpeg", 900 * 1024).await;
    f.feb_small_jpeg = message(&mut conn, a, msg(f.jane_direct, "2024-02-21T10:00:00Z", false, Some(f.jane_handle), "small photo")).await;
    attachment(&mut conn, f.feb_small_jpeg, "thumb.jpg", "image/jpeg", 100 * 1024).await;
    f.feb_pdf = message(&mut conn, a, msg(f.jane_direct, "2024-02-22T10:00:00Z", false, Some(f.jane_handle), "the document")).await;
    attachment(&mut conn, f.feb_pdf, "notes.pdf", "application/pdf", 2 * 1024 * 1024).await;
    f.may_big_jpeg = message(&mut conn, a, msg(f.jane_direct, "2024-05-20T10:00:00Z", false, Some(f.jane_handle), "later photo")).await;
    attachment(&mut conn, f.may_big_jpeg, "hike.jpg", "image/jpeg", 900 * 1024).await;
    f.big_group_msg = message(&mut conn, a, Msg { subject: Some("Dinner plans"), ..msg(f.big_group, "2024-03-01T10:00:00Z", false, Some(f.sam_handle), "who is in") }).await;
    f.archive_msg = message(&mut conn, a, Msg { source: "whatsapp", service: "whatsapp", ..msg(f.archive_group, "2019-01-01T10:00:00Z", false, Some(f.bo_handle), "old") }).await;
    f.trashed_msg = message(&mut conn, a, msg(f.trashed_conv, "2019-02-01T10:00:00Z", false, Some(f.bo_handle), "gone")).await;

    f.family = group(&mut conn, a, "Family", &[f.ana]).await;
    f.archive = tag(&mut conn, a, "Archive", &[f.archive_group]).await;

    // The other account has one contact and one message that must never show.
    let other_handle = handle(&mut conn, OTHER_ACCOUNT, "+15559999", "imessage").await;
    contact(&mut conn, OTHER_ACCOUNT, "Ana", &[other_handle]).await;
    let other_conv = conversation(&mut conn, OTHER_ACCOUNT, other_handle, "individual", None, &[other_handle]).await;
    message(&mut conn, OTHER_ACCOUNT, msg(other_conv, "2024-02-10T10:00:00Z", false, Some(other_handle), "avocado")).await;

    drop(conn);
    (pool, dir, f)
}

/// Compile `q` for `list` and return the matching ids, ascending.
pub(crate) async fn run(conn: &mut AnyConnection, list: ListKind, q: &str) -> Vec<i64> {
    let f = compile(CompileRequest { list, query: q, account_id: ACCOUNT, engine: engine_of(conn), today: today() })
        .unwrap_or_else(|e| panic!("{q:?} on {list:?}: {}", e.message));
    let (table, alias) = match list {
        ListKind::Contacts => ("contacts", "ct"),
        ListKind::Conversations => ("conversations", "c"),
        ListKind::Messages => ("messages", "m"),
    };
    let sql = renumber_placeholders(&format!(
        "SELECT {alias}.id FROM {table} {alias} WHERE {} ORDER BY {alias}.id",
        f.where_sql()
    ));
    let rows: Vec<(i64,)> = sqlx::query_as_with(&sql, bind_args(f.params()))
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("{q:?} on {list:?}: {e}\n{sql}"));
    rows.into_iter().map(|(id,)| id).collect()
}

pub(crate) fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

/// Compile `q` for `list` expecting a refusal.
pub(crate) fn err(list: ListKind, q: &str) -> QueryError {
    compile(CompileRequest { list, query: q, account_id: ACCOUNT, engine: DbEngine::Sqlite, today: today() })
        .expect_err("expected a refusal")
}

mod free_text {
    use super::*;

    #[tokio::test]
    async fn empty_query_returns_the_account_rows_minus_trash() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "").await,
            sorted(vec![f.ana, f.bo, f.cy, f.jane, f.sam, f.nameless])
        );
        let convs = run(&mut conn, ListKind::Conversations, "").await;
        assert!(!convs.contains(&f.trashed_conv));
        assert_eq!(convs.len(), 6);
        let msgs = run(&mut conn, ListKind::Messages, "").await;
        assert!(msgs.contains(&f.ana_2018));
        assert!(!msgs.contains(&f.trashed_msg));
        assert!(msgs.iter().all(|id| *id <= f.trashed_msg), "other account's message leaked");
    }

    #[tokio::test]
    async fn bare_text_is_the_rows_own_text() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Contacts: name or handle.
        assert_eq!(run(&mut conn, ListKind::Contacts, "ana").await, vec![f.ana]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "gmail").await, vec![f.jane]);
        // Conversations: title, or a participant's name or handle.
        assert_eq!(run(&mut conn, ListKind::Conversations, "book").await, vec![f.big_group]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "jane").await, sorted(vec![f.jane_direct, f.big_group]));
        // Messages: body, subject, attachment names, through the full-text index.
        assert_eq!(
            run(&mut conn, ListKind::Messages, "avocado").await,
            sorted(vec![f.jane_avocado_from_me, f.jane_avocado_to_me, f.sam_avocado_from_me])
        );
        assert_eq!(run(&mut conn, ListKind::Messages, "dinner").await, vec![f.big_group_msg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "beach").await, vec![f.feb_big_jpeg]);
        // A person's name is not message text.
        assert_eq!(run(&mut conn, ListKind::Messages, "jane").await, Vec::<i64>::new());
    }

    #[tokio::test]
    async fn phrases_prefixes_negation_and_or() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Messages, "\"guacamole night\"").await, vec![f.jane_guac_from_me]);
        assert_eq!(run(&mut conn, ListKind::Messages, "avoc*").await.len(), 3);
        assert_eq!(
            run(&mut conn, ListKind::Messages, "avocado -toast").await,
            sorted(vec![f.jane_avocado_from_me, f.sam_avocado_from_me])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "toast or guacamole").await,
            sorted(vec![f.jane_guac_from_me, f.jane_avocado_to_me])
        );
        assert_eq!(run(&mut conn, ListKind::Messages, "(toast or guacamole) avocado").await, vec![f.jane_avocado_to_me]);
    }

    #[test]
    fn compile_is_deterministic_and_pure() {
        let req = || CompileRequest {
            list: ListKind::Messages,
            query: "avocado -toast",
            account_id: ACCOUNT,
            engine: DbEngine::Postgres,
            today: today(),
        };
        let a = compile(req()).unwrap();
        let b = compile(req()).unwrap();
        assert_eq!(a.where_sql(), b.where_sql());
        assert_eq!(a.params(), b.params());
    }

    #[test]
    fn a_fragment_mentions_only_its_base_alias_and_binds_in_order() {
        let f = compile(CompileRequest {
            list: ListKind::Contacts,
            query: "ana",
            account_id: ACCOUNT,
            engine: DbEngine::Sqlite,
            today: today(),
        })
        .unwrap();
        assert!(f.where_sql().starts_with("(ct.account_id = ?"));
        assert_eq!(f.where_sql().matches('?').count(), f.params().len());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::tests`
Expected: compile error, `compile` and `CompileRequest` missing.

- [ ] **Step 3: Write `bridge.rs`**

```rust
//! `Sql`, a fragment plus the values it binds, and `ListCtx`, which says
//! what "this contact", "this conversation", and "this message" mean from
//! each list's base row. Every emitter is written once against the alias it
//! needs and asks the context to wrap it.

use crate::db::dialect::like_ci;
use crate::db::engine::DbEngine;
use crate::db::sql::SqlParam;

use super::ListKind;

/// A fragment under construction. `params` are in the textual order of `text`.
#[derive(Debug, Default)]
pub(crate) struct Sql {
    pub text: String,
    pub params: Vec<SqlParam>,
}

impl Sql {
    pub fn push(&mut self, s: &str) {
        self.text.push_str(s);
    }

    /// Write `?` and bind a text value.
    pub fn bind_text(&mut self, v: impl Into<String>) {
        self.text.push('?');
        self.params.push(SqlParam::Text(v.into()));
    }

    /// Write `?` and bind an integer.
    pub fn bind_int(&mut self, v: i64) {
        self.text.push('?');
        self.params.push(SqlParam::Int(v));
    }

    /// `column LIKE ?` case-insensitively, binding `pattern`.
    pub fn like(&mut self, engine: DbEngine, column: &str, pattern: &str) {
        self.text.push_str(column);
        self.text.push(' ');
        self.text.push_str(like_ci(engine));
        self.params.push(SqlParam::Text(pattern.to_string()));
    }
}

/// Conversation `conv` involves the contact `contact_expr`: one of the
/// contact's handles is the chat handle or a participant.
pub(crate) fn conversation_involves(conv: &str, contact_expr: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM contact_handles chi
           WHERE chi.account_id = {conv}.account_id AND chi.contact_id = {contact_expr}
             AND (chi.handle_id = {conv}.chat_handle_id
                  OR EXISTS (SELECT 1 FROM participants pi WHERE pi.conversation_id = {conv}.id AND pi.handle_id = chi.handle_id)))"
    )
}

/// Which list a fragment is for, and what it needs from the request.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ListCtx<'a> {
    pub list: ListKind,
    pub engine: DbEngine,
    pub account_id: &'a str,
}

impl ListCtx<'_> {
    /// The base row's account column.
    pub fn account_col(&self) -> &'static str {
        match self.list {
            ListKind::Contacts => "ct.account_id",
            ListKind::Conversations => "c.account_id",
            ListKind::Messages => "m.account_id",
        }
    }

    /// Wrap `inner`, written against conversation alias `c`, so it is true of
    /// the base row: the conversation itself, the message's conversation, or
    /// some conversation the contact is in.
    pub fn conversation(&self, out: &mut Sql, inner: impl FnOnce(&mut Sql)) {
        match self.list {
            ListKind::Conversations => {
                out.push("(");
                inner(out);
                out.push(")");
            }
            ListKind::Messages => {
                out.push("EXISTS (SELECT 1 FROM conversations c WHERE c.id = m.conversation_id AND (");
                inner(out);
                out.push("))");
            }
            ListKind::Contacts => {
                out.push(&format!(
                    "EXISTS (SELECT 1 FROM conversations c WHERE c.account_id = ct.account_id AND {} AND (",
                    conversation_involves("c", "ct.id")
                ));
                inner(out);
                out.push("))");
            }
        }
    }

    /// Wrap `inner`, written against message alias `m` (a non-duplicate
    /// message; conversation alias `c` is also in scope), so it is true of the
    /// base row.
    pub fn message(&self, out: &mut Sql, inner: impl FnOnce(&mut Sql)) {
        match self.list {
            ListKind::Messages => {
                out.push("EXISTS (SELECT 1 FROM conversations c WHERE c.id = m.conversation_id AND (");
                inner(out);
                out.push("))");
            }
            ListKind::Conversations => {
                out.push("EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL AND (");
                inner(out);
                out.push("))");
            }
            ListKind::Contacts => {
                out.push(&format!(
                    "EXISTS (SELECT 1 FROM conversations c JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
                       WHERE c.account_id = ct.account_id AND {} AND (",
                    conversation_involves("c", "ct.id")
                ));
                inner(out);
                out.push("))");
            }
        }
    }

    /// Wrap `inner`, written against contact alias `ct`, so it is true of the
    /// base row: the contact itself, or some contact linked to a participant.
    pub fn contact(&self, out: &mut Sql, inner: impl FnOnce(&mut Sql)) {
        match self.list {
            ListKind::Contacts => {
                out.push("(");
                inner(out);
                out.push(")");
            }
            ListKind::Conversations => {
                out.push(&format!(
                    "EXISTS (SELECT 1 FROM contacts ct WHERE ct.account_id = c.account_id AND {} AND (",
                    conversation_involves("c", "ct.id")
                ));
                inner(out);
                out.push("))");
            }
            ListKind::Messages => {
                out.push(&format!(
                    "EXISTS (SELECT 1 FROM conversations c JOIN contacts ct ON ct.account_id = c.account_id
                       WHERE c.id = m.conversation_id AND {} AND (",
                    conversation_involves("c", "ct.id")
                ));
                inner(out);
                out.push("))");
            }
        }
    }

    /// A WHERE fragment tying messages alias `m2` to the base row, for
    /// MIN, MAX, and COUNT subqueries. Excludes duplicates.
    pub fn messages_link(&self, m2: &str) -> String {
        match self.list {
            ListKind::Messages => format!("{m2}.conversation_id = m.conversation_id AND {m2}.duplicate_of IS NULL"),
            ListKind::Conversations => format!("{m2}.conversation_id = c.id AND {m2}.duplicate_of IS NULL"),
            ListKind::Contacts => format!(
                "{m2}.account_id = ct.account_id AND {m2}.duplicate_of IS NULL AND EXISTS (SELECT 1 FROM conversations c2 WHERE c2.id = {m2}.conversation_id AND {})",
                conversation_involves("c2", "ct.id")
            ),
        }
    }

    /// A WHERE fragment tying conversations alias `c2` to the base contact.
    /// Only meaningful on Contacts.
    pub fn conversations_link(&self, c2: &str) -> String {
        format!("{c2}.account_id = ct.account_id AND {}", conversation_involves(c2, "ct.id"))
    }
}
```

- [ ] **Step 4: Write `fts.rs`**

```rust
//! The full-text leaf for a free-text term on Messages. SQLite uses the
//! contentless FTS5 table; Postgres uses the `search_tsv` column. Both index
//! body, subject, attachment names, and transcriptions.

use crate::db::engine::DbEngine;

use super::bridge::Sql;
use super::parse::TextTerm;

/// Quote for FTS5 so operators and punctuation are literal text.
fn fts5_literal(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// `'term':*` for `to_tsquery`, or `None` when the term holds a quote or
/// backslash, which tsquery literals cannot carry.
fn pg_prefix(term: &str) -> Option<String> {
    if term.is_empty() || term.contains(['\\', '\'']) {
        return None;
    }
    Some(format!("'{term}':*"))
}

/// Message `m` matches `term` in the full-text index.
pub(crate) fn leaf(out: &mut Sql, engine: DbEngine, term: &TextTerm) {
    match engine {
        DbEngine::Sqlite => {
            let q = match term {
                TextTerm::Term { text, prefix: true } => format!("{}*", fts5_literal(text)),
                TextTerm::Term { text, prefix: false } => fts5_literal(text),
                TextTerm::Phrase(text) => fts5_literal(text),
            };
            out.push("EXISTS (SELECT 1 FROM messages_fts fts WHERE fts.rowid = m.id AND messages_fts MATCH ");
            out.bind_text(q);
            out.push(")");
        }
        DbEngine::Postgres => {
            let (func, arg) = match term {
                TextTerm::Term { text, prefix: true } => match pg_prefix(text) {
                    Some(q) => ("to_tsquery", q),
                    None => ("plainto_tsquery", text.clone()),
                },
                TextTerm::Term { text, prefix: false } => ("plainto_tsquery", text.clone()),
                TextTerm::Phrase(text) => ("phraseto_tsquery", text.clone()),
            };
            out.push(&format!("m.search_tsv @@ {func}('simple', "));
            out.bind_text(arg);
            out.push(")");
        }
    }
}
```

- [ ] **Step 5: Write `emit.rs` with the defaults and the free-text emitters**

```rust
//! Defaults plus one emitter per word. Every emitter writes SQL against the
//! innermost alias it needs and lets `ListCtx` wrap it for the base row.

use crate::db::engine::DbEngine;

use super::bridge::{ListCtx, Sql};
use super::error::{QueryError, QueryErrorKind};
use super::fts;
use super::parse::{Expr, FieldTerm, TextTerm};
use super::{Filter, ListKind};

/// Contact `ct` is not in the trash.
const NOT_TRASHED_CONTACT: &str =
    "NOT EXISTS (SELECT 1 FROM trashed_contacts tct WHERE tct.account_id = ct.account_id AND tct.contact_id = ct.id)";
/// Conversation `c` is not in the trash and neither is its chat handle.
const NOT_TRASHED_CONVERSATION: &str =
    "NOT EXISTS (SELECT 1 FROM trashed_conversations tc WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id)
     AND NOT EXISTS (SELECT 1 FROM trashed_handles th WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id)";

/// Compile a parsed query into one parenthesised WHERE fragment.
pub(crate) fn compile(
    list: ListKind,
    expr: Option<&Expr>,
    account_id: &str,
    engine: DbEngine,
) -> Result<Filter, QueryError> {
    let ctx = ListCtx { list, engine, account_id };
    let mut out = Sql::default();
    out.push("(");
    out.push(ctx.account_col());
    out.push(" = ");
    out.bind_text(account_id);
    let uses = |word: &str| expr.is_some_and(|e| e.uses(word));
    match list {
        ListKind::Contacts => {
            if !uses("trashed") {
                out.push(" AND ");
                out.push(NOT_TRASHED_CONTACT);
            }
        }
        ListKind::Conversations => {
            if !uses("trashed") {
                out.push(" AND ");
                out.push(NOT_TRASHED_CONVERSATION);
            }
            // A thread with only duplicate messages is hidden, unless the
            // query is about an Import Run, whose threads may be exactly that.
            if !uses("import") {
                out.push(" AND EXISTS (SELECT 1 FROM messages m0 WHERE m0.conversation_id = c.id AND m0.duplicate_of IS NULL)");
            }
        }
        ListKind::Messages => {
            // A query about one source wants that source's copies, duplicates included.
            if !uses("source") {
                out.push(" AND m.duplicate_of IS NULL");
            }
            out.push(" AND EXISTS (SELECT 1 FROM conversations c WHERE c.id = m.conversation_id AND ");
            out.push(NOT_TRASHED_CONVERSATION);
            out.push(")");
        }
    }
    if let Some(expr) = expr {
        out.push(" AND ");
        emit_expr(&ctx, &mut out, expr)?;
    }
    out.push(")");
    Ok(Filter { where_sql: out.text, params: out.params })
}

fn emit_expr(ctx: &ListCtx<'_>, out: &mut Sql, expr: &Expr) -> Result<(), QueryError> {
    match expr {
        Expr::And(parts) | Expr::Or(parts) => {
            let joiner = if matches!(expr, Expr::And(_)) { " AND " } else { " OR " };
            out.push("(");
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    out.push(joiner);
                }
                emit_expr(ctx, out, part)?;
            }
            out.push(")");
        }
        Expr::Not(inner) => {
            out.push("NOT (");
            emit_expr(ctx, out, inner)?;
            out.push(")");
        }
        Expr::Text(term) => emit_text(ctx, out, term),
        Expr::Field(term) => emit_field(ctx, out, term)?,
    }
    Ok(())
}

/// `%text%`, or `text%` for a prefix. A phrase is matched as one string.
fn contains_pattern(term: &TextTerm) -> String {
    match term {
        TextTerm::Term { text, prefix: true } => format!("{text}%"),
        TextTerm::Term { text, prefix: false } | TextTerm::Phrase(text) => format!("%{text}%"),
    }
}

/// Free text: the row's own text, one meaning applied per row type.
fn emit_text(ctx: &ListCtx<'_>, out: &mut Sql, term: &TextTerm) {
    let e = ctx.engine;
    let pat = contains_pattern(term);
    match ctx.list {
        ListKind::Contacts => {
            out.push("(");
            out.like(e, "COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)')", &pat);
            out.push(" OR EXISTS (SELECT 1 FROM contact_handles ch JOIN handles h ON h.id = ch.handle_id WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id AND (");
            out.like(e, "h.raw", &pat);
            out.push(" OR ");
            out.like(e, "coalesce(h.normalized, '')", &pat);
            out.push(")))");
        }
        ListKind::Conversations => {
            out.push("(");
            out.like(e, "coalesce(c.group_title, '')", &pat);
            out.push(" OR EXISTS (SELECT 1 FROM handles hc WHERE hc.id = c.chat_handle_id AND ");
            out.like(e, "hc.raw", &pat);
            out.push(") OR EXISTS (SELECT 1 FROM participants p JOIN handles ph ON ph.id = p.handle_id LEFT JOIN contacts pct ON pct.id = p.contact_id WHERE p.conversation_id = c.id AND (");
            out.like(e, "ph.raw", &pat);
            out.push(" OR ");
            out.like(e, "coalesce(p.name_alias, '')", &pat);
            out.push(" OR ");
            out.like(e, "coalesce(pct.preferred_name, '')", &pat);
            out.push(")))");
        }
        ListKind::Messages => fts::leaf(out, e, term),
    }
}

/// One `word:values`. Values are OR-ed. Tasks 5 to 8 add the arms.
fn emit_field(ctx: &ListCtx<'_>, out: &mut Sql, term: &FieldTerm) -> Result<(), QueryError> {
    let _ = (ctx, out);
    Err(QueryError::new(
        QueryErrorKind::BadValue,
        term.span.clone(),
        format!("{}: is not built yet", term.spec.word),
    ))
}
```

- [ ] **Step 6: Add the interface to `mod.rs`**

```rust
pub(crate) mod bridge;
pub(crate) mod emit;
pub(crate) mod fts;
#[cfg(test)]
pub(crate) mod tests;

use chrono::NaiveDate;

use crate::db::engine::DbEngine;
use crate::db::sql::SqlParam;

/// Everything `compile` needs that is not in the query string.
#[derive(Debug, Clone, Copy)]
pub struct CompileRequest<'a> {
    /// Which list the query is for.
    pub list: ListKind,
    /// The query string as typed.
    pub query: &'a str,
    /// The signed-in account; every fragment is scoped to it.
    pub account_id: &'a str,
    /// Which engine's SQL to write.
    pub engine: DbEngine,
    /// Relative dates resolve against this day. Never read from the clock here.
    pub today: NaiveDate,
}

/// One parenthesised boolean expression plus the values it binds.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    where_sql: String,
    params: Vec<SqlParam>,
}

impl Filter {
    /// One parenthesised expression with `?` placeholders. Never empty: an
    /// empty query compiles to the account scope and the defaults alone.
    pub fn where_sql(&self) -> &str {
        &self.where_sql
    }

    /// The values to bind, in the textual order of `where_sql`.
    pub fn params(&self) -> &[SqlParam] {
        &self.params
    }
}

/// Parse `query` and compile it for `list`. Pure: no database, no clock.
///
/// # Errors
///
/// A [`QueryError`] naming the word and the list, with a byte span into the input.
pub fn compile(req: CompileRequest<'_>) -> Result<Filter, QueryError> {
    let tokens = lex::tokenize(req.query)?;
    let expr = parse::parse(req.list, &tokens, req.today)?;
    emit::compile(req.list, expr.as_ref(), req.account_id, req.engine)
}
```

Add `Clone` to the derive on `SqlParam` in `db/sql.rs`, so `Filter` can derive it.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::tests::free_text`
Expected: 5 passed. If the Messages `beach` case fails, check that `ensure_vault_schema` installs the FTS triggers on the test pool (it does for the export tests); the fixture inserts attachments after their message on purpose so the attachment trigger re-indexes the row.

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server/src/search crates/vault/server/src/db/sql.rs
git commit -m "feat(search): compile free text for all three lists over per-list bridges

The bridges say what this contact, this conversation, and this message
mean from each list's base row, so every filter is written once and
wrapped for the list that asked. compile() answers with one
parenthesised fragment that already carries the account scope, the
dedupe rule, and the trash default, so a caller adds no joins. Free
text is the row's own text: a contact's name and handles, a
conversation's title and participants, a message's indexed text."
```

---

### Task 5: Text words: `body:`, `subject:`, `name:`, `title:`, `handle:`, `filename:`

**Files:**
- Modify: `crates/vault/server/src/search/emit.rs` (replace the `emit_field` stub with the dispatcher below; add the `text` helpers)
- Modify: `crates/vault/server/src/search/bridge.rs` (add `Sql::param_text`)
- Modify: `crates/vault/server/src/search/tests.rs` (add `mod text_words`)

**Interfaces:**
- Consumes: `bridge::{ListCtx, Sql}`, `value::Value`, `parse::FieldTerm`.
- Produces in `emit.rs`: `emit_field` that ORs a word's values and dispatches on `term.spec.word`; `text_match(out, engine, column, &Value)`; `emit_text_word(ctx, out, word, &Value)`. Adds `Sql::param_text(&mut self, v)` which binds a value for a `?` already written.

- [ ] **Step 1: Write the failing tests**

Append to `tests.rs`:

```rust
mod text_words {
    use super::*;

    #[tokio::test]
    async fn name_and_handle_on_contacts() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Contacts, "name:jane").await, vec![f.jane]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "name:none").await, vec![f.nameless]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "name:any").await, sorted(vec![f.ana, f.bo, f.cy, f.jane, f.sam]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "handle:gmail").await, vec![f.jane]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "handle:+1555*").await.len(), 4);
        assert_eq!(run(&mut conn, ListKind::Contacts, "handle:none").await, Vec::<i64>::new());
    }

    #[tokio::test]
    async fn name_handle_and_title_on_conversations() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Conversations, "name:jane").await, sorted(vec![f.jane_direct, f.big_group]));
        assert_eq!(run(&mut conn, ListKind::Conversations, "handle:icloud").await, sorted(vec![f.sam_direct, f.archive_group, f.big_group]));
        assert_eq!(run(&mut conn, ListKind::Conversations, "title:book").await, vec![f.big_group]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "title:none").await, sorted(vec![f.ana_direct, f.bo_direct, f.jane_direct, f.sam_direct]));
    }

    #[tokio::test]
    async fn body_subject_and_filename() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Messages, "body:toast").await, vec![f.jane_avocado_to_me]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "body:toast").await, vec![f.jane_direct]);
        assert_eq!(run(&mut conn, ListKind::Messages, "subject:dinner").await, vec![f.big_group_msg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "subject:any").await, vec![f.big_group_msg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "filename:beach*").await, vec![f.feb_big_jpeg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "filename:.jpg").await, sorted(vec![f.feb_big_jpeg, f.feb_small_jpeg, f.may_big_jpeg]));
        assert_eq!(run(&mut conn, ListKind::Conversations, "filename:notes").await, vec![f.jane_direct]);
        assert_eq!(run(&mut conn, ListKind::Messages, "-body:avocado body:any").await.len(), 11);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::tests::text_words`
Expected: 3 failed with "is not built yet".

- [ ] **Step 3: Write the dispatcher and the text emitters**

In `bridge.rs` add to `impl Sql`:

```rust
    /// Bind a text value for a `?` the caller already wrote (dialect helpers
    /// like `name_eq_ci` write their own placeholder).
    pub fn param_text(&mut self, v: impl Into<String>) {
        self.params.push(SqlParam::Text(v.into()));
    }
```

In `emit.rs`, replace the `emit_field` stub with:

```rust
use super::value::Value;

/// One `word:values`. The values are OR-ed, so `service:imessage,sms` is
/// either. Each arm is written against the innermost alias it needs.
fn emit_field(ctx: &ListCtx<'_>, out: &mut Sql, term: &FieldTerm) -> Result<(), QueryError> {
    out.push("(");
    for (i, value) in term.values.iter().enumerate() {
        if i > 0 {
            out.push(" OR ");
        }
        emit_one(ctx, out, term, value)?;
    }
    out.push(")");
    Ok(())
}

fn emit_one(ctx: &ListCtx<'_>, out: &mut Sql, term: &FieldTerm, v: &Value) -> Result<(), QueryError> {
    match term.spec.word {
        "body" | "subject" | "name" | "title" | "handle" | "filename" => {
            emit_text_word(ctx, out, term.spec.word, v);
            Ok(())
        }
        _ => Err(QueryError::new(
            QueryErrorKind::BadValue,
            term.span.clone(),
            format!("{}: is not built yet", term.spec.word),
        )),
    }
}

/// `column` contains, starts with, is empty, or is not empty.
fn text_match(out: &mut Sql, engine: DbEngine, column: &str, v: &Value) {
    match v {
        Value::Text(t) => out.like(engine, column, &format!("%{t}%")),
        Value::Prefix(p) => out.like(engine, column, &format!("{p}%")),
        Value::Keyword("none") => out.push(&format!("NULLIF(trim({column}), '') IS NULL")),
        Value::Keyword("any") => out.push(&format!("NULLIF(trim({column}), '') IS NOT NULL")),
        _ => out.push("1=0"),
    }
}

/// A participant's display name: the linked contact's name, else the
/// per-conversation alias. Alias `p` is a participants row, `pct` its contact.
const PARTICIPANT_NAME: &str =
    "coalesce(NULLIF(trim(pct.preferred_name), ''), NULLIF(trim(p.name_alias), ''), '')";

fn emit_text_word(ctx: &ListCtx<'_>, out: &mut Sql, word: &str, v: &Value) {
    let e = ctx.engine;
    match (word, ctx.list) {
        ("body", _) => ctx.message(out, |o| text_match(o, e, "coalesce(m.body, '')", v)),
        ("subject", _) => ctx.message(out, |o| text_match(o, e, "coalesce(m.subject, '')", v)),
        ("title", _) => ctx.conversation(out, |o| text_match(o, e, "coalesce(c.group_title, '')", v)),
        ("name", ListKind::Contacts) => text_match(out, e, "ct.preferred_name", v),
        ("name", _) => ctx.conversation(out, |o| {
            o.push("EXISTS (SELECT 1 FROM participants p LEFT JOIN contacts pct ON pct.id = p.contact_id WHERE p.conversation_id = c.id AND ");
            text_match(o, e, PARTICIPANT_NAME, v);
            o.push(")");
        }),
        ("handle", ListKind::Contacts) => match v {
            Value::Keyword("none") => out.push("NOT EXISTS (SELECT 1 FROM contact_handles ch WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id)"),
            Value::Keyword("any") => out.push("EXISTS (SELECT 1 FROM contact_handles ch WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id)"),
            _ => {
                out.push("EXISTS (SELECT 1 FROM contact_handles ch JOIN handles h ON h.id = ch.handle_id WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id AND (");
                text_match(out, e, "h.raw", v);
                out.push(" OR ");
                text_match(out, e, "coalesce(h.normalized, '')", v);
                out.push("))");
            }
        },
        ("handle", _) => ctx.conversation(out, |o| match v {
            Value::Keyword("none") => o.push("NOT EXISTS (SELECT 1 FROM participants p WHERE p.conversation_id = c.id AND p.handle_id IS NOT NULL)"),
            Value::Keyword("any") => o.push("1=1"),
            _ => {
                o.push("EXISTS (SELECT 1 FROM handles h WHERE (h.id = c.chat_handle_id OR EXISTS (SELECT 1 FROM participants p WHERE p.conversation_id = c.id AND p.handle_id = h.id)) AND (");
                text_match(o, e, "h.raw", v);
                o.push(" OR ");
                text_match(o, e, "coalesce(h.normalized, '')", v);
                o.push("))");
            }
        }),
        ("filename", _) => ctx.message(out, |o| {
            o.push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id AND ");
            text_match(o, e, "coalesce(a.original_name, '')", v);
            o.push(")");
        }),
        _ => out.push("1=0"),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::tests`
Expected: the free-text and text-word tests pass (8).

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "feat(search): the six text words

body:, subject:, name:, title:, handle:, and filename: match as
contains, a trailing star matches a prefix, and none or any ask whether
the field is empty. On Conversations and Messages, name: and handle:
look at participants; on Contacts they look at the contact itself."
```

---

### Task 6: People and places: `with:`, `from:`, `to:`, `in:`, `group:`, `tag:`, `import:`

**Files:**
- Modify: `crates/vault/server/src/search/emit.rs` (add arms and helpers)
- Modify: `crates/vault/server/src/search/tests.rs` (add `mod people_words`)

**Interfaces:**
- Consumes: `db::dialect::name_eq_ci`, `db::contacts::UNKNOWN_CONTACT_SQL`, `bridge::conversation_involves`.
- Produces in `emit.rs`: `person_matches(out, engine, handle_id_expr, &Value)`, `with_person(ctx, out, &Value)`, `named_set(out, engine, table, members, member_col, row_expr, &Value)`, and the seven arms in `emit_one`.

- [ ] **Step 1: Write the failing tests**

Append to `tests.rs`:

```rust
mod people_words {
    use super::*;

    #[tokio::test]
    async fn from_to_and_with() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Spec case 4.
        assert_eq!(
            run(&mut conn, ListKind::Messages, r#"from:me to:"Jane Doe" (avocado or "guacamole night")"#).await,
            sorted(vec![f.jane_avocado_from_me, f.jane_guac_from_me])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "from:jane").await,
            sorted(vec![f.jane_avocado_to_me, f.jane_2018, f.feb_big_jpeg, f.feb_small_jpeg, f.feb_pdf, f.may_big_jpeg])
        );
        assert_eq!(run(&mut conn, ListKind::Messages, "from:me").await.len(), 4);
        assert_eq!(run(&mut conn, ListKind::Messages, "to:me").await.len(), 10);
        assert_eq!(run(&mut conn, ListKind::Messages, "from:gmail.com").await.len(), 6);
        assert_eq!(run(&mut conn, ListKind::Conversations, &format!("with:#{}", f.jane)).await, sorted(vec![f.jane_direct, f.big_group]));
        assert_eq!(run(&mut conn, ListKind::Conversations, "with:sam").await, sorted(vec![f.sam_direct, f.archive_group, f.big_group]));
        assert_eq!(run(&mut conn, ListKind::Messages, "with:bo body:old").await, vec![f.archive_msg]);
    }

    #[tokio::test]
    async fn in_one_conversation() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Messages, &format!("in:#{}", f.jane_direct)).await.len(), 8);
        assert_eq!(run(&mut conn, ListKind::Messages, "in:club").await, vec![f.big_group_msg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "in:+15550002").await, vec![f.bo_2023]);
    }

    #[tokio::test]
    async fn contact_groups_on_every_list() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Contacts, "group:Family").await, vec![f.ana]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "group:family").await, vec![f.ana]);
        assert_eq!(run(&mut conn, ListKind::Contacts, &format!("group:#{}", f.family)).await, vec![f.ana]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "group:none").await, sorted(vec![f.bo, f.cy, f.jane, f.sam, f.nameless]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "group:unknown").await, vec![f.nameless]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "group:Family").await, sorted(vec![f.ana_direct, f.archive_group, f.big_group]));
        assert_eq!(run(&mut conn, ListKind::Conversations, "-group:Family").await, sorted(vec![f.bo_direct, f.jane_direct, f.sam_direct]));
        assert_eq!(run(&mut conn, ListKind::Messages, "group:Family body:hello").await, vec![f.ana_2018]);
    }

    #[tokio::test]
    async fn message_tags_on_every_list() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Conversations, "tag:Archive").await, vec![f.archive_group]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "tag:none").await.len(), 5);
        assert_eq!(run(&mut conn, ListKind::Contacts, "tag:Archive").await, sorted(vec![f.ana, f.bo, f.sam]));
        assert_eq!(run(&mut conn, ListKind::Messages, &format!("tag:#{}", f.archive)).await, vec![f.archive_msg]);
    }

    #[tokio::test]
    async fn import_runs() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO vault_imports (account_id, source, mode, status, started_at)
             VALUES ($1, 'imessage', 'push', 'completed', '2024-01-01T00:00:00Z') RETURNING id",
        )
        .bind(ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("UPDATE messages SET import_id = $1 WHERE id = $2")
            .bind(run_id)
            .bind(f.bo_2023)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(run(&mut conn, ListKind::Messages, "import:last").await, vec![f.bo_2023]);
        assert_eq!(run(&mut conn, ListKind::Messages, &format!("import:#{run_id}")).await, vec![f.bo_2023]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "import:last").await, vec![f.bo_direct]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::tests::people_words`
Expected: 5 failed with "is not built yet".

- [ ] **Step 3: Write the emitters**

Add to `emit.rs`:

```rust
use crate::db::contacts::UNKNOWN_CONTACT_SQL;
use crate::db::dialect::name_eq_ci;

/// The handle with id `handle_id_expr` belongs to the person `v`: by contact
/// id, or by contains-match on the handle, the contact's name, or the alias.
fn person_matches(out: &mut Sql, engine: DbEngine, handle_id_expr: &str, v: &Value) {
    match v {
        Value::Id(id) => {
            out.push(&format!(
                "EXISTS (SELECT 1 FROM contact_handles chp WHERE chp.handle_id = {handle_id_expr} AND chp.contact_id = "
            ));
            out.bind_int(*id);
            out.push(")");
        }
        Value::Text(t) | Value::Prefix(t) => {
            let pat = if matches!(v, Value::Prefix(_)) { format!("{t}%") } else { format!("%{t}%") };
            out.push(&format!(
                "EXISTS (SELECT 1 FROM handles hp LEFT JOIN contact_handles chp ON chp.handle_id = hp.id AND chp.account_id = hp.account_id LEFT JOIN contacts ctp ON ctp.id = chp.contact_id WHERE hp.id = {handle_id_expr} AND ("
            ));
            out.like(engine, "hp.raw", &pat);
            out.push(" OR ");
            out.like(engine, "coalesce(hp.normalized, '')", &pat);
            out.push(" OR ");
            out.like(engine, "coalesce(ctp.preferred_name, '')", &pat);
            out.push(" OR ");
            out.like(engine, "coalesce(chp.name_alias, '')", &pat);
            out.push("))");
        }
        _ => out.push("1=0"),
    }
}

/// Some party to conversation `c` (chat handle or participant) is `v`.
fn with_person(ctx: &ListCtx<'_>, out: &mut Sql, v: &Value) {
    let e = ctx.engine;
    ctx.conversation(out, |o| {
        o.push("(");
        person_matches(o, e, "c.chat_handle_id", v);
        o.push(" OR EXISTS (SELECT 1 FROM participants p WHERE p.conversation_id = c.id AND ");
        person_matches(o, e, "p.handle_id", v);
        o.push("))");
    });
}

/// `row_expr` is a member of the named set `v` in `table` (via `members`,
/// whose `member_col` names the row). Handles `#id` and a case-insensitive name.
fn named_set(
    out: &mut Sql,
    engine: DbEngine,
    table: &str,
    members: &str,
    member_col: &str,
    row_expr: &str,
    account_expr: &str,
    v: &Value,
) {
    out.push(&format!(
        "EXISTS (SELECT 1 FROM {members} nm JOIN {table} ns ON ns.id = nm.{} WHERE nm.{member_col} = {row_expr} AND ns.account_id = {account_expr} AND ",
        if table == "contact_groups" { "group_id" } else { "tag_id" }
    ));
    match v {
        Value::Id(id) => {
            out.push("ns.id = ");
            out.bind_int(*id);
        }
        Value::Text(t) | Value::Prefix(t) => {
            out.push(&name_eq_ci(engine, "ns.name", "?"));
            out.param_text(t.clone());
        }
        _ => out.push("1=0"),
    }
    out.push(")");
}

fn no_named_set(out: &mut Sql, table: &str, members: &str, member_col: &str, row_expr: &str, account_expr: &str) {
    out.push(&format!(
        "NOT EXISTS (SELECT 1 FROM {members} nm JOIN {table} ns ON ns.id = nm.{} WHERE nm.{member_col} = {row_expr} AND ns.account_id = {account_expr})",
        if table == "contact_groups" { "group_id" } else { "tag_id" }
    ));
}

fn emit_people_word(ctx: &ListCtx<'_>, out: &mut Sql, word: &str, v: &Value) {
    let e = ctx.engine;
    match word {
        "with" => with_person(ctx, out, v),
        "from" => match v {
            Value::Keyword("me") => out.push("m.is_from_me = 1"),
            _ => {
                out.push("(m.is_from_me = 0 AND m.sender_handle_id IS NOT NULL AND ");
                person_matches(out, e, "m.sender_handle_id", v);
                out.push(")");
            }
        },
        "to" => match v {
            Value::Keyword("me") => out.push("m.is_from_me = 0"),
            _ => {
                out.push("(");
                with_person(ctx, out, v);
                out.push(" AND (m.is_from_me = 1 OR m.sender_handle_id IS NULL OR NOT ");
                person_matches(out, e, "m.sender_handle_id", v);
                out.push("))");
            }
        },
        "in" => match v {
            Value::Id(id) => {
                out.push("m.conversation_id = ");
                out.bind_int(*id);
            }
            Value::Text(t) | Value::Prefix(t) => {
                let pat = if matches!(v, Value::Prefix(_)) { format!("{t}%") } else { format!("%{t}%") };
                ctx.conversation(out, |o| {
                    o.push("(");
                    o.like(e, "coalesce(c.group_title, '')", &pat);
                    o.push(" OR EXISTS (SELECT 1 FROM handles hc WHERE hc.id = c.chat_handle_id AND ");
                    o.like(e, "hc.raw", &pat);
                    o.push("))");
                });
            }
            _ => out.push("1=0"),
        },
        "group" => match v {
            Value::Keyword("none") => match ctx.list {
                ListKind::Contacts => no_named_set(out, "contact_groups", "contact_group_members", "contact_id", "ct.id", "ct.account_id"),
                _ => {
                    out.push("NOT ");
                    ctx.contact(out, |o| {
                        o.push("NOT ");
                        no_named_set(o, "contact_groups", "contact_group_members", "contact_id", "ct.id", "ct.account_id");
                    });
                }
            },
            Value::Keyword("unknown") => ctx.contact(out, |o| o.push(UNKNOWN_CONTACT_SQL)),
            _ => ctx.contact(out, |o| {
                named_set(o, e, "contact_groups", "contact_group_members", "contact_id", "ct.id", "ct.account_id", v);
            }),
        },
        "tag" => match v {
            Value::Keyword("none") => ctx.conversation(out, |o| {
                no_named_set(o, "message_tags", "message_tag_members", "conversation_id", "c.id", "c.account_id");
            }),
            _ => ctx.conversation(out, |o| {
                named_set(o, e, "message_tags", "message_tag_members", "conversation_id", "c.id", "c.account_id", v);
            }),
        },
        "import" => match v {
            Value::Keyword("last") => {
                let account = ctx.account_id.to_string();
                ctx.message(out, |o| {
                    o.push("m.import_id = (SELECT MAX(vi.id) FROM vault_imports vi WHERE vi.account_id = ");
                    o.bind_text(account);
                    o.push(")");
                });
            }
            Value::Id(id) => ctx.message(out, |o| {
                o.push("m.import_id = ");
                o.bind_int(*id);
            }),
            _ => out.push("1=0"),
        },
        _ => out.push("1=0"),
    }
}
```

Add the arm to `emit_one`:

```rust
        "with" | "from" | "to" | "in" | "group" | "tag" | "import" => {
            emit_people_word(ctx, out, term.spec.word, v);
            Ok(())
        }
```

`group:none` on Conversations and Messages reads "no participant contact is in any Contact Group": the double negation above wraps the bridge's EXISTS so that it becomes "there is no linked contact with a group".

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::tests`
Expected: all pass (13). If `to:me` counts 11 rather than 10, the trashed conversation's message leaked: check the Messages default in `compile`.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "feat(search): people, places, groups, tags, and import runs

with:, from:, and to: name a person by contact id, name, handle, or
alias, and me is the account's own side of a message. in: is one
conversation by id, title, or handle. group: and tag: take a name or
#id, with none and the computed unknown group; on Conversations and
Messages a group is reached through the participants. import: takes
#id or last."
```

---

### Task 7: Kinds and attachments: `kind:`, `service:`, `source:`, `attachment:`, `size:`, `trashed:`

**Files:**
- Modify: `crates/vault/server/src/search/emit.rs`
- Modify: `crates/vault/server/src/search/tests.rs` (add `mod kind_words`)

**Interfaces:**
- Produces in `emit.rs`: `cmp_sql(out, expr, &Cmp<i64>)`, `attachment_kind_sql(kind) -> String`, `emit_kind_word`, and the six arms.

- [ ] **Step 1: Write the failing tests**

```rust
mod kind_words {
    use super::*;

    #[tokio::test]
    async fn kind_service_and_source() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Conversations, "kind:direct").await, sorted(vec![f.ana_direct, f.bo_direct, f.jane_direct, f.sam_direct]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "kind:group").await, sorted(vec![f.ana, f.bo, f.jane, f.sam]));
        assert_eq!(run(&mut conn, ListKind::Messages, "service:sms").await, vec![f.bo_2023]);
        assert_eq!(run(&mut conn, ListKind::Messages, "service:sms,whatsapp").await, sorted(vec![f.bo_2023, f.archive_msg]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "service:sms").await, vec![f.bo]);
        assert_eq!(run(&mut conn, ListKind::Messages, "source:whatsapp").await, vec![f.archive_msg]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "source:imessage").await.len(), 5);
    }

    #[tokio::test]
    async fn attachments_by_kind_and_size() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Messages, "attachment:image").await, sorted(vec![f.feb_big_jpeg, f.feb_small_jpeg, f.may_big_jpeg]));
        assert_eq!(run(&mut conn, ListKind::Messages, "attachment:pdf").await, vec![f.feb_pdf]);
        assert_eq!(run(&mut conn, ListKind::Messages, "attachment:document").await, vec![f.feb_pdf]);
        assert_eq!(run(&mut conn, ListKind::Messages, "attachment:video").await, Vec::<i64>::new());
        assert_eq!(run(&mut conn, ListKind::Messages, "attachment:any").await.len(), 4);
        assert_eq!(run(&mut conn, ListKind::Messages, "attachment:none").await.len(), 10);
        assert_eq!(run(&mut conn, ListKind::Conversations, "attachment:image").await, vec![f.jane_direct]);
        assert_eq!(run(&mut conn, ListKind::Messages, "size:>500k").await, sorted(vec![f.feb_big_jpeg, f.feb_pdf, f.may_big_jpeg]));
        assert_eq!(run(&mut conn, ListKind::Messages, "size:<500k").await, vec![f.feb_small_jpeg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "size:100k..2M").await.len(), 4);
    }

    #[tokio::test]
    async fn trash_is_a_word() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(run(&mut conn, ListKind::Conversations, "trashed:yes").await, vec![f.trashed_conv]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "trashed:no").await.len(), 6);
        assert_eq!(run(&mut conn, ListKind::Conversations, "trashed:any").await.len(), 7);
        assert_eq!(run(&mut conn, ListKind::Conversations, "trashed:yes gone").await, vec![f.trashed_conv]);
        sqlx::query("INSERT INTO trashed_contacts (account_id, contact_id) VALUES ($1, $2)")
            .bind(ACCOUNT)
            .bind(f.cy)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(run(&mut conn, ListKind::Contacts, "trashed:yes").await, vec![f.cy]);
        assert!(!run(&mut conn, ListKind::Contacts, "").await.contains(&f.cy));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::tests::kind_words`
Expected: 3 failed.

- [ ] **Step 3: Write the emitters**

Add to `emit.rs`:

```rust
use super::value::Cmp;

/// `expr <op> ?` for a count or size comparison; a range is two bounds.
fn cmp_sql(out: &mut Sql, expr: &str, cmp: &Cmp<i64>) {
    let (op, v) = match cmp {
        Cmp::Eq(v) => ("=", *v),
        Cmp::Gt(v) => (">", *v),
        Cmp::Gte(v) => (">=", *v),
        Cmp::Lt(v) => ("<", *v),
        Cmp::Lte(v) => ("<=", *v),
        Cmp::Range(a, b) => {
            out.push(&format!("({expr} >= "));
            out.bind_int(*a);
            out.push(&format!(" AND {expr} <= "));
            out.bind_int(*b);
            out.push(")");
            return;
        }
    };
    out.push(&format!("{expr} {op} "));
    out.bind_int(v);
}

/// Attachment `a` is of this kind, by MIME type.
fn attachment_kind_sql(kind: &str) -> String {
    const MIME: &str = "lower(coalesce(a.mime_type, ''))";
    let image = format!("{MIME} LIKE 'image/%'");
    let video = format!("{MIME} LIKE 'video/%'");
    let audio = format!("{MIME} LIKE 'audio/%'");
    let pdf = format!("{MIME} = 'application/pdf'");
    let contact = format!("{MIME} IN ('text/vcard', 'text/x-vcard')");
    let document = format!(
        "({pdf} OR {MIME} LIKE 'text/%' OR {MIME} LIKE 'application/vnd.%' OR {MIME} IN ('application/msword', 'application/rtf'))"
    );
    match kind {
        "image" => image,
        "video" => video,
        "audio" => audio,
        "pdf" => pdf,
        "contact" => contact,
        "document" => format!("({document} AND NOT {contact})"),
        _ => format!("NOT ({image} OR {video} OR {audio} OR {document} OR {contact})"),
    }
}

/// The `source` word's values, mapped to what the importers write.
fn source_id(choice: &str) -> &'static str {
    match choice {
        "imessage" => "imessage",
        "whatsapp" => "whatsapp",
        _ => "sms-backup-restore",
    }
}

fn emit_kind_word(ctx: &ListCtx<'_>, out: &mut Sql, word: &str, v: &Value) {
    match (word, v) {
        ("kind", Value::Choice(k)) => {
            let ty = if *k == "direct" { "individual" } else { "group" };
            ctx.conversation(out, |o| o.push(&format!("c.conversation_type = '{ty}'")));
        }
        ("service", Value::Choice(s)) => {
            let s = s.to_string();
            ctx.message(out, |o| {
                o.push("lower(coalesce(m.service, '')) = ");
                o.bind_text(s);
            });
        }
        ("source", Value::Choice(s)) => {
            let id = source_id(s);
            ctx.message(out, |o| {
                o.push("m.source = ");
                o.bind_text(id);
            });
        }
        ("attachment", Value::Choice("any")) => ctx.message(out, |o| o.push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)")),
        ("attachment", Value::Choice("none")) => ctx.message(out, |o| o.push("NOT EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)")),
        ("attachment", Value::Choice(k)) => {
            let pred = attachment_kind_sql(k);
            ctx.message(out, |o| o.push(&format!("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id AND {pred})")));
        }
        ("size", Value::Size(cmp)) => ctx.message(out, |o| {
            o.push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id AND a.size_bytes IS NOT NULL AND ");
            cmp_sql(o, "a.size_bytes", cmp);
            o.push(")");
        }),
        ("trashed", Value::Choice(flag)) => {
            let exists = match ctx.list {
                ListKind::Contacts => "EXISTS (SELECT 1 FROM trashed_contacts tct WHERE tct.account_id = ct.account_id AND tct.contact_id = ct.id)",
                _ => "(EXISTS (SELECT 1 FROM trashed_conversations tc WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id) OR EXISTS (SELECT 1 FROM trashed_handles th WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id))",
            };
            match *flag {
                "yes" => out.push(exists),
                "no" => out.push(&format!("NOT {exists}")),
                _ => out.push("1=1"),
            }
        }
        _ => out.push("1=0"),
    }
}
```

Add the arm to `emit_one`:

```rust
        "kind" | "service" | "source" | "attachment" | "size" | "trashed" => {
            emit_kind_word(ctx, out, term.spec.word, v);
            Ok(())
        }
```

Before committing, confirm the source id for SMS Backup & Restore with `grep -rn "sms-backup-restore" crates/exporters crates/libs | head`; if the exporter writes a different id, `source_id` uses that one.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::tests`
Expected: all pass (16).

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "feat(search): kind, service, source, attachment, size, and trashed

kind: is direct or group. service: is the transport on the message and
source: the backup family it came from. attachment: names a category by
MIME type, with any and none; size: compares an attachment's bytes.
trashed: is a word now, since it narrows rows: yes, no, or any."
```

---

### Task 8: Dates and counts: `date:`, `first-message:`, `last-message:`, `messages:`, `conversations:`, `groups:`, `participants:`, `attachments:`

**Files:**
- Modify: `crates/vault/server/src/search/emit.rs`
- Modify: `crates/vault/server/src/search/tests.rs` (add `mod measure_words`)

**Interfaces:**
- Produces in `emit.rs`: `date_sql(out, expr, &DateCmp)`, `emit_measure_word`, and the eight arms.

- [ ] **Step 1: Write the failing tests**

```rust
mod measure_words {
    use super::*;

    #[tokio::test]
    async fn dates_on_every_list() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Spec case 2.
        assert_eq!(run(&mut conn, ListKind::Messages, "date:2024-01..2024-03 attachment:image size:>500k").await, vec![f.feb_big_jpeg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "date:2018").await, sorted(vec![f.ana_2018, f.jane_2018]));
        assert_eq!(run(&mut conn, ListKind::Messages, "date:>=2024-05").await, vec![f.may_big_jpeg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "date:<2019").await.len(), 2);
        assert_eq!(run(&mut conn, ListKind::Messages, "date:2024-02-12").await, vec![f.jane_avocado_to_me]);
        assert_eq!(run(&mut conn, ListKind::Contacts, "date:2023").await, vec![f.bo]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "date:2019").await, vec![f.archive_group]);
        // A relative span resolves against the request's today, 2026-09-02.
        assert_eq!(run(&mut conn, ListKind::Messages, "date:1y").await, Vec::<i64>::new());
        assert_eq!(run(&mut conn, ListKind::Messages, "date:<1y").await.len(), 14);
    }

    #[tokio::test]
    async fn first_and_last_message() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Spec case 5.
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "first-message:<2020 last-message:>=2024-01-01 handle:@gmail.com").await,
            vec![f.jane]
        );
        assert_eq!(run(&mut conn, ListKind::Contacts, "first-message:<2019").await, sorted(vec![f.ana, f.jane]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "last-message:<2024-03").await, Vec::<i64>::new());
        assert_eq!(run(&mut conn, ListKind::Conversations, "last-message:<2022").await, sorted(vec![f.ana_direct, f.archive_group]));
        assert_eq!(run(&mut conn, ListKind::Messages, "first-message:2018 body:hi").await, vec![f.ana_2021]);
    }

    #[tokio::test]
    async fn counts() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Spec case 1: Ana is in Family, Cy has no messages.
        assert_eq!(run(&mut conn, ListKind::Contacts, "group:none messages:>0").await, sorted(vec![f.bo, f.jane, f.sam]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "messages:0").await, sorted(vec![f.cy, f.nameless]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "conversations:0").await, sorted(vec![f.cy, f.nameless]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "conversations:>=3").await, sorted(vec![f.ana, f.bo, f.sam]));
        assert_eq!(run(&mut conn, ListKind::Contacts, "groups:>0").await, vec![f.ana]);
        assert_eq!(run(&mut conn, ListKind::Conversations, "messages:>=2").await, sorted(vec![f.ana_direct, f.jane_direct]));
        // Spec case 3.
        assert_eq!(run(&mut conn, ListKind::Conversations, "participants:>2 -tag:Archive").await, vec![f.big_group]);
        assert_eq!(run(&mut conn, ListKind::Messages, "participants:>3").await, vec![f.big_group_msg]);
        assert_eq!(run(&mut conn, ListKind::Messages, "attachments:>0").await.len(), 4);
        assert_eq!(run(&mut conn, ListKind::Messages, "attachments:0 date:2024-02").await.len(), 4);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server search::tests::measure_words`
Expected: 3 failed.

- [ ] **Step 3: Write the emitters**

Add to `emit.rs`:

```rust
use super::value::{DateCmp, ymd};

/// `expr` (an RFC 3339 text timestamp) falls where `cmp` says. Text
/// comparison against `YYYY-MM-DD` works because the date is a prefix of the
/// timestamp.
fn date_sql(out: &mut Sql, expr: &str, cmp: &DateCmp) {
    match cmp {
        DateCmp::In(span) => {
            out.push(&format!("({expr} >= "));
            out.bind_text(ymd(span.start));
            out.push(&format!(" AND {expr} < "));
            out.bind_text(ymd(span.end));
            out.push(")");
        }
        DateCmp::Gte(d) | DateCmp::Gt(d) => {
            out.push(&format!("{expr} >= "));
            out.bind_text(ymd(*d));
        }
        DateCmp::Lt(d) | DateCmp::Lte(d) => {
            out.push(&format!("{expr} < "));
            out.bind_text(ymd(*d));
        }
    }
}

fn emit_measure_word(ctx: &ListCtx<'_>, out: &mut Sql, word: &str, v: &Value) {
    match (word, v) {
        ("date", Value::Date(cmp)) => ctx.message(out, |o| date_sql(o, "m.timestamp", cmp)),
        ("first-message", Value::Date(cmp)) => {
            let expr = format!("(SELECT MIN(m2.timestamp) FROM messages m2 WHERE {})", ctx.messages_link("m2"));
            date_sql(out, &expr, cmp);
        }
        ("last-message", Value::Date(cmp)) => {
            let expr = format!("(SELECT MAX(m2.timestamp) FROM messages m2 WHERE {})", ctx.messages_link("m2"));
            date_sql(out, &expr, cmp);
        }
        ("messages", Value::Count(cmp)) => {
            let expr = format!("(SELECT COUNT(*) FROM messages m2 WHERE {})", ctx.messages_link("m2"));
            cmp_sql(out, &expr, cmp);
        }
        ("conversations", Value::Count(cmp)) => {
            let expr = format!("(SELECT COUNT(*) FROM conversations c2 WHERE {})", ctx.conversations_link("c2"));
            cmp_sql(out, &expr, cmp);
        }
        ("groups", Value::Count(cmp)) => cmp_sql(
            out,
            "(SELECT COUNT(*) FROM contact_group_members cgm JOIN contact_groups cg ON cg.id = cgm.group_id WHERE cgm.contact_id = ct.id AND cg.account_id = ct.account_id)",
            cmp,
        ),
        ("participants", Value::Count(cmp)) => ctx.conversation(out, |o| {
            cmp_sql(o, "(SELECT COUNT(*) FROM participants p WHERE p.conversation_id = c.id)", cmp);
        }),
        ("attachments", Value::Count(cmp)) => cmp_sql(out, "(SELECT COUNT(*) FROM attachments a WHERE a.message_id = m.id)", cmp),
        _ => out.push("1=0"),
    }
}
```

Add the arm to `emit_one`, and make the fallback unreachable now that every word has an arm:

```rust
        "date" | "first-message" | "last-message" | "messages" | "conversations" | "groups"
        | "participants" | "attachments" => {
            emit_measure_word(ctx, out, term.spec.word, v);
            Ok(())
        }
        other => Err(QueryError::new(
            QueryErrorKind::BadValue,
            term.span.clone(),
            format!("{other}: has no emitter; add one in emit.rs"),
        )),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server search::tests`
Expected: all pass (19). The `date:<1y` count of 14 is every non-trashed message of the account; if it is off by one, check the `Lt` bound resolves to the span's start, which for `1y` is 2025-09-02.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "feat(search): dates and counts

date: is when a message was sent, and on Contacts and Conversations it
means having a message then. first-message: and last-message: compare
the earliest or latest message through a correlated MIN or MAX, so no
contact-id list is built first. The five plural words are counts."
```

---

### Task 9: Every word compiles on every list it claims

**Files:**
- Modify: `crates/vault/server/src/search/tests.rs` (add `mod coverage` and `mod refusals`)

**Interfaces:**
- Consumes: `fields::FIELDS`, `fields::ValueType`.

- [ ] **Step 1: Write the coverage and refusal tests**

```rust
mod coverage {
    use super::*;
    use crate::search::fields::{FIELDS, ValueType};

    /// One representative value per shape, plus every keyword a word lists.
    fn sample_values(vt: ValueType, keywords: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = keywords.iter().map(|k| k.to_string()).collect();
        match vt {
            ValueType::Text => out.extend(["x".into(), "pre*".into(), "\"two words\"".into()]),
            ValueType::Name | ValueType::Person => out.extend(["x".into(), "#7".into(), "\"Two Words\"".into()]),
            ValueType::Date => out.extend(["2019".into(), ">=2024-05".into(), "<7d".into(), "2019..2021".into()]),
            ValueType::Count => out.extend(["0".into(), ">1".into(), "1..3".into()]),
            ValueType::Size => out.extend(["1M".into(), "<500k".into(), "100k..2M".into()]),
            ValueType::Choice | ValueType::Flag => {}
        }
        out
    }

    #[tokio::test]
    async fn every_word_compiles_and_runs_on_every_list_it_claims() {
        let (pool, _dir, _f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        for spec in FIELDS {
            for list in spec.lists {
                for value in sample_values(spec.value_type, spec.values) {
                    for q in [format!("{}:{value}", spec.word), format!("-{}:{value}", spec.word), format!("{}:{value} or x", spec.word)] {
                        run(&mut conn, *list, &q).await;
                    }
                }
            }
        }
    }

    #[test]
    fn every_word_compiles_for_postgres_too() {
        for spec in FIELDS {
            for list in spec.lists {
                for value in sample_values(spec.value_type, spec.values) {
                    let q = format!("{}:{value}", spec.word);
                    let f = compile(CompileRequest { list: *list, query: &q, account_id: ACCOUNT, engine: DbEngine::Postgres, today: today() })
                        .unwrap_or_else(|e| panic!("{q} on {list:?}: {}", e.message));
                    assert_eq!(f.where_sql().matches('?').count(), f.params().len(), "{q} on {list:?}");
                    assert!(!f.where_sql().contains("COLLATE NOCASE"), "{q}: SQLite collation leaked into Postgres SQL");
                }
            }
        }
    }

    #[test]
    fn every_word_is_described_on_every_list_it_claims() {
        for spec in FIELDS {
            for list in spec.lists {
                let docs = crate::search::describe(*list);
                let doc = docs.iter().find(|d| d.word == spec.word).unwrap_or_else(|| panic!("{} missing from describe({list:?})", spec.word));
                assert_eq!(doc.lists, spec.lists.to_vec());
                assert!(!doc.help.is_empty() && !doc.example.is_empty());
            }
        }
    }
}

mod refusals {
    use super::*;
    use crate::search::QueryErrorKind;

    #[test]
    fn a_refusal_never_queries_and_names_the_word() {
        let e = err(ListKind::Contacts, "from:me");
        assert_eq!(e.kind, QueryErrorKind::WrongList);
        assert_eq!(e.span, 0..7);
        assert_eq!(e.field, Some("from"));
        let e = err(ListKind::Messages, "people:Family");
        assert_eq!(e.kind, QueryErrorKind::UnknownWord);
        assert_eq!(e.did_you_mean, None);
        let e = err(ListKind::Conversations, "paticipants:>2");
        assert_eq!(e.did_you_mean, Some("participants"));
        assert_eq!(err(ListKind::Messages, "tag:").kind, QueryErrorKind::EmptyValue);
        assert_eq!(err(ListKind::Messages, "(a or b").kind, QueryErrorKind::Unbalanced);
        assert_eq!(err(ListKind::Messages, "date:2019-13").kind, QueryErrorKind::BadValue);
        assert_eq!(err(ListKind::Messages, &"a ".repeat(40)).kind, QueryErrorKind::TooComplex);
        assert_eq!(err(ListKind::Messages, &"x".repeat(3000)).kind, QueryErrorKind::TooLong);
    }

    #[test]
    fn refusals_carry_no_memory_of_other_spellings() {
        // The module must not know these words ever existed. Each is simply unknown.
        for old in ["before:2020", "after:2020", "has:attachment", "is:direct", "larger:1M", "within:Family", "label:x", "text:hi", "filetype:image"] {
            let e = err(ListKind::Messages, old);
            assert_eq!(e.kind, QueryErrorKind::UnknownWord, "{old}");
            let word = old.split(':').next().unwrap();
            assert_eq!(e.message.trim_end_matches(|c: char| c != '.'), format!("{word}: is not a search word."), "{old}");
        }
    }
}
```

The last assertion trims anything after the first sentence, so a "Did you mean" suffix is allowed where the current word list happens to have a near word (`is:` is one edit from `in:`, `has:` two from `tag:`). What is asserted is that the first sentence is the plain unknown-word sentence and nothing more.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p message-vault-server search::`
Expected: all pass. The coverage test is the one that proves every arm exists and every fragment is valid SQL on SQLite for every list and value shape.

- [ ] **Step 3: Commit**

```bash
git add crates/vault/server/src/search
git commit -m "test(search): prove every word runs on every list it claims

One test compiles each word with each value shape and every keyword it
lists, negated and inside an or, on each list the registry says it
works on, and runs the SQL against a seeded vault. Another compiles
the same set for Postgres and checks the placeholders match the binds.
A third checks that spellings the language does not have are plain
unknown words with no memory attached."
```

---

### Task 10: The Contacts list calls the module

**Files:**
- Modify: `crates/vault/server/src/contacts_api.rs` (`list_contacts`, `contacts_list_handler`, the `use` block, and the parser and filter code between `involves_contact_expr` and `list_contacts`; the tests module)

**Interfaces:**
- Consumes: `search::{compile, CompileRequest, ListKind}`, `ApiError`.
- Produces: `pub async fn list_contacts(conn, account_id: &str, q: &str, limit: usize, offset: usize, today: NaiveDate) -> Result<ContactListPage, ApiError>`. The page shape does not change.
- Deletes from `contacts_api.rs`: `involves_ct_sql`, `contact_has_messages_sql`, `DateBoundOp`, `DateBound`, `date_bound_cmp`, `involved_message_date_agg`, `push_contact_date_bounds`, `normalize_ymd`, `parse_date_bound_value`, `expand_service_token`, `take_prefixed_quoted_or_bare`, `apply_group_token`, `parse_contact_list_filters`, `ContactListFilters`, `parse_contact_list_query`, and the imports of `ExportQueryError` and `extract_keyed_ops`. `involves_contact_expr`, `involves_contact_sql`, and the three `NOT_TRASHED_*` constants stay until Task 12 removes their last callers.

- [ ] **Step 1: Write the failing HTTP tests**

In the `tests` module at the bottom of `contacts_api.rs`, add:

```rust
    #[tokio::test]
    async fn contact_list_takes_the_search_language() {
        let (vault, token, account) = contacts_fixture_with_handles(&["+15550100", "+15550101"]).await;
        {
            let mut conn = vault.state.db.acquire().await.unwrap();
            let group_id: i64 = sqlx::query_scalar(
                "INSERT INTO contact_groups (account_id, name) VALUES ($1, 'Family') RETURNING id",
            )
            .bind(&account.account_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            let first: i64 = sqlx::query_scalar("SELECT MIN(id) FROM contacts WHERE account_id = $1")
                .bind(&account.account_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            sqlx::query("INSERT INTO contact_group_members (contact_id, group_id) VALUES ($1, $2)")
                .bind(first)
                .bind(group_id)
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        let page: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts?q=group:Family", &token).await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["contacts"][0]["name"], "Contact 0");
        let page: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts?q=group:none", &token).await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["contacts"][0]["name"], "Contact 1");
    }

    #[tokio::test]
    async fn contact_list_refuses_a_word_from_another_list() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let status = crate::test_support::get_status(&vault.state, "/v1/contacts?q=from:me", &token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server contacts_api::tests::contact_list_takes_the_search_language`
Expected: FAIL. `group:Family` still works by luck of the old spelling, but `from:me` is treated as words today and answers 200.

- [ ] **Step 3: Rewrite `list_contacts`**

Replace the whole function with:

```rust
/// One page of the contact list for `q`, a query in the search language.
///
/// # Errors
///
/// `BadRequest` for a query the language refuses or an offset past the cap;
/// `Internal` when a statement fails.
pub async fn list_contacts(
    conn: &mut AnyConnection,
    account_id: &str,
    q: &str,
    limit: usize,
    offset: usize,
    today: chrono::NaiveDate,
) -> Result<ContactListPage, ApiError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    if offset > MAX_LIST_OFFSET {
        return Err(ApiError::BadRequest(format!(
            "offset exceeds maximum of {MAX_LIST_OFFSET}"
        )));
    }
    let engine = engine_of(conn);
    let filter = crate::search::compile(crate::search::CompileRequest {
        list: crate::search::ListKind::Contacts,
        query: q,
        account_id,
        engine,
        today,
    })?;
    let where_sql = filter.where_sql();

    let count_sql = renumber_placeholders(&format!(
        "SELECT COUNT(*) FROM contacts ct WHERE {where_sql}"
    ));
    let total: i64 = sqlx::query_scalar_with(&count_sql, bind_args(filter.params()))
        .fetch_one(&mut *conn)
        .await?;
    let total = total.max(0) as u64;

    let order_by = format!("{}, ct.id", order_by_name_ci(engine, "name"));
    let sql = renumber_placeholders(&format!(
        "SELECT ct.id,
                COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)') AS name,
                (SELECT COUNT(*)
                 FROM contact_handles ch
                 WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id) AS handle_count,
                (SELECT {handles_agg}
                 FROM (
                   SELECT DISTINCT h.normalized AS val
                   FROM contact_handles ch
                   JOIN handles h ON h.id = ch.handle_id
                   WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                     AND h.normalized IS NOT NULL AND trim(h.normalized) != ''
                   UNION
                   SELECT DISTINCT h.raw AS val
                   FROM contact_handles ch
                   JOIN handles h ON h.id = ch.handle_id
                   WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                     AND h.raw IS NOT NULL AND trim(h.raw) != ''
                 )) AS handles,
                ct.last_modified,
                (SELECT {groups_agg}
                 FROM contact_group_members clm
                 JOIN contact_groups cl ON cl.id = clm.group_id
                 WHERE clm.contact_id = ct.id AND cl.account_id = ct.account_id) AS groups
         FROM contacts ct
         WHERE {where_sql}
         {order_by}
         LIMIT ? OFFSET ?",
        handles_agg = group_concat_unit_separator(engine, "val"),
        groups_agg = group_concat_unit_separator(engine, "cl.name"),
    ));
    let mut params = filter.params().to_vec();
    params.push(SqlParam::Int(limit as i64));
    params.push(SqlParam::Int(offset as i64));
    let rows: Vec<ContactRow> = sqlx::query_as_with(&sql, bind_args(&params))
        .fetch_all(&mut *conn)
        .await?;
    // The row-to-ContactSummary mapping below this line is unchanged.
```

Keep the existing mapping from `rows` to `ContactListPage` exactly as it is. In `contacts_list_handler` change the call to:

```rust
    let page = list_contacts(
        &mut conn,
        &auth.account_id,
        &q,
        limit,
        offset,
        chrono::Local::now().date_naive(),
    )
    .await?;
```

Delete the functions and types listed in the Interfaces block, and the `use crate::export_api::ExportQueryError;` and `use crate::search_query::extract_keyed_ops;` lines. Any other function in this file that returned `Result<_, ExportQueryError>` keeps doing so until Task 12; `list_contacts` is the only one that changes now, and the handler's `?` already converts either error.

- [ ] **Step 4: Fix the module's tests**

Run: `cargo test -p message-vault-server contacts_api`

For every failing test in this file, apply one of three rules, in this order:
1. It tests a function deleted in Step 3 (a parser or a date-bound helper): delete the test.
2. It calls `list_contacts` with an old spelling: change the spelling to the spec's word (`has:no-name` becomes `name:none`, `group:none` stays, `first-contact:>=2019` becomes `first-message:>=2019`, `has:messages` becomes `messages:>0`, `service:whatsapp` stays), and add the `today` argument as `crate::search::tests::today()`.
3. It asserts a behaviour the spec changed on purpose (an unknown token searched as words): rewrite it to assert the 400, or delete it if a search-module test already covers the case.

Expected after the fixes: `cargo test -p message-vault-server contacts_api` passes, including the two new HTTP tests.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/contacts_api.rs
git commit -m "refactor(contacts): the contact list compiles its query with the search module

list_contacts no longer parses the query or builds its own filter SQL.
It asks the search module for a fragment over the contacts alias and
appends its own SELECT, ORDER BY, and LIMIT. The private parser, the
date-bound helpers, and their tests are gone. A word the list does not
know is now a 400 instead of being searched as text."
```

---

### Task 11: The Conversations list calls the module

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs` (`list_conversations_sorted`, `conversations_list_handler`, the `use` block, the parser and filter code; the tests module)

**Interfaces:**
- Consumes: `search::{compile, CompileRequest, ListKind}`, `ApiError`.
- Produces: `pub async fn list_conversations_sorted(conn, account_id, q, order: ConversationOrder, limit, offset, today: NaiveDate) -> Result<ConversationListPage, ApiError>`.
- Deletes: `ConversationListQuery`, `ConversationTypeFilter`, `parse_participants_comparison`, `involves_people_group_sql`, `parse_conversation_list_query`, and the imports of `ExportQueryError`, `has_message_tag_sql`, `CountComparison`, `extract_keyed_ops`, `parse_count_comparison`, `like_ci`, `name_eq_ci`, `in_placeholders` where nothing else in the file uses them.

- [ ] **Step 1: Write the failing HTTP test**

In the `tests` module of `conversations_api.rs` (reuse whatever fixture helper it already has for seeding a conversation; the existing tests show the shape), add:

```rust
    #[tokio::test]
    async fn conversation_list_takes_the_search_language() {
        let (vault, token, _account) = conversations_fixture().await;
        let page: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/conversations?q=kind:direct", &token).await;
        assert!(page["total"].as_u64().unwrap() >= 1);
        let status = crate::test_support::get_status(&vault.state, "/v1/conversations?q=is:direct", &token).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        let status = crate::test_support::get_status(&vault.state, "/v1/conversations?q=trashed:yes", &token).await;
        assert_eq!(status, axum::http::StatusCode::OK);
    }
```

If the file has no fixture that registers an account and seeds a conversation, write `conversations_fixture` from `test_support::{test_vault, register_via_api, seed_one_message}`: create the vault, register `alice`, call `seed_one_message`, and return `(vault, token, account)`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p message-vault-server conversations_api::tests::conversation_list_takes_the_search_language`
Expected: FAIL on the `is:direct` assertion (today it answers 200).

- [ ] **Step 3: Rewrite `list_conversations_sorted`**

Replace the function body from its start down to the line `let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();` with:

```rust
pub async fn list_conversations_sorted(
    conn: &mut AnyConnection,
    account_id: &str,
    q: &str,
    order: ConversationOrder,
    limit: usize,
    offset: usize,
    today: chrono::NaiveDate,
) -> Result<ConversationListPage, ApiError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    if offset > MAX_LIST_OFFSET {
        return Err(ApiError::BadRequest(format!(
            "offset exceeds maximum of {MAX_LIST_OFFSET}"
        )));
    }
    let engine = engine_of(conn);
    let filter = crate::search::compile(crate::search::CompileRequest {
        list: crate::search::ListKind::Conversations,
        query: q,
        account_id,
        engine,
        today,
    })?;
    let where_sql = filter.where_sql();

    let count_sql = renumber_placeholders(&format!(
        "SELECT COUNT(*) FROM conversations c WHERE {where_sql}"
    ));
    let total: i64 = sqlx::query_scalar_with(&count_sql, bind_args(filter.params()))
        .fetch_one(&mut *conn)
        .await?;
    let total = total.max(0) as u64;

    let sql = renumber_placeholders(&format!(
        "SELECT c.id,
                c.conversation_type,
                c.group_title,
                (SELECT COUNT(*) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS message_count,
                (SELECT MAX(m.timestamp) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS last_message_at,
                (SELECT MIN(m.timestamp) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS date_range_start,
                (SELECT MAX(m.timestamp) FROM messages m
                 WHERE m.conversation_id = c.id AND m.duplicate_of IS NULL) AS date_range_end
         FROM conversations c
         WHERE {where_sql}
         ORDER BY {order_by}
         LIMIT ? OFFSET ?",
        order_by = order.order_by_sql(),
    ));
    let mut params = filter.params().to_vec();
    params.push(SqlParam::Int(limit as i64));
    params.push(SqlParam::Int(offset as i64));
    let rows: Vec<RawConversationRow> = sqlx::query_as_with(&sql, bind_args(&params))
        .fetch_all(&mut *conn)
        .await?;
    let rows: Vec<RawConversation> = rows
        .into_iter()
        .map(
            |(id, conversation_type, group_title, message_count, last_message_at, date_range_start, date_range_end)| {
                RawConversation { id, conversation_type, group_title, message_count, last_message_at, date_range_start, date_range_end }
            },
        )
        .collect();
```

The rest of the function (participants, sources, tags, and the `ConversationSummary` mapping) is unchanged except that its `?` operators now convert into `ApiError`; where it wrote `.map_err(|e| ExportQueryError::Internal(e.to_string()))?`, write `.map_err(|e| ApiError::Internal(e.to_string()))?`. The helpers `load_participants`, `load_conversation_sources`, `chat_handle_as_participant`, and `list_conversation_source_stats` change their error type from `ExportQueryError` to `ApiError` in the same way; `ApiError` already converts from `sqlx::Error`.

The `JOIN handles hc` is gone from both statements: the module reaches the chat handle through a subquery, and nothing in the SELECT list needed `hc`.

In `conversations_list_handler`, pass `chrono::Local::now().date_naive()` as the new last argument.

Delete the items listed in the Interfaces block.

- [ ] **Step 4: Fix the module's tests**

Run: `cargo test -p message-vault-server conversations_api`

Apply the same three rules as Task 10 Step 4. Spellings: `is:trash` becomes `trashed:yes`, `is:direct` becomes `kind:direct`, `contact:<id>` becomes `with:#<id>`, `people:` and `within:` become `group:`, `import:<id>` becomes `import:#<id>`, `participants:>3` stays. Add `crate::search::tests::today()` as the last argument to every direct `list_conversations_sorted` call.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/conversations_api.rs
git commit -m "refactor(conversations): the conversation list compiles its query with the search module

The list asks the search module for a fragment over the conversations
alias and keeps only its own SELECT, ORDER BY, and paging. Its private
parser, the contact-group and tag SQL it borrowed from other files,
and the handles join are gone."
```

---

### Task 12: Messages export calls the module; the old parser is deleted

**Files:**
- Modify: `crates/vault/server/src/export_api.rs`
- Modify: `crates/vault/server/src/contacts_api.rs` (remove now-unused `pub(crate)` items)
- Modify: `crates/vault/server/src/lib.rs`
- Modify: `crates/vault/server/tests/search_parity.rs`
- Delete: `crates/vault/server/src/search_query.rs`
- Delete: `tests/fixtures/search/parse-cases.json` (only if `grep -rn parse-cases .` finds no other reader)

**Interfaces:**
- Consumes: `search::{compile, CompileRequest, ListKind}`.
- Produces:
  - `ExportPageOpts { account_id, query, limit, offset, cursor, today: NaiveDate }` and `ExportCountOpts { account_id, query, today: NaiveDate }`. `source_override` is gone from both.
  - `export_messages(conn, opts) -> Result<ExportMessagesResponse, ApiError>` and `export_message_count(conn, opts) -> Result<ExportCountResponse, ApiError>`.
  - `source_word(raw: &str) -> Option<&'static str>` in `export_api.rs`: `imessage` to `imessage`, `whatsapp` to `whatsapp`, `sms-backup-restore` to `sms`, anything else `None`.
- Deletes: `ExportQueryError` and its impls, `BuiltFilters`, `prepare_message_export`, `nonempty_source`, `reject_unimplemented_message_filters`, `build_message_filters`, `append_metadata_text_filters`, `compile_metadata_fts_expr`, `compile_metadata_fts_children`, `push_metadata_like_chain`, `fts5_literal_query`, `pg_prefix_tsquery`, `has_message_tag_sql`, `involves_contacts_sql`, `list_group_member_contact_ids`, `contact_ids_within_day_bounds`, `push_participant_handle_or_alias_like`; the whole `search_query.rs`; `involves_contact_sql`, `NOT_TRASHED_CONVERSATION_SQL`, `NOT_TRASHED_CHAT_HANDLE_SQL`, `NOT_TRASHED_CONTACT_SQL` from `contacts_api.rs` (keep `involves_contact_expr` if `get_contact_detail` or `get_contact_summaries` still use it, and drop its `pub(crate)`).

- [ ] **Step 1: Write the failing tests**

In `export_api.rs`'s tests module, add (using whatever helper the module already has to seed messages; if none suits, use `crate::search::tests::seeded` and `ACCOUNT`):

```rust
    #[tokio::test]
    async fn export_takes_the_search_language_and_source_param() {
        let (pool, _dir, f) = crate::search::tests::seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        let today = crate::search::tests::today();
        let page = export_messages(
            &mut conn,
            ExportPageOpts { account_id: crate::search::tests::ACCOUNT, query: "from:me avocado", limit: 50, offset: None, cursor: None, today },
        )
        .await
        .unwrap();
        let mut ids: Vec<i64> = page.messages.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![f.jane_avocado_from_me, f.sam_avocado_from_me]);

        let count = export_message_count(
            &mut conn,
            ExportCountOpts { account_id: crate::search::tests::ACCOUNT, query: "source:whatsapp", today },
        )
        .await
        .unwrap();
        assert_eq!(count.messages, 1);

        let err = export_messages(
            &mut conn,
            ExportPageOpts { account_id: crate::search::tests::ACCOUNT, query: "has:attachment", limit: 50, offset: None, cursor: None, today },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn source_param_maps_to_the_source_word() {
        assert_eq!(source_word("imessage"), Some("imessage"));
        assert_eq!(source_word("sms-backup-restore"), Some("sms"));
        assert_eq!(source_word("whatsapp"), Some("whatsapp"));
        assert_eq!(source_word("mystery"), None);
        assert_eq!(source_word("  "), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server export_api::tests::export_takes_the_search_language_and_source_param`
Expected: compile error (`today` field, `source_word` missing).

- [ ] **Step 3: Rewrite the export functions**

Replace `ExportPageOpts` and `ExportCountOpts`:

```rust
/// Options for one exported page of messages.
#[derive(Debug, Clone)]
pub struct ExportPageOpts<'a> {
    /// Vault account to export from.
    pub account_id: &'a str,
    /// Search query string, in the search language.
    pub query: &'a str,
    /// Max messages on the page.
    pub limit: usize,
    /// Row offset; not combined with `cursor`.
    pub offset: Option<usize>,
    /// Opaque cursor from a previous page.
    pub cursor: Option<&'a str>,
    /// The day relative dates in `query` resolve against.
    pub today: chrono::NaiveDate,
}

/// Options for one export count query.
#[derive(Debug, Clone)]
pub struct ExportCountOpts<'a> {
    /// Vault account to count from.
    pub account_id: &'a str,
    /// Search query string, in the search language.
    pub query: &'a str,
    /// The day relative dates in `query` resolve against.
    pub today: chrono::NaiveDate,
}

/// The `source` query parameter as a word in the language, or `None` for a
/// value the language does not have.
pub(crate) fn source_word(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        "imessage" => Some("imessage"),
        "whatsapp" => Some("whatsapp"),
        "sms-backup-restore" => Some("sms"),
        _ => None,
    }
}

fn message_filter(
    engine: DbEngine,
    account_id: &str,
    query: &str,
    today: chrono::NaiveDate,
) -> Result<crate::search::Filter, ApiError> {
    Ok(crate::search::compile(crate::search::CompileRequest {
        list: crate::search::ListKind::Messages,
        query,
        account_id,
        engine,
        today,
    })?)
}
```

In `export_messages`, replace the call to `prepare_message_export` and the statement head with:

```rust
    let filter = message_filter(engine_of(conn), opts.account_id, opts.query, opts.today)?;
    let fetch_limit = limit + 1;

    let mut sql = format!(
        "SELECT m.id, m.conversation_id, m.source, m.service, m.guid, m.timestamp, m.timestamp_utc,
                m.sort_order, m.is_from_me, hs.raw AS sender, m.subject, m.body,
                m.is_announcement, m.is_reply, m.thread_originator_guid,
                m.thread_originator_part, m.num_replies,
                hc.raw AS chat_identifier, c.conversation_type, c.group_title
         {messages_from_sql}
         WHERE {where_sql}",
        messages_from_sql = messages_from_sql(),
        where_sql = filter.where_sql(),
    );
    let mut params = filter.params().to_vec();
```

The cursor, ordering, limit, and row mapping below are unchanged; every `ExportQueryError::bad(x)` becomes `ApiError::BadRequest(x)` and the function returns `Result<ExportMessagesResponse, ApiError>`. `messages_from_sql` and `conversation_join_sql` stay: they supply `hs`, `hc`, and `c` for the SELECT list only; the fragment itself mentions only `m`.

In `export_message_count`, the same: `let filter = message_filter(...)?;` and `WHERE {}` with `filter.where_sql()` and `filter.params()` in each of its statements, returning `ApiError`.

In the two handlers, build the query from the `source` parameter:

```rust
    let mut q = query.q.clone();
    if let Some(raw) = query.source.as_deref().filter(|s| !s.trim().is_empty()) {
        let word = source_word(raw).ok_or_else(|| {
            ApiError::BadRequest(format!("source: does not understand {}. Write one of: imessage, whatsapp, sms-backup-restore.", raw.trim()))
        })?;
        q = format!("source:{word} {q}");
    }
    let today = chrono::Local::now().date_naive();
```

and pass `query: &q` and `today` into the opts. The `load_participants`, `load_attachments`, and `load_tapbacks` helpers change their error type to `ApiError`.

Delete everything listed in the Interfaces block. Remove `pub(crate) mod search_query;` and `pub use search_query::parse_search_query;` from `lib.rs`, and delete `search_query.rs`. Then `grep -rn "ExportQueryError" crates/vault/server/src` must return nothing: any remaining function that returned it (in `contacts_api.rs`, for example `get_contact_detail`, `get_contact_summaries`, `unknown_contact_identifiers`) returns `ApiError` instead, with `ExportQueryError::bad(x)` becoming `ApiError::BadRequest(x)` and `ExportQueryError::Internal(x)` becoming `ApiError::Internal(x)`. Add `impl From<anyhow::Error> for ApiError` in `server.rs` (as `Internal(e.to_string())`) if those functions relied on the `anyhow` conversion.

- [ ] **Step 4: Update the parity test**

In `crates/vault/server/tests/search_parity.rs`: drop `parse_search_query` from the `use`, delete the line `parse_search_query(query).expect("committed parity query parses");`, replace `source_override: None,` with `today: chrono::NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),` in both `ExportPageOpts` literals, and change `.expect("committed parity query executes")` to stay as is (the error type changed, the call did not). The committed cases stay exactly as they are: the language keeps `AND`, `OR`, `NOT`, phrases, and prefixes.

- [ ] **Step 5: Fix the module's tests and run the workspace**

Run: `cargo test -p message-vault-server export_api` and apply the three rules from Task 10 Step 4. Spellings here: `has:attachment` becomes `attachment:any`, `after:2020` becomes `date:>=2020`, `before:2021` becomes `date:<2021`, `is:group` becomes `kind:group`, `in:<id>` becomes `in:#<id>`, `within:` and `people:` become `group:`, `search:contacts` tests are deleted (the mode switch is not a word). Then:

Run: `cargo build --workspace && cargo test --workspace`
Expected: green, including `search_parity`. `grep -rn "search_query\|ExportQueryError" crates/ src-tauri/` returns nothing.

- [ ] **Step 6: Commit**

```bash
git add -A crates/vault/server tests/fixtures/search
git commit -m "refactor(export): message export compiles its query with the search module; delete the old parser

export_messages and export_message_count ask the search module for a
fragment over the messages alias. The export routes' source parameter
becomes a leading source: word, so it means exactly what the word
means. search_query.rs, ExportQueryError, the metadata LIKE chain, and
the two pre-SQL lookups are deleted; the parity suite runs the same
committed cases through the new compiler."
```

---

### Task 13: `GET /v1/search/fields`

**Files:**
- Create: `crates/vault/server/src/search_api.rs`
- Modify: `crates/vault/server/src/lib.rs` (add `pub(crate) mod search_api;`)
- Modify: `crates/vault/server/src/openapi.rs` (register the route; add the `Search` tag; extend `dump_includes_browse_paths`)
- Modify: `docs/src/assets/openapi.json` (regenerated)
- Modify: `web/src/lib/vaultApi.types.ts` (regenerated)

**Interfaces:**
- Consumes: `search::{describe, FieldDoc, ListKind}`, `server::{AppState, FullAccess, ApiError}`.
- Produces: `search_api::search_fields_list` handler; `SearchFieldsQuery { list: ListKind }`; `SearchFieldsResponse { items: Vec<FieldDoc> }` (schema name `SearchFieldsResponse`); OpenAPI operationId `search_fields_list`, tag `Search`, `FullAccess`.

- [ ] **Step 1: Write the failing test**

`crates/vault/server/src/search_api.rs` with only its tests module:

```rust
#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support::{get_json, get_status, register_via_api, test_vault};

    #[tokio::test]
    async fn fields_are_served_per_list() {
        let vault = test_vault().await;
        let account = register_via_api(&vault.state, "alice", "hunter2hunter2").await;
        let body: serde_json::Value =
            get_json(&vault.state, "/v1/search/fields?list=contacts", &account.token).await;
        let words: Vec<&str> = body["items"].as_array().unwrap().iter().map(|i| i["word"].as_str().unwrap()).collect();
        assert!(words.contains(&"groups"));
        assert!(!words.contains(&"from"));
        let first = &body["items"][0];
        assert!(first["help"].is_string() && first["example"].is_string() && first["lists"].is_array());
        assert_eq!(get_status(&vault.state, "/v1/search/fields?list=nope", &account.token).await, StatusCode::BAD_REQUEST);
        assert_eq!(get_status(&vault.state, "/v1/search/fields?list=messages", "not-a-token").await, StatusCode::UNAUTHORIZED);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p message-vault-server search_api`
Expected: FAIL, route not found (404 where 200 is expected), after adding `pub(crate) mod search_api;` to `lib.rs`.

- [ ] **Step 3: Write the route**

Above the tests in `search_api.rs`:

```rust
//! `GET /v1/search/fields`: the words the search language accepts on one
//! list, so the web's suggestions and the docs read the server's own table.

use axum::Json;
use axum::extract::Query;
use serde::{Deserialize, Serialize};

use crate::search::{FieldDoc, ListKind, describe};
use crate::server::{ApiError, FullAccess};

/// Which list's words to describe.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(crate) struct SearchFieldsQuery {
    /// `contacts`, `conversations`, or `messages`.
    list: ListKind,
}

/// The words for one list, in the order the docs table shows them.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct SearchFieldsResponse {
    items: Vec<FieldDoc>,
}

/// The search words one list accepts.
#[utoipa::path(
    get,
    path = "/v1/search/fields",
    tag = "Search",
    security(("bearer" = [])),
    params(SearchFieldsQuery),
    responses(
        (status = 200, body = SearchFieldsResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn search_fields_list(
    FullAccess(_auth): FullAccess,
    Query(query): Query<SearchFieldsQuery>,
) -> Result<Json<SearchFieldsResponse>, ApiError> {
    Ok(Json(SearchFieldsResponse { items: describe(query.list) }))
}
```

In `openapi.rs`: add `(name = "Search", description = "The words the search language accepts")` to `tags`, add `.routes(routes!(crate::search_api::search_fields_list))` after the saved-searches routes, and add `"/v1/search/fields"` to the list in `dump_includes_browse_paths`.

An unknown `list` value is rejected by axum's `Query` extractor before the handler runs. Check that the rejection maps to a 400 with the JSON error body the rest of the API uses; if the crate has a custom `Query` rejection handler, this route inherits it, and if it does not, the test's status assertion still holds (axum answers 400 with plain text).

- [ ] **Step 4: Regenerate the OpenAPI document and the TypeScript types**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
cd web && npm run gen:api && cd ..
cargo test -p message-vault-server openapi
./scripts/check-generated-api-types.sh
```

Expected: `committed_openapi_matches_dump` passes; the script reports the types match.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p message-vault-server search_api openapi`
Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/search_api.rs crates/vault/server/src/lib.rs crates/vault/server/src/openapi.rs docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "feat(search): serve the word list at GET /v1/search/fields

The web's suggestions and the docs page read the server's own table of
words for a list, so nothing in the browser carries a copy that can
go stale."
```

---

### Task 14: The web speaks the new language

**Files:**
- Modify: `web/src/lib/vaultApi.ts` (add `listSearchFields`)
- Modify: `web/src/lib/vaultApi.test.ts`
- Modify: `web/src/lib/vaultKeys.ts` (add `searchFields`)
- Create: `web/src/lib/searchFields.ts`
- Create: `web/src/lib/searchFields.test.ts`
- Modify: `web/src/lib/contactGroups.ts` (delete `GROUP_FILTER_TOKEN_RE`, `hasGroupFilterToken`)
- Modify: `web/src/lib/useSearchSuggestions.ts` and its test
- Modify: `web/src/components/advancedSearch/buildAdvancedQuery.ts` and its test
- Modify: `web/src/screens/ContactList.tsx`
- Modify: `web/src/screens/ConversationList.tsx`
- Modify: `web/src/screens/TrashScreen.tsx`
- Modify: `web/src/components/AppLayout.tsx`
- Modify: `web/src/screens/message/useConversationMessages.ts`
- Modify: every `*.test.ts(x)` that carries an old spelling (found in Step 6)

**Interfaces:**
- Consumes: `Schema["SearchFieldsResponse"]`, `Schema["FieldDoc"]`, `Schema["ListKind"]` from the regenerated types.
- Produces:
  - `listSearchFields(list: Schema["ListKind"], opts?: VaultRequestOptions): Promise<Schema["SearchFieldsResponse"]>`
  - `keys.searchFields.all` and `keys.searchFields.list(list)`
  - `useSearchFields(list): { fields: Schema["FieldDoc"][]; loading: boolean }`
  - `hasFieldToken(q: string): boolean` (pure: true when the query has a `word:` token, quoted phrases excluded)
  - `stripFieldTokens(q: string): string` (pure: the free-text words only)

- [ ] **Step 1: Write the failing tests for the pure pieces**

`web/src/lib/searchFields.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { hasFieldToken, stripFieldTokens } from "./searchFields";

describe("hasFieldToken", () => {
  it("is true for any word: token, negated or not", () => {
    expect(hasFieldToken("group:Family")).toBe(true);
    expect(hasFieldToken("ana -tag:Work")).toBe(true);
    expect(hasFieldToken('title:"book club"')).toBe(true);
  });
  it("is false for plain words, phrases, and colons inside a phrase", () => {
    expect(hasFieldToken("ana")).toBe(false);
    expect(hasFieldToken('"re: dinner"')).toBe(false);
    expect(hasFieldToken("http://example.com")).toBe(false);
    expect(hasFieldToken("")).toBe(false);
  });
});

describe("stripFieldTokens", () => {
  it("keeps the words and drops the tokens", () => {
    expect(stripFieldTokens("ana group:Family")).toBe("ana");
    expect(stripFieldTokens('handle:"+1 555" bo -tag:x')).toBe("bo");
    expect(stripFieldTokens("just words")).toBe("just words");
  });
});
```

Add to `web/src/lib/vaultApi.test.ts`, inside the existing describe of route URLs:

```ts
  it("listSearchFields asks for one list's words", async () => {
    await listSearchFields("contacts");
    expect(get).toHaveBeenCalledWith("/v1/search/fields?list=contacts", undefined);
  });
```

and add `listSearchFields` to the import list at the top.

Replace the contents of `web/src/lib/useSearchSuggestions.test.ts` (or create it) with:

```ts
import { describe, expect, it } from "vitest";
import type { SearchField } from "./searchFields";
import { applySuggestionToQuery, buildSearchSuggestions } from "./useSearchSuggestions";

const fields: SearchField[] = [
  { word: "with", value_type: "person", values: [], help: "", example: "", lists: ["conversations", "messages"] },
  { word: "tag", value_type: "name", values: ["none"], help: "", example: "", lists: ["contacts", "conversations", "messages"] },
  { word: "kind", value_type: "choice", values: ["direct", "group"], help: "", example: "", lists: ["contacts", "conversations", "messages"] },
];

describe("buildSearchSuggestions", () => {
  it("completes a bare prefix to the words the list has", () => {
    const out = buildSearchSuggestions({ completingValue: false, personOp: false, lastToken: "t", fields: [...fields], contacts: [] });
    expect(out.map((s) => s.insert)).toEqual(["tag:"]);
  });
  it("offers a choice word's values after the colon", () => {
    const out = buildSearchSuggestions({ completingValue: true, personOp: false, lastToken: "kind:", fields: [...fields], contacts: [] });
    expect(out.map((s) => s.insert)).toEqual(["kind:direct ", "kind:group "]);
  });
  it("offers contacts by id for a person word", () => {
    const out = buildSearchSuggestions({
      completingValue: true,
      personOp: true,
      lastToken: "with:ja",
      fields: [...fields],
      contacts: [{ id: "42", name: "Jane Doe" }],
    });
    expect(out[0].insert).toBe("with:#42 ");
    expect(out[0].label).toBe("Jane Doe");
  });
});

describe("applySuggestionToQuery", () => {
  it("replaces the token being typed", () => {
    expect(applySuggestionToQuery("hello ta", { id: "tag", label: "tag:", insert: "tag:" })).toBe("hello tag:");
  });
});
```

Update `web/src/components/advancedSearch/buildAdvancedQuery.test.ts` so its expectations read (keep its existing structure; these are the strings that change):

```ts
    expect(buildMessagesQuery({ nameOrHandle: "jane", handle: "", msgType: "direct", participants: EMPTY_COUNT })).toBe("jane kind:direct");
    expect(buildMessagesQuery({ nameOrHandle: "", handle: "+1555", msgType: "group", participants: { comparator: ">", value: "3" } })).toBe("handle:+1555 kind:group participants:>3");
    expect(
      buildContactsQuery({
        contactName: "ana",
        handle: "+1 555",
        firstMsgBound: { op: "after", start: "2019-01-01", end: "" },
        lastMsgBound: { op: "between", start: "2022-01-01", end: "2023-01-01" },
        activity: "messages",
        noPreferredName: true,
        noHandle: false,
        services: ["whatsapp"],
      }),
    ).toBe('ana handle:"+1 555" first-message:>=2019-01-01 last-message:2022-01-01..2023-01-01 messages:>0 name:none service:whatsapp');
    expect(buildContactsQuery({ ...emptyContacts, activity: "no-messages", noHandle: true })).toBe("messages:0 handle:none");
    // `emptyContacts` is an all-empty ContactsQueryInput; define it at the top of the test file if it is not there already.
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/searchFields.test.ts src/lib/vaultApi.test.ts src/lib/useSearchSuggestions.test.ts src/components/advancedSearch/buildAdvancedQuery.test.ts`
Expected: FAIL (missing module, missing function, old strings).

- [ ] **Step 3: The route function, the key, and the hook**

`vaultApi.ts`, after the saved-searches functions:

```ts
// ── Search ──────────────────────────────────────────────────────────────────

/** The words the search language accepts on one list. */
export function listSearchFields(
  list: Schema["ListKind"],
  opts?: VaultRequestOptions,
): Promise<Schema["SearchFieldsResponse"]> {
  return apiClient.get<Schema["SearchFieldsResponse"]>(
    withQuery("/v1/search/fields", query({ list })),
    opts,
  );
}
```

`vaultKeys.ts`, inside `keys`:

```ts
  searchFields: {
    all: ["search-fields"] as const,
    list: (list: string) => ["search-fields", list] as const,
  },
```

`web/src/lib/searchFields.ts`:

```ts
import { listSearchFields } from "./vaultApi";
import type { components } from "./vaultApi.types";
import { keys } from "./vaultKeys";
import { useVaultQuery } from "./vaultQuery";

type Schema = components["schemas"];
export type SearchField = Schema["FieldDoc"];
export type SearchList = Schema["ListKind"];

/**
 * A `word:` token: a field name followed by a colon, at the start or after a
 * space, with or without a leading minus. Quoted phrases are removed first so
 * a colon inside one does not count.
 */
// The value must not start with `/`, so a pasted URL like `http://x` is a word.
const FIELD_TOKEN_RE = /(^|\s)-?[a-z][a-z-]*:(?!\/)/i;
const PHRASE_RE = /"(?:[^"]|"")*"/g;

/** True when the query has a `word:` token, which only the vault can apply. */
export function hasFieldToken(q: string): boolean {
  return FIELD_TOKEN_RE.test(q.replace(PHRASE_RE, " "));
}

/** The free-text words of a query, with every `word:value` token removed. */
export function stripFieldTokens(q: string): string {
  return q
    .replace(/(^|\s)-?[a-z][a-z-]*:(?!\/)("(?:[^"]|"")*"|\S*)/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** The words one list accepts, from the vault, cached for the session. */
export function useSearchFields(list: SearchList): { fields: SearchField[]; loading: boolean } {
  const { data, isPending } = useVaultQuery(
    keys.searchFields.list(list),
    async (signal) => (await listSearchFields(list, { signal })).items,
    { staleTime: Number.POSITIVE_INFINITY },
  );
  return { fields: data ?? [], loading: isPending };
}
```

- [ ] **Step 4: Suggestions and the advanced form**

Rewrite `web/src/lib/useSearchSuggestions.ts`:

```ts
import { useEffect, useState } from "react";
import { type SearchField, type SearchList, useSearchFields } from "./searchFields";
import { listContacts } from "./vaultApi";

interface ContactName {
  id: string;
  name: string;
}

/** One autocomplete entry: unique id, displayed label, text inserted into the query. */
export interface Suggestion {
  id: string;
  label: string;
  insert: string;
}

/** Words whose value is a person, so contact names are offered. */
function isPersonWord(field: SearchField | undefined): boolean {
  return field?.value_type === "person";
}

/** Autocomplete rows for a search box on one list. */
export function buildSearchSuggestions(args: {
  completingValue: boolean;
  personOp: boolean;
  lastToken: string;
  fields: SearchField[];
  contacts: ContactName[];
}): Suggestion[] {
  const colon = args.lastToken.indexOf(":");
  if (args.completingValue) {
    const word = args.lastToken.slice(0, colon).replace(/^-/, "").toLowerCase();
    const typed = args.lastToken.slice(colon + 1).toLowerCase();
    if (args.personOp) {
      return args.contacts.slice(0, 6).map((c) => ({
        id: c.id,
        label: c.name,
        // #id survives names with spaces and renames.
        insert: `${word}:#${c.id} `,
      }));
    }
    const field = args.fields.find((f) => f.word === word);
    if (!field) return [];
    return field.values
      .filter((v) => v.startsWith(typed))
      .map((v) => ({ id: `${word}:${v}`, label: `${word}:${v}`, insert: `${word}:${v} ` }));
  }
  if (args.lastToken.length === 0) return [];
  const typed = args.lastToken.replace(/^-/, "").toLowerCase();
  return args.fields
    .filter((f) => f.word.startsWith(typed))
    .map((f) => ({ id: f.word, label: `${f.word}:`, insert: `${f.word}:` }));
}

/** Replace the token being typed with a suggestion's text. */
export function applySuggestionToQuery(value: string, suggestion: Suggestion): string {
  const tokens = value.split(/\s+/);
  tokens.pop();
  return tokens.concat(suggestion.insert).join(" ");
}

/**
 * Word and value autocomplete for a search box. A bare prefix completes to a
 * word the list has; a choice word offers its values; a person word fetches
 * matching contacts and inserts `word:#id`.
 */
export function useSearchSuggestions(value: string, list: SearchList, enabled: boolean): Suggestion[] {
  const { fields } = useSearchFields(list);
  const [contacts, setContacts] = useState<ContactName[]>([]);

  const lastToken = value.split(/\s+/).pop() || "";
  const colonIdx = lastToken.indexOf(":");
  const completingValue = colonIdx !== -1;
  const word = completingValue ? lastToken.slice(0, colonIdx).replace(/^-/, "").toLowerCase() : "";
  const valuePart = completingValue ? lastToken.slice(colonIdx + 1).replace(/^"|"$/g, "") : "";
  const personOp = completingValue && isPersonWord(fields.find((f) => f.word === word));

  useEffect(() => {
    if (!enabled || !personOp) {
      setContacts([]);
      return;
    }
    const ac = new AbortController();
    const t = window.setTimeout(() => {
      listContacts({ q: valuePart, limit: 20, offset: 0 }, { signal: ac.signal })
        .then((res) => setContacts((res.contacts || []).map((c) => ({ id: String(c.id), name: c.name }))))
        .catch(() => {
          if (!ac.signal.aborted) setContacts([]);
        });
    }, 150);
    return () => {
      window.clearTimeout(t);
      ac.abort();
    };
  }, [enabled, personOp, valuePart]);

  if (!enabled) return [];
  return buildSearchSuggestions({ completingValue, personOp, lastToken, fields, contacts });
}
```

Every caller of `useSearchSuggestions` gains the `list` argument: the header search box passes `"conversations"` in conversation mode, `"contacts"` in contacts mode, and `"conversations"` in trash mode (`grep -rn "useSearchSuggestions(" web/src` finds them).

In `buildAdvancedQuery.ts`, change `pushDateBoundTokens` and the two builders:

```ts
/** Emit one `prefix:` date token: `>=D`, `<D`, or an inclusive `D..D` range. */
export function pushDateBoundTokens(
  push: (s: string) => void,
  prefix: "first-message" | "last-message",
  bound: DateBoundFilter,
): void {
  switch (bound.op) {
    case "any":
      return;
    case "after":
      if (bound.start) push(`${prefix}:>=${bound.start}`);
      return;
    case "before":
      if (bound.start) push(`${prefix}:<${bound.start}`);
      return;
    case "between":
      if (bound.start && bound.end) push(`${prefix}:${bound.start}..${bound.end}`);
      else if (bound.start) push(`${prefix}:>=${bound.start}`);
      else if (bound.end) push(`${prefix}:<${bound.end}`);
      return;
  }
}

export function buildMessagesQuery(input: MessagesQueryInput): string {
  const parts: string[] = [];
  const push = (s: string) => {
    if (s.trim()) parts.push(s.trim());
  };
  if (input.nameOrHandle.trim()) push(input.nameOrHandle.trim());
  if (input.handle.trim()) push(`handle:${input.handle.trim()}`);
  if (input.msgType === "direct") push("kind:direct");
  if (input.msgType === "group") push("kind:group");
  const participantCmp = composeCountComparison(input.participants);
  if (participantCmp) push(`participants:${participantCmp}`);
  return parts.join(" ");
}

export function buildContactsQuery(input: ContactsQueryInput): string {
  const parts: string[] = [];
  const push = (s: string) => {
    if (s.trim()) parts.push(s.trim());
  };
  if (input.contactName.trim()) push(input.contactName.trim());
  if (input.handle.trim()) push(`handle:"${input.handle.trim()}"`);
  pushDateBoundTokens(push, "first-message", input.firstMsgBound);
  pushDateBoundTokens(push, "last-message", input.lastMsgBound);
  if (input.activity === "messages") push("messages:>0");
  if (input.activity === "no-messages") push("messages:0");
  if (input.noPreferredName) push("name:none");
  if (input.noHandle) push("handle:none");
  for (const id of input.services) {
    push(`service:${String(id)}`);
  }
  return parts.join(" ");
}
```

The form's mode switch to Contacts is UI state; wherever the form used the presence of `search:contacts` in the query to pick the Contacts screen (`AppLayout.handleSearch`), it now reads the form's own mode. Trace `buildContactsQuery`'s caller to find where the mode is known and pass it through to `handleSearch` as a second argument `mode: "messages" | "contacts"`.

- [ ] **Step 5: The screens**

`ContactList.tsx`: delete `ADVANCED_TOKEN_RE` and `hasAdvancedContactTokens`; import `hasFieldToken` and `stripFieldTokens` from `../lib/searchFields`; replace `const advancedActive = hasAdvancedContactTokens(filter) || hasGroupFilterToken(filter);` with `const advancedActive = hasFieldToken(filter);`; in `filterNeedles`, replace the two `.replace(ADVANCED_TOKEN_RE, " ").replace(GROUP_FILTER_TOKEN_RE, " ")` calls with `q = stripFieldTokens(q);`, keeping the `handle:` extraction above it (it still reads a `handle:` value for local matching, now with the regex `/(^|\s)handle:("([^"]+)"|(\S+))/i`).

`ConversationList.tsx`: replace the immediate-apply regex test with `if (hasFieldToken(query)) {`.

`contactGroups.ts`: delete `GROUP_FILTER_TOKEN_RE` and `hasGroupFilterToken`; `queryToken: "group"` stays.

`TrashScreen.tsx`: `return term ? \`trashed:yes ${term}\` : "trashed:yes";`. `AppLayout.tsx`: `const trashListQuery = trashSearch.trim() ? \`trashed:yes ${trashSearch.trim()}\` : "trashed:yes";` and the comment above it; in `handleSearch`, replace `/\bsearch:contacts\b/i.test(q) || contactsMode` with `mode === "contacts" || contactsMode` using the new second argument (default `"messages"`); `contactBrowseQuery` becomes:

```ts
function contactBrowseQuery(contactId: string, kind: ContactBrowseKind, handle?: string): string {
  let kindSuffix = "";
  if (kind === "direct") kindSuffix = " kind:direct";
  else if (kind === "group") kindSuffix = " kind:group";
  const h = handle?.trim();
  if (h) {
    const quoted = /\s/.test(h) ? `"${h}"` : h;
    return `handle:${quoted}${kindSuffix}`;
  }
  return `with:#${contactId}${kindSuffix}`;
}
```

and its `service` parameter and the `serviceSuffix` are dropped at the one call site (`grep -n "contactBrowseQuery(" web/src/components/AppLayout.tsx`).

`useConversationMessages.ts`: `const q = \`in:#${conversationId}\`;` and

```ts
/** Search query that loads every message in one calendar year. */
function yearQuery(conversationId: string, year: number): string {
  return `in:#${conversationId} date:${year}`;
}
```

- [ ] **Step 6: Find every remaining old spelling and run the suite**

```bash
cd web && grep -rnE '\b(is:trash|is:direct|is:group|has:[a-z-]+|search:contacts|people:|within:|label:|contact:[0-9]|after:|before:|first-contact|last-contact|message-count|group-count|larger:|smaller:|filetype:|text:)' src --include=*.ts --include=*.tsx
```

Every hit is either a test fixture (rewrite it to the spec's word) or a screen this task missed (fix it the same way). Then:

Run: `cd web && npm run lint && npm test`
Expected: green.

- [ ] **Step 7: Walk it in the browser**

Start the vault (`./scripts/run-vault-dev.sh --reset-demo`) and Vite (`cd web && npm run dev`), then with the Playwright MCP on `http://127.0.0.1:5173`, signed in as `demo`:

1. Contacts: click a Contact Group in the sidebar; the list narrows. Type `name:none` in the search box; only nameless contacts show. Type `from:me`; the message under the box reads "from: is not a Contacts word. It works on Messages."
2. Conversations: type `kind:group participants:>2`; only group threads show. Type `tag:` and pick a suggestion; the value list appears.
3. Trash: the count loads and the search box narrows within it.
4. Open a conversation, pick a year in the footer; that year's messages load.
5. Open the contact drawer, click "direct conversations"; the conversation list is filtered by `with:#<id> kind:direct` (visible in the URL's `q`).

- [ ] **Step 8: Commit**

```bash
git add web/src
git commit -m "feat(web): speak the new search language and read the word list from the vault

Screens build with:, kind:, trashed:, in:#id, date:, and the other new
words, and the operator-sniffing regexes are replaced by one rule: a
query with any word: token goes to the vault. Suggestions come from
GET /v1/search/fields, so the browser holds no list of its own."
```

---

### Task 15: The docs page, and the test that keeps it true

**Files:**
- Modify: `docs/src/content/docs/vault/user/how-to/search.md` (rewrite the operator section)
- Modify: `crates/vault/server/src/search/tests.rs` (add `mod docs`)

**Interfaces:**
- Consumes: `fields::FIELDS`.

- [ ] **Step 1: Write the failing docs test**

```rust
mod docs {
    use crate::search::fields::FIELDS;

    const PAGE: &str = include_str!("../../../../../docs/src/content/docs/vault/user/how-to/search.md");

    /// The words the page's table lists: the first backticked `word:` in
    /// each table row.
    fn documented_words() -> Vec<String> {
        PAGE.lines()
            .filter(|l| l.starts_with("| `"))
            .filter_map(|l| {
                let cell = l.trim_start_matches("| `");
                let token = cell.split('`').next()?;
                let word = token.strip_suffix(':').or_else(|| token.split(':').next())?;
                if word.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !word.is_empty() {
                    Some(word.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn the_page_lists_every_word_and_nothing_else() {
        let documented = documented_words();
        for spec in FIELDS {
            assert!(documented.contains(&spec.word.to_string()), "docs page is missing {}:", spec.word);
        }
        for word in &documented {
            assert!(FIELDS.iter().any(|f| f.word == word), "docs page lists {word}:, which the language does not have");
        }
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p message-vault-server search::tests::docs`
Expected: FAIL, the page still lists `from:me` style rows and words the language does not have.

- [ ] **Step 3: Rewrite the page**

Replace everything from `## Query operators` to the end of `search.md` with:

````markdown
## The search language

One language works on all three lists: **Contacts**, **Conversations**, and **Messages**. Plain words search the row's own text: a contact's name and handles, a conversation's title and who is in it, a message's body, subject, and attachment names. Everything else is a word, a colon, and a value.

### How values work

- Put quotes around a value with a space or a colon: `group:"Book Club"`. Two quotes in a row are one quote.
- Case never matters.
- `#12` means the thing with that id: `group:#7`, `with:#42`, `in:#19`.
- `none` and `any` work on every word that names a thing or holds text: `tag:none`, `attachment:any`, `name:none`.
- Dates name a span: `2024` is the year, `2024-05` the month, `2024-05-01` the day, `7d` or `2w` or `3m` or `1y` the last that long, plus `today` and `yesterday`. Add `>=` for from its start, `<` for before its start, `>` for after its end, `<=` for up to its end, or `a..b` for a range: `date:2019`, `date:>=2019`, `date:<1m`, `date:2019..2021`.
- Sizes are `500k`, `1M`, `2G`, or plain bytes. Counts are plain numbers. Both take the same `>`, `>=`, `<`, `<=`, and `a..b`.
- A comma inside a value means either: `service:imessage,sms`. Repeating a word means both: `tag:Work tag:Urgent`.
- `-` in front of anything means not: `-tag:Work`, `-avocado`. `or` and parentheses work as you would expect: `(toast or guacamole) avocado`.
- `avoc*` matches words starting with avoc; `"exact phrase"` matches the phrase.

A word the list does not have is refused with a message that says so, and offers the nearest word when there is one.

### The words

C, V, and M mark which lists accept the word: Contacts, Conversations, Messages.

| Word | Means | Values | Lists |
|---|---|---|---|
| `body:` | message body only | text, `none`, `any` | V M |
| `subject:` | subject line only | text, `none`, `any` | V M |
| `name:` | a person's name: this contact, or someone in the conversation | text, `none`, `any` | C V M |
| `title:` | the conversation's title | text, `none`, `any` | V M |
| `handle:` | a phone number, email, or username | text, `none`, `any` | C V M |
| `with:` | this person is in the conversation | name, handle, `#id` | V M |
| `from:` | this person sent it | `me`, name, handle, `#id` | M |
| `to:` | it was sent to this person | `me`, name, handle, `#id` | M |
| `in:` | this one conversation | title, handle, `#id` | M |
| `group:` | in this Contact Group: the contact, or someone in the conversation | name, `#id`, `none`, `unknown` | C V M |
| `tag:` | the conversation carries this Message Tag | name, `#id`, `none` | C V M |
| `kind:` | direct or group conversation | `direct`, `group` | C V M |
| `service:` | how the message travelled | `imessage`, `sms`, `mms`, `rcs`, `whatsapp` | C V M |
| `source:` | which backup it was imported from | `imessage`, `whatsapp`, `sms` | V M |
| `import:` | brought in by this Import Run | `#id`, `last` | V M |
| `date:` | when a message was sent; on Contacts and Conversations, has a message then | date | C V M |
| `first-message:` | the date of the earliest message | date | C V M |
| `last-message:` | the date of the latest message | date | C V M |
| `attachment:` | what is attached | `image`, `video`, `audio`, `document`, `pdf`, `contact`, `other`, `any`, `none` | V M |
| `filename:` | an attachment's file name | text, `pre*` | V M |
| `size:` | an attachment's size | `>1M`, `<500k`, `100k..2M` | V M |
| `messages:` | how many messages | `>100`, `0`, `1..10` | C V |
| `conversations:` | how many conversations | count | C |
| `groups:` | how many Contact Groups | count | C |
| `participants:` | how many people in the conversation | count | V M |
| `attachments:` | how many attachments on the message | count | M |
| `trashed:` | in the trash | `yes`, `no`, `any` | C V |

### Examples

- `from:me to:"Jane Doe" (avocado or "guacamole night")` on Messages.
- `last-message:<2022` on Contacts: everyone you have not heard from since 2022.
- `group:Family date:2019..2021 attachment:image size:>1M` on Messages.
- `participants:>2 -tag:Archive` on Conversations.
- `group:none messages:>0` on Contacts: people with messages who are in no Contact Group.

Sorting, the Contacts switch, and how results are grouped are controls on the screen, not words in the search, so a Saved Search means what you want and never how the screen looked.
````

Keep the page's front matter and the two sections above `## Query operators` as they are, except the sentence "Full-text search across message bodies (operators for from/to/with, attachments, dates, and sources)" which becomes "Searches message text; the words below narrow it."

- [ ] **Step 4: Run the tests and the docs build**

Run: `cargo test -p message-vault-server search::tests::docs && cd docs && npm run check && npm run build`
Expected: the test passes; the docs build succeeds.

- [ ] **Step 5: Commit**

```bash
git add docs/src/content/docs/vault/user/how-to/search.md crates/vault/server/src/search/tests.rs
git commit -m "docs(search): rewrite the search page from the language spec, and test it

The page lists the twenty-seven words with the lists each applies to
and the value rules above them. A server test reads the page and fails
when it lists a word the language does not have, or misses one it has."
```

---

### Task 16: The gate, the walkthrough, and the pull request

**Files:**
- Modify: whatever the gate reports.

- [ ] **Step 1: Format and lint**

```bash
./scripts/format-all.sh
./scripts/lint-all.sh
```

Fix every Clippy and Biome finding with a real change, not an ignore. Commit as `chore: format and lint the search language branch`.

- [ ] **Step 2: The full gate**

```bash
./scripts/check-pr.sh
```

Expected: every step passes: rustfmt, workspace build and test (including `search_parity`), the desktop build, Biome `ci`, Vitest, the generated-types check, the docs check and build.

- [ ] **Step 3: Prove the old words are gone**

```bash
grep -rnE '"(people|within|label|before|after|larger|smaller|filetype|has|is|search|context|sort):' crates/vault/server/src web/src --include=*.rs --include=*.ts --include=*.tsx | grep -v test
grep -rn "search_query\|ExportQueryError\|GROUP_FILTER_TOKEN_RE\|hasGroupFilterToken\|ADVANCED_TOKEN_RE" crates web/src
```

Expected: nothing.

- [ ] **Step 4: The browser walkthrough**

Repeat Task 14 Step 7 against the freshly built branch, and add: type `people:Family` on Conversations and confirm the message under the box reads "people: is not a search word." with nothing else after it.

- [ ] **Step 5: Push and open the pull request**

```bash
git push -u origin worktree-search-language-design
gh pr create --title "feat(search): one search language, compiled in one module" --body-file - <<'EOF'
## What

One `search` module in the vault server now parses the search language and compiles it to SQL for the Contacts, Conversations, and Messages lists. The three route files no longer parse queries or build filter SQL. The language itself was redesigned: twenty-seven words, one meaning each, comparisons in the value, and presentation settings out of the string. A new `GET /v1/search/fields` route serves the word list, the web speaks the new words and reads that list for suggestions, and the docs page is rewritten from the spec with a test that keeps it true.

## Why

The server had three parsers for one language, with the same concept spelled differently on different lists and the same spelling meaning different things. The words were a quick first pass from August that was always expected to change. See `docs/adr/0004-one-search-language-compiled-in-one-module.md` and `docs/superpowers/specs/2026-09-02-search-language-design.md`.

## Breaks

Saved Searches written in spellings the language does not have stop working and say why. Nothing is rewritten on purpose. The export routes' `source` parameter now accepts `imessage`, `whatsapp`, and `sms-backup-restore` only.

## Checked

`./scripts/check-pr.sh` green; browser walkthrough of Contacts, Conversations, Trash, the thread year filter, and the contact drawer.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
gh pr checks --watch
```

Expected: CI green. Do not merge; the merge is the user's call.

---

## Self-review notes

Spec coverage, requirement by requirement:

- One module, parse and compile, pure: Tasks 1 to 4.
- The twenty-seven words with per-list applicability: Task 3 registry; emitters in Tasks 5 to 8; Task 9 proves every claimed word-list pair runs.
- Value rules (quoting, case, `#id`, `none`/`any`, dates, sizes, counts, commas, negation, free text per row): Tasks 1, 2, 3, 4.
- Rejection with word and list, did-you-mean from the current word list only, no memory of old spellings: Task 3 and Task 9's `refusals`.
- Presentation out of the language: Task 12 (`source` parameter becomes a word), Task 14 (screens send `trashed:yes`, no `search:contacts`), Task 15 (the page says so).
- Interface invariants (one alias, account scope and defaults inside, `?` placeholders, determinism, errors total): Task 4 tests and Task 9's Postgres check.
- The two entry points, `compile` and `describe`: Task 4 and Task 3.
- `GET /v1/search/fields`: Task 13.
- The three callers, deletions, and the parity test: Tasks 10 to 12.
- The web changes, file by file: Task 14.
- The docs page and its test: Task 15.
- The spec's ten test cases: 1 and 3 in Task 8 `counts`, 2 in Task 8 `dates_on_every_list`, 4 in Task 6, 5 and 6 in Task 8 `first_and_last_message`, 7 in Task 7 `trash_is_a_word`, 8 in Task 9 `refusals`, 9 in Task 4, 10 in Task 9 `coverage` and Task 15 `docs`.

Two deliberate departures from the spec text, recorded in the spec in the same change: the export routes keep their `source` parameter and map it to the `source:` word inside the handler (the desktop pull client passes a raw source id, and the compiled default for dedupe must see the word), and `describe` returns an owned `Vec<FieldDoc>`.
