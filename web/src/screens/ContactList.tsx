import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { apiClient } from "../lib/api";
import ContactInitialCircle from "../components/ContactInitialCircle";
import ContactSortMenu from "../components/ContactSortMenu";
import GroupsMenu from "../components/GroupsMenu";
import { checksFromMembers } from "../lib/membershipChecks";
import InfiniteOffsetList from "../components/InfiniteOffsetList";
import { highlightText } from "../lib/highlightText";
import {
  compareContactsByName,
  contactSortLetter,
  loadContactNameSort,
  saveContactNameSort,
  type ContactNameSortState,
} from "../lib/contactSort";
import {
  GROUP_FILTER_TOKEN_RE,
  contactBelongsToGroup,
  createContactGroup,
  groupListQuery,
  hasGroupFilterToken,
  setContactGroupMembership,
} from "../lib/contactGroups";
import { useContactGroups } from "../lib/useContactGroups";
import { invalidateContactDetail } from "../lib/contactDetailCache";
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
  groups?: string[];
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
  /\b(search:contacts|has:(?:messages|no-messages|no-name|no-label|no-group)|(?:first-contact|last-contact|message-count|group-count|service):\S+)\b/gi;

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
    .replace(GROUP_FILTER_TOKEN_RE, " ")
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
    groups: c.groups ?? [],
  }));
}

