# Web writes through TanStack Query

## Problem Statement

Reads moved onto TanStack Query in #290–#293. Writes did not. There is not one
`useMutation` call in `web/`: every write is a hand-rolled `async` function
with its own `busy` flag and error string, and what goes stale after it is
decided screen by screen.

Query keys are literals typed where they are used, with no shared prefix:

| Key | Where |
| --- | --- |
| `["contact-groups"]`, `["message-tags"]` | `lib/nameCollection.ts`, through the two configs |
| `["saved-searches"]` | `lib/savedSearches.ts:36` |
| `["account-profile"]` | `lib/useAccountProfile.ts:20` |
| `["contact-detail", String(id)]` | `lib/contactDetail.ts:24-26` |
| `["contacts", serverQ]` | `screens/ContactList.tsx:209` |
| `["conversations", debouncedQ, membershipRev, sort, order]` | `screens/ConversationList.tsx:97` |
| `["conversation-sources", conversationId]` | `components/SourcesPanel.tsx:27` |
| `["storage-overview"]`, `["import-detail", id]` | `screens/settings/storage/useStorageData.ts:40,57` |
| `["admin-users"]` | `screens/settings/useAdminUsers.ts:36` |
| `["api-tokens"]` | `screens/settings/useApiTokens.ts:33` |
| `["trash-count", query]` | `screens/TrashScreen.tsx:33` |

Nothing in that table can say "everything about contacts is stale". A key
that names a page and a search — `["contacts", serverQ]` — is invalidated by
prefix as `["contacts"]`, but that prefix is a string nobody owns, so the
knowledge that the contact list and the contact drawer are one resource lives
in a comment in `contactGroups.ts` rather than in code.

Because no write says what it invalidates, each screen keeps its own answer:

- `screens/ContactList.tsx` holds `groupOverrides`, a `Record<string,
  string[]>` of chips it has drawn but not yet confirmed, plus
  `groupOverridesRef` to read it inside callbacks, plus `detailCache.setGroups`
  to write the same names into the contact-detail entry, plus `groupsForContact`
  to decide which of the three sources a row's chips come from. Its
  `applyMembership` and `clearAllMembership` are about 75 lines, most of them
  the rollback: rebuild the override map with the change undone, and write the
  undo into the detail cache too.
- `screens/ConversationList.tsx` holds the same map as `tagOverrides`, and a
  `membershipRev` counter it increments and smuggles into the query key so a
  tag-filtered list refetches. A counter in a key is a cache-busting trick: it
  abandons the old entry rather than marking it stale.
- `screens/settings/useApiTokens.ts` and `useAdminUsers.ts` rename `refetch` to
  `reload` and call it after each write, so only the list that hook holds is
  refreshed, and only if that hook is mounted.
- `lib/savedSearches.ts` and `lib/useAccountProfile.ts` write mutation
  responses into the cache with `useVaultSetCached`, which is right, but each
  spells out its own key and its own `useMemo`-wrapped action object.
- `lib/contactDetail.ts` exports `useContactDetailCache`, a hand-built
  `getQueryData` / `setQueryData` / `invalidateQueries` trio, because the
  contact list edits an entry the drawer reads.
- `components/ContactDrawer.tsx` calls `updateContact` and then a `loadDetail`
  callback it passes down to `ContactDrawerHandles`, which passes it to
  `useHandleMutations`, which calls it after each of its two writes.

The visible symptom is the one #294 left behind. Rename a Message Tag while a
conversation carrying it is selected: the rows refresh, because
`messageTags.ts` invalidates `["conversations"]`, but the Tags menu still shows
the old name ticked. `tagChecks` is computed from `displayConversations`, and
`displayConversations` prefers `tagOverrides[c.id]` over the row the vault just
sent. The override map is keyed by conversation id and holds the name as it was
spelled when the person ticked the box, so it survives the rename and hides the
fresh answer underneath it.

## Solution

Every cache key comes from one factory. Every write is a `useMutation` hook in
the module that owns the resource, and that hook — not the screen — decides
what it draws early, what it puts back on failure, and what it marks stale.
No screen keeps a copy of vault data.

This is candidate E of the 2026-09-01 architecture review. It changes no URL,
no route function, and no server code. ADR 0002 stays true: the mechanism is
still TanStack Query and nothing else.

### The key factory

`web/src/lib/vaultKeys.ts`, importing nothing but the key type:

