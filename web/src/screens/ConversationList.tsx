import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import ConversationRow from "../components/ConversationRow";

export default function ConversationList({
  selectedId,
  onSelect,
  query,
  onNavigate,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
  onNavigate?: (view: string) => void;
}) {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    apiClient
      .get<{ conversations: Conversation[] }>(
        `/v1/export/conversations?q=${encodeURIComponent(query)}`,
      )
      .then((res) => setConversations(res.conversations))
      .catch(() => setConversations([]))
      .finally(() => setLoading(false));
  }, [query]);

  if (loading) {
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>;
  }

  if (conversations.length === 0) {
    return (
      <div style={{ padding: "1.5rem 1rem", fontSize: "0.813rem", color: "#9ca3af", textAlign: "center" }}>
        <p style={{ margin: "0 0 0.5rem", fontWeight: 600, color: "#6b7280" }}>No messages yet</p>
        <p style={{ margin: "0 0 1rem" }}>Import your first messages to get started.</p>
        {onNavigate && (
          <button
            onClick={() => onNavigate("import")}
            style={{
              padding: "0.5rem 1.25rem",
              fontSize: "0.813rem",
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            Import messages
          </button>
        )}
      </div>
    );
  }

  return (
    <div style={{ overflow: "auto", flex: 1 }}>
      {conversations.map((c) => (
        <ConversationRow
          key={c.id}
          conversation={c}
          isSelected={c.id === selectedId}
          onClick={() => onSelect(c)}
        />
      ))}
    </div>
  );
}
