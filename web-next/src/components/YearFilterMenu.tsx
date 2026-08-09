"use client";

import { useRef, useState } from "react";
import { ChevronDownIcon } from "./icons";
import { IconHoverTarget } from "./IconHoverLabel";
import { useDismissible } from "./useDismissible";

/** Year filter combo: All years + calendar years, with a down chevron. */
export function YearFilterMenu({
  years,
  value,
  onChange,
}: {
  years: number[];
  /** null = All years */
  value: number | null;
  onChange: (year: number | null) => void;
}) {
  const disabled = years.length === 0;
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

  const label = value == null ? "All years" : String(value);

  const toggle = () => {
    if (disabled) return;
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

  const pick = (next: number | null) => {
    setOpen(false);
    setMenuPos(null);
    onChange(next);
  };

  return (
    <div ref={rootRef} className="relative inline-flex shrink-0 items-center">
      <IconHoverTarget label="Filter by year" placement="bottom" hidden={open}>
        <button
          ref={buttonRef}
          type="button"
          aria-expanded={open}
          aria-haspopup="listbox"
          aria-label="Filter by year"
          disabled={disabled}
          onClick={toggle}
          className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[12px] leading-none transition-colors disabled:pointer-events-none disabled:opacity-40 ${
            open
              ? "bg-accent/20 text-accent"
              : "bg-elevated text-muted hover:text-text"
          }`}
        >
          <span className="tabular-nums">{label}</span>
          <ChevronDownIcon className="size-3.5 shrink-0 opacity-70" />
        </button>
      </IconHoverTarget>
      {open && menuPos && (
        <div
          role="listbox"
          className="fixed z-[100] max-h-64 min-w-[7rem] overflow-y-auto rounded-lg border border-border bg-popover py-1 shadow-xl"
          style={{ top: menuPos.top, left: menuPos.left }}
        >
          <button
            type="button"
            role="option"
            aria-selected={value == null}
            className={`flex w-full px-3 py-1.5 text-left text-[13px] hover:bg-hover-strong ${
              value == null ? "text-accent" : "text-text"
            }`}
            onClick={() => pick(null)}
          >
            All years
          </button>
          {years.map((y) => (
            <button
              key={y}
              type="button"
              role="option"
              aria-selected={value === y}
              className={`flex w-full px-3 py-1.5 text-left text-[13px] tabular-nums hover:bg-hover-strong ${
                value === y ? "text-accent" : "text-text"
              }`}
              onClick={() => pick(y)}
            >
              {y}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
