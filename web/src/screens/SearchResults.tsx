import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";

interface SearchResult {
  conversation: Conversation;
  match_count: number;
  snippet: string;
}

export default function SearchResults({
  query,
  onSelectResult,
}: {
  query: string;
  onSelectResult: (conversation: Conversation, term: string) => void;
}) {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!query.trim()) return;
    setLoading(true);
    apiClient
      .get<{ results: SearchResult[] }>(
        `/v1/export/messages?q=${encodeURIComponent(query)}&group_by=conversation&limit=50`,
      )
      .then((res) => setResults(res.results))
      .catch(() => setResults([]))
      .finally(() => setLoading(false));
  }, [query]);

  if (loading) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Searching…</div>;
  if (results.length === 0) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>No results for "{query}"</div>;

  return (
    <div style={{ overflow: "auto", flex: 1 }}>
      <div style={{ padding: "0.5rem 0.75rem", fontSize: "0.75rem", color: "#6b7280", borderBottom: "1px solid #e5e7eb" }}>
        {results.length} conversation{results.length !== 1 ? "s" : ""} matching "{query}"
      </div>
      {results.map((r) => (
        <button
          key={r.conversation.id}
          onClick={() => onSelectResult(r.conversation, query)}
          style={{
            display: "block", width: "100%", textAlign: "left", border: "none",
            background: "transparent", padding: "0.5rem 0.75rem", cursor: "pointer",
            borderBottom: "1px solid #f3f4f6",
          }}
        >
          <div style={{ fontSize: "0.875rem", fontWeight: 500, color: "#1f2937" }}>
            {r.conversation.label || r.conversation.participants.map((p) => p.name || p.handle).join(", ")}
          </div>
          <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
            {r.match_count} match{r.match_count !== 1 ? "es" : ""}
          </div>
          <div style={{ fontSize: "0.75rem", color: "#9ca3af", marginTop: "0.125rem", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {r.snippet}
          </div>
        </button>
      ))}
    </div>
  );
}
