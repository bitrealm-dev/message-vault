import { currentAccountId } from "./accountScope";
import {
  getDb,
  hasDuplicateOfColumn,
  hasTrashedHandlesTable,
  resetDb,
  usefulNameHint,
} from "./dbCore";
import { formatPhoneDisplay } from "./phoneE164";
import {
  contactMessageSourceCountsForConversations,
  contactYearlyThreadsForPhones,
  type ContactSourceCounts,
} from "./contactsRead";
import { contactGroupChatThreadsForPhones } from "./groupChatsRead";
import type { GroupChatThread, UnassignedHandle, YearThread } from "./types";

/** 1:1 conversations with messages whose handle is not on any contact. */
export function listUnassignedHandles(): UnassignedHandle[] {
  return listHandleSection("unassigned");
}

/** Unassigned handles that were moved to Trash. */
export function listTrashedHandles(): UnassignedHandle[] {
  resetDb();
  return listHandleSection("trash");
}


function listHandleSection(section: "unassigned" | "trash"): UnassignedHandle[] {
  const accountId = currentAccountId();
  const db = getDb();
  const hideDupes = hasDuplicateOfColumn() ? " AND m.duplicate_of IS NULL" : "";
  const hasTrash = hasTrashedHandlesTable(db);
  if (section === "trash" && !hasTrash) return [];

  const trashFilter = !hasTrash
    ? ""
    : section === "trash"
      ? `AND EXISTS (
           SELECT 1 FROM trashed_handles th
           WHERE th.handle = c.chat_identifier AND th.account_id = c.account_id
         )`
      : `AND NOT EXISTS (
           SELECT 1 FROM trashed_handles th
           WHERE th.handle = c.chat_identifier AND th.account_id = c.account_id
         )`;

  const trashedAtSelect =
    section === "trash" && hasTrash
      ? `, (
           SELECT th.trashed_at FROM trashed_handles th
           WHERE th.handle = c.chat_identifier AND th.account_id = c.account_id
           LIMIT 1
         ) AS trashed_at`
      : `, NULL AS trashed_at`;

  const rows = db
    .prepare(
      `SELECT c.chat_identifier AS handle,
              MAX(p.name_hint) AS name_hint,
              COUNT(m.id) AS message_count,
              MIN(substr(m.timestamp, 1, 10)) AS date_start,
              MAX(substr(m.timestamp, 1, 10)) AS date_end
              ${trashedAtSelect}
       FROM conversations c
       JOIN messages m ON m.conversation_id = c.id
       LEFT JOIN participants p
         ON p.conversation_id = c.id AND p.handle = c.chat_identifier
       WHERE c.account_id = ?
         AND c.conversation_type = 'individual'
         AND NOT EXISTS (
           SELECT 1 FROM contact_handles cp
           WHERE cp.handle = c.chat_identifier AND cp.account_id = c.account_id
         )
         ${trashFilter}${hideDupes}
       GROUP BY c.id
       HAVING message_count > 0
       ORDER BY ${section === "trash" ? "trashed_at DESC, " : ""}handle COLLATE NOCASE`,
    )
    .all(accountId) as Array<{
    handle: string;
    name_hint: string | null;
    message_count: number;
    date_start: string | null;
    date_end: string | null;
    trashed_at: string | null;
  }>;

  return rows
    .map((r) => {
      const hintUseful = usefulNameHint(r.name_hint, r.handle);
      const displayName = hintUseful ?? formatPhoneDisplay(r.handle);
      const sortKey = hintUseful ? `${hintUseful}\0${r.handle}` : r.handle;
      const ch = (hintUseful ?? r.handle).charAt(0).toUpperCase();
      const letter = ch >= "A" && ch <= "Z" ? ch : "#";
      return {
        handle: r.handle,
        displayName,
        nameHint: hintUseful,
        messageCount: r.message_count,
        dateStart: r.date_start,
        dateEnd: r.date_end,
        sortKey,
        letter,
        unverified: Boolean(hintUseful),
        ...(r.trashed_at ? { trashedAt: r.trashed_at } : {}),
      };
    })
    .sort((a, b) => {
      if (section === "trash") {
        const at = (b.trashedAt ?? "").localeCompare(a.trashedAt ?? "");
        if (at !== 0) return at;
      }
      return a.sortKey.localeCompare(b.sortKey, undefined, {
        sensitivity: "base",
      });
    });
}

export function unassignedThreadsBundle(
  handle: string,
  source?: string | null,
  opts?: { includeTrashed?: boolean },
): {
  handle: string;
  yearly: YearThread[];
  groupChats: GroupChatThread[];
  messageSources: string[];
  sourceCounts: ContactSourceCounts;
} | null {
  const accountId = currentAccountId();
  const trimmed = handle.trim();
  if (!trimmed) return null;
  const db = getDb();
  const conv = db
    .prepare(
      `SELECT id FROM conversations
       WHERE account_id = ? AND conversation_type = 'individual' AND chat_identifier = ?`,
    )
    .get(accountId, trimmed) as { id: number } | undefined;
  if (!conv) return null;

  if (!opts?.includeTrashed) {
    const owned = db
      .prepare(
        `SELECT 1 AS ok FROM contact_handles WHERE account_id = ? AND handle = ?`,
      )
      .get(accountId, trimmed) as { ok: number } | undefined;
    if (owned) return null;
  }

  const hasMsgs = db
    .prepare(
      `SELECT 1 AS ok FROM messages WHERE conversation_id = ? LIMIT 1`,
    )
    .get(conv.id) as { ok: number } | undefined;
  if (!hasMsgs) return null;

  const phones = [trimmed];
  const groupChats = contactGroupChatThreadsForPhones(phones, source);
  const individualIds = [conv.id];
  const groupConvIds = groupChats.flatMap((g) =>
    g.conversationIds?.length > 0 ? g.conversationIds : [g.conversationId],
  );
  const allConvIds = [...new Set([...individualIds, ...groupConvIds])];
  const sourceCounts =
    contactMessageSourceCountsForConversations(individualIds);
  const anySourceCounts =
    contactMessageSourceCountsForConversations(allConvIds);

  return {
    handle: trimmed,
    yearly: contactYearlyThreadsForPhones(phones, source, {
      includeTrashed: opts?.includeTrashed,
    }),
    groupChats,
    messageSources: Object.keys(anySourceCounts.bySource).sort(),
    sourceCounts,
  };
}
