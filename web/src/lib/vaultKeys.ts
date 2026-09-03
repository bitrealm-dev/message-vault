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
    sources: (id: number | null) => ["conversations", "sources", String(id)] as const,
  },
  contactGroups: { all: ["contact-groups"] as const },
  messageTags: { all: ["message-tags"] as const },
  savedSearches: { all: ["saved-searches"] as const },
  searchFields: {
    all: ["search-fields"] as const,
    list: (list: string) => ["search-fields", list] as const,
  },
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
