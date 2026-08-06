import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface ImportEntry {
  id: string;
  source: string;
  tool: string;
  mode: string;
  created_at: string;
  completed_at: string | null;
  message_count: number;
  conversation_count: number;
  duplicate_count: number;
  attachment_count: number;
  total_bytes: number;
}

export default function ImportHistoryScreen() {
  const [imports, setImports] = useState<ImportEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    apiClient
      .get<{ imports: ImportEntry[] }>("/v1/imports")
      .then((res) => setImports(res.imports))
      .catch(() => setImports([]))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div style={{ padding: "1.5rem", color: "#9ca3af" }}>Loading…</div>;

  return (
    <div style={{ padding: "1.5rem", maxWidth: "900px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Import History</h2>
      {imports.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "#9ca3af" }}>No imports yet.</div>
      ) : (
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.813rem" }}>
          <thead>
            <tr style={{ borderBottom: "2px solid #e5e7eb", textAlign: "left" }}>
              <th style={{ padding: "0.5rem" }}>Date</th>
              <th style={{ padding: "0.5rem" }}>Source</th>
              <th style={{ padding: "0.5rem" }}>Messages</th>
              <th style={{ padding: "0.5rem" }}>Attachments</th>
              <th style={{ padding: "0.5rem" }}>Size</th>
              <th style={{ padding: "0.5rem" }}>Conversations</th>
              <th style={{ padding: "0.5rem" }}>Duplicates</th>
            </tr>
          </thead>
          <tbody>
            {imports.map((imp) => (
              <tr key={imp.id} style={{ borderBottom: "1px solid #f3f4f6" }}>
                <td style={{ padding: "0.5rem" }}>
                  {new Date(imp.created_at).toLocaleDateString([], {
                    month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit",
                  })}
                </td>
                <td style={{ padding: "0.5rem" }}>{imp.source}</td>
                <td style={{ padding: "0.5rem" }}>{imp.message_count.toLocaleString()}</td>
                <td style={{ padding: "0.5rem" }}>{imp.attachment_count.toLocaleString()}</td>
                <td style={{ padding: "0.5rem" }}>{imp.total_bytes > 0 ? `${(imp.total_bytes / 1048576).toFixed(1)} MB` : "—"}</td>
                <td style={{ padding: "0.5rem" }}>{imp.conversation_count.toLocaleString()}</td>
                <td style={{ padding: "0.5rem" }}>{imp.duplicate_count.toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
