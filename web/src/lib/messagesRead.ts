import { currentAccountId } from "./accountScope";
import {
  encodeMessageCursor,
  type MessagePageCursor,
} from "./messageCursor";
import {
  DEFAULT_MESSAGE_PAGE_SIZE,
  MAX_MESSAGE_PAGE_SIZE,
} from "./messagePageSize";
import { loadAccountProfile } from "./accountProfile";
import {
  combinedDedupeSql,
  displayName,
  getDb,
  usefulNameHint,
} from "./dbCore";
import type { MessageRow } from "./types";

export { DEFAULT_MESSAGE_PAGE_SIZE, MAX_MESSAGE_PAGE_SIZE };

export type MessagePage = {
  messages: MessageRow[];
  /** Cursor for the next older page; null when the oldest page is loaded. */
  nextOlderCursor: string | null;
  hasOlder: boolean;
};

export function messagesForConversationYear(
  conversationIds: number | number[],
  year: number,
  source?: string | null,
): MessageRow[] {
  return loadConversationMessages(conversationIds, {
    year,
    source,
    order: "asc",
  }).messages;
}

/** All messages for conversation(s), newest first (no year filter). */
export function messagesForConversations(
  conversationIds: number | number[],
  source?: string | null,
): MessageRow[] {
  return loadConversationMessages(conversationIds, {
    source,
    order: "desc",
  }).messages;
}

/**
 * Newest page of messages in chronological display order, with a cursor for
 * older pages. Pass `before` to continue from a previous nextOlderCursor.
 */
export function messagesPageForConversations(
  conversationIds: number | number[],
  opts?: {
    source?: string | null;
    before?: MessagePageCursor | null;
    limit?: number;
  },
): MessagePage {
  const limit = Math.min(
    Math.max(1, opts?.limit ?? DEFAULT_MESSAGE_PAGE_SIZE),
    MAX_MESSAGE_PAGE_SIZE,
  );
  const loaded = loadConversationMessages(conversationIds, {
    source: opts?.source,
    order: "desc",
    before: opts?.before ?? null,
    limit: limit + 1,
  });
  const hasOlder = loaded.rows.length > limit;
  const pageRows = hasOlder
    ? loaded.rows.slice(0, limit)
    : loaded.rows;
  // Display chronological ascending.
  const messages = [...pageRows]
    .reverse()
    .map((r) => loaded.toMessage(r));
  const oldest = pageRows[pageRows.length - 1];
  const nextOlderCursor =
    hasOlder && oldest
      ? encodeMessageCursor({
          timestamp: oldest.timestamp,
          sortOrder: oldest.sort_order,
          id: oldest.id,
        })
      : null;
  return { messages, nextOlderCursor, hasOlder };
}

type RawMessageRow = {
  id: number;
  conversation_id: number;
  source: string;
  timestamp: string;
  sort_order: number;
  is_from_me: number;
  sender: string | null;
  body: string | null;
  is_announcement: number;
  first_name: string | null;
  last_name: string | null;
  preferred_handle: string | null;
  name_hint: string | null;
};

