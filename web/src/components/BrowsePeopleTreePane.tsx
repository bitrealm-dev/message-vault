"use client";

import {
  browseTreeMode,
  directDateRange,
  type BrowseTreeMode,
} from "@/lib/browseTree";
import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import type { SearchContactHit, SearchConversationHit } from "@/lib/search";
import type { SearchResultKey } from "@/lib/searchSelection";
import type { ContactListItem, YearThread } from "@/lib/types";
import {
  useRef,
  useState,
  type ChangeEvent,
  type MouseEvent,
  type RefObject,
} from "react";
import { BrowseContactRow } from "./BrowseContactRow";
import {
  DirectConversationRow,
  GroupConversationRow,
} from "./BrowseConversationRows";
import { NewContactIcon } from "./BrowseContactList";
import { ListHistoryMenu, type ListHistoryMenuItem } from "./history";
import { IconHoverTarget } from "./IconHoverLabel";
import {
  ChevronDownIcon,
  LockIcon,
  PencilIcon,
  PeopleGroupIcon,
  TrashMessagesIcon,
  XIcon,
} from "./icons";
import { PaneSearchField } from "./PaneSearchField";
import { SearchContactResultsList } from "./SearchContactResultsList";
import { SearchResultsList } from "./SearchResultsList";
import {
  BrowseGroupChatSortMenu,
  SortByMenu,
  type BrowseGroupChatSortBy,
  type SortMode,
  type SortOrder,
} from "./SortByMenu";
import { VaultSearchField } from "./VaultSearchField";
import { YearFilterMenu } from "./YearFilterMenu";

/**
 * Left edge of contact name / phone / first label: chevron (w-6) +
 * gap-1.5 + avatar column (w-10) + gap-1.5.
 */
const NESTED_FROM_NAME_PX = 76;
const EMPTY_SEARCH_EXPANSION: ReadonlySet<number> = new Set();

