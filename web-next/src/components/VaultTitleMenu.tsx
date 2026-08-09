"use client";

import { useRef, useState } from "react";
import { ChevronDownIcon } from "./icons";
import { useDismissible } from "./useDismissible";

export function VaultTitleMenu({
  demoResetAvailable,
  resettingDemo,
  onResetDemo,
  onLogout,
}: {
  demoResetAvailable: boolean;
  resettingDemo: boolean;
  onResetDemo: () => void;
  onLogout: () => void;
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

  const toggle = () => {
    if (open) {
      setOpen(false);
      setMenuPos(null);
      return;
    }
    const rect = buttonRef.current?.getBoundingClientRect();
    if (rect) {
      setMenuPos({ top: rect.bottom + 4, left: rect.left });
    }
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setMenuPos(null);
  };

  const itemClass =
    "flex w-full px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong disabled:pointer-events-none disabled:opacity-40";

  return (
    <div ref={rootRef} className="relative min-w-0">
      <button
        ref={buttonRef}
        type="button"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={toggle}
        className={`inline-flex max-w-full items-center gap-1 truncate rounded-md px-1.5 py-0.5 text-[13px] font-medium transition-colors ${
          open
            ? "bg-hover text-text"
            : "text-text hover:bg-hover hover:text-accent"
        }`}
      >
        <span className="truncate">Message Vault</span>
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
            className={itemClass}
            onClick={() => {
              close();
              onLogout();
            }}
          >
            Logout
          </button>
          {demoResetAvailable && (
            <>
              <div className="my-1 border-t border-border" role="separator" />
              <button
                type="button"
                role="menuitem"
                disabled={resettingDemo}
                className={itemClass}
                onClick={() => {
                  close();
                  onResetDemo();
                }}
              >
                {resettingDemo ? "Resetting demo…" : "Reset demo"}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
