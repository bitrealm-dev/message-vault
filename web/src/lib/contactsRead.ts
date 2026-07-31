import { currentAccountId } from "./accountScope";
import {
  combinedDedupeSql,
  displayName,
  getDb,
  hasDuplicateOfColumn,
  hasTrashedContactsTable,
  hasTrashedConversationsTable,
  hasTrashedHandlesTable,
  sortFields,
} from "./dbCore";
import { labelSlug } from "./labelSlug";
import { contactGroupChatThreadsForPhones, contactGroupChatThreadsForPhoneSets } from "./groupChatsRead";
import { RESERVED_LABEL_NAMES } from "./reservedLabels";
import type {
  ContactDetail,
  ContactListItem,
  ContactSection,
  GroupChatThread,
  TrashedContactItem,
  TrashedContactMessagesItem,
  YearThread,
} from "./types";

/** Contact labels (GUI "Labels"). Stored in SQLite `contact_labels` / `contact_label_members`. */
export function listLabels(): string[] {
  const accountId = currentAccountId();
  const db = getDb();
  const rows = db
    .prepare(
      `SELECT name FROM contact_labels
       WHERE account_id = ?
       ORDER BY name COLLATE NOCASE`,
    )
    .all(accountId) as Array<{ name: string }>;
  return rows
    .map((r) => r.name)
    .filter((name) => !RESERVED_LABEL_NAMES.has(name.trim().toLowerCase()));
}

/** Contact ids that currently belong to a named label (case-insensitive). */
export function listLabelMemberContactIds(name: string): number[] {
  const accountId = currentAccountId();
  const trimmed = name.trim();
  if (!trimmed) return [];
  const db = getDb();
  const rows = db
    .prepare(
      `SELECT clm.contact_id AS contact_id
       FROM contact_label_members clm
       JOIN contact_labels cl ON cl.id = clm.label_id
       WHERE cl.name = ? COLLATE NOCASE AND cl.account_id = ?
       ORDER BY clm.contact_id`,
    )
    .all(trimmed, accountId) as Array<{ contact_id: number }>;
  return rows.map((r) => r.contact_id);
}

export function labelFromSlug(slug: string): string | null {
  const trimmed = slug.trim();
  if (!trimmed) return null;

  const groups = listLabels();

  // Prefer exact (case-preserving) slug match.
  for (const name of groups) {
    if (labelSlug(name) === trimmed) return name;
  }

  // Fallback for older lowercase-only URLs: first case-insensitive hit.
  const folded = trimmed.toLowerCase();
  for (const name of groups) {
    if (labelSlug(name).toLowerCase() === folded) return name;
  }

  return null;
}

function notTrashedContactSql(alias = "c"): string {
  const db = getDb();
  if (!hasTrashedContactsTable(db)) return "";
  return `AND NOT EXISTS (
    SELECT 1 FROM trashed_contacts tc
    WHERE tc.contact_id = ${alias}.id AND tc.account_id = ${alias}.account_id
  )`;
}

function notTrashedHandleSql(handleExpr: string, accountExpr: string): string {
  const db = getDb();
  if (!hasTrashedHandlesTable(db)) return "";
  return `AND NOT EXISTS (
    SELECT 1 FROM trashed_handles th
    WHERE th.handle = ${handleExpr} AND th.account_id = ${accountExpr}
  )`;
}

/** Contact has visible (non-trashed) 1:1 messages or any group participation. */
function contactHasMessagesSql(): string {
  const trashOnHandle = notTrashedHandleSql("cp.handle", "cp.account_id");
  return `
  EXISTS (
    SELECT 1
    FROM contact_handles cp
    WHERE cp.contact_id = c.id AND cp.account_id = c.account_id
      AND (
        EXISTS (
          SELECT 1
          FROM conversations cv
          JOIN messages m ON m.conversation_id = cv.id
          WHERE cv.conversation_type = 'individual'
            AND cv.chat_identifier = cp.handle
            AND cv.account_id = cp.account_id
            ${trashOnHandle}
        )
        OR EXISTS (
          SELECT 1
          FROM participants p
          JOIN conversations gcv ON gcv.id = p.conversation_id
            AND gcv.conversation_type = 'group'
          JOIN messages m ON m.conversation_id = p.conversation_id
          WHERE p.handle = cp.handle
            AND gcv.account_id = cp.account_id
        )
      )
  )
`;
}

