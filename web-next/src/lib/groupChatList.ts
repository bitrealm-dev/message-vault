import type { GroupChatThread, GroupParticipant, GroupYearRow } from "./types";

export const GROUP_CHAT_SORT_ALLOWED = ["date", "messages", "people"] as const;
export const SORT_ORDER_ALLOWED = ["asc", "desc"] as const;

/** Shared by BrowseShell and GroupMessagesShell. */
export const GROUP_CHAT_SORT_KEY = "mv-browse-group-chat-sort";
export const GROUP_CHAT_SORT_ORDER_KEY = "mv-browse-group-chat-sort-order";

/** One conversation after collapsing year-bucketed group rows. */
export type CollapsedGroupConversation = {
  conversationId: number;
  conversationIds: number[];
  title: string;
  titleFull: string;
  namedTitle: string | null;
  participantCount: number;
  participantNames: string[];
  participantHandles: string[];
  participants: GroupParticipant[];
  messageCount: number;
  dateStart: string;
  dateEnd: string;
  newestYear: number;
};

/** Newest calendar year for a conversation in the year-row list. */
export function newestYearForConversation(
  rows: GroupYearRow[],
  id: number,
): number | null {
  let newest: number | null = null;
  for (const g of rows) {
    if (g.id !== id) continue;
    if (newest == null || g.year > newest) newest = g.year;
  }
  return newest;
}

export function groupYearRowsToThreads(rows: GroupYearRow[]): GroupChatThread[] {
  return rows.map((r) => ({
    conversationId: r.id,
    conversationIds: [r.id],
    title: r.title,
    titleFull: r.titleFull,
    namedTitle: r.namedTitle,
    participantCount: r.participantCount,
    participantNames: r.participantNames,
    participantHandles: r.participantHandles,
    participants: r.participants,
    year: r.year,
    messageCount: r.messageCount,
    dateStart: r.dateStart,
    dateEnd: r.dateEnd,
  }));
}

/**
 * Map year rows for collapse using full conversation date ranges
 * (trash / groups sidebar), not per-year bucket dates.
 */
export function groupYearRowsToCollapseThreads(
  rows: GroupYearRow[],
): GroupChatThread[] {
  return rows.map((r) => ({
    conversationId: r.id,
    conversationIds: [r.id],
    title: r.title,
    titleFull: r.titleFull,
    namedTitle: r.namedTitle,
    participantCount: r.participantCount,
    participantNames: r.participantNames,
    participantHandles: r.participantHandles,
    participants: r.participants,
    year: r.year,
    messageCount: r.messageCount,
    dateStart: r.conversationDateStart,
    dateEnd: r.conversationDateEnd,
  }));
}

/** Collapse year-bucketed group threads into one row per conversation. */
export function collapseGroupConversations(
  rows: GroupChatThread[],
): CollapsedGroupConversation[] {
  const map = new Map<number, CollapsedGroupConversation>();
  for (const r of rows) {
    const ids =
      r.conversationIds?.length > 0 ? r.conversationIds : [r.conversationId];
    const primary = ids[0]!;
    const prev = map.get(primary);
    if (!prev) {
      map.set(primary, {
        conversationId: primary,
        conversationIds: [...ids],
        title: r.title,
        titleFull: r.titleFull,
        namedTitle: r.namedTitle,
        participantCount: r.participantCount,
        participantNames: [...(r.participantNames ?? [])],
        participantHandles: [...(r.participantHandles ?? [])],
        participants: [...(r.participants ?? [])],
        messageCount: r.messageCount,
        dateStart: r.dateStart,
        dateEnd: r.dateEnd,
        newestYear: r.year,
      });
      continue;
    }
    prev.messageCount += r.messageCount;
    if (r.year > prev.newestYear) prev.newestYear = r.year;
    if (r.dateStart < prev.dateStart) prev.dateStart = r.dateStart;
    if (r.dateEnd > prev.dateEnd) prev.dateEnd = r.dateEnd;
    if (!prev.namedTitle && r.namedTitle) prev.namedTitle = r.namedTitle;
    if (r.titleFull && r.titleFull.length > prev.titleFull.length) {
      prev.titleFull = r.titleFull;
    }
    for (const name of r.participantNames ?? []) {
      if (!prev.participantNames.includes(name)) prev.participantNames.push(name);
    }
    for (const handle of r.participantHandles ?? []) {
      if (!prev.participantHandles.includes(handle)) {
        prev.participantHandles.push(handle);
      }
    }
    for (const p of r.participants ?? []) {
      if (!prev.participants.some((x) => x.handle === p.handle)) {
        prev.participants.push(p);
      }
    }
    for (const id of ids) {
      if (!prev.conversationIds.includes(id)) prev.conversationIds.push(id);
    }
  }
  return [...map.values()];
}

/** Trash groups sidebar: collapse GroupYearRow list. */
export function collapseGroupYearRows(
  rows: GroupYearRow[],
): CollapsedGroupConversation[] {
  return collapseGroupConversations(groupYearRowsToCollapseThreads(rows));
}
