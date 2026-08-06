import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface SourceInfo {
  backup_name: string;
  message_count: number;
  unique_count: number;
  percentage: number;
}

export default function SourcesPanel({
  conversationId,
  onClose,
}: {
  conversationId: string | null;
  onClose: () => void;
}) {
  const [sources, setSources] = useState<SourceInfo[]>([]);

  useEffect(() => {
    if (!conversationId) return;
    apiClient
      .get<{ sources: SourceInfo[] }>(`/v1/export/conversations/${conversationId}/sources`)
      .then((res) => setSources(res.sources))
      .catch(() => setSources([]));
  }, [conversationId]);

  if (!conversationId) return null;

  const total = sources.reduce((sum, s) => sum + s.unique_count, 0);

  return (
    <>
      <div onClick={onClose} style={{
        position: "fixed", inset: 0, background: "rgba(0,0,0,0.2)", zIndex: 40,
      }} />
      <div style={{
        position: "fixed", right: 0, top: 0, bottom: 0, width: "320px",
        background: "#fff", boxShadow: "-2px 0 8px rgba(0,0,0,0.1)", zIndex: 50,
        overflow: "auto", padding: "1.5rem",
      }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "1rem" }}>
          <h2 style={{ margin: 0, fontSize: "1.125rem" }}>Sources</h2>
          <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer" }}>×</button>
        </div>

        {sources.length === 0 ? (
          <div style={{ fontSize: "0.875rem", color: "#9ca3af" }}>No source data available.</div>
        ) : (
          <>
            {sources.map((s, i) => (
              <div key={i} style={{ marginBottom: "0.75rem", padding: "0.5rem", background: "#f9fafb", borderRadius: "4px" }}>
                <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>{s.backup_name}</div>
                <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
                  {s.message_count.toLocaleString()} messages ({s.percentage}% of total)
                </div>
                <div style={{ fontSize: "0.75rem", color: "#9ca3af" }}>
                  {s.unique_count.toLocaleString()} unique
                </div>
              </div>
            ))}
            <div style={{ marginTop: "0.5rem", fontSize: "0.813rem", color: "#6b7280" }}>
              Net total: {total.toLocaleString()} unique messages
            </div>
          </>
        )}
      </div>
    </>
  );
}
