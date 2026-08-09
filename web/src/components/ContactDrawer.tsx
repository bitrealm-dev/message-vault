import { useState, useEffect, type CSSProperties } from "react";
import { apiClient } from "../lib/api";
import {
  fetchContactDetail,
  getCachedContactDetail,
  invalidateContactDetail,
  type CachedContactDetail,
} from "../lib/contactDetailCache";
import Button from "./Button";

type ContactDetail = CachedContactDetail;

/** Lightweight row data so the drawer can paint before the detail API returns. */
export type ContactPreview = {
  id: string;
  name: string;
  handles?: string[];
};

export type ContactBrowseKind = "all" | "direct" | "group";

const SERVICES = ["phone", "email", "discord", "instagram", "telegram", "signal"];

function inferService(handle: string, service: string | null | undefined): string {
  if (service && service.trim()) return service.trim().toLowerCase();
  const h = handle.trim();
  if (h.includes("@") && !h.startsWith("@")) return "email";
  if (/^\+?\d[\d\s().-]{6,}$/.test(h)) return "phone";
  return "unknown";
}

function yearRangeLabel(
  handles: ContactDetail["handles"],
): string | null {
  let minY: number | null = null;
  let maxY: number | null = null;
  for (const h of handles) {
    if (h.start_date) {
      const y = new Date(h.start_date).getFullYear();
      if (!Number.isNaN(y)) minY = minY === null ? y : Math.min(minY, y);
    }
    if (h.end_date) {
      const y = new Date(h.end_date).getFullYear();
      if (!Number.isNaN(y)) maxY = maxY === null ? y : Math.max(maxY, y);
    }
  }
  if (minY === null && maxY === null) return null;
  if (minY === null) return String(maxY);
  if (maxY === null) return String(minY);
  return minY === maxY ? String(minY) : `${minY}–${maxY}`;
}

