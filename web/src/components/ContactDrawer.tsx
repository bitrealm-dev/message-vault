import { useState, useEffect, useLayoutEffect } from "react";
import { apiClient } from "../lib/api";
import {
  fetchContactDetail,
  getCachedContactDetail,
  invalidateContactDetail,
  type CachedContactDetail,
} from "../lib/contactDetailCache";
import Button from "./Button";
import { PencilIcon } from "./icons";
import { ContactDrawerHandles } from "./contactDrawer/ContactDrawerHandles";
import {
  type ContactPreview,
  type ContactBrowseKind,
  emptyHandleRow,
} from "./contactDrawer/contactDrawerTypes";

export type { ContactPreview, ContactBrowseKind };

type ContactDetail = CachedContactDetail;

const iconBtnClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !border-transparent !bg-transparent !p-0 !font-normal !leading-none !text-muted hover:!border-border hover:!bg-elevated hover:!text-text data-hovered:!border-border data-hovered:!bg-elevated data-hovered:!text-text data-pressed:!border-border data-pressed:!bg-hover";

/**
 * Overlay mode only: dock to the right edge of the list column.
 * Skips setState when the measured edge is unchanged to avoid jitter.
 */
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
      if (!col) {
        setLeft(null);
        return null;
      }
      const next = Math.round(col.getBoundingClientRect().right);
      setLeft((prev) => (prev === next ? prev : next));
      return col;
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
  /** `docked` = flex sibling (contacts page). `overlay` = fixed panel (e.g. from messages). */
  variant = "overlay",
}: {
  contactId: string | null;
  preview?: ContactPreview | null;
  onClose: () => void;
  onBrowseConversations?: (args: {
    contactId: string;
    kind: ContactBrowseKind;
    handle?: string;
    service?: string;
    handles?: string[];
  }) => void;
  variant?: "docked" | "overlay";
}) {
  const [detail, setDetail] = useState<ContactDetail | null>(null);
  const [editingName, setEditingName] = useState(false);
  const [nameValue, setNameValue] = useState("");
  const drawerLeft = useDrawerLeft(variant === "overlay" && !!contactId);

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
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (editingName) {
        setEditingName(false);
        setNameValue(detailMatches ? detail!.name : nameValue);
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [contactId, editingName, detailMatches, detail, nameValue, onClose]);

  if (!contactId) return null;

  const handleRows: ContactDetail["handles"] = detailMatches
    ? detail!.handles
    : (previewMatches ? preview!.handles : undefined)?.map((h) => emptyHandleRow(h)) ??
      [];

  const browse = (args: {
    kind: ContactBrowseKind;
    handle?: string;
    service?: string;
  }) => {
    if (!onBrowseConversations || !contactId) return;
    onBrowseConversations({
      contactId,
      kind: args.kind,
      handle: args.handle,
      service: args.service,
      handles: handleRows.map((h) => h.handle).filter(Boolean),
    });
  };

  const saveName = async () => {
    if (!detailMatches || nameValue === detail!.name) {
      setEditingName(false);
      return;
    }
    await apiClient.post(`/v1/export/contacts/${contactId}`, {
      name: nameValue,
    });
    setEditingName(false);
    loadDetail();
  };

  const panelClass =
    variant === "docked"
      ? "flex min-h-0 min-w-0 flex-1 flex-col overflow-auto border-l border-border bg-panel p-6 outline-none"
      : "fixed top-0 bottom-0 z-40 w-[min(920px,calc(100vw-14rem))] overflow-auto border-l border-border bg-panel p-6 shadow-[2px_0_12px_rgba(0,0,0,0.18)] outline-none";

  const panelStyle =
    variant === "overlay"
      ? {
          left: drawerLeft ?? undefined,
          right: drawerLeft == null ? 0 : undefined,
        }
      : undefined;

  return (
    <aside
      role="dialog"
      aria-label={displayName}
      className={panelClass}
      style={panelStyle}
    >
      <div className="mb-5 flex items-start justify-between gap-3">
        {editingName && detailMatches ? (
          <input
            type="text"
            value={nameValue}
            onChange={(e) => setNameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void saveName();
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setEditingName(false);
                setNameValue(detail!.name);
              }
            }}
            onBlur={() => {
              void saveName();
            }}
            autoFocus
            className="box-border min-w-0 flex-1 rounded border border-border bg-elevated p-1 text-[1.125rem] font-semibold text-text"
          />
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <h2 className="m-0 min-w-0 truncate text-[1.125rem] font-semibold">
              {displayName}
            </h2>
            {detailMatches ? (
              <Button
                variant="ghost"
                title="Edit name"
                aria-label="Edit name"
                onClick={() => setEditingName(true)}
                className={iconBtnClass}
              >
                <PencilIcon />
              </Button>
            ) : null}
          </div>
        )}
        <button
          type="button"
          aria-label="Close"
          onClick={onClose}
          className="shrink-0 cursor-pointer border-none bg-transparent p-0 text-[1.25rem] leading-none text-muted outline-none hover:text-text"
        >
          ×
        </button>
      </div>

      <ContactDrawerHandles
        contactId={contactId}
        handleRows={handleRows}
        loading={loading}
        onHandlesChanged={loadDetail}
        onBrowse={onBrowseConversations ? browse : undefined}
      />
    </aside>
  );
}
