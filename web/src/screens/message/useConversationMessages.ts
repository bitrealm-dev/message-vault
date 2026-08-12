import { useCallback, useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import type { Message } from "../../lib/types";

/** Page size for full-conversation browsing. */
export const PAGE_SIZE = 50;
/** Server clamp in export_api (`MAX_EXPORT_LIMIT`). */
const YEAR_FETCH_LIMIT = 500;

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

function yearQuery(conversationId: string, year: number): string {
  return `in:${conversationId} after:${year} before:${year + 1}`;
}

export function displaySourceLabel(source: string): string {
  const token = source.trim().toLowerCase();
  if (token === "sms-backup-restore") return "SMS/MMS";
  if (token === "whatsapp") return "WhatsApp";
  return source.trim() || "unknown";
}

/**
 * Footer count line. A year filter loads that year in full, so it always shows
 * the whole range; unfiltered browsing shows the current page window.
 */
export function buildFooterLabel(
  activeYear: number | null,
  total: number,
  offset: number,
): string {
  if (activeYear !== null) {
    if (total === 0) return `${activeYear}: 0 of 0`;
    return `${activeYear}: 1–${total} of ${total}`;
  }
  if (total === 0) return "Messages 0 of 0";
  return `Messages ${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${total}`;
}

async function fetchAllMessagesForQuery(q: string): Promise<{ messages: Message[]; total: number }> {
  const countRes = await apiClient.get<{ messages: number }>(
    `/v1/export/messages/count?q=${encodeURIComponent(q)}`,
  );
  const total = countRes.messages ?? 0;
  if (total === 0) return { messages: [], total: 0 };

  const collected: Message[] = [];
  let offset = 0;
  while (offset < total) {
    const msgRes = await apiClient.get<{ messages: Message[] }>(
      `/v1/export/messages?q=${encodeURIComponent(q)}&offset=${offset}&limit=${YEAR_FETCH_LIMIT}`,
    );
    const batch = msgRes.messages ?? [];
    collected.push(...batch);
    if (batch.length === 0) break;
    offset += batch.length;
    if (batch.length < YEAR_FETCH_LIMIT) break;
  }
  return { messages: collected, total };
}

export function useConversationMessages(conversationId: string) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  /** `null` = all years (paged). Otherwise load every message in that calendar year. */
  const [activeYear, setActiveYear] = useState<number | null>(null);
  const [findTerm, setFindTerm] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const [loading, setLoading] = useState(false);

  const fetchConversationPage = useCallback(
    async (newOffset: number) => {
      setLoading(true);
      try {
        const q = `in:${conversationId}`;
        const [msgRes, countRes] = await Promise.all([
          apiClient.get<{ messages: Message[] }>(
            `/v1/export/messages?q=${encodeURIComponent(q)}&offset=${newOffset}&limit=${PAGE_SIZE}`,
          ),
          apiClient.get<{ messages: number }>(
            `/v1/export/messages/count?q=${encodeURIComponent(q)}`,
          ),
        ]);
        setMessages(msgRes.messages);
        setTotal(countRes.messages);
        setOffset(newOffset);
      } catch {
        setMessages([]);
        setTotal(0);
      } finally {
        setLoading(false);
      }
    },
    [conversationId],
  );

  const fetchYear = useCallback(
    async (year: number) => {
      setLoading(true);
      try {
        const { messages: all, total: yearTotal } = await fetchAllMessagesForQuery(
          yearQuery(conversationId, year),
        );
        setMessages(all);
        setTotal(yearTotal);
        setOffset(0);
      } catch {
        setMessages([]);
        setTotal(0);
      } finally {
        setLoading(false);
      }
    },
    [conversationId],
  );

  useEffect(() => {
    setActiveYear(null);
    setFindTerm("");
    setActiveMatch(0);
    void fetchConversationPage(0);
  }, [conversationId, fetchConversationPage]);

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