```ts
/** Parameters that make one page of the conversation list its own entry. */
export type ConversationListKey = { q: string; sort: string; order: string };

export const keys = {
  contacts: {
    all: ["contacts"] as const,
    lists: ["contacts", "list"] as const,
    list: (q: string) => ["contacts", "list", q] as const,
    details: ["contacts", "detail"] as const,
    detail: (id: string | number) => ["contacts", "detail", String(id)] as const,
  },
  conversations: {
    all: ["conversations"] as const,
    lists: ["conversations", "list"] as const,
    list: ({ q, sort, order }: ConversationListKey) =>
      ["conversations", "list", q, sort, order] as const,
    sources: (id: string | null) => ["conversations", "sources", String(id)] as const,
  },
  contactGroups: { all: ["contact-groups"] as const },
  messageTags: { all: ["message-tags"] as const },
  savedSearches: { all: ["saved-searches"] as const },
  accountProfile: { all: ["account-profile"] as const },
  apiTokens: { all: ["api-tokens"] as const },
  adminUsers: { all: ["admin-users"] as const },
  storage: {
    all: ["storage"] as const,
    overview: ["storage", "overview"] as const,
    importDetail: (id: number | null) => ["storage", "import", String(id)] as const,
  },
  trash: {
    all: ["trash"] as const,
    count: (q: string) => ["trash", "count", q] as const,
  },
};
```

One rule, applied to every resource: a namespace with `all` as its prefix, and
builders nested under it. TanStack Query matches by prefix, so `keys.contacts.all`
covers every page, every search, and every open drawer, while
`keys.contacts.lists` covers the pages without touching the drawer — which is
what a contact rename wants, since it already holds the fresh drawer answer.

The account is still put in front of every key by `vaultQueryKey`, unchanged.
The factory produces the parts after the account; nothing here knows about
accounts. `contactDetailKey` in `contactDetail.ts` and `ACCOUNT_PROFILE_KEY` in
`useAccountProfile.ts` are deleted, and `nameCollection`'s `cacheKey: string`
config field becomes `key: VaultQueryKey`, taking `keys.contactGroups.all` or
`keys.messageTags.all`.

Two keys change shape rather than just moving: `["storage-overview"]` and
`["import-detail", id]` become `["storage", "overview"]` and `["storage",
"import", id]` so storage has one prefix, and `["trash-count", q]` becomes
`["trash", "count", q]` for the same reason — `messageTags` invalidates that
prefix and needs to be able to name it. Nothing reads either key by hand, so
the change is invisible.

### What every write invalidates

| Write | Hook | Drawn before the vault answers | Marked stale when it settles |
| --- | --- | --- | --- |
| Create, rename, delete a Contact Group | `useCreateNamedSet`, `useRenameNamedSet`, `useDeleteNamedSet` over `contactGroups` | — | `contactGroups.all`, `contacts.all` |
| Put contacts in or out of a Contact Group | `useSetNamedSetMembers(contactGroups)` | rows under `contacts.lists`, entries under `contacts.details` | `contactGroups.all`, `contacts.all` |
| Create, rename, delete a Message Tag | the same three over `messageTags` | — | `messageTags.all`, `conversations.all`, `trash.all` |
| Put conversations in or out of a Message Tag | `useSetNamedSetMembers(messageTags)` | rows under `conversations.lists` | `messageTags.all`, `conversations.all`, `trash.all` |
| Create, update, delete a Saved Search | `useCreateSavedSearch`, `useUpdateSavedSearch`, `useDeleteSavedSearch` | — | nothing: each answers the whole list, which is written to `savedSearches.all` |
| Rename a contact, add or remove one of its handles | `useUpdateContact` | — | `contacts.lists`; the answered contact is written to `contacts.detail(id)` |
| Change the account profile | `useUpdateAccountProfile` | — | nothing: the answered profile is written to `accountProfile.all` |
| Create, rename, revoke an API token | `useCreateApiToken`, `useRenameApiToken`, `useRevokeApiToken` | — | `apiTokens.all` |
| Create a user, change one, delete one, delete a user's messages | `useCreateUser`, `useUpdateUser`, `useDeleteUser`, `useDeleteUserMessages` | — | `adminUsers.all` |
| Set a user's password | `useSetUserPassword` | — | nothing: no list shows it |

A write that answers with the resource writes that answer into its entry and
does not then invalidate it, which is why `useUpdateContact` invalidates
`contacts.lists` and not `contacts.all`.

