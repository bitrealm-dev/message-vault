"use client";

import type {
  ContactDetail,
  GroupParticipant,
  MessageRow,
  YearThread,
} from "@/lib/types";
import { formatSourceLabel } from "@/lib/sourceLabels";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  GroupParticipantChip,
} from "./GroupParticipantChip";
import { MessageList } from "./MessageList";
import {
  ChevronDownIcon,
  MessageIcon,
  PaperclipIcon,
} from "./icons";
import { useDateTimeFormat } from "./useDateTimeFormat";

const HEADER_EXPANDED_KEY = "mv-browse-thread-header-expanded";

function yearFromTimestamp(ts: string): number | null {
  const y = Number(ts.slice(0, 4));
  return Number.isFinite(y) ? y : null;
}

export type BrowseGroupThreadMeta = {
  participants: GroupParticipant[];
  dateStart: string;
  dateEnd: string;
  messageCount: number;
};

export function BrowseThreadPane({
  detail,
  sources,
  messageSources,
  sourceCounts,
  source,
  onSourceChange,
  yearly,
  messages,
  loadingMessages,
  threadsReady = false,
  activeThread,
  groupThread,
  onParticipantClick,
  hasConversationChoices = false,
  conversationsPanelCollapsed = false,
  highlightTerms = [],
  scrollToMessageId = null,
}: {
  detail: ContactDetail | null;
  sources: string[];
  messageSources: string[];
  sourceCounts: { all: number; bySource: Record<string, number> };
  source: string | null;
  onSourceChange: (id: string | null) => void;
  yearly: YearThread[];
  messages: MessageRow[];
  loadingMessages: boolean;
  /** True once the current contact's threads have finished loading (so an empty state means "no messages"). */
  threadsReady?: boolean;
  activeThread: string | null;
  /** When viewing a group thread, show participants / date / counts under the contact name. */
  groupThread?: BrowseGroupThreadMeta | null;
  onParticipantClick?: (
    participant: GroupParticipant,
    anchor: DOMRect,
  ) => void;
  /** Direct and/or groups exist in column 3 — prompt the user to pick one. */
  hasConversationChoices?: boolean;
  /**
   * When the conversations panel (panel 3) is visible, hide the duplicated
   * group identity header. Show it only when that panel is collapsed.
   */
  conversationsPanelCollapsed?: boolean;
  highlightTerms?: string[];
  scrollToMessageId?: number | null;
}) {
  const { formatDateRange } = useDateTimeFormat();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [headerExpanded, setHeaderExpanded] = useState(true);
  const [activeYear, setActiveYear] = useState<number | null>(null);
  const [attachmentsOnly, setAttachmentsOnly] = useState(false);

  useEffect(() => {
    setAttachmentsOnly(false);
  }, [activeThread]);

  useEffect(() => {
    try {
      if (sessionStorage.getItem(HEADER_EXPANDED_KEY) === "0") {
        setHeaderExpanded(false);
      }
    } catch {
      /* ignore */
    }
  }, []);

  const toggleHeaderExpanded = () => {
    setHeaderExpanded((prev) => {
      const next = !prev;
      try {
        sessionStorage.setItem(HEADER_EXPANDED_KEY, next ? "1" : "0");
      } catch {
        /* ignore */
      }
      return next;
    });
  };

  const threadStats = useMemo(() => {
    if (groupThread) {
      return {
        messageCount: groupThread.messageCount,
        attachmentCount: messages.reduce((n, m) => n + m.attachments.length, 0),
      };
    }
    if (activeThread === "dm" && yearly.length > 0) {
      return {
        messageCount: yearly.reduce((n, y) => n + y.messageCount, 0),
        attachmentCount: yearly.reduce((n, y) => n + y.attachmentCount, 0),
      };
    }
    if (messages.length > 0) {
      return {
        messageCount: messages.length,
        attachmentCount: messages.reduce((n, m) => n + m.attachments.length, 0),
      };
    }
    return null;
  }, [groupThread, activeThread, yearly, messages]);

  const yearsInThread = useMemo(() => {
    if (yearly.length > 0 && activeThread === "dm") {
      return [...yearly].sort((a, b) => a.year - b.year);
    }
    const years = new Set<number>();
    for (const m of messages) {
      const y = yearFromTimestamp(m.timestamp);
      if (y != null) years.add(y);
    }
    return [...years]
      .sort((a, b) => a - b)
      .map((year) => ({
        year,
        messageCount: 0,
        attachmentCount: 0,
        dateStart: "",
        dateEnd: "",
        conversationIds: [] as number[],
      }));
  }, [yearly, messages, activeThread]);

  const visibleMessages = useMemo(
    () =>
      attachmentsOnly
        ? messages.filter((m) => m.attachments.length > 0)
        : messages,
    [messages, attachmentsOnly],
  );

  const messagesByYear = useMemo(() => {
    const chronological = [...visibleMessages].sort((a, b) =>
      a.timestamp < b.timestamp ? -1 : a.timestamp > b.timestamp ? 1 : 0,
    );
    const sections: Array<{ year: number; messages: MessageRow[] }> = [];
    for (const m of chronological) {
      const y = yearFromTimestamp(m.timestamp) ?? 0;
      const last = sections[sections.length - 1];
      if (!last || last.year !== y) {
        sections.push({ year: y, messages: [m] });
      } else {
        last.messages.push(m);
      }
    }
    return sections;
  }, [visibleMessages]);

  useEffect(() => {
    const first = messagesByYear[0]?.year;
    setActiveYear(first ?? null);
  }, [messagesByYear]);

  useEffect(() => {
    const root = scrollRef.current;
    if (!root || messagesByYear.length === 0) return;

    const sectionEls = messagesByYear
      .map((s) => root.querySelector(`#year-${s.year}`))
      .filter((el): el is Element => el != null);
    if (sectionEls.length === 0) return;

    const visible = new Map<number, IntersectionObserverEntry>();

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const id = entry.target.id;
          const year = Number(id.replace(/^year-/, ""));
          if (!Number.isFinite(year)) continue;
          if (entry.isIntersecting) visible.set(year, entry);
          else visible.delete(year);
        }
        if (visible.size === 0) return;
        let bestYear: number | null = null;
        let bestTop = Number.POSITIVE_INFINITY;
        for (const [year, entry] of visible) {
          const top = entry.boundingClientRect.top;
          if (top < bestTop) {
            bestTop = top;
            bestYear = year;
          }
        }
        if (bestYear != null) setActiveYear(bestYear);
      },
      {
        root,
        // Prefer the section occupying the upper portion of the scroll pane.
        rootMargin: "0px 0px -70% 0px",
        threshold: [0, 0.01, 0.1],
      },
    );

    for (const el of sectionEls) observer.observe(el);
    return () => observer.disconnect();
  }, [messagesByYear]);

  const jumpToYear = (year: number) => {
    setActiveYear(year);
    const el = scrollRef.current?.querySelector(`#year-${year}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  useEffect(() => {
    if (scrollToMessageId == null || loadingMessages) return;
    const root = scrollRef.current;
    if (!root) return;
    const el = root.querySelector(`#msg-${scrollToMessageId}`);
    if (!el) return;
    el.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [scrollToMessageId, loadingMessages, visibleMessages]);

  const sourceOptions = [
    {
      id: null as string | null,
      label: "Combined",
      enabled: true,
      count: sourceCounts.all,
    },
    ...sources.map((id) => ({
      id,
      label: formatSourceLabel(id),
      enabled: messageSources.includes(id),
      count: sourceCounts.bySource[id] ?? 0,
    })),
  ];

  const stripItems: Array<{
    key: string;
    label: string;
    title?: string;
    active: boolean;
    disabled?: boolean;
    onClick: () => void;
  }> = sourceOptions.map((opt) => ({
    key: `src-${opt.id ?? "all"}`,
    label: opt.label,
    title: `${opt.label}: ${opt.count.toLocaleString()} messages`,
    active: opt.id === null ? source === null : source === opt.id,
    disabled: !opt.enabled,
    onClick: () => {
      if (!opt.enabled) return;
      onSourceChange(opt.id);
    },
  }));

  const yearItems = yearsInThread.map((y) => ({
    key: `y-${y.year}`,
    year: y.year,
    label: String(y.year),
    title:
      y.messageCount > 0
        ? `${y.year}: ${y.messageCount.toLocaleString()} messages`
        : `Jump to ${y.year}`,
    active: activeYear === y.year,
    onClick: () => jumpToYear(y.year),
  }));

  const dateLabel = groupThread
    ? formatDateRange(groupThread.dateStart, groupThread.dateEnd)
    : null;

  // Group identity lives in panel 3 rows; only repeat it when that panel is collapsed.
  const showGroupIdentity =
    conversationsPanelCollapsed &&
    !!groupThread &&
    (groupThread.participants.length > 0 || !!dateLabel);
  const hasMessages = messages.length > 0;
  const showYearJump = hasMessages && yearItems.length > 0;
  const showHeader = showGroupIdentity || showYearJump;

  return (
    <section className="flex h-full min-h-0 flex-col bg-bg">
      {showHeader && (
        <div className="shrink-0 border-b border-border px-5 py-2 text-center">
          {(headerExpanded || !showYearJump) && showGroupIdentity && (
            <>
              {groupThread && groupThread.participants.length > 0 ? (
                <div className="flex flex-wrap items-center justify-center gap-y-0 text-[14px] font-medium leading-snug text-text">
                  {groupThread.participants.map((p, idx) => (
                    <span
                      key={`${p.handle}-${idx}`}
                      className="inline-flex items-center"
                    >
                      {onParticipantClick ? (
                        <GroupParticipantChip
                          label={p.name}
                          onClick={(anchor) => onParticipantClick(p, anchor)}
                        />
                      ) : (
                        <span className="whitespace-nowrap px-1.5 py-0.5">
                          {p.name}
                        </span>
                      )}
                    </span>
                  ))}
                </div>
              ) : null}

              {dateLabel && (
                <div className="mt-1 text-[14px] text-muted tabular-nums">
                  {dateLabel}
                </div>
              )}
            </>
          )}

          {showYearJump && (
            <div
              className={`relative flex min-h-7 items-center justify-center ${
                headerExpanded && showGroupIdentity ? "mt-1.5" : ""
              }`}
            >
              <div
                className={`flex flex-wrap items-center justify-center gap-x-3 gap-y-0.5 ${
                  showGroupIdentity ? "pr-8" : ""
                }`}
              >
                {yearItems.map((item) => (
                  <button
                    key={item.key}
                    type="button"
                    title={item.title}
                    onClick={item.onClick}
                    className={`text-[13px] font-medium tabular-nums ${
                      item.active
                        ? "text-accent"
                        : "text-text hover:text-accent"
                    }`}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
              {showGroupIdentity && (
                <button
                  type="button"
                  aria-expanded={headerExpanded}
                  aria-label={
                    headerExpanded
                      ? "Hide thread details"
                      : "Show thread details"
                  }
                  title={headerExpanded ? "Hide details" : "Show details"}
                  onClick={toggleHeaderExpanded}
                  className="absolute top-1/2 right-0 inline-flex size-7 -translate-y-1/2 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text"
                >
                  <ChevronDownIcon
                    className={`size-3.5 transition-transform ${
                      headerExpanded ? "rotate-180" : ""
                    }`}
                  />
                </button>
              )}
            </div>
          )}
        </div>
      )}

      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-y-auto px-4 pt-2.5 pb-4"
      >
        {stripItems.length > 0 && sourceCounts.all > 0 && (
          <div className="mb-1.5 flex flex-wrap items-center justify-center gap-y-1">
            {stripItems.map((item, i) => (
              <span key={item.key} className="flex items-center">
                {i > 0 && (
                  <span
                    className="mx-2 text-[13px] text-muted/50"
                    aria-hidden
                  >
                    |
                  </span>
                )}
                <button
                  type="button"
                  disabled={item.disabled}
                  title={item.title}
                  onClick={item.onClick}
                  className={`text-[13px] font-medium ${
                    item.disabled
                      ? "cursor-default text-muted/40"
                      : item.active
                        ? "text-accent"
                        : "text-text hover:text-accent"
                  }`}
                >
                  {item.label}
                </button>
              </span>
            ))}
          </div>
        )}

        {threadStats && (
          <div className="mb-2 flex flex-wrap items-center justify-center gap-x-2 gap-y-1 text-[12px] text-muted">
            <button
              type="button"
              title="Show all messages"
              aria-pressed={!attachmentsOnly}
              onClick={() => setAttachmentsOnly(false)}
              className={`inline-flex items-center gap-1 tabular-nums outline-none focus:outline-none focus-visible:outline-none ${
                !attachmentsOnly
                  ? "font-medium text-text"
                  : "hover:text-text"
              }`}
            >
              <MessageIcon className="size-3.5 shrink-0 opacity-80" />
              {threadStats.messageCount.toLocaleString()}
            </button>
            <span className="opacity-50">·</span>
            <button
              type="button"
              title={
                attachmentsOnly
                  ? "Show all messages"
                  : "Show only messages with attachments"
              }
              aria-pressed={attachmentsOnly}
              disabled={threadStats.attachmentCount === 0}
              onClick={() => setAttachmentsOnly((v) => !v)}
              className={`inline-flex items-center gap-1 tabular-nums outline-none focus:outline-none focus-visible:outline-none ${
                threadStats.attachmentCount === 0
                  ? "cursor-default opacity-40"
                  : attachmentsOnly
                    ? "font-medium text-accent"
                    : "hover:text-accent"
              }`}
            >
              <PaperclipIcon className="size-3.5 shrink-0 opacity-80" />
              {threadStats.attachmentCount.toLocaleString()}
            </button>
          </div>
        )}

        {!activeThread && !loadingMessages && detail && (
          <p className="pt-8 text-center text-[13px] text-muted">
            {!threadsReady
              ? "Loading messages…"
              : hasConversationChoices
                ? "Pick a conversation"
                : "No messages"}
          </p>
        )}
        {loadingMessages && messages.length === 0 && (
          <p className="pt-8 text-center text-[13px] text-muted">
            Loading messages…
          </p>
        )}
        {messages.length > 0 &&
          attachmentsOnly &&
          visibleMessages.length === 0 && (
            <p className="pt-8 text-center text-[13px] text-muted">
              No messages with attachments
            </p>
          )}
        {visibleMessages.length > 0 && (
          <div
            className={`mx-auto flex max-w-2xl flex-col gap-2 ${
              loadingMessages ? "opacity-60" : ""
            }`}
          >
            {messagesByYear.map((section) => (
              <div
                key={section.year}
                id={`year-${section.year}`}
                className="scroll-mt-3"
              >
                <div className="sticky top-0 z-10 -mx-1 mb-2 bg-bg/95 px-1 py-1.5 backdrop-blur-sm">
                  <div className="text-[13px] font-semibold text-text">
                    {section.year || "Unknown"}
                  </div>
                </div>
                <div className="flex flex-col gap-2">
                  <MessageList
                    messages={section.messages}
                    highlightTerms={highlightTerms}
                  />
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
