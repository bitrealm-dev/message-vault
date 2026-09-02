# Resources are addressed by id on the HTTP interface

Every vault resource that has a row id is addressed by that id in the URL:
`PATCH /v1/contact-groups/{id}`, `DELETE /v1/message-tags/{id}`,
`PATCH /v1/contact-groups/{id}/members`. Names are for people. They appear in
the web app's routes (`/group/family`), in the search language
(`within:Family`, `tag:Holiday`), and in the text of a Saved Search, and the
web app turns a name into an id inside the feature module that owns the
collection, never in a screen.

## Why

Contact Groups and Message Tags were the exception. Both tables have an integer
primary key that their membership tables reference, but the HTTP interface
never showed it: rename was `PATCH /v1/contact-groups {from, to}`, delete was
`DELETE /v1/contact-groups {name}`, and membership was
`POST /v1/contacts/groups {ids, name, enable}`. The key was a mutable string
with spaces and unicode, matched case-insensitively on every request, and a
rename re-keyed the resource. Saved Searches, with the same table shape, used
`/v1/saved-searches/{id}` from the start, so the API had two rules for one
kind of thing.

## Considered and rejected: the name in the path

`PATCH /v1/contact-groups/{name}` keeps every reference to a group the same
kind of thing, since web routes and search text also use names. It was
rejected because the identifier stays mutable, must be URL-encoded, and must be
matched case-insensitively on every request, and because it makes the API's
rule "id, except where the name is unique". One rule is worth more than the
symmetry.

## Consequences

- A rename is an update to the `name` field of a resource addressed by id.
- Membership lives under the collection it belongs to:
  `GET` and `PATCH /v1/contact-groups/{id}/members`, not `/v1/contacts/groups`.
- Web routes, the search language, and Saved Search text stay name-based. A
  rename still changes the slug and still breaks a Saved Search that names the
  old name, as before.
- The reserved-name list stays, because it protects the web routes and the
  search tokens, not the API.
