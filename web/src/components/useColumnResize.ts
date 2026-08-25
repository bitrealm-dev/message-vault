import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from "react";
import { useRef, useState } from "react";
import { clampWidth, loadWidth, saveWidth } from "./columnResize";

export type ColumnResizeHandleProps = {
  onPointerDown: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerCancel: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onKeyDown: (e: KeyboardEvent<HTMLDivElement>) => void;
  onMouseEnter: () => void;
  onMouseLeave: () => void;
};

export type UseColumnResizeOptions = {
  storageKey: string;
  defaultWidth: number;
  minWidth: number;
  maxWidth: number;
  /** Called when a drag starts (true) or ends (false). */
  onDraggingChange?: (dragging: boolean) => void;
};

export type UseColumnResizeResult = {
  width: number;
  dragging: boolean;
  handleHover: boolean;
  handleProps: ColumnResizeHandleProps;
};

/** Drag and keyboard resize for a vertical column, with localStorage persistence. */
export function useColumnResize({
  storageKey,
  defaultWidth,
  minWidth,
  maxWidth,
  onDraggingChange,
}: UseColumnResizeOptions): UseColumnResizeResult {
  const [width, setWidth] = useState(() => loadWidth(storageKey, defaultWidth, minWidth, maxWidth));
  const [dragging, setDragging] = useState(false);
  const [handleHover, setHandleHover] = useState(false);

  const startXRef = useRef(0);
  const startWidthRef = useRef(defaultWidth);
  const widthRef = useRef(width);
  widthRef.current = width;
  const onDraggingChangeRef = useRef(onDraggingChange);
  onDraggingChangeRef.current = onDraggingChange;

  const setDraggingState = (next: boolean) => {
    setDragging(next);
    onDraggingChangeRef.current?.(next);
  };

  const endDrag = (el: HTMLElement, pointerId: number) => {
    if (el.hasPointerCapture(pointerId)) {
      el.releasePointerCapture(pointerId);
    }
    setDraggingState(false);
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    saveWidth(storageKey, widthRef.current);
  };

  const onResizePointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    startXRef.current = e.clientX;
    startWidthRef.current = widthRef.current;
    setDraggingState(true);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";
  };

  const onResizePointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    const next = clampWidth(
      startWidthRef.current + (e.clientX - startXRef.current),
      minWidth,
      maxWidth,
    );
    widthRef.current = next;
    setWidth(next);
  };

  const onResizePointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    endDrag(e.currentTarget, e.pointerId);
  };

  const onResizeKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 24 : 8;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      const next = clampWidth(widthRef.current - step, minWidth, maxWidth);
      widthRef.current = next;
      setWidth(next);
      saveWidth(storageKey, next);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const next = clampWidth(widthRef.current + step, minWidth, maxWidth);
      widthRef.current = next;
      setWidth(next);
      saveWidth(storageKey, next);
    } else if (e.key === "Home") {
      e.preventDefault();
      widthRef.current = minWidth;
      setWidth(minWidth);
      saveWidth(storageKey, minWidth);
    } else if (e.key === "End") {
      e.preventDefault();
      widthRef.current = maxWidth;
      setWidth(maxWidth);
      saveWidth(storageKey, maxWidth);
    }
  };

  return {
    width,
    dragging,
    handleHover,
    handleProps: {
      onPointerDown: onResizePointerDown,
      onPointerMove: onResizePointerMove,
      onPointerUp: onResizePointerUp,
      onPointerCancel: onResizePointerUp,
      onKeyDown: onResizeKeyDown,
      onMouseEnter: () => setHandleHover(true),
      onMouseLeave: () => setHandleHover(false),
    },
  };
}
