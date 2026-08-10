import type { ReactNode, PointerEvent as ReactPointerEvent } from "react";
import { useEffect, useRef, useState } from "react";
import GlobalSearch from "./GlobalSearch";
import ContactSearch from "./ContactSearch";
import AdvancedSearchForm, { type AdvancedSearchMode } from "./AdvancedSearchForm";
import { ListColumnResizeContext } from "./ListColumnResizeContext";
import { isPortaledOverlayTarget } from "../lib/portaledOverlay";

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
  const [contactsSearchOpen, setContactsSearchOpen] = useState(false);
  const [width, setWidth] = useState(() => loadWidth());
  const [dragging, setDragging] = useState(false);
  const [handleHover, setHandleHover] = useState(false);
  const isContacts = searchMode === "contacts";
  const conversationsAdvancedRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showAdvancedSearch || isContacts) return;
    const onPointerDown = (e: MouseEvent) => {
      const root = conversationsAdvancedRef.current;
      if (!root || !(e.target instanceof Node)) return;
      if (root.contains(e.target) || isPortaledOverlayTarget(e.target)) return;
      setShowAdvancedSearch(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [showAdvancedSearch, isContacts]);

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
        // Visible so the advanced / contacts search panels can extend over the main column.
        zIndex: showAdvancedSearch || contactsSearchOpen ? 40 : 1,
      }}
      className="relative flex h-screen shrink-0 flex-col overflow-visible border-r border-border bg-panel text-text"
    >
      <div className="relative shrink-0 border-b border-border p-3">
        {isContacts ? (
          <ContactSearch
            value={searchQuery}
            onChange={onSearchChange}
            onSubmit={(q) => onSearch(q)}
            onOpenChange={setContactsSearchOpen}
          />
        ) : (
            <div ref={conversationsAdvancedRef} className="relative">
              <GlobalSearch
                value={searchQuery}
                mode="search"
                onChange={onSearchChange}
                onSubmit={(q) => onSearch(q)}
              />
              <button
                type="button"
                onClick={() => setShowAdvancedSearch(!showAdvancedSearch)}
                className="cursor-pointer border-none bg-none pt-1 text-[0.688rem] text-muted"
              >
                {showAdvancedSearch ? "Hide advanced search" : "Advanced search"}
              </button>
              {showAdvancedSearch ? (
                <div className="absolute left-0 top-full z-[70] -mt-px w-full min-w-[300px]">
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

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
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
        className={`absolute right-[-3px] top-0 h-full w-1.5 touch-none cursor-col-resize ${
          // Stay under the advanced/contacts search panel when it overhangs the main column.
          showAdvancedSearch || contactsSearchOpen ? "z-10" : "z-[60]"
        } ${dragging ? "bg-accent" : handleHover ? "bg-border" : "bg-transparent"}`}
      />
    </div>
    </ListColumnResizeContext.Provider>
  );
}
