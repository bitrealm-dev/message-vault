# Saved searches in the vault — 2026-08-30

> Superseded in part by PR #294: Contact Groups and Message Tags are addressed by id; `POST /v1/contacts/groups` no longer exists.

Move saved searches out of browser storage and into the vault database, so
that a saved search belongs to an account rather than to a browser profile
on one machine.

This spec records decisions from the 2026-08-30 design conversation. It is
not an implementation plan.

## Goal

A person writes a search they want again later, saves it, and finds it
still there when they open the vault from a different browser or a
different machine. When the vault's data is wiped, their saved searches go
with it, because saved searches are vault data.

## Current product

Saved searches are the only one of the three sidebar collections that never
reaches the server. `web/src/lib/savedGroups.ts` keeps them in
`localStorage` under the key `mv-saved-groups` as an array of
`{id, name, query}`. Contact groups and message tags both live in the vault
database, share one server implementation in `named_membership.rs`, and are
served per account.

Four consequences follow from that split, and all of them are bugs.

**A vault reset does not reset them.** `./scripts/run-vault-dev.sh --reset`
runs `rm -rf data`, which removes `data/vault.db` and cannot reach browser
storage. Saved searches survive the wipe that removes everything they point
at. This is what prompted the conversation.

**They do not follow the account.** They are invisible from a second
machine, a second browser, and a second browser profile. They are also
split by origin: the set on `http://127.0.0.1:5173` (Vite) is a different
set from the one on `http://127.0.0.1:8080` (the vault-served app), and
`localhost` splits them again.

**They outlive the session that made them.** `auth.tsx` removes only the
`message-vault-auth` key on logout, so one person's saved searches stay
visible to the next person who signs in on that browser.

**Import entries accumulate with no way to manage them.**
`useImportJob.ts` calls `saveImportSavedGroup` after every import that
inserted at least one message, appending `Import <source> <YYYY-MM-DD>`
with the query `import:<id>`. Nothing caps or prunes the list, nothing
distinguishes a machine-made entry from a hand-written one, and the entry
is written by a browser that may close before the import finishes.

One smaller fault found while tracing this: the query field's placeholder
in `SavedGroupForm.tsx` reads `e.g. from:bob service:discord`. A saved
search runs against the conversation-list grammar in `conversations_api.rs`,
which has no `from:` operator — that belongs to the message-search grammar
in `search_query.rs` — and accepts only `phone` and `whatsapp` as
`service:` values. The example we ship matches neither parser.

## Vocabulary

The three sidebar collections had drifted to seven names between them. The
canonical names are now fixed, and `CONTEXT.md` records them:

- **Contact Group** — a named collection of contacts, referenced from a
  search so a query can name a set of people without listing them.
  Originally called just "Group", which is the source of most of the
  naming confusion in the codebase.
- **Saved Search** — a named query, stored so it can be run again. It
  holds no members, and returns different results as messages arrive.
- **Message Tag** — a name marked onto conversations.

## Decisions

### 1. A real table, not the key/value store

`schema/sql/accounts.sql` already declares an `account_prefs` table that no
Rust file has ever read or written. A saved-search list is JSON-shaped, so
one blob row there would have worked and would have touched no DDL.

Rejected. The vault could not then query, constrain, or count saved
searches, and every write would be a read-modify-write of one blob that two
tabs can race. Storing an opaque array inside SQLite re-creates the problem
being fixed. `account_prefs` stays available for genuine key/value settings.

```sql
CREATE TABLE IF NOT EXISTS saved_searches (
    id         INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    query      TEXT NOT NULL,
    kind       TEXT NOT NULL DEFAULT 'manual',
    UNIQUE(account_id, name)
);
```

A Postgres twin goes in `schema/sql/pg_*.sql` alongside it.

The table is scoped per account with `ON DELETE CASCADE`, matching
`contact_groups` and `conversation_tags`. Deleting an account takes its
saved searches with it.

`kind` holds `manual` or `import` and records how a row was born. Without
it the two kinds are identical columns with different values, and the only
clue is a name the person can change. With it, the sidebar can label them,
group them, or clear the import ones in a single action.

### 2. The schema bump is not a problem to design around

Adding the table bumps `SCHEMA_VERSION` from 4 to 5, and
`rebuild_vault_schema` drops every table and reapplies the DDL, so every
existing vault rebuilds empty and is re-imported.

That is the intended behaviour. Message Vault has no first stable release,
there is no migration, and data preservation between releases, branches,
and pull requests is not a goal. A schema bump costs nothing today, so no
design here trades correctness to avoid one.

The same rule settles what happens to the entries currently in people's
browsers: they are discarded. Reading `mv-saved-groups` and uploading it on
first load would itself be a migration, and it could only ever reach
whichever origin the person happened to open. The key is dropped, and the
few saved searches worth keeping are retyped.

### 3. The API addresses a saved search by id

Contact groups address rows by name in a JSON body — `PATCH
/v1/contact-groups` takes `{from, to}`, `DELETE /v1/contact-groups` takes
`{name}` — so the URL names the collection and never the row.

Saved searches do not follow that convention, for a reason specific to
them: a contact group carries one field, while a saved search carries
`name` and `query` and the form edits both at once. A name-addressed rename
body would be `{from, to, query}`, where `from` identifies the row and `to`
overwrites the field that identified it.

