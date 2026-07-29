"use client";

import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import type { MouseEvent } from "react";
import { GroupConversationRowBody } from "./GroupConversationRow";
import { useDateTimeFormat } from "./useDateTimeFormat";

export function DirectConversationRow({
  active,
  dateStart = null,
  dateEnd = null,
  indentPx = 0,
  onClick,
}: {
  active: boolean;
  dateStart?: string | null;
  dateEnd?: string | null;
  indentPx?: number;
  onClick: () => void;
}) {
  const { formatDateRange } = useDateTimeFormat();
  const dateLabel =
    dateStart && dateEnd
      ? formatDateRange(dateStart, dateEnd, " – ")
      : null;

  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`group relative flex w-full items-start gap-1.5 border-b border-border/40 py-2.5 pr-3 text-left select-none outline-none focus:outline-none focus-visible:outline-none ${
        active
          ? "bg-accent/20 hover:bg-accent/25"
          : "hover:bg-hover-strong"
      }`}
      style={{ paddingLeft: indentPx }}
    >
      {active && (
        <span
          aria-hidden
          className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
        />
      )}
      <span className="w-10 shrink-0" aria-hidden />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-medium leading-snug text-text">
          1-1 messages
        </span>
        {dateLabel ? (
          <span className="mt-0.5 block text-right text-[11px] text-muted tabular-nums">
            {dateLabel}
          </span>
        ) : null}
      </span>
    </button>
  );
}

export function GroupConversationRow({
  conversation: g,
  active,
  checked,
  selectionActive,
  showBorder = true,
  indentPx = 0,
  onSelectColumnClick,
  onRowClick,
}: {
  conversation: CollapsedGroupConversation;
  active: boolean;
  checked: boolean;
  selectionActive: boolean;
  showBorder?: boolean;
  indentPx?: number;
  onSelectColumnClick: (id: number, e: MouseEvent) => void;
  onRowClick: (
    id: number,
    e: MouseEvent | { shiftKey: boolean; metaKey?: boolean; ctrlKey?: boolean },
  ) => void;
}) {
  return (
    <div
      role={selectionActive ? "button" : undefined}
      tabIndex={selectionActive ? 0 : undefined}
      title={g.titleFull}
      onClick={
        selectionActive ? (e) => onRowClick(g.conversationId, e) : undefined
      }
      onKeyDown={
        selectionActive
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onRowClick(g.conversationId, {
                  shiftKey: e.shiftKey,
                  metaKey: e.metaKey,
                  ctrlKey: e.ctrlKey,
                });
              }
            }
          : undefined
      }
      onMouseDown={(e) => {
        if (e.shiftKey) e.preventDefault();
      }}
      className={`group relative flex w-full items-start gap-1.5 py-2.5 pr-3 text-left select-none outline-none focus:outline-none focus-visible:outline-none ${
        selectionActive ? "cursor-pointer" : ""
      } ${
        checked
          ? "bg-accent/40 hover:bg-accent/50"
          : active
            ? "bg-accent/20 hover:bg-accent/25"
            : "hover:bg-hover-strong"
      } ${showBorder ? "border-b border-border/40" : ""}`}
      style={{ paddingLeft: indentPx }}
    >
      {active && !checked && (
        <span
          aria-hidden
          className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
        />
      )}
      {checked && (
        <span
          aria-hidden
          className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent"
        />
      )}
      <button
        type="button"
        aria-pressed={checked}
        aria-label={`Select ${g.namedTitle || g.title || "group message"}`}
        onClick={(e) => onSelectColumnClick(g.conversationId, e)}
        onMouseDown={(e) => {
          e.stopPropagation();
          if (e.shiftKey) e.preventDefault();
        }}
        className="flex w-10 shrink-0 cursor-pointer items-center justify-center self-stretch -my-2.5 outline-none focus:outline-none focus-visible:outline-none"
      >
        <span
          className={
            checked ? "inline-flex" : "hidden group-hover:inline-flex"
          }
        >
          <input
            type="checkbox"
            checked={checked}
            readOnly
            tabIndex={-1}
            aria-hidden
            className="checkbox-list pointer-events-none"
          />
        </span>
      </button>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onRowClick(g.conversationId, e);
        }}
        onMouseDown={(e) => {
          if (e.shiftKey) e.preventDefault();
        }}
        className="flex min-w-0 flex-1 items-start gap-2 text-left outline-none focus:outline-none focus-visible:outline-none"
      >
        <GroupConversationRowBody conversation={g} variant="browse" />
      </button>
    </div>
  );
}
