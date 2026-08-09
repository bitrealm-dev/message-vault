"use client";

import { useEffect, useRef, useState } from "react";
import { ChevronDownIcon } from "./icons";
import { useDismissible } from "./useDismissible";
import type { TrashTab } from "@/lib/trashList";

export function TrashTabPicker({
  tab,
  contactCount,
  groupCount,
  onSwitch,
}: {
  tab: TrashTab;
  contactCount: number;
  groupCount: number;
  onSwitch: (next: TrashTab) => void;
}) {
  const [open, setOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; left: number } | null>(
    null,
  );
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useDismissible({
    open,
    onDismiss: () => {
      setOpen(false);
      setMenuPos(null);
    },
    refs: [rootRef],
  });

  useEffect(() => {
    if (!open) return;
    const rect = buttonRef.current?.getBoundingClientRect();
    if (rect) setMenuPos({ top: rect.bottom + 4, left: rect.left });
  }, [open]);

  const viewLabel = tab === "group-messages" ? "Group Messages" : "Contacts";

  return (
    <div ref={rootRef} className="relative inline-flex shrink-0 items-center">
      <button
        ref={buttonRef}
        type="button"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((v) => !v)}
        className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[12px] leading-none transition-colors ${
          open
            ? "bg-accent/20 text-accent"
            : "bg-elevated text-muted hover:text-text"
        }`}
      >
        {viewLabel}
        <ChevronDownIcon className="size-3.5 shrink-0 opacity-70" />
      </button>
      {open && menuPos && (
        <div
          role="menu"
          className="fixed z-[100] min-w-[11rem] rounded-lg border border-border bg-popover py-1 shadow-xl"
          style={{ top: menuPos.top, left: menuPos.left }}
        >
          <button
            type="button"
            role="menuitem"
            className={`flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-[13px] hover:bg-hover-strong ${
              tab === "contacts" ? "text-accent" : "text-text"
            }`}
            onClick={() => {
              setOpen(false);
              setMenuPos(null);
              onSwitch("contacts");
            }}
          >
            <span>Contacts</span>
            <span className="text-muted tabular-nums">{contactCount}</span>
          </button>
          <button
            type="button"
            role="menuitem"
            className={`flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-[13px] hover:bg-hover-strong ${
              tab === "group-messages" ? "text-accent" : "text-text"
            }`}
            onClick={() => {
              setOpen(false);
              setMenuPos(null);
              onSwitch("group-messages");
            }}
          >
            <span>Group Messages</span>
            <span className="text-muted tabular-nums">{groupCount}</span>
          </button>
        </div>
      )}
    </div>
  );
}
