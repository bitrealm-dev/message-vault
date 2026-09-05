/** Home dashboard counts, assembled from `/v1` list totals and the count route. */
import type { HomeStats } from "@/lib/types";

import { listSources } from "./account";
import { dayOf, qs, vaultJson, type Schemas } from "./client";
import { listContacts } from "./contacts";
import { allConversations } from "./conversations";

type Count = Schemas["ExportCountResponse"];

async function count(q: string): Promise<Count> {
  return vaultJson<Count>(`/v1/export/messages/count${qs({ q })}`);
}

export async function homeStats(): Promise<HomeStats> {
  const [contacts, conversations, sources, all, sent] = await Promise.all([
    listContacts("all"),
    allConversations(),
    listSources(),
    count(""),
    count("from:me"),
  ]);

  let dateStart: string | null = null;
  let dateEnd: string | null = null;
  for (const c of conversations) {
    const start = dayOf(c.date_range_start);
    const end = dayOf(c.date_range_end) || dayOf(c.last_message_at);
    if (start && (!dateStart || start < dateStart)) dateStart = start;
    if (end && (!dateEnd || end > dateEnd)) dateEnd = end;
  }

  const recentContacts = contacts
    .filter((c): c is typeof c & { dateEnd: string } => c.dateEnd != null)
    .sort(
      (a, b) =>
        b.dateEnd.localeCompare(a.dateEnd) || b.messageCount - a.messageCount,
    )
    .slice(0, 6)
    .map((c) => ({
      id: c.id,
      displayName: c.displayName,
      messageCount: c.messageCount,
      groupChatCount: c.groupMessageCount,
      dateEnd: c.dateEnd,
    }));

  return {
    all: contacts.length,
    noMessages: contacts.filter(
      (c) => c.messageCount === 0 && c.groupMessageCount === 0,
    ).length,
    groupChats: conversations.filter((c) => c.is_group).length,
    messages: all.messages,
    // Hidden cross-source duplicates are not reported by the API.
    messageDuplicates: 0,
    contacts: contacts.length,
    sentMessages: sent.messages,
    receivedMessages: Math.max(0, all.messages - sent.messages),
    attachments: all.attachments,
    sources: sources.length,
    dateStart,
    dateEnd,
    recentContacts,
  };
}
