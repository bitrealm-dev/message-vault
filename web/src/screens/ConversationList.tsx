import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import ConversationRow from "../components/ConversationRow";

export default function ConversationList({
  selectedId,
  onSelect,
  query,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
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
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>No conversations</div>;
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