### Cache operations a mutation needs

Optimistic writing, snapshotting, and rollback all need the query client with
the account already in front of the key. `vaultQuery.ts` gains one hook that
hands over exactly those operations, and the four narrow hooks it supersedes —
`useVaultInvalidate`, `useVaultSetCached`, `useVaultCached`, `useVaultFetchFresh`
— are deleted as their callers move.

```ts
/** Entries as `snapshot` took them: the key is complete, account included. */
export type VaultCacheEntries = readonly [readonly unknown[], unknown][];

export type VaultCache = {
  /** What the cache holds under one key, without fetching. */
  read: <T>(key: VaultQueryKey) => T | undefined;
  /** Ask the vault now and store the answer under the key. */
  fetch: <T>(key: VaultQueryKey, queryFn: (signal: AbortSignal) => Promise<T>) => Promise<T>;
  /** Write one entry, for a mutation that answered with the whole value. */
  set: <T>(key: VaultQueryKey, value: T) => void;
  /** Stop fetches under a prefix, so none lands on top of an optimistic write. */
  cancel: (prefix: VaultQueryKey) => Promise<void>;
  /** Every entry under a prefix, as it stands, to put back on failure. */
  snapshot: (prefix: VaultQueryKey) => VaultCacheEntries;
  /** Rewrite every entry under a prefix. */
  patch: <T>(prefix: VaultQueryKey, update: (entry: T | undefined) => T | undefined) => void;
  /** Put snapshotted entries back. */
  restore: (entries: VaultCacheEntries) => void;
  /** Mark prefixes stale, so whatever is showing them refetches. */
  invalidate: (...prefixes: VaultQueryKey[]) => Promise<void>;
};

export function useVaultCache(): VaultCache;
```

This is not a cache. It is the account prefix applied to eight calls on the one
`QueryClient`, in the same spirit as the `useVaultQuery` wrapper beside it.

### Contact Groups and Message Tags

`nameCollection.ts` keeps its name-based interface and the `idOf` resolution
from #294 exactly as they are: screens, the sidebar, and the router hold names,
and the id is looked up in the cached list, then from the vault once, then
refused. What changes is that each of the four writes is a `useMutation`:

```ts
export function useCreateNamedSet(c: NameCollection):
  UseMutationResult<NamedSet, Error, string>;
export function useRenameNamedSet(c: NameCollection):
  UseMutationResult<NamedSet, Error, { from: string; to: string }>;
export function useDeleteNamedSet(c: NameCollection):
  UseMutationResult<void, Error, string>;
export function useSetNamedSetMembers(c: NameCollection):
  UseMutationResult<MembersChanged, Error, SetMembersVars, ChipSnapshot>;

export type SetMembersVars = { name: string; patch: MembersPatch };
export type MembersChanged = { added: number; removed: number };
type ChipSnapshot = { entries: VaultCacheEntries };
```

`useNameCollectionActions` composes the four and keeps the interface its five
callers already use, with two fields added:

```ts
useNameCollectionActions(collection): {
  create(name): Promise<string>;
  rename(from, to): Promise<string>;
  remove(name): Promise<void>;
  setMembers(name, patch): Promise<MembersChanged>;
  invalidate(): Promise<void>;
  /** Any of the four in flight. Replaces the `busy` state in `NavEntityList`. */
  pending: boolean;
  /** The most recent failure, or null. */
  error: Error | null;
}
```

`contactGroups.ts` and `messageTags.ts` also export a named membership hook, so
the two list screens read the mutation directly rather than through the
composed object:

```ts
export function useSetContactGroupMembers(): ReturnType<typeof useSetNamedSetMembers>;
export function useSetMessageTagMembers(): ReturnType<typeof useSetNamedSetMembers>;
```

### Membership drawn before the vault answers

A membership write is the only optimistic one, because it is the only write a
person makes repeatedly against a long list where a round trip would show.
What it draws is described by the collection, not by the screen:

```ts
/** A cached shape whose rows carry this collection's names. */
export type ChipTarget = {
  /** Prefix of the entries to patch. */
  key: VaultQueryKey;
  /** Field the names sit in on a row. */
  field: "groups" | "tags";
  /** `pages` for an offset-paged list entry, `row` for one row on its own. */
  shape: "pages" | "row";
};
```

