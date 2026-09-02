# Contact Groups and Message Tags addressed by id

## Problem Statement

Contact Groups and Message Tags are the same feature over different nouns: a
named set the account owns, and a membership that puts contacts or
conversations in or out of it. On the HTTP interface they are the only
resources whose key is the display name, carried in the request body:

| Today | What it does |
| --- | --- |
| `PATCH /v1/contact-groups {from, to}` | rename |
| `DELETE /v1/contact-groups {name}` | delete |
| `GET /v1/contact-groups/members?name=` | list member ids |
| `POST /v1/contacts/groups {ids, name, enable}` | add or remove members |

and the same four for message tags, with the membership write at
`POST /v1/conversations/tags`. Both tables already have an integer primary key
that the membership tables reference; the HTTP layer resolves name to id on
every call and never shows the id. Saved Searches, with the identical table
shape, use `/v1/saved-searches/{id}`.

The membership read and write for one collection live under different
prefixes, and the write sits beside `/v1/contacts/{id}`, which is why
`server.rs` carries a test whose only purpose is to prove the `{id}` route does
not swallow it.

`contact_groups_api.rs` and `message_tags_api.rs` are near-copies: six
handlers and eight request and response types each, differing by a noun swap.
The shared logic already lives behind `MembershipSpec` in
`named_membership.rs`; only the HTTP shell is duplicated. Neither file has a
test that sends an HTTP request, so the status codes the OpenAPI document
promises for these twelve routes are unverified.

On the web side, responses come back in five shapes. Create and rename answer
`{name, groups}` (the whole list), delete answers `{ok, groups}`, membership
answers `{changed}`, and the list answers `{groups: string[]}`. The feature
module unwraps them with a per-collection `responseKey`. After a rename it
writes the echoed list into its own cache and touches nothing else, so every
contact row and conversation row keeps showing the old chip name until it
remounts.

## Solution

Both collections are addressed by id, membership lives under the collection,
and one module implements the HTTP surface for both. Decision record:
`docs/adr/0003-resources-are-addressed-by-id-on-the-http-interface.md`.

### Routes

Six per collection. The message-tag six are identical with `message-tags`,
`message_tags`, and `MessageTag` in place of the group names.

| Method | Path | operationId | Body | Answers |
| --- | --- | --- | --- | --- |
| GET | `/v1/contact-groups` | `contact_groups_list` | | `200 {items: [NamedSet]}` |
| POST | `/v1/contact-groups` | `contact_groups_create` | `{name}` | `200 NamedSet` |
| PATCH | `/v1/contact-groups/{id}` | `contact_groups_update` | `{name}` | `200 NamedSet` |
| DELETE | `/v1/contact-groups/{id}` | `contact_groups_delete` | | `204` |
| GET | `/v1/contact-groups/{id}/members` | `contact_group_members_list` | | `200 {items: [i64]}` |
| PATCH | `/v1/contact-groups/{id}/members` | `contact_group_members_update` | `{add: [i64], remove: [i64]}` | `200 {added, removed}` |

`POST /v1/contacts/groups`, `POST /v1/conversations/tags`,
`GET /v1/contact-groups/members`, and `GET /v1/message-tags/members` are
removed. No response carries an `ok` field.

Shared schemas, used by both collections:

- `NamedSet { id: i64, name: string }`
- `NamedSetList { items: NamedSet[] }`
- `NamedSetBody { name: string }`
- `MemberIdList { items: i64[] }`
- `MembersPatch { add: i64[], remove: i64[] }` (both default to empty)
- `MembersChanged { added: u64, removed: u64 }`

Status codes:

- `400` on an empty name, a name over 80 characters, a reserved name, or a
  members patch whose `add` and `remove` are both empty.
- `404` when `{id}` is not a set of this account, or when a member id in a
  patch is not a row of this account. Nothing is written when any id fails.
- `409` when create or update would give two sets of one account the same name,
  ignoring case. A case-only change of the same name is allowed.

All twelve routes take `FullAccess`, as today.

### Server module shape

`named_set_api.rs` holds the six shared schemas and six functions, each taking
the `&'static MembershipSpec` and the `AppState`, account id, and parsed input:

```rust
pub(crate) async fn list(spec, state, account_id) -> Result<Json<NamedSetList>, ApiError>
pub(crate) async fn create(spec, state, account_id, body) -> Result<Json<NamedSet>, ApiError>
pub(crate) async fn update(spec, state, account_id, id, body) -> Result<Json<NamedSet>, ApiError>
pub(crate) async fn delete(spec, state, account_id, id) -> Result<StatusCode, ApiError>
pub(crate) async fn members_list(spec, state, account_id, id) -> Result<Json<MemberIdList>, ApiError>
pub(crate) async fn members_update(spec, state, account_id, id, body) -> Result<Json<MembersChanged>, ApiError>
```

`contact_groups_api.rs` and `message_tags_api.rs` keep one handler per route,
each three lines: the `#[utoipa::path]` attribute, the extractor list, and a
call into the shared function. utoipa needs a concrete function per path, so
this is the smallest shell that keeps every path greppable.

`named_membership.rs` gains id-addressed functions beside the name-addressed
ones it has, because the import path and several tests still create and fill
groups by name:

