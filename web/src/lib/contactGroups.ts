import { createNameCollection, useNameCollectionActions } from "./nameCollection";
import {
  createContactGroup,
  deleteContactGroup,
  listContactGroups,
  updateContactGroup,
  updateContactGroupMembers,
} from "./vaultApi";

/** Names that must not be created as user groups. */
export const RESERVED_GROUP_NAMES = new Set(
  [
    "home",
    "contacts",
    "all",
    "excluded",
    "no-messages",
    "no messages",
    "unassigned",
    "trash",
    "groups",
    "group",
    "group-chats",
    "group chats",
    "group-chats-2",
    "group chats 2",
    "group-messages",
    "group messages",
    "group-messages-2",
    "group messages 2",
    "no-label",
    "no-group",
    "no group",
    "labels",
    "label",
    "no label",
  ].map((s) => s.toLowerCase()),
);

export function reservedGroupError(name: string): string {
  const key = name.trim().toLowerCase();
  if (key === "contacts") return "Contacts is a reserved group";
  if (key === "all") return "All is a reserved group";
  if (key === "excluded") return "Excluded is a reserved group";
  if (key === "unassigned") return "Unassigned is a reserved group";
  if (key === "trash") return "Trash is a reserved group";
  if (key === "no messages" || key === "no-messages") {
    return "No messages is a reserved group";
  }
  if (
    key === "groups" ||
    key === "group" ||
    key === "group chats" ||
    key === "group-chats" ||
    key === "group chats 2" ||
    key === "group-chats-2" ||
    key === "group messages" ||
    key === "group-messages" ||
    key === "group messages 2" ||
    key === "group-messages-2"
  ) {
    return "Group Messages is a reserved name";
  }
  return `"${name.trim()}" is a reserved group`;
}

export const contactGroups = createNameCollection({
  routes: {
    list: listContactGroups,
    create: createContactGroup,
    update: updateContactGroup,
    remove: deleteContactGroup,
    updateMembers: updateContactGroupMembers,
  },
  cacheKey: "contact-groups",
  // Contact rows and the contact drawer show group names as chips.
  invalidates: [["contacts"], ["contact-detail"]],
  label: "group",
  queryToken: "group",
  reservedNames: RESERVED_GROUP_NAMES,
  reservedError: reservedGroupError,
});

export function isReservedGroupName(name: string): boolean {
  return contactGroups.isReserved(name);
}

/** URL slug for a contact group. Keeps letter case so Regroup and regroup stay distinct. */
export function groupSlug(name: string): string {
  return name
    .trim()
    .replace(/[^a-zA-Z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Find the group name that matches this URL slug, or null when none match. */
export function groupFromSlug(slug: string, groups: readonly string[]): string | null {
  const trimmed = slug.trim();
  if (!trimmed) return null;
  for (const name of groups) {
    if (groupSlug(name) === trimmed) return name;
  }
  const folded = trimmed.toLowerCase();
  for (const name of groups) {
    if (groupSlug(name).toLowerCase() === folded) return name;
  }
  return null;
}

/** Search words the contact list must send to the server (cannot filter locally). */
export const GROUP_FILTER_TOKEN_RE = /\b(?:group|label|within):(?:"[^"]*"|[^\s]+)/gi;

/** True when the typed filter includes `group:`, `label:`, or `within:`. */
export function hasGroupFilterToken(raw: string): boolean {
  GROUP_FILTER_TOKEN_RE.lastIndex = 0;
  return GROUP_FILTER_TOKEN_RE.test(raw);
}

/** True when this contact should appear on the given group page. */
/**
 * The permanent Contact Group holding what the vault could not identify: a
 * contact with no identity, or with identities and no preferred name. The
 * server computes its membership, so it is never stored on a contact.
 */
export const UNKNOWN_GROUP = "unknown";

export function contactBelongsToGroup(
  groups: readonly string[] | undefined,
  groupFilter: string | "none" | null,
): boolean {
  if (!groupFilter) return true;
  // Unknown is computed by the server from contact state, so it never appears
  // in a contact's stored group names. The rows in hand have already been
  // filtered; re-checking here would discard every one of them.
  if (groupFilter === UNKNOWN_GROUP) return true;
  if (groupFilter === "none") return !groups || groups.length === 0;
  const needle = groupFilter.toLowerCase();
  return (groups ?? []).some((g) => g.toLowerCase() === needle);
}

/** Build the contact-list query for a group page plus optional typed search. */
export const groupListQuery = contactGroups.listQuery;

/** Create, rename, delete, and set membership on Contact Groups. */
export function useContactGroupActions() {
  return useNameCollectionActions(contactGroups);
}
