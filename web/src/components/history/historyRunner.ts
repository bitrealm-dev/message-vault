import type { HistoryCommand } from "./historyTypes";

async function readError(res: Response, fallback: string): Promise<string> {
  try {
    const data = (await res.json()) as { error?: string };
    return data.error ?? fallback;
  } catch {
    return fallback;
  }
}

async function jsonFetch(
  url: string,
  init: RequestInit,
  fallback: string,
): Promise<void> {
  const res = await fetch(url, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init.headers ?? {}),
    },
  });
  if (!res.ok) throw new Error(await readError(res, fallback));
}

/** Run the inverse of a forward command (undo). */
export async function undoCommand(cmd: HistoryCommand): Promise<void> {
  switch (cmd.type) {
    case "trashContacts":
      if (cmd.mode === "messages_only") {
        const handles = cmd.handles ?? [];
        if (handles.length === 0) {
          throw new Error("no handles to restore");
        }
        for (const handle of handles) {
          await jsonFetch(
            "/api/contacts/trash",
            {
              method: "DELETE",
              body: JSON.stringify({ handle }),
            },
            "restore failed",
          );
        }
        return;
      }
      await jsonFetch(
        "/api/contacts/trash",
        {
          method: "DELETE",
          body: JSON.stringify({ ids: cmd.contactIds }),
        },
        "restore failed",
      );
      return;
    case "trashGroupThread":
      for (const conversationId of cmd.conversationIds) {
        await jsonFetch(
          "/api/group-chats/trash",
          {
            method: "DELETE",
            body: JSON.stringify({ conversationId }),
          },
          "restore failed",
        );
      }
      return;
    case "trashMessageThreads":
      await jsonFetch(
        "/api/messages/trash",
        {
          method: "DELETE",
          body: JSON.stringify({
            handles: cmd.handles,
            conversationIds: cmd.conversationIds,
          }),
        },
        "restore failed",
      );
      return;
    case "createContact":
      await jsonFetch(
        "/api/contacts/trash",
        {
          method: "POST",
          body: JSON.stringify({
            ids: [cmd.contactId],
            mode: "contact_and_messages",
          }),
        },
        "undo create failed",
      );
      return;
    case "createLabel": {
      const res = await fetch(
        `/api/contact-labels/members?name=${encodeURIComponent(cmd.name)}`,
      );
      if (!res.ok) throw new Error(await readError(res, "label lookup failed"));
      const data = (await res.json()) as { memberContactIds?: number[] };
      const members = data.memberContactIds ?? [];
      if (members.length > 0) {
        throw new Error(
          "Label has members; undo create is unavailable",
        );
      }
      await jsonFetch(
        "/api/contact-labels",
        { method: "DELETE", body: JSON.stringify({ name: cmd.name }) },
        "delete label failed",
      );
      return;
    }
    case "deleteLabel":
      await jsonFetch(
        "/api/contact-labels/restore",
        {
          method: "POST",
          body: JSON.stringify({
            name: cmd.name,
            memberContactIds: cmd.memberContactIds,
          }),
        },
        "restore label failed",
      );
      return;
    default: {
      const _exhaustive: never = cmd;
      void _exhaustive;
      throw new Error("unknown history command");
    }
  }
}

/** Re-apply a forward command (redo). */
export async function redoCommand(cmd: HistoryCommand): Promise<void> {
  switch (cmd.type) {
    case "trashContacts":
      await jsonFetch(
        "/api/contacts/trash",
        {
          method: "POST",
          body: JSON.stringify({
            ids: cmd.contactIds,
            mode: cmd.mode,
          }),
        },
        "trash failed",
      );
      return;
    case "trashGroupThread":
      for (const conversationId of cmd.conversationIds) {
        await jsonFetch(
          "/api/group-chats/trash",
          {
            method: "POST",
            body: JSON.stringify({ conversationId }),
          },
          "trash failed",
        );
      }
      return;
    case "trashMessageThreads":
      await jsonFetch(
        "/api/messages/trash",
        {
          method: "POST",
          body: JSON.stringify({
            handles: cmd.handles,
            conversationIds: cmd.conversationIds,
          }),
        },
        "trash failed",
      );
      return;
    case "createContact":
      await jsonFetch(
        "/api/contacts/trash",
        {
          method: "DELETE",
          body: JSON.stringify({ ids: [cmd.contactId] }),
        },
        "restore failed",
      );
      return;
    case "createLabel":
      await jsonFetch(
        "/api/contact-labels",
        { method: "POST", body: JSON.stringify({ name: cmd.name }) },
        "create label failed",
      );
      return;
    case "deleteLabel":
      await jsonFetch(
        "/api/contact-labels",
        { method: "DELETE", body: JSON.stringify({ name: cmd.name }) },
        "delete label failed",
      );
      return;
    default: {
      const _exhaustive: never = cmd;
      void _exhaustive;
      throw new Error("unknown history command");
    }
  }
}
