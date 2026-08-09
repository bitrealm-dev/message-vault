/** Serializable undo/redo commands (no closures). */

export const HISTORY_MAX_DEPTH = 20;
export const HISTORY_TOAST_MS = 15_000;

export type TrashContactMode = "contact_and_messages" | "messages_only";

export type HistoryCommand =
  | {
      type: "trashContacts";
      contactIds: number[];
      mode: TrashContactMode;
      /** Display names for toast / undo label (same order as contactIds). */
      names: string[];
      /** Populated for messages_only so undo can restore handles. */
      handles?: string[];
      label: string;
    }
  | {
      type: "trashGroupThread";
      conversationIds: number[];
      /** Display titles for toast / undo label (same order as conversationIds). */
      titles: string[];
      label: string;
    }
  | {
      type: "trashMessageThreads";
      handles: string[];
      conversationIds: number[];
      /** Direct-thread names followed by group titles. */
      subjects: string[];
      label: string;
    }
  | {
      type: "createContact";
      contactId: number;
      name: string;
      label: string;
    }
  | {
      type: "createLabel";
      name: string;
      label: string;
    }
  | {
      type: "renameLabel";
      from: string;
      to: string;
      label: string;
    }
  | {
      type: "labelMembership";
      name: string;
      beforeContactIds: number[];
      afterContactIds: number[];
      /** When set, clears all labels on these contacts (undo restores snapshots). */
      clearSnapshots?: Array<{ contactId: number; labels: string[] }>;
      label: string;
    }
  | {
      type: "deleteLabel";
      name: string;
      memberContactIds: number[];
      label: string;
    };

export function sortedContactIds(ids: number[]): number[] {
  return [...new Set(ids.filter((id) => Number.isFinite(id)))].sort(
    (a, b) => a - b,
  );
}

export function renameLabelHistoryLabel(from: string, to: string): string {
  const a = from.trim() || "label";
  const b = to.trim() || "label";
  return `Rename label ${a} to ${b}`;
}

export function labelMembershipHistoryLabel(
  name: string,
  before: number[],
  after: number[],
): string {
  const label = name.trim() || "label";
  const beforeSet = new Set(before);
  const added = after.filter((id) => !beforeSet.has(id)).length;
  const removed = before.filter((id) => !after.includes(id)).length;
  if (added > 0 && removed === 0) {
    const n = added;
    return n === 1
      ? `Add contact to ${label}`
      : `Add ${n} contacts to ${label}`;
  }
  if (removed > 0 && added === 0) {
    const n = removed;
    return n === 1
      ? `Remove contact from ${label}`
      : `Remove ${n} contacts from ${label}`;
  }
  return `Change ${label} membership`;
}

export function clearContactLabelsHistoryLabel(count: number): string {
  return count === 1
    ? "Clear labels for contact"
    : `Clear labels for ${count} contacts`;
}

export type HistoryToast = {
  text: string;
  /** When true, snackbar shows an Undo control (action toasts only). */
  showUndo: boolean;
};

function joinSubjects(subjects: string[], fallback: string): string {
  const cleaned = subjects.map((s) => s.trim()).filter(Boolean);
  return cleaned.length > 0 ? cleaned.join(", ") : fallback;
}

/**
 * Join names for toast copy, stopping before the string gets long and appending
 * an ellipsis when more subjects remain (matches the snackbar's truncated look).
 */
function joinSubjectsPreview(
  subjects: string[],
  fallback: string,
  maxChars = 48,
): string {
  const cleaned = subjects.map((s) => s.trim()).filter(Boolean);
  if (cleaned.length === 0) return fallback;
  let out = cleaned[0]!;
  for (let i = 1; i < cleaned.length; i++) {
    const next = `${out}, ${cleaned[i]}`;
    if (next.length > maxChars) return `${out}…`;
    out = next;
  }
  return out;
}

/** Past-tense snackbar copy for a just-pushed command. */
export function toastTextForCommand(cmd: HistoryCommand): string {
  switch (cmd.type) {
    case "createContact":
      return `Created new contact ${cmd.name.trim() || "contact"}`;
    case "createLabel":
      return `Created label ${cmd.name.trim() || "label"}`;
    case "renameLabel":
      return `Renamed label ${cmd.from.trim() || "label"} to ${cmd.to.trim() || "label"}`;
    case "labelMembership":
      if (cmd.clearSnapshots?.length) {
        const n = cmd.clearSnapshots.length;
        return n === 1
          ? "Cleared labels for contact"
          : `Cleared labels for ${n} contacts`;
      }
      return labelMembershipToastText(cmd.name, cmd.beforeContactIds, cmd.afterContactIds);
    case "deleteLabel":
      return `Deleted label ${cmd.name.trim() || "label"}`;
    case "trashContacts": {
      const n = cmd.contactIds.length;
      if (n === 1) {
        return `Deleted contact ${joinSubjects(cmd.names, "contact")}`;
      }
      const preview = joinSubjectsPreview(cmd.names, "contacts");
      return `Deleted ${n} contacts ${preview}`;
    }
    case "trashGroupThread": {
      const n = cmd.conversationIds.length;
      if (n === 1) {
        const title = joinSubjects(cmd.titles, "group message");
        return `Deleted group message ${title}`;
      }
      return `Deleted ${n} group messages`;
    }
    case "trashMessageThreads": {
      const n = cmd.handles.length + cmd.conversationIds.length;
      if (n === 1) {
        return `Deleted message ${joinSubjects(cmd.subjects, "message")}`;
      }
      return `Deleted ${n} messages ${joinSubjectsPreview(cmd.subjects, "messages")}`;
    }
  }
}

