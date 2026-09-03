import { useCallback, useEffect, useRef, useState } from "react";
import type { Message } from "../../lib/types";
import { exportMessages } from "../../lib/vaultApi";

/** Page size for full-conversation browsing. */
export const PAGE_SIZE = 50;
/** Largest page the messages API will return in one request. */
const YEAR_FETCH_LIMIT = 500;

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

/** Search query that loads every message in one calendar year. */
function yearQuery(conversationId: string, year: number): string {
  return `in:#${conversationId} date:${year}`;
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

/** Load every message matching this search, paging until the server has no more. */
async function fetchAllMessagesForQuery(
  q: string,
  signal: AbortSignal,
): Promise<{ messages: Message[]; total: number }> {
  const collected: Message[] = [];
  let offset = 0;
  let total = 0;
  while (true) {
    const page = await exportMessages({ q, offset, limit: YEAR_FETCH_LIMIT }, { signal });
    total = page.total;
    collected.push(...page.items);
    offset += page.items.length;
    if (page.items.length === 0 || offset >= total) break;
  }
  return { messages: collected, total };
}

/** True for the rejection `fetch` raises when its signal is aborted. */
function isAbortError(err: unknown): boolean {
  return err instanceof DOMException && err.name === "AbortError";
}

/** Load messages for one conversation, either a page at a time or a whole year. */
export function useConversationMessages(conversationId: string) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  /** `null` = all years (paged). Otherwise load every message in that calendar year. */
  const [activeYear, setActiveYear] = useState<number | null>(null);
  const [findTerm, setFindTerm] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const [loading, setLoading] = useState(false);

  /**
   * The one request whose result may reach state. Switching conversations, or
   * paging again before the previous page lands, aborts the old one — otherwise
   * a slow response can overwrite a newer conversation's messages.
   */
  const inFlightRef = useRef<AbortController | null>(null);
  const startRequest = useCallback(() => {
    inFlightRef.current?.abort();
    const controller = new AbortController();
    inFlightRef.current = controller;
    return controller.signal;
  }, []);

  useEffect(() => () => inFlightRef.current?.abort(), []);

  const fetchConversationPage = useCallback(
    async (newOffset: number) => {
      const signal = startRequest();
      setLoading(true);
      try {
        const q = `in:#${conversationId}`;
        const page = await exportMessages({ q, offset: newOffset, limit: PAGE_SIZE }, { signal });
        if (signal.aborted) return;
        setMessages(page.items);
        setTotal(page.total);
        setOffset(newOffset);
      } catch (err) {
        if (signal.aborted || isAbortError(err)) return;
        setMessages([]);
        setTotal(0);
      } finally {
        // A newer request owns `loading` once this one is superseded.
        if (!signal.aborted) setLoading(false);
      }
    },
    [conversationId, startRequest],
  );

  const fetchYear = useCallback(
    async (year: number) => {
      const signal = startRequest();
      setLoading(true);
      try {
        const { messages: all, total: yearTotal } = await fetchAllMessagesForQuery(
          yearQuery(conversationId, year),
          signal,
        );
        if (signal.aborted) return;
        setMessages(all);
        setTotal(yearTotal);
        setOffset(0);
      } catch (err) {
        if (signal.aborted || isAbortError(err)) return;
        setMessages([]);
        setTotal(0);
      } finally {
        if (!signal.aborted) setLoading(false);
      }
    },
    [conversationId, startRequest],
  );

  useEffect(() => {
    setActiveYear(null);
    setFindTerm("");
    setActiveMatch(0);
    void fetchConversationPage(0);
  }, [fetchConversationPage]);

  const selectAllYears = () => {
    setActiveYear(null);
    setActiveMatch(0);
    void fetchConversationPage(0);
  };

  const selectYear = (year: number) => {
    if (activeYear === year) {
      selectAllYears();
      return;
    }
    setActiveYear(year);
    setActiveMatch(0);
    void fetchYear(year);
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
    loading,
    fetchConversationPage,
    selectAllYears,
    selectYear,
  };
}
