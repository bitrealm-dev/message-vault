# One way to fetch data in the web app

## Problem Statement

Sign in to a vault as one account, look at the sidebar, sign out, then sign in
as a different account: the sidebar shows the first account's Saved Searches.
It keeps showing them until someone adds, renames, or deletes a Saved Search,
or reloads the page. A vault holds many accounts and isolates each account's
data from the others, so a person seeing another account's Saved Searches is
the product breaking its own promise.

That leak is a symptom. The web app has six separate mechanisms for fetching
and remembering vault data, and each one solved the same four problems its own
way: remember an answer, avoid firing an identical request twice at once, tell
other components when the answer changed, and track loading and error state.
Four of the six keep the signed-in account's data in a module-level variable,
so the sign-in and sign-out paths each clear them by hand. Both of those
clearing lists name the same four mechanisms and omit the fifth. Nobody was
careless: the list is maintained by memory, and memory failed in both copies.

Separately, every screen writes its own vault URLs. Fifty-two call sites across
twenty-five files each build a path as a template literal and declare the
response shape inline. Nothing checks either against the vault. A field renamed
on the server compiles cleanly on both sides and fails when a person opens the
screen. Tests do not catch it, because they decide what to return by comparing
URL text, so a renamed route leaves them passing rather than failing.

For anyone working on the codebase, the cost is that adding a screen means
choosing between six mechanisms with no basis for the choice, and that the
honest answer to "who needs to know this changed?" is a list somebody has to
remember.

## Solution

The web app gets one way to fetch vault data and one place that knows the
vault's routes.

TanStack Query becomes the only mechanism for caching, request deduplication,
invalidation, and loading state. All six existing mechanisms are removed. When
something changes, the code names what is now stale and the library refreshes
whichever components are showing it; the four browser events that do that job
today are deleted along with the listener-and-cleanup code in every component
that subscribes to them.

Every cache entry is named with the signed-in account. A second account asks
for an entry that has never been written, finds nothing, and fetches. Serving
one account another account's data becomes impossible rather than merely
unlikely, which is the fix the Saved Searches leak actually calls for.

One module, `vaultApi`, holds one named function per vault route. Screens call
`listConversations` and `renameContact` instead of writing URLs. Response types
are generated from the OpenAPI document the vault publishes, which a server
test already pins byte-for-byte to the running code, so a field renamed on the
server becomes a web build failure that names every screen using it.

The vault's route names are corrected in the same pass. Browsing conversations
and contacts, and editing a contact, currently sit under a URL prefix that says
`export`, which the glossary defines as moving messages out of the vault into
files on disk. Those five routes move to prefixes that name what they do, and
`/v1/export/` keeps only the two routes that are genuinely Export.

## User Stories

