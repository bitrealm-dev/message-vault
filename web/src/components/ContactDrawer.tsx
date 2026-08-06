import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface ContactDetail {
  id: string;
  name: string;
  handles: {
    handle: string;
    service: string;
    start_date: string | null;
    end_date: string | null;
    message_count: number;
  }[];
  direct_conversations: number;
  group_conversations: number;
  total_messages: number;
}

export default function ContactDrawer({
  contactId,
  onClose,
}: {
  contactId: string | null;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<ContactDetail | null>(null);
  const [editingName, setEditingName] = useState(false);
  const [nameValue, setNameValue] = useState(detail?.name ?? "");

  const loadDetail = () => {
    if (!contactId) return;
    apiClient
      .get<ContactDetail>(`/v1/export/contacts/${contactId}`)
      .then(setDetail)
      .catch(() => setDetail(null));
  };

  useEffect(() => {
    loadDetail();
  }, [contactId]);

  // Sync when detail changes (new contact selected)
  useEffect(() => {
    setNameValue(detail?.name ?? "");
    setEditingName(false);
  }, [detail?.name]);

  if (!contactId || !detail) return null;

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
          {editingName ? (
            <input
              type="text"
              value={nameValue}
              onChange={(e) => setNameValue(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter") {
                  await apiClient.post(`/v1/export/contacts/${contactId}`, { name: nameValue });
                  setEditingName(false);
                  loadDetail();
                }
              }}
              onBlur={async () => {
                if (nameValue !== detail.name) {
                  await apiClient.post(`/v1/export/contacts/${contactId}`, { name: nameValue });
                  loadDetail();
                }
                setEditingName(false);
              }}
              autoFocus
              style={{ fontSize: "1.125rem", fontWeight: 600, padding: "0.25rem", width: "100%" }}
            />
          ) : (
            <h2
              onClick={() => setEditingName(true)}
              style={{ margin: 0, fontSize: "1.125rem", cursor: "pointer" }}
              title="Click to edit"
            >
              {detail.name} ✎
            </h2>
          )}
          <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer" }}>×</button>
        </div>

        <h3 style={{ fontSize: "0.75rem", color: "#9ca3af", textTransform: "uppercase", marginBottom: "0.5rem" }}>Handles</h3>
        {detail.handles.map((h, i) => (
          <div key={i} style={{ marginBottom: "0.5rem", fontSize: "0.875rem" }}>
            <div style={{ fontWeight: 500 }}>{h.handle}</div>
            <div style={{ color: "#6b7280" }}>
              {h.service}
              {h.start_date && ` · ${new Date(h.start_date).getFullYear()}–${h.end_date ? new Date(h.end_date).getFullYear() : "present"}`}
              {h.message_count > 0 && ` · ${h.message_count} messages`}
            </div>
          </div>
        ))}

        <div style={{ marginTop: "1rem", fontSize: "0.875rem", color: "#6b7280" }}>
          <div>{detail.direct_conversations} direct conversation{detail.direct_conversations !== 1 ? "s" : ""}</div>
          <div>{detail.group_conversations} group conversation{detail.group_conversations !== 1 ? "s" : ""}</div>
          <div>{detail.total_messages} total messages</div>
        </div>
      </div>
    </>
  );
}
