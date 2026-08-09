"use client";

import {
  collapseGroupConversations,
  type CollapsedGroupConversation,
} from "@/lib/groupChatList";
import type { GroupChatThread } from "@/lib/types";
import { useMemo } from "react";
import type { BrowseGroupChatSortBy, SortOrder } from "./SortByMenu";

export function useCollapsedGroupChatList(options: {
  groupChats: GroupChatThread[];
  filterYear: number | null;
  query: string;
  sortBy: BrowseGroupChatSortBy;
  sortOrder: SortOrder;
}): {
  collapsedGroupChats: CollapsedGroupConversation[];
  orderedGroupIds: number[];
  collapsedById: Map<number, CollapsedGroupConversation>;
} {
  const { groupChats, filterYear, query, sortBy, sortOrder } = options;

  const collapsedGroupChats = useMemo(() => {
    const filtered =
      filterYear == null
        ? groupChats
        : groupChats.filter((g) => g.year === filterYear);
    let items = collapseGroupConversations(filtered);
    const q = query.trim().toLowerCase();
    if (q) {
      const qDigits = q.replace(/\D/g, "");
      items = items.filter((g) => {
        if (g.namedTitle && g.namedTitle.toLowerCase().includes(q)) return true;
        if (g.participantNames.some((n) => n.toLowerCase().includes(q))) {
          return true;
        }
        if (g.participantHandles.some((h) => h.toLowerCase().includes(q))) {
          return true;
        }
        if (qDigits.length > 0) {
          return g.participantHandles.some((h) =>
            h.replace(/\D/g, "").includes(qDigits),
          );
        }
        return false;
      });
    }
    items.sort((a, b) => {
      let cmp = 0;
      if (sortBy === "messages") {
        cmp = a.messageCount - b.messageCount;
      } else if (sortBy === "people") {
        cmp = a.participantCount - b.participantCount;
      } else {
        cmp = a.dateEnd.localeCompare(b.dateEnd);
      }
      return sortOrder === "desc" ? -cmp : cmp;
    });
    return items;
  }, [groupChats, filterYear, query, sortBy, sortOrder]);

  const orderedGroupIds = useMemo(
    () => collapsedGroupChats.map((g) => g.conversationId),
    [collapsedGroupChats],
  );

  const collapsedById = useMemo(() => {
    const map = new Map<number, CollapsedGroupConversation>();
    for (const g of collapsedGroupChats) map.set(g.conversationId, g);
    return map;
  }, [collapsedGroupChats]);

  return { collapsedGroupChats, orderedGroupIds, collapsedById };
}
