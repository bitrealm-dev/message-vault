/**
 * Names that must not appear as user-created labels, and are filtered
 * from the labels list (nav / section labels).
 */
export const RESERVED_LABEL_NAMES = new Set(
  [
    "home",
    "contacts",
    "all",
    "excluded",
    "no-messages",
    "no messages",
    "unassigned",
    "trash",
    "groups",
    "group",
    "group-chats",
    "group chats",
    "group-chats-2",
    "group chats 2",
    "group-messages",
    "group messages",
    "group-messages-2",
    "group messages 2",
    "no-label",
    "no group",
    "labels",
    "label",
    "no label",
  ].map((s) => s.toLowerCase()),
);

export function isReservedLabelName(name: string): boolean {
  return RESERVED_LABEL_NAMES.has(name.trim().toLowerCase());
}

export function reservedLabelError(name: string): string {
  const key = name.trim().toLowerCase();
  if (key === "contacts") return "Contacts is a reserved label";
  if (key === "all") return "All is a reserved label";
  if (key === "excluded") return "Excluded is a reserved label";
  if (key === "unassigned") return "Unassigned is a reserved label";
  if (key === "trash") return "Trash is a reserved label";
  if (key === "no messages" || key === "no-messages") {
    return "No messages is a reserved label";
  }
  if (
    key === "groups" ||
    key === "group" ||
    key === "group chats" ||
    key === "group-chats" ||
    key === "group chats 2" ||
    key === "group-chats-2" ||
    key === "group messages" ||
    key === "group-messages" ||
    key === "group messages 2" ||
    key === "group-messages-2"
  ) {
    return "Group Messages is a reserved name";
  }
  if (
    key === "labels" ||
    key === "label" ||
    key === "no-label" ||
    key === "no label" ||
    key === "no group"
  ) {
    return `"${name.trim()}" is a reserved label`;
  }
  return `"${name.trim()}" is a reserved label`;
}
