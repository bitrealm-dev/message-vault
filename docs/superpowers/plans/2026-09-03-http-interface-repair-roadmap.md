# HTTP Interface Repair: Roadmap

The design in `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`
ships as eight pull requests, one at a time, in this order. Each one gets its
own step-by-step plan in this folder when its turn comes, written against the
code as it is after the previous one merged, so the plans do not go stale.

| # | Pull request | Plan | Spec section |
| --- | --- | --- | --- |
| 1 | Import failures typed; `source` contract; schema docs say 4 | `2026-09-03-import-failures-and-schema-docs.md` | Import failures |
| 2 | One convention for every route: offset paging, `{items,total,limit,offset}`, `{error}`, integer ids, `source=` off Export, camelCase off Saved Searches; `vault-pull` on offset paging; `compose_query` deleted | `2026-09-03-route-convention.md` | Interface convention (ADR-0005) |
| 3 | An import names the Contact; one participant loader; `contact_handles.name_alias` and `contact_name_mode` deleted; drawer updated | to write | Names (ADR-0006) |
| 4 | `GET /v1/conversations/{id}` and `/messages`; message screen on TanStack Query with error and empty states; `fetchConversationById` deleted; tapbacks rendered; phantom fields deleted; tests type-checked | to write | Conversation read routes |
| 5 | Trash module; trash and restore for conversations and contacts; `trashed_handles` dropped; web actions | to write | Trash |
| 6 | One query builder on the web; shared example file checked by both sides; `api.md` rewritten | to write | Query text on the web |
| 7 | One test fixture; route-level tests for Export and the read routes | to write | Tests and fixtures |
| 8 | Contact Group and Message Tag route files folded into the named-set module | to write | Named sets |

Out of scope, filed as issues: searching messages across conversations (#313),
permanent delete and Empty Trash (#314).