```rust
pub async fn list_sets(spec, conn, account_id) -> Result<Vec<(i64, String)>, MembershipError>
pub async fn get_set(spec, conn, account_id, id) -> Result<(i64, String), MembershipError>
pub async fn rename_set(spec, conn, account_id, id, name) -> Result<String, MembershipError>
pub async fn delete_set(spec, conn, account_id, id) -> Result<(), MembershipError>
pub async fn list_member_ids_of(spec, conn, account_id, id) -> Result<Vec<i64>, MembershipError>
pub async fn patch_members(spec, conn, account_id, id, add, remove) -> Result<(u64, u64), MembershipError>
```

`rename_name`, `delete_name`, `list_member_ids`, and `set_membership` (the
name-addressed versions) are deleted once nothing but the old handlers calls
them; `set_membership` stays because the import path and the contact and
conversation test setups use it. The `on_change` hook fires per changed member
in `patch_members` exactly as it does in `set_membership`.

`MembershipChangedResponse` leaves `server.rs`.

### Web route functions

In `vaultApi.ts`, following the naming rule candidate D proposed for the whole
API. Every function takes `opts?` last, writes included.

```ts
listContactGroups(opts?)                       // GET    /v1/contact-groups
createContactGroup(body, opts?)                // POST   /v1/contact-groups
updateContactGroup(id, body, opts?)            // PATCH  /v1/contact-groups/{id}
deleteContactGroup(id, opts?)                  // DELETE /v1/contact-groups/{id}
listContactGroupMembers(id, opts?)             // GET    /v1/contact-groups/{id}/members
updateContactGroupMembers(id, body, opts?)     // PATCH  /v1/contact-groups/{id}/members
```

and the six `…MessageTag…` equivalents. `renameContactGroup`,
`setContactGroupMembership`, `renameMessageTag`, and `setMessageTagMembership`
are removed. `api.ts` answers `undefined` for a `204` instead of trying to
parse a body.

### Web feature module

`nameCollection.ts` keeps a name-based interface. Screens, the sidebar, and the
router never see an id.

```ts
useNameCollection(collection): { names: string[]; loading: boolean }

useNameCollectionActions(collection): {
  create(name): Promise<string>;                       // answers the created name
  rename(from, to): Promise<string>;                   // answers the new name
  remove(name): Promise<void>;
  setMembers(name, { add?: number[]; remove?: number[] }): Promise<{ added; removed }>;
  invalidate(): Promise<void>;
}
```

Name to id resolution happens inside the module. It reads the cached
`NamedSet[]` for the collection; on a miss it fetches the list once through the
query client and looks again; if the name is still absent it throws
`"<label> not found"` without sending a request. The miss path covers the
create-then-add flow in the contact and conversation lists, which calls
`create(name)` and then `setMembers(name, …)` before the invalidated list has
come back.

After every write the module invalidates its own list and the lists that show
the name as a chip:

| Collection | Invalidates |
| --- | --- |
| Contact Groups | `["contact-groups"]`, `["contacts"]`, `["contact-detail"]` |
| Message Tags | `["message-tags"]`, `["conversations"]`, `["trash-count"]` |

TanStack Query matches keys by prefix, so `["contacts"]` covers every page and
every search of the contact list. The `invalidates` list is part of the
collection config, next to `cacheKey` and `queryToken`. `responseKey` and
`namesFrom` go away.

The optimistic override systems in `ContactList.tsx` and
`ConversationList.tsx`, and the `membershipRev` counter, are not touched. They
belong to the key factory and `useMutation` work recorded as candidate E in
the architecture review.

`useContactGroups.ts` and `useMessageTags.ts` are unchanged.

### Tests

Server: one test module in `named_set_api.rs` drives the real router through
`test_support.rs` for both collections. Each case is a function taking the
base path and member table, run once per collection: create, list sorted A–Z,
update to a new name, case-only rename, 409 on a duplicate, 400 on a reserved
name, 404 on an unknown id, delete answers 204 and the members are gone,
members patch adds and removes in one call, 404 on a foreign member id writes
nothing, and every route refuses another account's id with 404. The DB-function
tests currently in the two route files move to `named_membership.rs`.

Web: `nameCollection.test.ts` mocks the six route functions and asserts at the
interface: `rename("Family", "Fam")` calls `updateContactGroup(12, {name:
"Fam"})` and invalidates the three keys; `setMembers` with an unknown name
throws without a request; `setMembers` after the list cache misses fetches
once and then calls the route. `vaultApi.test.ts` asserts the twelve URLs.
Existing component tests that mock the old route functions are updated to the
new names and shapes.

### What else changes

- `docs/src/assets/openapi.json` and `web/src/lib/vaultApi.types.ts` are
  regenerated.
- The `literal_contact_routes_are_not_captured_by_the_id_route` test in
  `server.rs` drops `/v1/contacts/groups` from its list and its doc comment.
- `openapi.rs` registers the twelve handlers under their new names.

### Not changing

- Web routes stay `/group/{slug}` and `/tag/{slug}`; the search language stays
  `within:`, `group:`, `tag:`, `label:`; Saved Search text is untouched.
- The reserved-name lists on server and web stay.
- `kind` on `contact_groups` stays off the wire.
- Import-created groups are still made by name inside the import path.
- Any route outside these twelve.
