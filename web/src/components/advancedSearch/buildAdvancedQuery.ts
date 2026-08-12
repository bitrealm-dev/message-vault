import type { Key } from "react-aria-components";

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

export function composeCountComparison(input: CountFilterInput): string | null {
  if (input.comparator === "any") return null;
  const value = input.value.trim();
  if (!/^\d+$/.test(value)) return null;
  return `${input.comparator}${value}`;
}

export function dateBoundHasValue(bound: DateBoundFilter): boolean {
  if (bound.op === "any") return false;
  return Boolean(bound.start || (bound.op === "between" && bound.end));
}

/** Emit `prefix:>=D` / `prefix:<D` tokens (Between = half-open pair). */
export function pushDateBoundTokens(
  push: (s: string) => void,
  prefix: "first-contact" | "last-contact",
  bound: DateBoundFilter,
): void {
  switch (bound.op) {
    case "any":
      return;
    case "after":
      if (bound.start) push(`${prefix}:>=${bound.start}`);
      return;
    case "before":
      if (bound.start) push(`${prefix}:<${bound.start}`);
      return;
    case "between":
      if (bound.start) push(`${prefix}:>=${bound.start}`);
      if (bound.end) push(`${prefix}:<${bound.end}`);
      return;
  }
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
  const parts: string[] = [];
  const push = (s: string) => {
    if (s.trim()) parts.push(s.trim());
  };
  if (input.nameOrHandle.trim()) push(input.nameOrHandle.trim());
  if (input.handle.trim()) push(`handle:${input.handle.trim()}`);
  if (input.msgType === "direct") push("is:direct");
  if (input.msgType === "group") push("is:group");
  const participantCmp = composeCountComparison(input.participants);
  if (participantCmp) push(`participants:${participantCmp}`);
  return parts.join(" ");
}

export function buildContactsQuery(input: ContactsQueryInput): string {
  const parts: string[] = [];
  const push = (s: string) => {
    if (s.trim()) parts.push(s.trim());
  };
  if (input.contactName.trim()) push(input.contactName.trim());
  if (input.handle.trim()) push(`handle:"${input.handle.trim()}"`);
  pushDateBoundTokens(push, "first-contact", input.firstMsgBound);
  pushDateBoundTokens(push, "last-contact", input.lastMsgBound);
  if (input.activity === "messages") push("has:messages");
  if (input.activity === "no-messages") push("has:no-messages");
  if (input.noPreferredName) push("has:no-name");
  if (input.noHandle) push("has:no-handle");
  for (const id of input.services) {
    push(`service:${String(id)}`);
  }
  push("search:contacts");
  return parts.join(" ");
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
