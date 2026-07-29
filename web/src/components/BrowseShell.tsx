"use client";

import type {
  ContactDetail,
  ContactListItem,
  ContactSection,
  GroupChatThread,
  GroupParticipant,
  YearThread,
} from "@/lib/types";
import {
  GROUP_CHAT_SORT_ALLOWED,
  GROUP_CHAT_SORT_KEY,
  GROUP_CHAT_SORT_ORDER_KEY,
  SORT_ORDER_ALLOWED,
} from "@/lib/groupChatList";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useVaultReadOnly } from "./useVaultReadOnly";
import { useVaultSearch } from "./useVaultSearch";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import type { SearchConversationHit } from "@/lib/search";
import { seedContactEditDraft } from "./contactEdit";
import {
  BrowseContactCtxMenu,
  BrowseMergeIntoPanel,
} from "./BrowseContactCtxMenu";
import { BrowseContactList } from "./BrowseContactList";
import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import { BrowseGroupChatsPane } from "./BrowseGroupChatsPane";
import { BrowseThreadColumn } from "./BrowseThreadColumn";
import {
  contactFormAnchorFromRect,
  type ContactFormAnchor,
} from "./ContactFormOverlay";
import {
  createGroupChatTrashOptions,
  groupChatToastTitle,
} from "./groupChatTrash";
import { LabelsMenu } from "./LabelsMenu";
import { useHistory } from "./history";
import { trashContactsLabel } from "./history/historyTypes";
import { ParticipantContactFormOverlay } from "./ParticipantContactFormOverlay";
import {
  type BrowseGroupChatSortBy,
  type SortMode,
  type SortOrder,
} from "./SortByMenu";
import { useSourceFilter } from "./SourceFilter";
import {
  useBrowseContactListBase,
  useBrowseContactListView,
} from "./useBrowseContactList";
import { useBrowseLabelMembership } from "./useBrowseLabelMembership";
import { useCollapsedGroupChatList } from "./useCollapsedGroupChatList";
import { useListSelection } from "./useListSelection";
import { useParticipantContactForm } from "./useParticipantContactForm";
import { useTrashActions } from "./useTrashActions";
import { useThreadMessages } from "./useThreadMessages";
import { useDismissible } from "./useDismissible";
import { usePersistedEnum } from "./usePersistedEnum";
import { PaneSeparator } from "./PaneSeparator";
import { usePanelLayoutStorage } from "./panelLayoutStorage";
import { usePanelCollapse } from "./usePanelCollapse";
import { Group, Panel, useDefaultLayout, usePanelRef } from "react-resizable-panels";

const GROUPS_PANEL_COLLAPSED_KEY = "mv-groups-panel-collapsed";

const SORT_MODE_KEY = "mv-contact-sort";
const SORT_ORDER_KEY = "mv-contact-sort-order";
const SORT_MODE_ALLOWED = [
  "first",
  "last",
  "messages",
  "group-messages",
  "phone",
] as const;

