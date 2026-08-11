# Contact Threads → service + identity search

**Date:** 2026-08-11  
**Status:** Approved for planning  
**Scope:** Contact info handles table Threads links + conversation list query (`web/`, `crates/vault/server`)

## Problem

In the contact info panel, each handle row shows a **Threads** count. Clicking it opens conversation search with `handle:<identity>` only. The same phone number can exist on more than one platform (`phone` vs `whatsapp`). A Text-message row and a WhatsApp row therefore open the same unscoped result set.

The footer summary Threads count (sum of the rows) is plain text. Users expect that number to open every thread covered by those rows. The footer label currently reads **Total**; it should read **Summary**.

Numeric Threads / Direct / Group columns are centered, which makes multi-digit counts harder to compare.

## Goals

- Row **Threads** opens conversation search scoped to that row’s **platform service + identity**.
- Footer **Summary** Threads opens conversation search for **all** of that contact’s identities (every service), so the linked count matches the sum of the row links (e.g. 11 + 1 → 12).
- Right-align Threads, Direct Messages, and Group Messages counts (column headers and cells, including the Summary row).
- Keep the Summary row as a summary footer (label **Summary**, identity **—**, date span across handles).

## Non-goals

- Linking Direct Messages or Group Messages counts.
- Adding a Service field to conversation Advanced Search in this change.
- Changing contact-list (`search:contacts`) operators.
- Schema migrations.

## Decisions

| Topic | Choice |
|--------|--------|
| Row search key | Platform service (`handles.service`: `phone` \| `whatsapp`) + identity (`handles.raw`) |
| Query shape (row) | `handle:<identity> service:<platform>` |
| Query shape (Summary Threads) | `contact:<contactId>` |
| Encoding | Separate `service:` token (not packed into `handle:`) |
| `handle:` alone | Unchanged: match identity on any platform |
| Footer label | **Summary** (replaces **Total**) |
| Summary Direct / Group | Stay non-links (summary numbers only) |
| Count alignment | Right-align Threads, Direct Messages, Group Messages |

## Approach

Extend the conversation list query parser and SQL filter so an optional `service:` token, when combined with `handle:`, requires the matching chat handle or participant handle to use that `handles.service`.

Wire the contact drawer browse path to pass platform service for row clicks, and pass no handle (contact-scoped query) for Summary Threads. Rename the footer label to **Summary**. Right-align the three count columns in the handles table.

## Behavior

| Control | Condition | Search opened | Result meaning |
|--------|-----------|---------------|----------------|
| Row Threads | count &gt; 0 | `handle:<identity> service:<platform>` | Threads where that identity appears on that platform only |
| Footer Summary Threads | count &gt; 0 | `contact:<contactId>` | Threads involving any identity linked to the contact |
| Direct / Group counts | any | — | Not links |

Navigation matches today’s browse flow: close the contact drawer, open the conversations column with visible `q` and API filter `f`.

Example (Albert Jones, same phone on Text message and WhatsApp):

- Text message **11** → `handle:+13157535867 service:phone`
- WhatsApp **1** → `handle:+13157535867 service:whatsapp`
- Summary **12** → `contact:<id>`

## Server

In `conversations_api` conversation list parsing:

- Accept optional `service:<value>` (`phone` or `whatsapp`; case-insensitive).
- When **both** `handle:` and `service:` are present, restrict the existing handle match so the chat handle or participant handle also has `handles.service` equal to that value.
- When only `handle:` is present, keep current behavior (raw identity, any platform).
- When only `service:` is present without `handle:` or `contact:`, ignore `service:` so typed junk does not empty the list unexpectedly.
- `contact:` filtering stays unchanged.

Add unit tests for: same raw on two platforms; `handle:` + `service:phone` vs `service:whatsapp`; `contact:` still returns both; `handle:` alone still returns both.

## UI

- Browse callback gains optional `service` (platform id) alongside `handle`.
- Row Threads: `onBrowse({ kind: "all", handle, service })` → build `handle:… service:…`.
- Summary Threads: `onBrowse({ kind: "all" })` (no handle) → build `contact:<id>` for both visible and API queries (stop preferring a single handle for the aggregate case).
- Footer first-column label: **Summary** (not **Total**).
- `CountCell` on Summary Threads uses the same link styling as row Threads when count &gt; 0.
- Right-align numeric content in Threads, Direct Messages, and Group Messages columns (headers + body + Summary).

## Out of scope / follow-ups

- Conversation Advanced Search UI for `service:`.
- GlobalSearch autocomplete for `service:`.
- Message-transport filters (`sms` / `imessage` / `rcs`) — those live on messages, not on handle platform.

## Testing

- Server: conversation list filter tests as above.
- Manual: open a contact with the same identity on Text message and WhatsApp; click each Threads link and confirm correctly scoped lists; click Summary Threads and confirm the union; confirm footer label is **Summary**; confirm count columns are right-aligned.
