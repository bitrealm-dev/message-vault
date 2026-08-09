import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import Button from "./Button";

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

const SERVICES = ["phone", "email", "discord", "instagram", "telegram", "signal"];

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
  const [newHandle, setNewHandle] = useState("");
  const [newService, setNewService] = useState("discord");
  const [matchResults, setMatchResults] = useState<Conversation[] | null>(null);

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

  const checkMatches = async () => {
    if (!newHandle.trim()) return;
    try {
      const res = await apiClient.get<{ conversations: Conversation[] }>(
        `/v1/export/conversations?q=handle:${encodeURIComponent(newHandle)}`,
      );
      setMatchResults(res.conversations);
    } catch {
      setMatchResults([]);
    }
  };

  const addHandle = async () => {
    if (!newHandle.trim()) return;
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: newHandle.trim(), service: newService },
      });
      setNewHandle("");
      setMatchResults(null);
      loadDetail();
    } catch {
      // Leave the input in place so the user can retry
    }
  };

  if (!contactId || !detail) return null;

  return (
    <>
      <div onClick={onClose} style={{
        position: "fixed", inset: 0, background: "rgba(0,0,0,0.2)", zIndex: 40,
      }} />
      <div style={{
        position: "fixed", right: 0, top: 0, bottom: 0, width: "320px",
        background: "var(--panel)", boxShadow: "-2px 0 8px rgba(0,0,0,0.1)", zIndex: 50,
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
              style={{
                fontSize: "1.125rem",
                fontWeight: 600,
                padding: "0.25rem",
                width: "100%",
                background: "var(--bg)",
                color: "var(--text)",
                border: "1px solid var(--border)",
                borderRadius: "4px",
              }}
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
          <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer", color: "var(--muted)" }}>×</button>
        </div>

        <h3 style={{ fontSize: "0.75rem", color: "var(--muted)", textTransform: "uppercase", marginBottom: "0.5rem" }}>Handles</h3>
        {detail.handles.map((h, i) => (
          <div key={i} style={{ marginBottom: "0.5rem", fontSize: "0.875rem" }}>
            <div style={{ fontWeight: 500 }}>{h.handle}</div>
            <div style={{ color: "var(--muted)" }}>
              {h.service}
              {h.start_date && ` · ${new Date(h.start_date).getFullYear()}–${h.end_date ? new Date(h.end_date).getFullYear() : "present"}`}
              {h.message_count > 0 && ` · ${h.message_count} messages`}
            </div>
          </div>
        ))}

        <h3 style={{ fontSize: "0.75rem", color: "var(--muted)", textTransform: "uppercase", margin: "1rem 0 0.5rem" }}>Add Handle</h3>
        <div style={{ display: "flex", gap: "0.375rem" }}>
          <select
            value={newService}
            onChange={(e) => setNewService(e.target.value)}
            style={{
              padding: "0.375rem 0.5rem",
              fontSize: "0.813rem",
              border: "1px solid var(--border)",
              borderRadius: "4px",
              width: "110px",
              background: "var(--bg)",
              color: "var(--text)",
            }}
          >
            {SERVICES.map((s) => <option key={s} value={s}>{s}</option>)}
          </select>
          <input
            type="text"
            value={newHandle}
            onChange={(e) => setNewHandle(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") checkMatches(); }}
            placeholder="user#1234, @handle…"
            style={{
              flex: 1,
              minWidth: 0,
              padding: "0.375rem 0.5rem",
              fontSize: "0.813rem",
              border: "1px solid var(--border)",
              borderRadius: "4px",
              background: "var(--bg)",
              color: "var(--text)",
            }}
          />
        </div>
        <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.5rem" }}>
          <Button
            onClick={checkMatches}
            disabled={!newHandle.trim()}
            style={{ fontSize: "0.813rem", padding: "0.25rem 0.5rem" }}
          >
            Check matches
          </Button>
          <Button
            variant="primary"
            onClick={addHandle}
            disabled={!newHandle.trim()}
            style={{ fontSize: "0.813rem", padding: "0.25rem 0.5rem" }}
          >
            Add handle
          </Button>
        </div>
        {matchResults !== null && matchResults.length > 0 && (
          <div style={{ marginTop: "0.5rem", padding: "0.5rem", background: "var(--info-soft-bg)", borderRadius: "4px", fontSize: "0.813rem" }}>
            We found {matchResults.length} conversation{matchResults.length !== 1 ? "s" : ""} matching {newHandle} on {newService}.
          </div>
        )}

        <div style={{ marginTop: "1rem", fontSize: "0.875rem", color: "var(--muted)" }}>
          <div>{detail.direct_conversations} direct conversation{detail.direct_conversations !== 1 ? "s" : ""}</div>
          <div>{detail.group_conversations} group conversation{detail.group_conversations !== 1 ? "s" : ""}</div>
          <div>{detail.total_messages} total messages</div>
        </div>
      </div>
    </>
  );
}
