"use client";

import { useRef, useState } from "react";
import { IconHoverTarget } from "./IconHoverLabel";
import { useDismissible } from "./useDismissible";

export type SortMode =
  | "first"
  | "last"
  | "messages"
  | "group-messages"
  | "phone";
export type SortOrder = "asc" | "desc";

type SortField<T extends string> = { id: T; label: string };

/** Config-driven sort field + ascending/descending menu. */
export function SortMenu<T extends string>({
  fields,
  sort,
  order,
  onChange,
  ariaLabel = "Sort by",
  /** Compact visible scope (e.g. Contacts / Messages) next to the icon. */
  scopeLabel,
  /** Extra classes for the scope label (e.g. container-query hide/show). */
  scopeLabelClassName,
  disabled = false,
}: {
  fields: SortField<T>[];
  sort: T;
  order: SortOrder;
  onChange: (next: { sort: T; order: SortOrder }) => void;
  ariaLabel?: string;
  scopeLabel?: string;
  scopeLabelClassName?: string;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number } | null>(
    null,
  );
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const close = () => {
    setOpen(false);
    setMenuPos(null);
  };

  const toggle = () => {
    if (disabled) return;
    if (open) {
      close();
      return;
    }
    const rect = buttonRef.current?.getBoundingClientRect();
    if (rect) {
      setMenuPos({ top: rect.bottom + 4, right: window.innerWidth - rect.right });
    }
    setOpen(true);
  };

  useDismissible({
    open,
    onDismiss: close,
    refs: [rootRef],
  });

  const sortLabel =
    fields.find((field) => field.id === sort)?.label ?? String(sort);
  const orderLabel = order === "asc" ? "Ascending" : "Descending";
  const hoverLabel = scopeLabel
    ? `${scopeLabel}: sorted by ${sortLabel}, ${orderLabel}`
    : `Sorted by ${sortLabel}, ${orderLabel}`;

  return (
    <div className="relative" ref={rootRef}>
      <IconHoverTarget label={hoverLabel} placement="bottom" hidden={open}>
        <button
          ref={buttonRef}
          type="button"
          aria-label={ariaLabel}
          aria-expanded={open}
          disabled={disabled}
          onClick={toggle}
          className={`flex h-7 min-w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted hover:text-text disabled:pointer-events-none disabled:opacity-40 ${
            scopeLabel ? "gap-1 px-2" : "w-7"
          }`}
        >
          <SortIcon />
          {scopeLabel ? (
            <span
              className={`whitespace-nowrap text-[11px] font-medium ${
                scopeLabelClassName ?? ""
              }`}
            >
              {scopeLabel}
            </span>
          ) : null}
        </button>
      </IconHoverTarget>
      {open && menuPos && (
        <div
          className="fixed z-[100] min-w-[10.5rem] rounded-xl border border-border bg-popover py-2 shadow-xl"
          style={{ top: menuPos.top, right: menuPos.right }}
        >
          {scopeLabel ? (
            <div className="px-3 pb-1 text-[11px] font-semibold tracking-wide text-muted uppercase">
              {scopeLabel}
            </div>
          ) : null}
          <div className="px-3 pb-1.5 text-[12px] font-semibold text-text">
            Sort By
          </div>
          {fields.map((field) => (
            <SortOption
              key={field.id}
              label={field.label}
              selected={sort === field.id}
              onSelect={() => {
                onChange({ sort: field.id, order });
                close();
              }}
            />
          ))}
          <div className="my-1.5 border-t border-border" />
          <div className="px-3 pb-1.5 text-[12px] font-semibold text-text">
            Order
          </div>
          <SortOption
            label="Ascending"
            selected={order === "asc"}
            onSelect={() => {
              onChange({ sort, order: "asc" });
              close();
            }}
          />
          <SortOption
            label="Descending"
            selected={order === "desc"}
            onSelect={() => {
              onChange({ sort, order: "desc" });
              close();
            }}
          />
        </div>
      )}
    </div>
  );
}

const CONTACT_SORT_FIELDS: SortField<SortMode>[] = [
  { id: "first", label: "First Name" },
  { id: "last", label: "Last Name" },
  { id: "phone", label: "Phone number" },
  { id: "messages", label: "Message count" },
  { id: "group-messages", label: "Group count" },
];

