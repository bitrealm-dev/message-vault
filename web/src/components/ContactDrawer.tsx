import { useState, useEffect, useLayoutEffect } from "react";
import { ModalOverlay, Modal, Dialog, Button } from "react-aria-components";
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

  const browseLinkClass = `block border-none bg-transparent p-0 font-[inherit] font-semibold text-accent text-left no-underline ${
    onBrowseConversations ? "cursor-pointer" : "cursor-default"
  }`;

  return (
    <ModalOverlay
      isOpen={!!contactId}
      isDismissable
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      className="fixed inset-0 z-40 bg-[rgba(0,0,0,0.2)]"
    >
      <Modal
        className="fixed top-0 bottom-0 z-50 w-[320px] overflow-auto border-l border-border bg-panel p-6 shadow-[2px_0_12px_rgba(0,0,0,0.18)] outline-none"
        style={{ left: drawerLeft ?? undefined, right: drawerLeft == null ? 0 : undefined }}
      >
        <Dialog aria-label={displayName} className="outline-none">
          <div className="mb-4 flex justify-between gap-2">
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
              className="box-border w-full rounded border border-border bg-elevated p-1 text-[1.125rem] font-semibold text-text"
            />
          ) : (
            <h2
              onClick={() => {
                if (detailMatches) setEditingName(true);
              }}
              className={`m-0 min-w-0 text-[1.125rem] ${
                detailMatches ? "cursor-pointer" : "cursor-default"
              }`}
              title={detailMatches ? "Click to edit" : undefined}
            >
              {displayName}
              {detailMatches ? " ✎" : null}
            </h2>
          )}
          <Button
            slot="close"
            aria-label="Close"
            className="shrink-0 cursor-pointer border-none bg-transparent p-0 text-[1.25rem] leading-none text-muted outline-none data-hovered:text-text"
          >
            ×
          </Button>
          </div>

          <ContactDrawerHandles
          contactId={contactId}
          handleRows={handleRows}
          loading={loading}
          onHandlesChanged={loadDetail}
        />

        <div className="mt-5 text-[0.875rem]">
          <h3 className="mb-[0.35rem] text-[0.75rem] uppercase text-muted">
            Messages
          </h3>
          {detailMatches ? (
            <>
              {years ? (
                <div className="mb-2 text-muted">
                  {years}
                </div>
              ) : null}
              <button
                type="button"
                className={`contact-drawer-browse-link mb-1 ${browseLinkClass}`}
                onClick={() => browse("direct")}
              >
                {directCount} direct conversation{directCount === 1 ? "" : "s"}
              </button>
              <button
                type="button"
                className={`contact-drawer-browse-link mb-1 ${browseLinkClass}`}
                onClick={() => browse("group")}
              >
                {groupCount} group conversation{groupCount === 1 ? "" : "s"}
              </button>
              <button
                type="button"
                className={`contact-drawer-browse-link ${browseLinkClass}`}
                onClick={() => browse("all")}
              >
                {totalMessages} total message{totalMessages === 1 ? "" : "s"}
              </button>
            </>
          ) : (
            <div className="text-muted">Loading details…</div>
          )}
        </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
