import type { ReactNode } from "react";
import { useState } from "react";
import GlobalSearch from "./GlobalSearch";
import AdvancedSearchForm, { type AdvancedSearchMode } from "./AdvancedSearchForm";

export default function ListColumn({
  searchQuery,
  searchMode,
  onSearchChange,
  onSearch,
  children,
}: {
  searchQuery: string;
  searchMode: AdvancedSearchMode;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
  children: ReactNode;
}) {
  const [showAdvancedSearch, setShowAdvancedSearch] = useState(false);

  return (
    <div
      style={{
        width: "300px",
        flexShrink: 0,
        borderRight: "1px solid var(--border)",
        background: "var(--panel)",
        color: "var(--text)",
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        // Visible so the advanced search panel can extend over the main column.
        overflow: "visible",
        position: "relative",
        zIndex: showAdvancedSearch ? 40 : 1,
      }}
    >
      <div
        style={{
          padding: "0.75rem",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
          position: "relative",
        }}
      >
        <GlobalSearch
          value={searchQuery}
          mode={searchMode === "contacts" ? "filter" : "search"}
          onChange={onSearchChange}
          onSubmit={(q) => onSearch(q)}
        />
        <button
          type="button"
          onClick={() => setShowAdvancedSearch(!showAdvancedSearch)}
          style={{
            fontSize: "0.688rem",
            border: "none",
            background: "none",
            color: "var(--muted)",
            cursor: "pointer",
            padding: "0.25rem 0 0",
          }}
        >
          {showAdvancedSearch
            ? "Hide advanced search"
            : searchMode === "contacts"
              ? "Advanced filters"
              : "Advanced search"}
        </button>
        {showAdvancedSearch && (
          <div
            style={{
              position: "absolute",
              top: "100%",
              left: 0,
              width: "560px",
              marginTop: "-1px",
              zIndex: 50,
            }}
          >
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
        )}
      </div>

      <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column", minHeight: 0 }}>
        {children}
      </div>
    </div>
  );
}
