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
      type: "deleteLabel";
      name: string;
      memberContactIds: number[];
      label: string;
    };

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
    default:
      return `Undid — ${toastTextForCommand(cmd)}`;
  }
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
