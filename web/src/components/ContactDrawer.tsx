import { useState, useEffect, useLayoutEffect, type CSSProperties } from "react";
import { apiClient } from "../lib/api";
import {
  fetchContactDetail,
  getCachedContactDetail,
  invalidateContactDetail,
  type CachedContactDetail,
} from "../lib/contactDetailCache";
import { ContactDrawerHandles } from "./contactDrawer/ContactDrawerHandles";
import {
  type ContactPreview,
  type ContactBrowseKind,
  yearRangeLabel,
} from "./contactDrawer/contactDrawerTypes";

export type { ContactPreview, ContactBrowseKind };

type ContactDetail = CachedContactDetail;

const DRAWER_WIDTH = 320;

/** Dock the drawer to the right edge of the list/contact column when present. */
function useDrawerLeft(open: boolean): number | null {
  const [left, setLeft] = useState<number | null>(null);

  useLayoutEffect(() => {
    if (!open) {
      setLeft(null);
      return;
    }

    let frame = 0;
    let observer: ResizeObserver | null = null;

    const measure = () => {
      const col = document.querySelector<HTMLElement>("[data-list-column]");
      if (col) {
        setLeft(Math.round(col.getBoundingClientRect().right));
        return col;
      }
      setLeft(null);
      return null;
    };

    const col = measure();
    if (col) {
      observer = new ResizeObserver(() => {
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(measure);
      });
      observer.observe(col);
    }

    const onResize = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };
    window.addEventListener("resize", onResize);

    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", onResize);
      observer?.disconnect();
    };
  }, [open]);

  return left;
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
  const drawerLeft = useDrawerLeft(!!contactId);

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
      <div
        style={{
          position: "fixed",
          top: 0,
          bottom: 0,
          left: drawerLeft != null ? drawerLeft : undefined,
          right: drawerLeft == null ? 0 : undefined,
          width: `${DRAWER_WIDTH}px`,
          background: "var(--panel)",
          boxShadow: "2px 0 12px rgba(0,0,0,0.18)",
          zIndex: 50,
          overflow: "auto",
          padding: "1.5rem",
          borderRight: "1px solid var(--border)",
        }}
      >
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

        <ContactDrawerHandles
          contactId={contactId}
          handleRows={handleRows}
          loading={loading}
          onHandlesChanged={loadDetail}
        />

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