export function SortByMenu({
  sort,
  order,
  onChange,
  scopeLabel = "Contacts",
  scopeLabelClassName,
}: {
  sort: SortMode;
  order: SortOrder;
  onChange: (next: { sort: SortMode; order: SortOrder }) => void;
  /** Visible scope chip; pass null to keep the icon-only trigger. */
  scopeLabel?: string | null;
  scopeLabelClassName?: string;
}) {
  return (
    <SortMenu
      fields={CONTACT_SORT_FIELDS}
      sort={sort}
      order={order}
      onChange={onChange}
      ariaLabel="Sort contacts"
      scopeLabel={scopeLabel ?? undefined}
      scopeLabelClassName={scopeLabelClassName}
    />
  );
}

export type TrashSortBy = "phone" | "first" | "last" | "count";

const TRASH_SORT_FIELDS: SortField<TrashSortBy>[] = [
  { id: "phone", label: "Phone number" },
  { id: "first", label: "First" },
  { id: "last", label: "Last" },
  { id: "count", label: "Count" },
];

/** Phone / first / last / count + ascending/descending for Trash contacts. */
export function TrashSortMenu({
  sortBy,
  order,
  onChange,
}: {
  sortBy: TrashSortBy;
  order: SortOrder;
  onChange: (next: { sortBy: TrashSortBy; order: SortOrder }) => void;
}) {
  return (
    <SortMenu
      fields={TRASH_SORT_FIELDS}
      sort={sortBy}
      order={order}
      onChange={({ sort, order: nextOrder }) =>
        onChange({ sortBy: sort, order: nextOrder })
      }
      ariaLabel="Sort trash"
    />
  );
}

export type GroupTrashSortBy = "start" | "end" | "people" | "messages";

const GROUP_TRASH_SORT_FIELDS: SortField<GroupTrashSortBy>[] = [
  { id: "start", label: "Start date" },
  { id: "end", label: "End date" },
  { id: "people", label: "People" },
  { id: "messages", label: "Messages" },
];

/** Start/end date, people, messages for Trash group chats. */
export function GroupTrashSortMenu({
  sortBy,
  order,
  onChange,
}: {
  sortBy: GroupTrashSortBy;
  order: SortOrder;
  onChange: (next: { sortBy: GroupTrashSortBy; order: SortOrder }) => void;
}) {
  return (
    <SortMenu
      fields={GROUP_TRASH_SORT_FIELDS}
      sort={sortBy}
      order={order}
      onChange={({ sort, order: nextOrder }) =>
        onChange({ sortBy: sort, order: nextOrder })
      }
      ariaLabel="Sort trashed group messages"
    />
  );
}

export type BrowseGroupChatSortBy = "date" | "messages" | "people";

const BROWSE_GROUP_CHAT_SORT_FIELDS: SortField<BrowseGroupChatSortBy>[] = [
  { id: "date", label: "Date" },
  { id: "messages", label: "Message Count" },
  { id: "people", label: "People" },
];

/** Date / message count / people for contact browse panel 3 group chats. */
export function BrowseGroupChatSortMenu({
  sortBy,
  order,
  onChange,
  disabled = false,
  scopeLabel = "Messages",
  scopeLabelClassName,
}: {
  sortBy: BrowseGroupChatSortBy;
  order: SortOrder;
  onChange: (next: { sortBy: BrowseGroupChatSortBy; order: SortOrder }) => void;
  disabled?: boolean;
  /** Visible scope chip; pass null to keep the icon-only trigger. */
  scopeLabel?: string | null;
  scopeLabelClassName?: string;
}) {
  return (
    <SortMenu
      fields={BROWSE_GROUP_CHAT_SORT_FIELDS}
      sort={sortBy}
      order={order}
      onChange={({ sort, order: nextOrder }) =>
        onChange({ sortBy: sort, order: nextOrder })
      }
      ariaLabel="Sort chats"
      scopeLabel={scopeLabel ?? undefined}
      scopeLabelClassName={scopeLabelClassName}
      disabled={disabled}
    />
  );
}

function SortOption({
  label,
  selected,
  onSelect,
}: {
  label: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong"
    >
      <span className="flex w-4 justify-center text-accent">
        {selected ? <CheckIcon /> : null}
      </span>
      {label}
    </button>
  );
}

function SortIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden>
      <path
        d="M5 3v10M5 3l-2.5 2.5M5 3l2.5 2.5M11 13V3M11 13l-2.5-2.5M11 13l2.5-2.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 12 12" fill="none" aria-hidden>
      <path
        d="M2 6.2L4.6 9 10 3"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