function sectionQueryBody(
  section: ContactSection,
): { fromWhere: string; params: unknown[] } {
  const accountId = currentAccountId();
  const hasMsgs = contactHasMessagesSql();
  const notTrashed = notTrashedContactSql("c");
  if (typeof section === "object" && "label" in section) {
    return {
      fromWhere: `
        FROM contacts c
        JOIN contact_label_members clm ON clm.contact_id = c.id
        JOIN contact_labels cl ON cl.id = clm.label_id AND cl.name = ?
        WHERE c.account_id = ?
          ${notTrashed}
      `,
      params: [section.label, accountId],
    };
  }
  switch (section) {
    case "all":
      return {
        fromWhere: `
          FROM contacts c
          WHERE c.account_id = ?
            ${notTrashed}
        `,
        params: [accountId],
      };
    case "no-messages":
      return {
        fromWhere: `
          FROM contacts c
          WHERE c.account_id = ?
            AND NOT (${hasMsgs})
            ${notTrashed}
        `,
        params: [accountId],
      };
    case "no-label":
      return {
        fromWhere: `
          FROM contacts c
          WHERE c.account_id = ?
            ${notTrashed}
            AND NOT EXISTS (
              SELECT 1 FROM contact_label_members clm WHERE clm.contact_id = c.id
            )
        `,
        params: [accountId],
      };
  }
}

function sectionSql(section: ContactSection): { sql: string; params: unknown[] } {
  const { fromWhere, params } = sectionQueryBody(section);
  return {
    sql: `SELECT DISTINCT c.*
      ${fromWhere}`,
    params,
  };
}

type ContactRow = {
  id: number;
  first_name: string | null;
  last_name: string | null;
  preferred_handle: string | null;
  exclude: number;
};

export function listContacts(section: ContactSection): ContactListItem[] {
  const db = getDb();
  const { sql, params } = sectionSql(section);
  const rows = db.prepare(sql).all(...params) as ContactRow[];
  return contactListItems(rows);
}

/**
 * Contact list items for specific ids, in the given order.
 * Used by search when results are grouped by contact.
 */
export function listContactsByIds(contactIds: number[]): ContactListItem[] {
  if (contactIds.length === 0) return [];
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = contactIds.map(() => "?").join(",");
  const rows = db
    .prepare(
      `SELECT id, first_name, last_name, preferred_handle, exclude
       FROM contacts
       WHERE account_id = ? AND id IN (${placeholders})`,
    )
    .all(accountId, ...contactIds) as ContactRow[];

  const byId = new Map(contactListItems(rows).map((item) => [item.id, item]));
  return contactIds
    .map((id) => byId.get(id))
    .filter((item): item is ContactListItem => item != null);
}

function contactListItems(rows: ContactRow[]): ContactListItem[] {
  const accountId = currentAccountId();
  const db = getDb();

  const groupRows = db
    .prepare(
      `SELECT clm.contact_id AS contact_id, cl.name AS name
       FROM contact_label_members clm
       JOIN contact_labels cl ON cl.id = clm.label_id
       WHERE cl.account_id = ?
       ORDER BY cl.name COLLATE NOCASE`,
    )
    .all(accountId) as Array<{ contact_id: number; name: string }>;
  const groupsByContact = new Map<number, string[]>();
  for (const row of groupRows) {
    const list = groupsByContact.get(row.contact_id);
    if (list) list.push(row.name);
    else groupsByContact.set(row.contact_id, [row.name]);
  }

  const contactIds = rows.map((r) => r.id);
  const messageCounts = contactMessageCountsById(contactIds);
  const groupMessageCounts = contactGroupMessageCountsById(contactIds);
  const dateRanges = contactDateRangesById(contactIds);

  return rows
    .map((row) => {
      const name = displayName(row);
      const sorts = sortFields(row);
      const range = dateRanges.get(row.id);
      return {
        id: row.id,
        displayName: name,
        preferredHandle: row.preferred_handle,
        firstName: row.first_name,
        lastName: row.last_name,
        labels: groupsByContact.get(row.id) ?? [],
        exclude: row.exclude !== 0,
        messageCount: messageCounts.get(row.id) ?? 0,
        groupMessageCount: groupMessageCounts.get(row.id) ?? 0,
        dateStart: range?.start ?? null,
        dateEnd: range?.end ?? null,
        ...sorts,
      };
    })
    .sort(
      (a, b) =>
        a.sortLast.localeCompare(b.sortLast, undefined, { sensitivity: "base" }) ||
        a.sortFirst.localeCompare(b.sortFirst, undefined, { sensitivity: "base" }),
    );
}

