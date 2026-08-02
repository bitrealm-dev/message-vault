"use client";

import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import type { MouseEvent } from "react";
import { GroupConversationRowBody } from "./GroupConversationRow";
import { useDateTimeFormat } from "./useDateTimeFormat";

/** Centered hairline (~95% width) between conversation rows. */
function RowDivider() {
  return (
    <span
      aria-hidden
      className="pointer-events-none absolute bottom-0 left-1/2 h-px w-[95%] -translate-x-1/2 bg-border/55"
    />
  );
}

export function DirectConversationRow({
  active,
  dateStart = null,
  dateEnd = null,
  showBorder = false,
  nested = false,
  onClick,
}: {
  active: boolean;
  dateStart?: string | null;
  dateEnd?: string | null;
  showBorder?: boolean;
  /** Under a contact: align content with the contact name column. */
  nested?: boolean;
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
      className={`group relative flex w-full items-start gap-1.5 py-2.5 pr-3 text-left select-none outline-none focus:outline-none focus-visible:outline-none ${
        nested ? "pl-3" : ""
      } ${
        active
          ? "bg-accent/25 hover:bg-accent/30"
          : "bg-transparent hover:bg-hover"
      }`}
    >
      {active && (
        <span
          aria-hidden
          className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
        />
      )}
      {showBorder ? <RowDivider /> : null}
      {!nested ? <span className="w-10 shrink-0" aria-hidden /> : null}
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
  nested = false,
  onSelectColumnClick,
  onRowClick,
}: {
  conversation: CollapsedGroupConversation;
  active: boolean;
  checked: boolean;
  selectionActive: boolean;
  showBorder?: boolean;
  /** Under a contact: align content with the contact name column. */
  nested?: boolean;
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
        nested ? "pl-3" : ""
      } ${selectionActive ? "cursor-pointer" : ""} ${
        checked
          ? "bg-accent/40 hover:bg-accent/50"
          : active
            ? "bg-accent/25 hover:bg-accent/30"
            : "bg-transparent hover:bg-hover"
      }`}
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
      {showBorder ? <RowDivider /> : null}
      <button
        type="button"
        aria-pressed={checked}
        aria-label={`Select ${g.namedTitle || g.title || "group message"}`}
        onClick={(e) => onSelectColumnClick(g.conversationId, e)}
        onMouseDown={(e) => {
          e.stopPropagation();
          if (e.shiftKey) e.preventDefault();
        }}
        className={
          nested
            ? "absolute top-0 bottom-0 left-0 z-10 flex w-8 cursor-pointer items-center justify-center outline-none focus:outline-none focus-visible:outline-none"
            : "flex w-10 shrink-0 cursor-pointer items-center justify-center self-stretch -my-2.5 outline-none focus:outline-none focus-visible:outline-none"
        }
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
