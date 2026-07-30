import Database from "better-sqlite3";

import { currentAccountId } from "./accountScope";
import { combinedDedupeSql, getDb, resetDb } from "./dbCore";
import { dbPath } from "./paths";
import {
  hasSearchCriteria,
  parseSearchQuery,
  toFtsMatch,
  type ParsedSearchQuery,
} from "./searchQuery";
import { ensureVaultSchema } from "./vaultSchema";

export type SearchHitMessage = {
  id: number;
  timestamp: string;
  snippet: string;
  isFromMe: boolean;
  sender: string | null;
};

export type SearchConversationHit = {
  conversationId: number;
  conversationType: "group" | "individual";
  /** Contact represented by a direct conversation, when its handle is assigned. */
  contactId: number | null;
  title: string;
  chatIdentifier: string;
  matchCount: number;
  dateStart: string | null;
  dateEnd: string | null;
  /** Best matching message for preview / scroll-to. */
  topMatch: SearchHitMessage | null;
};

export type SearchResult = {
  query: string;
  parsed: ParsedSearchQuery;
  totalConversations: number;
  /** Every distinct contact represented by matching direct conversations. */
  contactIds: number[];
  hits: SearchConversationHit[];
};

const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 100;

function openWritable(): Database.Database {
  const db = new Database(dbPath());
  ensureVaultSchema(db);
  db.pragma("foreign_keys = ON");
  return db;
}

function snippetFromBody(body: string | null, terms: string[]): string {
  const text = (body ?? "").replace(/\s+/g, " ").trim();
  if (!text) return "";
  const lower = text.toLowerCase();
  let idx = -1;
  for (const t of terms) {
    const i = lower.indexOf(t.toLowerCase());
    if (i >= 0 && (idx < 0 || i < idx)) idx = i;
  }
  if (idx < 0) {
    return text.length > 140 ? `${text.slice(0, 140)}…` : text;
  }
  const start = Math.max(0, idx - 40);
  const end = Math.min(text.length, idx + 100);
  const slice = text.slice(start, end);
  return `${start > 0 ? "…" : ""}${slice}${end < text.length ? "…" : ""}`;
}

function conversationTitle(
  conversationType: string,
  groupTitle: string | null,
  chatIdentifier: string,
  participantNames: string[],
): string {
  if (conversationType === "group") {
    if (groupTitle?.trim()) return groupTitle.trim();
    if (participantNames.length) return participantNames.slice(0, 4).join(", ");
    return chatIdentifier || "Group";
  }
  if (participantNames[0]) return participantNames[0];
  return chatIdentifier || "Conversation";
}

