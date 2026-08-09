import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import Button from "./Button";

interface ContactDetail {
  id: string;
  name: string;
  handles: {
    handle: string;
    service: string | null;
    start_date: string | null;
    end_date: string | null;
    message_count: number;
  }[];
  direct_conversations: number;
  group_conversations: number;
  total_messages: number;
}

/** Lightweight row data so the drawer can paint before the detail API returns. */
export type ContactPreview = {
  id: string;
  name: string;
  handles?: string[];
};

const SERVICES = ["phone", "email", "discord", "instagram", "telegram", "signal"];

export default function ContactDrawer({
  contactId,
  preview = null,
  onClose,
}: {
  contactId: string | null;
  preview?: ContactPreview | null;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<ContactDetail | null>(null);
  const [editingName, setEditingName] = useState(false);
  const [nameValue, setNameValue] = useState("");
  const [newHandle, setNewHandle] = useState("");
  const [newService, setNewService] = useState("discord");
  const [matchResults, setMatchResults] = useState<Conversation[] | null>(null);

  const detailMatches =
    !!contactId && !!detail && String(detail.id) === String(contactId);
  const previewMatches =
    !!contactId && !!preview && String(preview.id) === String(contactId);

  const displayName = detailMatches
    ? detail!.name
    : previewMatches
      ? preview!.name
      : "Loading…";
  const loading = !detailMatches;

  const loadDetail = () => {
    if (!contactId) return;
    apiClient
      .get<ContactDetail>(`/v1/export/contacts/${contactId}`)
      .then((next) => {
        if (String(next.id) !== String(contactId)) return;
        setDetail(next);
      })
      .catch(() => {
        /* keep preview; detail stays unset */
      });
  };

  useEffect(() => {
    setMatchResults(null);
    setEditingName(false);
    setNewHandle("");
    if (!contactId) {
      setDetail(null);
      return;
    }

    // Keep detail only when re-selecting the same contact (instant reopen).
    setDetail((prev) =>
      prev && String(prev.id) === String(contactId) ? prev : null,
    );

    const ac = new AbortController();
    apiClient
      .get<ContactDetail>(`/v1/export/contacts/${contactId}`, { signal: ac.signal })
      .then((next) => {
        if (ac.signal.aborted) return;
        setDetail(next);
      })
      .catch(() => {
        /* aborted or failed — preview still shown */
      });
    return () => ac.abort();
  }, [contactId]);

  useEffect(() => {
    setNameValue(displayName === "Loading…" ? "" : displayName);
    setEditingName(false);
  }, [displayName, contactId]);

  useEffect(() => {
    if (!contactId) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (editingName) {
        setEditingName(false);
        setNameValue(displayName === "Loading…" ? "" : displayName);
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [contactId, editingName, displayName, onClose]);

  const checkMatches = async () => {
    if (!newHandle.trim()) return;
    try {
      const res = await apiClient.get<{ conversations: Conversation[] }>(
        `/v1/export/conversations?q=${encodeURIComponent(`handle:${newHandle}`)}&limit=100&offset=0`,
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

  if (!contactId) return null;

  const handleRows: ContactDetail["handles"] = detailMatches
    ? detail!.handles
    : (previewMatches ? preview!.handles : undefined)?.map((h) => ({
        handle: h,
        service: null,
        start_date: null,
        end_date: null,
        message_count: 0,
      })) ?? [];

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
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "1rem", gap: "0.5rem" }}>
          {editingName && detailMatches ? (
            <input
              type="text"
              value={nameValue}
              onChange={(e) => setNameValue(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter") {
                  await apiClient.post(`/v1/export/contacts/${contactId}`, { name: nameValue });
                  setEditingName(false);
                  loadDetail();
                } else if (e.key === "Escape") {
                  e.stopPropagation();
                  setEditingName(false);
                  setNameValue(detail!.name);
                }
              }}
              onBlur={async () => {
                if (nameValue !== detail!.name) {
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
              onClick={() => {
                if (detailMatches) setEditingName(true);
              }}
              style={{
                margin: 0,
                fontSize: "1.125rem",
                cursor: detailMatches ? "pointer" : "default",
                minWidth: 0,
              }}
              title={detailMatches ? "Click to edit" : undefined}
            >
              {displayName}
              {detailMatches ? " ✎" : null}
            </h2>
          )}
          <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer", color: "var(--muted)", flexShrink: 0 }}>×</button>
        </div>

        <h3 style={{ fontSize: "0.75rem", color: "var(--muted)", textTransform: "uppercase", marginBottom: "0.5rem" }}>Handles</h3>
        {handleRows.length === 0 ? (
          <div style={{ fontSize: "0.813rem", color: "var(--muted)", marginBottom: "0.5rem" }}>
            {loading ? "Loading…" : "No handles"}
          </div>
        ) : (
          handleRows.map((h, i) => (
            <div key={`${h.handle}-${i}`} style={{ marginBottom: "0.5rem", fontSize: "0.875rem" }}>
              <div style={{ fontWeight: 500 }}>{h.handle}</div>
              {(h.service || h.start_date || h.message_count > 0) && (
                <div style={{ color: "var(--muted)" }}>
                  {h.service}
                  {h.start_date && ` · ${new Date(h.start_date).getFullYear()}–${h.end_date ? new Date(h.end_date).getFullYear() : "present"}`}
                  {h.message_count > 0 && ` · ${h.message_count} messages`}
                </div>
              )}
            </div>
          ))
        )}

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
            disabled={!newHandle.trim() || loading}
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
          {detailMatches ? (
            <>
              <div>{detail!.direct_conversations} direct conversation{detail!.direct_conversations !== 1 ? "s" : ""}</div>
              <div>{detail!.group_conversations} group conversation{detail!.group_conversations !== 1 ? "s" : ""}</div>
              <div>{detail!.total_messages} total messages</div>
            </>
          ) : (
            <div>Loading details…</div>
          )}
        </div>
      </div>
    </>
  );
}
