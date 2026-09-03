# One search language, compiled in one module

## Problem Statement

A person types the same kind of query on three lists: Contacts, Conversations,
and Messages (the Export screen and the web's message search). The docs
describe one search language, but the server has three parsers for it, one in
each route file:

- `contacts_api.rs` has its own token scanner, quoted-value reader, and date
  parser (`parse_contact_list_filters`, `take_prefixed_quoted_or_bare`,
  `normalize_ymd`).
- `conversations_api.rs` has another (`parse_conversation_list_query`).
- `export_api.rs` uses the fullest one, `search_query.rs`, then rejects six
  documented operators as "not implemented in SQL yet": `text:`, `filename:`,
  `filetype:`, `larger:`, `smaller:`, `has:noattachment`.

The SQL side is scattered the same way. The pieces that turn a parsed query
into SQL are spread across all three files and shared as crate-visible string
builders (`involves_contact_sql`, `has_message_tag_sql`,
`NOT_TRASHED_CONVERSATION_SQL`), and `conversations_api.rs` borrows from both
of the others. Every query error is `export_api::ExportQueryError`, so the
low-level `search_query.rs` depends upward on a route file.

The words themselves grew by accretion in the first week of August 2026, when
the goal was to get search working at all. The same concept has different
spellings on different lists: the Contact Group filter is `group:` on
Contacts and `people:`, `within:`, or `label:` on Conversations and Messages.
The same spelling has different meanings: `group:none` means "in no Contact
Group" on Contacts and "one row per message" on Messages. `has:` and `is:`
take a different set of values on every list. A token that does not belong to
a list is silently searched as words on Contacts, refused with a 400 on
Messages, and something in between on Conversations. A date token may behave
differently on the Contacts screen than in a message search, because two
different date parsers handle it.

The deletion test: delete any one route file and its list logic must
reappear, so each earns its keep. Delete the three parsers and the same
tokenizing, quoting, and date handling reappears three times. That is the
pass-through, and that is what this design removes.

## Decisions

Recorded in `docs/adr/0004-one-search-language-compiled-in-one-module.md`:

1. The search language is one module. It owns parsing every token and
   compiling it to SQL for all three lists.
2. The module's interface is parse and compile, and compile is pure: no
   database connection, no clock. Lookups that today run before SQL is built
   become subqueries.
3. A token that is not a filter for the requested list is rejected with a 400
   that names the word and the list. Nothing is silently treated as words or
   dropped.
4. One word per concept. Comparisons and ranges live in the value using
   conventional notation (`date:>2019`, `size:<500k`, `messages:>100`,
   `date:2019..2021`). No aliases, no `before:`/`after:`, no
   `larger:`/`smaller:`.
5. Presentation leaves the language. Sort order, context lines, one row per
   message versus per conversation, and the Contacts mode switch are request
   parameters. A query string only ever narrows which rows come back.
6. The module knows nothing about spellings that came before it. A word it
   does not have is an unknown word, the same as a typo. Stored Saved Search
   text is not rewritten; it fails the same way typed text does, and the
   person edits it once.

The language below follows design B ("the search language as a field
registry") from the design-it-twice round on 2 September 2026, with three
changes: `sent:` is spelled `date:`, the participant count is spelled
`participants:` rather than `people:` so that a person-shaped word never means
a number, and the module interface is trimmed to two entry points, compile
and describe. A validate call was considered and dropped: the web shows the
list request's own 400 under the search box, so nothing would call it.

## The language

### Grammar

```
query  := expr
expr   := term (WS term)*          space means AND; the word "and" is accepted
        | expr "or" expr           the word "or", case-insensitive
        | "(" expr ")"
term   := ["-"] atom               "-" means NOT; the word "not" is accepted
atom   := field ":" values | word | "phrase" | word "*"
values := value ("," value)*       comma means OR inside one field
value  := scalar
        | (">" | ">=" | "<" | "<=") scalar
        | scalar ".." scalar       inclusive range
```

Every filter is `field:values`. Repeating a field means AND (`tag:Work
tag:Urgent` is both). A comma means OR within one field (`service:imessage,sms`).

### Value rules

- **Quoting.** `"…"` around any value with a space or a colon:
  `group:"Book Club"`, `title:"Re: dinner"`. A doubled quote inside is a
  literal quote. An empty value (`tag:`) is an error, never a no-op.
- **Case.** Field names and keyword values fold to lower case. Text, names,
  and handles match case-insensitively, using `like_ci` and `name_eq_ci` from
  `db/dialect.rs`.
- **By id.** `#N` wherever a named thing is expected means "the one with this
  id": `group:#7`, `tag:#3`, `with:#42`, `in:#19`, `import:#12`. Names and
  ids interchange; the web uses ids where it has them.
- **`none` and `any`.** Universal values for any field that names a thing or
  holds text: `group:none`, `tag:none`, `attachment:none`, `attachment:any`,
  `name:none`, `subject:none`. These replace every `has:` and `is:` spelling.
  Counts use numbers: `messages:0`, `attachments:>0`.
- **Dates.** `2024`, `2024-05`, `2024-05-01` each name the whole span they
  spell out. `7d`, `2w`, `3m`, `1y` name the span from that long ago until
  today. `today` and `yesterday` name one day. A bare span means "inside it".
  `>=` means from the span's start, `<` before its start, `>` after its end,
  `<=` until its end, and `a..b` is inclusive of both spans. So `date:2019`
  is the year, `date:>=2019` is 2019 onward, `date:<1m` is older than a
  month, `date:2019..2021` is three years. Today's date is an input to
  compile, never read from the clock.
- **Sizes.** `500k`, `1M`, `2G` are 1024-based; a bare number is bytes.
- **Counts.** Integers, with the same comparisons and ranges as dates.
- **Free text.** Words, `"phrases"`, `avoc*` for a prefix, `-word`, `or`, and
  parentheses. Free text matches the row's own text, one meaning applied per
  row type: on Contacts the contact's name and handles; on Conversations the
  title and the participants' names and handles; on Messages the body,
  subject, and attachment names through full-text search.

### Words

Value types: **T** text, **N** a named thing, **P** a person, **E** one of a
fixed set, **D** date, **C** count, **Z** size, **F** yes or no. Lists:
**C** Contacts, **V** Conversations, **M** Messages. `-` negates any field.

| Word | Type | Means | Values | Lists |
|---|---|---|---|---|
| *(free text)* | T | the row's own text, see above | words, `"phrase"`, `pre*` | C V M |
| `body:` | T | message body only (contains) | text, `none`, `any` | V M |
| `subject:` | T | subject line only | text, `none`, `any` | V M |
| `name:` | T | a person's name: this contact, or a participant | text, `none`, `any` | C V M |
| `title:` | T | the conversation's title | text, `none`, `any` | V M |
| `handle:` | T | a phone number, email, or username | text, `none`, `any` | C V M |
| `with:` | P | this person is a participant | name, handle, `#id` | V M |
| `from:` | P | this person sent it | `me`, name, handle, `#id` | M |
| `to:` | P | it was sent to this person | `me`, name, handle, `#id` | M |
| `in:` | N | this one conversation | title, handle, `#id` | M |
| `group:` | N | in this Contact Group: the contact itself, or a participant | name, `#id`, `none`, `unknown` | C V M |
| `tag:` | N | the conversation carries this Message Tag | name, `#id`, `none` | C V M |
| `kind:` | E | the conversation's shape | `direct`, `group` | C V M |
| `service:` | E | the transport that carried the message | `imessage`, `sms`, `mms`, `rcs`, `whatsapp` | C V M |
| `source:` | E | the backup family it was imported from | `imessage`, `whatsapp`, `sms` | V M |
| `import:` | N | brought in by this Import Run | `#id`, `last` | V M |
| `date:` | D | when a message was sent; on C and V, has a message then | date | C V M |
| `first-message:` | D | the date of the earliest message | date | C V M |
| `last-message:` | D | the date of the latest message | date | C V M |
| `attachment:` | E | what is attached | `image`, `video`, `audio`, `document`, `pdf`, `contact`, `other`, `any`, `none` | V M |
| `filename:` | T | an attachment's file name | text, `pre*` | V M |
| `size:` | Z | an attachment's size | `>1M`, `<500k`, `100k..2M` | V M |
| `messages:` | C | how many messages | `>100`, `0`, `1..10` | C V |
| `conversations:` | C | how many conversations | count | C |
| `groups:` | C | how many Contact Groups | count | C |
| `participants:` | C | how many people in the conversation | count | V M |
| `attachments:` | C | how many attachments on the message | count | M |
| `trashed:` | F | in the trash | `yes`, `no` (default), `any` | C V |

Twenty-seven words plus free text, eight value types, three structural words (`or`,
`not`/`-`, parentheses), and six universal keyword values (`none`, `any`,
`me`, `unknown`, `today`, `yesterday`).

The rule that keeps the table small: **singular is membership, plural is a
count.** `group:Family` and `groups:>5`; `tag:Work` and `messages:>0`.

Notes on meaning:

- `unknown` for `group:` is the computed Contact Group of contacts the vault
  could not identify: no identity, or identities and no preferred name. It is
  not stored, so it compiles to the existing `UNKNOWN_CONTACT_SQL` predicate.
- `me` is the account's own handles. `from:me` is a message the account
  sent, `to:me` one it received. `to:alice` is a message alice did not send in
  a conversation she is a party to.
- `attachment:document` covers PDF, word-processing, spreadsheet,
  presentation, and plain-text types. `pdf` is `application/pdf` alone.
  `contact` is a vCard. `other` is anything not in the other categories.
- `import:last` is the account's most recent Import Run.
- Trashed conversations and trashed contacts are excluded unless `trashed:`
  says otherwise. `messages.duplicate_of IS NULL` is applied on Messages
  unless the query uses `source:`: a query about one backup wants that
  backup's copies, duplicates included, which is what the export routes
  did before.

### Rejection

Any token that is not a word for the requested list answers 400 with a
message that names the word and the list. The message is user-facing and
appears under the search box. There are two cases. A word that exists but not
on this list says where it does work. A word that does not exist at all is an
unknown word. In both cases the only help offered is a "did you mean",
computed by edit distance against the current word list for the requested
list, and only when a word is within two edits. The module carries no memory
of spellings that came before it. Examples of the wording:

| Typed | On | Message |
|---|---|---|
| `from:jane` | Contacts | `from: is not a Contacts word. It works on Messages.` |
| `people:Family` | any | `people: is not a search word.` |
| `paticipants:>2` | Conversations | `paticipants: is not a search word. Did you mean participants:?` |
| `tag:` | any | `tag: needs a value, for example tag:Holiday or tag:none.` |
| `date:2019-13` | any | `date: does not understand 2019-13. Write a year, a month like 2024-05, a day, or a span like 7d, with >, >=, <, <=, or a..b.` |

The "did you mean" uses a small edit distance against the words of the
requested list, and is omitted when nothing is close.

### Presentation is not in the language

None of these narrows a result set, and a Saved Search should mean "what I
want", not "how the screen looked", so none of them is a word:

| Was in the string | Now | On |
|---|---|---|
| `sort:date-asc`, `sort:relevance` | Conversations already takes `sort` and `order` query parameters. Messages export always answers oldest first, as it does today. | Conversations, Messages |
| `group:none` (one row per message) | Nothing. The old parser accepted it, but no server code ever read it and no client sent it. If row grouping is wanted later it is a request parameter. | Messages |
| `context:2` | Nothing, for the same reason. | Messages |
| `search:contacts` | Nothing; the web switches its own screen. | web only |
| `is:trash` | `trashed:yes` in the language, since it narrows rows. | Contacts, Conversations |
| `?source=` on the export routes | Stays as a request parameter for the desktop pull client. It keeps taking the stored source ids (`imessage`, `whatsapp`, `sms-backup-restore`); the handler maps the id to the word and prepends `source:<word>` to the query before compiling, so it means exactly what the word means. Any other value is a 400. | Messages |

## The module

### Interface

`crates/vault/server/src/search/mod.rs` is the whole seam. Two entry points.

```rust
pub enum ListKind { Contacts, Conversations, Messages }

/// Everything compile needs that is not in the query string.
pub struct CompileRequest<'a> {
    pub list: ListKind,
    pub query: &'a str,
    pub account_id: &'a str,
    pub engine: DbEngine,
    pub today: chrono::NaiveDate,
}

/// One boolean expression plus the values it binds. Opaque.
pub struct Filter { /* private */ }

impl Filter {
    /// One parenthesised expression with `?` placeholders. Never empty:
    /// an empty query compiles to the defaults alone.
    pub fn where_sql(&self) -> &str;
    /// The values to bind, in the textual order of `where_sql`.
    pub fn params(&self) -> &[SqlParam];
}

/// Parse `query` and compile it for `list`. Pure: no database, no clock.
pub fn compile(req: CompileRequest<'_>) -> Result<Filter, QueryError>;

/// The words for one list: spelling, value type, keyword values, one line
/// of help, an example. Served as JSON so the web and the docs read it.
pub fn describe(list: ListKind) -> Vec<FieldDoc>;

pub struct QueryError {
    pub kind: QueryErrorKind,        // UnknownWord | WrongList | BadValue | EmptyValue | Unbalanced | TooLong | TooComplex
    pub message: String,             // the 400 body; user-facing
    pub span: std::ops::Range<usize>,// byte range in the input
    pub field: Option<&'static str>,
    pub did_you_mean: Option<&'static str>,
}
```

Invariants a caller relies on:

1. The fragment refers to exactly one base alias, fixed per list: `ct` for
   `contacts`, `c` for `conversations`, `m` for `messages`. Subqueries name
   their own aliases (a Messages fragment reaches its conversation as `c`
   inside its own `EXISTS`), and never reach the caller's. Everything else is
   a correlated subquery naming its own tables. So the caller adds no joins
   and no `DISTINCT`; the fragment can never multiply the caller's rows.
2. Account scope, the dedupe rule, and the trash default sit inside the
   fragment. The caller never writes `account_id = ?` for the base row.
3. Placeholders are `?`. Postgres callers run `db::sql::renumber_placeholders`
   on the finished statement, as today. `params()` splice where `where_sql()`
   splices, in order.
4. Compile is deterministic: the same inputs give the same SQL byte for byte.
5. Errors are total. Every token compiles or yields a `QueryError`; there is
   no third outcome, and no rows are queried on an error.
6. Call `compile` once per request and reuse the `Filter` for the page query
   and the count query.

### Usage

```rust
// contacts_api::list_contacts
let f = search::compile(CompileRequest { list: Contacts, query: q, account_id, engine, today })?;
let sql = format!(
    "SELECT ct.id, ct.preferred_name FROM contacts ct WHERE {} {order} LIMIT ? OFFSET ?",
    f.where_sql());
let mut params = f.params().to_vec();
params.push(limit.into()); params.push(offset.into());

// conversations_api::list_conversations_sorted
let f = search::compile(CompileRequest { list: Conversations, .. })?;
let sql = format!("SELECT c.id, c.group_title FROM conversations c WHERE {} ORDER BY {order} LIMIT ? OFFSET ?", f.where_sql());
// `sort` and `order` stay function arguments

// export_api::export_messages and export_message_count share one filter
let f = search::compile(CompileRequest { list: Messages, .. })?;
let count = format!("SELECT count(*) FROM messages m WHERE {}", f.where_sql());
let page  = format!("SELECT ... FROM messages m WHERE {} AND {cursor} ORDER BY ... LIMIT ?", f.where_sql());
```

The route handlers map `QueryError` to `ApiError::BadRequest(message)` at the
edge. `ExportQueryError` is deleted; `search_query.rs` no longer depends on
`export_api.rs`.

### HTTP

One new route, so the web's suggestions and the docs page read the server's
own word list:

| Method | Path | operationId | Answers |
|---|---|---|---|
| GET | `/v1/search/fields?list=contacts\|conversations\|messages` | `search_fields_list` | `{items: [FieldDoc]}` |

`FieldDoc` is `{word, value_type, values: [string], help, example}`.
`values` is the keyword list for E-type words and the universal keywords a
T or N word accepts. The route sits in a new `search_api.rs`, following the
one-file-per-route-group rule, and is added to `openapi.rs` with the OpenAPI
document and TypeScript types regenerated.

No other route changes. The export routes keep their `source` parameter;
the handler turns it into a leading `source:` word.

### Behind the seam

**The registry.** One `&'static [FieldSpec]`, each entry a spelling, a value
type, the lists it applies to, one line of help, and an example; and one
emitter per word that writes its SQL. `describe` and `compile` read the same
table, so a word cannot exist for one and not the other, and adding a filter
later is one entry and one emitter with no grammar change. A test compiles
every word on every list it claims, so an entry without an emitter cannot
ship.

**Bridges, not per-list copies.** A `ListCtx` gives every emitter three
phrases written once per list: *this contact*, *this conversation*, *this
message*. On Messages, "this conversation" is `m.conversation_id`. On
Contacts, it is an EXISTS over `contact_handles` and `participants` back to
`ct.id`. Twenty-seven words across three lists cost twenty-seven emitters plus
three bridges, not eighty-four branches.

**The two former lookups become subqueries.** A Contact Group name no longer
resolves to member ids first: it is
`EXISTS (SELECT 1 FROM contact_group_members cgm JOIN contact_groups cg ON cg.id = cgm.group_id WHERE cg.account_id = ? AND {name_eq_ci} AND cgm.contact_id = <this contact>)`,
with `#id` swapping the name test for `cg.id = ?`. `first-message:` and
`last-message:` compare a correlated
`(SELECT MIN(m2.timestamp) FROM messages m2 ... WHERE m2.duplicate_of IS NULL)`
to the bound instead of building a contact-id list.

**Engine differences.** `like_ci`, `name_eq_ci`, and `order_by_name_ci` from
`db/dialect.rs`, plus one private `fts_leaf(engine, term, prefix)` that emits
FTS5 `messages_fts MATCH` on SQLite or `search_tsv @@ to_tsquery('simple', …)`
on Postgres, with the `plainto_tsquery` fallback, wrapped in the same
metadata LIKE chain both engines use today. Nothing else branches on engine.

**Also hidden.** Tokenizing, quoting, case folding, date and size arithmetic,
comparison folding (`date:>=2024-01 date:<2024-06` becomes one BETWEEN), the
dedupe and trash defaults, and the wording of every 400.

**Module layout.** `search/mod.rs` (the interface), `lex.rs`, `parse.rs`,
`value.rs` (dates, sizes, counts), `fields.rs` (the registry), `bridge.rs`,
`emit.rs`, `fts.rs`, `error.rs`, `tests.rs`. `search_query.rs` is deleted.
The desktop pull client and the search-parity integration test keep calling
`export_messages`; only the old parser's re-export goes.

### Tests at the interface

One fixture seeded into the SQLite test vault from `test_support.rs`, and a
table of cases in the form "query this on this list, expect these ids". The
old parser tests inside the three route files are deleted, not kept; the
interface is the test surface. Cases the fixture must cover:

1. Contacts, `group:none messages:>0`: Ana (in Family, 5 messages), Bo (no
   group, 3 messages), Cy (no group, 0 messages). Expect Bo.
2. Messages, `date:2024-01..2024-03 attachment:image size:>500k`: Feb 2024
   with a 900k JPEG, Feb 2024 with a 100k JPEG, Feb 2024 with a 2M PDF, May
   2024 with a 900k JPEG. Expect the first.
3. Conversations, `participants:>2 -tag:Archive`: a 3-person conversation
   tagged Archive, a 4-person untagged, a direct one. Expect the 4-person one.
4. Messages, `from:me to:"Jane Doe" (avocado or "guacamole night")`: mine to
   Jane saying avocado; mine to Jane saying guacamole night; Jane's to me
   saying avocado; mine to Sam saying avocado. Expect the first two.
5. Contacts, `first-message:<2020 last-message:>=2024-01-01 handle:@gmail.com`:
   a gmail contact first messaged 2018 and last 2024; a gmail contact first
   messaged 2021; an icloud contact first messaged 2018, last 2024. Expect the
   first.
6. Contacts, `last-message:<2022`: "who have I not heard from since 2022".
   A last messaged 2021-03-02, B last messaged 2023-01-01, C never. Expect A.
7. Every list, `trashed:yes` and the default: a trashed conversation appears
   only with `trashed:yes` or `trashed:any`.
8. Rejection: `from:me` on Contacts fails with `WrongList`, a span over
   `from:`, `field` set to `from`, and no suggestion, since no Contacts word
   is within two edits. `people:Family` fails on every list with
   `UnknownWord` and no suggestion. `paticipants:>2` on Conversations fails
   with `UnknownWord` and `did_you_mean: Some("participants")`. `tag:` fails
   with `EmptyValue`. `(a or b` fails with `Unbalanced`. No rows are queried
   in any of these.
9. Determinism: compile the same request twice and assert identical SQL.
10. Registry coverage: every `FieldSpec` appears in `describe()` for each list
    in its `ListSet`, and every word in the docs page's table is a registry
    entry. The docs test reads `docs/src/content/docs/vault/user/how-to/search.md`
    and fails on a word the registry does not know, so the page cannot drift.

Both engines: the SQLite fixture runs in the ordinary test suite. The
Postgres branch of `fts_leaf` is covered by the existing Postgres dev script
when it is available, not by CI.

## The web

- `contactGroups.ts:90` keeps `queryToken: "group"`; `messageTags.ts:55`
  keeps `"tag"`. `nameCollection.ts:100-110` keeps `token:none` and quoted
  names, and may send `token:#id` now that named sets have ids.
- Conversation screens that build `people:` send `group:`.
- The operator regexes in `ContactList.tsx:56`, `ConversationList.tsx:57,147`,
  and `contactGroups.ts:121-128` are deleted. The one rule that replaces them:
  a query containing any `word:` token is sent to the server, and the
  client-side name filter applies only to plain words. The word list comes
  from `GET /v1/search/fields` through TanStack Query, cached per account
  like everything else.
- `useSearchSuggestions.ts:5` drops its hard-coded four operators and reads
  the same route.
- `buildAdvancedQuery.ts` writes the new spellings: `kind:direct`,
  `messages:>0`, `messages:0`, `name:none`, `handle:none`, `date:>=`,
  `first-message:`, `last-message:`, and it stops pushing `search:contacts`.
- `TrashScreen.tsx:18` and `AppLayout.tsx:183` send `trashed:yes` instead of
  `is:trash`.
- The thread view builds `in:#<id>` and `in:#<id> date:<year>`, and the
  contact drawer builds `with:#<id>` or `handle:"…"` plus `kind:`.
- A 400 from a list request is shown under the search box, as today.

## The docs

`docs/src/content/docs/vault/user/how-to/search.md` is rewritten from the
table in this spec: one table of words with the lists each applies to, the
value rules above it, and the presentation parameters gone. Test case 10
keeps it honest.

## Not changing

- URLs of the three list routes, their pagination, and their response shapes.
- The schema. No table, column, or index changes. If the correlated
  `first-message:` and `last-message:` subqueries prove slow on a large vault,
  an index on `messages (conversation_id, timestamp)` is the fix, and it is
  not needed for a one-person vault today.
- Saved Search storage and routes.
- Web routes `/group/{slug}` and `/tag/{slug}`.

## Not built now

- Cursor-aware suggestions, plain-sentence explanations, and a validate-only
  call from the server. The web builds suggestions from `describe`. If a
  second caller appears, each is one function over the registry.
- Row grouping and context lines for message search. Nothing implements them
  today. If wanted, they are request parameters, not words.
- A rewriter for stored Saved Search text, or any other memory of earlier
  spellings. Stored text the language does not understand fails as an
  unknown word, and the person edits once.
- "Sent by a member of this Contact Group" (Fastmail's `fromin:`). If wanted,
  it is a value form on `from:` and `to:`, not a new word.
- The three route files moving out of `*_api.rs` into resource modules. That
  is a mechanical move once the shared parsing and SQL are gone, and it is a
  separate change.
