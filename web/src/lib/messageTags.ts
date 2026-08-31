import { groupFromSlug, groupSlug } from "./contactGroups";
import { createNameCollection } from "./nameCollection";

/** Names that must not be created as message tags. */
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

export const MESSAGE_TAGS_CHANGED_EVENT = "mv-message-tags-changed";

export const messageTags = createNameCollection({
  endpoint: "/v1/message-tags",
  membershipEndpoint: "/v1/conversations/tags",
  responseKey: "tags",
  queryToken: "tag",
  changedEvent: MESSAGE_TAGS_CHANGED_EVENT,
  reservedNames: RESERVED_TAG_NAMES,
  reservedError: reservedTagError,
});

export function isReservedTagName(name: string): boolean {
  return messageTags.isReserved(name);
}

// Slugs are URL syntax, not vocabulary — tags and groups share one rule.
export const tagSlug = groupSlug;
export const tagFromSlug = groupFromSlug;

/** Build the thread-list query for a tag page plus optional typed search. */
export const tagListQuery = messageTags.listQuery;

export const fetchMessageTags = messageTags.fetchAll;
export const invalidateMessageTags = messageTags.invalidate;
export const createMessageTag = messageTags.create;
export const renameMessageTag = messageTags.rename;
export const deleteMessageTag = messageTags.remove;
export const setConversationTagMembership = messageTags.setMembership;
