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

/** Emit one `prefix:` date token: `>=D`, `<D`, or an inclusive `D..D` range. */
export function pushDateBoundTokens(
  push: (s: string) => void,
  prefix: "first-message" | "last-message",
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
      if (bound.start && bound.end) push(`${prefix}:${bound.start}..${bound.end}`);
      else if (bound.start) push(`${prefix}:>=${bound.start}`);
      else if (bound.end) push(`${prefix}:<${bound.end}`);
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
  if (input.msgType === "direct") push("kind:direct");
  if (input.msgType === "group") push("kind:group");
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
  pushDateBoundTokens(push, "first-message", input.firstMsgBound);
  pushDateBoundTokens(push, "last-message", input.lastMsgBound);
  if (input.activity === "messages") push("messages:>0");
  if (input.activity === "no-messages") push("messages:0");
  if (input.noPreferredName) push("name:none");
  if (input.noHandle) push("handle:none");
  // Several ticked transports go in one word, comma separated, which the
  // language reads as "any of these".
  const services = input.services.map((id) => String(id).trim()).filter(Boolean);
  if (services.length > 0) push(`service:${services.join(",")}`);
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
