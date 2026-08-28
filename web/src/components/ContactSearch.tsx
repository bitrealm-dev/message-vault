import { useCallback, useEffect, useEffectEvent, useRef, useState } from "react";
import {
  clearContactRecentSearches,
  loadContactRecentSearches,
  pushContactRecentSearch,
} from "../lib/contactRecentSearches";
import { popupShadow } from "../lib/uiStyles";
import { useDismissable } from "../lib/useDismissable";
import { Z_INLINE_PANEL, Z_POPOVER } from "../lib/zLayers";
import AdvancedSearchForm from "./AdvancedSearchForm";

/** Element id for one recent-search row, referenced by `aria-activedescendant`. */
function optionId(query: string): string {
  return `contact-search-recent-${encodeURIComponent(query)}`;
}

const ADVANCED_OPTION_ID = "contact-search-advanced";

function MagnifyingGlassIcon() {
  return (
    <svg
      aria-hidden
      viewBox="0 0 24 24"
      className="ml-3 size-4 shrink-0 text-muted"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="11" cy="11" r="7" />
      <path d="M20 20l-3.5-3.5" />
    </svg>
  );
}

function ClockIcon() {
  return (
    <svg
      aria-hidden
      viewBox="0 0 24 24"
      className="size-3.5 shrink-0 text-muted"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </svg>
  );
}

function SlidersIcon() {
  return (
    <svg
      aria-hidden
      viewBox="0 0 24 24"
      className="size-3.5 shrink-0 text-muted"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3" />
      <path d="M1 14h6M9 8h6M17 16h6" />
    </svg>
  );
}