| Collection | `chips` | `invalidates` |
| --- | --- | --- |
| Contact Groups | `{ contacts.lists, "groups", "pages" }`, `{ contacts.details, "groups", "row" }` | `contacts.all` |
| Message Tags | `{ conversations.lists, "tags", "pages" }` | `conversations.all`, `trash.all` |

The mutation then reads the same for both:

- `onMutate` cancels fetches under each chip prefix, snapshots every entry
  under them, and patches each entry: rows whose id is in `patch.add` gain the
  name, rows whose id is in `patch.remove` lose it. Names are compared without
  regard to letter case, which is what `withGroupMembership` in `ContactList.tsx`
  does today. A `pages` entry is `InfiniteData<OffsetPage<Row>>` — the shape
  `useVaultPagedList` stores — so the patch maps `data.pages[].items[]`; a `row`
  entry is one object with an `id`.
- `onError` puts the snapshot back, entry by entry.
- `onSettled` invalidates the collection's own list and the lists in
  `invalidates`.

The snapshot is per mutation, so two membership writes in flight at once — the
Clear-all path fires one per name — each roll back only their own change
instead of the whole map, which is what `clearAllMembership` gets wrong today
by reconstructing the map from `priorById`.

### The stale Tags-menu checkbox

`tagOverrides` is deleted, and with it the reason a rename cannot show.
`ConversationList` renders `conversations` straight from the paged query, so
`tagChecks` is computed from cached rows. A rename invalidates
`conversations.all` and `messageTags.all`; both refetch; the menu's names and
the rows' names come from the same fresh pair, and there is no third copy
spelling the name as it was before. The checkbox is correct because there is
nowhere left for a stale name to live.

`membershipRev` goes with it. It existed to force a refetch of a tag-filtered
list after a membership change; `onSettled` invalidates `conversations.all`
after every membership write, which refetches the filtered list whether or not
the query text mentions a tag.

### What each screen loses

| File | Deleted | Replaced by |
| --- | --- | --- |
| `screens/ContactList.tsx` | `groupOverrides`, `groupOverridesRef`, `groupsForContact`, `withGroupMembership`, the rollback halves of `applyMembership` and `clearAllMembership`, the `useContactDetailCache` import | `useSetContactGroupMembers()`; rows render from the paged query |
| `screens/ConversationList.tsx` | `tagOverrides`, `membershipRev`, `displayConversations` | `useSetMessageTagMembers()`; `conversations` is what renders |
| `components/NavEntityList.tsx` | its `busy` state and the `setBusy` calls in `create`, `rename`, `remove` | `actions.pending` |
| `lib/contactDetail.ts` | `useContactDetailCache`, `contactDetailKey` | `useUpdateContact()`, `keys.contacts.detail` |
| `components/ContactDrawer.tsx` | `loadDetail`, the `updateContact` call, the `onHandlesChanged` prop | `useUpdateContact()` |
| `components/contactDrawer/useHandleMutations.ts` | `busy`, `error`, both try/catch/finally blocks, the `onHandlesChanged` argument | `useUpdateContact()`; `isPending` and `error` come off the mutation |
| `lib/savedSearches.ts` | the `useMemo` action object and its `adopt` helper | three mutation hooks; `useSavedSearchActions` composes them |
| `lib/useAccountProfile.ts` | `setProfile`, `reload` | `useUpdateAccountProfile()` |
| `screens/settings/useApiTokens.ts` | `useAsyncAction`, `reload()` after each write | three mutation hooks |
| `screens/settings/useAdminUsers.ts` | `useAsyncAction`, `reload()` after each write | five mutation hooks |
| `lib/vaultQuery.ts` | `useVaultInvalidate`, `useVaultSetCached`, `useVaultCached`, `useVaultFetchFresh` | `useVaultCache()` |

`busy` survives as a word where it is a component prop — `<Button busy>`,
`<ConfirmDialog busy>` — so `useApiTokens` and `useAdminUsers` keep returning a
field of that name. What changes is where the value comes from: the union of
their mutations' `isPending` rather than a `useState` flag those hooks set and
cleared themselves. `useAsyncAction` itself stays, for the sign-in, create
account, and onboarding forms.

### Tests

Vitest with jsdom, as now: a real `QueryClient` in the wrapper, route functions
faked by name through `vi.mock("./vaultApi")`, never a URL.

Per mutation hook, one test that asserts:

