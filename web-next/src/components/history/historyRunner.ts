import type { HistoryCommand } from "./historyTypes";
import { sortedContactIds } from "./historyTypes";

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

async function fetchLabelMemberIds(name: string): Promise<number[]> {
  const res = await fetch(
    `/api/contact-labels/members?name=${encodeURIComponent(name)}`,
  );
  if (!res.ok) return [];
  const data = (await res.json()) as { memberContactIds?: number[] };
  return sortedContactIds(data.memberContactIds ?? []);
}

/** Set label membership to exactly the given contact IDs. */
async function setLabelMemberIds(
  name: string,
  contactIds: number[],
): Promise<void> {
  const target = sortedContactIds(contactIds);
  const current = await fetchLabelMemberIds(name);
  const targetSet = new Set(target);
  const toAdd = target.filter((id) => !current.includes(id));
  const toRemove = current.filter((id) => !targetSet.has(id));
  if (toAdd.length > 0) {
    await jsonFetch(
      "/api/contacts/labels",
      {
        method: "POST",
        body: JSON.stringify({ ids: toAdd, name, enable: true }),
      },
      "label membership failed",
    );
  }
  if (toRemove.length > 0) {
    await jsonFetch(
      "/api/contacts/labels",
      {
        method: "POST",
        body: JSON.stringify({ ids: toRemove, name, enable: false }),
      },
      "label membership failed",
    );
  }
}

async function restoreContactLabelSnapshots(
  snapshots: Array<{ contactId: number; labels: string[] }>,
): Promise<void> {
  for (const { contactId, labels } of snapshots) {
    await jsonFetch(
      `/api/contacts/${contactId}`,
      {
        method: "PATCH",
        body: JSON.stringify({ labels }),
      },
      "restore labels failed",
    );
  }
}

async function clearContactLabelSnapshots(
  snapshots: Array<{ contactId: number; labels: string[] }>,
): Promise<void> {
  for (const { contactId } of snapshots) {
    await jsonFetch(
      `/api/contacts/${contactId}`,
      {
        method: "PATCH",
        body: JSON.stringify({ labels: [] }),
      },
      "clear labels failed",
    );
  }
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
    case "renameLabel":
      await jsonFetch(
        "/api/contact-labels",
        {
          method: "PATCH",
          body: JSON.stringify({ from: cmd.to, to: cmd.from }),
        },
        "rename label failed",
      );
      return;
    case "labelMembership":
      if (cmd.clearSnapshots?.length) {
        await restoreContactLabelSnapshots(cmd.clearSnapshots);
        return;
      }
      await setLabelMemberIds(cmd.name, cmd.beforeContactIds);
      return;
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
    case "renameLabel":
      await jsonFetch(
        "/api/contact-labels",
        {
          method: "PATCH",
          body: JSON.stringify({ from: cmd.from, to: cmd.to }),
        },
        "rename label failed",
      );
      return;
    case "labelMembership":
      if (cmd.clearSnapshots?.length) {
        await clearContactLabelSnapshots(cmd.clearSnapshots);
        return;
      }
      await setLabelMemberIds(cmd.name, cmd.afterContactIds);
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
