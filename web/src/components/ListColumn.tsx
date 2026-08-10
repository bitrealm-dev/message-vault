import type { ReactNode, PointerEvent as ReactPointerEvent } from "react";
import { useRef, useState } from "react";
import GlobalSearch from "./GlobalSearch";
import AdvancedSearchForm, { type AdvancedSearchMode } from "./AdvancedSearchForm";
import { ListColumnResizeContext } from "./ListColumnResizeContext";

const DEFAULT_WIDTH = 300;
const MIN_WIDTH = 220;
const MAX_WIDTH = 560;
const STORAGE_KEY = "listColumnWidth:v1";

function clampWidth(n: number): number {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(n)));
}

function loadWidth(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_WIDTH;
    const n = Number(raw);
    if (!Number.isFinite(n)) return DEFAULT_WIDTH;
    return clampWidth(n);
  } catch {
    return DEFAULT_WIDTH;
  }
}

function saveWidth(n: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(n));
  } catch {
    // private browsing / quota
  }
}

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
  const [width, setWidth] = useState(() => loadWidth());
  const [dragging, setDragging] = useState(false);
  const [handleHover, setHandleHover] = useState(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(DEFAULT_WIDTH);
  const widthRef = useRef(width);
  widthRef.current = width;

  const endDrag = (el: HTMLElement, pointerId: number) => {
    if (el.hasPointerCapture(pointerId)) {
      el.releasePointerCapture(pointerId);
    }
    setDragging(false);
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    saveWidth(widthRef.current);
  };

  const onResizePointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    startXRef.current = e.clientX;
    startWidthRef.current = widthRef.current;
    setDragging(true);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";
  };

  const onResizePointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    const next = clampWidth(startWidthRef.current + (e.clientX - startXRef.current));
    widthRef.current = next;
    setWidth(next);
  };

  const onResizePointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    endDrag(e.currentTarget, e.pointerId);
  };

  const onResizeKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 24 : 8;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      const next = clampWidth(widthRef.current - step);
      widthRef.current = next;
      setWidth(next);
      saveWidth(next);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const next = clampWidth(widthRef.current + step);
      widthRef.current = next;
      setWidth(next);
      saveWidth(next);
    } else if (e.key === "Home") {
      e.preventDefault();
      widthRef.current = MIN_WIDTH;
      setWidth(MIN_WIDTH);
      saveWidth(MIN_WIDTH);
    } else if (e.key === "End") {
      e.preventDefault();
      widthRef.current = MAX_WIDTH;
      setWidth(MAX_WIDTH);
      saveWidth(MAX_WIDTH);
    }
  };

  return (
    <ListColumnResizeContext.Provider value={dragging}>
    <div
      data-list-column
      style={{
        width: `${width}px`,
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

      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize list column"
        aria-valuenow={width}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        tabIndex={0}
        onPointerDown={onResizePointerDown}
        onPointerMove={onResizePointerMove}
        onPointerUp={onResizePointerUp}
        onPointerCancel={onResizePointerUp}
        onKeyDown={onResizeKeyDown}
        onMouseEnter={() => setHandleHover(true)}
        onMouseLeave={() => setHandleHover(false)}
        style={{
          position: "absolute",
          top: 0,
          right: -3,
          width: 6,
          height: "100%",
          cursor: "col-resize",
          zIndex: 60,
          touchAction: "none",
          background: dragging
            ? "var(--accent)"
            : handleHover
              ? "var(--border)"
              : "transparent",
        }}
      />
    </div>
    </ListColumnResizeContext.Provider>
  );
}