export function getContact(id: number): ContactDetail | null {
  const accountId = currentAccountId();
  const db = getDb();
  const row = db
    .prepare(
      `SELECT id, first_name, last_name, exclude, preferred_handle
       FROM contacts WHERE id = ? AND account_id = ?`,
    )
    .get(id, accountId) as
    | {
        id: number;
        first_name: string | null;
        last_name: string | null;
        exclude: number;
        preferred_handle: string | null;
      }
    | undefined;
  if (!row) return null;

  const phones = db
    .prepare(
      `SELECT handle FROM contact_handles WHERE contact_id = ? AND account_id = ? ORDER BY handle`,
    )
    .all(id, accountId) as Array<{ handle: string }>;

  const groups = db
    .prepare(
      `SELECT cl.name FROM contact_label_members clm
       JOIN contact_labels cl ON cl.id = clm.label_id
       WHERE clm.contact_id = ? AND cl.account_id = ?
       ORDER BY cl.name COLLATE NOCASE`,
    )
    .all(id, accountId) as Array<{ name: string }>;

  const phoneList = phones.map((p) => p.handle);
  const dateRange = contactDateRange(phoneList);
  const messageCount = contactMessageSourceCountsForConversations(
    contactIndividualConversationIds(phoneList),
  ).all;
  const groupMessageCount = contactGroupMessageCountsById([id]).get(id) ?? 0;

  const sorts = sortFields(row);
  return {
    id: row.id,
    displayName: displayName(row),
    preferredHandle: row.preferred_handle,
    firstName: row.first_name,
    lastName: row.last_name,
    exclude: row.exclude !== 0,
    labels: groups.map((t) => t.name),
    phones: phoneList,
    dateStart: dateRange?.start ?? null,
    dateEnd: dateRange?.end ?? null,
    messageCount,
    groupMessageCount,
    ...sorts,
  };
}

function contactDateRange(
  phones: string[],
): { start: string; end: string } | null {
  if (!phones.length) return null;
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = phones.map(() => "?").join(",");
  const hideDupes = hasDuplicateOfColumn() ? " AND m.duplicate_of IS NULL" : "";
  const trashFilter = notTrashedHandleSql("c.chat_identifier", "c.account_id");
  const row = db
    .prepare(
      `SELECT MIN(substr(m.timestamp, 1, 10)) AS start, MAX(substr(m.timestamp, 1, 10)) AS end
       FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE c.conversation_type = 'individual'
         AND c.account_id = ?
         AND c.chat_identifier IN (${placeholders})${trashFilter}${hideDupes}`,
    )
    .get(accountId, ...phones) as { start: string | null; end: string | null } | undefined;
  if (!row?.start || !row?.end) return null;
  return { start: row.start, end: row.end };
}

function contactPhones(contactId: number): string[] {
  const accountId = currentAccountId();
  const db = getDb();
  return (
    db
      .prepare(
        `SELECT handle FROM contact_handles WHERE contact_id = ? AND account_id = ?`,
      )
      .all(contactId, accountId) as Array<{ handle: string }>
  ).map((r) => r.handle);
}

function contactIndividualConversationIds(
  phones: string[],
  opts?: { includeTrashed?: boolean },
): number[] {
  if (!phones.length) return [];
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = phones.map(() => "?").join(",");
  const trashFilter = opts?.includeTrashed
    ? ""
    : notTrashedHandleSql("chat_identifier", "account_id");
  return (
    db
      .prepare(
        `SELECT id FROM conversations
         WHERE account_id = ?
           AND conversation_type = 'individual' AND chat_identifier IN (${placeholders})
           ${trashFilter}`,
      )
      .all(accountId, ...phones) as Array<{ id: number }>
  ).map((r) => r.id);
}