export function BrowsePeopleTreePane({
  sectionLabel,
  // Contact list / filter
  contactQuery,
  onContactQueryChange,
  grouped,
  sortedCount,
  visibleCount,
  contactId,
  contextMenuId = null,
  selectedContactIds,
  contactSelectAllRef,
  allContactsSelected,
  onToggleSelectAllContacts,
  onContactSelectColumnClick,
  onContactNamePhoneClick,
  onContactContextMenu,
  onToggleExpandContact,
  expandedContactId,
  // Contact toolbar
  onNewContact,
  onImportVcf,
  onExportContactsCsv,
  vaultReadOnly = false,
  onLabels,
  labelsDisabled = false,
  onEdit,
  editDisabled = false,
  onTrashContact,
  deleteDisabled = false,
  contactSort,
  contactSortOrder,
  onContactSortChange,
  // Expanded contact threads
  yearly,
  groupItems,
  loadingThreads = false,
  // Shared groups / group selection
  selectedConversationId,
  selectedGroupIds,
  onGroupSelectColumnClick,
  onGroupRowClick,
  onTrashMessages,
  trashDisabled = false,
  years,
  filterYear,
  onFilterYearChange,
  groupSortBy,
  groupSortOrder,
  onGroupSortChange,
  // Search
  searchQuery,
  onSearchQueryChange,
  onSearchSubmit,
  searchSources = [],
  searchLabels = [],
  resultsMode = false,
  searchMode = "messages",
  searchHits = [],
  searchContactHits = [],
  searchTotal = 0,
  searchLoading = false,
  searchHighlightTerms,
  searchContactIds = [],
  allSearchContactsSelected = false,
  onToggleSelectAllSearchContacts,
  onToggleSearchContact,
  onOpenSearchContact,
  selectedSearchResultKeys,
  allSearchResultsSelected = false,
  onToggleSelectAllSearchResults,
  onToggleSearchResult,
  onSelectSearchHit,
  onSearchContactContextMenu,
  onSearchResultContextMenu,
  onDeleteSearchResults,
  onUnlockVault,
  // Direct
  onDirectClick,
  directActive = false,
  emptyGroupsLabel = "No group messages",
}: {
  sectionLabel: string;
  contactQuery: string;
  onContactQueryChange: (q: string) => void;
  grouped: [string, ContactListItem[]][];
  sortedCount: number;
  visibleCount: number;
  contactId: number | null;
  contextMenuId?: number | null;
  selectedContactIds: Set<number>;
  contactSelectAllRef: RefObject<HTMLInputElement | null>;
  allContactsSelected: boolean;
  onToggleSelectAllContacts: () => void;
  onContactSelectColumnClick: (id: number, e: MouseEvent) => void;
  onContactNamePhoneClick: (
    id: number,
    e: MouseEvent | { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
  ) => void;
  onContactContextMenu: (id: number, x: number, y: number) => void;
  onToggleExpandContact: (id: number) => void;
  expandedContactId: number | null;
  onNewContact: (anchorEl: HTMLElement) => void;
  onImportVcf?: (file: File) => Promise<void>;
  onExportContactsCsv?: () => void;
  vaultReadOnly?: boolean;
  onUnlockVault?: () => void;
  onLabels?: (anchorEl: HTMLElement) => void;
  labelsDisabled?: boolean;
  onEdit?: (anchorEl: HTMLElement) => void;
  editDisabled?: boolean;
  onTrashContact?: () => void;
  deleteDisabled?: boolean;
  contactSort: SortMode;
  contactSortOrder: SortOrder;
  onContactSortChange: (next: { sort: SortMode; order: SortOrder }) => void;
  yearly: YearThread[];
  groupItems: CollapsedGroupConversation[];
  loadingThreads?: boolean;
  selectedConversationId: number | null;
  selectedGroupIds: Set<number>;
  onGroupSelectColumnClick: (id: number, e: MouseEvent) => void;
  onGroupRowClick: (
    id: number,
    e: MouseEvent | { shiftKey: boolean; metaKey?: boolean; ctrlKey?: boolean },
  ) => void;
  onTrashMessages?: () => void;
  trashDisabled?: boolean;
  years: number[];
  filterYear: number | null;
  onFilterYearChange: (year: number | null) => void;
  groupSortBy: BrowseGroupChatSortBy;
  groupSortOrder: SortOrder;
  onGroupSortChange: (next: {
    sortBy: BrowseGroupChatSortBy;
    order: SortOrder;
  }) => void;
  searchQuery: string;
  onSearchQueryChange: (query: string) => void;
  onSearchSubmit: (query: string) => void;
  searchSources?: string[];
  searchLabels?: string[];
  resultsMode?: boolean;
  searchMode?: "contacts" | "messages";
  searchHits?: SearchConversationHit[];
  searchContactHits?: SearchContactHit[];
  searchTotal?: number;
  searchLoading?: boolean;
  searchHighlightTerms?: string[];
  searchContactIds?: number[];
  allSearchContactsSelected?: boolean;
  onToggleSelectAllSearchContacts?: () => void;
  onToggleSearchContact?: (
    contactId: number,
    mods?: { shiftKey: boolean },
  ) => void;
  onOpenSearchContact?: (contactId: number) => void;
  selectedSearchResultKeys?: ReadonlySet<SearchResultKey>;
  allSearchResultsSelected?: boolean;
  onToggleSelectAllSearchResults?: () => void;
  onToggleSearchResult?: (
    hit: SearchConversationHit,
    mods: {
      shiftKey: boolean;
      altKey: boolean;
      metaKey: boolean;
      ctrlKey: boolean;
    },
  ) => void;
  onSelectSearchHit?: (
    hit: SearchConversationHit,
    mods?: {
      shiftKey: boolean;
      altKey: boolean;
      metaKey: boolean;
      ctrlKey: boolean;
    },
  ) => void;
  onSearchContactContextMenu?: (id: number, x: number, y: number) => void;
  onSearchResultContextMenu?: (
    hit: SearchConversationHit,
    x: number,
    y: number,
  ) => void;
  onDeleteSearchResults?: () => void;
  onDirectClick?: () => void;
  directActive?: boolean;
  emptyGroupsLabel?: string;
}) {
  const vcfInputRef = useRef<HTMLInputElement>(null);
  const [vcfImporting, setVcfImporting] = useState(false);
  const [searchExpansion, setSearchExpansion] = useState<{
    query: string;
    ids: ReadonlySet<number>;
  }>({ query: "", ids: EMPTY_SEARCH_EXPANSION });
  const expandedSearchContactIds =
    searchExpansion.query === searchQuery
      ? searchExpansion.ids
      : EMPTY_SEARCH_EXPANSION;
  const allSearchContactsExpanded =
    searchContactHits.length > 0 &&
    searchContactHits.every(({ contact }) =>
      expandedSearchContactIds.has(contact.id),
    );
  const toggleSearchContactExpanded = (id: number) => {
    setSearchExpansion((prev) => {
      const next =
        prev.query === searchQuery ? new Set(prev.ids) : new Set<number>();
      if (!next.delete(id)) next.add(id);
      return { query: searchQuery, ids: next };
    });
  };
  const toggleAllSearchContactsExpanded = () => {
    setSearchExpansion({
      query: searchQuery,
      ids: allSearchContactsExpanded
        ? EMPTY_SEARCH_EXPANSION
        : new Set(searchContactHits.map(({ contact }) => contact.id)),
    });
  };
  const hasContactSelection = selectedContactIds.size >= 1;
  const mode: BrowseTreeMode = browseTreeMode({
    resultsMode,
    hasContactSelection,
  });

  const onVcfPicked = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file || !onImportVcf) return;
    setVcfImporting(true);
    try {
      await onImportVcf(file);
    } finally {
      setVcfImporting(false);
    }
  };

  const menuItems: ListHistoryMenuItem[] = [
    ...(vaultReadOnly && onUnlockVault
      ? [
          {
            key: "unlock-vault",
            label: "Unlock vault to edit",
            icon: <LockIcon className="size-5 shrink-0 opacity-80" />,
            onClick: () => onUnlockVault(),
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(!vaultReadOnly
      ? [
          {
            key: "new-contact",
            label: "New",
            icon: <NewContactIcon className="size-5 shrink-0 opacity-80" />,
            onClick: (triggerEl: HTMLElement | null) => {
              if (triggerEl) onNewContact(triggerEl);
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(!vaultReadOnly && onImportVcf
      ? [
          {
            key: "import-vcf",
            label: vcfImporting ? "Importing…" : "Import VCF",
            icon: <ImportVcfIcon className="size-5 shrink-0 opacity-80" />,
            disabled: vcfImporting,
            onClick: () => {
              vcfInputRef.current?.click();
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(onExportContactsCsv
      ? [
          {
            key: "export-contacts-csv",
            label: "Export contacts CSV",
            icon: <ExportCsvIcon className="size-5 shrink-0 opacity-80" />,
            onClick: () => onExportContactsCsv(),
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(!vaultReadOnly && onEdit
      ? [
          {
            key: "edit",
            label: "Edit",
            icon: <PencilIcon className="size-5 shrink-0 opacity-80" />,
            disabled: editDisabled,
            onClick: (triggerEl: HTMLElement | null) => {
              if (triggerEl) onEdit(triggerEl);
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(!vaultReadOnly && onLabels
      ? [
          {
            key: "labels",
            label: "Labels",
            icon: <PeopleGroupIcon className="size-5 shrink-0 opacity-80" />,
            disabled: labelsDisabled,
            onClick: (triggerEl: HTMLElement | null) => {
              if (triggerEl) onLabels(triggerEl);
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(!vaultReadOnly &&
    onTrashMessages &&
    (hasContactSelection || expandedContactId != null)
      ? [
          {
            key: "delete-group-messages",
            label: "Delete group messages",
            icon: (
              <TrashMessagesIcon className="size-5 shrink-0 opacity-80" />
            ),
            disabled: trashDisabled,
            danger: true,
            onClick: () => onTrashMessages(),
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(!vaultReadOnly && onTrashContact
      ? [
          {
            key: "delete",
            label:
              selectedContactIds.size > 1
                ? "Delete contacts"
                : "Delete contact",
            icon: <XIcon className="size-5 shrink-0 opacity-80" />,
            disabled: deleteDisabled,
            danger: true,
            onClick: () => onTrashContact(),
          } satisfies ListHistoryMenuItem,
        ]
      : []),
  ];

  const directRange = directDateRange(yearly);
  const hasDirect = yearly.some((y) => y.conversationIds.length > 0);
  const groupSelectionActive = selectedGroupIds.size >= 1;
  const contactSelectionActive = selectedContactIds.size >= 1;
  const selectedSearchContactCount = searchContactIds.reduce(
    (count, id) => count + (selectedContactIds.has(id) ? 1 : 0),
    0,
  );
  const selectedSearchResultCount = selectedSearchResultKeys?.size ?? 0;
  const searchMenuItems: ListHistoryMenuItem[] =
    searchMode === "messages"
      ? [
          ...(vaultReadOnly && onUnlockVault
            ? [
                {
                  key: "unlock-vault",
                  label: "Unlock vault to edit",
                  icon: <LockIcon className="size-5 shrink-0 opacity-80" />,
                  onClick: () => onUnlockVault(),
                } satisfies ListHistoryMenuItem,
              ]
            : []),
          ...(!vaultReadOnly && onDeleteSearchResults
            ? [
                {
                  key: "delete-search-messages",
                  label:
                    selectedSearchResultCount === 1
                      ? "Delete message"
                      : "Delete messages",
                  icon: <TrashMessagesIcon className="size-5 shrink-0 opacity-80" />,
                  disabled: selectedSearchResultCount === 0,
                  danger: true,
                  onClick: () => onDeleteSearchResults(),
                } satisfies ListHistoryMenuItem,
              ]
            : []),
        ]
      : menuItems;

  return (
    <aside className="flex h-full min-h-0 w-full flex-col bg-sidebar">
      {onImportVcf && (
        <input
          ref={vcfInputRef}
          type="file"
          accept=".vcf,.vcard,text/vcard,text/x-vcard"
          className="hidden"
          onChange={(e) => void onVcfPicked(e)}
        />
      )}

      <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
        <VaultSearchField
          value={searchQuery}
          onChange={onSearchQueryChange}
          onSubmit={onSearchSubmit}
          sources={searchSources}
          labels={searchLabels}
        />
      </div>

      {mode === "search" ? (
        <div className="@container/tree-tools flex h-[45px] shrink-0 items-center justify-between overflow-visible border-b border-border px-3">
          <div className="flex min-w-0 items-center gap-2">
            <IconHoverTarget
              label={
                searchMode === "contacts"
                  ? "Select all matching people"
                  : "Select all matching messages"
              }
              placement="bottom"
            >
              <input
                type="checkbox"
                checked={
                  searchMode === "contacts"
                    ? allSearchContactsSelected
                    : allSearchResultsSelected
                }
                disabled={
                  searchMode === "contacts"
                    ? searchContactIds.length === 0
                    : searchHits.length === 0
                }
                aria-label={
                  searchMode === "contacts"
                    ? "Select all matching people"
                    : "Select all matching messages"
                }
                onChange={
                  searchMode === "contacts"
                    ? onToggleSelectAllSearchContacts
                    : onToggleSelectAllSearchResults
                }
                className="checkbox-list"
              />
            </IconHoverTarget>
            <span className="text-[13px] text-muted">Search results</span>
            <span className="text-[13px] text-muted tabular-nums">
              {(searchMode === "contacts"
                ? selectedSearchContactCount
                : selectedSearchResultCount) > 0
                ? (searchMode === "contacts"
                    ? selectedSearchContactCount
                    : selectedSearchResultCount
                  ).toLocaleString()
                : ""}
            </span>
          </div>
          <div className="flex shrink-0 items-center gap-1.5 overflow-visible">
            {searchMode === "contacts" ? (
              <IconHoverTarget
                label={
                  allSearchContactsExpanded ? "Collapse all" : "Expand all"
                }
                placement="bottom"
              >
                <button
                  type="button"
                  disabled={searchContactHits.length === 0}
                  aria-label={
                    allSearchContactsExpanded ? "Collapse all" : "Expand all"
                  }
                  onClick={toggleAllSearchContactsExpanded}
                  className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted transition-colors hover:bg-hover hover:text-text disabled:pointer-events-none disabled:opacity-40"
                >
                  <ChevronDownIcon
                    className={`size-4 ${
                      allSearchContactsExpanded ? "rotate-180" : ""
                    }`}
                  />
                </button>
              </IconHoverTarget>
            ) : null}
            <SortByMenu
              sort={contactSort}
              order={contactSortOrder}
              onChange={onContactSortChange}
              scopeLabel="People"
              scopeLabelClassName="@[14rem]/tree-tools:inline hidden"
            />
            <ListHistoryMenu items={searchMenuItems} />
          </div>
        </div>
      ) : (
        <>
          <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
            <PaneSearchField
              value={contactQuery}
              onChange={onContactQueryChange}
              placeholder="Filter contacts"
            />
          </div>
          <div className="@container/tree-tools flex h-[45px] shrink-0 items-center justify-between overflow-visible border-b border-border px-3">
            <label className="flex min-w-0 items-center gap-2">
              <IconHoverTarget label="Select all" placement="bottom">
                <input
                  ref={contactSelectAllRef}
                  type="checkbox"
                  checked={allContactsSelected}
                  disabled={visibleCount === 0}
                  aria-label={`Select all ${sectionLabel}`}
                  onChange={onToggleSelectAllContacts}
                  className="checkbox-list"
                />
              </IconHoverTarget>
              <span className="truncate text-[13px] text-muted tabular-nums">
                {selectedContactIds.size > 0 ? selectedContactIds.size : ""}
              </span>
            </label>
            <div className="flex shrink-0 items-center gap-1.5 overflow-visible">
              <SortByMenu
                sort={contactSort}
                order={contactSortOrder}
                onChange={onContactSortChange}
                scopeLabel="People"
                scopeLabelClassName="@[14rem]/tree-tools:inline hidden"
              />
              {(hasContactSelection || expandedContactId != null) && (
                <>
                  <YearFilterMenu
                    years={years}
                    value={filterYear}
                    onChange={onFilterYearChange}
                  />
                  <BrowseGroupChatSortMenu
                    sortBy={groupSortBy}
                    order={groupSortOrder}
                    onChange={onGroupSortChange}
                    disabled={groupItems.length === 0}
                    scopeLabel="Chats"
                    scopeLabelClassName="@[14rem]/tree-tools:inline hidden"
                  />
                </>
              )}
              <ListHistoryMenu items={menuItems} />
            </div>
          </div>
        </>
      )}

      {mode === "search" && searchMode === "contacts" ? (
        <SearchContactResultsList
          contacts={searchContactHits}
          total={searchTotal}
          loading={searchLoading}
          highlightTerms={searchHighlightTerms}
          selectedConversationId={selectedConversationId}
          selectedContactIds={selectedContactIds}
          expandedContactIds={expandedSearchContactIds}
          contactId={contactId}
          onToggleContact={(id, mods) => onToggleSearchContact?.(id, mods)}
          onOpenContact={onOpenSearchContact}
          onToggleExpand={toggleSearchContactExpanded}
          onSelectHit={(hit) => onSelectSearchHit?.(hit)}
          onContactContextMenu={onSearchContactContextMenu}
        />
      ) : mode === "search" ? (
        <SearchResultsList
          hits={searchHits}
          total={searchTotal}
          loading={searchLoading}
          highlightTerms={searchHighlightTerms}
          selectedConversationId={selectedConversationId}
          selectedResultKeys={selectedSearchResultKeys}
          onToggleResult={onToggleSearchResult}
          onSelect={(hit, mods) => onSelectSearchHit?.(hit, mods)}
          onResultContextMenu={onSearchResultContextMenu}
          emptyLabel="No matches"
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
          {sortedCount === 0 && (
            <p className="px-3 py-4 text-[12px] text-muted">No matches</p>
          )}
          {grouped.map(([letter, items]) => (
            <div key={letter || "all"}>
              {!contactQuery.trim() && letter && (
                <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold text-muted">
                  {letter}
                </div>
              )}
              {items.map((c, i) => {
                const menuTarget =
                  contextMenuId != null && c.id === contextMenuId;
                // While contacts are checkbox-selected, keep the list flat so
                // shared groups can appear as their own section below.
                const expanded =
                  !hasContactSelection && expandedContactId === c.id;
                const checked = selectedContactIds.has(c.id);
                // Only the focused contact (or context-menu target) is active —
                // not merely expanded/checked, so the previous contact drops
                // its highlight when focus moves.
                const active = c.id === contactId || menuTarget;
                return (
                  <div key={c.id}>
                    <BrowseContactRow
                      contact={c}
                      active={active}
                      checked={checked}
                      selectionActive={contactSelectionActive}
                      expanded={expanded}
                      showExpandChevron={!hasContactSelection}
                      showInsetDivider={!expanded && i < items.length - 1}
                      onSelectColumnClick={onContactSelectColumnClick}
                      onNamePhoneClick={onContactNamePhoneClick}
                      onContextMenu={onContactContextMenu}
                      onToggleExpand={onToggleExpandContact}
                    />
                    {expanded && (
                      <div
                        className="mb-1 mr-2 overflow-hidden rounded-md bg-elevated/55"
                        style={{ marginLeft: NESTED_FROM_NAME_PX }}
                      >
                        {loadingThreads ? (
                          <p className="px-3 py-2.5 text-[12px] text-muted">
                            Loading…
                          </p>
                        ) : (
                          <>
                            {hasDirect && onDirectClick && (
                              <DirectConversationRow
                                active={directActive}
                                dateStart={directRange.dateStart}
                                dateEnd={directRange.dateEnd}
                                showBorder={groupItems.length > 0}
                                nested
                                onClick={onDirectClick}
                              />
                            )}
                            {groupItems.length === 0 ? (
                              !hasDirect ? (
                                <p className="px-3 py-2.5 text-[12px] text-muted">
                                  {emptyGroupsLabel}
                                </p>
                              ) : null
                            ) : (
                              groupItems.map((g, gi) => (
                                <GroupConversationRow
                                  key={g.conversationId}
                                  conversation={g}
                                  active={
                                    g.conversationId === selectedConversationId
                                  }
                                  checked={selectedGroupIds.has(
                                    g.conversationId,
                                  )}
                                  selectionActive={groupSelectionActive}
                                  showBorder={gi < groupItems.length - 1}
                                  nested
                                  onSelectColumnClick={onGroupSelectColumnClick}
                                  onRowClick={onGroupRowClick}
                                />
                              ))
                            )}
                          </>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          {hasContactSelection && (
            <>
              <div className="sticky top-0 z-10 border-b border-t border-border bg-sidebar px-3 py-1 text-[11px] font-semibold text-muted">
                {selectedContactIds.size > 1
                  ? "Shared group messages"
                  : "Group messages"}
              </div>
              {groupItems.length === 0 ? (
                <p className="px-3 py-4 text-[12px] text-muted">
                  {emptyGroupsLabel}
                </p>
              ) : (
                groupItems.map((g, i) => (
                  <GroupConversationRow
                    key={g.conversationId}
                    conversation={g}
                    active={g.conversationId === selectedConversationId}
                    checked={selectedGroupIds.has(g.conversationId)}
                    selectionActive={groupSelectionActive}
                    showBorder={i < groupItems.length - 1}
                    onSelectColumnClick={onGroupSelectColumnClick}
                    onRowClick={onGroupRowClick}
                  />
                ))
              )}
            </>
          )}
        </div>
      )}
    </aside>
  );
}

function ImportVcfIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M12 3v12" />
      <path d="m7 10 5 5 5-5" />
      <path d="M5 19h14" />
    </svg>
  );
}

function ExportCsvIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M12 21V9" />
      <path d="m7 14 5 5 5-5" />
      <path d="M5 5h14" />
    </svg>
  );
}
