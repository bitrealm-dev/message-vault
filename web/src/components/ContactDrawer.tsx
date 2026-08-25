import { useEffect, useLayoutEffect, useState } from "react";
import { apiClient } from "../lib/api";
import {
  type CachedContactDetail,
  CONTACT_DETAIL_CHANGED_EVENT,
  fetchContactDetail,
  getCachedContactDetail,
  invalidateContactDetail,
} from "../lib/contactDetailCache";
import Button from "./Button";
import { ContactDrawerHandles } from "./contactDrawer/ContactDrawerHandles";
import {
  type ContactBrowseKind,
  type ContactPreview,
  previewHandleStubRows,
} from "./contactDrawer/contactDrawerTypes";
import { PencilIcon } from "./icons";

type ContactDetail = CachedContactDetail;

const iconBtnClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !border-transparent !bg-transparent !p-0 !font-normal !leading-none !text-muted hover:!border-border hover:!bg-elevated hover:!text-text data-hovered:!border-border data-hovered:!bg-elevated data-hovered:!text-text data-pressed:!border-border data-pressed:!bg-hover disabled:pointer-events-none disabled:hover:!border-transparent disabled:hover:!bg-transparent disabled:hover:!text-muted";

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

  // Prefer in-state detail only when it matches this contact; otherwise use cache
  // during render so a cache hit never paints a loading flash.
  const matchedDetail =
    contactId && detail && String(detail.id) === String(contactId)
      ? detail
      : contactId
        ? getCachedContactDetail(contactId)
        : null;
  const detailMatches = !!matchedDetail;
  const previewMatches = !!contactId && !!preview && String(preview.id) === String(contactId);
  const matchedName = matchedDetail?.name;

  const displayName = detailMatches
    ? matchedDetail.name
    : previewMatches
      ? preview?.name
      : "Loading…";
  const loading = !detailMatches;

  const loadDetail = () => {
    if (!contactId) return;
    invalidateContactDetail(contactId);
    void fetchContactDetail(contactId, (path, opts) => apiClient.get<ContactDetail>(path, opts))
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
    if (!contactId) return;
    const onChange = (e: Event) => {
      const ce = e as CustomEvent<{ id?: string; groups?: string[] }>;
      if (String(ce.detail?.id) !== String(contactId)) return;
      const groups = ce.detail?.groups;
      if (!groups) return;
      setDetail((prev) => {
        if (!prev || String(prev.id) !== String(contactId)) return prev;
        return { ...prev, groups };
      });
    };
    globalThis.addEventListener(CONTACT_DETAIL_CHANGED_EVENT, onChange);
    return () => globalThis.removeEventListener(CONTACT_DETAIL_CHANGED_EVENT, onChange);
  }, [contactId]);

  useEffect(() => {
    setNameValue(displayName === "Loading…" ? "" : displayName);
    setEditingName(false);
  }, [displayName]);

  useEffect(() => {
    if (!contactId) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (editingName) {
        setEditingName(false);
        setNameValue(detailMatches ? (matchedName ?? nameValue) : nameValue);
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [contactId, editingName, detailMatches, matchedName, nameValue, onClose]);

  if (!contactId) return null;

  const handleRows: ContactDetail["handles"] = detailMatches
    ? matchedDetail.handles
    : previewMatches
      ? previewHandleStubRows(preview?.handles, preview?.handleCount)
      : [];

  // null = membership unknown (loading, no preview groups); [] = known empty.
  const displayGroups: string[] | null = detailMatches
    ? (matchedDetail.groups ?? [])
    : previewMatches && preview?.groups != null
      ? preview.groups
      : loading
        ? null
        : [];

  const browse = (args: { kind: ContactBrowseKind; handle?: string; service?: string }) => {
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
    if (!detailMatches || nameValue === matchedDetail?.name) {
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
      ? "flex h-full min-h-0 min-w-0 flex-col overflow-auto [scrollbar-gutter:stable] bg-panel px-6 pb-6 pt-2 outline-none"
      : "fixed top-0 bottom-0 z-40 w-[min(920px,calc(100vw-14rem))] overflow-auto [scrollbar-gutter:stable] border-l border-border bg-panel p-6 shadow-[2px_0_12px_rgba(0,0,0,0.18)] outline-none";

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
      aria-busy={loading || undefined}
      className={panelClass}
      style={panelStyle}
    >
      <ContactDrawerHandles
        contactId={contactId}
        handleRows={handleRows}
        loading={loading}
        onHandlesChanged={loadDetail}
        onBrowse={onBrowseConversations ? browse : undefined}
        title={
          editingName && detailMatches ? (
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
                  setNameValue(matchedDetail?.name);
                }
              }}
              onBlur={() => {
                void saveName();
              }}
              className="box-border w-full min-w-0 rounded border border-border bg-elevated p-1 text-[1.125rem] font-semibold text-text"
            />
          ) : (
            <div className="flex min-w-0 items-center gap-2">
              <h2 className="m-0 min-w-0 truncate text-[1.125rem] font-semibold">{displayName}</h2>
              <Button
                variant="ghost"
                title="Edit name"
                aria-label="Edit name"
                disabled={!detailMatches}
                onClick={() => setEditingName(true)}
                className={iconBtnClass}
              >
                <PencilIcon />
              </Button>
            </div>
          )
        }
        intro={
          <div>
            <div className="mb-1.5">
              <span className="text-[0.75rem] font-semibold uppercase tracking-[0.04em] text-muted">
                Contact groups
              </span>
            </div>
            <div className="flex min-h-6 flex-wrap items-center gap-1.5">
              {displayGroups == null ? (
                <span className="py-0.5 text-[0.75rem] leading-4 text-muted" aria-hidden>
                  …
                </span>
              ) : displayGroups.length > 0 ? (
                displayGroups.map((name) => (
                  <span
                    key={name}
                    className="rounded-full bg-elevated px-2 py-0.5 text-[0.75rem] leading-4 text-text"
                  >
                    {name}
                  </span>
                ))
              ) : (
                <span className="py-0.5 text-[0.75rem] leading-4 text-muted">No groups</span>
              )}
            </div>
          </div>
        }
        toolbarExtra={
          <button
            type="button"
            aria-label="Close"
            onClick={onClose}
            className="shrink-0 cursor-pointer border-none bg-transparent p-0 text-[1.25rem] leading-none text-muted outline-none hover:text-text"
          >
            ×
          </button>
        }
      />
    </aside>
  );
}
