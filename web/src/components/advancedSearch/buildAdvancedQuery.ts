import type { Key } from "react-aria-components";
import { advancedContacts, advancedMessages, composeCountComparison } from "../../lib/searchQuery";

export { composeCountComparison };

export type AdvancedSearchMode = "messages" | "contacts";

/** Same operators as web-next CountField (Any / Equal to / More than / Less than). */
export type CountComparator = "=" | ">" | "<";
export type CountFilterInput = {
  comparator: CountComparator | "any";
  value: string;
};

export type ActivityFilter = "any" | "messages" | "no-messages";

/** Operator for first-/last-message calendar bounds. */
export type DateBoundOp = "any" | "after" | "before" | "between";

export type DateBoundFilter = {
  op: DateBoundOp;
  /** On or after / Before date, or Between start. */
  start: string;
  /** Between end only. */
  end: string;
};

export const EMPTY_COUNT: CountFilterInput = { comparator: "any", value: "" };
export const EMPTY_DATE_BOUND: DateBoundFilter = { op: "any", start: "", end: "" };

export function dateBoundHasValue(bound: DateBoundFilter): boolean {
  if (bound.op === "any") return false;
  return Boolean(bound.start || (bound.op === "between" && bound.end));
}

export type MessagesQueryInput = {
  nameOrHandle: string;
  handle: string;
  msgType: "all" | "direct" | "group";
  participants: CountFilterInput;
};

export type ContactsQueryInput = {
  contactName: string;
  handle: string;
  firstMsgBound: DateBoundFilter;
  lastMsgBound: DateBoundFilter;
  activity: ActivityFilter;
  noPreferredName: boolean;
  noHandle: boolean;
  services: Key[];
};

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