1. the route function it calls, and with what,
2. what the cache holds before the vault answers (membership only),
3. that a rejected route call leaves the cache as it was (membership only),
4. the prefixes handed to `invalidateQueries`, read off a
   `vi.spyOn(client, "invalidateQueries")` — the pattern
   `nameCollection.test.tsx` already uses.

New and changed files:

- `lib/vaultKeys.test.ts` — one test per namespace, asserting the parts and
  that `all` is a prefix of the builders (`keys.contacts.list("ada")` starts
  with `keys.contacts.all`). Pure, no jsdom.
- `lib/vaultQuery.test.tsx` — a `useVaultCache` block covering the account
  prefix on `read`, `fetch` and `set`, and a snapshot / patch / restore round
  trip over two entries under one prefix. The `useVaultCached and
  useVaultFetchFresh` block is deleted with the hooks.
- `lib/nameCollection.test.tsx` — keeps every case it has, since the
  interface it tests is unchanged, and gains: a membership write patches both
  a `pages` entry and a `row` entry before the route resolves; a rejected
  membership write restores both; `pending` is true while a write is in flight.
- `lib/savedSearches.test.ts` — keeps its cases; the "takes the refreshed list
  from a mutation instead of asking again" case is the one that proves
  `onSuccess` still writes the answer.
- `lib/contactDetail.test.tsx` (new) — `useUpdateContact` sends the body to
  `updateContact`, writes the answer to `keys.contacts.detail(id)`, and
  invalidates `keys.contacts.lists` but not the detail it just wrote.
- `lib/useAccountProfile.test.tsx` (new) — `useUpdateAccountProfile` writes the
  answer where `useAccountProfile` reads it, with one `getAccountProfile` call.
- `screens/settings/useApiTokens.test.tsx` and `useAdminUsers.test.tsx` (new) —
  each write invalidates its list prefix, and `busy` is true while one is in
  flight.
- `components/ContactDrawer.test.tsx` — its seeding helper moves from
  `contactDetailKey` to `keys.contacts.detail`; its rename case now asserts
  that the drawer shows the answered name without a second `getContact`.
- `screens/settings/ApiTokensSection.test.tsx` and `AdminUsersPanel.test.tsx`
  keep faking route functions and `fetch` respectively and are expected to pass
  unchanged: invalidating a list refetches it exactly as `reload()` did.

The two list screens get no new test. Neither has one today, both need the
right-toolbar context to render, and everything this change moves out of them
is covered where it lands, in `nameCollection.test.tsx`. The browser check in
the plan covers the rest.

### What else changes

- `@tanstack/react-query` moves from `devDependencies` to `dependencies` in
  `web/package.json`. It is what the app fetches and writes through; it has
  been in the wrong list since #291, and this change makes it load-bearing for
  writes as well as reads.
- `screens/settings/AddressBookSection.tsx` keeps its own `busy` state and its
  `groupActions.invalidate()` call. It reads a file in the browser and uploads
  it; the busy flag covers the read, which is not a mutation.
- `lib/auth.tsx` is untouched. It clears the whole client on sign-in and
  sign-out and needs to know nothing about keys.
- `fetchAccountProfileFor` keeps building its key from `vaultQueryKey` by hand,
  because sign-in knows the account before the auth state carries it. It takes
  the key parts from `keys.accountProfile.all` instead of a local constant.

### Not changing

- No URL, no route function in `vaultApi.ts`, no server code, no OpenAPI
  document, and no regeneration of `vaultApi.types.ts`.
- `useNameCollection` stays name-based, and `idOf` keeps resolving a name to an
  id inside the module. ADR 0003 is untouched.
- Sign-in and create-account forms (`screens/auth/LoginForm.tsx`,
  `CreateAccountForm.tsx`) keep `useAsyncAction`: they write no vault data and
  have no cached list to keep fresh.
- The Import Run machine (`screens/import/useImportJob.ts`) and every write it
  drives. It is a long-running state machine with its own polling, not a
  request whose answer belongs in a cache.
- `screens/OnboardingScreen.tsx` keeps `useAsyncAction` and its direct
  `updateAccountProfile` call. It runs before the signed-in tree renders any
  list.
- `useVaultQuery`, `useVaultPagedList`, `vaultQueryKey`, and the account prefix
  rule.
- `InfiniteOffsetList`, `VirtualList`, `GroupsMenu`, `TagsMenu`,
  `checksFromMembers`: they render what they are given.
- `CONTEXT.md`. Nothing here introduces a product concept.
