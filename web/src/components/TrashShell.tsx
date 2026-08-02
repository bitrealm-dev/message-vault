"use client";

import type {
  ContactDetail,
  GroupYearRow,
  TrashedContactItem,
  TrashedContactMessagesItem,
  UnassignedHandle,
  YearThread,
} from "@/lib/types";
import { newestYearForConversation } from "@/lib/groupChatList";
import {
  buildTrashListItems,
  countTrashTabs,
  filterTrashListItems,
  itemsForTrashTab,
  parseTrashTab,
  type TrashListItem,
  type TrashTab,
} from "@/lib/trashList";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Group, Panel, useDefaultLayout } from "react-resizable-panels";
import {
  BrowseThreadPane,
  type BrowseGroupThreadMeta,
} from "./BrowseThreadPane";
import { useHistory } from "./history";
import { PaneSeparator } from "./PaneSeparator";
import { usePanelLayoutStorage } from "./panelLayoutStorage";
import { useConfirmDialog } from "./useConfirmDialog";
import { useDismissible } from "./useDismissible";
import { useListSelection } from "./useListSelection";
import { useSourceFilter } from "./SourceFilter";
import { useThreadMessages } from "./useThreadMessages";
import { TrashUnifiedList } from "./TrashUnifiedList";

function syntheticContactDetail(
  item: Extract<TrashListItem, { category: "contacts" }>,
): ContactDetail {
  return {
    id: item.contactId ?? -1,
    displayName: item.displayName,
    preferredName: item.displayName,
    preferredHandle: item.handle,
    firstName: null,
    lastName: null,
    sortFirst: item.displayName,
    sortLast: item.displayName,
    letter: "#",
    labels: [],
    messageCount: item.messageCount,
    groupMessageCount: 0,
    phones: [item.handle],
    dateStart: null,
    dateEnd: null,
  };
}

