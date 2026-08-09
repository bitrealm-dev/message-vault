import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import {
  trashGroupThreadLabel,
  type HistoryCommand,
} from "./history/historyTypes";

type TrashStatusMessages = {
  trashedOne: string;
  trashedMany: (n: number) => string;
  restoredOne: string;
  restoredMany: (n: number) => string;
  deletedOne: string;
  deletedMany: (n: number) => string;
};

const DEFAULT_STATUS: TrashStatusMessages = {
  trashedOne: "Moved to Trash",
  trashedMany: (n) => `Moved ${n} to Trash`,
  restoredOne: "Undeleted — back in Group Messages",
  restoredMany: (n) => `Undeleted ${n} group messages`,
  deletedOne: "Deleted forever",
  deletedMany: (n) => `Deleted ${n} group messages forever`,
};

const BROWSE_STATUS: TrashStatusMessages = {
  trashedOne: "Moved group message to Trash",
  trashedMany: (n) => `Moved ${n} group messages to Trash`,
  restoredOne: "",
  restoredMany: () => "",
  deletedOne: "",
  deletedMany: () => "",
};

/** Short display title for history toast / undo label. */
export function groupChatToastTitle(g: CollapsedGroupConversation): string {
  if (g.namedTitle?.trim()) return g.namedTitle.trim();
  if (g.participantNames.length > 0) {
    return g.participantNames.join(" · ");
  }
  return g.title?.trim() || "group message";
}

export function groupChatTrashHistoryEntry(
  ids: number[],
  titles: string[],
): Extract<HistoryCommand, { type: "trashGroupThread" }> {
  const resolved =
    titles.length === ids.length
      ? titles
      : ids.map((_, i) => titles[i]?.trim() || "group message");
  return {
    type: "trashGroupThread",
    conversationIds: ids,
    titles: resolved,
    label: trashGroupThreadLabel(resolved),
  };
}

/**
 * Shared useTrashActions option pieces for group-chat trash across shells.
 * Shells still supply getTargets, canTrash, onRemoved, setStatus, etc.
 * Soft trash skips confirm (undoable); permanent delete still confirms.
 */
export function createGroupChatTrashOptions(options?: {
  /** Browse uses browse-specific status strings. */
  variant?: "default" | "browse";
}): {
  endpoint: string;
  idField: string;
  confirmPermanent: (targets: number[]) => string;
  status: TrashStatusMessages;
  historyEntry: (
    ids: number[],
    titles: string[],
  ) => Extract<HistoryCommand, { type: "trashGroupThread" }>;
} {
  const variant = options?.variant ?? "default";

  return {
    endpoint: "/api/group-chats/trash",
    idField: "conversationId",
    confirmPermanent: (targets) => {
      if (targets.length === 1) return "Delete forever?";
      return `Delete ${targets.length} group messages forever?`;
    },
    status: variant === "browse" ? BROWSE_STATUS : DEFAULT_STATUS,
    historyEntry: groupChatTrashHistoryEntry,
  };
}
