"use client";

import type { MessageRow } from "@/lib/types";
import {
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

/**
 * Fetch messages for one or more conversations.
 * Pass `year` to scope to a calendar year; omit or pass `null` for the full conversation.
 */
export async function fetchThreadMessages(
  conversationIds: number[],
  year: number | null | undefined,
  sourceQuery: string,
): Promise<MessageRow[]> {
  const ids = conversationIds.join(",");
  const yearPart = year != null ? `&year=${year}` : "";
  const res = await fetch(
    `/api/messages?conversationIds=${ids}${yearPart}${sourceQuery}`,
  );
  const data = await res.json();
  return data.messages ?? [];
}

/** Convenience alias for full-conversation loads (no year query param). */
export async function fetchFullThreadMessages(
  conversationIds: number[],
  sourceQuery: string,
): Promise<MessageRow[]> {
  return fetchThreadMessages(conversationIds, null, sourceQuery);
}

export function useThreadMessages(options: {
  conversationIds: number[] | null;
  /**
   * Calendar year to load. When `fullConversation` is false (default), null
   * clears messages. When `fullConversation` is true, null/omitted loads all years.
   */
  year?: number | null;
  /** Includes leading `&` when present, e.g. `"&source=x"` or `""`. */
  sourceQuery: string;
  enabled?: boolean;
  /**
   * When true, fetch the full conversation without a year query param
   * (year may be null/omitted). Year-scoped loads keep the default.
   */
  fullConversation?: boolean;
  /**
   * When this value changes, refetch even if ids/year/source are unchanged
   * (e.g. browse threadsEpoch after same-contact re-open).
   */
  reloadToken?: number | string;
}): {
  messages: MessageRow[];
  loading: boolean;
  setMessages: Dispatch<SetStateAction<MessageRow[]>>;
} {
  const {
    conversationIds,
    year = null,
    sourceQuery,
    enabled = true,
    fullConversation = false,
    reloadToken,
  } = options;
  const [messages, setMessages] = useState<MessageRow[]>([]);
  const [loading, setLoading] = useState(false);

  const idsKey = conversationIds?.join(",") ?? "";
  const canFetch =
    enabled &&
    conversationIds != null &&
    conversationIds.length > 0 &&
    (fullConversation || year != null);

  useEffect(() => {
    if (!canFetch || conversationIds == null) {
      setMessages([]);
      setLoading(false);
      return;
    }
    if (!fullConversation && year == null) {
      setMessages([]);
      setLoading(false);
      return;
    }
    const ids = conversationIds;
    const fetchYear = fullConversation ? null : year;
    let cancelled = false;
    setLoading(true);
    fetchThreadMessages(ids, fetchYear, sourceQuery)
      .then((next) => {
        if (!cancelled) setMessages(next);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // idsKey stands in for conversationIds identity
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [canFetch, idsKey, year, sourceQuery, fullConversation, reloadToken]);

  return { messages, loading, setMessages };
}
