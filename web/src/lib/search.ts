import Database from "better-sqlite3";

import { currentAccountId } from "./accountScope";
import { listContactsByIds, listLabelMemberContactIds } from "./contactsRead";
import { combinedDedupeSql, getDb, hasDuplicateOfColumn, resetDb } from "./dbCore";
import { ownerHandleMatcher } from "./owner";
import { dbPath } from "./paths";
import {
  hasDateBounds,
  hasSearchCriteria,
  parseSearchQuery,
  toFtsMatch,
  type DateBounds,
  type ParsedSearchQuery,
} from "./searchQuery";
import type { ContactListItem } from "./types";
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
  /** Present when `show:contact` groups results under each contact. */
  contacts?: SearchContactHit[];
  totalContacts?: number;
};

/** One contact plus the conversations of theirs that matched. */
export type SearchContactHit = {
  contact: ContactListItem;
  /** Matching conversations this contact takes part in. */
  hits: SearchConversationHit[];
  /** Total matching messages across those conversations. */
  matchCount: number;
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

/**
 * A contact is linked to a conversation by being its 1:1 handle or one of its
 * participants, so group filters reach every member.
 */
const CONTACT_LINKED_SQL = `(
  ch.handle = c.chat_identifier
  OR EXISTS (
    SELECT 1 FROM participants p_link
    WHERE p_link.conversation_id = c.id AND p_link.handle = ch.handle
  )
)`;

/**
 * Contacts whose overall first / last 1:1 message day falls within bounds,
 * independent of the query's other filters. Mirrors the date range shown in the
 * contact list. Resolved in one pass rather than per candidate conversation.
 */
function contactIdsWithinDayBounds(
  db: Database.Database,
  bound: "first" | "last",
  bounds: DateBounds,
): number[] {
  const accountId = currentAccountId();
  const hideDupes = hasDuplicateOfColumn() ? " AND m.duplicate_of IS NULL" : "";
  const day = bound === "first" ? "MIN" : "MAX";
  const having: string[] = [];
  const params: unknown[] = [accountId];
  if (bounds.from) having.push(`${day}(substr(m.timestamp, 1, 10)) >= ?`);
  if (bounds.to) having.push(`${day}(substr(m.timestamp, 1, 10)) < ?`);
  if (bounds.from) params.push(bounds.from);
  if (bounds.to) params.push(bounds.to);
  if (having.length === 0) return [];

  const rows = db
    .prepare(
      `SELECT cp.contact_id AS contact_id
       FROM contact_handles cp
       JOIN conversations cv
         ON cv.chat_identifier = cp.handle
        AND cv.conversation_type = 'individual'
        AND cv.account_id = cp.account_id
       JOIN messages m ON m.conversation_id = cv.id
       WHERE cp.account_id = ?${hideDupes}
       GROUP BY cp.contact_id
       HAVING ${having.join(" AND ")}`,
    )
    .all(...params) as Array<{ contact_id: number }>;
  return rows.map((row) => row.contact_id);
}

/** Restrict conversations to those involving one of these contacts. */
function involvesContactsSql(contactIds: number[]): string {
  const ids = contactIds.filter((id) => Number.isInteger(id));
  if (ids.length === 0) return "1=0";
  return `EXISTS (
    SELECT 1 FROM contact_handles ch
    WHERE ch.account_id = c.account_id
      AND ch.contact_id IN (${ids.join(",")})
      AND ${CONTACT_LINKED_SQL}
  )`;
}

type SearchFilters = {
  fts: string | null;
  whereSql: string;
  params: unknown[];
  dedupe: string;
  sourceFilter: string | null;
  /**
   * Contact id sets from contact-level filters (`within:`, first/last contact).
   * Conversations only need to involve one such contact, but contact-grouped
   * results must also drop the contacts that failed the filter themselves.
   */
  contactScopes: number[][];
};

/** Shared WHERE clause for both the flat and contact-grouped queries. */
function buildSearchFilters(
  parsed: ParsedSearchQuery,
  sourceOverride?: string | null,
): SearchFilters {
  const accountId = currentAccountId();
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

  const sourceFilter = sourceOverride ?? parsed.source;
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

  const contactScopes: number[][] = [];
  const scopeToContacts = (contactIds: number[]) => {
    contactScopes.push(contactIds);
    where.push(involvesContactsSql(contactIds));
  };

  // Scoped to a label's contacts whether or not they are marked inactive.
  if (parsed.within) {
    scopeToContacts(listLabelMemberContactIds(parsed.within));
  }

  const db = getDb();
  if (hasDateBounds(parsed.firstContact)) {
    scopeToContacts(contactIdsWithinDayBounds(db, "first", parsed.firstContact));
  }
  if (hasDateBounds(parsed.lastContact)) {
    scopeToContacts(contactIdsWithinDayBounds(db, "last", parsed.lastContact));
  }

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

  where.push("1=1"); // keep join shape stable

  return {
    fts,
    whereSql: where.join(" AND "),
    params,
    dedupe: combinedDedupeSql(sourceFilter, "m"),
    sourceFilter: sourceFilter ?? null,
    contactScopes,
  };
}

/** Create FTS if the readonly connection opened before the migration ran. */
function ensureFtsReady(): void {
  const writeDb = openWritable();
  writeDb.close();
  resetDb();
}

export function searchVault(
  rawQuery: string,
  opts: { limit?: number; offset?: number; source?: string | null } = {},
): SearchResult {
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

  ensureFtsReady();

  const db = getDb();
  const { fts, whereSql, params, dedupe } = buildSearchFilters(
    parsed,
    opts.source,
  );
  const countRow = db
    .prepare(
      `SELECT COUNT(*) AS n FROM (
         SELECT c.id
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE ${whereSql}${dedupe}
         GROUP BY c.id
       )`,
    )
    .get(...params) as { n: number };

  const convRows = db
    .prepare(
      `SELECT ${CONVERSATION_HIT_COLUMNS}
       FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE ${whereSql}${dedupe}
       GROUP BY c.id
       ORDER BY MAX(m.timestamp) DESC
       LIMIT ? OFFSET ?`,
    )
    .all(...params, limit, offset) as ConversationRow[];

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
       )
       ORDER BY contact_id`,
    )
    .all(...params) as Array<{ contact_id: number }>;
  const contactIds = contactRows.map((row) => row.contact_id);

  const hits = buildHits(db, convRows, parsed, fts);

  return {
    query: rawQuery,
    parsed,
    totalConversations: countRow.n,
    contactIds,
    hits,
  };
}

type ConversationRow = {
  conversation_id: number;
  conversation_type: string;
  group_title: string | null;
  chat_identifier: string;
  contact_id: number | null;
  match_count: number;
  date_start: string | null;
  date_end: string | null;
  sample_message_id: number;
};

const CONVERSATION_HIT_COLUMNS = `
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
  MIN(m.id) AS sample_message_id`;

function buildHits(
  db: Database.Database,
  convRows: ConversationRow[],
  parsed: ParsedSearchQuery,
  fts: string | null,
): SearchConversationHit[] {
  const highlightTerms = [
    ...parsed.terms,
    ...parsed.phrases,
    ...(parsed.subject ? [parsed.subject] : []),
  ];

  return convRows.map((row) => {
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
}

export type ConversationMatch = {
  id: number;
  timestamp: string;
};

export type ConversationMatchResult = {
  query: string;
  parsed: ParsedSearchQuery;
  conversationIds: number[];
  matches: ConversationMatch[];
};

/**
 * Every matching message in the given conversations, oldest first, so an
 * in-thread find bar can step through them. A direct thread may span several
 * conversation rows (one per source). Applies the same filters as
 * {@link searchVault}.
 */
export function searchConversationMatches(
  rawQuery: string,
  conversationIds: number[],
  opts: { source?: string | null } = {},
): ConversationMatchResult {
  const parsed = parseSearchQuery(rawQuery);
  const ids = conversationIds.filter((id) => Number.isInteger(id) && id > 0);
  if (ids.length === 0 || (!hasSearchCriteria(parsed) && !rawQuery.trim())) {
    return { query: rawQuery, parsed, conversationIds: ids, matches: [] };
  }

  ensureFtsReady();

  const db = getDb();
  const { whereSql, params, dedupe } = buildSearchFilters(parsed, opts.source);
  const matches = db
    .prepare(
      `SELECT DISTINCT m.id AS id, m.timestamp AS timestamp
       FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE ${whereSql}${dedupe} AND c.id IN (${ids.map(() => "?").join(",")})
       ORDER BY m.timestamp ASC, m.id ASC`,
    )
    .all(...params, ...ids) as ConversationMatch[];

  return { query: rawQuery, parsed, conversationIds: ids, matches };
}

/** Contacts allowed to head a result group; null when unfiltered. */
function intersectContactScopes(scopes: number[][]): Set<number> | null {
  const [first, ...rest] = scopes;
  if (!first) return null;
  let allowed = new Set<number>(first);
  for (const scope of rest) {
    const scopeSet = new Set<number>(scope);
    allowed = new Set([...allowed].filter((id) => scopeSet.has(id)));
  }
  return allowed;
}

/** Contacts holding a vault-owner handle, so search can skip them. */
function ownerContactIds(db: Database.Database): Set<number> {
  const accountId = currentAccountId();
  const rows = db
    .prepare(
      `SELECT handle, contact_id FROM contact_handles WHERE account_id = ?`,
    )
    .all(accountId) as Array<{ handle: string; contact_id: number }>;
  const isOwner = ownerHandleMatcher();
  const ids = new Set<number>();
  for (const row of rows) {
    if (isOwner(row.handle)) ids.add(row.contact_id);
  }
  return ids;
}

function countMatches(
  actual: number,
  comparison: ParsedSearchQuery["messageCount"],
): boolean {
  if (!comparison) return true;
  switch (comparison.comparator) {
    case "=":
      return actual === comparison.value;
    case ">":
      return actual > comparison.value;
    case ">=":
      return actual >= comparison.value;
    case "<":
      return actual < comparison.value;
    case "<=":
      return actual <= comparison.value;
  }
}

function dateMatches(value: string | null, bounds: DateBounds): boolean {
  if (!hasDateBounds(bounds)) return true;
  if (!value) return false;
  const day = value.slice(0, 10);
  return (!bounds.from || day >= bounds.from) && (!bounds.to || day < bounds.to);
}

/**
 * Contact-primary search. Unlike message search, this includes contacts with no
 * messages and applies message/group counts to the contact as a whole.
 */
export function searchVaultContacts(
  rawQuery: string,
  opts: { limit?: number; offset?: number } = {},
): SearchResult {
  const parsed = parseSearchQuery(rawQuery);
  const limit = Math.min(Math.max(1, opts.limit ?? DEFAULT_LIMIT), MAX_LIMIT);
  const offset = Math.max(0, opts.offset ?? 0);
  const empty: SearchResult = {
    query: rawQuery,
    parsed,
    totalConversations: 0,
    contactIds: [],
    hits: [],
    contacts: [],
    totalContacts: 0,
  };
  if (parsed.mode !== "contacts") return empty;

  const db = getDb();
  const accountId = currentAccountId();
  const candidateRows = db
    .prepare(
      `SELECT c.id
       FROM contacts c
       WHERE c.account_id = ?
         AND NOT EXISTS (
           SELECT 1 FROM trashed_contacts tc
           WHERE tc.account_id = c.account_id AND tc.contact_id = c.id
         )
       ORDER BY c.id`,
    )
    .all(accountId) as Array<{ id: number }>;
  const skipIds = ownerContactIds(db);
  const candidateIds = candidateRows
    .map((row) => row.id)
    .filter((id) => !skipIds.has(id));
  const allItems = listContactsByIds(candidateIds);

  const handleRows = db
    .prepare(
      `SELECT contact_id, handle
       FROM contact_handles
       WHERE account_id = ?
       ORDER BY contact_id, handle`,
    )
    .all(accountId) as Array<{ contact_id: number; handle: string }>;
  const handlesByContact = new Map<number, string[]>();
  for (const row of handleRows) {
    const handles = handlesByContact.get(row.contact_id);
    if (handles) handles.push(row.handle);
    else handlesByContact.set(row.contact_id, [row.handle]);
  }
  const withinIds = parsed.within
    ? new Set(listLabelMemberContactIds(parsed.within))
    : null;
  const handleNeedle = parsed.handle?.trim().toLocaleLowerCase() ?? "";
  const matchingItems = allItems
    .filter((contact) => {
      if (withinIds && !withinIds.has(contact.id)) return false;
      if (
        handleNeedle &&
        ![
          contact.displayName,
          contact.preferredHandle ?? "",
          ...(handlesByContact.get(contact.id) ?? []),
        ].some((value) => value.toLocaleLowerCase().includes(handleNeedle))
      ) {
        return false;
      }
      return (
        dateMatches(contact.dateStart, parsed.firstContact) &&
        dateMatches(contact.dateEnd, parsed.lastContact) &&
        countMatches(contact.groupMessageCount, parsed.groupCount) &&
        countMatches(contact.messageCount, parsed.messageCount)
      );
    })
    .sort(
      (a, b) =>
        (b.dateEnd ?? "").localeCompare(a.dateEnd ?? "") ||
        a.sortLast.localeCompare(b.sortLast, undefined, {
          sensitivity: "base",
        }) ||
        a.id - b.id,
    );
  const pageItems = matchingItems.slice(offset, offset + limit);
  const pageIds = pageItems.map((contact) => contact.id);
  if (pageIds.length === 0) {
    return {
      ...empty,
      contactIds: matchingItems.map((contact) => contact.id),
      totalContacts: matchingItems.length,
    };
  }

  const placeholders = pageIds.map(() => "?").join(",");
  const pairRows = db
    .prepare(
      `SELECT DISTINCT ch.contact_id, c.id AS conversation_id
       FROM contact_handles ch
       JOIN conversations c
         ON c.account_id = ch.account_id
        AND (
          (c.conversation_type = 'individual' AND c.chat_identifier = ch.handle)
          OR (
            c.conversation_type = 'group'
            AND EXISTS (
              SELECT 1 FROM participants p
              WHERE p.conversation_id = c.id AND p.handle = ch.handle
            )
          )
        )
       WHERE ch.account_id = ?
         AND ch.contact_id IN (${placeholders})
         AND NOT EXISTS (
           SELECT 1 FROM trashed_conversations tc
           WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
         )
         AND NOT EXISTS (
           SELECT 1 FROM trashed_handles th
           WHERE th.account_id = c.account_id AND th.handle = c.chat_identifier
         )`,
    )
    .all(accountId, ...pageIds) as Array<{
    contact_id: number;
    conversation_id: number;
  }>;
  const conversationIds = [
    ...new Set(pairRows.map((row) => row.conversation_id)),
  ];
  const convRows =
    conversationIds.length === 0
      ? []
      : (db
          .prepare(
            `SELECT ${CONVERSATION_HIT_COLUMNS}
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             WHERE c.account_id = ?
               AND c.id IN (${conversationIds.map(() => "?").join(",")})
               ${combinedDedupeSql(null, "m")}
             GROUP BY c.id
             ORDER BY MAX(m.timestamp) DESC`,
          )
          .all(accountId, ...conversationIds) as ConversationRow[]);
  const hits = buildHits(db, convRows, parsed, null);
  const hitsById = new Map(hits.map((hit) => [hit.conversationId, hit]));
  const hitIdsByContact = new Map<number, number[]>();
  for (const row of pairRows) {
    const ids = hitIdsByContact.get(row.contact_id);
    if (ids) ids.push(row.conversation_id);
    else hitIdsByContact.set(row.contact_id, [row.conversation_id]);
  }
  const contacts: SearchContactHit[] = pageItems.map((contact) => {
    const contactHits = (hitIdsByContact.get(contact.id) ?? [])
      .map((id) => hitsById.get(id))
      .filter((hit): hit is SearchConversationHit => hit != null)
      .sort((a, b) => (b.dateEnd ?? "").localeCompare(a.dateEnd ?? ""));
    return {
      contact,
      hits: contactHits,
      matchCount: contactHits.reduce((sum, hit) => sum + hit.matchCount, 0),
    };
  });

  return {
    query: rawQuery,
    parsed,
    totalConversations: conversationIds.length,
    contactIds: matchingItems.map((contact) => contact.id),
    hits,
    contacts,
    totalContacts: matchingItems.length,
  };
}

/**
 * Same filters as {@link searchVault}, but grouped under each contact who takes
 * part in a matching conversation. A group conversation appears under every
 * participating contact. Contacts are paginated, not conversations.
 */
export function searchVaultByContact(
  rawQuery: string,
  opts: { limit?: number; offset?: number; source?: string | null } = {},
): SearchResult {
  const parsed = parseSearchQuery(rawQuery);
  const limit = Math.min(Math.max(1, opts.limit ?? DEFAULT_LIMIT), MAX_LIMIT);
  const offset = Math.max(0, opts.offset ?? 0);

  const empty: SearchResult = {
    query: rawQuery,
    parsed,
    totalConversations: 0,
    contactIds: [],
    hits: [],
    contacts: [],
    totalContacts: 0,
  };
  if (!hasSearchCriteria(parsed) && !rawQuery.trim()) return empty;

  ensureFtsReady();

  const db = getDb();
  const { fts, whereSql, params, dedupe, contactScopes } = buildSearchFilters(
    parsed,
    opts.source,
  );

  // Every (contact, conversation) pair that matched, newest conversation first.
  // Conversations are collapsed before fanning out to contacts, so the handle
  // lookups run per conversation rather than per matching message.
  const accountId = currentAccountId();
  const pairRows = db
    .prepare(
      `WITH matched AS (
         SELECT c.id AS conversation_id,
                c.chat_identifier AS chat_identifier,
                MAX(m.timestamp) AS last_ts
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE ${whereSql}${dedupe}
         GROUP BY c.id
       )
       SELECT ch.contact_id AS contact_id, mt.conversation_id, mt.last_ts
       FROM matched mt
       JOIN contact_handles ch
         ON ch.account_id = ? AND ch.handle = mt.chat_identifier
       UNION
       SELECT ch.contact_id AS contact_id, mt.conversation_id, mt.last_ts
       FROM matched mt
       JOIN participants p ON p.conversation_id = mt.conversation_id
       JOIN contact_handles ch
         ON ch.account_id = ? AND ch.handle = p.handle
       ORDER BY last_ts DESC`,
    )
    .all(...params, accountId, accountId) as Array<{
    contact_id: number;
    conversation_id: number;
    last_ts: string | null;
  }>;

  if (pairRows.length === 0) return empty;

  // Older vaults may have a contact holding an owner handle; the owner is a
  // participant in their own groups and should not be listed as a match.
  const skipContactIds = ownerContactIds(db);
  // A contact-level filter also decides who may head a result group: sharing a
  // group chat with a match is not the same as being one.
  const allowedContactIds = intersectContactScopes(contactScopes);

  const conversationIdsByContact = new Map<number, number[]>();
  for (const row of pairRows) {
    if (skipContactIds.has(row.contact_id)) continue;
    if (allowedContactIds && !allowedContactIds.has(row.contact_id)) continue;
    const list = conversationIdsByContact.get(row.contact_id);
    if (list) list.push(row.conversation_id);
    else conversationIdsByContact.set(row.contact_id, [row.conversation_id]);
  }
  if (conversationIdsByContact.size === 0) return empty;

  // Contacts ordered by their most recent matching message.
  const orderedContactIds = [...conversationIdsByContact.keys()];
  const pageContactIds = orderedContactIds.slice(offset, offset + limit);
  const conversationIds = [
    ...new Set(pageContactIds.flatMap((id) => conversationIdsByContact.get(id) ?? [])),
  ];

  const convRows =
    conversationIds.length === 0
      ? []
      : (db
          .prepare(
            `SELECT ${CONVERSATION_HIT_COLUMNS}
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             WHERE ${whereSql}${dedupe}
               AND c.id IN (${conversationIds.map(() => "?").join(",")})
             GROUP BY c.id
             ORDER BY MAX(m.timestamp) DESC`,
          )
          .all(...params, ...conversationIds) as ConversationRow[]);

  const hits = buildHits(db, convRows, parsed, fts);
  const hitsById = new Map(hits.map((hit) => [hit.conversationId, hit]));

  const contactItems = listContactsByIds(pageContactIds);
  const contacts: SearchContactHit[] = [];
  for (const contact of contactItems) {
    const contactHits = (conversationIdsByContact.get(contact.id) ?? [])
      .map((id) => hitsById.get(id))
      .filter((hit): hit is SearchConversationHit => hit != null);
    if (contactHits.length === 0) continue;
    contacts.push({
      contact,
      hits: contactHits,
      matchCount: contactHits.reduce((n, hit) => n + hit.matchCount, 0),
    });
  }

  return {
    query: rawQuery,
    parsed,
    totalConversations: new Set(pairRows.map((r) => r.conversation_id)).size,
    contactIds: orderedContactIds,
    hits,
    contacts,
    totalContacts: orderedContactIds.length,
  };
}
