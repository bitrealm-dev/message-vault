import { useEffect, useEffectEvent, useRef, useState } from "react";
import {
  clearContactRecentSearches,
  loadContactRecentSearches,
  pushContactRecentSearch,
} from "../lib/contactRecentSearches";
import { shouldIgnoreOutsideDismiss } from "../lib/portaledOverlay";
import { popupShadow } from "../lib/uiStyles";
import AdvancedSearchForm from "./AdvancedSearchForm";

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
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const notifyOpen = useEffectEvent((open: boolean) => {
    onOpenChange?.(open);
  });

  useEffect(() => {
    notifyOpen(popdownOpen || showAdvanced);
  }, [popdownOpen, showAdvanced]);

  useEffect(() => {
    if (!popdownOpen && !showAdvanced) return;
    const onPointerDown = (e: MouseEvent) => {
      // Capture phase: see open Select/Date menus before RAC removes them.
      if (shouldIgnoreOutsideDismiss(e, rootRef.current)) return;
      setPopdownOpen(false);
      setShowAdvanced(false);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [popdownOpen, showAdvanced]);

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
          onChange={(e) => onChange(e.target.value)}
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
              }
              return;
            }
            if (e.key === "Enter") {
              e.preventDefault();
              applyQuery(value, { save: true });
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
          className={`absolute left-0 right-0 top-full z-50 mt-1 overflow-hidden rounded-md border border-border bg-popover ${popupShadow}`}
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
                {recents.map((q) => (
                  <li key={q}>
                    <button
                      type="button"
                      role="option"
                      onClick={() => applyQuery(q, { save: true })}
                      className="flex w-full cursor-pointer items-center gap-2 border-none bg-transparent px-3 py-2 text-left text-[0.875rem] text-text hover:bg-hover"
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
            onClick={() => {
              setPopdownOpen(false);
              setShowAdvanced(true);
            }}
            className="flex w-full cursor-pointer items-center gap-2 border-none bg-transparent px-3 py-2.5 text-left text-[0.875rem] text-text hover:bg-hover"
          >
            <SlidersIcon />
            Advanced search
          </button>
        </div>
      ) : null}

      {showAdvanced ? (
        <div className="absolute left-0 top-full z-[70] mt-2 w-full min-w-[300px]">
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
