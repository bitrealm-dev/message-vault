import { currentAccountId } from "./accountScope";
import { getDb, hasDuplicateOfColumn, hasTrashedContactsTable } from "./dbCore";
import { countContacts } from "./contactsRead";
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
  };
}