export default function ContactList({
  filter = "",
  groupFilter = null,
  selectedId = null,
  onSelect,
  onCheckedChange,
}: {
  filter?: string;
  /** Named group, or `"none"` for contacts with no group. */
  groupFilter?: string | "none" | null;
  selectedId?: string | null;
  onSelect: (contact: Contact) => void;
  /** Checked rows, so the right panel can list them. */
  onCheckedChange?: (contacts: Contact[]) => void;
}) {
  const [serverQ, setServerQ] = useState("");
  const [membershipRev, setMembershipRev] = useState(0);
  const [groupOverrides, setGroupOverrides] = useState<Record<string, string[]>>(
    {},
  );
  const [nameSort, setNameSort] = useState<ContactNameSortState>(() =>
    loadContactNameSort(),
  );
  const [checkedIds, setCheckedIds] = useState<Set<string>>(() => new Set());
  /** Ignores the row click that follows a checkbox press (nested control). */
  const skipRowSelectRef = useRef(false);
  const catalogCompleteRef = useRef(false);
  /** Unfiltered contact list, so group clicks can filter in the browser. */
  const fullCatalogRef = useRef<Contact[] | null>(null);
  const { groups: allGroups } = useContactGroups();

  const onNameSortChange = (next: ContactNameSortState) => {
    setNameSort(next);
    saveContactNameSort(next);
  };

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
  } = usePagedList(`${serverQ}#${membershipRev}`, fetchPage, {
    firstPageSize: serverQ.trim() ? PAGE_SIZE_FIRST : PAGE_SIZE_CONTACTS_FIRST,
  });

  const catalogComplete =
    !loading && !refreshing && contacts.length >= total && (total > 0 || contacts.length === 0);
  catalogCompleteRef.current = catalogComplete && !serverQ.trim();
  if (catalogCompleteRef.current) {
    fullCatalogRef.current = contacts;
  }

  const groupActive = Boolean(groupFilter);
  const advancedActive =
    hasAdvancedContactTokens(filter) || hasGroupFilterToken(filter);

  useEffect(() => {
    setCheckedIds(new Set());
  }, [filter, groupFilter]);

  useEffect(() => {
    setGroupOverrides({});
    const combined = groupListQuery(groupFilter, filter);
    // Empty filter: load the full catalog.
    if (!combined.trim()) {
      setServerQ("");
      return;
    }
    // The full catalog is already in memory, so filter it in the browser.
    if (fullCatalogRef.current && !advancedActive) {
      setServerQ("");
      return;
    }
    // Group-page click: do not wait for the search debounce.
    if (groupActive && !filter.trim()) {
      setServerQ(combined);
      return;
    }
    if (catalogCompleteRef.current && !advancedActive) return;

    const t = window.setTimeout(() => setServerQ(combined), FILTER_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [filter, catalogComplete, advancedActive, groupFilter, groupActive]);

  const filterActive = filter.trim().length > 0;
  const needles = filterNeedles(filter);
  /** Prefer plain text for names. Fall back to the handle when that is all the user typed. */
  const nameMarkTerm = needles.text || needles.handle || "";
  const handleMarkTerm = highlightNeedle(filter);

  // Filter by name and handle in the browser. Server results are used when the
  // filter has search words the client cannot apply.
  const filteredContacts =
    filterActive && !advancedActive
      ? contacts.filter((c) => contactMatchesFilter(c, filter))
      : contacts;

  const displayContacts = useMemo(
    () =>
      [...filteredContacts]
        .map((c) =>
          groupOverrides[c.id] ? { ...c, groups: groupOverrides[c.id] } : c,
        )
        .filter((c) => contactBelongsToGroup(c.groups, groupFilter))
        .sort((a, b) =>
          compareContactsByName(a.name, b.name, nameSort.sort, nameSort.order),
        ),
    [filteredContacts, nameSort, groupOverrides, groupFilter],
  );

  const selectedContact =
    displayContacts.find((c) => c.id === selectedId) ?? null;
  const checkedContacts = useMemo(
    () => displayContacts.filter((c) => checkedIds.has(c.id)),
    [checkedIds, displayContacts],
  );
  const targetContacts = useMemo(() => {
    if (checkedContacts.length > 0) return checkedContacts;
    return selectedContact ? [selectedContact] : [];
  }, [checkedContacts, selectedContact]);

  useEffect(() => {
    onCheckedChange?.(checkedContacts);
  }, [checkedContacts, onCheckedChange]);

  useEffect(() => {
    return () => onCheckedChange?.([]);
  }, [onCheckedChange]);

  const toggleChecked = (id: string) => {
    setCheckedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const groupChecks = useMemo(
    () =>
      checksFromMembers(
        allGroups,
        targetContacts.map((c) => c.groups ?? []),
      ),
    [allGroups, targetContacts],
  );

  const applyMembership = async (name: string, enable: boolean) => {
    const ids = targetContacts
      .map((c) => Number(c.id))
      .filter((id) => Number.isFinite(id) && id > 0);
    if (ids.length === 0) return;
    await setContactGroupMembership(ids, name, enable);
    for (const id of ids) {
      invalidateContactDetail(String(id));
    }
    setGroupOverrides((prev) => {
      const next = { ...prev };
      for (const c of targetContacts) {
        const current = next[c.id] ?? c.groups ?? [];
        next[c.id] = enable
          ? current.some((g) => g.toLowerCase() === name.toLowerCase())
            ? current
            : [...current, name]
          : current.filter((g) => g.toLowerCase() !== name.toLowerCase());
      }
      return next;
    });
    if (groupActive) {
      setMembershipRev((n) => n + 1);
    }
  };

  const localSlice =
    !advancedActive &&
    (catalogCompleteRef.current || !serverQ.trim()) &&
    (filterActive || groupActive);
  const rangeTotal = localSlice ? displayContacts.length : total;

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
      onSelect={(c) => {
        if (skipRowSelectRef.current) {
          skipRowSelectRef.current = false;
          return;
        }
        if (checkedIds.size > 0) {
          toggleChecked(c.id);
          return;
        }
        onSelect(c);
      }}
      isRowHighlighted={(c) =>
        checkedIds.size > 0 ? checkedIds.has(c.id) : c.id === selectedId
      }
      getId={(c) => c.id}
      getTextValue={(c) => c.name}
      ariaLabel="Contacts"
      errorPrefix="Could not load contacts"
      headerActions={
        <div className="flex items-center gap-1">
          <GroupsMenu
            allGroups={allGroups}
            checks={groupChecks}
            disabled={targetContacts.length === 0}
            onToggle={(name) => {
              const on = groupChecks[name] === "on";
              void applyMembership(name, !on);
            }}
            onCreate={(name) => {
              void (async () => {
                const existing = allGroups.find(
                  (g) => g.toLowerCase() === name.toLowerCase(),
                );
                if (!existing) {
                  await createContactGroup(name);
                }
                await applyMembership(existing ?? name, true);
              })();
            }}
            onClearAll={() => {
              const names = new Set<string>();
              for (const c of targetContacts) {
                for (const g of c.groups ?? []) names.add(g);
              }
              void (async () => {
                for (const name of names) {
                  await applyMembership(name, false);
                }
              })();
            }}
          />
          <ContactSortMenu
            sort={nameSort.sort}
            order={nameSort.order}
            onChange={onNameSortChange}
          />
        </div>
      }
      getSectionLetter={
        filterActive
          ? undefined
          : (c) => contactSortLetter(c.name, nameSort.sort)
      }
      empty={
        !loading ? (
          <div className="p-4 text-[0.813rem] text-muted">
            {filterActive
              ? "No contacts match this filter"
              : groupFilter === "none"
                ? "Every contact has a group"
                : groupFilter
                  ? "No contacts in this group"
                  : "No contacts"}
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
        const checked = checkedIds.has(c.id);
        return (
          <>
            <span
              className="group/avatar relative flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center self-center"
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                skipRowSelectRef.current = true;
                toggleChecked(c.id);
                // The row click is suppressed only for this press. A later
                // click on the name must still toggle the row.
                queueMicrotask(() => {
                  skipRowSelectRef.current = false;
                });
              }}
            >
              <span
                className={
                  checked ? "invisible" : "group-hover/avatar:invisible"
                }
              >
                <ContactInitialCircle
                  displayName={c.name}
                  preferredHandle={c.handles?.[0] ?? null}
                />
              </span>
              <input
                type="checkbox"
                checked={checked}
                aria-label={`Select ${c.name}`}
                onChange={() => toggleChecked(c.id)}
                className={`pointer-events-none absolute size-5 accent-accent ${
                  checked ? "" : "invisible group-hover/avatar:visible"
                }`}
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
