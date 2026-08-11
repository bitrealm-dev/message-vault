import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { apiClient } from "../lib/api";
import type { Conversation, Message, MessageAttachment } from "../lib/types";
import { personDisplayLabel } from "../lib/nameAliases";
import { useNameAliases } from "../lib/useNameAliases";
import MessageBubble from "../components/MessageBubble";
import AttachmentLightbox, { type LightboxItem } from "../components/AttachmentLightbox";
import SourcesPanel from "../components/SourcesPanel";
import Button from "../components/Button";

/** Page size for full-conversation browsing. */
const PAGE_SIZE = 50;
/** Server clamp in export_api (`MAX_EXPORT_LIMIT`). */
const YEAR_FETCH_LIMIT = 500;

function conversationYears(
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

function displaySourceLabel(source: string): string {
  const token = source.trim().toLowerCase();
  if (token === "sms-backup-restore") return "SMS/MMS";
  if (token === "whatsapp") return "WhatsApp";
  return source.trim() || "unknown";
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

export default function MessageView({
  conversation,
  onOpenContact,
}: {
  conversation: Conversation;
  onOpenContact?: (contactId: string) => void;
}) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  /** `null` = all years (paged). Otherwise load every message in that calendar year. */
  const [activeYear, setActiveYear] = useState<number | null>(null);
  const [findTerm, setFindTerm] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const [loading, setLoading] = useState(false);
  const [lightboxItems, setLightboxItems] = useState<LightboxItem[] | null>(null);
  // The server derives this fallback from message sources before any page is loaded.
  const sourceLabel = messages[0]
    ? displaySourceLabel(messages[0].source)
    : "unknown";
  const [lightboxIndex, setLightboxIndex] = useState(0);
  const [showSources, setShowSources] = useState(false);
  const [participantsOpen, setParticipantsOpen] = useState(true);
  const listRef = useRef<HTMLDivElement>(null);

  const years = useMemo(
    () => conversationYears(conversation.date_range_start, conversation.date_range_end),
    [conversation.date_range_start, conversation.date_range_end],
  );

  // Open the lightbox at the clicked image; prev/next walks this page's images
  const handleAttachmentClick = useCallback((att: MessageAttachment, source: string) => {
    const images = messages.flatMap((m) =>
      (m.attachments || [])
        .filter((a) => a.sha256 && a.mime_type?.startsWith("image/"))
        .map((a) => ({ attachment: a, source: m.source })),
    );
    const idx = images.findIndex(
      (item) => item.attachment.sha256 === att.sha256 && item.source === source,
    );
    setLightboxItems(images.length > 0 ? images : [{ attachment: att, source }]);
    setLightboxIndex(idx >= 0 ? idx : 0);
  }, [messages]);

  const fetchConversationPage = useCallback(
    async (newOffset: number) => {
      setLoading(true);
      try {
        const q = `in:${conversation.id}`;
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
    [conversation.id],
  );

  const fetchYear = useCallback(
    async (year: number) => {
      setLoading(true);
      try {
        const { messages: all, total: yearTotal } = await fetchAllMessagesForQuery(
          yearQuery(conversation.id, year),
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
    [conversation.id],
  );

  useEffect(() => {
    setActiveYear(null);
    setFindTerm("");
    setActiveMatch(0);
    void fetchConversationPage(0);
  }, [conversation.id, fetchConversationPage]);

  useEffect(() => {
    setParticipantsOpen(true);
  }, [conversation.id]);

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

  /** Prefer list-API participants; fall back to the loaded page's conversation header. */
  const useAliases = useNameAliases();
  const displayParticipants = useMemo(() => {
    if (conversation.participants.length > 0) {
      return conversation.participants.map((p) => ({
        label: personDisplayLabel(
          {
            preferredName: p.name,
            nameAlias: p.name_alias,
            handle: p.handle,
          },
          useAliases,
        ),
        contact_id: p.contact_id,
      }));
    }
    const fromMsg = messages[0]?.conversation.participants || [];
    return fromMsg.map((p) => ({
      label: personDisplayLabel(
        {
          preferredName: p.preferred_name,
          nameAlias: p.name_alias,
          handle: p.handle,
        },
        useAliases,
      ),
      contact_id: p.contact_id,
    }));
  }, [conversation.participants, messages, useAliases]);

  // Find bar: collect visible message IDs that match the find term
  const matchIds = useMemo(() => {
    const t = findTerm.trim().toLowerCase();
    if (!t) return [] as string[];
    return messages
      .filter((m) => (m.text || "").toLowerCase().includes(t))
      .map((m) => m.id);
  }, [messages, findTerm]);

  // Scroll active match into view
  useEffect(() => {
    if (!matchIds.length) return;
    const el = document.getElementById(`msg-${matchIds[activeMatch]}`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [activeMatch, matchIds]);

  const yearMode = activeYear !== null;
  const footerLabel = yearMode
    ? total === 0
      ? `${activeYear}: 0 of 0`
      : `${activeYear}: 1–${total} of ${total}`
    : total === 0
      ? "Messages 0 of 0"
      : `Messages ${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${total}`;

  const chipClass = (active: boolean) =>
    `cursor-pointer rounded border px-1.5 py-0.5 text-[0.688rem] ${
      active
        ? "border-accent bg-accent font-semibold text-[var(--sent-text,#fff)]"
        : "border-border bg-panel font-normal text-accent"
    }`;

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="border-b border-border bg-elevated px-6 py-3">
        <button
          type="button"
          aria-expanded={participantsOpen}
          onClick={() => setParticipantsOpen((o) => !o)}
          disabled={displayParticipants.length === 0}
          className={`m-0 flex w-full items-center gap-2 border-none bg-transparent p-0 text-left text-[1rem] font-semibold text-text ${
            displayParticipants.length > 0 ? "cursor-pointer" : "cursor-default"
          }`}
        >
          {displayParticipants.length > 0 && (
            <span
              aria-hidden
              className={`inline-block shrink-0 text-[0.688rem] font-semibold text-muted transition-transform duration-150 ${
                participantsOpen ? "rotate-90" : ""
              }`}
            >
              ▶
            </span>
          )}
          <span className="min-w-0">
            {conversation.label ||
              (conversation.is_group
                ? `${conversation.participants.length} participants`
                : conversation.participants[0]?.name || conversation.participants[0]?.handle)}
          </span>
        </button>

        {participantsOpen && displayParticipants.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1.5">
            {displayParticipants.map((p, i) =>
              p.contact_id ? (
                <button
                  key={`${p.contact_id}-${p.label}-${i}`}
                  type="button"
                  onClick={() => onOpenContact?.(p.contact_id!)}
                  title={`Open contact for ${p.label}`}
                  className="cursor-pointer rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent"
                >
                  {p.label}
                </button>
              ) : (
                <span
                  key={`${p.label}-${i}`}
                  className="rounded-full border border-border bg-elevated px-2 py-0.5 text-[0.75rem] text-muted"
                >
                  {p.label}
                </span>
              ),
            )}
          </div>
        )}

        <div
          role="separator"
          aria-hidden
          className="my-3 h-px bg-border"
        />

        <div className="flex flex-wrap gap-4 text-[0.75rem] text-muted">
          <span>{sourceLabel}</span>
          {conversation.date_range_start && conversation.date_range_end && (
            <span>
              {new Date(conversation.date_range_start).toLocaleDateString([], { month: "short", year: "numeric" })} –{" "}
              {new Date(conversation.date_range_end).toLocaleDateString([], { month: "short", year: "numeric" })}
            </span>
          )}
          <span>{conversation.message_count} messages</span>
          <button
            type="button"
            onClick={() => setShowSources(true)}
            className="cursor-pointer rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent"
          >
            Sources
          </button>
        </div>

        {years.length > 0 && (
          <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              onClick={selectAllYears}
              title="Show all years (paged)"
              className={chipClass(activeYear === null)}
            >
              All
            </button>
            {years.map((year) => (
              <button
                key={year}
                type="button"
                onClick={() => selectYear(year)}
                title={
                  activeYear === year
                    ? `Clear ${year} filter`
                    : `Load all messages from ${year}`
                }
                className={chipClass(activeYear === year)}
              >
                {year}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Find bar */}
      <div className="flex items-center gap-2 border-b border-border px-6 py-1.5">
        <input
          type="text"
          value={findTerm}
          onChange={(e) => { setFindTerm(e.target.value); setActiveMatch(0); }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (matchIds.length > 0) {
                setActiveMatch((a) => (a + 1) % matchIds.length);
              }
            }
          }}
          placeholder="Find in conversation…"
          className="box-border flex-1 rounded border border-border bg-bg px-2 py-1 text-[0.813rem] text-text"
        />
        {matchIds.length > 0 && (
          <>
            <span className="whitespace-nowrap text-[0.75rem] text-muted">
              {activeMatch + 1} of {matchIds.length}
              {yearMode ? " in this year" : " on this page"}
            </span>
            <Button onClick={() => setActiveMatch((a) => (a - 1 + matchIds.length) % matchIds.length)}
              className="!px-1.5 !py-1 !text-[0.813rem]">
              ↑
            </Button>
            <Button onClick={() => setActiveMatch((a) => (a + 1) % matchIds.length)}
              className="!px-1.5 !py-1 !text-[0.813rem]">
              ↓
            </Button>
          </>
        )}
      </div>

      {/* Messages */}
      <div ref={listRef} className="flex-1 overflow-auto">
        {loading ? (
          <div className="p-4 text-[0.813rem] text-muted">Loading…</div>
        ) : (
          messages.map((m) => (
            <MessageBubble
              key={m.id}
              message={m}
              highlight={findTerm.trim() || undefined}
              isActive={matchIds.length > 0 && matchIds[activeMatch] === m.id}
              onAttachmentClick={handleAttachmentClick}
            />
          ))
        )}
      </div>

      {/* Pagination / year summary */}
      <div className="flex items-center justify-center gap-4 border-t border-border p-2 text-[0.813rem] text-muted">
        {!yearMode && (
          <Button
            onClick={() => void fetchConversationPage(Math.max(0, offset - PAGE_SIZE))}
            disabled={offset === 0 || loading}
            className="!px-3 !py-1 !text-[0.813rem]"
          >
            Previous
          </Button>
        )}
        <span>{footerLabel}</span>
        {!yearMode && (
          <Button
            onClick={() => void fetchConversationPage(offset + PAGE_SIZE)}
            disabled={offset + PAGE_SIZE >= total || loading}
            className="!px-3 !py-1 !text-[0.813rem]"
          >
            Next
          </Button>
        )}
      </div>

      {/* Attachment lightbox */}
      {lightboxItems && (
        <AttachmentLightbox
          items={lightboxItems}
          currentIndex={lightboxIndex}
          onClose={() => setLightboxItems(null)}
          onPrev={() => setLightboxIndex((i) => (i - 1 + lightboxItems.length) % lightboxItems.length)}
          onNext={() => setLightboxIndex((i) => (i + 1) % lightboxItems.length)}
        />
      )}

      {/* Backup provenance slide-over */}
      {showSources && (
        <SourcesPanel conversationId={conversation.id} onClose={() => setShowSources(false)} />
      )}
    </div>
  );
}