function loadConversationMessages(
  conversationIds: number | number[],
  opts: {
    year?: number;
    source?: string | null;
    order: "asc" | "desc";
    before?: MessagePageCursor | null;
    limit?: number;
  },
): {
  rows: RawMessageRow[];
  messages: MessageRow[];
  toMessage: (r: RawMessageRow) => MessageRow;
} {
  const accountId = currentAccountId();
  const ids = (
    Array.isArray(conversationIds) ? conversationIds : [conversationIds]
  ).filter((id) => Number.isFinite(id));
  if (!ids.length) {
    return {
      rows: [],
      messages: [],
      toMessage: () => {
        throw new Error("empty conversation");
      },
    };
  }
  const db = getDb();
  const owner = loadAccountProfile(accountId);
  const placeholders = ids.map(() => "?").join(",");
  const sourceSql = opts.source ? " AND m.source = ?" : "";
  const yearSql =
    opts.year != null ? " AND m.timestamp >= ? AND m.timestamp < ?" : "";
  const beforeSql = opts.before
    ? ` AND (
         m.timestamp < ?
         OR (m.timestamp = ? AND m.sort_order < ?)
         OR (m.timestamp = ? AND m.sort_order = ? AND m.id < ?)
       )`
    : "";
  const orderSql =
    opts.order === "desc"
      ? "ORDER BY m.timestamp DESC, m.sort_order DESC, m.id DESC"
      : "ORDER BY m.timestamp, m.sort_order, m.id";
  const limitSql =
    opts.limit != null ? ` LIMIT ${Math.trunc(opts.limit)}` : "";

  const params: Array<string | number> = [accountId, ...ids];
  if (opts.year != null) {
    params.push(`${opts.year}-`, `${opts.year + 1}-`);
  }
  if (opts.before) {
    params.push(
      opts.before.timestamp,
      opts.before.timestamp,
      opts.before.sortOrder,
      opts.before.timestamp,
      opts.before.sortOrder,
      opts.before.id,
    );
  }
  if (opts.source) params.push(opts.source);

  const rows = db
    .prepare(
      `SELECT m.id, m.conversation_id, m.source, m.timestamp, m.sort_order, m.is_from_me, m.sender, m.body, m.is_announcement,
              c.first_name, c.last_name, c.preferred_handle,
              p.name_hint
       FROM messages m
       JOIN conversations conv ON conv.id = m.conversation_id
       LEFT JOIN contact_handles cp ON cp.handle = m.sender AND cp.account_id = conv.account_id
       LEFT JOIN contacts c ON c.id = cp.contact_id AND c.account_id = cp.account_id
       LEFT JOIN participants p
         ON p.conversation_id = m.conversation_id AND p.handle = m.sender
       WHERE conv.account_id = ? AND m.conversation_id IN (${placeholders})${yearSql}${beforeSql}${sourceSql}${combinedDedupeSql(opts.source, "m")}
       ${orderSql}${limitSql}`,
    )
    .all(...params) as RawMessageRow[];

  const attsByMsg = new Map<
    number,
    Array<{
      id: number;
      mimeType: string | null;
      originalName: string | null;
      assetsPath: string | null;
      sha256: string | null;
      derivedMimeType: string | null;
      derivedAssetsPath: string | null;
      derivedSha256: string | null;
    }>
  >();
  if (rows.length) {
    const msgIds = rows.map((r) => r.id);
    const chunkSize = 400;
    for (let i = 0; i < msgIds.length; i += chunkSize) {
      const chunk = msgIds.slice(i, i + chunkSize);
      const attPlaceholders = chunk.map(() => "?").join(",");
      const attRows = db
        .prepare(
          `SELECT message_id, id, mime_type, original_name, assets_path, sha256,
                  derived_mime_type, derived_assets_path, derived_sha256
           FROM attachments
           WHERE message_id IN (${attPlaceholders})
           ORDER BY message_id, id`,
        )
        .all(...chunk) as Array<{
        message_id: number;
        id: number;
        mime_type: string | null;
        original_name: string | null;
        assets_path: string | null;
        sha256: string | null;
        derived_mime_type: string | null;
        derived_assets_path: string | null;
        derived_sha256: string | null;
      }>;
      for (const a of attRows) {
        const list = attsByMsg.get(a.message_id) ?? [];
        list.push({
          id: a.id,
          mimeType: a.mime_type,
          originalName: a.original_name,
          assetsPath: a.assets_path,
          sha256: a.sha256,
          derivedMimeType: a.derived_mime_type,
          derivedAssetsPath: a.derived_assets_path,
          derivedSha256: a.derived_sha256,
        });
        attsByMsg.set(a.message_id, list);
      }
    }
  }

  const toMessage = (r: RawMessageRow): MessageRow => {
    const isFromMe = r.is_from_me !== 0;
    let senderName: string;
    if (isFromMe) {
      senderName = owner.display_name;
    } else {
      const hasContactName = Boolean(
        r.first_name?.trim() || r.last_name?.trim(),
      );
      senderName = displayName({
        first_name: r.first_name,
        last_name: r.last_name,
        preferred_handle: r.preferred_handle ?? r.sender,
      });
      if (!hasContactName) {
        const hint = usefulNameHint(r.name_hint, r.sender);
        if (hint) senderName = hint;
      }
    }

    return {
      id: r.id,
      conversationId: r.conversation_id,
      source: r.source,
      timestamp: r.timestamp,
      isFromMe,
      sender: r.sender,
      senderName,
      body: r.body,
      isAnnouncement: r.is_announcement !== 0,
      attachments: attsByMsg.get(r.id) ?? [],
    };
  };

  return {
    rows,
    messages: rows.map(toMessage),
    toMessage,
  };
}
