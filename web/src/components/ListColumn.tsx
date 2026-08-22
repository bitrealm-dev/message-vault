import type { ReactNode, PointerEvent as ReactPointerEvent } from "react";
import { useRef, useState } from "react";
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

export default function ListColumn({ children }: { children: ReactNode }) {
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
        style={{ width: `${width}px` }}
        className="relative flex h-full shrink-0 flex-col overflow-hidden border-r border-border bg-panel text-text"
      >
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</div>

        {/* biome-ignore lint/a11y/useSemanticElements: interactive column resize grip cannot use native hr */}
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
          className="absolute top-0 right-0 z-[60] h-full w-3 translate-x-full touch-none cursor-col-resize bg-transparent"
        >
          <div
            aria-hidden
            className={`pointer-events-none absolute top-0 bottom-0 left-0 w-px ${
              dragging || handleHover ? "bg-accent" : "bg-transparent"
            }`}
          />
        </div>
      </div>
    </ListColumnResizeContext.Provider>
  );
}