export function BrowseShell({
  paneStorageKey,
  sectionLabel,
  contactSection,
  contacts,
  allLabels = [],
  initialContactId,
}: {
  paneStorageKey: string;
  sectionLabel: string;
  contactSection: ContactSection;
  contacts: ContactListItem[];
  allLabels?: string[];
  initialContactId: number | null;
}) {
  const vaultReadOnly = useVaultReadOnly() === true;
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const { push: pushHistory, revision: historyRevision } = useHistory();
  const { sources, source, setSource, sourceQuery } = useSourceFilter();
  const [sortMode, setSortMode] = usePersistedEnum(
    SORT_MODE_KEY,
    SORT_MODE_ALLOWED,
    "last",
  );
  const [sortOrder, setSortOrder] = usePersistedEnum(
    SORT_ORDER_KEY,
    SORT_ORDER_ALLOWED,
    "asc",
  );
  const setSort = useCallback(
    (next: { sort: SortMode; order: SortOrder }) => {
      setSortMode(next.sort);
      setSortOrder(next.order);
    },
    [setSortMode, setSortOrder],
  );
  const sort = sortMode;
  const [query, setQuery] = useState("");
  const [contactId, setContactId] = useState<number | null>(initialContactId);
  const [detail, setDetail] = useState<ContactDetail | null>(null);
  const [yearly, setYearly] = useState<YearThread[]>([]);
  const [groupChats, setGroupChats] = useState<GroupChatThread[]>([]);
  const [messageSources, setMessageSources] = useState<string[]>([]);
  const [sourceCounts, setSourceCounts] = useState<{
    all: number;
    bySource: Record<string, number>;
  }>({ all: 0, bySource: {} });
  const [threadConversationIds, setThreadConversationIds] = useState<
    number[] | null
  >(null);
  const [activeThread, setActiveThread] = useState<string | null>(null);
  const [loadingThreads, setLoadingThreads] = useState(false);
  const loadedContactIdRef = useRef<number | null>(null);
  /** State mirror of loadedContactIdRef so the thread pane can tell "empty" from "still loading". */
  const [threadsLoadedFor, setThreadsLoadedFor] = useState<number | null>(null);
  const activeThreadRef = useRef<string | null>(null);
  activeThreadRef.current = activeThread;
  /** False after URL/hydration restore so Panel 4 stays empty until a click. */
  const allowAutoOpenThreadRef = useRef(initialContactId == null);
  const activeSourceRef = useRef<string | null>(null);
  activeSourceRef.current = source;
  const cancelContactFormRef = useRef<() => void>(() => {});

  const [saving, setSaving] = useState(false);
  const [labelOverrides, setLabelOverrides] = useState<Map<number, string[]>>(
    () => new Map(),
  );
  const [excludeOverrides, setExcludeOverrides] = useState<Map<number, boolean>>(
    () => new Map(),
  );
  const labelOverridesRef = useRef(labelOverrides);
  labelOverridesRef.current = labelOverrides;
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const [threadsEpoch, setThreadsEpoch] = useState(0);
  const [ctxMenu, setCtxMenu] = useState<{
    id: number;
    x: number;
    y: number;
  } | null>(null);
  const [mergeFromId, setMergeFromId] = useState<number | null>(null);
  const [mergeQuery, setMergeQuery] = useState("");
  const [mergePos, setMergePos] = useState<{ x: number; y: number } | null>(
    null,
  );
  const ctxMenuRef = useRef<HTMLDivElement>(null);
  const mergePanelRef = useRef<HTMLDivElement>(null);
  const pendingEditIdRef = useRef<number | null>(null);
  const statusShowTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const statusClearTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const storage = usePanelLayoutStorage();
  const mainLayout = useDefaultLayout({
    id: "mv-browse-main-v2",
    panelIds: ["list", "groups", "thread"],
    storage,
  });
  const listPanelRef = usePanelRef();
  const groupsPanelRef = usePanelRef();
  const {
    onGroupsResize: onGroupsPanelResize,
    onListResize: onListPanelResize,
    groupsCollapsed,
  } = usePanelCollapse(
    groupsPanelRef,
    listPanelRef,
    GROUPS_PANEL_COLLAPSED_KEY,
  );
  const [groupChatSortBy, setGroupChatSortBy] = usePersistedEnum(
    GROUP_CHAT_SORT_KEY,
    GROUP_CHAT_SORT_ALLOWED,
    "date",
  );
  const [groupChatSortOrder, setGroupChatSortOrder] = usePersistedEnum(
    GROUP_CHAT_SORT_ORDER_KEY,
    SORT_ORDER_ALLOWED,
    "desc",
  );
  const setGroupChatSort = useCallback(
    (next: { sortBy: BrowseGroupChatSortBy; order: SortOrder }) => {
      setGroupChatSortBy(next.sortBy);
      setGroupChatSortOrder(next.order);
    },
    [setGroupChatSortBy, setGroupChatSortOrder],
  );
  const [groupChatFilterYear, setGroupChatFilterYear] = useState<number | null>(
    null,
  );
  const initialVaultQuery = searchParams.get("q") ?? "";
  const vaultSearch = useVaultSearch(initialVaultQuery);
  const [scrollToMessageId, setScrollToMessageId] = useState<number | null>(
    null,
  );
  const [selectedGroupConversationId, setSelectedGroupConversationId] =
    useState<number | null>(null);
  const [selectionGroupChats, setSelectionGroupChats] = useState<
    GroupChatThread[]
  >([]);
  const [loadingSelectionGroups, setLoadingSelectionGroups] = useState(false);

  const saveContactPatch = useCallback(
    async (
      patch: {
        exclude?: boolean;
        labels?: string[];
        firstName?: string | null;
        lastName?: string | null;
        phones?: string[];
      },
      id?: number,
    ): Promise<boolean> => {
      const targetId = id ?? contactId;
      if (targetId == null) return false;
      setSaving(true);
      try {
        const res = await fetch(`/api/contacts/${targetId}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(patch),
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error ?? "save failed");
        if (data.contact && targetId === contactId) setDetail(data.contact);
        return true;
      } catch (err) {
        console.error(err);
        return false;
      } finally {
        setSaving(false);
      }
    },
    [contactId],
  );

  const isContactExcluded = useCallback(
    (c: { id: number; exclude: boolean }) =>
      excludeOverrides.get(c.id) ?? c.exclude,
    [excludeOverrides],
  );

  const {
    visibleContacts,
    sortedRaw,
    selectAllIds,
    compareContacts,
  } = useBrowseContactListBase({
    contacts,
    contactSection,
    isContactExcluded,
    sort,
    sortOrder,
    query,
  });

  const selectContactRef = useRef<(id: number) => void>(() => {});

  const validIds = useMemo(() => contacts.map((c) => c.id), [contacts]);
  const [listOrderIds, setListOrderIds] = useState<number[]>([]);

  const {
    selectedIds,
    setSelectedIds,
    hasSelection,
    allSelected: allGroupSelected,
    selectAllRef,
    clearSelection: clearSelectionBase,
    toggleSelectAll: toggleSelectAllInGroup,
    onSelectColumnClick,
    onRowClick: onNamePhoneClick,
  } = useListSelection<number>({
    orderedIds:
      listOrderIds.length > 0 ? listOrderIds : sortedRaw.map((c) => c.id),
    selectAllIds,
    validIds,
    rangeMode: "selectionSpan",
    multiThreshold: "any",
    focusedId: contactId,
    rowClickMode: "openWhenEmptyElseToggle",
    checkboxEvents: "preventAndStop",
    escapeToClear: false,
    selectAllSetsAnchor: false,
    onOpen: (id) => selectContactRef.current(id),
  });

  const { sorted, grouped } = useBrowseContactListView({
    sortedRaw,
    visibleContacts,
    compareContacts,
    query,
    selectedIds,
    sort,
  });

  useLayoutEffect(() => {
    const next = sorted.map((c) => c.id);
    setListOrderIds((prev) => {
      if (
        prev.length === next.length &&
        prev.every((id, i) => id === next[i])
      ) {
        return prev;
      }
      return next;
    });
  }, [sorted]);

  const selectContact = useCallback(
    (id: number) => {
      allowAutoOpenThreadRef.current = true;
      setSelectedGroupConversationId(null);
      setGroupChatFilterYear(null);
      cancelContactFormRef.current();
      if (id === contactId) {
        // Re-focus: reload and let the threads effect apply Direct-only auto-open.
        setThreadConversationIds(null);
        setActiveThread(null);
        setThreadsEpoch((e) => e + 1);
        return;
      }
      setContactId(id);
      setThreadConversationIds(null);
      setActiveThread(null);
      const params = new URLSearchParams(searchParams.toString());
      params.set("c", String(id));
      params.delete("h");
      params.delete("y");
      params.delete("conv");
      router.replace(`${pathname}?${params.toString()}`, { scroll: false });
    },
    [contactId, pathname, router, searchParams],
  );
  selectContactRef.current = selectContact;

  const clearContactFocus = useCallback(() => {
    allowAutoOpenThreadRef.current = false;
    setContactId(null);
    setThreadConversationIds(null);
    setActiveThread(null);
    setSelectedGroupConversationId(null);
    const params = new URLSearchParams(searchParams.toString());
    params.delete("c");
    params.delete("h");
    params.delete("y");
    params.delete("conv");
    const qs = params.toString();
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
  }, [pathname, router, searchParams]);

  useEffect(() => {
    if (selectedIds.size === 0 || contactId == null) return;
    if (selectedIds.has(contactId)) return;
    clearContactFocus();
  }, [selectedIds, contactId, clearContactFocus]);

  useEffect(() => {
    return () => {
      if (statusShowTimerRef.current) clearTimeout(statusShowTimerRef.current);
      if (statusClearTimerRef.current) clearTimeout(statusClearTimerRef.current);
    };
  }, []);

  const queueStatusMessage = useCallback((message: string) => {
    if (statusShowTimerRef.current) clearTimeout(statusShowTimerRef.current);
    if (statusClearTimerRef.current) clearTimeout(statusClearTimerRef.current);
    statusShowTimerRef.current = setTimeout(() => {
      setStatusMsg(message);
      statusClearTimerRef.current = setTimeout(() => {
        setStatusMsg(null);
        statusClearTimerRef.current = null;
      }, 5000);
      statusShowTimerRef.current = null;
    }, 300);
  }, []);

  useEffect(() => {
    if (!contactId) {
      loadedContactIdRef.current = null;
      allowAutoOpenThreadRef.current = false;
      setThreadsLoadedFor(null);
      setDetail(null);
      setYearly([]);
      setGroupChats([]);
      setMessageSources([]);
      setSourceCounts({ all: 0, bySource: {} });
      setThreadConversationIds(null);
      setActiveThread(null);
      setSelectedGroupConversationId(null);
      return;
    }
    let cancelled = false;
    // Keep the existing cards mounted while the next contact loads (swap data in place).
    // Only show a blank "Loading…" state when there is nothing to display yet.
    const switchingContact = loadedContactIdRef.current !== contactId;
    // URL/hydration restore: load contact metadata for Panel 2/3, but leave Panel 4
    // empty until the user clicks the contact or a group.
    const hydrateOnly =
      !allowAutoOpenThreadRef.current && activeThreadRef.current == null;
    if (switchingContact && loadedContactIdRef.current == null) {
      setLoadingThreads(true);
    }
    fetch(
      `/api/contacts/${contactId}/threads${
        sourceQuery ? `?${sourceQuery.slice(1)}` : ""
      }`,
    )
      .then((r) => r.json())
      .then((data) => {
        if (cancelled) return;
        if (data.error) {
          if (switchingContact) {
            setDetail(null);
            setYearly([]);
            setGroupChats([]);
            setMessageSources([]);
            setSourceCounts({ all: 0, bySource: {} });
            setThreadConversationIds(null);
            setActiveThread(null);
          }
          return;
        }
        // Contact fields don't depend on source — only replace detail when the person changes
        // so the top card shell/content don't flash on source filter updates.
        if (switchingContact) {
          const contact = data.contact as ContactDetail;
          const ov = labelOverridesRef.current.get(contact.id);
          setDetail(ov ? { ...contact, labels: ov } : contact);
        }
        const nextYearly: YearThread[] = data.yearly ?? [];
        const nextGroupChats: GroupChatThread[] = data.groupChats ?? [];
        setYearly(nextYearly);
        setGroupChats(nextGroupChats);
        setMessageSources(data.messageSources ?? []);
        setSourceCounts(
          data.sourceCounts ?? { all: 0, bySource: {} },
        );
        loadedContactIdRef.current = contactId;
        setThreadsLoadedFor(contactId);

        const available: string[] = data.messageSources ?? [];
        const selected = activeSourceRef.current;
        if (selected && !available.includes(selected)) {
          setSource(null);
        }

        if (hydrateOnly) {
          setThreadConversationIds(null);
          setActiveThread(null);
          setSelectedGroupConversationId(null);
          return;
        }

        // Keep a valid already-open thread on the same contact; reset when switching.
        setActiveThread((prev) => {
          if (switchingContact) return null;
          if (prev === "dm") return prev;
          if (prev?.startsWith("gfull-")) {
            const stillThere = nextGroupChats.some((t) => {
              const ids =
                t.conversationIds?.length > 0
                  ? t.conversationIds
                  : [t.conversationId];
              return `gfull-${ids.join("-")}` === prev;
            });
            return stillThere ? prev : null;
          }
          return null;
        });

        // Keep a valid already-open group; auto-open Direct only when there are no groups.
        let key = switchingContact ? null : activeThreadRef.current;
        if (key?.startsWith("gfull-")) {
          const stillThere = nextGroupChats.some((t) => {
            const ids =
              t.conversationIds?.length > 0
                ? t.conversationIds
                : [t.conversationId];
            return `gfull-${ids.join("-")}` === key;
          });
          if (!stillThere) key = null;
        } else if (key !== "dm") {
          key = null;
        }

        const dmIds = [
          ...new Set(nextYearly.flatMap((y) => y.conversationIds)),
        ];
        const hasGroups = nextGroupChats.length > 0;

        if (switchingContact) {
          setSelectedGroupConversationId(null);
          if (dmIds.length > 0 && !hasGroups) {
            key = "dm";
            setActiveThread("dm");
          } else {
            setActiveThread(null);
            setThreadConversationIds(null);
            return;
          }
        } else if (!key) {
          if (dmIds.length > 0 && !hasGroups) {
            key = "dm";
            setActiveThread("dm");
          } else {
            setThreadConversationIds(null);
            return;
          }
        } else if (key === "dm" && dmIds.length === 0) {
          setActiveThread(null);
          setThreadConversationIds(null);
          return;
        }

        let convIds: number[] | null = null;
        if (key === "dm") {
          convIds = dmIds;
        } else if (key.startsWith("gfull-")) {
          const g = nextGroupChats.find((t) => {
            const ids =
              t.conversationIds?.length > 0
                ? t.conversationIds
                : [t.conversationId];
            return `gfull-${ids.join("-")}` === key;
          });
          if (g) {
            convIds =
              g.conversationIds?.length > 0
                ? g.conversationIds
                : [g.conversationId];
          }
        }
        if (!convIds?.length) {
          setThreadConversationIds(null);
          return;
        }
        setThreadConversationIds(convIds);
      })
      .finally(() => {
        if (!cancelled) setLoadingThreads(false);
      });
    return () => {
      cancelled = true;
    };
  }, [contactId, sourceQuery, setSource, threadsEpoch]);

  const openThread = useCallback((conversationIds: number[], key: string) => {
    allowAutoOpenThreadRef.current = true;
    setActiveThread(key);
    setThreadConversationIds(conversationIds);
  }, []);

  const selectedContacts = useMemo(() => {
    const selected = new Set(selectedIds);
    const fromSorted = sorted.filter((c) => selected.has(c.id));
    if (fromSorted.length === selectedIds.size) return fromSorted;
    // Keep contacts that left the visible list (e.g. Excluded while on All).
    const have = new Set(fromSorted.map((c) => c.id));
    const byId = new Map(contacts.map((c) => [c.id, c]));
    const extras: ContactListItem[] = [];
    for (const id of selectedIds) {
      if (have.has(id)) continue;
      const c = byId.get(id);
      if (c) extras.push(c);
    }
    return [...fromSorted, ...extras];
  }, [sorted, selectedIds, contacts]);

  const trashIdsForContext = useCallback(
    (ctxId: number): number[] => {
      if (hasSelection && selectedIds.has(ctxId)) {
        return selectedContacts.map((c) => c.id);
      }
      return [ctxId];
    },
    [hasSelection, selectedIds, selectedContacts],
  );

  const selectionIdsKey = useMemo(
    () =>
      [...selectedIds]
        .sort((a, b) => a - b)
        .join(","),
    [selectedIds],
  );

  useEffect(() => {
    if (!selectionIdsKey) {
      setSelectionGroupChats([]);
      setLoadingSelectionGroups(false);
      return;
    }
    let cancelled = false;
    setLoadingSelectionGroups(true);
    const params = new URLSearchParams({ ids: selectionIdsKey });
    if (source) params.set("source", source);
    fetch(`/api/contacts/shared-group-chats?${params.toString()}`)
      .then((r) => r.json())
      .then((data) => {
        if (cancelled) return;
        const next: GroupChatThread[] = data.groupChats ?? [];
        setSelectionGroupChats(next);
        const prev = activeThreadRef.current;
        if (prev?.startsWith("gfull-")) {
          const stillThere = next.some((t) => {
            const ids =
              t.conversationIds?.length > 0
                ? t.conversationIds
                : [t.conversationId];
            return `gfull-${ids.join("-")}` === prev;
          });
          if (!stillThere) {
            setActiveThread(null);
            setSelectedGroupConversationId(null);
            setThreadConversationIds(null);
          }
        }
      })
      .catch((err) => {
        console.error(err);
        if (!cancelled) setSelectionGroupChats([]);
      })
      .finally(() => {
        if (!cancelled) setLoadingSelectionGroups(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectionIdsKey, source, threadsEpoch]);

  // Undo/redo restores server state but Panel 3 uses client-fetched lists.
  useEffect(() => {
    if (historyRevision === 0) return;
    setThreadsEpoch((n) => n + 1);
  }, [historyRevision]);

  const panelGroupChats = hasSelection ? selectionGroupChats : groupChats;

  const groupChatYears = useMemo(() => {
    const years = new Set<number>();
    for (const g of panelGroupChats) years.add(g.year);
    return [...years].sort((a, b) => b - a);
  }, [panelGroupChats]);

  useEffect(() => {
    if (
      groupChatFilterYear != null &&
      !groupChatYears.includes(groupChatFilterYear)
    ) {
      setGroupChatFilterYear(null);
    }
  }, [groupChatFilterYear, groupChatYears]);

  const { collapsedGroupChats, orderedGroupIds, collapsedById } =
    useCollapsedGroupChatList({
      groupChats: panelGroupChats,
      filterYear: groupChatFilterYear,
      query: "",
      sortBy: groupChatSortBy,
      sortOrder: groupChatSortOrder,
    });
  const selectGroupRef = useRef<(id: number) => void>(() => {});

  const {
    selectedIds: selectedGroupIds,
    setSelectedIds: setSelectedGroupIds,
    hasSelection: hasGroupSelection,
    allSelected: allGroupsSelected,
    selectAllRef: groupSelectAllRef,
    clearSelection: clearGroupSelection,
    toggleSelectAll: toggleSelectAllGroups,
    onSelectColumnClick: onGroupSelectColumnClick,
    onRowClick: onGroupRowClick,
  } = useListSelection<number>({
    orderedIds: orderedGroupIds,
    validIds: orderedGroupIds,
    rangeMode: "selectionSpan",
    multiThreshold: "any",
    focusedId: selectedGroupConversationId,
    rowClickMode: "openWhenEmptyElseToggle",
    checkboxEvents: "preventAndStop",
    escapeToClear: true,
    selectAllSetsAnchor: false,
    onOpen: (id) => selectGroupRef.current(id),
  });

  const selectGroupConversation = useCallback(
    (g: CollapsedGroupConversation) => {
      if (
        !hasGroupSelection &&
        hasSelection &&
        selectedGroupConversationId === g.conversationId
      ) {
        setSelectedGroupConversationId(null);
        setActiveThread(null);
        setThreadConversationIds(null);
        return;
      }
      setSelectedGroupConversationId(g.conversationId);
      const key = `gfull-${g.conversationIds.join("-")}`;
      openThread(g.conversationIds, key);
    },
    [
      hasGroupSelection,
      hasSelection,
      selectedGroupConversationId,
      openThread,
    ],
  );

  const openGroupById = useCallback(
    (id: number) => {
      const g = collapsedById.get(id);
      if (!g) return;
      selectGroupConversation(g);
    },
    [collapsedById, selectGroupConversation],
  );
  selectGroupRef.current = openGroupById;

  useEffect(() => {
    if (!hasGroupSelection) return;
    setActiveThread(null);
    setThreadConversationIds(null);
    setSelectedGroupConversationId(null);
  }, [hasGroupSelection]);

  // Multi-select is for shared-group discovery — clear an open Direct thread.
  useEffect(() => {
    if (!hasSelection) return;
    if (activeThreadRef.current !== "dm") return;
    setActiveThread(null);
    setThreadConversationIds(null);
  }, [hasSelection]);

  useEffect(() => {
    clearGroupSelection();
    setSelectedGroupConversationId(null);
  }, [selectionIdsKey, contactId, paneStorageKey, clearGroupSelection]);

  const openDirectThread = useCallback(() => {
    const dmIds = [...new Set(yearly.flatMap((y) => y.conversationIds))];
    if (dmIds.length === 0) return;
    clearGroupSelection();
    setSelectedGroupConversationId(null);
    openThread(dmIds, "dm");
  }, [yearly, openThread, clearGroupSelection]);

  const { messages, loading: loadingMessages } = useThreadMessages({
    conversationIds: threadConversationIds,
    sourceQuery,
    fullConversation: true,
    enabled: !hasGroupSelection,
    reloadToken: threadsEpoch,
  });

  const createDefaults = useMemo(() => {
    if (typeof contactSection === "object") {
      return { labels: [contactSection.label], exclude: false };
    }
    if (contactSection === "excluded") {
      return { labels: [] as string[], exclude: true };
    }
    // all, no-label
    return { labels: [] as string[], exclude: false };
  }, [contactSection]);

  const participantForm = useParticipantContactForm({
    vaultReadOnly,
    knownLabels: allLabels,
    createDefaults,
    setStatus: setStatusMsg,
    shouldIgnoreEscape: () => ctxMenu != null || labelsPanelPos != null,
    onSaved: (result) => {
      if (result.kind === "edit") {
        if (result.contact && result.contactId === contactId) {
          setDetail(result.contact);
        }
        setThreadsEpoch((e) => e + 1);
        router.refresh();
        return;
      }
      if (result.contact) {
        const name = result.contact.displayName ?? "contact";
        pushHistory({
          type: "createContact",
          contactId: result.contact.id,
          name,
          label: `Create contact ${name}`,
        });
        if (contactId == null) {
          setDetail(result.contact);
          selectContact(result.contact.id);
        } else {
          setThreadsEpoch((e) => e + 1);
        }
      }
      router.refresh();
    },
  });
  cancelContactFormRef.current = participantForm.cancelContactForm;

  const {
    formOpen,
    contactCreating,
    editContactId,
    contactSaving,
  } = participantForm;
  const contactEditing = editContactId != null;

  const {
    labelsPanelWrapRef,
    labelsCreatePinnedRef,
    labelsPanelPos,
    selectionDirtyRef,
    canEditLabels,
    menuLabels,
    labelChecks,
    excludedCheck,
    toggleLabel,
    createAndAssignLabel,
    clearAllLabelsForSelection,
    toggleExcludedForSelection,
    onSelectionMenuOpenChange,
    openCtxLabels,
    closeLabelsPanel,
    scheduleCloseLabelsPanel,
    cancelCloseLabelsPanel,
    flushSelectionDirty,
  } = useBrowseLabelMembership({
    allLabels,
    contacts,
    selectedContacts,
    hasSelection,
    detail,
    setDetail,
    contactId,
    setThreadsEpoch,
    formOpen,
    labelOverrides,
    setLabelOverrides,
    excludeOverrides,
    setExcludeOverrides,
    ctxMenu,
    trashIdsForContext,
    queueStatusMessage,
  });

  useEffect(() => {
    setSelectedIds(new Set());
    setLabelOverrides(new Map());
    setExcludeOverrides(new Map());
    selectionDirtyRef.current = false;
    cancelContactFormRef.current();
  }, [paneStorageKey, setSelectedIds, selectionDirtyRef]);

  const clearSelection = useCallback(() => {
    clearSelectionBase();
    if (selectionDirtyRef.current) {
      selectionDirtyRef.current = false;
      setLabelOverrides(new Map());
      setExcludeOverrides(new Map());
      router.refresh();
    } else {
      setLabelOverrides(new Map());
      setExcludeOverrides(new Map());
    }
  }, [clearSelectionBase, router, selectionDirtyRef]);

  useEffect(() => {
    if (!hasSelection) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (ctxMenu != null || labelsPanelPos != null) return;
      e.preventDefault();
      clearSelection();
      const el = document.activeElement;
      if (el instanceof HTMLElement) el.blur();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [hasSelection, clearSelection, ctxMenu, labelsPanelPos]);

  useEffect(() => {
    if (!hasSelection) return;
    participantForm.cancelContactForm();
  }, [hasSelection, participantForm.cancelContactForm]);

  const beginContactEdit = useCallback(
    (anchor?: ContactFormAnchor | null) => {
      if (!detail || hasSelection || contactCreating) return;
      participantForm.openEditFromDraft(
        detail.id,
        seedContactEditDraft({
          ...detail,
          labels:
            labelOverrides.get(detail.id) ?? detail.labels,
          exclude: excludeOverrides.get(detail.id) ?? detail.exclude,
        }),
        anchor ?? null,
      );
    },
    [
      detail,
      hasSelection,
      contactCreating,
      labelOverrides,
      excludeOverrides,
      participantForm.openEditFromDraft,
    ],
  );

  const onContactNameClick = useCallback(
    (anchorRect: DOMRect) => {
      if (vaultReadOnly || saving || contactSaving || formOpen) return;
      beginContactEdit(contactFormAnchorFromRect(anchorRect));
    },
    [
      vaultReadOnly,
      saving,
      contactSaving,
      formOpen,
      beginContactEdit,
    ],
  );

  // Finish Edit from context menu once the contact detail has loaded.
  useEffect(() => {
    const pending = pendingEditIdRef.current;
    if (pending == null || !detail || detail.id !== pending) return;
    if (hasSelection || contactCreating) return;
    pendingEditIdRef.current = null;
    participantForm.openEditFromDraft(
      detail.id,
      seedContactEditDraft({
        ...detail,
        labels:
          labelOverrides.get(detail.id) ?? detail.labels,
        exclude: excludeOverrides.get(detail.id) ?? detail.exclude,
      }),
      null,
    );
  }, [
    detail,
    hasSelection,
    contactCreating,
    labelOverrides,
    excludeOverrides,
    participantForm.openEditFromDraft,
  ]);

  const openCreateContactInPlace = useCallback(
    (handle: string, anchor: ContactFormAnchor) => {
      if (vaultReadOnly) return;
      participantForm.openCreateContactWithHandle(handle, anchor);
    },
    [vaultReadOnly, participantForm.openCreateContactWithHandle],
  );

  const canDelete =
    !contactCreating && (hasSelection || contactId != null);

  const deleteTargetIds = useCallback((): number[] => {
    if (hasSelection) return selectedContacts.map((c) => c.id);
    if (contactId != null) return [contactId];
    return [];
  }, [hasSelection, selectedContacts, contactId]);

  const executeTrash = useCallback(
    async (idsOverride?: number[]) => {
      const ids = idsOverride ?? deleteTargetIds();
      if (ids.length === 0) return;
      setCtxMenu(null);
      setSaving(true);
      try {
        const res = await fetch("/api/contacts/trash", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ ids, mode: "contact_and_messages" }),
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error ?? "delete failed");

        const byId = new Map(contacts.map((c) => [c.id, c]));
        const names = ids.map((id) => {
          const c =
            byId.get(id) ??
            selectedContacts.find((x) => x.id === id) ??
            (detail?.id === id ? detail : null);
          return c?.displayName?.trim() || "contact";
        });
        pushHistory({
          type: "trashContacts",
          contactIds: ids,
          mode: "contact_and_messages",
          names,
          label: trashContactsLabel(names),
        });

        setSelectedIds(new Set());
        setSelectedGroupIds(new Set());
        setLabelOverrides(new Map());
        setExcludeOverrides(new Map());
        selectionDirtyRef.current = false;
        cancelContactFormRef.current();

        setDetail(null);
        setYearly([]);
        setGroupChats([]);
        setMessageSources([]);
        setSourceCounts({ all: 0, bySource: {} });
        setThreadConversationIds(null);
        setActiveThread(null);
        setContactId(null);
        setGroupChatFilterYear(null);
        setSelectedGroupConversationId(null);
        loadedContactIdRef.current = null;
        setThreadsLoadedFor(null);
        queueStatusMessage(
          ids.length === 1
            ? "Moved contact & messages to Trash"
            : `Moved ${ids.length} contacts to Trash`,
        );
        const params = new URLSearchParams(searchParams.toString());
        params.delete("c");
        params.delete("h");
        params.delete("y");
        params.delete("conv");
        const qs = params.toString();
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
        router.refresh();
      } catch (err) {
        console.error(err);
        queueStatusMessage(
          err instanceof Error ? err.message : "delete failed",
        );
        router.refresh();
      } finally {
        setSaving(false);
      }
    },
    [
      deleteTargetIds,
      contacts,
      selectedContacts,
      detail,
      queueStatusMessage,
      router,
      setSelectedIds,
      setSelectedGroupIds,
      pathname,
      searchParams,
      pushHistory,
    ],
  );

  const openContactCtxMenu = useCallback(
    (id: number, x: number, y: number) => {
      closeLabelsPanel();
      setCtxMenu({ id, x, y });
    },
    [closeLabelsPanel],
  );

  const onCtxEdit = useCallback(
    (anchorEl: HTMLElement) => {
      if (!ctxMenu || hasSelection || formOpen) return;
      const id = ctxMenu.id;
      setCtxMenu(null);
      void participantForm.openEditContact(
        id,
        contactFormAnchorFromRect(anchorEl.getBoundingClientRect()),
      );
    },
    [
      ctxMenu,
      hasSelection,
      formOpen,
      participantForm.openEditContact,
    ],
  );

  const requestTrash = useCallback(
    (idsOverride?: number[]) => {
      void executeTrash(idsOverride);
    },
    [executeTrash],
  );

  const onCtxDelete = useCallback(() => {
    if (!ctxMenu) return;
    requestTrash(trashIdsForContext(ctxMenu.id));
  }, [ctxMenu, trashIdsForContext, requestTrash]);

  useDismissible({
    open: ctxMenu != null || mergeFromId != null,
    onDismiss: () => {
      setCtxMenu(null);
      setMergeFromId(null);
      setMergeQuery("");
      setMergePos(null);
      closeLabelsPanel();
      flushSelectionDirty();
    },
    refs: [ctxMenuRef, labelsPanelWrapRef, mergePanelRef],
    onEscape: (e) => {
      if (mergeFromId != null) {
        e.preventDefault();
        setMergeFromId(null);
        setMergeQuery("");
        setMergePos(null);
        return false;
      }
      if (labelsPanelPos != null) {
        e.preventDefault();
        closeLabelsPanel();
        return false;
      }
    },
  });

  const ctxMenuContact = useMemo(
    () => (ctxMenu ? contacts.find((c) => c.id === ctxMenu.id) : null),
    [contacts, ctxMenu],
  );
  const ctxMenuIsNameless = Boolean(
    ctxMenuContact &&
      !(ctxMenuContact.firstName ?? "").trim() &&
      !(ctxMenuContact.lastName ?? "").trim(),
  );

  const mergeTargets = useMemo(() => {
    if (mergeFromId == null) return [];
    const q = mergeQuery.trim().toLowerCase();
    return contacts
      .filter((c) => {
        if (c.id === mergeFromId) return false;
        const hasName =
          Boolean((c.firstName ?? "").trim()) ||
          Boolean((c.lastName ?? "").trim());
        if (!hasName) return false;
        if (!q) return true;
        return (
          c.displayName.toLowerCase().includes(q) ||
          (c.preferredHandle ?? "").toLowerCase().includes(q)
        );
      })
      .sort((a, b) =>
        a.sortFirst.localeCompare(b.sortFirst, undefined, {
          sensitivity: "base",
        }),
      )
      .slice(0, 40);
  }, [contacts, mergeFromId, mergeQuery]);

  const runMergeInto = useCallback(
    async (intoId: number) => {
      if (mergeFromId == null || vaultReadOnly) return;
      setSaving(true);
      try {
        const res = await fetch("/api/contacts/merge", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ fromId: mergeFromId, intoId }),
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error ?? "merge failed");
        setMergeFromId(null);
        setMergeQuery("");
        setMergePos(null);
        setCtxMenu(null);
        queueStatusMessage(
          `Merged into ${data.contact?.displayName ?? "contact"}`,
        );
        selectContact(intoId);
        router.refresh();
      } catch (err) {
        console.error(err);
        queueStatusMessage(
          err instanceof Error ? err.message : "merge failed",
        );
      } finally {
        setSaving(false);
      }
    },
    [
      mergeFromId,
      vaultReadOnly,
      queueStatusMessage,
      selectContact,
      router,
    ],
  );
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      if (ctxMenu != null || labelsPanelPos != null) {
        return;
      }
      if (formOpen) return;
      if (!canDelete) return;
      const t = e.target;
      if (t instanceof HTMLElement) {
        const tag = t.tagName;
        if (
          tag === "INPUT" ||
          tag === "TEXTAREA" ||
          tag === "SELECT" ||
          t.isContentEditable
        ) {
          return;
        }
      }
      const ids = deleteTargetIds();
      if (ids.length === 0) return;
      e.preventDefault();
      requestTrash(ids);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [
    ctxMenu,
    labelsPanelPos,
    formOpen,
    canDelete,
    deleteTargetIds,
    requestTrash,
  ]);
  const injectSelectedParticipants = useCallback(
    (participants: GroupParticipant[]) => {
      const next = [...participants];
      const extras = hasSelection
        ? selectedContacts
        : detail
          ? [detail]
          : [];
      for (const c of extras) {
        const handles = new Set(
          [
            c.preferredHandle,
            ...("phones" in c && Array.isArray(c.phones) ? c.phones : []),
          ]
            .filter(Boolean)
            .map((h) => String(h).trim()),
        );
        const already = next.some(
          (p) =>
            p.contactId === c.id ||
            (p.handle && handles.has(p.handle.trim())),
        );
        if (already) continue;
        next.push({
          name: c.displayName || c.preferredHandle || "Contact",
          handle: c.preferredHandle ?? "",
          contactId: c.id,
        });
      }
      next.sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
      );
      return next;
    },
    [hasSelection, selectedContacts, detail],
  );

  const groupThread = useMemo(() => {
    if (hasGroupSelection) return null;
    if (!activeThread?.startsWith("gfull-")) return null;
    const g = collapsedGroupChats.find(
      (t) => `gfull-${t.conversationIds.join("-")}` === activeThread,
    );
    if (!g) return null;
    return {
      participants: injectSelectedParticipants([...(g.participants ?? [])]),
      dateStart: g.dateStart,
      dateEnd: g.dateEnd,
      messageCount: g.messageCount,
    };
  }, [
    hasGroupSelection,
    collapsedGroupChats,
    activeThread,
    injectSelectedParticipants,
  ]);

  const selectedGroupRows = useMemo(
    () =>
      collapsedGroupChats.filter((g) => selectedGroupIds.has(g.conversationId)),
    [collapsedGroupChats, selectedGroupIds],
  );

  const onGroupParticipantClick = useCallback(
    (participant: GroupParticipant, anchorRect: DOMRect) => {
      if (vaultReadOnly || saving || contactSaving) return;
      participantForm.onParticipantClick(participant, anchorRect);
    },
    [
      vaultReadOnly,
      saving,
      contactSaving,
      participantForm.onParticipantClick,
    ],
  );

  const onImportVcf = useCallback(
    async (file: File) => {
      if (vaultReadOnly) return;
      const body = new FormData();
      body.set("file", file);
      try {
        const res = await fetch("/api/contacts/import-vcf", {
          method: "POST",
          body,
        });
        const data = (await res.json()) as {
          error?: string;
          created?: number;
          updated?: number;
          skipped?: number;
          errors?: string[];
        };
        if (!res.ok) throw new Error(data.error ?? "VCF import failed");
        const created = data.created ?? 0;
        const updated = data.updated ?? 0;
        const skipped = data.skipped ?? 0;
        const errCount = data.errors?.length ?? 0;
        const parts = [
          `Created ${created}`,
          `updated ${updated}`,
          `skipped ${skipped}`,
        ];
        if (errCount > 0) parts.push(`${errCount} note${errCount === 1 ? "" : "s"}`);
        queueStatusMessage(`VCF import: ${parts.join(", ")}`);
        if (errCount > 0 && data.errors) {
          console.warn("VCF import notes:", data.errors);
        }
        router.refresh();
      } catch (err) {
        console.error(err);
        queueStatusMessage(
          err instanceof Error ? err.message : "VCF import failed",
        );
      }
    },
    [vaultReadOnly, queueStatusMessage, router],
  );

  const groupTrashTargets = useCallback(
    (forId?: number) => {
      const primaryIds =
        forId != null && !hasGroupSelection
          ? [forId]
          : hasGroupSelection
            ? [...selectedGroupIds]
            : selectedGroupConversationId != null
              ? [selectedGroupConversationId]
              : [];
      const out: number[] = [];
      for (const id of primaryIds) {
        const g = collapsedById.get(id);
        const ids = g?.conversationIds?.length ? g.conversationIds : [id];
        for (const cid of ids) {
          if (!out.includes(cid)) out.push(cid);
        }
      }
      return out;
    },
    [
      hasGroupSelection,
      selectedGroupIds,
      selectedGroupConversationId,
      collapsedById,
    ],
  );

  const canTrashGroups =
    !vaultReadOnly &&
    (hasGroupSelection || selectedGroupConversationId != null);

  const browseGroupTrash = useMemo(
    () => createGroupChatTrashOptions({ variant: "browse" }),
    [],
  );

  const {
    saving: groupTrashSaving,
    moveToTrash: moveGroupsToTrash,
    confirmDialog: groupTrashConfirmDialog,
  } = useTrashActions<number>({
    endpoint: browseGroupTrash.endpoint,
    idField: browseGroupTrash.idField,
    getTargets: groupTrashTargets,
    canTrash: canTrashGroups,
    canRestoreOrDelete: false,
    status: browseGroupTrash.status,
    setStatus: (s) => {
      if (s) queueStatusMessage(s);
    },
    onRemoved: (targets) => {
      clearGroupSelection();
      setSelectedGroupConversationId(null);
      setThreadConversationIds(null);
      setActiveThread(null);
      const removed = new Set(targets);
      setGroupChats((prev) =>
        prev.filter((g) => {
          const ids =
            g.conversationIds?.length > 0
              ? g.conversationIds
              : [g.conversationId];
          return !ids.some((id) => removed.has(id));
        }),
      );
      setSelectionGroupChats((prev) =>
        prev.filter((g) => {
          const ids =
            g.conversationIds?.length > 0
              ? g.conversationIds
              : [g.conversationId];
          return !ids.some((id) => removed.has(id));
        }),
      );
    },
    onTrashed: (ids) => {
      const titles = ids.map((id) => {
        const g = collapsedById.get(id);
        return g ? groupChatToastTitle(g) : "group message";
      });
      pushHistory(browseGroupTrash.historyEntry(ids, titles));
    },
    afterTrash: () => {
      setThreadsEpoch((n) => n + 1);
      router.refresh();
    },
  });

  return (
    <>
    <Group
      id="mv-browse-main-v2"
      orientation="horizontal"
      className="h-full w-full"
      defaultLayout={mainLayout.defaultLayout}
      onLayoutChanged={mainLayout.onLayoutChanged}
    >
      <Panel
        id="list"
        panelRef={listPanelRef}
        defaultSize={240}
        minSize={100}
        maxSize={480}
        groupResizeBehavior="preserve-pixel-size"
        onResize={onListPanelResize}
        className="min-h-0"
      >
        <BrowseContactList
          sectionLabel={sectionLabel}
          selectAllRef={selectAllRef}
          allGroupSelected={allGroupSelected}
          visibleCount={visibleContacts.length}
          sortedCount={sorted.length}
          query={query}
          onQueryChange={setQuery}
          onToggleSelectAll={toggleSelectAllInGroup}
          onNewContact={(el) =>
            openCreateContactInPlace(
              "",
              contactFormAnchorFromRect(el.getBoundingClientRect()),
            )
          }
          onImportVcf={vaultReadOnly ? undefined : onImportVcf}
          vaultReadOnly={vaultReadOnly}
          labelsMenu={
            <LabelsMenu
              allLabels={menuLabels}
              checks={labelChecks}
              excludedCheck={excludedCheck}
              disabled={!canEditLabels}
              onToggle={toggleLabel}
              onToggleExcluded={() => void toggleExcludedForSelection()}
              onCreate={createAndAssignLabel}
              onClearAll={() => void clearAllLabelsForSelection()}
              onOpenChange={onSelectionMenuOpenChange}
            />
          }
          onEdit={(el) =>
            beginContactEdit(
              contactFormAnchorFromRect(el.getBoundingClientRect()),
            )
          }
          editDisabled={!detail || hasSelection || formOpen}
          onTrashContact={() => requestTrash()}
          deleteDisabled={!canDelete || saving || groupTrashSaving}
          sort={sort}
          sortOrder={sortOrder}
          onSortChange={setSort}
          grouped={grouped}
          contactId={contactId}
          contextMenuId={ctxMenu?.id ?? null}
          selectedIds={selectedIds}
          onSelectColumnClick={onSelectColumnClick}
          onNamePhoneClick={onNamePhoneClick}
          onContextMenu={openContactCtxMenu}
        />
      </Panel>

      <PaneSeparator orientation="vertical" />

      <Panel
        id="groups"
        panelRef={groupsPanelRef}
        defaultSize={360}
        minSize={180}
        maxSize={520}
        collapsible
        collapsedSize={0}
        onResize={onGroupsPanelResize}
        className="min-h-0 overflow-hidden"
      >
        <BrowseGroupChatsPane
          items={collapsedGroupChats}
          selectedConversationId={selectedGroupConversationId}
          selectedIds={selectedGroupIds}
          selectAllRef={groupSelectAllRef}
          allSelected={allGroupsSelected}
          onToggleSelectAll={toggleSelectAllGroups}
          onSelectColumnClick={onGroupSelectColumnClick}
          onRowClick={onGroupRowClick}
          onTrashMessages={() => void moveGroupsToTrash()}
          trashDisabled={!canTrashGroups || saving || groupTrashSaving}
          vaultReadOnly={vaultReadOnly}
          years={groupChatYears}
          filterYear={groupChatFilterYear}
          onFilterYearChange={setGroupChatFilterYear}
          sortBy={groupChatSortBy}
          sortOrder={groupChatSortOrder}
          onSortChange={setGroupChatSort}
          searchQuery={vaultSearch.draft}
          onSearchQueryChange={vaultSearch.setDraft}
          onSearchSubmit={(q) => {
            vaultSearch.submit(q);
            const params = new URLSearchParams(searchParams.toString());
            if (q.trim()) params.set("q", q.trim());
            else params.delete("q");
            const qs = params.toString();
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
              scroll: false,
            });
          }}
          searchSources={sources}
          searchLabels={allLabels}
          resultsMode={vaultSearch.resultsMode}
          searchHits={vaultSearch.hits}
          searchTotal={vaultSearch.total}
          searchLoading={vaultSearch.loading}
          onSelectSearchHit={(hit: SearchConversationHit) => {
            clearGroupSelection();
            setSelectedGroupConversationId(hit.conversationId);
            setScrollToMessageId(hit.topMatch?.id ?? null);
            openThread(
              [hit.conversationId],
              hit.conversationType === "group"
                ? `gfull-${hit.conversationId}`
                : "dm",
            );
          }}
          emptyLabel={
            hasSelection
              ? loadingSelectionGroups
                ? "Loading…"
                : selectedIds.size > 1
                  ? "No shared group messages"
                  : "No group messages"
              : contactId && loadingThreads
                ? "Loading…"
                : "No group messages"
          }
          directAvailable={
            !hasSelection &&
            contactId != null &&
            yearly.some((y) => y.conversationIds.length > 0)
          }
          directActive={activeThread === "dm"}
          directDateStart={
            yearly.length > 0
              ? yearly.reduce(
                  (min, y) => (y.dateStart < min ? y.dateStart : min),
                  yearly[0].dateStart,
                )
              : null
          }
          directDateEnd={
            yearly.length > 0
              ? yearly.reduce(
                  (max, y) => (y.dateEnd > max ? y.dateEnd : max),
                  yearly[0].dateEnd,
                )
              : null
          }
          onDirectClick={openDirectThread}
        />
      </Panel>

      <PaneSeparator orientation="vertical" />

      <Panel id="thread" minSize="30%" className="min-h-0 min-w-0">
        <BrowseThreadColumn
          paneStorageKey={paneStorageKey}
          selectedIds={selectedIds}
          selectedContacts={selectedContacts}
          hasSelection={hasSelection}
          hasGroupSelection={hasGroupSelection}
          selectedGroupIds={selectedGroupIds}
          selectedGroupRows={selectedGroupRows}
          detail={detail}
          groupThread={groupThread}
          vaultReadOnly={vaultReadOnly}
          statusMsg={statusMsg}
          contactId={contactId}
          contacts={contacts}
          activeThread={activeThread}
          sources={sources}
          messageSources={messageSources}
          sourceCounts={sourceCounts}
          source={source}
          onSourceChange={setSource}
          yearly={yearly}
          messages={messages}
          loadingMessages={loadingMessages}
          loadingSelectionGroups={loadingSelectionGroups}
          threadsLoadedFor={threadsLoadedFor}
          hasConversationChoices={
            !hasSelection &&
            (yearly.some((y) => y.conversationIds.length > 0) ||
              groupChats.length > 0)
          }
          conversationsPanelCollapsed={groupsCollapsed}
          highlightTerms={vaultSearch.highlightTerms}
          scrollToMessageId={scrollToMessageId}
          onContactNameClick={onContactNameClick}
          onGroupParticipantClick={onGroupParticipantClick}
          onClearContactSelection={clearSelection}
          onClearGroupSelection={clearGroupSelection}
          onClearContactFocus={clearContactFocus}
        />
      </Panel>
    </Group>
    {ctxMenu && (
      <BrowseContactCtxMenu
        menuRef={ctxMenuRef}
        ctxMenu={ctxMenu}
        vaultReadOnly={vaultReadOnly}
        saving={saving}
        groupTrashSaving={groupTrashSaving}
        hasSelection={hasSelection}
        contactCreating={contactCreating}
        contactEditing={contactEditing}
        isNameless={ctxMenuIsNameless}
        onMouseEnterItem={scheduleCloseLabelsPanel}
        onNewContact={(el) => {
          setCtxMenu(null);
          openCreateContactInPlace(
            "",
            contactFormAnchorFromRect(el.getBoundingClientRect()),
          );
        }}
        onEdit={onCtxEdit}
        onMergeInto={() => {
          setMergeFromId(ctxMenu.id);
          setMergePos({ x: ctxMenu.x, y: ctxMenu.y });
          setMergeQuery("");
          setCtxMenu(null);
        }}
        onLabelsEnter={openCtxLabels}
        onLabelsLeave={scheduleCloseLabelsPanel}
        onDelete={onCtxDelete}
      />
    )}
    {mergeFromId != null && mergePos && (
      <BrowseMergeIntoPanel
        panelRef={mergePanelRef}
        x={mergePos.x}
        y={mergePos.y}
        query={mergeQuery}
        onQueryChange={setMergeQuery}
        targets={mergeTargets}
        saving={saving}
        onSelect={(id) => void runMergeInto(id)}
      />
    )}
    {labelsPanelPos && (
      <div
        ref={labelsPanelWrapRef}
        onMouseEnter={cancelCloseLabelsPanel}
        onMouseLeave={scheduleCloseLabelsPanel}
      >
        <LabelsMenu
          fixedPosition={labelsPanelPos}
          allLabels={menuLabels}
          checks={labelChecks}
          excludedCheck={excludedCheck}
          disabled={formOpen}
          onToggle={toggleLabel}
          onToggleExcluded={() => void toggleExcludedForSelection()}
          onCreate={createAndAssignLabel}
          onClearAll={() => void clearAllLabelsForSelection()}
          onModeChange={(mode) => {
            labelsCreatePinnedRef.current = mode === "create";
            if (mode === "create") cancelCloseLabelsPanel();
          }}
          onOpenChange={(open) => {
            if (!open) closeLabelsPanel();
            else onSelectionMenuOpenChange(true);
          }}
        />
      </div>
    )}
    <ParticipantContactFormOverlay
      titleId="mv-contact-form-title"
      phonesView={detail?.phones ?? []}
      form={participantForm}
    />
    {groupTrashConfirmDialog}
    </>
  );
}
