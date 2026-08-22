import { useEffect, useRef, useState } from "react";
import type { ContactNameSort, ContactNameSortState, ContactSortOrder } from "../lib/contactSort";
import { shouldIgnoreOutsideDismiss } from "../lib/portaledOverlay";
import { popupShadow } from "../lib/uiStyles";

const FIELDS = [
  { id: "first", label: "First Name" },
  { id: "last", label: "Last Name" },
] as const;

export default function ContactSortMenu({
  sort,
  order,
  onChange,
}: {
  sort: ContactNameSort;
  order: ContactSortOrder;
  onChange: (next: ContactNameSortState) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: MouseEvent) => {
      if (shouldIgnoreOutsideDismiss(e, rootRef.current)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [open]);

  const sortLabel = FIELDS.find((f) => f.id === sort)?.label ?? "Last Name";
  const orderLabel = order === "asc" ? "Ascending" : "Descending";

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-label={`Sort contacts by ${sortLabel}, ${orderLabel}`}
        aria-expanded={open}
        aria-haspopup="menu"
        title={`Sorted by ${sortLabel}, ${orderLabel}`}
        onClick={() => setOpen((v) => !v)}
        className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-md border border-border bg-elevated text-muted hover:text-text"
      >
        <SortIcon />
      </button>
      {open ? (
        <div
          role="menu"
          data-mv-overlay=""
          className={`absolute top-full right-0 z-[100] mt-1 min-w-[10.5rem] rounded-xl border border-border bg-popover py-2 ${popupShadow}`}
        >
          <div className="px-3 pb-1.5 text-[0.75rem] font-semibold text-text">Sort By</div>
          {FIELDS.map((field) => (
            <SortOption
              key={field.id}
              label={field.label}
              selected={sort === field.id}
              onSelect={() => {
                onChange({ sort: field.id, order });
                setOpen(false);
              }}
            />
          ))}
          <div className="my-1.5 border-t border-border" />
          <div className="px-3 pb-1.5 text-[0.75rem] font-semibold text-text">Order</div>
          <SortOption
            label="Ascending"
            selected={order === "asc"}
            onSelect={() => {
              onChange({ sort, order: "asc" });
              setOpen(false);
            }}
          />
          <SortOption
            label="Descending"
            selected={order === "desc"}
            onSelect={() => {
              onChange({ sort, order: "desc" });
              setOpen(false);
            }}
          />
        </div>
      ) : null}
    </div>
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
      role="menuitemradio"
      aria-checked={selected}
      onClick={onSelect}
      className="flex w-full cursor-pointer items-center gap-2 border-none bg-transparent px-3 py-1.5 text-left text-[0.813rem] text-text hover:bg-hover-strong"
    >
      <span className="flex w-4 justify-center text-accent">{selected ? <CheckIcon /> : null}</span>
      {label}
    </button>
  );
}

function SortIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
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
    <svg width="14" height="14" viewBox="0 0 12 12" fill="none" aria-hidden>
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
