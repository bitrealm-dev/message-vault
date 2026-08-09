import type { ReactNode } from "react";
import { useState } from "react";
import GlobalSearch from "./GlobalSearch";
import AdvancedSearchForm from "./AdvancedSearchForm";

export default function ListColumn({
  searchQuery,
  onSearchChange,
  onSearch,
  children,
}: {
  searchQuery: string;
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
        borderRight: "1px solid #e5e7eb",
        background: "#fff",
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        overflow: "hidden",
      }}
    >
      <div style={{ padding: "0.75rem", borderBottom: "1px solid #e5e7eb", flexShrink: 0 }}>
        <GlobalSearch
          value={searchQuery}
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
            color: "#6b7280",
            cursor: "pointer",
            padding: "0.25rem 0 0",
          }}
        >
          {showAdvancedSearch ? "Hide advanced search" : "Advanced search"}
        </button>
        {showAdvancedSearch && (
          <div style={{ marginTop: "0.5rem" }}>
            <AdvancedSearchForm
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
