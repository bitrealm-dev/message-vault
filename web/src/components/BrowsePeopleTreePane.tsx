"use client";

import {
  browseTreeMode,
  directDateRange,
  type BrowseTreeMode,
} from "@/lib/browseTree";
import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import type { SearchConversationHit } from "@/lib/search";
import type { ContactListItem, YearThread } from "@/lib/types";
import {
  useRef,
  useState,
  type ChangeEvent,
  type MouseEvent,
  type ReactNode,
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
import { PencilIcon, TrashMessagesIcon, XIcon } from "./icons";
import { PaneSearchField } from "./PaneSearchField";
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

const CHILD_INDENT_PX = 20;

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
  vaultReadOnly = false,
  labelsMenu,
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
  groupSelectAllRef,
  allGroupsSelected,
  onToggleSelectAllGroups,
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
  searchHits = [],
  searchTotal = 0,
  searchLoading = false,
  onSelectSearchHit,
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
  vaultReadOnly?: boolean;
  labelsMenu?: ReactNode;
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
  groupSelectAllRef: RefObject<HTMLInputElement | null>;
  allGroupsSelected: boolean;
  onToggleSelectAllGroups: () => void;
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
  searchHits?: SearchConversationHit[];
  searchTotal?: number;
  searchLoading?: boolean;
  onSelectSearchHit?: (hit: SearchConversationHit) => void;
  onDirectClick?: () => void;
  directActive?: boolean;
  emptyGroupsLabel?: string;
}) {
  const vcfInputRef = useRef<HTMLInputElement>(null);
  const [vcfImporting, setVcfImporting] = useState(false);
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
    {
      key: "new-contact",
      label: "New",
      icon: <NewContactIcon className="size-5 shrink-0 opacity-80" />,
      onClick: (triggerEl) => {
        if (triggerEl) onNewContact(triggerEl);
      },
    },
    ...(onImportVcf
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
    ...(onEdit
      ? [
          {
            key: "edit",
            label: "Edit",
            icon: <PencilIcon className="size-5 shrink-0 opacity-80" />,
            disabled: editDisabled,
            onClick: (triggerEl) => {
              if (triggerEl) onEdit(triggerEl);
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(onTrashContact
      ? [
          {
            key: "delete",
            label: "Delete",
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
        <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
          <span className="text-[13px] text-muted">Search results</span>
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
              {!vaultReadOnly && labelsMenu}
              <SortByMenu
                sort={contactSort}
                order={contactSortOrder}
                onChange={onContactSortChange}
                scopeLabel="People"
                scopeLabelClassName="@[14rem]/tree-tools:inline hidden"
              />
              {(hasContactSelection || expandedContactId != null) && (
                <>
                  {hasContactSelection && (
                    <IconHoverTarget
                      label="Select all group messages"
                      placement="bottom"
                    >
                      <input
                        ref={groupSelectAllRef}
                        type="checkbox"
                        checked={allGroupsSelected}
                        disabled={groupItems.length === 0}
                        aria-label="Select all group messages"
                        onChange={onToggleSelectAllGroups}
                        className="checkbox-list"
                      />
                    </IconHoverTarget>
                  )}
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
                  {!vaultReadOnly && onTrashMessages && (
                    <IconHoverTarget
                      label="Delete group messages"
                      placement="bottom"
                    >
                      <button
                        type="button"
                        aria-label="Delete group messages"
                        disabled={trashDisabled}
                        onClick={onTrashMessages}
                        className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted transition-colors hover:border-red-500/40 hover:bg-red-500/15 hover:text-red-300 disabled:pointer-events-none disabled:opacity-40"
                      >
                        <TrashMessagesIcon className="size-4" />
                      </button>
                    </IconHoverTarget>
                  )}
                </>
              )}
              <ListHistoryMenu items={vaultReadOnly ? [] : menuItems} />
            </div>
          </div>
        </>
      )}

      {mode === "search" ? (
        <SearchResultsList
          hits={searchHits}
          total={searchTotal}
          loading={searchLoading}
          selectedConversationId={selectedConversationId}
          onSelect={(hit) => onSelectSearchHit?.(hit)}
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
                const active =
                  c.id === contactId || menuTarget || expanded || checked;
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
                      <div className="border-b border-border/40">
                        {loadingThreads ? (
                          <p
                            className="py-2.5 text-[12px] text-muted"
                            style={{ paddingLeft: CHILD_INDENT_PX + 40 }}
                          >
                            Loading…
                          </p>
                        ) : (
                          <>
                            {hasDirect && onDirectClick && (
                              <DirectConversationRow
                                active={directActive}
                                dateStart={directRange.dateStart}
                                dateEnd={directRange.dateEnd}
                                indentPx={CHILD_INDENT_PX}
                                onClick={onDirectClick}
                              />
                            )}
                            {groupItems.length === 0 ? (
                              !hasDirect ? (
                                <p
                                  className="py-2.5 text-[12px] text-muted"
                                  style={{
                                    paddingLeft: CHILD_INDENT_PX + 40,
                                  }}
                                >
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
                                  indentPx={CHILD_INDENT_PX}
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
