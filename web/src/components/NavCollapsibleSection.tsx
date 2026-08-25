import { type ReactNode, useState } from "react";
import { ChevronRightIcon, PlusIcon } from "./icons";
import NavGlyphButton from "./NavGlyphButton";
import { NAV_SECTION_GRID_CLASS } from "./navSectionLayout";

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
  className = "px-3 py-2",
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
      <div className={`mb-1 ${NAV_SECTION_GRID_CLASS}`}>
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
          className="flex min-w-0 cursor-pointer items-center gap-1.5 border-none bg-transparent p-0 text-left text-[0.875rem] font-bold text-text"
        >
          <ChevronRightIcon
            size={12}
            className={`shrink-0 text-muted transition-transform duration-150 ${
              open ? "rotate-90" : ""
            }`}
          />
          <span className="truncate">{title}</span>
        </button>
        <NavGlyphButton aria-label={addLabel} disabled={addDisabled} onClick={onAdd}>
          <PlusIcon size={14} />
        </NavGlyphButton>
      </div>
      {open ? children : null}
    </div>
  );
}
