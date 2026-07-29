"use client";

import {
  contactAvatarColor,
  contactInitials,
} from "@/lib/contactInitials";
import { formatPhoneDisplay } from "@/lib/phoneE164";
import type { ContactListItem } from "@/lib/types";
import type { MouseEvent } from "react";
import { CountBadge } from "./CountBadge";
import { ChevronDownIcon, GroupMessagesOutlineIcon } from "./icons";
import { useDateTimeFormat } from "./useDateTimeFormat";
import { useMessageBadgePrefs } from "./useMessageBadgePrefs";

export function BrowseContactRow({
  contact: c,
  active,
  checked,
  selectionActive,
  expanded = false,
  showExpandChevron = false,
  showInsetDivider = false,
  indentPx = 0,
  onSelectColumnClick,
  onNamePhoneClick,
  onContextMenu,
  onToggleExpand,
}: {
  contact: ContactListItem;
  active: boolean;
  checked: boolean;
  selectionActive: boolean;
  expanded?: boolean;
  showExpandChevron?: boolean;
  showInsetDivider?: boolean;
  indentPx?: number;
  onSelectColumnClick: (id: number, e: MouseEvent) => void;
  onNamePhoneClick: (
    id: number,
    e: MouseEvent | { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
  ) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
  onToggleExpand?: (id: number) => void;
}) {
  const {
    showMessageBadge,
    showGroupMessageBadge,
    showContactInitials,
    showContactDateRange,
  } = useMessageBadgePrefs();
  const { formatDateRange } = useDateTimeFormat();

  return (
    <div
      role={selectionActive ? "button" : undefined}
      tabIndex={selectionActive ? 0 : undefined}
      aria-expanded={showExpandChevron ? expanded : undefined}
      onClick={
        selectionActive
          ? (e) => onNamePhoneClick(c.id, e)
          : undefined
      }
      onKeyDown={
        selectionActive
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onNamePhoneClick(c.id, {
                  shiftKey: e.shiftKey,
                  metaKey: e.metaKey,
                  ctrlKey: e.ctrlKey,
                });
              }
            }
          : undefined
      }
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(c.id, e.clientX, e.clientY);
      }}
      onMouseDown={(e) => {
        if (e.shiftKey) e.preventDefault();
      }}
      className={`group relative flex w-full items-start gap-1.5 py-3 pr-3 pl-0 select-none outline-none focus:outline-none focus-visible:outline-none ${
        selectionActive ? "cursor-pointer" : ""
      } ${
        checked
          ? "bg-accent/40 hover:bg-accent/50"
          : active
            ? "bg-accent/20 hover:bg-accent/25"
            : "hover:bg-hover-strong"
      }`}
      style={indentPx > 0 ? { paddingLeft: indentPx } : undefined}
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
      {showExpandChevron ? (
        <button
          type="button"
          aria-label={expanded ? "Collapse conversations" : "Expand conversations"}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand?.(c.id);
          }}
          className="flex w-6 shrink-0 items-center justify-center self-stretch text-muted outline-none hover:text-text"
        >
          <ChevronDownIcon
            className={`size-3.5 transition-transform ${
              expanded ? "" : "-rotate-90"
            }`}
          />
        </button>
      ) : null}
      <button
        type="button"
        aria-pressed={checked}
        aria-label={`Select ${c.displayName}`}
        onClick={(e) => onSelectColumnClick(c.id, e)}
        onMouseDown={(e) => {
          e.stopPropagation();
          if (e.shiftKey) e.preventDefault();
        }}
        className="group/select flex w-10 shrink-0 cursor-pointer items-center justify-center self-stretch -my-3 outline-none focus:outline-none focus-visible:outline-none"
      >
        {showContactInitials ? (
          <span
            aria-hidden
            className={`flex size-7 items-center justify-center rounded-full text-[11px] font-semibold text-white ${
              checked
                ? "hidden"
                : selectionActive
                  ? "group-hover:hidden"
                  : "group-hover/select:hidden"
            }`}
            style={{
              backgroundColor: contactAvatarColor({
                displayName: c.displayName,
                preferredHandle: c.preferredHandle,
                firstName: c.firstName,
                lastName: c.lastName,
              }),
            }}
          >
            {contactInitials(c)}
          </span>
        ) : null}
        <span
          className={
            !showContactInitials || checked
              ? "inline-flex"
              : selectionActive
                ? "hidden group-hover:inline-flex"
                : "hidden group-hover/select:inline-flex"
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
          onNamePhoneClick(c.id, e);
        }}
        onMouseDown={(e) => {
          if (e.shiftKey) e.preventDefault();
        }}
        className="flex min-w-0 flex-1 items-stretch justify-between gap-2 self-stretch text-left outline-none focus:outline-none focus-visible:outline-none"
      >
        <span className="min-w-0 flex-1 self-start">
          <span className="block truncate text-[13px] font-semibold text-text">
            {c.displayName}
          </span>
          {(() => {
            const formattedHandle = c.preferredHandle
              ? formatPhoneDisplay(c.preferredHandle)
              : "";
            const showHandle =
              !!formattedHandle && formattedHandle !== c.displayName;
            const dateLabel =
              showContactDateRange && c.dateStart && c.dateEnd
                ? formatDateRange(c.dateStart, c.dateEnd, " – ")
                : null;
            const showGroupIcon =
              showGroupMessageBadge && c.groupMessageCount > 0;
            const showCountBadge = showMessageBadge && c.messageCount > 0;
            const rowLabels = (c.labels ?? []).filter(Boolean);
            const hasBottomLine =
              !!dateLabel ||
              showGroupIcon ||
              showCountBadge ||
              rowLabels.length > 0;
            if (!showHandle && !hasBottomLine) return null;
            return (
              <>
                {showHandle ? (
                  <span className="block truncate text-[12px] text-muted">
                    {formattedHandle}
                  </span>
                ) : null}
                {rowLabels.length > 0 ? (
                  <span className="mt-0.5 flex min-w-0 flex-wrap gap-1">
                    {rowLabels.slice(0, 3).map((label) => (
                      <span
                        key={label}
                        className="max-w-[7rem] truncate rounded bg-elevated px-1.5 py-0.5 text-[10px] font-medium text-muted"
                        title={label}
                      >
                        {label}
                      </span>
                    ))}
                    {rowLabels.length > 3 ? (
                      <span className="rounded bg-elevated px-1.5 py-0.5 text-[10px] font-medium text-muted">
                        +{rowLabels.length - 3}
                      </span>
                    ) : null}
                  </span>
                ) : null}
                {dateLabel || showGroupIcon || showCountBadge ? (
                  <span className="mt-0.5 flex min-w-0 items-center justify-between gap-2">
                    <span className="inline-flex shrink-0 items-center gap-1.5">
                      {showGroupMessageBadge && (
                        <span
                          title={
                            showGroupIcon ? "In group messages" : undefined
                          }
                          className="inline-flex items-center"
                        >
                          <GroupMessagesOutlineIcon
                            className={`size-3.5 shrink-0 text-muted opacity-80 ${
                              showGroupIcon ? "" : "invisible"
                            }`}
                          />
                        </span>
                      )}
                      {showCountBadge && (
                        <CountBadge
                          count={c.messageCount}
                          title="1:1 messages"
                        />
                      )}
                    </span>
                    {dateLabel ? (
                      <span className="min-w-0 truncate text-right text-[11px] text-muted tabular-nums">
                        {dateLabel}
                      </span>
                    ) : null}
                  </span>
                ) : null}
              </>
            );
          })()}
        </span>
      </button>
      {showInsetDivider && (
        <span
          aria-hidden
          className="pointer-events-none absolute right-3 bottom-0 left-3 h-px bg-border/60"
        />
      )}
    </div>
  );
}