function contactConversationIds(
  phones: string[],
  opts?: { includeTrashed?: boolean },
): number[] {
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = phones.map(() => "?").join(",");
  const individual = contactIndividualConversationIds(phones, opts);
  const groups = db
    .prepare(
      `SELECT DISTINCT c.id AS id
       FROM conversations c
       JOIN participants p ON p.conversation_id = c.id
       WHERE c.account_id = ?
         AND c.conversation_type = 'group' AND p.handle IN (${placeholders})`,
    )
    .all(accountId, ...phones) as Array<{ id: number }>;
  const ids = new Set<number>(individual);
  for (const r of groups) ids.add(r.id);
  return [...ids];
}

export type ContactSourceCounts = {
  /** Soft-deduped 1:1 total (Combined view). Group chats are listed separately. */
  all: number;
  /** Per-source 1:1 totals (single-source view; includes soft-hidden copies). */
  bySource: Record<string, number>;
};

export function contactMessageSourceCountsForConversations(
  conversationIds: number[],
): ContactSourceCounts {
  if (!conversationIds.length) {
    return { all: 0, bySource: {} };
  }
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = conversationIds.map(() => "?").join(",");
  const bySource: Record<string, number> = {};
  const sourceRows = db
    .prepare(
      `SELECT m.source, COUNT(*) AS n
       FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE c.account_id = ? AND m.conversation_id IN (${placeholders})
       GROUP BY m.source`,
    )
    .all(accountId, ...conversationIds) as Array<{ source: string; n: number }>;
  for (const r of sourceRows) {
    if (r.source) bySource[r.source] = r.n;
  }

  const hideDupes = hasDuplicateOfColumn()
    ? " AND m.duplicate_of IS NULL"
    : "";
  const allRow = db
    .prepare(
      `SELECT COUNT(*) AS n FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE c.account_id = ? AND m.conversation_id IN (${placeholders})${hideDupes}`,
    )
    .get(accountId, ...conversationIds) as { n: number };
  return { all: allRow.n, bySource };
}

/** Soft-deduped 1:1 message totals for many contacts (Combined view). */
function contactMessageCountsById(
  contactIds: number[],
): Map<number, number> {
  const counts = new Map<number, number>();
  if (!contactIds.length) return counts;
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = contactIds.map(() => "?").join(",");
  const hideDupes = hasDuplicateOfColumn()
    ? " AND m.duplicate_of IS NULL"
    : "";
  const rows = db
    .prepare(
      `SELECT cp.contact_id AS contact_id, COUNT(m.id) AS n
       FROM contact_handles cp
       JOIN conversations c
         ON c.chat_identifier = cp.handle
        AND c.conversation_type = 'individual'
        AND c.account_id = cp.account_id
       JOIN messages m ON m.conversation_id = c.id
       WHERE cp.account_id = ? AND cp.contact_id IN (${placeholders})${hideDupes}
         ${notTrashedHandleSql("cp.handle", "cp.account_id")}
       GROUP BY cp.contact_id`,
    )
    .all(accountId, ...contactIds) as Array<{ contact_id: number; n: number }>;
  for (const r of rows) counts.set(r.contact_id, r.n);
  return counts;
}

/** Distinct group chats each contact participates in (non-trashed). */
function contactGroupMessageCountsById(
  contactIds: number[],
): Map<number, number> {
  const counts = new Map<number, number>();
  if (!contactIds.length) return counts;
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = contactIds.map(() => "?").join(",");
  const trashFilter = hasTrashedConversationsTable(db)
    ? `AND NOT EXISTS (
         SELECT 1 FROM trashed_conversations tc
         WHERE tc.conversation_id = c.id AND tc.account_id = c.account_id
       )`
    : "";
  const rows = db
    .prepare(
      `SELECT cp.contact_id AS contact_id, COUNT(DISTINCT c.id) AS n
       FROM contact_handles cp
       JOIN participants p ON p.handle = cp.handle
       JOIN conversations c
         ON c.id = p.conversation_id
        AND c.conversation_type = 'group'
        AND c.account_id = cp.account_id
       WHERE cp.account_id = ? AND cp.contact_id IN (${placeholders})
         ${notTrashedHandleSql("cp.handle", "cp.account_id")}
         ${trashFilter}
       GROUP BY cp.contact_id`,
    )
    .all(accountId, ...contactIds) as Array<{ contact_id: number; n: number }>;
  for (const r of rows) counts.set(r.contact_id, r.n);
  return counts;
}

