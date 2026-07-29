"use client";

import { formatPhoneDisplay } from "@/lib/phoneE164";
import {
  type TrashListItem,
  type TrashTab,
} from "@/lib/trashList";
import type { MouseEvent, ReactNode, RefObject } from "react";
import { CountBadge } from "./CountBadge";
import { TrashIcon } from "./icons";
import { TrashListChrome } from "./TrashListChrome";
import { TrashTabPicker } from "./TrashTabPicker";
import { GroupConversationRowBody } from "./GroupConversationRow";
import { useDateTimeFormat } from "./useDateTimeFormat";

export function TrashUnifiedList({
  tab,
  contactCount,
  groupCount,
  onTabChange,
  selectAllRef,
  allSelected,
  query,
  onQueryChange,
  items,
  focusedKey,
  selectedKeys,
  saving,
  canDeleteForever,
  onToggleSelectAll,
  onSelectColumnClick,
  onRowClick,
  onRestoreHeader,
  onDeleteForeverHeader,
  onOpenCtxMenu,
}: {
  tab: TrashTab;
  contactCount: number;
  groupCount: number;
  onTabChange: (tab: TrashTab) => void;
  selectAllRef: RefObject<HTMLInputElement | null>;
  allSelected: boolean;
  query: string;
  onQueryChange: (q: string) => void;
  items: TrashListItem[];
  focusedKey: string | null;
  selectedKeys: Set<string>;
  saving: boolean;
  canDeleteForever: boolean;
  onToggleSelectAll: () => void;
  onSelectColumnClick: (key: string, e: MouseEvent) => void;
  onRowClick: (key: string, e: MouseEvent) => void;
  onRestoreHeader: () => void;
  onDeleteForeverHeader: () => void;
  onOpenCtxMenu: (x: number, y: number, key: string, menuH: number) => void;
}) {
  const { formatDateTime } = useDateTimeFormat();
  const tabBar: ReactNode = (
    <TrashTabPicker
      tab={tab}
      contactCount={contactCount}
      groupCount={groupCount}
      onSwitch={onTabChange}
    />
  );

  const stickyLabel = tab === "group-messages" ? "Group Messages" : "Contacts";

  return (
    <aside className="flex h-full min-h-0 w-full flex-col bg-sidebar">
      <TrashListChrome
        tabBar={tabBar}
        selectAllRef={selectAllRef}
        allSelected={allSelected}
        selectedCount={selectedKeys.size}
        itemCount={items.length}
        query={query}
        onQueryChange={onQueryChange}
        saving={saving}
        canDeleteForever={canDeleteForever}
        onToggleSelectAll={onToggleSelectAll}
        onRestore={onRestoreHeader}
        onDeleteForever={onDeleteForeverHeader}
        selectAllLabel={
          tab === "group-messages"
            ? "Select all group messages"
            : "Select all contacts"
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        {items.length === 0 ? (
          <p className="px-3 py-4 text-[12px] text-muted">
            {query.trim()
              ? "No matches"
              : tab === "group-messages"
                ? "No group messages in Trash"
                : "No contacts in Trash"}
          </p>
        ) : (
          <>
            <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold text-muted">
              {stickyLabel}
            </div>
            {tab === "contacts"
              ? items.map((item, i) => {
                  if (item.category !== "contacts") return null;
                  const active = item.key === focusedKey;
                  const checked = selectedKeys.has(item.key);
                  const formattedHandle = item.handle
                    ? formatPhoneDisplay(item.handle)
                    : "";
                  const phone =
                    formattedHandle && formattedHandle !== item.displayName
                      ? formattedHandle
                      : null;
                  return (
                    <div
                      key={item.key}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        onOpenCtxMenu(e.clientX, e.clientY, item.key, 96);
                      }}
                      className={`relative flex items-start gap-0 border-b border-border/60 py-2 pr-3 ${
                        checked
                          ? "bg-accent/40 hover:bg-accent/50"
                          : active
                            ? "bg-accent/20 hover:bg-accent/25"
                            : "hover:bg-hover"
                      }`}
                    >
                      {active && !checked && (
                        <span
                          aria-hidden
                          className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
                        />
                      )}
                      <button
                        type="button"
                        aria-pressed={checked}
                        aria-label={`Select ${item.displayName}`}
                        onClick={(e) => onSelectColumnClick(item.key, e)}
                        onMouseDown={(e) => {
                          e.stopPropagation();
                          if (e.shiftKey) e.preventDefault();
                        }}
                        className="flex w-10 shrink-0 cursor-pointer items-center justify-center self-stretch -my-2"
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          readOnly
                          tabIndex={-1}
                          aria-hidden
                          className="checkbox-list pointer-events-none"
                        />
                      </button>
                      <button
                        type="button"
                        onClick={(e) => onRowClick(item.key, e)}
                        onMouseDown={(e) => {
                          if (e.shiftKey) e.preventDefault();
                        }}
                        className="flex min-w-0 flex-1 items-start justify-between gap-2 text-left"
                      >
                        <span className="min-w-0">
                          <span className="block truncate text-[13px] font-semibold text-text">
                            {item.displayName}
                            {item.unverified ? (
                              <span className="font-normal text-muted">
                                {" "}
                                (Unverified)
                              </span>
                            ) : null}
                          </span>
                          {phone ? (
                            <span className="block truncate text-[12px] text-muted">
                              {phone}
                            </span>
                          ) : (
                            <span
                              className="block h-[1.5rem] text-[12px]"
                              aria-hidden
                            />
                          )}
                          <span className="mt-0.5 flex items-center gap-1 text-[11px] text-muted tabular-nums">
                            <TrashIcon className="size-3 shrink-0 opacity-70" />
                            <span>
                              {item.trashedAt
                                ? formatDateTime(item.trashedAt)
                                : "—"}
                            </span>
                          </span>
                        </span>
                        {item.messageCount > 0 && (
                          <span className="shrink-0 pt-0.5">
                            <CountBadge
                              count={item.messageCount}
                              title="Messages"
                            />
                          </span>
                        )}
                      </button>
                    </div>
                  );
                })
              : items.map((item) => {
                  if (item.category !== "groupMessages") return null;
                  const active = item.key === focusedKey;
                  const checked = selectedKeys.has(item.key);
                  return (
                    <div
                      key={item.key}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        onOpenCtxMenu(e.clientX, e.clientY, item.key, 96);
                      }}
                      className={`relative flex items-start gap-0 border-b border-border/40 py-2.5 pr-3 ${
                        checked
                          ? "bg-accent/40 hover:bg-accent/50"
                          : active
                            ? "bg-accent/20 hover:bg-accent/25"
                            : "hover:bg-hover"
                      }`}
                    >
                      {active && !checked && (
                        <span
                          aria-hidden
                          className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
                        />
                      )}
                      <button
                        type="button"
                        aria-pressed={checked}
                        aria-label={`Select ${item.displayName}`}
                        onClick={(e) => onSelectColumnClick(item.key, e)}
                        onMouseDown={(e) => {
                          e.stopPropagation();
                          if (e.shiftKey) e.preventDefault();
                        }}
                        className="flex w-10 shrink-0 cursor-pointer items-center justify-center self-stretch -my-2.5"
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          readOnly
                          tabIndex={-1}
                          aria-hidden
                          className="checkbox-list pointer-events-none"
                        />
                      </button>
                      <button
                        type="button"
                        onClick={(e) => onRowClick(item.key, e)}
                        className="min-w-0 flex-1 text-left"
                      >
                        <GroupConversationRowBody
                          conversation={item.group}
                          variant="trash"
                          trashedAt={item.trashedAt}
                        />
                      </button>
                    </div>
                  );
                })}
          </>
        )}
      </div>
    </aside>
  );
}
