import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import Checkbox from "../components/Checkbox";
import ContactInitialCircle from "../components/ContactInitialCircle";
import ContactSortMenu from "../components/ContactSortMenu";
import GroupsMenu from "../components/GroupsMenu";
import InfiniteOffsetList from "../components/InfiniteOffsetList";
import { useSetRightToolbar } from "../components/useRightToolbar";
import { apiClient } from "../lib/api";
import { getCachedContactDetail, updateCachedContactGroups } from "../lib/contactDetailCache";
import {
  contactBelongsToGroup,
  createContactGroup,
  GROUP_FILTER_TOKEN_RE,
  groupListQuery,
  hasGroupFilterToken,
  setContactGroupMembership,
} from "../lib/contactGroups";
import {
  type ContactNameSortState,
  compareContactsByName,
  contactSortLetter,
  loadContactNameSort,
  saveContactNameSort,
} from "../lib/contactSort";
import { highlightText } from "../lib/highlightText";
import { checksFromMembers } from "../lib/membershipChecks";
import { useContactGroups } from "../lib/useContactGroups";
import {
  PAGE_SIZE_CONTACTS_FIRST,
  PAGE_SIZE_FIRST,
  type PagedFetchPage,
  usePagedList,
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
function normalizeContacts(rows: ContactsPage["contacts"] | undefined): Contact[] {
  return (rows || []).map((c) => ({
    ...c,
    id: String(c.id),
    handles: c.handles ?? [],
    groups: c.groups ?? [],
  }));
}

/** Prefer a local override, then the open-drawer cache, then the list row. */
function groupsForContact(c: Contact, overrides: Record<string, string[]>): string[] {
  return overrides[c.id] ?? getCachedContactDetail(c.id)?.groups ?? c.groups ?? [];
}

/** Add or remove one group name, matching letter case the same way the list does. */
function withGroupMembership(groups: string[], name: string, enable: boolean): string[] {
  if (enable) {
    return groups.some((g) => g.toLowerCase() === name.toLowerCase()) ? groups : [...groups, name];
  }
  return groups.filter((g) => g.toLowerCase() !== name.toLowerCase());
}

export default function ContactList({
  filter = "",
  groupFilter = null,
  selectedId = null,
  onSelect,
  onCheckedChange,
  clearCheckedRev = 0,
}: {
  filter?: string;
  /** Named group, or `"none"` for contacts with no group. */
  groupFilter?: string | "none" | null;
  selectedId?: string | null;
  onSelect: (contact: Contact) => void;
  /** Checked rows, so the right panel can list them. */
  onCheckedChange?: (contacts: Contact[]) => void;
  /** Increment to uncheck every row (Clear contacts on the selection card). */
  clearCheckedRev?: number;
}) {
  const [serverQ, setServerQ] = useState("");
  const [groupOverrides, setGroupOverrides] = useState<Record<string, string[]>>({});
  const [nameSort, setNameSort] = useState<ContactNameSortState>(() => loadContactNameSort());
  const [checkedIds, setCheckedIds] = useState<Set<string>>(() => new Set());
  const [groupsMenuOpen, setGroupsMenuOpen] = useState(false);
  /** Last contacts the Groups menu assigned to, so a list filter change does not disable an open menu. */
  const assignTargetsRef = useRef<Contact[]>([]);
  const groupOverridesRef = useRef(groupOverrides);
  groupOverridesRef.current = groupOverrides;
  /** Ignores the row click that follows a checkbox press (nested control). */
  const skipRowSelectRef = useRef(false);
  const catalogCompleteRef = useRef(false);
  /** Unfiltered contact list, so group clicks can filter in the browser. */
  const fullCatalogRef = useRef<Contact[] | null>(null);
  const { groups: allGroups } = useContactGroups();
  const setRightToolbar = useSetRightToolbar();

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
      const res = await apiClient.get<ContactsPage>(`/v1/export/contacts?${params}`, { signal });
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
  if (catalogCompleteRef.current) {
    fullCatalogRef.current = contacts;
  }

  const groupActive = Boolean(groupFilter);
  const advancedActive = hasAdvancedContactTokens(filter) || hasGroupFilterToken(filter);

  useEffect(() => {
    void filter;
    void groupFilter;
    setCheckedIds(new Set());
  }, [filter, groupFilter]);

  useEffect(() => {
    if (clearCheckedRev === 0) return;
    setCheckedIds(new Set());
  }, [clearCheckedRev]);

  useEffect(() => {
    void catalogComplete;
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
  // Memoized: a fresh array here would invalidate `displayContacts` and
  // `checkedContacts` on every render, and the `onCheckedChange` effect below
  // would then re-render the parent in a loop.
  const filteredContacts = useMemo(
    () =>
      filterActive && !advancedActive
        ? contacts.filter((c) => contactMatchesFilter(c, filter))
        : contacts,
    [contacts, filter, filterActive, advancedActive],
  );

  const displayContacts = useMemo(
    () =>
      [...filteredContacts]
        .map((c) => (groupOverrides[c.id] ? { ...c, groups: groupOverrides[c.id] } : c))
        .filter((c) => contactBelongsToGroup(c.groups, groupFilter))
        .sort((a, b) => compareContactsByName(a.name, b.name, nameSort.sort, nameSort.order)),
    [filteredContacts, nameSort, groupOverrides, groupFilter],
  );

  const selectedContact = displayContacts.find((c) => c.id === selectedId) ?? null;
  const checkedContacts = useMemo(
    () => displayContacts.filter((c) => checkedIds.has(c.id)),
    [checkedIds, displayContacts],
  );
  const selectAllChecked =
    displayContacts.length > 0 && displayContacts.every((c) => checkedIds.has(c.id));
  const selectAllIndeterminate =
    !selectAllChecked && displayContacts.some((c) => checkedIds.has(c.id));
  const targetContacts = useMemo(() => {
    if (checkedContacts.length > 0) return checkedContacts;
    return selectedContact ? [selectedContact] : [];
  }, [checkedContacts, selectedContact]);
  if (targetContacts.length > 0) {
    assignTargetsRef.current = targetContacts;
  }
  const assignTargets = targetContacts.length > 0 ? targetContacts : assignTargetsRef.current;

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
        assignTargets.map((c) => groupsForContact(c, groupOverrides)),
      ),
    [allGroups, assignTargets, groupOverrides],
  );

  const applyMembership = useCallback(async (name: string, enable: boolean) => {
    const targets = assignTargetsRef.current;
    const ids = targets.map((c) => Number(c.id)).filter((id) => Number.isFinite(id) && id > 0);
    if (ids.length === 0) return;
    const nextOverrides = { ...groupOverridesRef.current };
    for (const c of targets) {
      const groups = withGroupMembership(groupsForContact(c, nextOverrides), name, enable);
      nextOverrides[c.id] = groups;
      updateCachedContactGroups(c.id, groups);
    }
    groupOverridesRef.current = nextOverrides;
    setGroupOverrides(nextOverrides);
    try {
      await setContactGroupMembership(ids, name, enable);
    } catch {
      const reverted = { ...groupOverridesRef.current };
      for (const c of targets) {
        const groups = withGroupMembership(groupsForContact(c, reverted), name, !enable);
        reverted[c.id] = groups;
        updateCachedContactGroups(c.id, groups);
      }
      groupOverridesRef.current = reverted;
      setGroupOverrides(reverted);
    }
  }, []);

  /** Drop every group on the selected contacts in one paint, then tell the server in parallel. */
  const clearAllMembership = useCallback(async () => {
    const targets = assignTargetsRef.current;
    const ids = targets.map((c) => Number(c.id)).filter((id) => Number.isFinite(id) && id > 0);
    if (ids.length === 0) return;
    const priorById: Record<string, string[]> = {};
    const names = new Set<string>();
    const nextOverrides = { ...groupOverridesRef.current };
    for (const c of targets) {
      const current = groupsForContact(c, nextOverrides);
      priorById[c.id] = current;
      for (const g of current) names.add(g);
      nextOverrides[c.id] = [];
      updateCachedContactGroups(c.id, []);
    }
    if (names.size === 0) return;
    groupOverridesRef.current = nextOverrides;
    setGroupOverrides(nextOverrides);
    const results = await Promise.allSettled(
      [...names].map((name) => setContactGroupMembership(ids, name, false)),
    );
    const failed = [...names].filter((_, i) => results[i].status === "rejected");
    if (failed.length === 0) return;
    const reverted = { ...groupOverridesRef.current };
    for (const c of targets) {
      const groups = priorById[c.id].filter((g) =>
        failed.some((name) => name.toLowerCase() === g.toLowerCase()),
      );
      reverted[c.id] = groups;
      updateCachedContactGroups(c.id, groups);
    }
    groupOverridesRef.current = reverted;
    setGroupOverrides(reverted);
  }, []);

  const menuDisabled = assignTargets.length === 0 && !groupsMenuOpen;

  useEffect(() => {
    setRightToolbar(
      <GroupsMenu
        allGroups={allGroups}
        checks={groupChecks}
        open={groupsMenuOpen}
        onOpenChange={setGroupsMenuOpen}
        disabled={menuDisabled}
        checksDisabled={menuDisabled}
        onToggle={(name) => {
          const on = groupChecks[name] === "on";
          void applyMembership(name, !on);
        }}
        onCreate={(name) => {
          void (async () => {
            const existing = allGroups.find((g) => g.toLowerCase() === name.toLowerCase());
            if (!existing) {
              await createContactGroup(name);
            }
            await applyMembership(existing ?? name, true);
          })();
        }}
        onClearAll={() => {
          void clearAllMembership();
        }}
      />,
    );
  }, [
    allGroups,
    applyMembership,
    clearAllMembership,
    groupChecks,
    groupsMenuOpen,
    menuDisabled,
    setRightToolbar,
  ]);

  useEffect(() => () => setRightToolbar(null), [setRightToolbar]);

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
      isRowHighlighted={(c) => (checkedIds.size > 0 ? checkedIds.has(c.id) : c.id === selectedId)}
      selectAllChecked={selectAllChecked}
      selectAllIndeterminate={selectAllIndeterminate}
      onSelectAllChange={(on) => {
        startTransition(() => {
          setCheckedIds(on ? new Set(displayContacts.map((c) => c.id)) : new Set());
        });
      }}
      selectAllLabel="Select all contacts"
      getId={(c) => c.id}
      getTextValue={(c) => c.name}
      ariaLabel="Contacts"
      errorPrefix="Could not load contacts"
      headerActions={
        <ContactSortMenu sort={nameSort.sort} order={nameSort.order} onChange={onNameSortChange} />
      }
      getSectionLetter={filterActive ? undefined : (c) => contactSortLetter(c.name, nameSort.sort)}
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
      renderRowLead={(c) => {
        const checked = checkedIds.has(c.id);
        // The whole avatar square toggles the box, so the label points at it by id.
        const checkId = `contact-check-${c.id}`;
        return (
          <label
            htmlFor={checkId}
            className="group/avatar relative flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center self-center"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              skipRowSelectRef.current = true;
              queueMicrotask(() => {
                skipRowSelectRef.current = false;
              });
            }}
            onKeyDown={(e) => e.stopPropagation()}
          >
            {/*
             * The initials hide behind the checkbox on hover, on keyboard focus,
             * and once checked. `opacity-0` rather than `invisible` so the input
             * stays in the tab order when it is not yet visible.
             */}
            <span
              className={
                checked
                  ? "invisible"
                  : "group-hover/avatar:invisible group-focus-within/avatar:invisible"
              }
            >
              <ContactInitialCircle displayName={c.name} preferredHandle={c.handles?.[0] ?? null} />
            </span>
            <Checkbox
              id={checkId}
              checked={checked}
              aria-label={`Select ${c.name}`}
              onChange={() => toggleChecked(c.id)}
              className={`absolute ${
                checked ? "" : "opacity-0 group-hover/avatar:opacity-100 focus-visible:opacity-100"
              }`}
            />
          </label>
        );
      }}
      renderRow={(c) => {
        const nameKey = c.name.trim().toLowerCase();
        const shownHandles = filterActive
          ? matchingHandles(c.handles, filter).filter((h) => h.trim().toLowerCase() !== nameKey)
          : [];
        return (
          <div className="min-w-0 flex-1">
            <div className="truncate text-[0.875rem] font-medium">
              {filterActive && nameMarkTerm ? highlightText(c.name, nameMarkTerm) : c.name}
            </div>
            {shownHandles.length > 0 && (
              <div className="mt-0.5">
                {shownHandles.map((h) => (
                  <div key={h} className="truncate text-[0.75rem] text-muted">
                    {highlightText(h, handleMarkTerm)}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      }}
    />
  );
}
