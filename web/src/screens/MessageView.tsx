import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { apiClient } from "../lib/api";
import type { Conversation, Message, MessageAttachment } from "../lib/types";
import MessageBubble from "../components/MessageBubble";
import AttachmentLightbox from "../components/AttachmentLightbox";

const PAGE_SIZE = 50;

export default function MessageView({
  conversation,
  onOpenContact,
  initialFindTerm,
}: {
  conversation: Conversation;
  onOpenContact?: (contactId: string) => void;
  initialFindTerm?: string;
}) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [findTerm, setFindTerm] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const [loading, setLoading] = useState(false);
  const [lightboxAttachments, setLightboxAttachments] = useState<MessageAttachment[] | null>(null);
  const [lightboxIndex, setLightboxIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  // Open the lightbox at the clicked image; prev/next walks this page's images
  const handleAttachmentClick = useCallback((att: MessageAttachment) => {
    const images = messages.flatMap((m) =>
      (m.attachments || []).filter(
        (a) => a.sha256 && a.mime_type?.startsWith("image/"),
      ),
    );
    const idx = images.findIndex((a) => a.sha256 === att.sha256);
    setLightboxAttachments(images.length > 0 ? images : [att]);
    setLightboxIndex(idx >= 0 ? idx : 0);
  }, [messages]);

  const fetchPage = useCallback(
    async (newOffset: number, _searchTerm?: string) => {
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

  useEffect(() => {
    fetchPage(0);
  }, [fetchPage]);

  // Pre-fill the find bar when arriving from a search result
  useEffect(() => {
    if (initialFindTerm) {
      setFindTerm(initialFindTerm);
      setActiveMatch(0);
    }
  }, [initialFindTerm, conversation.id]);

  const headerParticipants = messages[0]?.conversation.participants || [];

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

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Header */}
      <div style={{
        padding: "0.75rem 1.5rem", borderBottom: "1px solid #e5e7eb",
        background: "#fafafa",
      }}>
        <div style={{ fontSize: "1rem", fontWeight: 600, marginBottom: "0.25rem" }}>
          {conversation.label ||
            (conversation.is_group
              ? `${conversation.participants.length} participants`
              : conversation.participants[0]?.name || conversation.participants[0]?.handle)}
        </div>
        <div style={{ display: "flex", gap: "1rem", fontSize: "0.75rem", color: "#6b7280", flexWrap: "wrap" }}>
          <span>{conversation.service}</span>
          {conversation.date_range_start && conversation.date_range_end && (
            <span>
              {new Date(conversation.date_range_start).toLocaleDateString([], { month: "short", year: "numeric" })} –{" "}
              {new Date(conversation.date_range_end).toLocaleDateString([], { month: "short", year: "numeric" })}
            </span>
          )}
          <span>{conversation.message_count} messages</span>
        </div>
        {headerParticipants.length > 0 && (
          <div style={{ display: "flex", gap: "0.375rem", flexWrap: "wrap", marginTop: "0.375rem" }}>
            {headerParticipants.map((p, i) => {
              const label = p.name_hint || p.handle;
              return p.contact_id ? (
                <button
                  key={i}
                  onClick={() => onOpenContact?.(p.contact_id!)}
                  title={`Open contact for ${label}`}
                  style={{
                    fontSize: "0.75rem", padding: "0.125rem 0.5rem", borderRadius: "999px",
                    border: "1px solid #d1d5db", background: "#fff",
                    color: "#2563eb", cursor: "pointer",
                  }}
                >
                  {label}
                </button>
              ) : (
                <span
                  key={i}
                  style={{
                    fontSize: "0.75rem", padding: "0.125rem 0.5rem", borderRadius: "999px",
                    border: "1px solid #e5e7eb", background: "#f9fafb", color: "#6b7280",
                  }}
                >
                  {label}
                </span>
              );
            })}
          </div>
        )}
      </div>

      {/* Find bar */}
      <div style={{
        padding: "0.375rem 1.5rem", borderBottom: "1px solid #e5e7eb",
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
            border: "1px solid #d1d5db", borderRadius: "4px",
          }}
        />
        {matchIds.length > 0 && (
          <>
            <span style={{ fontSize: "0.75rem", color: "#6b7280", whiteSpace: "nowrap" }}>
              {activeMatch + 1} of {matchIds.length} on this page
            </span>
            <button onClick={() => setActiveMatch((a) => (a - 1 + matchIds.length) % matchIds.length)}
              style={{ padding: "0.25rem 0.375rem", fontSize: "0.813rem", border: "1px solid #d1d5db", borderRadius: "4px", background: "#fff", cursor: "pointer" }}>
              ↑
            </button>
            <button onClick={() => setActiveMatch((a) => (a + 1) % matchIds.length)}
              style={{ padding: "0.25rem 0.375rem", fontSize: "0.813rem", border: "1px solid #d1d5db", borderRadius: "4px", background: "#fff", cursor: "pointer" }}>
              ↓
            </button>
          </>
        )}
      </div>

      {/* Messages */}
      <div ref={listRef} style={{ flex: 1, overflow: "auto" }}>
        {loading ? (
          <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>
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

      {/* Pagination */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "center",
        gap: "1rem", padding: "0.5rem", borderTop: "1px solid #e5e7eb",
        fontSize: "0.813rem", color: "#6b7280",
      }}>
        <button onClick={() => fetchPage(Math.max(0, offset - PAGE_SIZE))} disabled={offset === 0}
          style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
          Previous
        </button>
        <span>
          Messages {total === 0 ? 0 : offset + 1}–{Math.min(offset + PAGE_SIZE, total)} of {total}
        </span>
        <button onClick={() => fetchPage(offset + PAGE_SIZE)} disabled={offset + PAGE_SIZE >= total}
          style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
          Next
        </button>
      </div>

      {/* Attachment lightbox */}
      {lightboxAttachments && (
        <AttachmentLightbox
          attachments={lightboxAttachments}
          currentIndex={lightboxIndex}
          onClose={() => setLightboxAttachments(null)}
          onPrev={() => setLightboxIndex((i) => (i - 1 + lightboxAttachments.length) % lightboxAttachments.length)}
          onNext={() => setLightboxIndex((i) => (i + 1) % lightboxAttachments.length)}
        />
      )}
    </div>
  );
}
