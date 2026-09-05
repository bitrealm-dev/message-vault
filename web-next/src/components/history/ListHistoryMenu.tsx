"use client";

import { useRef, useState, type ReactNode } from "react";
import { IconHoverTarget } from "../IconHoverLabel";
import { EllipsisIcon } from "../icons";
import { useDismissible } from "../useDismissible";

export type ListHistoryMenuItem = {
  key: string;
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
  /** Red hover styling for destructive actions. */
  danger?: boolean;
  /** `triggerEl` is the ⋯ button that opened the menu. */
  onClick: (triggerEl: HTMLElement | null) => void;
};

/**
 * Fastmail-style ⋯ menu for list headers. Undo and redo are not features of
 * web-next, so the menu holds only the list's own actions and renders
 * nothing when there are none.
 */
export function ListHistoryMenu({
  items = [],
}: {
  items?: ListHistoryMenuItem[];
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

  if (items.length === 0) return null;

  return (
    <div className="relative" ref={rootRef}>
      <IconHoverTarget label="Actions" placement="bottom" hidden={open}>
        <button
          ref={buttonRef}
          type="button"
          aria-label="Actions"
          aria-expanded={open}
          onClick={toggle}
          className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted hover:text-text disabled:opacity-40"
        >
          <EllipsisIcon className="size-5" />
        </button>
      </IconHoverTarget>
      {open && menuPos && (
        <div
          className="fixed z-[100] min-w-[10.5rem] rounded-xl border border-border bg-popover py-1 shadow-xl"
          style={{ top: menuPos.top, right: menuPos.right }}
        >
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              disabled={item.disabled}
              className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-text disabled:opacity-40 ${
                item.danger
                  ? "hover:bg-red-500/15 hover:text-red-300"
                  : "hover:bg-hover-strong"
              }`}
              onClick={() => {
                close();
                item.onClick(buttonRef.current);
              }}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
