import type { Conversation } from "./types";
import { listConversations } from "./vaultApi";

const PAGE_SIZE = 100;

/** Load one conversation summary by id, scanning list pages until it is found. */
export async function fetchConversationById(
  conversationId: number,
  signal?: AbortSignal,
): Promise<Conversation | null> {
  let offset = 0;
  while (true) {
    const page = await listConversations({ q: "", limit: PAGE_SIZE, offset }, { signal });
    const match = page.items.find((c) => c.id === conversationId);
    if (match) return match;
    offset += PAGE_SIZE;
    if (offset >= page.total || page.items.length === 0) return null;
  }
}
