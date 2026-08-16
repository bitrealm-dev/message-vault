import { apiClient } from "./api";
import { groupFromSlug, groupSlug } from "./contactGroups";

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

export function isReservedTagName(name: string): boolean {
  return RESERVED_TAG_NAMES.has(name.trim().toLowerCase());
}

export function reservedTagError(name: string): string {
  return `"${name.trim()}" is a reserved tag`;
}

export const tagSlug = groupSlug;
export const tagFromSlug = groupFromSlug;

/** Build the thread-list query for a tag page plus optional typed search. */
export function tagListQuery(
  tag: string | "none" | null,
  search: string,
): string {
  const parts: string[] = [];
  if (tag === "none") {
    parts.push("tag:none");
  } else if (tag) {
    parts.push(/\s/.test(tag) ? `tag:"${tag}"` : `tag:${tag}`);
  }
  const extra = search.trim();
  if (extra) parts.push(extra);
  return parts.join(" ");
}

export const THREAD_TAGS_CHANGED_EVENT = "mv-thread-tags-changed";

function notifyThreadTagsChanged(): void {
  cachedTags = null;
  try {
    globalThis.dispatchEvent?.(new Event(THREAD_TAGS_CHANGED_EVENT));
  } catch {
    // Some browsers block custom events. The next fetch still works.
  }
}

type TagsResponse = { tags: string[] };
type NameTagsResponse = { name: string; tags: string[] };

let cachedTags: string[] | null = null;
let inflight: Promise<string[]> | null = null;

export async function fetchThreadTags(signal?: AbortSignal): Promise<string[]> {
  if (cachedTags && !signal) return cachedTags;
  if (inflight && !signal) return inflight;
  const req = apiClient
    .get<TagsResponse>("/v1/thread-tags", { signal })
    .then((res) => {
      const tags = Array.isArray(res.tags) ? res.tags : [];
      cachedTags = tags;
      return tags;
    })
    .finally(() => {
      inflight = null;
    });
  if (!signal) inflight = req;
  return req;
}

export function invalidateThreadTags(): void {
  notifyThreadTagsChanged();
}

export async function createThreadTag(name: string): Promise<string> {
  const trimmed = name.trim();
  if (!trimmed) throw new Error("name required");
  if (isReservedTagName(trimmed)) throw new Error(reservedTagError(trimmed));
  const res = await apiClient.post<NameTagsResponse>("/v1/thread-tags", {
    name: trimmed,
  });
  cachedTags = res.tags;
  notifyThreadTagsChanged();
  return res.name;
}

export async function renameThreadTag(from: string, to: string): Promise<string> {
  const res = await apiClient.patch<NameTagsResponse>("/v1/thread-tags", {
    from,
    to,
  });
  cachedTags = res.tags;
  notifyThreadTagsChanged();
  return res.name;
}

export async function deleteThreadTag(name: string): Promise<void> {
  const res = await apiClient.delete<TagsResponse>("/v1/thread-tags", { name });
  cachedTags = res.tags;
  notifyThreadTagsChanged();
}

export async function setConversationTagMembership(
  ids: number[],
  name: string,
  enable: boolean,
): Promise<number> {
  const res = await apiClient.post<{ changed: number }>("/v1/conversations/tags", {
    ids,
    name,
    enable,
  });
  notifyThreadTagsChanged();
  return res.changed;
}
