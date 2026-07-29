import type { CollapsedGroupConversation } from "./groupChatList";
import type { ContactListItem, YearThread } from "./types";
import type { SearchConversationHit } from "./search";

/** Discriminated focus for the merged browse tree. */
export type BrowseFocus =
  | { type: "none" }
  | { type: "contact"; id: number }
  | {
      type: "conversation";
      conversationId: number;
      threadKey: string;
      contactId?: number;
    }
  | { type: "searchHit"; hit: SearchConversationHit };

/** Stable string key for a visible tree row (keyboard / selection). */
export type BrowseTreeRowKey = string;

export type BrowseTreeRow =
  | { kind: "contact"; key: BrowseTreeRowKey; contact: ContactListItem }
  | {
      kind: "direct";
      key: BrowseTreeRowKey;
      contactId: number;
      dateStart: string | null;
      dateEnd: string | null;
    }
  | {
      kind: "group";
      key: BrowseTreeRowKey;
      contactId: number | null;
      conversation: CollapsedGroupConversation;
    }
  | {
      kind: "search";
      key: BrowseTreeRowKey;
      hit: SearchConversationHit;
    };

export type BrowseTreeMode = "browse" | "shared-groups" | "search";

export function contactRowKey(id: number): BrowseTreeRowKey {
  return `contact:${id}`;
}

export function directRowKey(contactId: number): BrowseTreeRowKey {
  return `direct:${contactId}`;
}

export function groupRowKey(conversationId: number): BrowseTreeRowKey {
  return `group:${conversationId}`;
}

export function searchRowKey(conversationId: number): BrowseTreeRowKey {
  return `search:${conversationId}`;
}

export function parseBrowseTreeRowKey(
  key: BrowseTreeRowKey,
):
  | { kind: "contact"; id: number }
  | { kind: "direct"; contactId: number }
  | { kind: "group"; conversationId: number }
  | { kind: "search"; conversationId: number }
  | null {
  const [kind, raw] = key.split(":");
  const id = Number(raw);
  if (!Number.isFinite(id)) return null;
  if (kind === "contact") return { kind, id };
  if (kind === "direct") return { kind, contactId: id };
  if (kind === "group") return { kind, conversationId: id };
  if (kind === "search") return { kind, conversationId: id };
  return null;
}

/** Same-kind keys only — used to restrict Shift-range selection. */
export function sameKindKeys(
  keys: BrowseTreeRowKey[],
  anchor: BrowseTreeRowKey,
): BrowseTreeRowKey[] {
  const parsed = parseBrowseTreeRowKey(anchor);
  if (!parsed) return [];
  return keys.filter((k) => parseBrowseTreeRowKey(k)?.kind === parsed.kind);
}

export function directDateRange(yearly: YearThread[]): {
  dateStart: string | null;
  dateEnd: string | null;
} {
  if (yearly.length === 0) return { dateStart: null, dateEnd: null };
  let dateStart = yearly[0]!.dateStart;
  let dateEnd = yearly[0]!.dateEnd;
  for (const y of yearly) {
    if (y.dateStart < dateStart) dateStart = y.dateStart;
    if (y.dateEnd > dateEnd) dateEnd = y.dateEnd;
  }
  return { dateStart, dateEnd };
}

/**
 * Flatten the visible browse tree for keyboard order.
 * Mode precedence: search > shared-groups > browse (expanded contact).
 */
export function flattenBrowseTree(options: {
  mode: BrowseTreeMode;
  contacts: ContactListItem[];
  expandedContactId: number | null;
  yearly: YearThread[];
  groups: CollapsedGroupConversation[];
  sharedGroups: CollapsedGroupConversation[];
  searchHits: SearchConversationHit[];
}): BrowseTreeRow[] {
  const {
    mode,
    contacts,
    expandedContactId,
    yearly,
    groups,
    sharedGroups,
    searchHits,
  } = options;

  if (mode === "search") {
    return searchHits.map((hit) => ({
      kind: "search" as const,
      key: searchRowKey(hit.conversationId),
      hit,
    }));
  }

  if (mode === "shared-groups") {
    return sharedGroups.map((conversation) => ({
      kind: "group" as const,
      key: groupRowKey(conversation.conversationId),
      contactId: null,
      conversation,
    }));
  }

  const rows: BrowseTreeRow[] = [];
  for (const contact of contacts) {
    rows.push({
      kind: "contact",
      key: contactRowKey(contact.id),
      contact,
    });
    if (expandedContactId !== contact.id) continue;

    const { dateStart, dateEnd } = directDateRange(yearly);
    const hasDirect = yearly.some((y) => y.conversationIds.length > 0);
    if (hasDirect) {
      rows.push({
        kind: "direct",
        key: directRowKey(contact.id),
        contactId: contact.id,
        dateStart,
        dateEnd,
      });
    }
    for (const conversation of groups) {
      rows.push({
        kind: "group",
        key: groupRowKey(conversation.conversationId),
        contactId: contact.id,
        conversation,
      });
    }
  }
  return rows;
}

export function browseTreeMode(options: {
  resultsMode: boolean;
  hasContactSelection: boolean;
}): BrowseTreeMode {
  if (options.resultsMode) return "search";
  if (options.hasContactSelection) return "shared-groups";
  return "browse";
}
