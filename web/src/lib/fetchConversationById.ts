import { apiClient } from "./api";
import type { Conversation } from "./types";

type ConversationsPage = {
  conversations: Conversation[];
  total: number;
  limit: number;
  offset: number;
};

const PAGE_SIZE = 100;

/** Load one conversation summary by id, scanning list pages until it is found. */
export async function fetchConversationById(
  conversationId: string,
  signal?: AbortSignal,
): Promise<Conversation | null> {
  let offset = 0;

  while (true) {
    const params = new URLSearchParams({
      q: "",
      limit: String(PAGE_SIZE),
      offset: String(offset),
    });
    const page = await apiClient.get<ConversationsPage>(
      `/v1/export/conversations?${params}`,
      { signal },
    );

    const match = (page.conversations || []).find((c) => c.id === conversationId);
    if (match) return match;

    const total = page.total ?? 0;
    offset += PAGE_SIZE;
    if (offset >= total || (page.conversations || []).length === 0) {
      return null;
    }
  }
}