export function TrashShell({
  handles,
  groupChats: initialGroupChats,
  trashedContacts,
  trashedContactMessages,
  initialHandle,
  initialConversationId,
  initialYear,
}: {
  handles: UnassignedHandle[];
  groupChats: GroupYearRow[];
  trashedContacts: TrashedContactItem[];
  trashedContactMessages: TrashedContactMessagesItem[];
  initialHandle: string | null;
  initialConversationId: number | null;
  initialYear: number | null;
}) {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const { clear: clearHistory } = useHistory();
  const { sources, source, setSource, sourceQuery } = useSourceFilter();

  useEffect(() => {
    clearHistory();
  }, [clearHistory]);

  const tab = useMemo((): TrashTab => {
    const raw = searchParams.get("tab");
    if (raw === "group-messages" || raw === "contacts") {
      return parseTrashTab(raw);
    }
    if (searchParams.get("g")) return "group-messages";
    return "contacts";
  }, [searchParams]);

  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{
    x: number;
    y: number;
    key: string;
  } | null>(null);
  const ctxMenuRef = useRef<HTMLDivElement>(null);

  const allItems = useMemo(
    () =>
      buildTrashListItems({
        handles,
        contacts: trashedContacts,
        messagesOnly: trashedContactMessages,
        groupChats: initialGroupChats,
      }),
    [handles, trashedContacts, trashedContactMessages, initialGroupChats],
  );
  const [items, setItems] = useState(allItems);
  useEffect(() => {
    setItems(allItems);
  }, [allItems]);

  const tabCounts = useMemo(() => countTrashTabs(items), [items]);
  const tabItems = useMemo(
    () => filterTrashListItems(itemsForTrashTab(items, tab), query),
    [items, tab, query],
  );

  const orderedKeys = useMemo(() => tabItems.map((i) => i.key), [tabItems]);
  const validKeys = useMemo(
    () => itemsForTrashTab(items, tab).map((i) => i.key),
    [items, tab],
  );
  const byKey = useMemo(() => {
    const map = new Map<string, TrashListItem>();
    for (const i of items) map.set(i.key, i);
    return map;
  }, [items]);

  const initialFocusKey = useMemo(() => {
    if (initialConversationId != null) {
      const key = `g:${initialConversationId}`;
      if (allItems.some((i) => i.key === key)) return key;
    }
    if (initialHandle) {
      const match = allItems.find(
        (i) => i.category === "contacts" && i.handle === initialHandle,
      );
      if (match) return match.key;
    }
    return null;
  }, [allItems, initialConversationId, initialHandle]);

  const [focusedKey, setFocusedKey] = useState<string | null>(initialFocusKey);
  const focusedItem = focusedKey ? (byKey.get(focusedKey) ?? null) : null;

  // Keep focus on the active tab's items only.
  useEffect(() => {
    if (!focusedKey) return;
    const item = byKey.get(focusedKey);
    if (!item) {
      setFocusedKey(null);
      return;
    }
    const onContacts = item.category === "contacts";
    if (tab === "contacts" && !onContacts) setFocusedKey(null);
    if (tab === "group-messages" && onContacts) setFocusedKey(null);
  }, [tab, focusedKey, byKey]);

  const selectItemRef = useRef<(key: string) => void>(() => {});

  const {
    selectedIds: selectedKeys,
    setSelectedIds: setSelectedKeys,
    selectionAnchorRef,
    multiSelected,
    allSelected,
    selectAllRef,
    clearSelection,
    toggleSelectAll,
    onSelectColumnClick,
    onRowClick,
  } = useListSelection<string>({
    orderedIds: orderedKeys,
    validIds: validKeys,
    rangeMode: "anchor",
    multiThreshold: "any",
    focusedId: focusedKey,
    rowClickMode: "alwaysOpen",
    checkboxEvents: "preventAndStop",
    escapeToClear: true,
    escapeBlocked: () => ctxMenu != null,
    selectAllSetsAnchor: true,
    onOpen: (key) => selectItemRef.current(key),
    onSelectionMutation: () => setCtxMenu(null),
  });

  const [yearly, setYearly] = useState<YearThread[]>([]);
  const [messageSources, setMessageSources] = useState<string[]>([]);
  const [sourceCounts, setSourceCounts] = useState<{
    all: number;
    bySource: Record<string, number>;
  }>({ all: 0, bySource: {} });
  const [threadsLoadedFor, setThreadsLoadedFor] = useState<string | null>(null);
  const [activeThread, setActiveThread] = useState<string | null>(null);
  const [threadConversationIds, setThreadConversationIds] = useState<
    number[] | null
  >(null);

  const switchTab = useCallback(
    (next: TrashTab) => {
      const params = new URLSearchParams(searchParams.toString());
      if (next === "contacts") params.delete("tab");
      else params.set("tab", "group-messages");
      if (next === "contacts") {
        params.delete("g");
        params.delete("y");
      } else {
        params.delete("h");
      }
      setFocusedKey(null);
      clearSelection();
      setYearly([]);
      setActiveThread(null);
      setThreadConversationIds(null);
      setThreadsLoadedFor(null);
      setQuery("");
      const qs = params.toString();
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
    },
    [clearSelection, pathname, router, searchParams],
  );

  const selectItem = useCallback(
    (key: string) => {
      const item = byKey.get(key);
      if (!item) return;
      setFocusedKey(key);
      setSelectedKeys(new Set());
      selectionAnchorRef.current = key;
      setCtxMenu(null);
      setYearly([]);
      setActiveThread(null);
      setThreadConversationIds(null);
      setThreadsLoadedFor(null);

      const params = new URLSearchParams(searchParams.toString());
      if (item.category === "groupMessages") {
        params.set("tab", "group-messages");
        params.delete("h");
        params.set("g", String(item.conversationId));
        const y = newestYearForConversation(
          initialGroupChats,
          item.conversationId,
        );
        if (y != null) params.set("y", String(y));
        else params.delete("y");
      } else {
        params.delete("tab");
        params.set("h", item.handle);
        params.delete("g");
        params.delete("y");
      }
      router.replace(`${pathname}?${params.toString()}`, { scroll: false });
    },
    [
      byKey,
      initialGroupChats,
      pathname,
      router,
      searchParams,
      selectionAnchorRef,
      setSelectedKeys,
    ],
  );
  selectItemRef.current = selectItem;

  const focusIsGroup = focusedItem?.category === "groupMessages";

  useEffect(() => {
    if (
      multiSelected ||
      !focusedItem ||
      focusedItem.category === "groupMessages"
    ) {
      if (!focusIsGroup) {
        setYearly([]);
        setMessageSources([]);
        setSourceCounts({ all: 0, bySource: {} });
        setThreadsLoadedFor(null);
        setActiveThread(null);
        setThreadConversationIds(null);
      }
      return;
    }

    let cancelled = false;
    const loadKey = focusedItem.key;
    setThreadsLoadedFor(null);
    const qs = new URLSearchParams();
    if (source) qs.set("source", source);
    qs.set("trashed", "1");
    const url =
      focusedItem.trashKind === "contact" && focusedItem.contactId != null
        ? `/api/contacts/${focusedItem.contactId}/threads?${qs.toString()}`
        : (() => {
            qs.set("handle", focusedItem.handle);
            return `/api/unassigned/threads?${qs.toString()}`;
          })();

    fetch(url)
      .then((r) => r.json())
      .then((data) => {
        if (cancelled) return;
        if (data.error) {
          setYearly([]);
          setMessageSources([]);
          setSourceCounts({ all: 0, bySource: {} });
          setThreadsLoadedFor(loadKey);
          return;
        }
        const nextYearly: YearThread[] = data.yearly ?? [];
        setYearly(nextYearly);
        setMessageSources(data.messageSources ?? []);
        setSourceCounts(data.sourceCounts ?? { all: 0, bySource: {} });
        const available: string[] = data.messageSources ?? [];
        if (source && !available.includes(source)) setSource(null);

        if (nextYearly.length > 0) {
          const ids = nextYearly.flatMap((y) => y.conversationIds);
          setActiveThread("dm");
          setThreadConversationIds(ids.length > 0 ? ids : null);
        } else {
          setActiveThread(null);
          setThreadConversationIds(null);
        }
        setThreadsLoadedFor(loadKey);
      })
      .catch(() => {
        if (!cancelled) setThreadsLoadedFor(loadKey);
      });

    return () => {
      cancelled = true;
    };
  }, [focusedItem, focusIsGroup, multiSelected, source, setSource]);

  useEffect(() => {
    if (
      multiSelected ||
      !focusedItem ||
      focusedItem.category !== "groupMessages"
    ) {
      return;
    }
    const id = focusedItem.conversationId;
    setActiveThread(`gfull-${id}`);
    setThreadConversationIds([id]);
    setYearly([]);
  }, [focusedItem, multiSelected]);

  useEffect(() => {
    if (initialConversationId == null || initialYear == null) return;
    if (focusedItem?.category !== "groupMessages") return;
    if (focusedItem.conversationId !== initialConversationId) return;
    const params = new URLSearchParams(searchParams.toString());
    if (params.get("y") == null) {
      params.set("y", String(initialYear));
      router.replace(`${pathname}?${params.toString()}`, { scroll: false });
    }
  }, [
    focusedItem,
    initialConversationId,
    initialYear,
    pathname,
    router,
    searchParams,
  ]);

  const groupThreadMeta: BrowseGroupThreadMeta | null = useMemo(() => {
    if (!activeThread?.startsWith("gfull-")) return null;
    if (focusedItem?.category !== "groupMessages") return null;
    const g = focusedItem.group;
    return {
      participants: g.participants,
      dateStart: g.dateStart,
      dateEnd: g.dateEnd,
      messageCount: g.messageCount,
    };
  }, [activeThread, focusedItem]);

  const { messages, loading: loadingMessages } = useThreadMessages({
    conversationIds: multiSelected ? null : threadConversationIds,
    fullConversation: true,
    sourceQuery,
    enabled: !multiSelected && threadConversationIds != null,
  });

  const detail =
    focusedItem?.category === "contacts" && !multiSelected
      ? syntheticContactDetail(focusedItem)
      : null;

  const headerTitle = useMemo(() => {
    if (multiSelected) return null;
    if (focusedItem?.category === "groupMessages") {
      return focusedItem.displayName;
    }
    if (detail) return detail.displayName;
    return null;
  }, [detail, focusedItem, multiSelected]);

  const actionKeys = useMemo(() => {
    if (multiSelected) return [...selectedKeys];
    if (focusedKey) return [focusedKey];
    return [] as string[];
  }, [multiSelected, selectedKeys, focusedKey]);

  const { confirm: askConfirm, dialog: permanentConfirmDialog } =
    useConfirmDialog();

  const clearFocus = useCallback(() => {
    setFocusedKey(null);
    setYearly([]);
    setActiveThread(null);
    setThreadConversationIds(null);
    setThreadsLoadedFor(null);
    const params = new URLSearchParams(searchParams.toString());
    params.delete("h");
    params.delete("g");
    params.delete("y");
    const qs = params.toString();
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
  }, [pathname, router, searchParams]);

  const clearFocusAfterRemoval = useCallback(
    (keys: string[]) => {
      const removed = new Set(keys);
      setItems((prev) => prev.filter((i) => !removed.has(i.key)));
      clearSelection();
      if (focusedKey && removed.has(focusedKey)) clearFocus();
    },
    [clearFocus, clearSelection, focusedKey],
  );

  const runRestoreOrDelete = useCallback(
    async (keys: string[], permanent: boolean) => {
      if (keys.length === 0) return;
      if (permanent) {
        const msg =
          keys.length === 1
            ? "Delete forever?"
            : `Delete ${keys.length} items forever?`;
        if (!(await askConfirm(msg, "Delete"))) return;
      }
      setSaving(true);
      setCtxMenu(null);
      try {
        for (const key of keys) {
          const row = byKey.get(key);
          if (!row) continue;
          let res: Response;
          if (row.category === "groupMessages") {
            res = await fetch("/api/group-chats/trash", {
              method: "DELETE",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                conversationId: row.conversationId,
                permanent: permanent || undefined,
              }),
            });
          } else if (row.trashKind === "contact" && row.contactId != null) {
            res = await fetch("/api/contacts/trash", {
              method: "DELETE",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                ids: [row.contactId],
                permanent: permanent || undefined,
              }),
            });
          } else if (row.trashKind === "messages_only") {
            res = await fetch("/api/contacts/trash", {
              method: "DELETE",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                handle: row.handle,
                permanent: permanent || undefined,
              }),
            });
          } else {
            res = await fetch("/api/unassigned/trash", {
              method: "DELETE",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                handle: row.handle,
                permanent: permanent || undefined,
              }),
            });
          }
          const data = await res.json();
          if (!res.ok) throw new Error(data.error ?? "action failed");
        }
        setStatus(
          permanent
            ? keys.length === 1
              ? "Deleted forever"
              : `Deleted ${keys.length} forever`
            : keys.length === 1
              ? "Undeleted"
              : `Undeleted ${keys.length}`,
        );
        if (permanent) clearHistory();
        clearFocusAfterRemoval(keys);
        router.refresh();
      } catch (err) {
        console.error(err);
        setStatus(err instanceof Error ? err.message : "action failed");
        router.refresh();
      } finally {
        setSaving(false);
      }
    },
    [askConfirm, byKey, clearFocusAfterRemoval, clearHistory, router],
  );

  useDismissible({
    open: ctxMenu != null,
    onDismiss: () => setCtxMenu(null),
    refs: [ctxMenuRef],
    dismissOnPointerLeave: 0,
  });

  const clampMenu = (x: number, y: number, w: number, h: number) => ({
    x: Math.max(8, Math.min(x, window.innerWidth - w - 8)),
    y: Math.max(8, Math.min(y, window.innerHeight - h - 8)),
  });

  const openCtxMenuAt = (
    x: number,
    y: number,
    key: string,
    menuH: number,
  ) => {
    if (multiSelected && selectedKeys.has(key)) {
      setCtxMenu({ ...clampMenu(x, y, 200, menuH), key });
      return;
    }
    if (multiSelected) {
      selectItem(key);
      setCtxMenu({ ...clampMenu(x, y, 200, menuH), key });
      return;
    }
    if (key !== focusedKey) selectItem(key);
    setCtxMenu({ ...clampMenu(x, y, 200, menuH), key });
  };

  useEffect(() => {
    if (!focusedKey) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (ctxMenu != null) return;
      e.preventDefault();
      clearFocus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [focusedKey, ctxMenu, clearFocus]);

  const storage = usePanelLayoutStorage();
  const mainLayout = useDefaultLayout({
    id: "mv-trash-main-v4",
    panelIds: ["list", "thread"],
    storage,
  });

  return (
    <>
      <Group
        id="mv-trash-main-v4"
        orientation="horizontal"
        className="h-full w-full"
        defaultLayout={mainLayout.defaultLayout}
        onLayoutChanged={mainLayout.onLayoutChanged}
      >
        <Panel
          id="list"
          defaultSize={280}
          minSize={100}
          maxSize={560}
          className="min-h-0"
        >
          <TrashUnifiedList
            tab={tab}
            contactCount={tabCounts.contacts}
            groupCount={tabCounts.groupMessages}
            onTabChange={switchTab}
            selectAllRef={selectAllRef}
            allSelected={allSelected}
            query={query}
            onQueryChange={setQuery}
            items={tabItems}
            focusedKey={focusedKey}
            selectedKeys={selectedKeys}
            saving={saving}
            canDeleteForever={actionKeys.length > 0}
            onToggleSelectAll={toggleSelectAll}
            onSelectColumnClick={onSelectColumnClick}
            onRowClick={onRowClick}
            onRestoreHeader={() =>
              void runRestoreOrDelete(actionKeys, false)
            }
            onDeleteForeverHeader={() =>
              void runRestoreOrDelete(actionKeys, true)
            }
            onOpenCtxMenu={openCtxMenuAt}
          />
        </Panel>

        <PaneSeparator orientation="vertical" />

        <Panel id="thread" minSize="30%" className="min-h-0 min-w-0">
          <div className="flex h-full min-h-0 min-w-0 flex-col">
            <div className="flex h-[45px] shrink-0 items-center gap-2 border-b border-border px-5">
              <div className="flex min-w-0 flex-1 items-center justify-center">
                {headerTitle ? (
                  <h1 className="truncate text-lg font-semibold tracking-tight text-text">
                    {headerTitle}
                  </h1>
                ) : (
                  <span className="text-[13px] text-muted">
                    {multiSelected
                      ? `${selectedKeys.size} selected`
                      : "Choose an item from Trash"}
                  </span>
                )}
              </div>
              {status && (
                <span className="shrink-0 truncate text-[12px] text-muted">
                  {status}
                </span>
              )}
            </div>
            <div className="min-h-0 flex-1">
              {!multiSelected && focusedItem && activeThread ? (
                <BrowseThreadPane
                  detail={detail}
                  sources={sources}
                  messageSources={messageSources}
                  sourceCounts={sourceCounts}
                  source={source}
                  onSourceChange={setSource}
                  yearly={yearly}
                  messages={messages}
                  loadingMessages={loadingMessages}
                  threadsReady={
                    focusIsGroup || threadsLoadedFor === focusedItem.key
                  }
                  activeThread={activeThread}
                  groupThread={groupThreadMeta}
                  conversationsPanelCollapsed
                />
              ) : null}
            </div>
          </div>
        </Panel>
      </Group>

      {ctxMenu && (
        <div
          ref={ctxMenuRef}
          role="menu"
          className="fixed z-[100] min-w-[10rem] rounded-lg border border-border bg-popover py-1 shadow-xl"
          style={{ top: ctxMenu.y, left: ctxMenu.x }}
        >
          <button
            type="button"
            role="menuitem"
            className="flex w-full px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong"
            onClick={() => void runRestoreOrDelete(actionKeys, false)}
          >
            Undelete
          </button>
          <button
            type="button"
            role="menuitem"
            className="flex w-full px-3 py-1.5 text-left text-[13px] text-red-300 hover:bg-hover-strong"
            onClick={() => void runRestoreOrDelete(actionKeys, true)}
          >
            Delete forever
          </button>
        </div>
      )}
      {permanentConfirmDialog}
    </>
  );
}