Consistency does not argue against this as strongly as it first appears.
The shared behaviour lives in `web/src/lib/nameCollection.ts`, whose
`createNameCollection` builds both `contactGroups` and the message tags
client from one config — and its type cannot hold a saved search at all.
`fetchAll` returns `string[]`, `create` takes one string, `rename` takes
two. Saved searches need their own client module whichever addressing is
chosen, so copying the convention would buy a surface resemblance and no
shared code.

| Method | Path | Body |
|---|---|---|
| `GET` | `/v1/saved-searches` | — |
| `POST` | `/v1/saved-searches` | `{name, query}` |
| `PATCH` | `/v1/saved-searches/{id}` | `{name, query}` |
| `DELETE` | `/v1/saved-searches/{id}` | — |

Two halves of the contact-groups convention do transfer and are kept: every
handler resolves auth and calls `require_full_access`, and every mutation
returns the refreshed list so the client never issues a follow-up `GET`.

### 4. The server returns the list A–Z

Today's order is insertion order, which is an accident of `groups.push()`
followed by `groups.map()` rather than a feature — there is no `order`,
`position`, `created_at`, or `pinned` field in the model, and no way to
rearrange the list. Contact groups and message tags are both returned A–Z.

Three sidebar sections ordering themselves three different ways is worse
than losing an arrangement nobody can make. An explicit ordering column
stays available later as an additive change.

### 5. The server creates the import entry

The client currently mints it. That cannot survive a browser closed
mid-import, its uniquifying loop races itself when two runs finish
together, and a CLI or API import gets no entry at all.

The server writes the entry because the server writes the `vault_imports`
row. A unique constraint settles concurrent runs, and every import path
produces the same result.

The entry is written **on completion, and only when the run inserted at
least one message**. This matters because the `vault_imports` row is
written twice: inserted when a run starts, with `status = 'running'` and
`stage` moving through `parse`, `write`, `awaiting_gate_1`, `transcode`,
`awaiting_gate_2`, `pushing`; then updated when the run ends. Creating the
entry at insert time would produce saved searches for runs the person then
cancels at an approval gate. Creating one for a run that inserted nothing
would produce a saved search that matches nothing.

Failed, cancelled, and empty runs appear in Import History and produce no
sidebar entry.

### 6. The entry and the import record have different lifetimes

This is the point of the split, and it is deliberate:

- The person **can** delete the sidebar entry.
- The person **cannot** delete the `vault_imports` row.
- Deleting the entry must never touch `vault_imports`.

The saved search is a shortcut to the messages a run brought in. The
import record is the account's permanent history of what happened. A
future screen under user settings will track import results and let people
reach those messages directly, at which point the shortcut is purely a
convenience. Import History under Settings → Storage is unchanged by this
work.

### 7. The stored query is not validated

The server stores whatever string it is given.

Two grammars disagree about what is valid. The conversation-list parser
accepts `is:`, `handle:`, `service:`, `contact:`, `import:`,
`participants:`, `people:`, and `tag:`. The message-search parser accepts a
larger, different set, and does not accept `import:` at all — there,
`import:9` lexes as free text. Clicking a saved search always routes to
`/?q=`, which is the conversation-list grammar.

Rejecting on write would refuse queries that work today. The two-grammar
split is a real problem and a larger one than this change should absorb.

The placeholder is corrected in the same work, since it currently teaches a
query that cannot match: `service:whatsapp is:group` replaces
`from:bob service:discord`.

### 8. Nothing else moves out of browser storage

The web app keeps around twenty keys in `localStorage`. Sorted by the
question *would you miss this on another machine?*, they fall into three
tiers: things the person authored (saved searches), choices about how they
read their archive (`conversationSort:v1`, `contactNameSort:v1`,
`mv-use-name-aliases`, and the three `*-recent-searches:v1` keys), and
per-device view state (column widths, collapsed nav sections, theme). The
auth token and the ffmpeg path are machine-local by nature.

This work moves the first tier only. The second tier is a same-shaped
follow-up. Doing all of it at once would turn a small feature into a
preferences subsystem.

## Shipping

Two pull requests, because one has behaviour to review and the other does
not.

**First — the feature.** The table, the API, the server-side import entry,
and the client rewrite: `savedGroups.ts` becomes `savedSearches.ts` talking
to the API, `SavedGroup` becomes `SavedSearch`, `mv-saved-groups` is
dropped, and the placeholder is corrected.

**Second — the rename.** Mechanical, touching many files and changing no
behaviour, so a reviewer can tell it apart from the feature:

- Contact-group code moves off bare "group": `group_spec()`,
  `GroupsNav.tsx`, and the endpoint `POST /v1/contacts/groups`.
- Message tags become `message_tags`, `message_tag_members`,
  `/v1/message-tags`, and `messageTags.ts`. The foreign key still points at
  `conversations`, and `message_tag_members(conversation_id)` reads
  correctly — the product already treats the conversation as the unit
  behind a message-level word.
- The user docs page and the API reference follow.
- `CLAUDE.md:28` describes demo mode as running "through a guest pool
  (`guest_pool.rs`)". That file does not exist, `/v1/auth/try-demo` is
  gone, and `server.rs` holds a regression test named
  `try_demo_route_is_gone`. Demo mode is a seeded ordinary account. The
  line is corrected.