1. As someone with a vault, I want the sidebar to show my own Saved Searches after I sign in, so that I never see another account's data.
2. As someone with a vault, I want signing in as a different account to show that account's Contact Groups, Message Tags, Saved Searches, contact details, and profile, so that nothing carries over from the previous session.
3. As someone with a vault, I want a screen I have already visited to appear immediately when I return to it, so that moving around the app does not feel like reloading it.
4. As someone with a vault, I want two parts of a screen that need the same data to cause one request rather than two, so that a large vault stays responsive.
5. As someone with a vault, I want a conversation I trash to disappear from the conversation list and appear in Trash without my reloading anything, so that the screen tells me the truth about what I just did.
6. As someone with a vault, I want the counts on a contact to update after I trash one of their conversations, so that the numbers on screen are not stale.
7. As someone with a vault, I want a contact I rename to show its new name everywhere it appears, immediately, so that I do not have to hunt for stale copies.
8. As someone with a vault, I want a Contact Group I create, rename, or delete to appear correctly in the sidebar and in the contact drawer at the same time, so that two parts of the app never disagree.
9. As someone with a vault, I want a Message Tag I apply to a conversation to be reflected in the sidebar counts, so that tagging feels like it took effect.
10. As someone with a vault, I want the conversation and contact lists to keep their scrolling behaviour exactly as they are today, so that this work does not cost me anything I already rely on.
11. As someone with a vault, I want a failed request to show an error rather than an empty screen, so that I can tell "nothing here" apart from "something went wrong".
12. As someone with a vault, I want a request that is still running to show that it is running, so that I do not think the app has frozen.
13. As someone with a vault, I want the app to recover on its own after the vault becomes reachable again, so that a brief disconnection does not require a reload.
14. As someone with a vault, I want a finished import to make the new conversations, contacts, and tags visible without a reload, so that the import's result is what I see.
15. As someone with a vault, I want the URL of a request to have nothing to do with whether my data is correct, so that a rename in the vault cannot silently break a screen.
16. As a developer on this codebase, I want one documented way to fetch data, so that building a screen does not start with choosing between six mechanisms.
17. As a developer on this codebase, I want the response types to come from the vault's own published description, so that the compiler tells me when the server changed rather than a person discovering it.
18. As a developer on this codebase, I want one file that lists every vault route, so that I can see the whole surface without searching twenty-five files.
19. As a developer on this codebase, I want renaming a route to be a one-file change on the web side, so that route naming is cheap enough to get right.
20. As a developer on this codebase, I want a route whose URL says what it does, so that reading the code does not mislead me about which operation I am looking at.
21. As a developer on this codebase, I want the check that keeps generated types current to run in the same script as the rest of the pre-pull-request checks, so that drift is caught before review rather than after merge.
22. As a developer on this codebase, I want tests to name functions rather than URLs, so that renaming a route makes tests fail rather than quietly stop matching.
23. As a developer on this codebase, I want cache invalidation to be a statement about what is stale, so that I do not have to know which components are listening.
24. As a developer on this codebase, I want no way to accidentally serve one account another account's data, so that account isolation does not depend on remembering to clear something.
25. As a developer on this codebase, I want each pull request in this work to be revertible on its own, so that a problem found later can be isolated.
26. As an agent working in this repository, I want the rule recorded where I will read it, so that I do not add a seventh caching mechanism when a screen needs one.
27. As a reviewer, I want the pull request that adds the route functions to be readable as one idea, so that fifty-two mechanical call-site changes do not hide a decision.

## Implementation Decisions

**One transport module.** A module named `vaultApi` holds one exported function
per vault route. Each function does nothing but issue the request and return
the parsed response. It holds no cache, dispatches no event, and contains no
React hook. Its companion types module is generated and checked in.

**Types are generated, functions are not.** The response and request types come
from the OpenAPI document the vault publishes. The functions themselves are
written by hand, because a generated client names operations after their HTTP
paths and the point of this module is that callers read `renameContact` rather
than a name derived from a URL.

**Generation is guarded like the document it reads.** The vault already has a
test that fails when the committed OpenAPI document differs from what the
running code produces. The web side gets the mirror of that: regenerating the
types must produce no diff, checked by the same script that runs the existing
pre-pull-request checks. Without it the generated file drifts and the guarantee
is worthless.

**The existing HTTP client stays, demoted.** The module that owns the base URL,
the Bearer header, and error-message extraction from a failed response remains,
and `vaultApi` is its only caller. Screens stop importing it.

**Hand-written response types are deleted.** Types describing a vault response
move to the generated module. Types describing something the interface needs
but the vault does not return stay where they are.

**TanStack Query is the only retrieval mechanism.** It replaces `useResource`,
`usePagedList`, `nameCollection`, the hand-written Saved Searches cache,
`contactDetailCache`, and the account-profile cache. All six are removed. Their
per-feature wrappers survive as thin query definitions with no caches of their
own.

**Cache keys carry the signed-in account.** Every key includes the account
identifier, which is already available from the auth context at every call
site. The cache is also emptied on sign-out, to release memory rather than for
correctness.

**Invalidation replaces browser events.** The four custom DOM events used to
announce that a cached list changed are deleted, together with every
`addEventListener` and its cleanup in the components that subscribe.

