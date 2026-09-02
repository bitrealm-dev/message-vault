# Web writes through TanStack Query Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the web app one key factory and one `useMutation` hook per write, so a write says what it draws early, what it puts back on failure, and what it marks stale — and no screen keeps a copy of vault data.

**Architecture:** `web/src/lib/vaultKeys.ts` exports one `keys` object with a prefix per resource; every literal key moves onto it, and `vaultQueryKey` still puts the account in front. `vaultQuery.ts` gains `useVaultCache()`, the account-scoped cache operations a mutation needs, replacing `useVaultInvalidate`, `useVaultSetCached`, `useVaultCached` and `useVaultFetchFresh`. Each feature module — `nameCollection.ts`, `savedSearches.ts`, `contactDetail.ts`, `useAccountProfile.ts`, `useApiTokens.ts`, `useAdminUsers.ts` — turns its writes into `useMutation` hooks that own `onMutate`, `onError` and `onSettled`. The two list screens lose their override maps, their manual rollback, and the `membershipRev` counter.

**Tech Stack:** React 19 + TypeScript, TanStack Query v5, Vitest + Testing Library (jsdom), Biome.

**Spec:** `docs/superpowers/specs/2026-09-02-web-writes-through-tanstack-query-design.md`. Decision record: `docs/adr/0002-one-way-to-fetch-data-in-the-web-app.md`. Issue: #299.

## Global Constraints

- Work on the `feat/web-writes-through-tanstack-query` branch. Never commit to `main`. Never create or push tags.
- Never run `./scripts/run-vault-dev.sh --reset` or `--reset-demo`. Use `./scripts/run-vault-dev.sh` alone, which keeps `data/`.
- ADR 0002 stays true: no new cache, no new change-notification event, no new fetching hook. `useVaultCache()` is the account prefix applied to calls on the one `QueryClient`, nothing more.
- `useNameCollection` and `useNameCollectionActions` stay name-based. Screens, the sidebar, and the router never see a set's id; `idOf` keeps resolving it inside `nameCollection.ts` (ADR 0003).
- No URL, no route function in `vaultApi.ts`, and no server code changes. `web/src/lib/vaultApi.types.ts` is generated and is never hand-edited.
- Biome gates `web/`: prefix unused bindings with `_`, prefer a real fix over `biome-ignore`, and run `npx biome format --write src` before each commit.
- Tests use invented data only (`Family`, `Work`, `Holiday`, `Ada`, `Ben`, `alice`, `bob`). Never commit real message data.
- Tests fake route functions from `web/src/lib/vaultApi.ts` by name, never a URL, and render against a real `QueryClient` with `retry: false`.
- Every commit message is a conventional commit whose body says what changed and why, in plain language at a high-school reading level, and ends with these two trailers:

  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9
  ```

- After each task: `cd web && npx tsc --noEmit -p . && npm run lint && npm test` must pass before committing. The app must work at the end of every task.
- The TypeScript below was written against the current sources but not compiled before this plan was written. Where wiring details differ, the compiler and the existing tests are authoritative; keep the names and types the Interfaces blocks state.

---

### Task 1: One key factory, every literal moved onto it

**Files:**
- Create: `web/src/lib/vaultKeys.ts`, `web/src/lib/vaultKeys.test.ts`
- Modify: `web/src/lib/contactDetail.ts`, `web/src/lib/useAccountProfile.ts`, `web/src/lib/savedSearches.ts`, `web/src/lib/nameCollection.ts`, `web/src/lib/contactGroups.ts`, `web/src/lib/messageTags.ts`, `web/src/screens/ContactList.tsx`, `web/src/screens/ConversationList.tsx`, `web/src/components/SourcesPanel.tsx`, `web/src/screens/settings/storage/useStorageData.ts`, `web/src/screens/settings/useAdminUsers.ts`, `web/src/screens/settings/useApiTokens.ts`, `web/src/screens/TrashScreen.tsx`, `web/src/components/ContactDrawer.test.tsx`, `web/src/lib/nameCollection.test.tsx`

**Interfaces:**
- Produces `keys` and `type ConversationListKey` from `web/src/lib/vaultKeys.ts`. Every later task uses them.
- `NameCollectionConfig.cacheKey: string` becomes `key: VaultQueryKey`; `invalidates` becomes `readonly VaultQueryKey[]`; `NameCollection.key` becomes `VaultQueryKey`.
- Removes `contactDetailKey` from `contactDetail.ts` and `ACCOUNT_PROFILE_KEY` from `useAccountProfile.ts`.
- Behaviour does not change in this task.

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/vaultKeys.test.ts`:

```ts
/**
 * The shape of a cache key, which is the only thing this module has.
 *
 * What matters is not the words in a key but which keys sit under which
 * prefix: TanStack Query invalidates by prefix, so `keys.contacts.all` being a
 * prefix of every contact key is what lets one write say "everything about
 * contacts is stale".
 */

import { describe, expect, it } from "vitest";
import { keys } from "./vaultKeys";

/** True when invalidating `prefix` would mark `key` stale. */
function coveredBy(key: readonly unknown[], prefix: readonly unknown[]): boolean {
  return prefix.every((part, i) => key[i] === part);
}

describe("keys", () => {
  it("puts a contact list page and one contact under the contacts prefix", () => {
    expect(keys.contacts.list("ada")).toEqual(["contacts", "list", "ada"]);
    expect(keys.contacts.detail(12)).toEqual(["contacts", "detail", "12"]);
    expect(coveredBy(keys.contacts.list("ada"), keys.contacts.all)).toBe(true);
    expect(coveredBy(keys.contacts.detail(12), keys.contacts.all)).toBe(true);
    // The list prefix leaves an open drawer alone, which is what a contact
    // rename wants: it already holds the answer for the drawer.
    expect(coveredBy(keys.contacts.detail(12), keys.contacts.lists)).toBe(false);
  });

  it("carries a contact id as text, so 12 and \"12\" name one entry", () => {
    expect(keys.contacts.detail(12)).toEqual(keys.contacts.detail("12"));
  });

  it("gives each search and sort of the conversation list its own entry", () => {
    const key = keys.conversations.list({ q: "tag:Holiday", sort: "date", order: "desc" });
    expect(key).toEqual(["conversations", "list", "tag:Holiday", "date", "desc"]);
    expect(key).not.toEqual(keys.conversations.list({ q: "tag:Holiday", sort: "date", order: "asc" }));
    expect(coveredBy(key, keys.conversations.all)).toBe(true);
    expect(coveredBy(keys.conversations.sources("7"), keys.conversations.all)).toBe(true);
  });

  it("gives every named collection a prefix of its own", () => {
    expect(keys.contactGroups.all).toEqual(["contact-groups"]);
    expect(keys.messageTags.all).toEqual(["message-tags"]);
    expect(keys.savedSearches.all).toEqual(["saved-searches"]);
    expect(keys.accountProfile.all).toEqual(["account-profile"]);
    expect(keys.apiTokens.all).toEqual(["api-tokens"]);
    expect(keys.adminUsers.all).toEqual(["admin-users"]);
  });

  it("puts the storage overview and one import run under the storage prefix", () => {
    expect(keys.storage.overview).toEqual(["storage", "overview"]);
    expect(keys.storage.importDetail(4)).toEqual(["storage", "import", "4"]);
    expect(coveredBy(keys.storage.overview, keys.storage.all)).toBe(true);
    expect(coveredBy(keys.storage.importDetail(4), keys.storage.all)).toBe(true);
  });

  it("puts every trash count under one prefix, so a tag write can name them all", () => {
    expect(keys.trash.count("is:trash")).toEqual(["trash", "count", "is:trash"]);
    expect(coveredBy(keys.trash.count("is:trash"), keys.trash.all)).toBe(true);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/vaultKeys.test.ts`
Expected: the suite fails to load — `Failed to resolve import "./vaultKeys"`.

- [ ] **Step 3: Write the factory**

Create `web/src/lib/vaultKeys.ts`:

```ts
/**
 * Every cache key the web app uses, in one place.
 *
 * A key is built here and nowhere else, for one reason: TanStack Query marks
 * entries stale by prefix, so a write can only say "everything about contacts
 * is stale" if something owns the word `contacts`. When keys were literals
 * typed at each call site, that knowledge lived in comments, and screens kept
 * their own override maps rather than trusting an invalidation they could not
 * name.
 *
 * One rule, for every resource: a namespace whose `all` is the prefix, with
 * the builders nested under it. The account is not here — `vaultQueryKey` puts
 * it in front of whatever these produce, so no key in this file is complete on
 * its own. See `docs/adr/0002-one-way-to-fetch-data-in-the-web-app.md`.
 */

/** What makes one page of the conversation list its own cache entry. */
export type ConversationListKey = { q: string; sort: string; order: string };

export const keys = {
  contacts: {
    /** Every contact list page and every open contact. */
    all: ["contacts"] as const,
    /** The list pages only, leaving an open drawer's entry alone. */
    lists: ["contacts", "list"] as const,
    list: (q: string) => ["contacts", "list", q] as const,
    details: ["contacts", "detail"] as const,
    /** Ids arrive as numbers from the vault and as strings from the router. */
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

- [ ] **Step 4: Move every literal key onto the factory**

`web/src/lib/contactDetail.ts`:
- Delete the exported `contactDetailKey` function and its doc comment.
- Add `import { keys } from "./vaultKeys";`.
- Replace all four uses of `contactDetailKey(...)` with `keys.contacts.detail(...)`: in `useContactDetail`, and in `read`, `setGroups` and `invalidate` inside `useContactDetailCache`.

`web/src/lib/useAccountProfile.ts`:
- Delete `export const ACCOUNT_PROFILE_KEY = ["account-profile"] as const;` and its comment.
- Add `import { keys } from "./vaultKeys";` and replace all three uses with `keys.accountProfile.all`, including the one in `fetchAccountProfileFor`.

`web/src/lib/savedSearches.ts`:
- Delete `const SAVED_SEARCHES_KEY = ["saved-searches"] as const;`.
- Add `import { keys } from "./vaultKeys";` and replace both uses with `keys.savedSearches.all`.

`web/src/lib/nameCollection.ts`:
- In `NameCollectionConfig`, replace

  ```ts
  /** Name of this collection in a cache key, e.g. `contact-groups`. */
  cacheKey: string;
  ```

  with

  ```ts
  /** This collection's cache prefix, from `vaultKeys`. */
  key: VaultQueryKey;
  ```

- Change `invalidates` on both `NameCollectionConfig` and `NameCollection` to `readonly VaultQueryKey[]`, and `NameCollection.key` from `readonly [string]` to `VaultQueryKey`.
- In `createNameCollection`, replace `key: [config.cacheKey] as const,` with `key: config.key,`.

`web/src/lib/contactGroups.ts`, in the `createNameCollection` call:

```ts
  key: keys.contactGroups.all,
  // Contact rows and the contact drawer show group names as chips; one prefix
  // covers both.
  invalidates: [keys.contacts.all],
```

and `import { keys } from "./vaultKeys";`.

`web/src/lib/messageTags.ts`, in the same place:

```ts
  key: keys.messageTags.all,
  // Conversation rows and the Trash count show tag names.
  invalidates: [keys.conversations.all, keys.trash.all],
