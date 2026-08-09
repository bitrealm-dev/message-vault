"use client";

import type { ReactNode, RefObject } from "react";
import { RestoreIcon, XIcon } from "./icons";
import {
  GroupTrashSortMenu,
  TrashSortMenu,
  type GroupTrashSortBy,
  type SortOrder,
  type TrashSortBy,
} from "./SortByMenu";

/** Handlers + state the active trash tab reports to the shared chrome. */
export type TrashChromeController = {
  selectAllRef: RefObject<HTMLInputElement | null>;
  allSelected: boolean;
  selectedCount: number;
  itemCount: number;
  query: string;
  onQueryChange: (q: string) => void;
  saving: boolean;
  canDeleteForever: boolean;
  onToggleSelectAll: () => void;
  onRestore?: () => void;
  onDeleteForever: () => void;
  selectAllLabel?: string;
  sort?: {
    kind: "contacts" | "groups";
    sortBy: string;
    order: SortOrder;
    onChange: (next: { sortBy: string; order: SortOrder }) => void;
  };
};

/** Shared list toolbar + search (trash forever, or active soft-delete). */
export function TrashListChrome({
  tabBar,
  selectAllRef,
  allSelected,
  selectedCount,
  itemCount,
  query,
  onQueryChange,
  saving,
  canDeleteForever,
  onToggleSelectAll,
  onRestore,
  onDeleteForever,
  selectAllLabel = "Select all trash",
  sort,
  trailing,
}: TrashChromeController & {
  tabBar?: ReactNode;
  /** trash = permanent delete; active = soft-delete (move to trash). */
  variant?: "trash" | "active";
  /** e.g. ListHistoryMenu — sits with sort on the right. */
  trailing?: ReactNode;
}) {
  const sortMenu =
    sort?.kind === "groups" ? (
      <GroupTrashSortMenu
        sortBy={sort.sortBy as GroupTrashSortBy}
        order={sort.order}
        onChange={(next) =>
          sort.onChange({ sortBy: next.sortBy, order: next.order })
        }
      />
    ) : sort ? (
      <TrashSortMenu
        sortBy={sort.sortBy as TrashSortBy}
        order={sort.order}
        onChange={(next) =>
          sort.onChange({ sortBy: next.sortBy, order: next.order })
        }
      />
    ) : null;

  return (
    <>
      <div className="flex h-[45px] shrink-0 items-center justify-between gap-2 overflow-visible border-b border-border bg-sidebar px-3">
        <div className="flex min-w-0 items-center gap-1.5">
          <label className="flex min-w-0 items-center gap-2">
            <input
              ref={selectAllRef}
              type="checkbox"
              checked={allSelected}
              disabled={itemCount === 0}
              aria-label={selectAllLabel}
              onChange={onToggleSelectAll}
              className="checkbox-list"
            />
            <span className="truncate text-[13px] text-muted tabular-nums">
              {selectedCount > 0 ? selectedCount : ""}
            </span>
          </label>
          {tabBar}
        </div>
        <div className="flex shrink-0 items-center gap-1.5 overflow-visible">
          {onRestore && (
            <button
              type="button"
              aria-label="Restore"
              title="Restore"
              disabled={saving || !canDeleteForever}
              onClick={onRestore}
              className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted transition-colors hover:border-accent/40 hover:bg-accent/15 hover:text-accent disabled:pointer-events-none disabled:opacity-40"
            >
              <RestoreIcon className="size-3.5 shrink-0 opacity-80" />
            </button>
          )}
          <button
            type="button"
            aria-label="Delete forever"
            title="Delete forever"
            disabled={saving || !canDeleteForever}
            onClick={onDeleteForever}
            className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted transition-colors hover:border-red-500/40 hover:bg-red-500/15 hover:text-red-300 disabled:pointer-events-none disabled:opacity-40"
          >
            <XIcon className="size-3.5 shrink-0 opacity-80" />
          </button>
          {sortMenu}
          {trailing}
        </div>
      </div>
      <div className="flex h-[45px] shrink-0 items-center border-b border-border bg-sidebar px-3">
        <input
          type="search"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          placeholder="Search by name or phone…"
          className="w-full rounded-md border border-border bg-elevated px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent"
        />
      </div>
    </>
  );
}
