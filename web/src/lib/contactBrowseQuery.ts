import { type ConversationKind, forContact, forHandle, withKind } from "./searchQuery";

/** Search query used when browsing a contact's conversations from the drawer. */
export function contactBrowseQuery(
  contactId: string,
  kind: ConversationKind,
  handle?: string,
): string {
  const h = handle?.trim();
  return withKind(h ? forHandle(h) : forContact(contactId), kind);
}
