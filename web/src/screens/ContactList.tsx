import { useState, useEffect, useCallback, useRef } from "react";
import { apiClient } from "../lib/api";
import ContactInitialCircle from "../components/ContactInitialCircle";
import InfiniteOffsetList from "../components/InfiniteOffsetList";
import { highlightText } from "../lib/highlightText";
import {
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

/** Search words that the client cannot apply locally; those go to the server. */
const ADVANCED_TOKEN_RE =
  /\b(search:contacts|has:(?:messages|no-messages|no-name)|(?:first-contact|last-contact|message-count|group-count|service):\S+)\b/gi;

/** True when the filter uses search words the client cannot apply on its own. */
function hasAdvancedContactTokens(raw: string): boolean {
  ADVANCED_TOKEN_RE.lastIndex = 0;
  return ADVANCED_TOKEN_RE.test(raw);
}

/** Pull plain name text and a handle:"…" value out of the filter for local matching. */
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
    .replace(ADVANCED_TOKEN_RE, " ")
    .replace(/\s+/g, " ")
    .trim();

  return { text: q, handle };
}

/** True when this handle contains the search text. */
function handleMatchesNeedle(handle: string, needle: string): boolean {
  const n = needle.trim().toLowerCase();
  if (!n) return false;
  return handle.toLowerCase().includes(n);
}

/** Handles on this contact that match the current filter. */
function matchingHandles(handles: string[] | undefined, filter: string): string[] {
  const { text, handle } = filterNeedles(filter);
  if (!text && !handle) return [];
  return (handles ?? []).filter((h) => {
    if (handle && handleMatchesNeedle(h, handle)) return true;
    if (text && handleMatchesNeedle(h, text)) return true;
    return false;
  });
}

/** Text to highlight in handle subtitles. */
function highlightNeedle(filter: string): string {
  const { text, handle } = filterNeedles(filter);
  return handle || text;
}

/** True when this contact matches the typed filter, so the list can shrink as the user types. */
function contactMatchesFilter(c: Contact, filter: string): boolean {
  const { text, handle } = filterNeedles(filter);
  if (!text && !handle) return true;
  if (text && c.name.toLowerCase().includes(text.toLowerCase())) return true;
  return matchingHandles(c.handles, filter).length > 0;
}

/** Make every contact id a string so list keys stay stable. */
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
    loadMore: requestMore,
  } = usePagedList(serverQ, fetchPage, {
    firstPageSize: serverQ.trim() ? PAGE_SIZE_FIRST : PAGE_SIZE_CONTACTS_FIRST,
  });

  const catalogComplete =
    !loading && !refreshing && contacts.length >= total && (total > 0 || contacts.length === 0);
  catalogCompleteRef.current = catalogComplete && !serverQ.trim();

  const advancedActive = hasAdvancedContactTokens(filter);

  useEffect(() => {
    // Empty filter: load the full catalog.
    if (!filter.trim()) {
      setServerQ("");
      return;
    }
    // Filters the client cannot apply always go to the server.
    if (advancedActive) {
      const t = window.setTimeout(() => setServerQ(filter), FILTER_DEBOUNCE_MS);
      return () => window.clearTimeout(t);
    }
    // The full catalog is already in memory, so filter it locally.
    if (catalogCompleteRef.current) return;

    const t = window.setTimeout(() => setServerQ(filter), FILTER_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [filter, catalogComplete, advancedActive]);

  const filterActive = filter.trim().length > 0;
  const needles = filterNeedles(filter);
  /** Prefer plain text for names. Fall back to the handle when that is all the user typed. */
  const nameMarkTerm = needles.text || needles.handle || "";
  const handleMarkTerm = highlightNeedle(filter);

  // Filter by name and handle in the browser. Server results are used when the
  // filter has search words the client cannot apply.
  const displayContacts =
    filterActive && !advancedActive
      ? contacts.filter((c) => contactMatchesFilter(c, filter))
      : contacts;

  const rangeTotal =
    filterActive &&
    !advancedActive &&
    (catalogCompleteRef.current || !serverQ.trim())
      ? displayContacts.length
      : total;

  return (
    <InfiniteOffsetList
      items={displayContacts}
      total={total}
      rangeTotal={rangeTotal}
      loading={loading}
      refreshing={refreshing}
      filling={filling}
      error={error}
      hasMore={hasMore}
      requestMore={requestMore}
      estimateSize={CONTACT_ROW_HEIGHT}
      dynamicSize={filterActive}
      selectedId={selectedId}
      onSelect={onSelect}
      getId={(c) => c.id}
      getTextValue={(c) => c.name}
      ariaLabel="Contacts"
      errorPrefix="Could not load contacts"
      empty={
        !loading ? (
          <div className="p-4 text-[0.813rem] text-muted">
            {filterActive ? "No contacts match this filter" : "No contacts"}
          </div>
        ) : null
      }
      renderRow={(c) => {
        const nameKey = c.name.trim().toLowerCase();
        const shownHandles = filterActive
          ? matchingHandles(c.handles, filter).filter(
              (h) => h.trim().toLowerCase() !== nameKey,
            )
          : [];
        return (
          <>
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
          </>
        );
      }}
    />
  );
}