export default function ContactSearch({
  value,
  onChange,
  onSubmit,
  onOpenChange,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (q: string) => void;
  /** True while the popdown or advanced panel is open (for list-column stacking). */
  onOpenChange?: (open: boolean) => void;
}) {
  const [popdownOpen, setPopdownOpen] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [recents, setRecents] = useState(() => loadContactRecentSearches());
  /** Row the arrow keys are on; -1 means the typed text itself. */
  const [activeIndex, setActiveIndex] = useState(-1);
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const notifyOpen = useEffectEvent((open: boolean) => {
    onOpenChange?.(open);
  });

  useEffect(() => {
    notifyOpen(popdownOpen || showAdvanced);
  }, [popdownOpen, showAdvanced]);

  const dismissAll = useCallback(() => {
    setPopdownOpen(false);
    setShowAdvanced(false);
  }, []);
  useDismissable(popdownOpen || showAdvanced, rootRef, dismissAll);

  const refreshRecents = () => {
    setRecents(loadContactRecentSearches());
  };

  const applyQuery = (q: string, { save }: { save: boolean }) => {
    onChange(q);
    onSubmit(q);
    if (save && q.trim()) {
      pushContactRecentSearch(q);
      refreshRecents();
    }
    setPopdownOpen(false);
    setShowAdvanced(false);
  };

  const openPopdown = () => {
    if (showAdvanced) return;
    setRecents(loadContactRecentSearches());
    setPopdownOpen(true);
  };

  const openAdvanced = () => {
    setPopdownOpen(false);
    setShowAdvanced(true);
  };

  /**
   * Rows the arrow keys walk: every recent search, then the Advanced search row.
   * The input keeps DOM focus throughout and points at the current row with
   * `aria-activedescendant`, which is what `role="combobox"` promises — before
   * this the list was reachable only with a pointer.
   */
  const options = [
    ...recents.map((q) => ({ id: optionId(q), run: () => applyQuery(q, { save: true }) })),
    { id: ADVANCED_OPTION_ID, run: openAdvanced },
  ];
  const active = activeIndex >= 0 && activeIndex < options.length ? options[activeIndex] : null;

  const moveActive = (delta: number) => {
    if (!popdownOpen) {
      openPopdown();
      setActiveIndex(0);
      return;
    }
    const count = options.length;
    if (count === 0) return;
    setActiveIndex((at) => (at < 0 ? (delta > 0 ? 0 : count - 1) : (at + delta + count) % count));
  };

  return (
    <div ref={rootRef} className="relative">
      <div className="flex items-center rounded-xl border border-border bg-bg focus-within:border-accent">
        <MagnifyingGlassIcon />
        <input
          ref={inputRef}
          type="search"
          role="combobox"
          value={value}
          placeholder="Search contacts"
          aria-label="Search contacts"
          aria-expanded={popdownOpen}
          aria-controls="contact-search-popdown"
          aria-autocomplete="list"
          aria-activedescendant={popdownOpen && active ? active.id : undefined}
          onChange={(e) => {
            onChange(e.target.value);
            setActiveIndex(-1);
          }}
          onFocus={openPopdown}
          onClick={openPopdown}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              if (showAdvanced) {
                e.preventDefault();
                setShowAdvanced(false);
                return;
              }
              if (popdownOpen) {
                e.preventDefault();
                setPopdownOpen(false);
                setActiveIndex(-1);
              }
              return;
            }
            if (e.key === "ArrowDown") {
              e.preventDefault();
              moveActive(1);
              return;
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              moveActive(-1);
              return;
            }
            if (e.key === "Enter") {
              e.preventDefault();
              // A highlighted row wins; otherwise submit what was typed.
              if (active) active.run();
              else applyQuery(value, { save: true });
            }
          }}
          className="min-w-0 flex-1 border-none bg-transparent px-2 py-2.5 text-[0.875rem] text-text outline-none"
        />
        {value ? (
          <button
            type="button"
            aria-label="Clear search"
            onClick={() => {
              onChange("");
              onSubmit("");
              inputRef.current?.focus();
            }}
            className="mr-2 cursor-pointer border-none bg-transparent px-1 text-[1rem] leading-none text-muted hover:text-text"
          >
            ×
          </button>
        ) : null}
      </div>

      {popdownOpen && !showAdvanced ? (
        <div
          id="contact-search-popdown"
          role="listbox"
          className={`absolute top-full right-0 left-0 mt-1 overflow-hidden rounded-md border border-border bg-popover ${Z_POPOVER} ${popupShadow}`}
        >
          {recents.length > 0 ? (
            <>
              <div className="flex items-center justify-between px-3 pb-1 pt-2">
                <span className="text-[0.688rem] font-semibold uppercase tracking-[0.04em] text-muted">
                  Recent searches
                </span>
                <button
                  type="button"
                  onClick={() => {
                    clearContactRecentSearches();
                    setRecents([]);
                  }}
                  className="cursor-pointer border-none bg-transparent text-[0.688rem] text-muted hover:text-text"
                >
                  Clear all
                </button>
              </div>
              <ul className="m-0 list-none p-0">
                {recents.map((q, i) => (
                  <li key={q}>
                    <button
                      type="button"
                      role="option"
                      id={optionId(q)}
                      aria-selected={activeIndex === i}
                      onMouseEnter={() => setActiveIndex(i)}
                      onClick={() => applyQuery(q, { save: true })}
                      className={`flex w-full cursor-pointer items-center gap-2 border-none px-3 py-2 text-left text-[0.875rem] text-text hover:bg-hover ${
                        activeIndex === i ? "bg-hover" : "bg-transparent"
                      }`}
                    >
                      <ClockIcon />
                      <span className="min-w-0 truncate">{q}</span>
                    </button>
                  </li>
                ))}
              </ul>
              <div className="mx-2 border-t border-border" />
            </>
          ) : null}
          <button
            type="button"
            role="option"
            id={ADVANCED_OPTION_ID}
            aria-selected={activeIndex === recents.length}
            onMouseEnter={() => setActiveIndex(recents.length)}
            onClick={openAdvanced}
            className={`flex w-full cursor-pointer items-center gap-2 border-none px-3 py-2.5 text-left text-[0.875rem] text-text hover:bg-hover ${
              activeIndex === recents.length ? "bg-hover" : "bg-transparent"
            }`}
          >
            <SlidersIcon />
            Advanced search
          </button>
        </div>
      ) : null}

      {showAdvanced ? (
        <div className={`absolute top-full left-0 mt-2 w-full min-w-[300px] ${Z_INLINE_PANEL}`}>
          <AdvancedSearchForm
            mode="contacts"
            withTail
            onApply={(q) => applyQuery(q, { save: true })}
            onClose={() => setShowAdvanced(false)}
          />
        </div>
      ) : null}
    </div>
  );
}