**Virtualised list components are untouched.** The components that render long
lists take their items as input and never fetch. Only the hook that loads pages
changes, from the hand-written paged loader to the library's paged query, and
its page shape is already `{ limit, offset, signal }` returning items and a
total.

**Route names and verbs are corrected in the same pass.** Browsing
conversations, browsing contacts, fetching contact summaries, fetching one
contact, and editing a contact move off the `export` prefix onto prefixes
naming the resource. Editing a contact becomes a `PATCH`, since it modifies an
existing contact rather than creating something. The two routes that read
messages for Export keep the `export` prefix. This is safe to do bluntly: the
vault already requires a signed-in session for all five moved routes and
accepts an export-scoped API token only on the two that remain, so the prefix
is a naming problem and not an access-control one.

**Three pull requests, split for reviewability.**

1. Route functions and generated types; the vault's route renames and verb
   corrections; the drift check; every call site converted.
2. TanStack Query added; the simple reads and the paged lists converted; the
   two hand-written loading hooks deleted.
3. The four caches converted; the browser events and the contact-detail cache
   deleted; account-scoped keys; the Saved Searches leak closed.

Nothing in this work preserves an existing interface for compatibility. There
are no users, so routes, types, and module layouts change wherever a simpler
result follows.

## Testing Decisions

**A good test here asserts what a person sees or what the vault receives**, not
how the code arranged itself. A test that a screen shows the signed-in
account's Saved Searches is good. A test that a particular cache variable was
cleared is not, because the mechanism it names is the thing being replaced.

**Screens and hooks are tested by faking the named route functions.** A test
says what `listContactGroups` returns; it never mentions a URL. After this
work, no test outside `vaultApi`'s own tests contains a `/v1/` string. Thirteen
test files name one today and stop doing so.

**The route functions are tested against a faked HTTP client**, asserting the
method, path, and query string each one builds. This is the only place URLs
appear in the suite, and it is what stops a route rename from being invisible.

**TanStack Query is not faked.** Component tests render inside a real query
provider with a fresh client per test and retries disabled. Faking the library
would test the fake. Prior art: the existing tests already run real hooks and
fake the module beneath them.

**Two tests are written specifically because they would have caught the leak.**
One signs in as one account, populates the sidebar, signs out, signs in as a
second account, and asserts the second account's Saved Searches appear. One
asserts that a query for a second account does not read an entry written for
the first.

**Tests belonging to deleted modules are deleted with them**, not migrated. The
tests for the two loading hooks and the name-collection factory go when those
modules go. Rewriting tests to fit the new shape is expected work, not a cost
to avoid.

**Prior art for the shapes involved**: existing component tests using Testing
Library with a module faked beneath the component; existing pure-function tests
for the query builder and the gate calculations; and, on the vault side, the
route tests that already cover each route group.

## Out of Scope

- The `useImportJob` hook and the Import screen. That hook is its own
  restructuring, already identified separately. It becomes a caller of the
  route functions in pull request one and is otherwise untouched here.
- The vault's search query grammar, which is parsed in three places on the
  server and built in three places on the web. Separate work.
- Splitting the vault's import module, the contacts route module, or any other
  server module. Only route paths and verbs change on the server.
- The desktop-only Tauri command wrappers. They are not vault HTTP routes and
  keep their current shape.
- Adding a fake HTTP server to the test suite.
- Any change to how attachments are fetched or cached as object URLs.

## Further Notes

The reasoning behind these choices, including the alternatives that were
rejected and why, is recorded as an architecture decision record in this
repository titled "One way to fetch data in the web app". A short pointer to it
sits in the repository's agent instructions, so that the next person or agent
who needs caching on one screen finds the rule before writing a seventh
mechanism. That pointer currently marks the rule as decided but not built, and
the clause saying so should be removed when pull request one lands.

The Saved Searches leak exists in the default branch now and is closed in the
third pull request. If that is too long to carry it, adding the missing call to
both clearing lists is a one-line change that can ship on its own; it does not
change anything else in this plan.

The generated types read an OpenAPI document that lives in the documentation
tree rather than under the web application, which is why the drift check is
worth having rather than optional.
