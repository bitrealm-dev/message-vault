import { useState } from "react";
import type { SearchScope } from "../lib/recentSearches";
import type { AdvancedSearchMode } from "./AdvancedSearchForm";
import AppAccountMenu from "./AppAccountMenu";
import { loadWidth } from "./columnResize";
import {
  LEFT_PANEL_DEFAULT_WIDTH,
  LEFT_PANEL_MAX_WIDTH,
  LEFT_PANEL_MIN_WIDTH,
  LEFT_PANEL_STORAGE_KEY,
  LEFT_PANEL_WIDTH_VAR,
} from "./leftPanelWidth";
import SearchBar from "./SearchBar";

/** Which list the header search runs against. */
export type HeaderSearchTarget = "contacts" | "messages" | "trash";

/**
 * Every target uses the same bar; only the wording, the recents bucket, and the
 * advanced form differ. Trash searches conversations, so it takes the messages
 * form and its operator autocomplete.
 */
const SEARCH_TARGETS: Record<
  HeaderSearchTarget,
  { scope: SearchScope; placeholder: string; advancedMode: AdvancedSearchMode }
> = {
  contacts: { scope: "contact", placeholder: "Search contacts", advancedMode: "contacts" },
  messages: { scope: "message", placeholder: "Search messages", advancedMode: "messages" },
  trash: { scope: "trash", placeholder: "Search Trash", advancedMode: "messages" },
};

/** Full-width bar: app name on the left, search in the remaining space. */
export default function AppHeader({
  searchQuery,
  searchTarget,
  onSearchChange,
  onSearch,
}: {
  searchQuery: string;
  searchTarget: HeaderSearchTarget;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
}) {
  const target = SEARCH_TARGETS[searchTarget];
  // Same key as LeftPanel so a stored width does not flash at the default.
  const [brandWidth] = useState(() =>
    loadWidth(
      LEFT_PANEL_STORAGE_KEY,
      LEFT_PANEL_DEFAULT_WIDTH,
      LEFT_PANEL_MIN_WIDTH,
      LEFT_PANEL_MAX_WIDTH,
    ),
  );

  return (
    <header className="relative z-20 flex shrink-0 items-center border-b border-border bg-panel">
      <div
        className="box-border flex h-12 shrink-0 items-center px-3"
        style={{ width: `var(${LEFT_PANEL_WIDTH_VAR}, ${brandWidth}px)` }}
      >
        <AppAccountMenu />
      </div>
      <div className="flex min-w-0 flex-1 items-center justify-center px-3 py-2">
        <div className="w-full max-w-xl">
          <SearchBar
            key={searchTarget}
            value={searchQuery}
            scope={target.scope}
            placeholder={target.placeholder}
            advancedMode={target.advancedMode}
            onChange={onSearchChange}
            onSubmit={onSearch}
          />
        </div>
      </div>
    </header>
  );
}
