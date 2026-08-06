import { useState, useEffect, useCallback } from "react";
import { apiClient } from "../lib/api";
import type { Conversation, Message } from "../lib/types";
import MessageBubble from "../components/MessageBubble";
import PaginationBar from "../components/PaginationBar";

const PAGE_SIZE = 50;

export default function MessageView({ conversation }: { conversation: Conversation }) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [findTerm, setFindTerm] = useState("");
  const [loading, setLoading] = useState(false);

  const fetchPage = useCallback(
    async (newOffset: number, searchTerm?: string) => {
      setLoading(true);
      try {
        const q = searchTerm
          ? `conversation:${conversation.id} ${searchTerm}`
          : `conversation:${conversation.id}`;
        const res = await apiClient.get<{ messages: Message[]; total: number }>(
          `/v1/export/messages?q=${encodeURIComponent(q)}&offset=${newOffset}&limit=${PAGE_SIZE}`,
        );
        setMessages(res.messages);
        setTotal(res.total);
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

  const handleSearch = () => {
    fetchPage(0, findTerm);
  };

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
      </div>

      {/* Find bar */}
      <div style={{
        padding: "0.375rem 1.5rem", borderBottom: "1px solid #e5e7eb",
        display: "flex", gap: "0.5rem", alignItems: "center",
      }}>
        <input
          type="text"
          value={findTerm}
          onChange={(e) => setFindTerm(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          placeholder="Find in conversation…"
          style={{
            flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.813rem",
            border: "1px solid #d1d5db", borderRadius: "4px",
          }}
        />
        <button onClick={handleSearch} style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
          Find
        </button>
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflow: "auto" }}>
        {loading ? (
          <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>
        ) : (
          messages.map((m) => <MessageBubble key={m.id} message={m} />)
        )}
      </div>

      {/* Pagination */}
      <PaginationBar
        offset={offset}
        limit={PAGE_SIZE}
        total={total}
        onPrev={() => fetchPage(Math.max(0, offset - PAGE_SIZE))}
        onNext={() => fetchPage(offset + PAGE_SIZE)}
      />
    </div>
  );
}
