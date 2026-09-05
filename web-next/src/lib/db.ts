/**
 * The read surface pages and route handlers import. Every function here
 * reads the vault through its `/v1` HTTP API (`./vault/*`). The SQL
 * implementations in `contactsRead.ts`, `groupChatsRead.ts`,
 * `messagesRead.ts`, `unassignedRead.ts` and `homeStats.ts` are no longer
 * called from anywhere.
 */
export { labelSlug } from "./labelSlug";
export {
  listLabels,
  listLabelMemberContactIds,
  labelFromSlug,
} from "./vault/labels";
export {
  listContacts,
  listContactsByIds,
  getContact,
  contactThreadsBundle,
  loadContactThreadsPage,
  listTrashedContacts,
  listTrashedContactMessages,
} from "./vault/contacts";
export {
  groupChatsContainingContacts,
  listGroupYearRows,
} from "./vault/conversations";
export {
  messagesForConversationYear,
  messagesForConversations,
  messagesPageForConversations,
  DEFAULT_MESSAGE_PAGE_SIZE,
  MAX_MESSAGE_PAGE_SIZE,
} from "./vault/messages";
export { mergeMessagePages, messagesCoverIds } from "./messageCursor";
export { homeStats } from "./vault/home";

import { listGroupYearRows as groupYearRows } from "./vault/conversations";
import type { GroupYearRow, UnassignedHandle } from "./types";

/** Trashed group chats split by calendar year. */
export function listTrashedGroupYearRows(): Promise<GroupYearRow[]> {
  return groupYearRows("trash");
}

/**
 * "Trashed handles" no longer exist: the vault trashes contacts and
 * conversations, and `/unassigned` already redirects to `/all`.
 */
export async function listTrashedHandles(): Promise<UnassignedHandle[]> {
  return [];
}