```

and `import { keys } from "./vaultKeys";`.

`web/src/screens/ContactList.tsx`: `useVaultPagedList(["contacts", serverQ], fetchPage, {` becomes `useVaultPagedList(keys.contacts.list(serverQ), fetchPage, {`, with `import { keys } from "../lib/vaultKeys";`.

`web/src/screens/ConversationList.tsx`: the key argument becomes

```ts
  } = useVaultPagedList(
    // `membershipRev` is a counter this screen bumps to force a refetch; it
    // goes away with the override map in a later task.
    [...keys.conversations.list({ q: debouncedQ, sort: sortState.sort, order: sortState.order }), membershipRev],
    fetchPage,
  );
```

`web/src/components/SourcesPanel.tsx`: `["conversation-sources", conversationId]` becomes `keys.conversations.sources(conversationId)`.

`web/src/screens/settings/storage/useStorageData.ts`: `["storage-overview"]` becomes `keys.storage.overview`, and `["import-detail", selectedImportId]` becomes `keys.storage.importDetail(selectedImportId)`.

`web/src/screens/settings/useAdminUsers.ts`: `["admin-users"]` becomes `keys.adminUsers.all`.

`web/src/screens/settings/useApiTokens.ts`: `["api-tokens"]` becomes `keys.apiTokens.all`.

`web/src/screens/TrashScreen.tsx`: `["trash-count", query]` becomes `keys.trash.count(query)`.

`web/src/components/ContactDrawer.test.tsx`: replace the import

```tsx
import { type ContactDetail, contactDetailKey } from "../lib/contactDetail";
```

with

```tsx
import type { ContactDetail } from "../lib/contactDetail";
import { keys } from "../lib/vaultKeys";
```

and in `seed`, `vaultQueryKey("test-account", contactDetailKey(detail.id))` becomes `vaultQueryKey("test-account", keys.contacts.detail(detail.id))`.

`web/src/lib/nameCollection.test.tsx`:
- Add `import { keys } from "./vaultKeys";`.
- In `groupsOver`, replace `cacheKey: "contact-groups",` with `key: keys.contactGroups.all,` and `invalidates: [["contacts"], ["contact-detail"]],` with `invalidates: [keys.contacts.all],`.
- In the rename test, the expected keys become

  ```ts
    expect(keys_).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "contact-groups"],
        ["vault", "account-1", "contacts"],
      ]),
    );
  ```

  Rename the local `const keys = invalidate.mock.calls…` to `const invalidated = …` first, so it does not shadow the imported `keys`, and use `invalidated` in the assertion.

- [ ] **Step 5: Run the checks**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean, Biome clean, every suite passes including the new `vaultKeys.test.ts`. Nothing outside `vaultKeys.ts` should hold a bracketed key literal any more.

Run: `grep -rn '\["contact-groups"\]\|\["message-tags"\]\|\["saved-searches"\]\|\["account-profile"\]\|"contact-detail"\|\["contacts",\|\["conversations",\|\["conversation-sources"\|\["storage-overview"\]\|\["import-detail"\|\["admin-users"\]\|\["api-tokens"\]\|\["trash-count"' web/src --include=*.ts --include=*.tsx | grep -v vaultKeys`
Expected: no output.

- [ ] **Step 6: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/vaultKeys.ts web/src/lib/vaultKeys.test.ts web/src/lib web/src/screens web/src/components
git commit -m "refactor(web): build every cache key in one place

Query keys were literals typed at each call site, so the fact that the
contact list and the contact drawer are one resource was written in a
comment rather than in code. vaultKeys.ts now builds every key, with one
prefix per resource, which is what lets a write say what it makes stale.
Nothing changes on screen.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 2: The cache operations a mutation needs

**Files:**
- Modify: `web/src/lib/vaultQuery.ts` (add after `useVaultFetchFresh`), `web/src/lib/vaultQuery.test.tsx`

**Interfaces:**
- Produces `useVaultCache(): VaultCache` and `type VaultCacheEntries` from `vaultQuery.ts`.
- `VaultCache` has `read`, `fetch`, `set`, `cancel`, `snapshot`, `patch`, `restore`, `invalidate`, each putting the signed-in account in front of the key it is given.
- The four hooks it supersedes stay until their callers move (Tasks 3, 6 and 8).

- [ ] **Step 1: Write the failing tests**

Append to `web/src/lib/vaultQuery.test.tsx`, and add `useVaultCache` to the import from `./vaultQuery`:

```tsx
describe("useVaultCache", () => {
  it("reads and writes under the signed-in account's name", () => {
    const { result } = renderHook(() => useVaultCache(), { wrapper });
    act(() => {
      result.current.set(["contact-groups"], [{ id: 1, name: "Family" }]);
    });
    expect(client.getQueryData(["vault", "account-1", "contact-groups"])).toEqual([
      { id: 1, name: "Family" },
    ]);
    expect(result.current.read(["contact-groups"])).toEqual([{ id: 1, name: "Family" }]);

    // Another account's entry is not this account's to read.
    client.setQueryData(["vault", "account-2", "contact-groups"], [{ id: 9, name: "Work" }]);
    expect(result.current.read(["contact-groups"])).toEqual([{ id: 1, name: "Family" }]);
  });

  it("asks the vault and stores the answer under the account's key", async () => {
    const { result } = renderHook(() => useVaultCache(), { wrapper });
    await expect(result.current.fetch(["contact-groups"], async () => ["Family"])).resolves.toEqual(
      ["Family"],
    );
    expect(client.getQueryData(["vault", "account-1", "contact-groups"])).toEqual(["Family"]);
  });

  it("patches every entry under one prefix and puts them all back from a snapshot", () => {
    client.setQueryData(["vault", "account-1", "contacts", "list", ""], { total: 1 });
    client.setQueryData(["vault", "account-1", "contacts", "list", "ada"], { total: 2 });
    client.setQueryData(["vault", "account-1", "conversations", "list", ""], { total: 3 });
    const { result } = renderHook(() => useVaultCache(), { wrapper });

    const taken = result.current.snapshot(["contacts"]);
    expect(taken).toHaveLength(2);

    act(() => {
      result.current.patch<{ total: number }>(["contacts"], (entry) =>
        entry ? { total: entry.total + 10 } : entry,
      );
    });
    expect(client.getQueryData(["vault", "account-1", "contacts", "list", ""])).toEqual({ total: 11 });
    expect(client.getQueryData(["vault", "account-1", "contacts", "list", "ada"])).toEqual({ total: 12 });
    // A different resource under a different prefix is untouched.
    expect(client.getQueryData(["vault", "account-1", "conversations", "list", ""])).toEqual({ total: 3 });

    act(() => {
      result.current.restore(taken);
    });
    expect(client.getQueryData(["vault", "account-1", "contacts", "list", ""])).toEqual({ total: 1 });
    expect(client.getQueryData(["vault", "account-1", "contacts", "list", "ada"])).toEqual({ total: 2 });
  });

  it("marks several prefixes stale in one call", async () => {
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useVaultCache(), { wrapper });
    await result.current.invalidate(["message-tags"], ["conversations"]);
    expect(invalidate.mock.calls.map((call) => call[0]?.queryKey)).toEqual([
      ["vault", "account-1", "message-tags"],
      ["vault", "account-1", "conversations"],
    ]);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/vaultQuery.test.tsx`
Expected: the file fails to compile — `useVaultCache` is not exported from `./vaultQuery`.

- [ ] **Step 3: Add the hook**

In `web/src/lib/vaultQuery.ts`, add `useMemo` to the `react` import and append after `useVaultFetchFresh`:

```ts
/** Entries as `snapshot` took them. The key is complete, account included. */
export type VaultCacheEntries = readonly [readonly unknown[], unknown][];

/**
 * The cache operations a write needs, with the account already in front of
 * every key.
 *
 * A mutation draws its change before the vault answers, puts the old value
 * back when the vault refuses, and says what is stale once it settles. Each of
 * those is one call on the query client — this is that client with the account
 * rule applied, and nothing else. It is not a cache.
 */
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
  /** Put snapshotted entries back where they came from. */
  restore: (entries: VaultCacheEntries) => void;
  /** Mark prefixes stale, so whatever is showing them refetches. */
  invalidate: (...prefixes: VaultQueryKey[]) => Promise<void>;
};

export function useVaultCache(): VaultCache {
  const client = useQueryClient();
  const account = useAccountScope();
  return useMemo(() => {
    const at = (key: VaultQueryKey) => vaultQueryKey(account, key);
    return {
      read: <T>(key: VaultQueryKey) => client.getQueryData<T>(at(key)),
      fetch: <T>(key: VaultQueryKey, queryFn: (signal: AbortSignal) => Promise<T>) =>
        client.fetchQuery<T>({
          queryKey: at(key),
          queryFn: ({ signal }) => queryFn(signal),
          staleTime: 0,
        }),
      set: <T>(key: VaultQueryKey, value: T) => {
        client.setQueryData(at(key), value);
      },
      cancel: (prefix: VaultQueryKey) => client.cancelQueries({ queryKey: at(prefix) }),
      snapshot: (prefix: VaultQueryKey) => client.getQueriesData({ queryKey: at(prefix) }),
      patch: <T>(prefix: VaultQueryKey, update: (entry: T | undefined) => T | undefined) => {
        client.setQueriesData<T>({ queryKey: at(prefix) }, update);
      },
      restore: (entries: VaultCacheEntries) => {
        for (const [key, data] of entries) client.setQueryData(key, data);
      },
      invalidate: async (...prefixes: VaultQueryKey[]) => {
        for (const prefix of prefixes) {
          await client.invalidateQueries({ queryKey: at(prefix) });
        }
      },
    };
  }, [client, account]);
}
```

`invalidate` awaits in order rather than in parallel so the test above can read the calls in the order they were asked for; each `invalidateQueries` resolves when its refetches settle, and there are at most three per write.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx tsc --noEmit -p . && npx vitest run src/lib/vaultQuery.test.tsx`
Expected: type-check clean; every case in the file passes, including the four new ones.

- [ ] **Step 5: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/vaultQuery.ts web/src/lib/vaultQuery.test.tsx
git commit -m "feat(web): add the cache operations a write needs

A write has to draw its change before the vault answers, put the old
value back if the vault refuses, and say what is stale afterwards.
useVaultCache gives a mutation those operations with the signed-in
account already in front of every key, so no feature module has to
remember the account rule. It replaces four narrow hooks, which come out
as their callers move.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 3: Contact Group and Message Tag writes as mutations

**Files:**
- Modify: `web/src/lib/nameCollection.ts` (types at the top, everything from `useNameCollectionActions` down), `web/src/lib/contactGroups.ts`, `web/src/lib/messageTags.ts`, `web/src/components/NavEntityList.tsx`, `web/src/lib/nameCollection.test.tsx`, `web/src/lib/vaultQuery.ts`, `web/src/lib/vaultQuery.test.tsx`

**Interfaces:**
- Produces from `nameCollection.ts`:
  - `type ChipTarget = { key: VaultQueryKey; field: "groups" | "tags"; shape: "pages" | "row" }`
  - `type MembersChanged = { added: number; removed: number }`
  - `type SetMembersVars = { name: string; patch: MembersPatch }`
  - `withName(names, name, enable): string[]` and `patchChips(entry, target, ids, name, enable): unknown`, exported for their tests
  - `useCreateNamedSet(c): UseMutationResult<NamedSet, Error, string>`
  - `useRenameNamedSet(c): UseMutationResult<NamedSet, Error, { from: string; to: string }>`
  - `useDeleteNamedSet(c): UseMutationResult<void, Error, string>`
  - `useSetNamedSetMembers(c): UseMutationResult<MembersChanged, Error, SetMembersVars, ChipSnapshot>`
  - `useNameCollectionActions(c)` keeps `create`, `rename`, `remove`, `setMembers`, `invalidate` and gains `pending: boolean` and `error: Error | null`
- `NameCollectionConfig` and `NameCollection` gain `chips: readonly ChipTarget[]`.
- `contactGroups.ts` exports `useSetContactGroupMembers()`; `messageTags.ts` exports `useSetMessageTagMembers()`. Tasks 4 and 5 call them.
- Deletes `useVaultInvalidate`, `useVaultSetCached`, `useVaultCached` and `useVaultFetchFresh`… except `useVaultSetCached`, which `savedSearches.ts` and `useAccountProfile.ts` still call until Tasks 6 and 8. Delete the other three here.

- [ ] **Step 1: Write the failing tests**

In `web/src/lib/nameCollection.test.tsx`, replace the import block from `./nameCollection` and add the new fixtures:

```tsx
import { act, renderHook, waitFor } from "@testing-library/react";
import {
  createNameCollection,
  type MembersChanged,
  type NameCollectionRoutes,
  useNameCollection,
  useNameCollectionActions,
  useSetNamedSetMembers,
} from "./nameCollection";
import { keys } from "./vaultKeys";
```

Give `groupsOver` the chip targets:

```tsx
function groupsOver(routes: NameCollectionRoutes) {
  return createNameCollection({
    routes,
    key: keys.contactGroups.all,
    invalidates: [keys.contacts.all],
    chips: [
      { key: keys.contacts.lists, field: "groups", shape: "pages" },
      { key: keys.contacts.details, field: "groups", shape: "row" },
    ],
    label: "group",
    queryToken: "group",
    reservedNames: new Set(["trash"]),
    reservedError: (name) => `${name} is reserved`,
  });
}

const PAGE_KEY = ["vault", "account-1", "contacts", "list", ""];
const DETAIL_KEY = ["vault", "account-1", "contacts", "detail", "1"];

/** A contact list page and an open contact, as the two queries would hold them. */
function seedContacts(): void {
  client.setQueryData(PAGE_KEY, {
    pages: [
      {
        items: [
          { id: "1", name: "Ada", groups: [] },
          { id: "2", name: "Ben", groups: ["Work"] },
        ],
        total: 2,
      },
    ],
    pageParams: [0],
  });
  client.setQueryData(DETAIL_KEY, { id: 1, name: "Ada", groups: [] });
}

/** Group chips on one row of the seeded page. */
function pageGroups(id: string): string[] | undefined {
  const entry = client.getQueryData<{ pages: { items: { id: string; groups: string[] }[] }[] }>(
    PAGE_KEY,
  );
  return entry?.pages[0].items.find((row) => row.id === id)?.groups;
}

/** Group chips on the open contact. */
function detailGroups(): string[] | undefined {
  return client.getQueryData<{ groups: string[] }>(DETAIL_KEY)?.groups;
}

/** A promise this test resolves when it chooses, so it can look mid-write. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => {};
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
```

Add a describe block for the membership mutation:

```tsx
describe("useSetNamedSetMembers", () => {
  it("draws the chips on the list and the open contact before the vault answers", async () => {
    const routes = fakeRoutes();
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    seedContacts();
    const answer = deferred<MembersChanged>();
    routes.updateMembers.mockReturnValue(answer.promise);

    const { result } = renderHook(() => useSetNamedSetMembers(groupsOver(routes)), { wrapper });
    let write: Promise<MembersChanged> = Promise.resolve({ added: 0, removed: 0 });
    act(() => {
      write = result.current.mutateAsync({ name: "Family", patch: { add: [1] } });
    });

    await waitFor(() => expect(pageGroups("1")).toEqual(["Family"]));
    expect(detailGroups()).toEqual(["Family"]);
    expect(pageGroups("2")).toEqual(["Work"]);

    answer.resolve({ added: 1, removed: 0 });
    await write;
    expect(routes.updateMembers).toHaveBeenCalledWith(12, { add: [1], remove: [] });
  });

  it("takes a name off the rows it was removed from", async () => {
    const routes = fakeRoutes();
    client.setQueryData(KEY, [{ id: 5, name: "Work" }]);
    seedContacts();
    const { result } = renderHook(() => useSetNamedSetMembers(groupsOver(routes)), { wrapper });
    await result.current.mutateAsync({ name: "Work", patch: { remove: [2] } });
    expect(pageGroups("2")).toEqual([]);
    expect(routes.updateMembers).toHaveBeenCalledWith(5, { add: [], remove: [2] });
  });

  it("puts every row back when the vault refuses", async () => {
    const routes = fakeRoutes();
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    seedContacts();
    routes.updateMembers.mockRejectedValue(new Error("nope"));

    const { result } = renderHook(() => useSetNamedSetMembers(groupsOver(routes)), { wrapper });
    await expect(
      result.current.mutateAsync({ name: "Family", patch: { add: [1] } }),
    ).rejects.toThrow("nope");

    expect(pageGroups("1")).toEqual([]);
    expect(detailGroups()).toEqual([]);
  });

  it("marks the group list and every contact stale once it settles", async () => {
    const routes = fakeRoutes();
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useSetNamedSetMembers(groupsOver(routes)), { wrapper });
    await result.current.mutateAsync({ name: "Family", patch: { add: [1] } });
    expect(invalidate.mock.calls.map((call) => call[0]?.queryKey)).toEqual([
      ["vault", "account-1", "contact-groups"],
      ["vault", "account-1", "contacts"],
    ]);
  });
});
```

And one case for the composed object's new fields, inside the existing `useNameCollectionActions` describe:

```tsx
  it("reports a write in flight, so a screen needs no busy flag of its own", async () => {
    const routes = fakeRoutes();
    const answer = deferred<{ id: number; name: string }>();
    routes.create.mockReturnValue(answer.promise);

    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    expect(result.current.pending).toBe(false);

    let write: Promise<string> = Promise.resolve("");
    act(() => {
      write = result.current.create("Work");
    });
    await waitFor(() => expect(result.current.pending).toBe(true));

    answer.resolve({ id: 3, name: "Work" });
    await write;
    await waitFor(() => expect(result.current.pending).toBe(false));
  });

  it("reports the failure a write ended in", async () => {
    const routes = fakeRoutes();
    routes.create.mockRejectedValue(new Error("vault said no"));
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.create("Work")).rejects.toThrow("vault said no");
    await waitFor(() => expect(result.current.error?.message).toBe("vault said no"));
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/nameCollection.test.tsx`
Expected: the file fails to compile — `useSetNamedSetMembers` is not exported, and `createNameCollection` has no `chips` in its config type.

- [ ] **Step 3: Rewrite the write half of `nameCollection.ts`**

Replace the imports at the top of `web/src/lib/nameCollection.ts` with:

```ts
import { type InfiniteData, type UseMutationResult, useMutation } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import {
  type OffsetPage,
  useVaultCache,
  type VaultCacheEntries,
  useVaultQuery,
  type VaultQueryKey,
} from "./vaultQuery";
```

Add the chip types beside `MembersPatch`:

```ts
/** What a membership write answers with. */
export type MembersChanged = { added: number; removed: number };

/** A name to put on or take off some rows, as a screen asks for it. */
export type SetMembersVars = { name: string; patch: MembersPatch };

/**
 * A cached shape whose rows carry this collection's names as chips.
 *
 * A membership write patches these before the vault answers, so a ticked box
 * shows on a long list without a round trip. The collection describes them;
 * the screen does not, which is why no screen keeps an override map.
 */
export type ChipTarget = {
  /** Prefix of the entries to patch. */
  key: VaultQueryKey;
  /** Field the names sit in on a row. */
  field: "groups" | "tags";
  /** `pages` for an offset-paged list entry, `row` for one row on its own. */
  shape: "pages" | "row";
};
```

Add `chips: readonly ChipTarget[];` to both `NameCollectionConfig` and `NameCollection`, with the doc comment on the config field, and add `chips: config.chips,` to the object `createNameCollection` returns.

Add the two pure helpers after `fetchSets`:

```ts
/** One row of a list that shows names as chips. */
type ChipRow = { id: string | number } & Record<string, unknown>;

/**
 * Add or remove one name, matching letter case the way the lists match it.
 *
 * `Family` and `family` are one name to a person, so ticking the box when a
 * row already has the name under another spelling changes nothing.
 */
export function withName(names: readonly string[], name: string, enable: boolean): string[] {
  const has = names.some((n) => n.toLowerCase() === name.toLowerCase());
  if (enable) return has ? [...names] : [...names, name];
  return names.filter((n) => n.toLowerCase() !== name.toLowerCase());
}

/** Rewrite one cache entry so the rows named by `ids` gain or lose the name. */
export function patchChips(
  entry: unknown,
  target: ChipTarget,
  ids: ReadonlySet<string>,
  name: string,
  enable: boolean,
): unknown {
  if (!entry || ids.size === 0) return entry;
  const patchRow = (row: ChipRow): ChipRow => {
    if (!ids.has(String(row.id))) return row;
    const current = row[target.field];
    return {
      ...row,
      [target.field]: withName(Array.isArray(current) ? (current as string[]) : [], name, enable),
    };
  };
  if (target.shape === "row") return patchRow(entry as ChipRow);
  const paged = entry as InfiniteData<OffsetPage<ChipRow>>;
  if (!Array.isArray(paged.pages)) return entry;
  return {
    ...paged,
    pages: paged.pages.map((page) => ({ ...page, items: page.items.map(patchRow) })),
  };
}
```

Replace everything from `useNameCollectionActions` to the end of the file with:

```ts
/**
 * The id behind a name: from the cache, else from the vault once, else an
 * error and no request. The vault-once path covers creating a set and adding
 * to it before the invalidated list has come back.
 */
function useIdOf(collection: NameCollection): (name: string) => Promise<number> {
  const cache = useVaultCache();
  return useCallback(
    async (name: string) => {
      const wanted = name.trim().toLowerCase();
      const find = (sets: NamedSet[] | undefined) =>
        sets?.find((set) => set.name.toLowerCase() === wanted)?.id;
      const hit = find(cache.read<NamedSet[]>(collection.key));
      if (hit !== undefined) return hit;
      const fresh = find(
        await cache.fetch<NamedSet[]>(collection.key, (signal) => fetchSets(collection, signal)),
      );
      if (fresh !== undefined) return fresh;
      throw new Error(`${collection.label} not found`);
    },
    [cache, collection],
  );
}

/** A name nobody may use, refused before any request is sent. */
function checkedName(collection: NameCollection, name: string): string {
  const trimmed = name.trim();
  if (!trimmed) throw new Error("name required");
  if (collection.isReserved(trimmed)) throw new Error(collection.reservedError(trimmed));
  return trimmed;
}

/** This collection's list, plus every list that shows its names as chips. */
function useMarkStale(collection: NameCollection): () => Promise<void> {
  const cache = useVaultCache();
  return useCallback(
    async () => {
      await cache.invalidate(collection.key, ...collection.invalidates);
    },
    [cache, collection],
  );
}

export function useCreateNamedSet(
  collection: NameCollection,
): UseMutationResult<NamedSet, Error, string> {
  const markStale = useMarkStale(collection);
  return useMutation<NamedSet, Error, string>({
    mutationFn: async (name) => collection.routes.create({ name: checkedName(collection, name) }),
    onSettled: markStale,
  });
}

export function useRenameNamedSet(
  collection: NameCollection,
): UseMutationResult<NamedSet, Error, { from: string; to: string }> {
  const idOf = useIdOf(collection);
  const markStale = useMarkStale(collection);
  return useMutation<NamedSet, Error, { from: string; to: string }>({
    mutationFn: async ({ from, to }) => {
      const name = checkedName(collection, to);
      return collection.routes.update(await idOf(from), { name });
    },
    onSettled: markStale,
  });
}

export function useDeleteNamedSet(
  collection: NameCollection,
): UseMutationResult<void, Error, string> {
  const idOf = useIdOf(collection);
  const markStale = useMarkStale(collection);
  return useMutation<void, Error, string>({
    mutationFn: async (name) => collection.routes.remove(await idOf(name)),
    onSettled: markStale,
  });
}

/** The rows as they were before an optimistic membership write touched them. */
type ChipSnapshot = { entries: VaultCacheEntries };

/**
 * Put rows in or out of one set, drawn before the vault answers.
 *
 * The chips change on the list and on the open contact at once, the old rows
 * come back if the vault refuses, and every list showing the name is marked
 * stale afterwards. Two of these can be in flight together — the Clear all
 * button fires one per name — and each rolls back only its own change.
 */
export function useSetNamedSetMembers(
  collection: NameCollection,
): UseMutationResult<MembersChanged, Error, SetMembersVars, ChipSnapshot> {
  const cache = useVaultCache();
  const idOf = useIdOf(collection);
  const markStale = useMarkStale(collection);
  return useMutation<MembersChanged, Error, SetMembersVars, ChipSnapshot>({
    mutationFn: async ({ name, patch }) =>
      collection.routes.updateMembers(await idOf(name), {
        add: patch.add ?? [],
        remove: patch.remove ?? [],
      }),
    onMutate: async ({ name, patch }) => {
      const add = new Set((patch.add ?? []).map(String));
      const remove = new Set((patch.remove ?? []).map(String));
      for (const target of collection.chips) await cache.cancel(target.key);
      const entries = collection.chips.flatMap((target) => cache.snapshot(target.key));
      for (const target of collection.chips) {
        cache.patch<unknown>(target.key, (entry) =>
          patchChips(patchChips(entry, target, add, name, true), target, remove, name, false),
        );
      }
      return { entries };
    },
    onError: (_error, _vars, context) => {
      if (context) cache.restore(context.entries);
    },
    onSettled: markStale,
  });
}

/** What a screen or the sidebar does to one of these collections. */
export type NameCollectionActions = {
  create: (name: string) => Promise<string>;
  rename: (from: string, to: string) => Promise<string>;
  remove: (name: string) => Promise<void>;
  setMembers: (name: string, patch: MembersPatch) => Promise<MembersChanged>;
  invalidate: () => Promise<void>;
  /** Any of the four in flight, so a screen needs no busy flag of its own. */
  pending: boolean;
  /** The most recent failure, or null. */
  error: Error | null;
};

/**
 * The four writes, behind names.
 *
 * Screens keep passing names; the ids, the optimistic chips, the rollback and
 * the invalidation all belong to the mutations above.
 */
export function useNameCollectionActions(collection: NameCollection): NameCollectionActions {
  const cache = useVaultCache();
  const createSet = useCreateNamedSet(collection);
  const renameSet = useRenameNamedSet(collection);
  const deleteSet = useDeleteNamedSet(collection);
  const members = useSetNamedSetMembers(collection);

  const create = createSet.mutateAsync;
  const rename = renameSet.mutateAsync;
  const remove = deleteSet.mutateAsync;
  const setMembers = members.mutateAsync;
  const pending =
    createSet.isPending || renameSet.isPending || deleteSet.isPending || members.isPending;
  const error = createSet.error ?? renameSet.error ?? deleteSet.error ?? members.error;

  return useMemo(
    () => ({
      create: async (name: string) => (await create(name)).name,
      rename: async (from: string, to: string) => (await rename({ from, to })).name,
      remove: (name: string) => remove(name),
      setMembers: (name: string, patch: MembersPatch) => setMembers({ name, patch }),
      invalidate: () => cache.invalidate(collection.key),
      pending,
      error,
    }),
    [create, rename, remove, setMembers, cache, collection.key, pending, error],
  );
}
```

- [ ] **Step 4: Give the two collections their chip targets**

`web/src/lib/contactGroups.ts`, in the `createNameCollection` call, after `invalidates`:

```ts
  // A ticked box shows on the contact rows and on the open contact at once.
  chips: [
    { key: keys.contacts.lists, field: "groups", shape: "pages" },
    { key: keys.contacts.details, field: "groups", shape: "row" },
  ],
```

and at the end of the file, beside `useContactGroupActions`:

```ts
/** Put contacts in or out of one Contact Group, drawn before the vault answers. */
export function useSetContactGroupMembers() {
  return useSetNamedSetMembers(contactGroups);
}
```

with `useSetNamedSetMembers` added to the import from `./nameCollection`.

`web/src/lib/messageTags.ts`, the same two edits:

```ts
  chips: [{ key: keys.conversations.lists, field: "tags", shape: "pages" }],
```

```ts
/** Put conversations in or out of one Message Tag, drawn before the vault answers. */
export function useSetMessageTagMembers() {
  return useSetNamedSetMembers(messageTags);
}
```

- [ ] **Step 5: Let `NavEntityList` read the mutation's pending flag**

In `web/src/components/NavEntityList.tsx`:
- Replace `const [busy, setBusy] = useState(false);` with `const busy = actions.pending;`.
- Delete the six `setBusy(true);` / `setBusy(false);` lines in `create`, `rename` and `remove`, and the `finally` blocks that hold them, keeping each `try`/`catch`.
- Drop `useState` from the `react` import if nothing else in the file uses it (`createOpen`, `renameFor` and `menuFor` do, so it stays).

Each of the three ends up as, for example:

```ts
  const remove = async (name: string) => {
    setError(null);
    setMenuFor(null);
    try {
      await actions.remove(name);
      if (location.pathname === `${copy.routeBase}/${slug(name)}`) {
        navigate(copy.fallbackRoute);
      }
    } catch (err) {
      setError(apiErrorMessage(err, copy.deleteError));
    }
  };
```

- [ ] **Step 6: Delete the hooks nothing calls any more**

In `web/src/lib/vaultQuery.ts`, delete `useVaultInvalidate`, `useVaultCached` and `useVaultFetchFresh` with their doc comments. Keep `useVaultSetCached`: `savedSearches.ts` and `useAccountProfile.ts` still call it until Tasks 6 and 8.

In `web/src/lib/vaultQuery.test.tsx`, delete the `describe("useVaultCached and useVaultFetchFresh")` block and drop those two names from the import. The `useVaultCache` block added in Task 2 covers the same ground.

Run: `grep -rn 'useVaultInvalidate\|useVaultCached\|useVaultFetchFresh' web/src`
Expected: no output.

- [ ] **Step 7: Run the checks**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean, Biome clean, all suites pass. `GroupsNav.test.tsx`, `MessageTagsNav.test.tsx` and `AddressBookSection.test.tsx` exercise `NavEntityList` and `useContactGroupActions`; read any failure before changing them, since the interface they use has not changed.

- [ ] **Step 8: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/nameCollection.ts web/src/lib/nameCollection.test.tsx web/src/lib/contactGroups.ts web/src/lib/messageTags.ts web/src/components/NavEntityList.tsx web/src/lib/vaultQuery.ts web/src/lib/vaultQuery.test.tsx
git commit -m "feat(web): make Contact Group and Message Tag writes mutations

Each of the four writes is now a mutation that owns what it draws early,
what it puts back when the vault refuses, and what it marks stale. The
chips a membership write paints are described by the collection, so the
screens no longer need a map of names they have drawn but not confirmed.
The sidebar reads the mutation's pending flag instead of keeping a busy
flag of its own.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 4: The contact list stops keeping its own chips

**Files:**
- Modify: `web/src/screens/ContactList.tsx`

**Interfaces:**
- Consumes `useSetContactGroupMembers()` from Task 3.
- Removes `groupOverrides`, `groupOverridesRef`, `groupsForContact`, `withGroupMembership`, and every call to `useContactDetailCache`.
- `useContactGroupActions()` stays, for `create` from the Groups menu.

- [ ] **Step 1: Delete the override map and its helpers**

In `web/src/screens/ContactList.tsx`:

- Delete the import `import { useContactDetailCache } from "../lib/contactDetail";`.
- Add `useSetContactGroupMembers` to the import from `../lib/contactGroups`.
- Delete the two helper functions above the component, `groupsForContact` and `withGroupMembership`, with their doc comments. Nothing else uses them.
- Delete these three lines from the component body:

  ```tsx
  const [groupOverrides, setGroupOverrides] = useState<Record<string, string[]>>({});
  const groupOverridesRef = useRef(groupOverrides);
  groupOverridesRef.current = groupOverrides;
  ```

- Delete `const detailCache = useContactDetailCache();` and add, beside `groupActions`:

  ```tsx
  const setGroupMembers = useSetContactGroupMembers();
  ```

- In the filter effect, delete the line `setGroupOverrides({});`.
- In the `displayContacts` memo, drop the override map:

  ```tsx
  const displayContacts = useMemo(
    () =>
      [...filteredContacts]
        .filter((c) => contactBelongsToGroup(c.groups, groupFilter))
        .sort((a, b) => compareContactsByName(a.name, b.name, nameSort.sort, nameSort.order)),
    [filteredContacts, nameSort, groupFilter],
  );
  ```

- [ ] **Step 2: Read the chips from the rows, and write through the mutation**

Replace `groupChecks`, `applyMembership` and `clearAllMembership` with:

```tsx
  const groupChecks = useMemo(
    () =>
      checksFromMembers(
        allGroups,
        assignTargets.map((c) => c.groups ?? []),
      ),
    [allGroups, assignTargets],
  );

  const applyMembership = useCallback(
    (name: string, enable: boolean) => {
      const ids = assignTargetsRef.current
        .map((c) => Number(c.id))
        .filter((id) => Number.isFinite(id) && id > 0);
      if (ids.length === 0) return Promise.resolve();
      // A refused write puts the chips back in the mutation's onError, and the
      // chips going back is the report, so there is nothing to handle here.
      return setGroupMembers
        .mutateAsync({ name, patch: enable ? { add: ids } : { remove: ids } })
        .then(
          () => undefined,
          () => undefined,
        );
    },
    [setGroupMembers.mutateAsync],
  );

  /** Drop every group on the selected contacts: one write per name, each with its own rollback. */
  const clearAllMembership = useCallback(async () => {
    const targets = assignTargetsRef.current;
    const ids = targets.map((c) => Number(c.id)).filter((id) => Number.isFinite(id) && id > 0);
    if (ids.length === 0) return;
    const names = new Set<string>();
    for (const c of targets) {
      for (const g of c.groups ?? []) names.add(g);
    }
    if (names.size === 0) return;
    await Promise.allSettled(
      [...names].map((name) => setGroupMembers.mutateAsync({ name, patch: { remove: ids } })),
    );
  }, [setGroupMembers.mutateAsync]);
```

The `setRightToolbar` effect keeps its dependency list, with `applyMembership` and `clearAllMembership` still in it.

- [ ] **Step 3: Run the checks**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean; Biome clean; all suites pass. If `useRef` or `useState` is now unused in this file, Biome says so — remove the unused import.

Run: `grep -n 'groupOverrides\|detailCache\|withGroupMembership\|groupsForContact' web/src/screens/ContactList.tsx`
Expected: no output.

- [ ] **Step 4: Check it in the browser**

Start the vault and the web app, in two terminals, without resetting anything:

```bash
./scripts/run-vault-dev.sh    # terminal 1 — never --reset or --reset-demo
cd web && npm run dev         # terminal 2
```

Sign in at `http://127.0.0.1:5173`. With the Playwright MCP or by hand:
1. Open Contacts, check two rows, open the Groups menu and tick `Family`. Both rows show the chip at once.
2. Untick it. The chips go at once.
3. Open one of those contacts. The drawer's group chips agree with the row.
4. Press Clear all with two rows checked that are in different groups. Every chip goes.

- [ ] **Step 5: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/screens/ContactList.tsx
git commit -m "refactor(web): let the contact list read its chips from the cache

The list kept a map of group names it had drawn but not confirmed, a ref
to read that map inside callbacks, a copy of the same names written into
the open contact, and about seventy lines of undo. All four are the
membership mutation's now, so the rows simply show what the cache holds.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 5: The conversation list stops hiding the fresh answer

**Files:**
- Modify: `web/src/screens/ConversationList.tsx`

**Interfaces:**
- Consumes `useSetMessageTagMembers()` from Task 3 and `keys.conversations.list` from Task 1.
- Removes `tagOverrides`, `membershipRev` and `displayConversations`.
- This is the task that fixes the stale Tags-menu checkbox from issue #299.

- [ ] **Step 1: Delete the override map and the counter**

In `web/src/screens/ConversationList.tsx`:

- Add `useSetMessageTagMembers` to the import from `../lib/messageTags`, and `import { keys } from "../lib/vaultKeys";`.
- Delete these two lines:

  ```tsx
  const [tagOverrides, setTagOverrides] = useState<Record<string, string[]>>({});
  const [membershipRev, setMembershipRev] = useState(0);
  ```

- Add, beside `tagActions`:

  ```tsx
  const setTagMembers = useSetMessageTagMembers();
  ```

- In the effect that clears the checked rows when the query changes, delete `setTagOverrides({});`.
- Put the key back to what the factory builds, with no counter in it:

  ```tsx
  } = useVaultPagedList(
    keys.conversations.list({ q: debouncedQ, sort: sortState.sort, order: sortState.order }),
    fetchPage,
  );
  ```

- Delete the `displayConversations` memo and replace all twelve remaining uses of `displayConversations` with `conversations`.

- [ ] **Step 2: Write through the mutation**

Replace `applyMembership` with:

```tsx
  const applyMembership = useCallback(
    (name: string, enable: boolean) => {
      const ids = targetConversations
        .map((c) => Number(c.id))
        .filter((id) => Number.isFinite(id) && id > 0);
      if (ids.length === 0) return Promise.resolve();
      // The tags on the rows change in the cache before the vault answers and
      // go back if it refuses, so nothing here has to remember them. Marking
      // every conversation stale afterwards is what used to need the
      // `membershipRev` counter in the query key.
      return setTagMembers
        .mutateAsync({ name, patch: enable ? { add: ids } : { remove: ids } })
        .then(
          () => undefined,
          () => undefined,
        );
    },
    [targetConversations, setTagMembers.mutateAsync],
  );
```

The `query` dependency goes with the counter it guarded.

- [ ] **Step 3: Run the checks**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean; Biome clean; all suites pass.

Run: `grep -n 'tagOverrides\|membershipRev\|displayConversations' web/src/screens/ConversationList.tsx`
Expected: no output.

- [ ] **Step 4: Check the reported bug is gone**

With the vault and the web app running as in Task 4 (again, never `--reset` or `--reset-demo`):

1. Open Messages, select a conversation, open the Tags menu and tick `Holiday`. The row shows the chip at once.
2. Leave that conversation selected. In the sidebar, rename `Holiday` to `Vacation`.
3. Open the Tags menu again. It lists `Vacation`, ticked, and no `Holiday`. Before this task the box was unticked and the old name was still on the row — that is the symptom issue #299 reports.
4. Search `tag:Vacation`, untick the tag on a row: the row leaves the filtered list without a reload.
5. Delete `Vacation` in the sidebar. The chip goes from the row.

- [ ] **Step 5: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/screens/ConversationList.tsx
git commit -m "fix(web): show a renamed Message Tag in the Tags menu at once

The list kept the tag names it had drawn in a map keyed by conversation,
and preferred that map over the rows the vault sent. A rename therefore
refreshed the rows and stayed hidden underneath the old names, so the
menu still showed the old name ticked. The rows now come straight from
the cache, which the rename marks stale, and the counter the screen used
to smuggle into its query key to force a refetch is gone with it.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 6: Saved Search writes as mutations

**Files:**
- Modify: `web/src/lib/savedSearches.ts` (everything from `useSavedSearchActions` down), `web/src/lib/savedSearches.test.ts`

**Interfaces:**
- Produces `useCreateSavedSearch()`, `useUpdateSavedSearch()`, `useDeleteSavedSearch()`, each `UseMutationResult<SavedSearch[], Error, …>`.
- `useSavedSearchActions()` keeps `create(name, query)`, `update(id, name, query)` and `remove(id)` — `LeftPanel.tsx` calls all three — and gains `pending` and `error`.
- Stops calling `useVaultSetCached`.

- [ ] **Step 1: Write the failing test**

Append to `web/src/lib/savedSearches.test.ts`, inside the `useSavedSearchActions` describe:

```ts
  it("reports a write in flight and the failure it ended in", async () => {
    let refuse: (reason: Error) => void = () => {};
    create.mockReturnValue(
      new Promise((_resolve, reject) => {
        refuse = reject;
      }),
    );
    const { result } = renderHook(() => useSavedSearchActions(), { wrapper });
    expect(result.current.pending).toBe(false);

    const write = result.current.create("Family", "is:group");
    await waitFor(() => expect(result.current.pending).toBe(true));

    refuse(new Error("vault said no"));
    await expect(write).rejects.toThrow("vault said no");
    await waitFor(() => expect(result.current.error?.message).toBe("vault said no"));
    expect(result.current.pending).toBe(false);
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/savedSearches.test.ts`
Expected: the new case fails — `result.current.pending` is `undefined`, not `false`.

- [ ] **Step 3: Rewrite the write half**

In `web/src/lib/savedSearches.ts`, replace the `useVaultSetCached` import with

```ts
import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { useVaultCache, useVaultQuery } from "./vaultQuery";
```

and replace `useSavedSearchActions` and everything below it with:

```ts
/**
 * Every write answers with the whole list, so each stores that answer where
 * the sidebar reads it and asks for nothing again.
 */
function useSavedSearchWrite<V>(
  write: (vars: V) => Promise<ListResponse>,
): UseMutationResult<SavedSearch[], Error, V> {
  const cache = useVaultCache();
  return useMutation<SavedSearch[], Error, V>({
    mutationFn: async (vars) => listFrom(await write(vars)),
    onSuccess: (list) => {
      cache.set(keys.savedSearches.all, list);
    },
  });
}

export function useCreateSavedSearch(): UseMutationResult<
  SavedSearch[],
  Error,
  { name: string; query: string }
> {
  return useSavedSearchWrite((body) => createVaultSavedSearch(body));
}

export function useUpdateSavedSearch(): UseMutationResult<
  SavedSearch[],
  Error,
  { id: number; name: string; query: string }
> {
  return useSavedSearchWrite(({ id, name, query }) => updateVaultSavedSearch(id, { name, query }));
}

export function useDeleteSavedSearch(): UseMutationResult<SavedSearch[], Error, number> {
  return useSavedSearchWrite((id) => deleteVaultSavedSearch(id));
}

export type SavedSearchActions = {
  create: (name: string, query: string) => Promise<void>;
  update: (id: number, name: string, query: string) => Promise<void>;
  remove: (id: number) => Promise<void>;
  /** Any of the three in flight. */
  pending: boolean;
  /** The most recent failure, or null. */
  error: Error | null;
};

/** Create, rename and delete saved searches, as the sidebar asks for them. */
export function useSavedSearchActions(): SavedSearchActions {
  const createSearch = useCreateSavedSearch();
  const updateSearch = useUpdateSavedSearch();
  const deleteSearch = useDeleteSavedSearch();

  const create = createSearch.mutateAsync;
  const update = updateSearch.mutateAsync;
  const remove = deleteSearch.mutateAsync;
  const pending = createSearch.isPending || updateSearch.isPending || deleteSearch.isPending;
  const error = createSearch.error ?? updateSearch.error ?? deleteSearch.error;

  return useMemo(
    () => ({
      create: async (name: string, query: string) => {
        await create({ name, query });
      },
      update: async (id: number, name: string, query: string) => {
        await update({ id, name, query });
      },
      remove: async (id: number) => {
        await remove(id);
      },
      pending,
      error,
    }),
    [create, update, remove, pending, error],
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx tsc --noEmit -p . && npx vitest run src/lib/savedSearches.test.ts src/components/LeftPanel.test.tsx`
Expected: both suites pass, including "takes the refreshed list from a mutation instead of asking again", which is what proves `onSuccess` still writes the answer rather than asking the vault twice.

- [ ] **Step 5: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/savedSearches.ts web/src/lib/savedSearches.test.ts
git commit -m "refactor(web): make Saved Search writes mutations

The three writes were hand-rolled async functions in a useMemo object.
They are mutations now, so a caller can see one in flight and see why one
failed. Each still takes the whole list out of the vault's answer and
puts it where the sidebar reads it, which is why none of them asks the
vault for the list again.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 7: One contact, read and written by one module

**Files:**
- Modify: `web/src/lib/contactDetail.ts`, `web/src/components/ContactDrawer.tsx`, `web/src/components/contactDrawer/ContactDrawerHandles.tsx`, `web/src/components/contactDrawer/useHandleMutations.ts`
- Create: `web/src/lib/contactDetail.test.tsx`

**Interfaces:**
- Produces `useUpdateContact(): UseMutationResult<ContactDetail, Error, { contactId: string; body: ContactChange }>` and `type ContactChange` from `contactDetail.ts`.
- Deletes `useContactDetailCache` — Task 4 removed its last reader in the contact list, and this task removes the drawer's.
- `useHandleMutations({ contactId })` loses its `onHandlesChanged` argument; `ContactDrawerHandles` loses the prop.

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/contactDetail.test.tsx`:

```tsx
/** @vitest-environment jsdom */

/**
 * One contact, read and written through one entry.
 *
 * The vault answers a change with the contact as it now stands, so the drawer
 * should show the new name without asking again — and the list pages, which
 * show the name too, should be the only thing marked stale.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useContactDetail, useUpdateContact } from "./contactDetail";
import { getContact, updateContact } from "./vaultApi";
import { keys } from "./vaultKeys";

vi.mock("./auth", () => ({ useAuth: () => ({ accountId: "account-1" }) }));

vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  getContact: vi.fn(),
  updateContact: vi.fn(),
}));

const read = vi.mocked(getContact);
const write = vi.mocked(updateContact);

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function contact(name: string) {
  return {
    id: 7,
    name,
    last_modified: "2024-01-01T00:00:00Z",
    handles: [],
    groups: ["Family"],
    direct_conversations: 1,
    group_conversations: 0,
    message_count: 3,
  } as unknown as Awaited<ReturnType<typeof getContact>>;
}

beforeEach(() => {
  vi.clearAllMocks();
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useUpdateContact", () => {
  it("puts the answered contact where the drawer reads it, without asking again", async () => {
    read.mockResolvedValue(contact("Ada"));
    write.mockResolvedValue(contact("Ada Lovelace"));

    const both = renderHook(
      () => ({ detail: useContactDetail("7"), update: useUpdateContact() }),
      { wrapper },
    );
    await waitFor(() => expect(both.result.current.detail.detail?.name).toBe("Ada"));
    expect(read).toHaveBeenCalledTimes(1);

    await both.result.current.update.mutateAsync({ contactId: "7", body: { name: "Ada Lovelace" } });

    expect(write).toHaveBeenCalledWith("7", { name: "Ada Lovelace" });
    await waitFor(() => expect(both.result.current.detail.detail?.name).toBe("Ada Lovelace"));
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("marks the contact list pages stale, and not the contact it just wrote", async () => {
    write.mockResolvedValue(contact("Ada Lovelace"));
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useUpdateContact(), { wrapper });
    await result.current.mutateAsync({ contactId: "7", body: { name: "Ada Lovelace" } });
    expect(invalidate.mock.calls.map((call) => call[0]?.queryKey)).toEqual([
      ["vault", "account-1", "contacts", "list"],
    ]);
    expect(client.getQueryData(["vault", "account-1", ...keys.contacts.detail("7")])).toBeDefined();
  });

  it("reports a refusal instead of writing anything", async () => {
    client.setQueryData(["vault", "account-1", "contacts", "detail", "7"], contact("Ada"));
    write.mockRejectedValue(new Error("handle already linked"));
    const { result } = renderHook(() => useUpdateContact(), { wrapper });
    await expect(
      result.current.mutateAsync({ contactId: "7", body: { name: "Ada Lovelace" } }),
    ).rejects.toThrow("handle already linked");
    expect(
      client.getQueryData<{ name: string }>(["vault", "account-1", "contacts", "detail", "7"])?.name,
    ).toBe("Ada");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/contactDetail.test.tsx`
Expected: the file fails to compile — `useUpdateContact` is not exported from `./contactDetail`.

- [ ] **Step 3: Replace the hand-built cache with a mutation**

Rewrite `web/src/lib/contactDetail.ts` below its doc comment as:

```ts
import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { getContact, updateContact } from "./vaultApi";
import type { components } from "./vaultApi.types";
import { keys } from "./vaultKeys";
import { useVaultCache, useVaultQuery } from "./vaultQuery";

export type ContactDetail = components["schemas"]["ContactDetail"];
export type ContactHandle = components["schemas"]["ContactHandleInfo"];
/** One change to a contact: its name, or one handle added, updated or removed. */
export type ContactChange = components["schemas"]["ContactMutationBody"];

/** The contact behind an open drawer. Skipped entirely when no contact is open. */
export function useContactDetail(contactId: string | null): {
  detail: ContactDetail | null;
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(
    keys.contacts.detail(contactId ?? ""),
    (signal) => getContact(contactId ?? "", { signal }),
    { enabled: contactId !== null },
  );
  return { detail: contactId ? (data ?? null) : null, loading: isPending };
}

/**
 * Change one thing about a contact.
 *
 * The vault answers with the contact as it now stands, so the answer goes
 * straight into the entry the drawer reads and nothing asks for it again. The
 * list pages are marked stale because they show the name too; the contact's
 * own entry is not, because it is already right.
 */
export function useUpdateContact(): UseMutationResult<
  ContactDetail,
  Error,
  { contactId: string; body: ContactChange }
> {
  const cache = useVaultCache();
  return useMutation<ContactDetail, Error, { contactId: string; body: ContactChange }>({
    mutationFn: ({ contactId, body }) => updateContact(contactId, body),
    onSuccess: (detail, { contactId }) => {
      cache.set(keys.contacts.detail(contactId), detail);
    },
    onSettled: () => cache.invalidate(keys.contacts.lists),
  });
}
```

Update the module's doc comment: `useContactDetailCache` was the last hand-built piece of it, and the group chips a contact-list edit writes are now the membership mutation's optimistic patch.

- [ ] **Step 4: Point the drawer at the mutation**

`web/src/components/ContactDrawer.tsx`:
- Replace the two imports

  ```tsx
  import { type ContactDetail, useContactDetail, useContactDetailCache } from "../lib/contactDetail";
  import { updateContact } from "../lib/vaultApi";
  ```

  with

  ```tsx
  import { type ContactDetail, useContactDetail, useUpdateContact } from "../lib/contactDetail";
  ```

- Replace `const detailCache = useContactDetailCache();` with `const updateContact = useUpdateContact();`.
- Delete `loadDetail` and its comment.
- In `saveName`, replace the write and the reload with one call:

  ```tsx
      await updateContact.mutateAsync({ contactId, body: { name: nameValue } });
      setEditingName(false);
  ```

- Delete the `onHandlesChanged={loadDetail}` prop from `<ContactDrawerHandles …>`.

`web/src/components/contactDrawer/ContactDrawerHandles.tsx`:
- Delete `onHandlesChanged,` from the destructured props and `onHandlesChanged: () => void;` from the prop type.
- Change the call to `useHandleMutations({ contactId })`.

`web/src/components/contactDrawer/useHandleMutations.ts`, in full below its imports:

```ts
export function useHandleMutations({ contactId }: { contactId: string }) {
  const [adding, setAdding] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<RemoveIdentityTarget | null>(null);
  const updateContact = useUpdateContact();
  const busy = updateContact.isPending;
  // The dialogs stay open on a refusal and show this, so a person can retry.
  const error = updateContact.error ? updateContact.error.message : "";
  const reset = updateContact.reset;

  useEffect(() => {
    void contactId;
    setAdding(false);
    setRemoveTarget(null);
    reset();
  }, [contactId, reset]);

  const requestRemoveHandle = (h: ContactHandle) => {
    if (busy) return;
    setRemoveTarget({
      handle: h.handle,
      service: h.service ?? null,
      serviceLabel: formatHandleServiceLabel(h.handle, h.service),
      threadCount: conversationCount(h),
    });
  };

  const confirmRemoveHandle = () => {
    if (!removeTarget || busy) return;
    const handle = removeTarget.handle;
    const service = handleServiceSelectValue(handle, removeTarget.service);
    updateContact.mutate(
      { contactId, body: { remove_handle: { handle, service } } },
      { onSuccess: () => setRemoveTarget(null) },
    );
  };

  const confirmAdd = (args: { handle: string; service: string }) => {
    if (busy) return;
    updateContact.mutate(
      { contactId, body: { add_handle: { handle: args.handle, service: args.service } } },
      { onSuccess: () => setAdding(false) },
    );
  };

  return {
    adding,
    setAdding,
    busy,
    error,
    removeTarget,
    setRemoveTarget,
    requestRemoveHandle,
    confirmRemoveHandle,
    confirmAdd,
  };
}
```

with the imports

```ts
import { useEffect, useState } from "react";
import { type ContactHandle, useUpdateContact } from "../../lib/contactDetail";
import { formatHandleServiceLabel, handleServiceSelectValue } from "./contactDrawerTypes";
import { conversationCount, type RemoveIdentityTarget } from "./handleTableLogic";
```

`mutate` never rejects, so the two `void confirmAdd(args)` / `void confirmRemoveHandle()` call sites in `ContactDrawerHandles.tsx` keep working unchanged.

- [ ] **Step 5: Run the checks**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean, Biome clean, all suites pass, including `ContactDrawer.test.tsx` and the three `contactDrawer/` suites. `ContactDrawer.test.tsx` seeds the cache under `keys.contacts.detail` since Task 1; a rename case that expected a second `getContact` should now expect none, because the answer is written straight into the entry — update it if it fails for that reason.

Run: `grep -rn 'useContactDetailCache\|onHandlesChanged' web/src`
Expected: no output.

- [ ] **Step 6: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/contactDetail.ts web/src/lib/contactDetail.test.tsx web/src/components/ContactDrawer.tsx web/src/components/contactDrawer/ContactDrawerHandles.tsx web/src/components/contactDrawer/useHandleMutations.ts
git commit -m "refactor(web): write a contact through one mutation

Renaming a contact or changing its handles went through a hand-built
cache helper and a callback passed down two components, whose only job
was to say the contact had changed. The vault answers each of those
writes with the contact as it now stands, so the mutation stores that
answer where the drawer reads it and marks only the list pages stale.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 8: The account profile writes through a mutation

**Files:**
- Modify: `web/src/lib/useAccountProfile.ts`, `web/src/screens/settings/ProfileSettingsPanel.tsx`, `web/src/screens/ImportScreen.tsx`, `web/src/lib/vaultQuery.ts`
- Create: `web/src/lib/useAccountProfile.test.tsx`

**Interfaces:**
- Produces `useUpdateAccountProfile(): UseMutationResult<AccountProfile, Error, AccountProfileChange>` and `type AccountProfileChange`.
- `useAccountProfile()` returns `{ profile, loading, error }`; `setProfile` and `reload` are removed. Nothing calls `reload`; `setProfile` had two callers, both updated here.
- `fetchAccountProfileFor` and `useFetchAccountProfile` are unchanged.
- Deletes `useVaultSetCached` from `vaultQuery.ts`: this was its last caller.

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/useAccountProfile.test.tsx`:

```tsx
/** @vitest-environment jsdom */

/**
 * The profile is one entry that every screen reads and two screens write.
 *
 * A write answers with the whole profile, so it belongs in that entry
 * directly: asking the vault again would show the old name for as long as the
 * round trip takes.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAccountProfile, useUpdateAccountProfile } from "./useAccountProfile";
import { getAccountProfile, updateAccountProfile } from "./vaultApi";

vi.mock("./auth", () => ({ useAuth: () => ({ accountId: "account-1" }) }));

vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  getAccountProfile: vi.fn(),
  updateAccountProfile: vi.fn(),
}));

const read = vi.mocked(getAccountProfile);
const write = vi.mocked(updateAccountProfile);

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function profile(name: string) {
  return { preferred_name: name, phones: [], emails: [] } as unknown as Awaited<
    ReturnType<typeof getAccountProfile>
  >;
}

beforeEach(() => {
  vi.clearAllMocks();
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useUpdateAccountProfile", () => {
  it("shows the answered profile without asking the vault again", async () => {
    read.mockResolvedValue(profile("Ada"));
    write.mockResolvedValue(profile("Ada Lovelace"));

    const both = renderHook(
      () => ({ profile: useAccountProfile(), update: useUpdateAccountProfile() }),
      { wrapper },
    );
    await waitFor(() => expect(both.result.current.profile.profile?.preferred_name).toBe("Ada"));
    expect(read).toHaveBeenCalledTimes(1);

    await both.result.current.update.mutateAsync({ preferred_name: "Ada Lovelace" });

    expect(write).toHaveBeenCalledWith({ preferred_name: "Ada Lovelace" });
    await waitFor(() =>
      expect(both.result.current.profile.profile?.preferred_name).toBe("Ada Lovelace"),
    );
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("leaves the profile alone when the vault refuses", async () => {
    read.mockResolvedValue(profile("Ada"));
    write.mockRejectedValue(new Error("that address is already claimed"));

    const both = renderHook(
      () => ({ profile: useAccountProfile(), update: useUpdateAccountProfile() }),
      { wrapper },
    );
    await waitFor(() => expect(both.result.current.profile.profile?.preferred_name).toBe("Ada"));

    await expect(
      both.result.current.update.mutateAsync({ handles: [{ handle: "+15550000", service: "phone" }] }),
    ).rejects.toThrow("that address is already claimed");
    expect(both.result.current.profile.profile?.preferred_name).toBe("Ada");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/useAccountProfile.test.tsx`
Expected: the file fails to compile — `useUpdateAccountProfile` is not exported.

- [ ] **Step 3: Rewrite the hook and add the mutation**

In `web/src/lib/useAccountProfile.ts`, replace the read hook and add the write:

```ts
export function useAccountProfile(): {
  profile: AccountProfile | null;
  loading: boolean;
  error: string;
} {
  const { data, isPending, error } = useVaultQuery(keys.accountProfile.all, (signal) =>
    getAccountProfile({ signal }),
  );
  return { profile: data ?? null, loading: isPending, error: error ? error.message : "" };
}

/** What a change to the profile can carry: a name, handles to add, handles to drop. */
export type AccountProfileChange = Parameters<typeof updateAccountProfile>[0];

/**
 * Change the account's own name or handles.
 *
 * The vault answers with the profile as it now stands, so that answer goes
 * into the entry every screen reads. Nothing is marked stale: there is nothing
 * left to refresh.
 */
export function useUpdateAccountProfile(): UseMutationResult<
  AccountProfile,
  Error,
  AccountProfileChange
> {
  const cache = useVaultCache();
  return useMutation<AccountProfile, Error, AccountProfileChange>({
    mutationFn: (body) => updateAccountProfile(body),
    onSuccess: (profile) => {
      cache.set(keys.accountProfile.all, profile);
    },
  });
}
```

with imports `import { type UseMutationResult, useMutation, useQueryClient } from "@tanstack/react-query";`, `updateAccountProfile` added to the `vaultApi` import, and `useVaultCache` in place of `useVaultSetCached`. `useCallback` is no longer used by this file except in `useFetchAccountProfile`, which keeps it.

- [ ] **Step 4: Point the two writers at it**

`web/src/screens/settings/ProfileSettingsPanel.tsx`:
- `const { profile, setProfile, loading, error: loadError } = useAccountProfile();` becomes

  ```tsx
  const { profile, loading, error: loadError } = useAccountProfile();
  const updateProfile = useUpdateAccountProfile();
  ```

  with `useUpdateAccountProfile` added to the import and `updateAccountProfile` removed from the `vaultApi` import.
- Delete `const [handleBusy, setHandleBusy] = useState(false);` and add `const handleBusy = updateProfile.isPending;`.
- In all three handlers, `await updateAccountProfile({…})` becomes `await updateProfile.mutateAsync({…})`, the `setProfile(updated)` line goes, and the `setHandleBusy(true)` / `finally { setHandleBusy(false); }` lines go. The `try`/`catch` that turns a refusal into `nameError` or `handleError` stays, as does each check that the vault really did add or remove the handle.

`web/src/screens/ImportScreen.tsx`:
- `const { profile, setProfile } = useAccountProfile();` becomes

  ```tsx
  const { profile } = useAccountProfile();
  const updateProfile = useUpdateAccountProfile();
  ```

- Delete `const [identityAddBusy, setIdentityAddBusy] = useState(false);` and add `const identityAddBusy = updateProfile.isPending;`.
- In `addIdentityToProfile`, `const updated = await updateAccountProfile({ handles: [{ handle: value, service }] });` becomes `const updated = await updateProfile.mutateAsync({ handles: [{ handle: value, service }] });`, `setProfile(updated)` goes, and the `setIdentityAddBusy` calls with their `finally` go. The `catch` that sets `identityAddError` stays, and so does the `identityOnProfile(value, updated)` check.
- Remove `updateAccountProfile` from the `vaultApi` import if `matchContacts` is the only other name on it.

- [ ] **Step 5: Delete the hook nothing calls**

In `web/src/lib/vaultQuery.ts`, delete `useVaultSetCached` and its doc comment.

Run: `grep -rn 'useVaultSetCached\|setProfile' web/src`
Expected: no output apart from `setProfilePhones` and friends in `ImportScreen.tsx`, which are a different piece of state.

- [ ] **Step 6: Run the checks**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean, Biome clean, all suites pass, including `ImportScreen.test.tsx`, `SettingsScreen.test.tsx` and `useImportJob.test.tsx`, which fake `useAccountProfile` and are unaffected by a field it no longer returns.

- [ ] **Step 7: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/useAccountProfile.ts web/src/lib/useAccountProfile.test.tsx web/src/screens/settings/ProfileSettingsPanel.tsx web/src/screens/ImportScreen.tsx web/src/lib/vaultQuery.ts
git commit -m "refactor(web): write the account profile through a mutation

Two screens wrote the profile and then handed the answer back through a
setProfile function the read hook exposed, each keeping its own busy
flag around the call. The mutation stores the vault's answer itself, so
both screens lost the flag and the hand-back, and the last caller of
useVaultSetCached is gone.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 9: API token writes as mutations

**Files:**
- Modify: `web/src/screens/settings/useApiTokens.ts`
- Create: `web/src/screens/settings/useApiTokens.test.tsx`

**Interfaces:**
- Produces `useCreateApiToken()`, `useRenameApiToken()`, `useRevokeApiToken()`, each marking `keys.apiTokens.all` stale when it settles.
- `useApiTokens()` returns the same fields it does today. `busy` is now the union of the three `isPending` flags, `actionError` the message of whichever failed, and `clearError` resets all three. `refetch`/`reload` is gone.
- `ApiTokensSection.tsx` is untouched.

- [ ] **Step 1: Write the failing test**

Create `web/src/screens/settings/useApiTokens.test.tsx`:

```tsx
/** @vitest-environment jsdom */

/**
 * What a token write leaves behind.
 *
 * The hook used to call `refetch` on its own query after each write, which
 * refreshed the list this hook holds and nothing else. It marks the list stale
 * instead, so anything showing tokens refreshes.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createApiToken, deleteApiToken, listApiTokens, renameApiToken } from "../../lib/vaultApi";
import type { ApiTokenItem } from "./apiTokensUtils";
import { useApiTokens } from "./useApiTokens";

vi.mock("../../lib/auth", () => ({ useAuth: () => ({ accountId: "account-1" }) }));

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  listApiTokens: vi.fn(),
  createApiToken: vi.fn(),
  renameApiToken: vi.fn(),
  deleteApiToken: vi.fn(),
}));

const list = vi.mocked(listApiTokens);
const create = vi.mocked(createApiToken);
const rename = vi.mocked(renameApiToken);
const revoke = vi.mocked(deleteApiToken);

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

const token: ApiTokenItem = {
  id: "tok_1",
  label: "Laptop",
  can_import: true,
  can_export: true,
  can_delete: false,
  created_at: "1700000000",
  token_hint: "mv-api-la..op",
};

beforeEach(() => {
  vi.clearAllMocks();
  list.mockResolvedValue({ items: [token] } as unknown as Awaited<ReturnType<typeof listApiTokens>>);
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useApiTokens", () => {
  it("asks for the list again after a token is created", async () => {
    create.mockResolvedValue({ ...token, token: "mv-api-secret" } as unknown as Awaited<
      ReturnType<typeof createApiToken>
    >);
    const { result } = renderHook(() => useApiTokens(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(list).toHaveBeenCalledTimes(1);

    act(() => {
      result.current.setLabel("Laptop");
    });
    act(() => {
      result.current.create();
    });

    await waitFor(() => expect(create).toHaveBeenCalledWith({
      label: "Laptop",
      can_import: true,
      can_export: true,
      can_delete: false,
    }));
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.reveal?.token).toBe("mv-api-secret"));
  });

  it("asks for the list again after a token is revoked, and closes the dialog either way", async () => {
    revoke.mockRejectedValue(new Error("already revoked"));
    const { result } = renderHook(() => useApiTokens(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => {
      result.current.revoke(token);
    });

    await waitFor(() => expect(result.current.actionError).toBe("already revoked"));
    expect(result.current.revokeTarget).toBeNull();
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));

    act(() => {
      result.current.cancelCompose();
    });
    await waitFor(() => expect(result.current.actionError).toBe(""));
  });

  it("reports a write in flight while a rename is unanswered", async () => {
    let finish: () => void = () => {};
    rename.mockReturnValue(
      new Promise((resolve) => {
        finish = () => resolve({} as never);
      }),
    );
    const { result } = renderHook(() => useApiTokens(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => {
      result.current.openRename(token);
    });
    act(() => {
      result.current.setRenameLabel("Desktop");
    });
    act(() => {
      result.current.rename();
    });

    await waitFor(() => expect(result.current.busy).toBe(true));
    act(() => {
      finish();
    });
    await waitFor(() => expect(result.current.busy).toBe(false));
    expect(rename).toHaveBeenCalledWith("tok_1", { label: "Desktop" });
    expect(result.current.renameTarget).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/screens/settings/useApiTokens.test.tsx`
Expected: the revoke case fails at `expect(list).toHaveBeenCalledTimes(2)`. Today `reload()` sits after the awaited call inside the `try`, so a refused revoke never refreshes the list — the vault is asked once, not twice. That is the case this task exists for: `onSettled` runs whether the vault agreed or not. The create and rename cases pass against the current code; they are here to pin behaviour the rewrite must keep, and they must still pass afterwards.

- [ ] **Step 3: Turn the three writes into mutations**

In `web/src/screens/settings/useApiTokens.ts`, replace the `useAsyncAction` import with

```ts
import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { useVaultCache, useVaultQuery } from "../../lib/vaultQuery";
```

and add above `useApiTokens`:

```ts
type NewToken = Parameters<typeof createApiToken>[0];
type CreatedToken = Awaited<ReturnType<typeof createApiToken>>;

/** Every token write marks the list stale, and the list refetches itself. */
function useApiTokenWrite<T, V>(write: (vars: V) => Promise<T>): UseMutationResult<T, Error, V> {
  const cache = useVaultCache();
  return useMutation<T, Error, V>({
    mutationFn: write,
    onSettled: () => cache.invalidate(keys.apiTokens.all),
  });
}

export function useCreateApiToken(): UseMutationResult<CreatedToken, Error, NewToken> {
  return useApiTokenWrite((body: NewToken) => createApiToken(body));
}

export function useRenameApiToken(): UseMutationResult<
  Awaited<ReturnType<typeof renameApiToken>>,
  Error,
  { id: string; label: string }
> {
  return useApiTokenWrite(({ id, label }: { id: string; label: string }) =>
    renameApiToken(id, { label }),
  );
}

export function useRevokeApiToken(): UseMutationResult<
  Awaited<ReturnType<typeof deleteApiToken>>,
  Error,
  string
> {
  return useApiTokenWrite((id: string) => deleteApiToken(id));
}
```

Inside `useApiTokens`, replace the query destructuring and the `useAsyncAction` line with:

```ts
  const { data, isPending: loading, error: loadError } = useVaultQuery(keys.apiTokens.all, fetchTokens);
  const createToken = useCreateApiToken();
  const renameToken = useRenameApiToken();
  const revokeToken = useRevokeApiToken();

  const busy = createToken.isPending || renameToken.isPending || revokeToken.isPending;
  const failure = createToken.error ?? renameToken.error ?? revokeToken.error;
  const actionError = failure ? failure.message : "";

  const resetCreate = createToken.reset;
  const resetRename = renameToken.reset;
  const resetRevoke = revokeToken.reset;
  const clearError = useCallback(() => {
    resetCreate();
    resetRename();
    resetRevoke();
  }, [resetCreate, resetRename, resetRevoke]);
```

and replace the three actions with:

```ts
  const create = () => {
    const trimmed = label.trim();
    if (!trimmed) return;
    createToken.mutate(
      {
        label: trimmed,
        can_import: canImport,
        can_export: canExport,
        can_delete: canDelete,
      },
      {
        onSuccess: (res) => {
          setLabel("");
          setCanImport(true);
          setCanExport(true);
          setCanDelete(false);
          setComposing(false);
          setReveal({ label: res.label, token: res.token });
        },
      },
    );
  };

  const rename = () => {
    if (!renameTarget) return;
    const trimmed = renameLabel.trim();
    if (!trimmed) return;
    renameToken.mutate(
      { id: renameTarget.id, label: trimmed },
      {
        onSuccess: () => {
          setRenameTarget(null);
          setRenameLabel("");
        },
      },
    );
  };

  /** The dialog closes whether or not the vault agreed; the refusal shows in `actionError`. */
  const revoke = (item: ApiTokenItem) => {
    revokeToken.mutate(item.id, { onSettled: () => setRevokeTarget(null) });
  };
```

`mutate` never rejects, so the `void create()`, `void rename()` and `void revoke(revokeTarget)` call sites in `ApiTokensSection.tsx` keep working unchanged. Add `import { keys } from "../../lib/vaultKeys";` if Task 1 has not already put it there.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx tsc --noEmit -p . && npx vitest run src/screens/settings/useApiTokens.test.tsx src/screens/settings/ApiTokensSection.test.tsx`
Expected: both suites pass. `ApiTokensSection.test.tsx` should need no change: marking the list stale refetches it exactly as `reload()` did.

- [ ] **Step 5: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/screens/settings/useApiTokens.ts web/src/screens/settings/useApiTokens.test.tsx
git commit -m "refactor(web): make API key writes mutations

Each write called refetch on the one query this hook holds, which is a
narrower promise than it looks: only this hook's list refreshed. The
three writes are mutations now, each marking the API key list stale, and
the busy flag and error string come off the mutations instead of a
separate piece of state.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 10: Administration writes as mutations

**Files:**
- Modify: `web/src/screens/settings/useAdminUsers.ts`
- Create: `web/src/screens/settings/useAdminUsers.test.tsx`

**Interfaces:**
- Produces `useCreateUser()`, `useUpdateUser()`, `useSetUserPassword()`, `useDeleteUser()`, `useDeleteUserMessages()`. All but `useSetUserPassword` mark `keys.adminUsers.all` stale; a password change shows on no list.
- `useAdminUsers()` returns the same fields. `deleteMessages` and `deleteUser` keep answering `Promise<boolean>`, which is what keeps the confirmation dialog open on a refusal.
- `AdminUsersPanel.tsx` is untouched.

- [ ] **Step 1: Write the failing test**

Create `web/src/screens/settings/useAdminUsers.test.tsx`:

```tsx
/** @vitest-environment jsdom */

/**
 * What an administration write leaves behind, and what it answers.
 *
 * The panel keeps its confirmation dialog open when a delete is refused, so
 * `deleteUser` has to answer whether the vault agreed — a mutation that
 * swallows its error would close the dialog on a failure.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { deleteUser, listUsers, updateUser } from "../../lib/vaultApi";
import { useAdminUsers } from "./useAdminUsers";

vi.mock("../../lib/auth", () => ({ useAuth: () => ({ accountId: "account-1" }) }));

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  listUsers: vi.fn(),
  createUser: vi.fn(),
  updateUser: vi.fn(),
  deleteUser: vi.fn(),
  deleteUserMessages: vi.fn(),
  setUserPassword: vi.fn(),
}));

const list = vi.mocked(listUsers);
const patchUser = vi.mocked(updateUser);
const removeUser = vi.mocked(deleteUser);

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

const alice = {
  account_id: "a1",
  username: "alice",
  is_admin: false,
  disabled: false,
  can_import: true,
  can_export: true,
  can_delete: false,
  message_count: 12,
  storage_bytes: 2048,
};

beforeEach(() => {
  vi.clearAllMocks();
  list.mockResolvedValue({ items: [alice] } as unknown as Awaited<ReturnType<typeof listUsers>>);
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useAdminUsers", () => {
  it("asks for the list again after one account is changed", async () => {
    patchUser.mockResolvedValue(undefined);
    const { result } = renderHook(() => useAdminUsers(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(list).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.patch("a1", { disabled: true });
    });

    expect(patchUser).toHaveBeenCalledWith("a1", { disabled: true });
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });

  it("answers false and reports why when a delete is refused", async () => {
    removeUser.mockRejectedValue(new Error("the last administrator cannot be deleted"));
    const { result } = renderHook(() => useAdminUsers(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let answered = true;
    await act(async () => {
      answered = await result.current.deleteUser("a1");
    });

    expect(answered).toBe(false);
    await waitFor(() =>
      expect(result.current.actionError).toBe("the last administrator cannot be deleted"),
    );
    // A refused delete still means the list in hand may be out of date.
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });

  it("answers true when the vault agrees", async () => {
    removeUser.mockResolvedValue(undefined);
    const { result } = renderHook(() => useAdminUsers(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let answered = false;
    await act(async () => {
      answered = await result.current.deleteUser("a1");
    });

    expect(answered).toBe(true);
    expect(result.current.actionError).toBe("");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/screens/settings/useAdminUsers.test.tsx`
Expected: the refused-delete case fails at `expect(list).toHaveBeenCalledTimes(2)`. Today `reload()` runs only after a write the vault agreed to, so a refusal leaves the list unrefreshed. The other two cases pass against the current code and pin what the rewrite must keep: `patch` refreshes the list, and `deleteUser` answers whether the vault agreed.

- [ ] **Step 3: Turn the five writes into mutations**

In `web/src/screens/settings/useAdminUsers.ts`, replace the `useAsyncAction` import with

```ts
import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { useVaultCache, useVaultQuery } from "../../lib/vaultQuery";
```

keeping the `keys` import Task 1 added, and add above `useAdminUsers`:

```ts
/** Every write but a password change shows on the account list. */
function useAdminWrite<V>(
  write: (vars: V) => Promise<unknown>,
  refreshesTheList = true,
): UseMutationResult<unknown, Error, V> {
  const cache = useVaultCache();
  return useMutation<unknown, Error, V>({
    mutationFn: write,
    onSettled: refreshesTheList ? () => cache.invalidate(keys.adminUsers.all) : undefined,
  });
}

export function useCreateUser(): UseMutationResult<
  unknown,
  Error,
  { username: string; password: string; is_admin: boolean }
> {
  return useAdminWrite((body) => createVaultUser(body));
}

export function useUpdateUser(): UseMutationResult<
  unknown,
  Error,
  { id: string; changes: AdminUserChanges }
> {
  return useAdminWrite(({ id, changes }) => updateUser(id, changes));
}

export function useDeleteUser(): UseMutationResult<unknown, Error, string> {
  return useAdminWrite((id: string) => deleteVaultUser(id));
}

export function useDeleteUserMessages(): UseMutationResult<unknown, Error, string> {
  return useAdminWrite((id: string) => deleteUserMessages(id));
}

export function useSetUserPassword(): UseMutationResult<
  unknown,
  Error,
  { id: string; password: string }
> {
  return useAdminWrite(({ id, password }) => setVaultUserPassword(id, { password }), false);
}
```

with, beside the `AdminUser` type:

```ts
/** The flags an administrator can change on one account. */
export type AdminUserChanges = Partial<
  Pick<AdminUser, "is_admin" | "disabled" | "can_import" | "can_export" | "can_delete">
>;
```

Inside `useAdminUsers`, replace the query destructuring and `useAsyncAction` with:

```ts
  const {
    data,
    isPending: loading,
    error: loadError,
  } = useVaultQuery(keys.adminUsers.all, fetchUsers);
  const createAccount = useCreateUser();
  const changeAccount = useUpdateUser();
  const removeAccount = useDeleteUser();
  const removeMessages = useDeleteUserMessages();
  const changePassword = useSetUserPassword();

  const busy =
    createAccount.isPending ||
    changeAccount.isPending ||
    removeAccount.isPending ||
    removeMessages.isPending ||
    changePassword.isPending;
  const failure =
    createAccount.error ??
    changeAccount.error ??
    removeAccount.error ??
    removeMessages.error ??
    changePassword.error;
  const actionError = failure ? failure.message : "";

  const resets = [
    createAccount.reset,
    changeAccount.reset,
    removeAccount.reset,
    removeMessages.reset,
    changePassword.reset,
  ];
  const clearError = useCallback(() => {
    for (const reset of resets) reset();
    // biome-ignore lint/correctness/useExhaustiveDependencies: each `reset` is stable for the life of its mutation.
  }, resets);
```

If Biome objects to the spread dependency list, write the five resets out as named consts and list them, the way Task 9 does — that is the preferred form; use the ignore comment only if the named form still trips the rule.

Replace the five actions with:

```ts
  const createUser = useCallback(() => {
    const username = newUsername.trim();
    const password = newPassword;
    if (!username || !password) return;
    createAccount.mutate(
      { username, password, is_admin: newIsAdmin },
      {
        onSuccess: () => {
          setNewUsername("");
          setNewPassword("");
          setNewIsAdmin(false);
          setComposing(false);
        },
      },
    );
  }, [newUsername, newPassword, newIsAdmin, createAccount.mutate]);

  const setUserPassword = useCallback(() => {
    if (!passwordTarget || !resetPassword) return;
    changePassword.mutate(
      { id: passwordTarget.account_id, password: resetPassword },
      {
        onSuccess: () => {
          setPasswordTarget(null);
          setResetPassword("");
        },
      },
    );
  }, [passwordTarget, resetPassword, changePassword.mutate]);

  const patch = useCallback(
    (id: string, changes: AdminUserChanges) =>
      changeAccount.mutateAsync({ id, changes }).then(
        () => undefined,
        () => undefined,
      ),
    [changeAccount.mutateAsync],
  );

  // These two answer whether the vault agreed, so the confirmation dialog can
  // stay open and show the refusal instead of closing as though it had worked.
  const deleteMessages = useCallback(
    (id: string) => removeMessages.mutateAsync(id).then(() => true, () => false),
    [removeMessages.mutateAsync],
  );

  const deleteUser = useCallback(
    (id: string) => removeAccount.mutateAsync(id).then(() => true, () => false),
    [removeAccount.mutateAsync],
  );
```

`openPasswordReset` and `cancelCompose` keep calling `clearError()`; `closePasswordReset` and `closeRename` keep checking `busy`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx tsc --noEmit -p . && npx vitest run src/screens/settings/useAdminUsers.test.tsx src/screens/settings/AdminUsersPanel.test.tsx`
Expected: both suites pass. `AdminUsersPanel.test.tsx` stubs `fetch` and exercises the real route functions; it should need no change.

- [ ] **Step 5: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/screens/settings/useAdminUsers.ts web/src/screens/settings/useAdminUsers.test.tsx
git commit -m "refactor(web): make administration writes mutations

The five writes shared one busy flag and each called refetch on this
hook's own query afterwards. They are mutations now: four mark the
account list stale, and a password change marks nothing, because it
shows on no list. Deleting an account still answers whether the vault
agreed, so the confirmation dialog stays open on a refusal.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 11: Prove nothing is left behind

**Files:**
- Modify: `web/package.json`
- Modify: whatever the greps below turn up, if anything

**Interfaces:**
- Adds no interface. This task is the proof that the ones above replaced what they were meant to.

- [ ] **Step 1: No key is built outside the factory**

Run: `grep -rn '\["contact-groups"\|\["message-tags"\|\["saved-searches"\|\["account-profile"\|\["contact-detail"\|\["contacts",\|\["conversations",\|\["conversation-sources"\|\["storage-overview"\|\["import-detail"\|\["admin-users"\|\["api-tokens"\|\["trash-count"' web/src --include=*.ts --include=*.tsx | grep -v 'vaultKeys\.\(ts\|test\.ts\)' | grep -v '\["vault", "account'`
Expected: no output. Keys written out in full inside a test — `["vault", "account-1", "contacts"]` — are what a test is for and are excluded above.

- [ ] **Step 2: No screen keeps vault data of its own**

Run: `grep -rn 'groupOverrides\|tagOverrides\|membershipRev\|setGroups\|useContactDetailCache' web/src`
Expected: no output.

- [ ] **Step 3: No write refetches by hand**

Run: `grep -rn 'refetch: reload\|reload()' web/src --include=*.ts --include=*.tsx`
Expected: no output. `useVaultHealth` has a `reload` of its own that is not a vault query — if it appears, confirm it comes from `web/src/lib/useVaultHealth.ts` and leave it alone.

Run: `grep -rn 'useVaultInvalidate\|useVaultSetCached\|useVaultCached\|useVaultFetchFresh' web/src`
Expected: no output.

- [ ] **Step 4: Every write is a mutation**

Run: `grep -rln 'useMutation' web/src --include=*.ts --include=*.tsx | sort`
Expected: exactly these six modules, each one the owner of a resource — `web/src/lib/contactDetail.ts`, `web/src/lib/nameCollection.ts`, `web/src/lib/savedSearches.ts`, `web/src/lib/useAccountProfile.ts`, `web/src/screens/settings/useAdminUsers.ts`, `web/src/screens/settings/useApiTokens.ts`. A seventh entry means a screen has been given a mutation of its own; move it into the module that owns the resource.

Run: `grep -rn 'useAsyncAction' web/src --include=*.ts --include=*.tsx | grep -v useAsyncAction.ts`
Expected: only `screens/auth/LoginForm.tsx`, `screens/auth/CreateAccountForm.tsx`, `screens/OnboardingScreen.tsx` and `lib/useAsyncAction.test.tsx`. Those three forms write no vault data that a list shows and keep the hook on purpose.

- [ ] **Step 5: Put the query library where it belongs**

In `web/package.json`, move `"@tanstack/react-query": "^5.102.8"` from `devDependencies` to `dependencies`, keeping both lists alphabetical. It is what the app fetches and writes through at run time, and it has been in the development list since #291.

Run: `cd web && npm install --package-lock-only && git diff --stat package.json package-lock.json`
Expected: `package.json` changes; `package-lock.json` either does not change or records only the moved entry. Nothing is downloaded that was not installed already.

- [ ] **Step 6: Run everything**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: all clean.

- [ ] **Step 7: Commit**

```bash
git add web/package.json web/package-lock.json
git commit -m "chore(web): list the query library as a dependency

TanStack Query is what the app fetches and writes through, so it belongs
in dependencies rather than devDependencies. Vite bundles it either way,
which is why nothing was broken; the list was simply wrong.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

---

### Task 12: The whole gate, the documents, and the pull request

**Files:**
- Create: `docs/superpowers/specs/2026-09-02-web-writes-through-tanstack-query-design.md`, `docs/superpowers/plans/2026-09-02-web-writes-through-tanstack-query.md`
- Modify: the spec only if the implementation had to differ from it — record the difference under "What else changes".

- [ ] **Step 1: Put the design and the plan in the repository**

Copy both documents from the scratch directory they were written in:

```bash
cp /tmp/claude-1000/-home-mbeisser-repo-message-vault/cecaa52d-f949-4734-a0c4-fc9a012b72b9/scratchpad/2026-09-02-web-writes-through-tanstack-query-design.md docs/superpowers/specs/
cp /tmp/claude-1000/-home-mbeisser-repo-message-vault/cecaa52d-f949-4734-a0c4-fc9a012b72b9/scratchpad/2026-09-02-web-writes-through-tanstack-query.md docs/superpowers/plans/
git add docs/superpowers/specs/2026-09-02-web-writes-through-tanstack-query-design.md docs/superpowers/plans/2026-09-02-web-writes-through-tanstack-query.md
git commit -m "docs: record the design and plan for web writes

The design says which prefix each write makes stale and which writes
draw their change before the vault answers, and the plan is the order
the work was done in.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9"
```

- [ ] **Step 2: Run the full gate**

Run: `./scripts/check-pr.sh`
Expected: rustfmt, workspace build and test, `src-tauri` build, Biome `ci`, Vitest, and the generated-types drift check all pass. Nothing in this change touches Rust or the OpenAPI document, so any Rust failure is pre-existing — say so rather than fixing it here.

- [ ] **Step 3: Walk the app once more**

With the vault and the web app running (`./scripts/run-vault-dev.sh`, never `--reset` or `--reset-demo`, and `cd web && npm run dev`), sign in and confirm in one pass:

1. Create a Contact Group, tick it on two contacts, rename it in the sidebar: both rows and the open drawer show the new name without a reload.
2. Delete it: the chips go.
3. The same for a Message Tag on the conversation list, including the Tags menu showing the renamed tag ticked.
4. Settings → Profile: change the display name; the header and the sidebar show it at once.
5. Settings → API keys: create one, rename it, revoke it; the list is right after each.
6. Settings → Users, as an administrator: change a permission and confirm the row updates.

- [ ] **Step 4: Open the pull request**

```bash
git push -u origin feat/web-writes-through-tanstack-query
gh pr create --title "feat(web): write through TanStack Query, with one key factory and a mutation per write" --body "$(cat <<'EOF'
Closes #299.

Reads moved onto TanStack Query in #290–#293; writes did not. Every write is now a `useMutation` in the module that owns the resource, and every cache key comes from one factory, so a write can say what it makes stale instead of each screen deciding for itself.

- `web/src/lib/vaultKeys.ts` builds every key: one prefix per resource, with list and detail builders under it. `vaultQueryKey` still puts the signed-in account in front.
- `useVaultCache()` replaces `useVaultInvalidate`, `useVaultSetCached`, `useVaultCached` and `useVaultFetchFresh` with the account-scoped operations a write needs.
- Contact Group and Message Tag membership is drawn before the vault answers, from a description on the collection, and rolled back from a snapshot if the vault refuses.
- Deleted: `groupOverrides`, `tagOverrides`, `membershipRev`, `useContactDetailCache`, the `onHandlesChanged` callback chain, and the `reload()` wrappers in the two settings hooks.

The reported symptom — rename a Message Tag with a conversation selected and the Tags menu still shows the old name ticked — is fixed because the menu now reads the rows the rename marked stale, and there is no override map left holding the old spelling.

Spec: docs/superpowers/specs/2026-09-02-web-writes-through-tanstack-query-design.md
Decision: docs/adr/0002-one-way-to-fetch-data-in-the-web-app.md

No URL, route function, or server code changes.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_013dhrPR3HiEgeJJLAGVbrG9
EOF
)"
```

Then `gh pr checks --watch` until green. Do not merge.
