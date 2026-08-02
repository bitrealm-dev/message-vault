"use client";

import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import type { SearchConversationHit } from "@/lib/search";
import type { MouseEvent, RefObject } from "react";
import {
  DirectConversationRow,
  GroupConversationRow,
} from "./BrowseConversationRows";
import { IconHoverTarget } from "./IconHoverLabel";
import { TrashMessagesIcon } from "./icons";
import { SearchResultsList } from "./SearchResultsList";
import {
  BrowseGroupChatSortMenu,
  type BrowseGroupChatSortBy,
  type SortOrder,
} from "./SortByMenu";
import { VaultSearchField } from "./VaultSearchField";
import { YearFilterMenu } from "./YearFilterMenu";

export function BrowseGroupChatsPane({
  items,
  selectedConversationId,
  selectedIds,
  selectAllRef,
  allSelected,
  onToggleSelectAll,
  onSelectColumnClick,
  onRowClick,
  onTrashMessages,
  trashDisabled = false,
  vaultReadOnly = false,
  years,
  filterYear,
  onFilterYearChange,
  sortBy,
  sortOrder,
  onSortChange,
  searchQuery,
  onSearchQueryChange,
  onSearchSubmit,
  searchSources = [],
  searchLabels = [],
  resultsMode = false,
  searchHits = [],
  searchTotal = 0,
  searchLoading = false,
  searchHighlightTerms,
  onSelectSearchHit,
  emptyLabel = "No group messages",
  directAvailable = false,
  directActive = false,
  directDateStart = null,
  directDateEnd = null,
  onDirectClick,
}: {
  items: CollapsedGroupConversation[];
  selectedConversationId: number | null;
  selectedIds: Set<number>;
  selectAllRef: RefObject<HTMLInputElement | null>;
  allSelected: boolean;
  onToggleSelectAll: () => void;
  onSelectColumnClick: (id: number, e: MouseEvent) => void;
  onRowClick: (
    id: number,
    e: MouseEvent | { shiftKey: boolean; metaKey?: boolean; ctrlKey?: boolean },
  ) => void;
  onTrashMessages?: () => void;
  trashDisabled?: boolean;
  vaultReadOnly?: boolean;
  years: number[];
  filterYear: number | null;
  onFilterYearChange: (year: number | null) => void;
  sortBy: BrowseGroupChatSortBy;
  sortOrder: SortOrder;
  onSortChange: (next: {
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
  searchHighlightTerms?: string[];
  onSelectSearchHit?: (hit: SearchConversationHit) => void;
  emptyLabel?: string;
  /** Synthetic 1:1 chooser row (focused contact path only). */
  directAvailable?: boolean;
  directActive?: boolean;
  directDateStart?: string | null;
  directDateEnd?: string | null;
  onDirectClick?: () => void;
}) {
  const selectionActive = selectedIds.size >= 1;

  return (
    <aside className="flex h-full min-h-0 w-full flex-col bg-sidebar">
      <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
        <VaultSearchField
          value={searchQuery}
          onChange={onSearchQueryChange}
          onSubmit={onSearchSubmit}
          sources={searchSources}
          labels={searchLabels}
        />
      </div>
      {!resultsMode ? (
        <div className="flex h-[45px] shrink-0 items-center justify-between gap-2 border-b border-border px-3">
          <label className="flex min-w-0 items-center gap-2">
            <IconHoverTarget label="Select all" placement="bottom">
              <input
                ref={selectAllRef}
                type="checkbox"
                checked={allSelected}
                disabled={items.length === 0}
                aria-label="Select all group messages"
                onChange={onToggleSelectAll}
                className="checkbox-list"
              />
            </IconHoverTarget>
            <span className="truncate text-[13px] text-muted tabular-nums">
              {selectedIds.size > 0 ? selectedIds.size : ""}
            </span>
          </label>
          <div className="flex shrink-0 items-center gap-1.5">
            <YearFilterMenu
              years={years}
              value={filterYear}
              onChange={onFilterYearChange}
            />
            <BrowseGroupChatSortMenu
              sortBy={sortBy}
              order={sortOrder}
              onChange={onSortChange}
              disabled={items.length === 0}
            />
            {!vaultReadOnly && onTrashMessages && (
              <IconHoverTarget label="Delete group messages" placement="bottom">
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
          </div>
        </div>
      ) : (
        <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
          <span className="text-[13px] text-muted">Search results</span>
        </div>
      )}
      {resultsMode ? (
        <SearchResultsList
          hits={searchHits}
          total={searchTotal}
          loading={searchLoading}
          highlightTerms={searchHighlightTerms}
          selectedConversationId={selectedConversationId}
          onSelect={(hit) => onSelectSearchHit?.(hit)}
          emptyLabel="No matches"
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
          {directAvailable && onDirectClick && (
            <DirectConversationRow
              active={directActive}
              dateStart={directDateStart}
              dateEnd={directDateEnd}
              showBorder
              onClick={onDirectClick}
            />
          )}
          <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold text-muted">
            Group Messages
          </div>
          {items.length === 0 ? (
            <p className="px-3 py-4 text-[12px] text-muted">{emptyLabel}</p>
          ) : (
            items.map((g, i) => (
              <GroupConversationRow
                key={g.conversationId}
                conversation={g}
                active={g.conversationId === selectedConversationId}
                checked={selectedIds.has(g.conversationId)}
                selectionActive={selectionActive}
                showBorder={i < items.length - 1}
                onSelectColumnClick={onSelectColumnClick}
                onRowClick={onRowClick}
              />
            ))
          )}
        </div>
      )}
    </aside>
  );
}
