import { useState, useEffect, useCallback, useRef, type ReactNode } from "react";
import { apiClient } from "../lib/api";
import ContactInitialCircle from "../components/ContactInitialCircle";
import VirtualList, { type VisibleRange } from "../components/VirtualList";
import { listRowDividers } from "../lib/tw";
import {
  formatVisibleRange,
  PAGE_SIZE_CONTACTS_FIRST,
  PAGE_SIZE_FIRST,
  usePagedList,
  type PagedFetchPage,
} from "../lib/usePagedList";

const FILTER_DEBOUNCE_MS = 300;
/** Fixed row height keeps virtualization slots aligned with flex-centered content. */
const CONTACT_ROW_HEIGHT = 49;

interface Contact {
  id: string;
  name: string;
  handle_count: number;
  handles?: string[];
}

type ContactsPage = {
  contacts: Array<Omit<Contact, "id"> & { id: string | number }>;
  total: number;
  limit: number;
  offset: number;
};

type FilterNeedles = { text: string; handle: string | null };

/** Strip advanced tokens; keep plain text + handle:"…" for subtitle matching. */
function filterNeedles(raw: string): FilterNeedles {
  let q = raw.trim();
  if (!q) return { text: "", handle: null };

  let handle: string | null = null;
  const quoted = q.match(/\bhandle:"([^"]+)"/i);
  const bare = q.match(/\bhandle:(\S+)/i);
  if (quoted) {
    handle = quoted[1];
    q = q.replace(quoted[0], " ");
  } else if (bare) {
    handle = bare[1].replace(/^"|"$/g, "");
    q = q.replace(bare[0], " ");
  }

  q = q
    .replace(/\bsearch:contacts\b/gi, " ")
    .replace(/\b(first-contact|last-contact|message-count|group-count):\S+/gi, " ")
    .replace(/\s+/g, " ")
    .trim();

  return { text: q, handle };
}

function handleMatchesNeedle(handle: string, needle: string): boolean {
  const n = needle.trim().toLowerCase();
  if (!n) return false;
  return handle.toLowerCase().includes(n);
}

function matchingHandles(handles: string[] | undefined, filter: string): string[] {
  const { text, handle } = filterNeedles(filter);
  if (!text && !handle) return [];
  return (handles ?? []).filter((h) => {
    if (handle && handleMatchesNeedle(h, handle)) return true;
    if (text && handleMatchesNeedle(h, text)) return true;
    return false;
  });
}

function highlightNeedle(filter: string): string {
  const { text, handle } = filterNeedles(filter);
  return handle || text;
}

/** Client-side match so the list can shrink/expand as the user types without waiting on the API. */
function contactMatchesFilter(c: Contact, filter: string): boolean {
  const { text, handle } = filterNeedles(filter);
  if (!text && !handle) return true;
  if (text && c.name.toLowerCase().includes(text.toLowerCase())) return true;
  return matchingHandles(c.handles, filter).length > 0;
}

function highlightText(text: string, term: string): ReactNode[] {
  const t = term.trim().toLowerCase();
  if (!t) return [text];
  const out: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (true) {
    const idx = rest.toLowerCase().indexOf(t);
    if (idx === -1) {
      out.push(rest);
      break;
    }
    if (idx > 0) out.push(rest.slice(0, idx));
    out.push(
      <mark
        key={key++}
        className="rounded-sm bg-search-mark px-px"
      >
        {rest.slice(idx, idx + t.length)}
      </mark>,
    );
    rest = rest.slice(idx + t.length);
  }
  return out;
}

function normalizeContacts(
  rows: ContactsPage["contacts"] | undefined,
): Contact[] {
  return (rows || []).map((c) => ({
    ...c,
    id: String(c.id),
    handles: c.handles ?? [],
  }));
}

