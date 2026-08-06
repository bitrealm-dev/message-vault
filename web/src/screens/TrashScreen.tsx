import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface TrashEntry {
  id: string;
  label: string;
  message_count: number;
  deleted_at: string;
  conversation_exists: boolean;
}

export default function TrashScreen() {
  const [entries, setEntries] = useState<TrashEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState("");

  const fetchTrash = () => {
    setLoading(true);
    apiClient
      .get<{ trash: TrashEntry[] }>("/v1/export/trash")
      .then((res) => setEntries(res.trash))
      .catch(() => setEntries([]))
      .finally(() => setLoading(false));
  };

  useEffect(() => { fetchTrash(); }, []);

  const restore = async (id: string) => {
    await apiClient.post(`/v1/trash/${id}/restore`);
    setMessage("Conversation restored.");
    fetchTrash();
  };

  const emptyTrash = async () => {
    if (!confirm("Permanently delete all trashed messages?")) return;
    await apiClient.post("/v1/trash/empty");
    setMessage("Trash emptied.");
    fetchTrash();
  };

  if (loading) return <div style={{ padding: "1.5rem", fontSize: "0.875rem", color: "#9ca3af" }}>Loading…</div>;

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1.5rem" }}>
        <h2 style={{ margin: 0 }}>Trash</h2>
        {entries.length > 0 && (
          <button onClick={emptyTrash}
            style={{ fontSize: "0.813rem", color: "#dc2626", border: "1px solid #fecaca", background: "#fef2f2", padding: "0.375rem 0.75rem", borderRadius: "4px", cursor: "pointer" }}>
            Empty trash
          </button>
        )}
      </div>
      {message && (
        <div style={{ marginBottom: "1rem", padding: "0.5rem 0.75rem", background: "#f0fdf4", borderRadius: "4px", fontSize: "0.813rem", color: "#166534" }}>
          {message}
        </div>
      )}
      {entries.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "#9ca3af" }}>Trash is empty.</div>
      ) : (
        entries.map((entry) => (
          <div key={entry.id} style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "0.75rem", borderBottom: "1px solid #f3f4f6" }}>
            <div>
              <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>{entry.label}</div>
              <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
                {entry.message_count} message{entry.message_count !== 1 ? "s" : ""} · deleted {new Date(entry.deleted_at).toLocaleDateString()}
              </div>
            </div>
            <button onClick={() => restore(entry.id)} style={{ fontSize: "0.813rem", padding: "0.25rem 0.75rem", cursor: "pointer" }}>
              Restore
            </button>
          </div>
        ))
      )}
    </div>
  );
}
