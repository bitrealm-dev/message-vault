import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { apiClient } from "../lib/api";
import type { Conversation, Message, MessageAttachment } from "../lib/types";
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
  const displayParticipants = useMemo(() => {
    if (conversation.participants.length > 0) {
      return conversation.participants.map((p) => ({
        label: p.name?.trim() || p.handle,
        contact_id: p.contact_id,
      }));
    }
    const fromMsg = messages[0]?.conversation.participants || [];
    return fromMsg.map((p) => ({
      label: p.name_hint || p.handle,
      contact_id: p.contact_id,
    }));
  }, [conversation.participants, messages]);

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

  const chipStyle = (active: boolean) => ({
    fontSize: "0.688rem",
    border: `1px solid ${active ? "var(--accent)" : "var(--border)"}`,
    background: active ? "var(--accent)" : "var(--panel)",
    padding: "0.125rem 0.375rem",
    borderRadius: "4px",
    cursor: "pointer" as const,
    color: active ? "var(--sent-text, #fff)" : "var(--accent)",
    fontWeight: active ? 600 : 400,
  });

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Header */}
      <div style={{
        padding: "0.75rem 1.5rem", borderBottom: "1px solid var(--border)",
        background: "var(--elevated)",
      }}>
        <button
          type="button"
          aria-expanded={participantsOpen}
          onClick={() => setParticipantsOpen((o) => !o)}
          disabled={displayParticipants.length === 0}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
            width: "100%",
            padding: 0,
            margin: 0,
            border: "none",
            background: "transparent",
            color: "var(--text)",
            cursor: displayParticipants.length > 0 ? "pointer" : "default",
            fontSize: "1rem",
            fontWeight: 600,
            textAlign: "left",
          }}
        >
          {displayParticipants.length > 0 && (
            <span
              aria-hidden
              style={{
                display: "inline-block",
                fontSize: "0.688rem",
                color: "var(--muted)",
                fontWeight: 600,
                transform: participantsOpen ? "rotate(90deg)" : "none",
                transition: "transform 0.15s ease",
                flexShrink: 0,
              }}
            >
              ▶
            </span>
          )}
          <span style={{ minWidth: 0 }}>
            {conversation.label ||
              (conversation.is_group
                ? `${conversation.participants.length} participants`
                : conversation.participants[0]?.name || conversation.participants[0]?.handle)}
          </span>
        </button>

        {participantsOpen && displayParticipants.length > 0 && (
          <div
            style={{
              display: "flex",
              gap: "0.375rem",
              flexWrap: "wrap",
              marginTop: "0.5rem",
            }}
          >
            {displayParticipants.map((p, i) =>
              p.contact_id ? (
                <button
                  key={`${p.contact_id}-${p.label}-${i}`}
                  type="button"
                  onClick={() => onOpenContact?.(p.contact_id!)}
                  title={`Open contact for ${p.label}`}
                  style={{
                    fontSize: "0.75rem",
                    padding: "0.125rem 0.5rem",
                    borderRadius: "999px",
                    border: "1px solid var(--border)",
                    background: "var(--panel)",
                    color: "var(--accent)",
                    cursor: "pointer",
                  }}
                >
                  {p.label}
                </button>
              ) : (
                <span
                  key={`${p.label}-${i}`}
                  style={{
                    fontSize: "0.75rem",
                    padding: "0.125rem 0.5rem",
                    borderRadius: "999px",
                    border: "1px solid var(--border)",
                    background: "var(--elevated)",
                    color: "var(--muted)",
                  }}
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
          style={{
            height: 1,
            background: "var(--border)",
            margin: "0.75rem 0",
          }}
        />

        <div style={{ display: "flex", gap: "1rem", fontSize: "0.75rem", color: "var(--muted)", flexWrap: "wrap" }}>
          <span>{conversation.service}</span>
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
            style={{
              fontSize: "0.75rem", padding: "0.125rem 0.5rem", borderRadius: "999px",
              border: "1px solid var(--border)", background: "var(--panel)",
              color: "var(--accent)", cursor: "pointer",
            }}
          >
            Sources
          </button>
        </div>

        {years.length > 0 && (
          <div style={{ display: "flex", gap: "0.375rem", flexWrap: "wrap", marginTop: "0.375rem", alignItems: "center" }}>
            <button
              type="button"
              onClick={selectAllYears}
              title="Show all years (paged)"
              style={chipStyle(activeYear === null)}
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
                style={chipStyle(activeYear === year)}
              >
                {year}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Find bar */}
      <div style={{
        padding: "0.375rem 1.5rem", borderBottom: "1px solid var(--border)",
        display: "flex", gap: "0.5rem", alignItems: "center",
      }}>
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
          style={{
            flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.813rem",
            border: "1px solid var(--border)", borderRadius: "4px",
            background: "var(--bg)", color: "var(--text)",
          }}
        />
        {matchIds.length > 0 && (
          <>
            <span style={{ fontSize: "0.75rem", color: "var(--muted)", whiteSpace: "nowrap" }}>
              {activeMatch + 1} of {matchIds.length}
              {yearMode ? " in this year" : " on this page"}
            </span>
            <Button onClick={() => setActiveMatch((a) => (a - 1 + matchIds.length) % matchIds.length)}
              style={{ padding: "0.25rem 0.375rem", fontSize: "0.813rem" }}>
              ↑
            </Button>
            <Button onClick={() => setActiveMatch((a) => (a + 1) % matchIds.length)}
              style={{ padding: "0.25rem 0.375rem", fontSize: "0.813rem" }}>
              ↓
            </Button>
          </>
        )}
      </div>

      {/* Messages */}
      <div ref={listRef} style={{ flex: 1, overflow: "auto" }}>
        {loading ? (
          <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--muted)" }}>Loading…</div>
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
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "center",
        gap: "1rem", padding: "0.5rem", borderTop: "1px solid var(--border)",
        fontSize: "0.813rem", color: "var(--muted)",
      }}>
        {!yearMode && (
          <Button
            onClick={() => void fetchConversationPage(Math.max(0, offset - PAGE_SIZE))}
            disabled={offset === 0 || loading}
            style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}
          >
            Previous
          </Button>
        )}
        <span>{footerLabel}</span>
        {!yearMode && (
          <Button
            onClick={() => void fetchConversationPage(offset + PAGE_SIZE)}
            disabled={offset + PAGE_SIZE >= total || loading}
            style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}
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
