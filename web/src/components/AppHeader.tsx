import { useEffect, useRef, useState } from "react";
import AdvancedSearchForm, { type AdvancedSearchMode } from "./AdvancedSearchForm";
import AppAccountMenu from "./AppAccountMenu";
import ContactSearch from "./ContactSearch";
import GlobalSearch from "./GlobalSearch";
import { shouldIgnoreOutsideDismiss } from "../lib/portaledOverlay";

/** Full-width bar: app name on the left, search in the remaining space. */
export default function AppHeader({
  searchQuery,
  searchMode,
  onSearchChange,
  onSearch,
}: {
  searchQuery: string;
  searchMode: AdvancedSearchMode;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
}) {
  const [showAdvancedSearch, setShowAdvancedSearch] = useState(false);
  const conversationsAdvancedRef = useRef<HTMLDivElement>(null);
  const isContacts = searchMode === "contacts";

  useEffect(() => {
    if (!showAdvancedSearch || isContacts) return;
    const onPointerDown = (e: MouseEvent) => {
      if (shouldIgnoreOutsideDismiss(e, conversationsAdvancedRef.current)) return;
      setShowAdvancedSearch(false);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [showAdvancedSearch, isContacts]);

  return (
    <header className="relative z-20 flex shrink-0 items-center border-b border-border bg-panel">
      <div className="flex h-12 w-[220px] shrink-0 items-center px-3">
        <AppAccountMenu />
      </div>
      <div className="flex min-w-0 flex-1 items-center justify-center px-3 py-2">
        <div className="w-full max-w-xl">
          {isContacts ? (
            <ContactSearch
              value={searchQuery}
              onChange={onSearchChange}
              onSubmit={onSearch}
            />
          ) : (
            <div ref={conversationsAdvancedRef} className="relative flex items-center gap-2">
              <div className="min-w-0 flex-1">
                <GlobalSearch
                  value={searchQuery}
                  mode="search"
                  onChange={onSearchChange}
                  onSubmit={onSearch}
                />
              </div>
              <button
                type="button"
                onClick={() => setShowAdvancedSearch(!showAdvancedSearch)}
                className="shrink-0 cursor-pointer border-none bg-none text-[0.688rem] text-muted"
              >
                {showAdvancedSearch ? "Hide" : "Advanced"}
              </button>
              {showAdvancedSearch ? (
                <div className="absolute left-0 top-full z-[70] mt-1 w-full min-w-[300px]">
                  <AdvancedSearchForm
                    mode={searchMode}
                    onApply={(q) => {
                      onSearchChange(q);
                      onSearch(q);
                      setShowAdvancedSearch(false);
                    }}
                    onClose={() => setShowAdvancedSearch(false)}
                  />
                </div>
              ) : null}
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
