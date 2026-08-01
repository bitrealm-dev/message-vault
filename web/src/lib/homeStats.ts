import { currentAccountId } from "./accountScope";
import { getDb, hasDuplicateOfColumn, hasTrashedContactsTable } from "./dbCore";
import { countContacts, listContacts } from "./contactsRead";
import { countGroupChats } from "./groupChatsRead";
import type { HomeStats } from "./types";

export function homeStats(): HomeStats {
  const accountId = currentAccountId();
  const db = getDb();
  const notTrashed = hasTrashedContactsTable(db)
    ? `WHERE account_id = ? AND NOT EXISTS (
         SELECT 1 FROM trashed_contacts tc
         WHERE tc.contact_id = contacts.id AND tc.account_id = contacts.account_id
       )`
    : "WHERE account_id = ?";

  let messages: number;
  let messageDuplicates: number;
  if (hasDuplicateOfColumn()) {
    const row = db
      .prepare(
        `SELECT
           SUM(CASE WHEN m.duplicate_of IS NULL THEN 1 ELSE 0 END) AS primary_n,
           SUM(CASE WHEN m.duplicate_of IS NOT NULL THEN 1 ELSE 0 END) AS dup_n
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE c.account_id = ?`,
      )
      .get(accountId) as { primary_n: number | null; dup_n: number | null };
    messages = row.primary_n ?? 0;
    messageDuplicates = row.dup_n ?? 0;
  } else {
    messages = (
      db
        .prepare(
          `SELECT COUNT(*) AS n FROM messages m
           JOIN conversations c ON c.id = m.conversation_id
           WHERE c.account_id = ?`,
        )
        .get(accountId) as { n: number }
    ).n;
    messageDuplicates = 0;
  }

  const primaryOnly = hasDuplicateOfColumn()
    ? "AND m.duplicate_of IS NULL"
    : "";
  const activity = db
    .prepare(
      `SELECT
         SUM(CASE WHEN m.is_from_me != 0 THEN 1 ELSE 0 END) AS sent_n,
         SUM(CASE WHEN m.is_from_me = 0 THEN 1 ELSE 0 END) AS received_n,
         COUNT(DISTINCT m.source) AS source_n,
         MIN(substr(m.timestamp, 1, 10)) AS date_start,
         MAX(substr(m.timestamp, 1, 10)) AS date_end
       FROM messages m
       JOIN conversations c ON c.id = m.conversation_id
       WHERE c.account_id = ? ${primaryOnly}`,
    )
    .get(accountId) as {
    sent_n: number | null;
    received_n: number | null;
    source_n: number;
    date_start: string | null;
    date_end: string | null;
  };
  const attachments = (
    db
      .prepare(
        `SELECT COUNT(*) AS n
         FROM attachments a
         JOIN messages m ON m.id = a.message_id
         JOIN conversations c ON c.id = m.conversation_id
         WHERE c.account_id = ? ${primaryOnly}`,
      )
      .get(accountId) as { n: number }
  ).n;
  const recentContacts = listContacts("all")
    .filter(
      (contact): contact is typeof contact & { dateEnd: string } =>
        contact.dateEnd != null,
    )
    .sort(
      (a, b) =>
        b.dateEnd.localeCompare(a.dateEnd) ||
        b.messageCount - a.messageCount,
    )
    .slice(0, 6)
    .map((contact) => ({
      id: contact.id,
      displayName: contact.displayName,
      messageCount: contact.messageCount,
      groupChatCount: contact.groupMessageCount,
      dateEnd: contact.dateEnd,
    }));

  return {
    all: countContacts("all"),
    noMessages: countContacts("no-messages"),
    groupChats: countGroupChats(),
    messages,
    messageDuplicates,
    contacts: (
      db.prepare(`SELECT COUNT(*) AS n FROM contacts ${notTrashed}`).get(accountId) as {
        n: number;
      }
    ).n,
    sentMessages: activity.sent_n ?? 0,
    receivedMessages: activity.received_n ?? 0,
    attachments,
    sources: activity.source_n,
    dateStart: activity.date_start,
    dateEnd: activity.date_end,
    recentContacts,
  };
}
