import { useState, type ReactNode } from "react";
import { ChevronRightIcon, PlusIcon } from "./icons";

const STORAGE_PREFIX = "mv-left-nav-open:";

function readOpen(id: string): boolean {
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + id);
    if (raw === "0") return false;
    if (raw === "1") return true;
  } catch {
    /* private mode */
  }
  return true;
}

function writeOpen(id: string, open: boolean) {
  try {
    localStorage.setItem(STORAGE_PREFIX + id, open ? "1" : "0");
  } catch {
    /* private mode */
  }
}

/** Sidebar block whose heading toggles the list; the plus still creates an item. */
export default function NavCollapsibleSection({
  id,
  title,
  addLabel,
  onAdd,
  addDisabled = false,
  className = "p-3",
  children,
}: {
  id: string;
  title: string;
  addLabel: string;
  onAdd: () => void;
  addDisabled?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => readOpen(id));

  return (
    <div className={className}>
      <div className="mb-1 flex items-center justify-between gap-1">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => {
            setOpen((prev) => {
              const next = !prev;
              writeOpen(id, next);
              return next;
            });
          }}
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 border-none bg-transparent p-0 text-left text-[0.875rem] font-bold text-text"
        >
          <ChevronRightIcon
            size={12}
            className={`shrink-0 text-muted transition-transform duration-150 ${
              open ? "rotate-90" : ""
            }`}
          />
          <span className="truncate">{title}</span>
        </button>
        <button
          type="button"
          aria-label={addLabel}
          disabled={addDisabled}
          onClick={onAdd}
          className="cursor-pointer border-none bg-transparent p-0.5 text-muted hover:text-accent disabled:opacity-40"
        >
          <PlusIcon size={14} />
        </button>
      </div>
      {open ? children : null}
    </div>
  );
}
