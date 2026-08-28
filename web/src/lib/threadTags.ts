import { groupFromSlug, groupSlug } from "./contactGroups";
import { createNameCollection } from "./nameCollection";

/** Names that must not be created as thread tags. */
export const RESERVED_TAG_NAMES = new Set(
  [
    "home",
    "contacts",
    "threads",
    "thread",
    "all",
    "excluded",
    "unassigned",
    "trash",
    "tags",
    "tag",
    "no-tag",
    "no tag",
    "groups",
    "group",
    "labels",
    "label",
  ].map((s) => s.toLowerCase()),
);

export function reservedTagError(name: string): string {
  return `"${name.trim()}" is a reserved tag`;
}

export const THREAD_TAGS_CHANGED_EVENT = "mv-thread-tags-changed";

export const threadTags = createNameCollection({
  endpoint: "/v1/thread-tags",
  membershipEndpoint: "/v1/conversations/tags",
  responseKey: "tags",
  queryToken: "tag",
  changedEvent: THREAD_TAGS_CHANGED_EVENT,
  reservedNames: RESERVED_TAG_NAMES,
  reservedError: reservedTagError,
});

export function isReservedTagName(name: string): boolean {
  return threadTags.isReserved(name);
}

// Slugs are URL syntax, not vocabulary — tags and groups share one rule.
export const tagSlug = groupSlug;
export const tagFromSlug = groupFromSlug;

/** Build the thread-list query for a tag page plus optional typed search. */
export const tagListQuery = threadTags.listQuery;

export const fetchThreadTags = threadTags.fetchAll;
export const invalidateThreadTags = threadTags.invalidate;
export const createThreadTag = threadTags.create;
export const renameThreadTag = threadTags.rename;
export const deleteThreadTag = threadTags.remove;
export const setConversationTagMembership = threadTags.setMembership;