/** 1:1 message date ranges for many contacts (non-trashed, soft-deduped). */
function contactDateRangesById(
  contactIds: number[],
): Map<number, { start: string; end: string }> {
  const ranges = new Map<number, { start: string; end: string }>();
  if (!contactIds.length) return ranges;
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = contactIds.map(() => "?").join(",");
  const hideDupes = hasDuplicateOfColumn()
    ? " AND m.duplicate_of IS NULL"
    : "";
  const rows = db
    .prepare(
      `SELECT cp.contact_id AS contact_id,
              MIN(substr(m.timestamp, 1, 10)) AS start,
              MAX(substr(m.timestamp, 1, 10)) AS end
       FROM contact_handles cp
       JOIN conversations c
         ON c.chat_identifier = cp.handle
        AND c.conversation_type = 'individual'
        AND c.account_id = cp.account_id
       JOIN messages m ON m.conversation_id = c.id
       WHERE cp.account_id = ? AND cp.contact_id IN (${placeholders})${hideDupes}
         ${notTrashedHandleSql("cp.handle", "cp.account_id")}
       GROUP BY cp.contact_id`,
    )
    .all(accountId, ...contactIds) as Array<{
    contact_id: number;
    start: string | null;
    end: string | null;
  }>;
  for (const r of rows) {
    if (r.start && r.end) ranges.set(r.contact_id, { start: r.start, end: r.end });
  }
  return ranges;
}

/** One contact open: yearly + groups + available sources with shared phone/conv lookups. */
export function contactThreadsBundle(
  contactId: number,
  source?: string | null,
  opts?: { includeTrashed?: boolean },
): {
  yearly: YearThread[];
  groupChats: GroupChatThread[];
  messageSources: string[];
  sourceCounts: ContactSourceCounts;
} {
  const page = loadContactThreadsPage(contactId, source, opts);
  if (!page) {
    return {
      yearly: [],
      groupChats: [],
      messageSources: [],
      sourceCounts: { all: 0, bySource: {} },
    };
  }
  return {
    yearly: page.yearly,
    groupChats: page.groupChats,
    messageSources: page.messageSources,
    sourceCounts: page.sourceCounts,
  };
}

/**
 * Contact detail + thread metadata in one pass.
 * Phones, conversation IDs, and source counts are fetched once and reused.
 */
export function loadContactThreadsPage(
  contactId: number,
  source?: string | null,
  opts?: { includeTrashed?: boolean },
): {
  contact: ContactDetail;
  yearly: YearThread[];
  groupChats: GroupChatThread[];
  messageSources: string[];
  sourceCounts: ContactSourceCounts;
} | null {
  const accountId = currentAccountId();
  const db = getDb();
  const row = db
    .prepare(
      `SELECT id, first_name, last_name, exclude, preferred_handle
       FROM contacts WHERE id = ? AND account_id = ?`,
    )
    .get(contactId, accountId) as
    | {
        id: number;
        first_name: string | null;
        last_name: string | null;
        exclude: number;
        preferred_handle: string | null;
      }
    | undefined;
  if (!row) return null;

  const phones = (
    db
      .prepare(
        `SELECT handle FROM contact_handles WHERE contact_id = ? AND account_id = ? ORDER BY handle`,
      )
      .all(contactId, accountId) as Array<{ handle: string }>
  ).map((p) => p.handle);

  const labels = (
    db
      .prepare(
        `SELECT cl.name FROM contact_label_members clm
         JOIN contact_labels cl ON cl.id = clm.label_id
         WHERE clm.contact_id = ? AND cl.account_id = ?
         ORDER BY cl.name COLLATE NOCASE`,
      )
      .all(contactId, accountId) as Array<{ name: string }>
  ).map((t) => t.name);

  const dateRange = contactDateRange(phones);
  const individualIds = contactIndividualConversationIds(phones, opts);
  const sourceCounts =
    contactMessageSourceCountsForConversations(individualIds);
  const allConvIds = phones.length
    ? contactConversationIds(phones, opts)
    : [];
  // Enable sources that appear in 1:1 or groups so group-only archives stay selectable.
  const anySourceCounts =
    allConvIds.length === individualIds.length
      ? sourceCounts
      : contactMessageSourceCountsForConversations(allConvIds);
  const groupMessageCount =
    contactGroupMessageCountsById([contactId]).get(contactId) ?? 0;
  const sorts = sortFields(row);

  return {
    contact: {
      id: row.id,
      displayName: displayName(row),
      preferredHandle: row.preferred_handle,
      firstName: row.first_name,
      lastName: row.last_name,
      exclude: row.exclude !== 0,
      labels,
      phones,
      dateStart: dateRange?.start ?? null,
      dateEnd: dateRange?.end ?? null,
      messageCount: sourceCounts.all,
      groupMessageCount,
      ...sorts,
    },
    yearly: phones.length
      ? contactYearlyThreadsForPhones(phones, source, opts)
      : [],
    groupChats: phones.length
      ? contactGroupChatThreadsForPhones(phones, source)
      : [],
    messageSources: Object.keys(anySourceCounts.bySource).sort(),
    sourceCounts,
  };
}