export default function ContactDrawer({
  contactId,
  preview = null,
  onClose,
  onBrowseConversations,
}: {
  contactId: string | null;
  preview?: ContactPreview | null;
  onClose: () => void;
  onBrowseConversations?: (args: {
    contactId: string;
    kind: ContactBrowseKind;
    handles?: string[];
  }) => void;
}) {
  const [detail, setDetail] = useState<ContactDetail | null>(null);
  const [editingName, setEditingName] = useState(false);
  const [nameValue, setNameValue] = useState("");
  const [newHandle, setNewHandle] = useState("");
  const [newService, setNewService] = useState("discord");

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
    invalidateContactDetail(contactId);
    void fetchContactDetail(contactId, (path, opts) =>
      apiClient.get<ContactDetail>(path, opts),
    )
      .then((next) => {
        if (String(next.id) !== String(contactId)) return;
        setDetail(next);
      })
      .catch(() => {
        /* keep preview; detail stays unset */
      });
  };

  useEffect(() => {
    setEditingName(false);
    setNewHandle("");
    if (!contactId) {
      setDetail(null);
      return;
    }

    const cached = getCachedContactDetail(contactId);
    setDetail(cached);

    const ac = new AbortController();
    // Cache hit: skip network. Miss: fetch and store.
    if (!cached) {
      void fetchContactDetail(
        contactId,
        (path, opts) => apiClient.get<ContactDetail>(path, opts),
        ac.signal,
      )
        .then((next) => {
          if (ac.signal.aborted) return;
          setDetail(next);
        })
        .catch(() => {
          /* aborted or failed — preview still shown */
        });
    }
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

  const addHandle = async () => {
    if (!newHandle.trim()) return;
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: newHandle.trim(), service: newService },
      });
      setNewHandle("");
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

  const years = detailMatches ? yearRangeLabel(detail!.handles) : null;
  const totalMessages = detailMatches ? detail!.total_messages : null;
  const directCount = detailMatches ? detail!.direct_conversations : null;
  const groupCount = detailMatches ? detail!.group_conversations : null;

  const browse = (kind: ContactBrowseKind) => {
    if (!onBrowseConversations || !contactId) return;
    onBrowseConversations({
      contactId,
      kind,
      handles: handleRows.map((h) => h.handle).filter(Boolean),
    });
  };

  const browseLinkStyle: CSSProperties = {
    background: "none",
    border: "none",
    padding: 0,
    margin: 0,
    font: "inherit",
    fontWeight: 600,
    color: "var(--accent)",
    cursor: onBrowseConversations ? "pointer" : "default",
    textAlign: "left",
    display: "block",
    marginBottom: "0.25rem",
    textDecoration: "none",
  };

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
                backgroundColor: "var(--elevated)",
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
          <div style={{ marginBottom: "0.75rem" }}>
            {handleRows.map((h, i) => (
              <div
                key={`${h.handle}-${i}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.75rem",
                  padding: "0.375rem 0",
                  borderBottom: "1px solid var(--border)",
                  fontSize: "0.875rem",
                }}
              >
                <span style={{ color: "var(--muted)", minWidth: "5.5rem", flexShrink: 0 }}>
                  {inferService(h.handle, h.service)}
                </span>
                <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
                  {h.handle}
                </span>
              </div>
            ))}
          </div>
        )}

        <div
          style={{
            display: "flex",
            gap: "0.5rem",
            flexWrap: "wrap",
            marginBottom: "0.35rem",
            alignItems: "center",
          }}
        >
          <select
            value={newService}
            onChange={(e) => setNewService(e.target.value)}
            style={{
              padding: "0.375rem 0.5rem",
              fontSize: "0.813rem",
              border: "1px solid var(--border)",
              borderRadius: "4px",
              width: "110px",
              backgroundColor: "var(--elevated)",
              color: "var(--text)",
            }}
          >
            {SERVICES.map((s) => <option key={s} value={s}>{s}</option>)}
          </select>
          <input
            type="text"
            value={newHandle}
            onChange={(e) => setNewHandle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void addHandle();
              }
            }}
            placeholder="user#1234, @handle…"
            style={{
              flex: 1,
              minWidth: 0,
              padding: "0.375rem 0.5rem",
              fontSize: "0.813rem",
              border: "1px solid var(--border)",
              borderRadius: "4px",
              backgroundColor: "var(--elevated)",
              color: "var(--text)",
            }}
          />
          <Button
            variant="primary"
            onClick={addHandle}
            disabled={!newHandle.trim() || loading}
            style={{ fontSize: "0.813rem", padding: "0.25rem 0.75rem" }}
          >
            Add
          </Button>
        </div>

        <div style={{ marginTop: "1.25rem", fontSize: "0.875rem" }}>
          <h3
            style={{
              fontSize: "0.75rem",
              color: "var(--muted)",
              textTransform: "uppercase",
              marginBottom: "0.35rem",
            }}
          >
            Messages
          </h3>
          {detailMatches ? (
            <>
              {years ? (
                <div style={{ color: "var(--muted)", marginBottom: "0.5rem" }}>
                  {years}
                </div>
              ) : null}
              <button
                type="button"
                className="contact-drawer-browse-link"
                onClick={() => browse("direct")}
                style={browseLinkStyle}
              >
                {directCount} direct conversation{directCount === 1 ? "" : "s"}
              </button>
              <button
                type="button"
                className="contact-drawer-browse-link"
                onClick={() => browse("group")}
                style={browseLinkStyle}
              >
                {groupCount} group conversation{groupCount === 1 ? "" : "s"}
              </button>
              <button
                type="button"
                className="contact-drawer-browse-link"
                onClick={() => browse("all")}
                style={{ ...browseLinkStyle, marginBottom: 0 }}
              >
                {totalMessages} total message{totalMessages === 1 ? "" : "s"}
              </button>
            </>
          ) : (
            <div style={{ color: "var(--muted)" }}>Loading details…</div>
          )}
        </div>
      </div>
    </>
  );
}