export function searchVault(
  rawQuery: string,
  opts: { limit?: number; offset?: number; source?: string | null } = {},
): SearchResult {
  const accountId = currentAccountId();
  const parsed = parseSearchQuery(rawQuery);
  const limit = Math.min(
    Math.max(1, opts.limit ?? DEFAULT_LIMIT),
    MAX_LIMIT,
  );
  const offset = Math.max(0, opts.offset ?? 0);

  if (!hasSearchCriteria(parsed) && !rawQuery.trim()) {
    return {
      query: rawQuery,
      parsed,
      totalConversations: 0,
      contactIds: [],
      hits: [],
    };
  }

  // Ensure FTS exists even if the readonly connection opened before migration.
  const writeDb = openWritable();
  writeDb.close();
  resetDb();

  const db = getDb();
  const fts = toFtsMatch(parsed);
  const params: unknown[] = [accountId];
  const where: string[] = ["c.account_id = ?"];

  if (fts) {
    where.push(
      `m.id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)`,
    );
    params.push(fts);
  }

  if (parsed.from) {
    where.push(
      `(m.is_from_me = 0 AND (m.sender LIKE ? OR EXISTS (
         SELECT 1 FROM participants p
         WHERE p.conversation_id = c.id
           AND (p.handle LIKE ? OR coalesce(p.name_hint, '') LIKE ?)
       )))`,
    );
    const like = `%${parsed.from}%`;
    params.push(like, like, like);
  }

  if (parsed.to) {
    where.push(
      `EXISTS (
         SELECT 1 FROM participants p
         WHERE p.conversation_id = c.id
           AND (p.handle LIKE ? OR coalesce(p.name_hint, '') LIKE ?)
       )`,
    );
    const like = `%${parsed.to}%`;
    params.push(like, like);
  }

  if (parsed.after) {
    where.push(`m.timestamp >= ?`);
    params.push(parsed.after);
  }
  if (parsed.before) {
    where.push(`m.timestamp < ?`);
    // exclusive upper bound: allow full day by appending time if date-only
    const before = parsed.before.length === 10
      ? `${parsed.before}T23:59:59.999Z`
      : parsed.before;
    params.push(before);
  }

  const sourceFilter = opts.source ?? parsed.source;
  if (sourceFilter) {
    where.push(`m.source = ?`);
    params.push(sourceFilter);
  }

  if (parsed.conversationType) {
    where.push(`c.conversation_type = ?`);
    params.push(parsed.conversationType);
  }

  if (parsed.hasAttachment) {
    where.push(
      `EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)`,
    );
  }

  if (parsed.label) {
    where.push(
      `EXISTS (
         SELECT 1
         FROM contact_handles ch
         JOIN contact_label_members clm ON clm.contact_id = ch.contact_id
         JOIN contact_labels cl ON cl.id = clm.label_id
         JOIN participants p ON p.handle = ch.handle AND p.conversation_id = c.id
         WHERE ch.account_id = c.account_id
           AND cl.account_id = c.account_id
           AND cl.name = ?
       )`,
    );
    params.push(parsed.label);
  }

  if (!parsed.includeTrash) {
    where.push(
      `NOT EXISTS (
         SELECT 1 FROM trashed_conversations tc
         WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
       )`,
    );
    where.push(
      `NOT EXISTS (
         SELECT 1 FROM trashed_handles th
         WHERE th.account_id = c.account_id AND th.handle = c.chat_identifier
       )`,
    );
  }

  where.push("1=1"); // keep join shape stable
  const dedupe = combinedDedupeSql(sourceFilter, "m");

  const whereSql = where.join(" AND ");

  /** Date-only bounds include the full day, matching `before:`. */
  const endOfDayBound = (raw: string) =>
    raw.length === 10 ? `${raw}T23:59:59.999Z` : raw;

  const having: string[] = [];
  const havingParams: unknown[] = [];
  if (parsed.lastContact) {
    having.push(`MAX(m.timestamp) < ?`);
    havingParams.push(endOfDayBound(parsed.lastContact));
  }
  if (parsed.firstContact) {
    having.push(`MIN(m.timestamp) < ?`);
    havingParams.push(endOfDayBound(parsed.firstContact));
  }
  const havingSql =
    having.length > 0 ? `HAVING ${having.join(" AND ")}` : "";

  const countRow = db
    .prepare(
      `SELECT COUNT(*) AS n FROM (
         SELECT c.id
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE ${whereSql}${dedupe}
         GROUP BY c.id
         ${havingSql}
       )`,
    )
    .get(...params, ...havingParams) as { n: number };

  const convRows = db
    .prepare(
      `SELECT
         c.id AS conversation_id,
         c.conversation_type AS conversation_type,
         c.group_title AS group_title,
         c.chat_identifier AS chat_identifier,
         (
           SELECT ch.contact_id
           FROM contact_handles ch
           WHERE ch.account_id = c.account_id
             AND ch.handle = c.chat_identifier
             AND c.conversation_type = 'individual'
           LIMIT 1
         ) AS contact_id,
         COUNT(DISTINCT m.id) AS match_count,
         MIN(m.timestamp) AS date_start,
         MAX(m.timestamp) AS date_end,
         MIN(m.id) AS sample_message_id
       FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE ${whereSql}${dedupe}
       GROUP BY c.id
       ${havingSql}
       ORDER BY MAX(m.timestamp) DESC
       LIMIT ? OFFSET ?`,
    )
    .all(...params, ...havingParams, limit, offset) as Array<{
    conversation_id: number;
    conversation_type: string;
    group_title: string | null;
    chat_identifier: string;
    contact_id: number | null;
    match_count: number;
    date_start: string | null;
    date_end: string | null;
    sample_message_id: number;
  }>;

  const contactRows = db
    .prepare(
      `SELECT DISTINCT contact_id
       FROM (
         SELECT ch.contact_id AS contact_id
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         JOIN contact_handles ch
           ON ch.account_id = c.account_id
          AND ch.handle = c.chat_identifier
         WHERE ${whereSql}${dedupe}
           AND c.conversation_type = 'individual'
         GROUP BY c.id, ch.contact_id
         ${havingSql}
       )
       ORDER BY contact_id`,
    )
    .all(...params, ...havingParams) as Array<{ contact_id: number }>;
  const contactIds = contactRows.map((row) => row.contact_id);

  const highlightTerms = [
    ...parsed.terms,
    ...parsed.phrases,
    ...(parsed.subject ? [parsed.subject] : []),
  ];

  const hits: SearchConversationHit[] = convRows.map((row) => {
    const participants = db
      .prepare(
        `SELECT handle, name_hint FROM participants WHERE conversation_id = ? ORDER BY id`,
      )
      .all(row.conversation_id) as Array<{
      handle: string;
      name_hint: string | null;
    }>;
    const names = participants.map((p) => p.name_hint?.trim() || p.handle);

    let topMatch: SearchHitMessage | null = null;
    const msg = db
      .prepare(
        `SELECT id, timestamp, body, is_from_me, sender
         FROM messages
         WHERE id = ?`,
      )
      .get(row.sample_message_id) as
      | {
          id: number;
          timestamp: string;
          body: string | null;
          is_from_me: number;
          sender: string | null;
        }
      | undefined;

    // Prefer a message that actually contains a highlight term when FTS was used.
    let best = msg;
    if (fts && highlightTerms.length > 0) {
      const candidates = db
        .prepare(
          `SELECT m.id, m.timestamp, m.body, m.is_from_me, m.sender
           FROM messages m
           WHERE m.conversation_id = ?
             AND m.id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)
           ORDER BY m.timestamp DESC
           LIMIT 5`,
        )
        .all(row.conversation_id, fts) as Array<{
        id: number;
        timestamp: string;
        body: string | null;
        is_from_me: number;
        sender: string | null;
      }>;
      if (candidates.length > 0) best = candidates[0];
    }

    if (best) {
      topMatch = {
        id: best.id,
        timestamp: best.timestamp,
        snippet: snippetFromBody(best.body, highlightTerms),
        isFromMe: best.is_from_me === 1,
        sender: best.sender,
      };
    }

    return {
      conversationId: row.conversation_id,
      conversationType:
        row.conversation_type === "group" ? "group" : "individual",
      contactId: row.contact_id,
      title: conversationTitle(
        row.conversation_type,
        row.group_title,
        row.chat_identifier,
        names,
      ),
      chatIdentifier: row.chat_identifier,
      matchCount: row.match_count,
      dateStart: row.date_start,
      dateEnd: row.date_end,
      topMatch,
    };
  });

  return {
    query: rawQuery,
    parsed,
    totalConversations: countRow.n,
    contactIds,
    hits,
  };
}