/** Group chats that include every listed contact (extra participants allowed). */
export function groupChatsContainingContacts(
  contactIds: number[],
  source?: string | null,
): GroupChatThread[] {
  const uniqueIds = [...new Set(contactIds.filter((id) => Number.isFinite(id)))];
  if (!uniqueIds.length) return [];
  const phoneSets = uniqueIds.map((id) => contactPhones(id));
  // Any contact without handles cannot appear as a participant.
  if (phoneSets.some((phones) => phones.length === 0)) return [];
  return contactGroupChatThreadsForPhoneSets(phoneSets, source);
}

export function contactYearlyThreadsForPhones(
  phones: string[],
  source?: string | null,
  opts?: { includeTrashed?: boolean },
): YearThread[] {
  if (!phones.length) return [];
  const accountId = currentAccountId();
  const db = getDb();
  const placeholders = phones.map(() => "?").join(",");
  const sourceSql = source ? " AND m.source = ?" : "";
  const params: Array<string | number> = [accountId, ...phones];
  if (source) params.push(source);
  const trashFilter = opts?.includeTrashed
    ? ""
    : notTrashedHandleSql("c.chat_identifier", "c.account_id");
  const rows = db
    .prepare(
      `SELECT CAST(substr(m.timestamp, 1, 4) AS INTEGER) AS year,
              COUNT(DISTINCT m.id) AS message_count,
              COUNT(a.id) AS attachment_count,
              MIN(substr(m.timestamp, 1, 10)) AS date_start,
              MAX(substr(m.timestamp, 1, 10)) AS date_end,
              GROUP_CONCAT(DISTINCT c.id) AS conversation_ids
       FROM conversations c
       JOIN messages m ON m.conversation_id = c.id
       LEFT JOIN attachments a ON a.message_id = m.id
       WHERE c.account_id = ?
         AND c.conversation_type = 'individual'
         AND c.chat_identifier IN (${placeholders})${sourceSql}${combinedDedupeSql(source, "m")}
         ${trashFilter}
       GROUP BY year
       ORDER BY year DESC`,
    )
    .all(...params) as Array<{
    year: number;
    message_count: number;
    attachment_count: number;
    date_start: string;
    date_end: string;
    conversation_ids: string;
  }>;

  return rows.map((r) => ({
    year: r.year,
    messageCount: r.message_count,
    attachmentCount: r.attachment_count,
    dateStart: r.date_start,
    dateEnd: r.date_end,
    conversationIds: r.conversation_ids
      .split(",")
      .map((id) => Number(id))
      .filter((id) => Number.isFinite(id)),
  }));
}


export function countContacts(section: ContactSection): number {
  const db = getDb();
  const { fromWhere, params } = sectionQueryBody(section);
  const row = db
    .prepare(
      `SELECT COUNT(DISTINCT c.id) AS n
       ${fromWhere}`,
    )
    .get(...params) as { n: number };
  return row.n;
}