/** Snackbar after a successful undo (no nested Undo control). */
export function undoToastTextForCommand(cmd: HistoryCommand): string {
  switch (cmd.type) {
    case "trashContacts": {
      const n = cmd.contactIds.length;
      if (n === 1) {
        return `Undeleted contact ${joinSubjects(cmd.names, "contact")}`;
      }
      return `Undeleted ${n} contacts ${joinSubjectsPreview(cmd.names, "contacts")}`;
    }
    case "trashGroupThread": {
      const n = cmd.conversationIds.length;
      if (n === 1) {
        return `Undeleted group message ${joinSubjects(cmd.titles, "group message")}`;
      }
      return `Undeleted ${n} group messages`;
    }
    case "trashMessageThreads": {
      const n = cmd.handles.length + cmd.conversationIds.length;
      if (n === 1) {
        return `Undeleted message ${joinSubjects(cmd.subjects, "message")}`;
      }
      return `Undeleted ${n} messages ${joinSubjectsPreview(cmd.subjects, "messages")}`;
    }
    case "renameLabel":
      return `Renamed label ${cmd.to.trim() || "label"} to ${cmd.from.trim() || "label"}`;
    case "labelMembership":
      if (cmd.clearSnapshots?.length) {
        const n = cmd.clearSnapshots.length;
        return n === 1
          ? "Restored labels for contact"
          : `Restored labels for ${n} contacts`;
      }
      return labelMembershipUndoToastText(
        cmd.name,
        cmd.beforeContactIds,
        cmd.afterContactIds,
      );
    default:
      return `Undid — ${toastTextForCommand(cmd)}`;
  }
}

function labelMembershipToastText(
  name: string,
  before: number[],
  after: number[],
): string {
  const label = name.trim() || "label";
  const beforeSet = new Set(before);
  const added = after.filter((id) => !beforeSet.has(id)).length;
  const removed = before.filter((id) => !after.includes(id)).length;
  if (added > 0 && removed === 0) {
    return added === 1
      ? `Added contact to ${label}`
      : `Added ${added} contacts to ${label}`;
  }
  if (removed > 0 && added === 0) {
    return removed === 1
      ? `Removed contact from ${label}`
      : `Removed ${removed} contacts from ${label}`;
  }
  return `Updated ${label} membership`;
}

function labelMembershipUndoToastText(
  name: string,
  before: number[],
  after: number[],
): string {
  const label = name.trim() || "label";
  const beforeSet = new Set(before);
  const added = after.filter((id) => !beforeSet.has(id)).length;
  const removed = before.filter((id) => !after.includes(id)).length;
  if (added > 0 && removed === 0) {
    return added === 1
      ? `Removed contact from ${label}`
      : `Removed ${added} contacts from ${label}`;
  }
  if (removed > 0 && added === 0) {
    return removed === 1
      ? `Added contact to ${label}`
      : `Added ${removed} contacts to ${label}`;
  }
  return `Restored ${label} membership`;
}

/** Snackbar after a successful redo (no nested Undo control). */
export function redoToastTextForCommand(cmd: HistoryCommand): string {
  return `Redid — ${toastTextForCommand(cmd)}`;
}

/** Undo/Redo menu tooltip for a command with named subjects. */
export function trashContactsLabel(names: string[]): string {
  if (names.length <= 1) {
    return `Delete contact ${joinSubjects(names, "contact")}`;
  }
  return `Delete ${names.length} contacts ${joinSubjectsPreview(names, "contacts")}`;
}

export function trashGroupThreadLabel(titles: string[]): string {
  if (titles.length <= 1) {
    const joined = joinSubjects(titles, "group message");
    return `Delete group message ${joined}`;
  }
  return `Delete ${titles.length} group messages`;
}

export function trashMessageThreadsLabel(subjects: string[]): string {
  if (subjects.length <= 1) {
    return `Delete message ${joinSubjects(subjects, "message")}`;
  }
  return `Delete ${subjects.length} messages`;
}
