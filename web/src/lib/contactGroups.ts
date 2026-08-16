import { apiClient } from "./api";

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

export function isReservedGroupName(name: string): boolean {
  return RESERVED_GROUP_NAMES.has(name.trim().toLowerCase());
}

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
export const GROUP_FILTER_TOKEN_RE =
  /\b(?:group|label|within):(?:"[^"]*"|[^\s]+)/gi;

/** True when the typed filter includes `group:`, `label:`, or `within:`. */
export function hasGroupFilterToken(raw: string): boolean {
  GROUP_FILTER_TOKEN_RE.lastIndex = 0;
  return GROUP_FILTER_TOKEN_RE.test(raw);
}

/** True when this contact should appear on the given group page. */
export function contactBelongsToGroup(
  groups: readonly string[] | undefined,
  groupFilter: string | "none" | null,
): boolean {
  if (!groupFilter) return true;
  if (groupFilter === "none") return !groups || groups.length === 0;
  const needle = groupFilter.toLowerCase();
  return (groups ?? []).some((g) => g.toLowerCase() === needle);
}

/** Build the contact-list query for a group page plus optional typed search. */
export function groupListQuery(
  group: string | "none" | null,
  search: string,
): string {
  const parts: string[] = [];
  if (group === "none") {
    parts.push("group:none");
  } else if (group) {
    parts.push(/\s/.test(group) ? `group:"${group}"` : `group:${group}`);
  }
  const extra = search.trim();
  if (extra) parts.push(extra);
  return parts.join(" ");
}

export const CONTACT_GROUPS_CHANGED_EVENT = "mv-contact-groups-changed";

function notifyContactGroupsChanged(): void {
  cachedGroups = null;
  try {
    globalThis.dispatchEvent?.(new Event(CONTACT_GROUPS_CHANGED_EVENT));
  } catch {
    // Some browsers block custom events. The next fetch still works.
  }
}

type GroupsResponse = { groups: string[] };
type NameGroupsResponse = { name: string; groups: string[] };

let cachedGroups: string[] | null = null;
let inflight: Promise<string[]> | null = null;

/** Load group names for the signed-in account. Reuses an in-flight request. */
export async function fetchContactGroups(signal?: AbortSignal): Promise<string[]> {
  if (cachedGroups && !signal) return cachedGroups;
  if (inflight && !signal) return inflight;
  const req = apiClient
    .get<GroupsResponse>("/v1/contact-groups", { signal })
    .then((res) => {
      const groups = Array.isArray(res.groups) ? res.groups : [];
      cachedGroups = groups;
      return groups;
    })
    .finally(() => {
      inflight = null;
    });
  if (!signal) inflight = req;
  return req;
}

export function invalidateContactGroups(): void {
  notifyContactGroupsChanged();
}

export async function createContactGroup(name: string): Promise<string> {
  const trimmed = name.trim();
  if (!trimmed) throw new Error("name required");
  if (isReservedGroupName(trimmed)) throw new Error(reservedGroupError(trimmed));
  const res = await apiClient.post<NameGroupsResponse>("/v1/contact-groups", {
    name: trimmed,
  });
  cachedGroups = res.groups;
  notifyContactGroupsChanged();
  return res.name;
}

export async function renameContactGroup(from: string, to: string): Promise<string> {
  const res = await apiClient.patch<NameGroupsResponse>("/v1/contact-groups", {
    from,
    to,
  });
  cachedGroups = res.groups;
  notifyContactGroupsChanged();
  return res.name;
}

export async function deleteContactGroup(name: string): Promise<void> {
  const res = await apiClient.delete<GroupsResponse>("/v1/contact-groups", { name });
  cachedGroups = res.groups;
  notifyContactGroupsChanged();
}

export async function setContactGroupMembership(
  ids: number[],
  name: string,
  enable: boolean,
): Promise<number> {
  const res = await apiClient.post<{ changed: number }>("/v1/contacts/groups", {
    ids,
    name,
    enable,
  });
  notifyContactGroupsChanged();
  return res.changed;
}