/** Contacts soft-trashed with their 1:1 messages. */
export function listTrashedContacts(): TrashedContactItem[] {
  const accountId = currentAccountId();
  const db = getDb();
  if (!hasTrashedContactsTable(db)) return [];
  const hideDupes = hasDuplicateOfColumn() ? " AND m.duplicate_of IS NULL" : "";
  const rows = db
    .prepare(
      `SELECT c.id AS id,
              c.first_name AS first_name,
              c.last_name AS last_name,
              c.preferred_handle AS preferred_handle,
              tc.trashed_at AS trashed_at,
              (SELECT COUNT(*) FROM contact_handles cp
               WHERE cp.contact_id = c.id AND cp.account_id = c.account_id) AS handle_count,
              (
                SELECT COUNT(m.id)
                FROM contact_handles cp
                JOIN conversations cv
                  ON cv.chat_identifier = cp.handle
                 AND cv.conversation_type = 'individual'
                 AND cv.account_id = cp.account_id
                JOIN messages m ON m.conversation_id = cv.id
                WHERE cp.contact_id = c.id AND cp.account_id = c.account_id${hideDupes}
              ) AS message_count
       FROM contacts c
       JOIN trashed_contacts tc ON tc.contact_id = c.id AND tc.account_id = c.account_id
       WHERE c.account_id = ?
       ORDER BY tc.trashed_at DESC, c.last_name COLLATE NOCASE, c.first_name COLLATE NOCASE`,
    )
    .all(accountId) as Array<{
    id: number;
    first_name: string | null;
    last_name: string | null;
    preferred_handle: string | null;
    trashed_at: string;
    handle_count: number;
    message_count: number;
  }>;

  return rows.map((row) => {
    const name = displayName(row);
    const sorts = sortFields(row);
    let preferred = row.preferred_handle;
    if (!preferred) {
      const first = db
        .prepare(
          `SELECT handle FROM contact_handles
           WHERE contact_id = ? AND account_id = ? ORDER BY handle LIMIT 1`,
        )
        .get(row.id, accountId) as { handle: string } | undefined;
      preferred = first?.handle ?? null;
    }
    return {
      kind: "contact" as const,
      contactId: row.id,
      displayName: name,
      preferredHandle: preferred,
      handleCount: row.handle_count,
      messageCount: row.message_count,
      sortKey: `${sorts.sortLast}\0${sorts.sortFirst}`,
      letter: sorts.letter,
      sortFirst: sorts.sortFirst,
      sortLast: sorts.sortLast,
      firstName: row.first_name,
      lastName: row.last_name,
      trashedAt: row.trashed_at,
    };
  });
}

/**
 * Trashed 1:1 handles that still belong to a live (non-trashed) contact —
 * "delete messages only".
 */
export function listTrashedContactMessages(): TrashedContactMessagesItem[] {
  const accountId = currentAccountId();
  const db = getDb();
  if (!hasTrashedHandlesTable(db)) return [];
  const hideDupes = hasDuplicateOfColumn() ? " AND m.duplicate_of IS NULL" : "";
  const notTrashedContact = hasTrashedContactsTable(db)
    ? `AND NOT EXISTS (
         SELECT 1 FROM trashed_contacts tc
         WHERE tc.contact_id = cp.contact_id AND tc.account_id = cp.account_id
       )`
    : "";
  const rows = db
    .prepare(
      `SELECT cp.contact_id AS contact_id,
              cp.handle AS handle,
              c.first_name AS first_name,
              c.last_name AS last_name,
              c.preferred_handle AS preferred_handle,
              MAX(th.trashed_at) AS trashed_at,
              COUNT(m.id) AS message_count
       FROM trashed_handles th
       JOIN contact_handles cp ON cp.handle = th.handle AND cp.account_id = th.account_id
       JOIN contacts c ON c.id = cp.contact_id AND c.account_id = cp.account_id
       JOIN conversations cv
         ON cv.chat_identifier = cp.handle
        AND cv.conversation_type = 'individual'
        AND cv.account_id = cp.account_id
       JOIN messages m ON m.conversation_id = cv.id
       WHERE th.account_id = ? ${notTrashedContact}${hideDupes}
       GROUP BY cp.contact_id, cp.handle, c.first_name, c.last_name, c.preferred_handle
       HAVING message_count > 0
       ORDER BY trashed_at DESC, cp.handle COLLATE NOCASE`,
    )
    .all(accountId) as Array<{
    contact_id: number;
    handle: string;
    first_name: string | null;
    last_name: string | null;
    preferred_handle: string | null;
    trashed_at: string;
    message_count: number;
  }>;

  return rows.map((row) => {
    const name = displayName(row);
    const sorts = sortFields(row);
    return {
      kind: "messages_only" as const,
      contactId: row.contact_id,
      handle: row.handle,
      displayName: name,
      messageCount: row.message_count,
      sortKey: `${name}\0${row.handle}`,
      letter: sorts.letter,
      sortFirst: sorts.sortFirst,
      sortLast: sorts.sortLast,
      firstName: row.first_name,
      lastName: row.last_name,
      trashedAt: row.trashed_at,
    };
  });
}