export default function ContactList({
  filter = "",
  selectedId = null,
  onSelect,
}: {
  filter?: string;
  selectedId?: string | null;
  onSelect: (contact: Contact) => void;
}) {
  const [serverQ, setServerQ] = useState("");
  const [visibleRange, setVisibleRange] = useState<VisibleRange>({ start: 0, end: 0 });
  const catalogCompleteRef = useRef(false);

  const fetchPage = useCallback<PagedFetchPage<Contact>>(
    async ({ limit, offset, signal }) => {
      const params = new URLSearchParams({
        q: serverQ,
        limit: String(limit),
        offset: String(offset),
      });
      const res = await apiClient.get<ContactsPage>(
        `/v1/export/contacts?${params}`,
        { signal },
      );
      return {
        items: normalizeContacts(res.contacts),
        total: res.total ?? 0,
      };
    },
    [serverQ],
  );

  const {
    items: contacts,
    total,
    loading,
    refreshing,
    filling,
    error,
    hasMore,
    loadMore,
  } = usePagedList(serverQ, fetchPage, {
    firstPageSize: serverQ.trim() ? PAGE_SIZE_FIRST : PAGE_SIZE_CONTACTS_FIRST,
  });

  const catalogComplete =
    !loading && !refreshing && contacts.length >= total && (total > 0 || contacts.length === 0);
  catalogCompleteRef.current = catalogComplete && !serverQ.trim();

  useEffect(() => {
    // Empty filter → catalog query.
    if (!filter.trim()) {
      setServerQ("");
      return;
    }
    // Full catalog already in memory → client filter only (Next-like).
    if (catalogCompleteRef.current) return;

    const t = window.setTimeout(() => setServerQ(filter), FILTER_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [filter, catalogComplete]);

  const filterActive = filter.trim().length > 0;
  const needles = filterNeedles(filter);
  /** Prefer plain text for names; fall back to handle needle when that is all the user typed. */
  const nameMarkTerm = needles.text || needles.handle || "";
  const handleMarkTerm = highlightNeedle(filter);

  // Live client filter for immediate feedback.
  const displayContacts = filterActive
    ? contacts.filter((c) => contactMatchesFilter(c, filter))
    : contacts;

  const rangeLabel =
    loading && contacts.length === 0
      ? "Loading…"
      : formatVisibleRange(
          visibleRange.start,
          visibleRange.end,
          filterActive && (catalogCompleteRef.current || !serverQ.trim())
            ? displayContacts.length
            : total,
          displayContacts.length,
        );

  if (error && contacts.length === 0) {
    return (
      <div className="p-4 text-[0.813rem] text-danger">
        Could not load contacts: {error}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border px-3 py-1.5 text-[0.688rem] text-muted">
        {rangeLabel}
        {refreshing ? " · updating…" : filling ? " · loading more…" : null}
      </div>
      <VirtualList
        count={displayContacts.length}
        estimateSize={CONTACT_ROW_HEIGHT}
        // Expanded filter subtitles need dynamic measure; plain rows stay fixed.
        dynamicSize={filterActive}
        onVisibleRangeChange={setVisibleRange}
        onNearEnd={() => {
          if (hasMore && !filterActive) loadMore();
          // When filtering a partial catalog, also load more so matches can appear.
          if (hasMore && filterActive && !catalogCompleteRef.current && !serverQ.trim()) {
            loadMore();
          }
        }}
        empty={
          !loading ? (
            <div className="p-4 text-[0.813rem] text-muted">
              {filterActive ? "No contacts match this filter" : "No contacts"}
            </div>
          ) : null
        }
        renderItem={(index) => {
          const c = displayContacts[index];
          if (!c) return null;
          // If the preferred name is itself a handle, don't repeat it under the name.
          const nameKey = c.name.trim().toLowerCase();
          const shownHandles = filterActive
            ? matchingHandles(c.handles, filter).filter(
                (h) => h.trim().toLowerCase() !== nameKey,
              )
            : [];
          return (
            <button
              type="button"
              onClick={() => onSelect(c)}
              style={{
                height: filterActive ? "auto" : "100%",
                minHeight: filterActive ? CONTACT_ROW_HEIGHT : undefined,
              }}
              className={`box-border flex w-full cursor-pointer items-center gap-2.5 border-none p-2 px-3 text-left text-text ${listRowDividers} ${
                c.id === selectedId ? "bg-hover" : "bg-transparent"
              }`}
            >
              <span className="flex h-7 w-7 shrink-0 items-center justify-center self-center">
                <ContactInitialCircle
                  displayName={c.name}
                  preferredHandle={c.handles?.[0] ?? null}
                />
              </span>
              <div className="min-w-0 flex-1">
                <div className="truncate text-[0.875rem] font-medium">
                  {filterActive && nameMarkTerm
                    ? highlightText(c.name, nameMarkTerm)
                    : c.name}
                </div>
                {shownHandles.length > 0 && (
                  <div className="mt-0.5">
                    {shownHandles.map((h) => (
                      <div
                        key={h}
                        className="truncate text-[0.75rem] text-muted"
                      >
                        {highlightText(h, handleMarkTerm)}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </button>
          );
        }}
      />
    </div>
  );
}
