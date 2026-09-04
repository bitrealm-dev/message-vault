import { useState } from "react";
import type { Message } from "../../lib/types";
import { listConversationMessages } from "../../lib/vaultApi";
import { keys } from "../../lib/vaultKeys";
import { useVaultQuery } from "../../lib/vaultQuery";

/** Page size for full-conversation browsing. */
export const PAGE_SIZE = 50;
/** Largest page the messages API will return in one request. */
const YEAR_FETCH_LIMIT = 500;

/** One page's worth of conversation messages, as the hook hands them to a screen. */
type MessagesResult = { items: Message[]; total: number };

/** Calendar years covered by a conversation's first and last message dates. */
export function conversationYears(
  startIso: string | null | undefined,
  endIso: string | null | undefined,
): number[] {
  if (!startIso || !endIso) return [];
  const startYear = new Date(startIso).getFullYear();
  const endYear = new Date(endIso).getFullYear();
  if (!Number.isFinite(startYear) || !Number.isFinite(endYear) || endYear < startYear) {
    return [];
  }
  const years: number[] = [];
  for (let y = startYear; y <= endYear; y++) years.push(y);
  return years;
}

/** Short label for a backup source shown in the message footer. */
export function displaySourceLabel(source: string): string {
  const token = source.trim().toLowerCase();
  if (token === "sms-backup-restore") return "SMS/MMS";
  if (token === "whatsapp") return "WhatsApp";
  return source.trim() || "unknown";
}

/**
 * Footer count line. A year filter loads that year in full, so it always shows
 * the whole range. Unfiltered browsing shows the current page window.
 */
export function buildFooterLabel(activeYear: number | null, total: number, offset: number): string {
  if (activeYear !== null) {
    if (total === 0) return `${activeYear}: 0 of 0`;
    return `${activeYear}: 1–${total} of ${total}`;
  }
  if (total === 0) return "Messages 0 of 0";
  return `Messages ${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${total}`;
}

/**
 * Load every message in one calendar year, paging until the server has no
 * more. Runs inside a query function so the whole year lands in one cache
 * entry — see issue #323: a year is not paged lazily.
 */
async function fetchYear(
  conversationId: number,
  year: number,
  signal: AbortSignal,
): Promise<MessagesResult> {
  const items: Message[] = [];
  let offset = 0;
  let total = 0;
  while (true) {
    const page = await listConversationMessages(
      conversationId,
      { offset, limit: YEAR_FETCH_LIMIT, year },
      { signal },
    );
    total = page.total;
    items.push(...page.items);
    offset += page.items.length;
    if (page.items.length === 0 || offset >= total) break;
  }
  return { items, total };
}

/** Load messages for one conversation, either a page at a time or a whole year. */
export function useConversationMessages(conversationId: number) {
  /** `offset`, `activeYear`, `findTerm` and `activeMatch` are view state, not
   * server state — a screen's choice of what to look at, not anything the
   * vault owns. The messages themselves come from `useVaultQuery` below. */
  const [offset, setOffset] = useState(0);
  /** `null` = all years (paged). Otherwise load every message in that calendar year. */
  const [activeYear, setActiveYear] = useState<number | null>(null);
  const [findTerm, setFindTerm] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);

  // A new conversation starts back at page one, with no year filter or find
  // term carried over from the last one. Adjusted during render, per React's
  // own guidance for resetting state on a prop change, rather than in an
  // effect that would otherwise run one render late.
  const [renderedConversationId, setRenderedConversationId] = useState(conversationId);
  if (conversationId !== renderedConversationId) {
    setRenderedConversationId(conversationId);
    setOffset(0);
    setActiveYear(null);
    setFindTerm("");
    setActiveMatch(0);
  }

  const keyParams =
    activeYear !== null
      ? { offset: 0, limit: YEAR_FETCH_LIMIT, year: activeYear }
      : { offset, limit: PAGE_SIZE, year: null };

  const query = useVaultQuery<MessagesResult>(
    keys.conversations.messages(conversationId, keyParams),
    (signal) =>
      activeYear !== null
        ? fetchYear(conversationId, activeYear, signal)
        : listConversationMessages(conversationId, { offset, limit: PAGE_SIZE }, { signal }),
  );

  const messages = query.data?.items ?? [];
  const total = query.data?.total ?? 0;

  const fetchConversationPage = (newOffset: number) => {
    setOffset(newOffset);
  };

  const selectAllYears = () => {
    setActiveYear(null);
    setOffset(0);
    setActiveMatch(0);
  };

  const selectYear = (year: number) => {
    if (activeYear === year) {
      selectAllYears();
      return;
    }
    setActiveYear(year);
    setOffset(0);
    setActiveMatch(0);
  };

  return {
    messages,
    total,
    offset,
    activeYear,
    findTerm,
    setFindTerm,
    activeMatch,
    setActiveMatch,
    loading: query.isLoading,
    fetchConversationPage,
    selectAllYears,
    selectYear,
    data: query.data,
    error: query.error,
    isLoading: query.isLoading,
  };
}
