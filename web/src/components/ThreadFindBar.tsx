"use client";

import { useEffect, useRef } from "react";
import { ChevronDownIcon, SearchIcon, XIcon } from "./icons";
import type { ThreadFind } from "./useThreadFind";

/**
 * Find-in-conversation bar shown under the thread header. Enter / Shift+Enter
 * and the arrows step through matches; Esc closes.
 */
export function ThreadFindBar({ find }: { find: ThreadFind }) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (find.open) inputRef.current?.focus();
  }, [find.open]);

  if (!find.open) return null;

  const count = find.matches.length;
  const position = count === 0 ? 0 : find.index + 1;
  const hasQuery = find.query.trim().length > 0;
  const statusLabel = find.loading
    ? "Searching…"
    : hasQuery
      ? count === 0
        ? "No matches"
        : `${position.toLocaleString()} of ${count.toLocaleString()}`
      : "";

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-elevated/60 px-4 py-1.5">
      <SearchIcon className="size-3.5 shrink-0 text-muted" />
      <input
        ref={inputRef}
        type="text"
        value={find.query}
        onChange={(e) => find.setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            if (e.shiftKey) find.prev();
            else find.next();
          } else if (e.key === "Escape") {
            e.preventDefault();
            find.close();
          }
        }}
        placeholder="Find in conversation"
        aria-label="Find in conversation"
        className="w-56 min-w-0 bg-transparent text-[13px] text-text outline-none placeholder:text-muted"
      />
      <span
        aria-live="polite"
        className="min-w-16 text-[12px] whitespace-nowrap text-muted tabular-nums"
      >
        {statusLabel}
      </span>
      <div className="flex items-center">
        <button
          type="button"
          title="Previous match (Shift+Enter)"
          aria-label="Previous match"
          disabled={count === 0}
          onClick={find.prev}
          className="inline-flex size-6 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted"
        >
          <ChevronDownIcon className="size-3.5 rotate-180" />
        </button>
        <button
          type="button"
          title="Next match (Enter)"
          aria-label="Next match"
          disabled={count === 0}
          onClick={find.next}
          className="inline-flex size-6 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted"
        >
          <ChevronDownIcon className="size-3.5" />
        </button>
      </div>
      <button
        type="button"
        title="Close find (Esc)"
        aria-label="Close find"
        onClick={find.close}
        className="ml-auto inline-flex size-6 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text"
      >
        <XIcon className="size-3.5" />
      </button>
    </div>
  );
}
