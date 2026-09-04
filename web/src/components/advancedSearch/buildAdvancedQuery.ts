import {
  advancedContacts,
  advancedMessages,
  type ContactsQueryInput,
  type CountFilterInput,
  composeCountComparison,
  type DateBoundFilter,
  type MessagesQueryInput,
} from "../../lib/searchQuery";

// The form's shapes are the builders' shapes: searchQuery.ts declares them
// once and this file passes them straight through, so a widened union or a
// new optional field cannot mean two different things on the two sides.
export type {
  ActivityFilter,
  ContactsQueryInput,
  CountComparator,
  CountFilterInput,
  DateBoundFilter,
  DateBoundOp,
  MessagesQueryInput,
} from "../../lib/searchQuery";
export { composeCountComparison };

export type AdvancedSearchMode = "messages" | "contacts";

export const EMPTY_COUNT: CountFilterInput = { comparator: "any", value: "" };
export const EMPTY_DATE_BOUND: DateBoundFilter = { op: "any", start: "", end: "" };

export function dateBoundHasValue(bound: DateBoundFilter): boolean {
  if (bound.op === "any") return false;
  return Boolean(bound.start || (bound.op === "between" && bound.end));
}

export function buildMessagesQuery(input: MessagesQueryInput): string {
  return advancedMessages(input);
}

export function buildContactsQuery(input: ContactsQueryInput): string {
  return advancedContacts(input);
}

export function canSubmitMessages(input: MessagesQueryInput): boolean {
  return Boolean(
    input.nameOrHandle.trim() ||
      input.handle.trim() ||
      input.msgType !== "all" ||
      composeCountComparison(input.participants),
  );
}

export function canSubmitContacts(input: ContactsQueryInput): boolean {
  return Boolean(
    input.contactName.trim() ||
      input.handle.trim() ||
      dateBoundHasValue(input.firstMsgBound) ||
      dateBoundHasValue(input.lastMsgBound) ||
      input.activity !== "any" ||
      input.noPreferredName ||
      input.noHandle ||
      input.services.length > 0,
  );
}
