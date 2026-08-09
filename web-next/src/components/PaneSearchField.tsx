"use client";

import { SearchIcon } from "./icons";

/** Pane toolbar search with a persistent magnifying glass. */
export function PaneSearchField({
  value,
  onChange,
  placeholder,
  disabled = false,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange?: (value: string) => void;
  placeholder: string;
  disabled?: boolean;
  "aria-label"?: string;
}) {
  return (
    <div
      className={`flex w-full items-center gap-2 rounded-md border border-border bg-elevated px-2.5 py-1.5 focus-within:border-accent ${
        disabled ? "cursor-default opacity-40" : ""
      }`}
    >
      <SearchIcon className="size-4 shrink-0 text-muted" />
      <input
        type="search"
        value={value}
        disabled={disabled}
        aria-label={ariaLabel ?? placeholder}
        onChange={onChange ? (e) => onChange(e.target.value) : undefined}
        placeholder={placeholder}
        className="min-w-0 flex-1 bg-transparent text-[13px] text-text outline-none placeholder:text-muted disabled:cursor-default"
      />
    </div>
  );
}
