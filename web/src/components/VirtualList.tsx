import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
import { useListColumnResizing } from "./ListColumnResizeContext";

/** How often the x–y of z label may update while scrolling. */
const RANGE_REPORT_MS = 50;

export type VisibleRange = {
  /** 1-based inclusive index of the first row intersecting the viewport. */
  start: number;
  /** 1-based inclusive index of the last row intersecting the viewport. */
  end: number;
};

type VirtualListProps = {
  count: number;
  estimateSize?: number;
  /**
   * When false (default), every row uses `estimateSize` exactly — avoids scroll
   * drift from measureElement on uniform lists. Set true only when row heights vary.
   */
  dynamicSize?: boolean;
  overscan?: number;
  onVisibleRangeChange?: (range: VisibleRange) => void;
  /** Fired when the last visible row is within `nearEndThreshold` of the end. */
  onNearEnd?: () => void;
  /** 0-based distance from the last index that counts as “near end” (default 10). */
  nearEndThreshold?: number;
  renderItem: (index: number, virtualRow: VirtualItem) => ReactNode;
  style?: CSSProperties;
  empty?: ReactNode;
  footer?: ReactNode;
};

function rangeFromVirtualItems(
  virtualItems: VirtualItem[],
  scrollOffset: number,
  viewportHeight: number,
  count: number,
): VisibleRange {
  if (count === 0 || virtualItems.length === 0 || viewportHeight <= 0) {
    return { start: 0, end: 0 };
  }

  const viewTop = scrollOffset;
  const viewBottom = scrollOffset + viewportHeight;
  let startIdx: number | null = null;
  let endIdx: number | null = null;

  for (const row of virtualItems) {
    if (row.end > viewTop && row.start < viewBottom) {
      if (startIdx === null) startIdx = row.index;
      endIdx = row.index;
    }
  }

  if (startIdx === null || endIdx === null) return { start: 0, end: 0 };
  return { start: startIdx + 1, end: endIdx + 1 };
}

export default function VirtualList({
  count,
  estimateSize = 56,
  dynamicSize = false,
  overscan = 10,
  onVisibleRangeChange,
  onNearEnd,
  nearEndThreshold = 10,
  renderItem,
  style,
  empty,
  footer,
}: VirtualListProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const onRangeRef = useRef(onVisibleRangeChange);
  onRangeRef.current = onVisibleRangeChange;
  const onNearEndRef = useRef(onNearEnd);
  onNearEndRef.current = onNearEnd;
  const publishedRef = useRef<VisibleRange>({ start: 0, end: 0 });
  const pendingRef = useRef<VisibleRange | null>(null);
  const throttleTimerRef = useRef<number | null>(null);
  const lastScrollHeightRef = useRef<number | null>(null);
  // Bump after layout/resize so scrollport metrics are re-read once the DOM has size.
  const [layoutTick, setLayoutTick] = useState(0);

  // Column drag: freeze dynamic row measurement so width changes don't remasure every frame.
  const columnResizing = useListColumnResizing();
  const measureRows = dynamicSize && !columnResizing;
  const wasResizingRef = useRef(false);

  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimateSize,
    overscan,
  });

  // After column drag ends: wait for wrap layout + measureElement refs, then remasure once.
  useEffect(() => {
    if (columnResizing) {
      wasResizingRef.current = true;
      return;
    }
    if (!dynamicSize || !wasResizingRef.current) return;
    wasResizingRef.current = false;

    let raf2 = 0;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        virtualizer.measure();
        setLayoutTick((n) => n + 1);
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      if (raf2) cancelAnimationFrame(raf2);
    };
  }, [columnResizing, dynamicSize, virtualizer]);

  const virtualItems = virtualizer.getVirtualItems();
  const scrollOffset = parentRef.current?.scrollTop ?? 0;
  const viewportHeight = parentRef.current?.clientHeight ?? 0;
  const nextRange = rangeFromVirtualItems(
    virtualItems,
    scrollOffset,
    viewportHeight,
    count,
  );
  void layoutTick;

  useEffect(() => {
    const prev = publishedRef.current;
    if (prev.start === nextRange.start && prev.end === nextRange.end) {
      pendingRef.current = null;
      return;
    }

    const publish = (range: VisibleRange) => {
      publishedRef.current = range;
      onRangeRef.current?.(range);
      if (
        range.end >= 1 &&
        count > 0 &&
        range.end >= count - nearEndThreshold
      ) {
        onNearEndRef.current?.();
      }
    };

    // First real measurement: publish immediately so the label is not stuck on "… of N".
    if (prev.start < 1 && nextRange.start >= 1) {
      if (throttleTimerRef.current != null) {
        window.clearTimeout(throttleTimerRef.current);
        throttleTimerRef.current = null;
      }
      pendingRef.current = null;
      publish(nextRange);
      return;
    }

    pendingRef.current = nextRange;
    if (throttleTimerRef.current != null) return;

    throttleTimerRef.current = window.setTimeout(() => {
      throttleTimerRef.current = null;
      const pending = pendingRef.current;
      pendingRef.current = null;
      if (!pending) return;
      const last = publishedRef.current;
      if (last.start === pending.start && last.end === pending.end) return;
      publish(pending);
    }, RANGE_REPORT_MS);
    // Depend on start/end primitives, not `nextRange` object identity — a new
    // range object each render would defeat the throttle and re-fire forever.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- nextRange.start/end
  }, [nextRange.start, nextRange.end, count, nearEndThreshold]);

  useEffect(() => {
    return () => {
      if (throttleTimerRef.current != null) {
        window.clearTimeout(throttleTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const el = parentRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      // Width-only changes (list column drag) must not remasure/re-render every pixel —
      // that is what made the conversation panel jitter. React only when height changes.
      const nextH = entries[0]?.contentRect.height ?? el.clientHeight;
      if (
        lastScrollHeightRef.current != null &&
        Math.abs(nextH - lastScrollHeightRef.current) < 0.5
      ) {
        return;
      }
      lastScrollHeightRef.current = nextH;
      if (columnResizing) return;
      virtualizer.measure();
      setLayoutTick((n) => n + 1);
    });
    ro.observe(el);
    // First paint often has clientHeight 0 until flex layout settles.
    lastScrollHeightRef.current = el.clientHeight;
    setLayoutTick((n) => n + 1);
    return () => ro.disconnect();
  }, [virtualizer, count, columnResizing]);

  if (count === 0 && empty) {
    return (
      <div ref={parentRef} className="min-h-0 flex-1 overflow-auto" style={style}>
        {empty}
      </div>
    );
  }

  return (
    <div ref={parentRef} className="min-h-0 flex-1 overflow-auto" style={style}>
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualItems.map((virtualRow) => (
          <div
            key={virtualRow.key}
            data-index={virtualRow.index}
            ref={measureRows ? virtualizer.measureElement : undefined}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              height: measureRows ? undefined : `${virtualRow.size}px`,
              transform: `translateY(${virtualRow.start}px)`,
              boxSizing: "border-box",
            }}
          >
            {renderItem(virtualRow.index, virtualRow)}
          </div>
        ))}
      </div>
      {footer}
    </div>
  );
}
