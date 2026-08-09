import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";

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
  renderItem,
  style,
  empty,
  footer,
}: VirtualListProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const onRangeRef = useRef(onVisibleRangeChange);
  onRangeRef.current = onVisibleRangeChange;
  const publishedRef = useRef<VisibleRange>({ start: 0, end: 0 });
  const pendingRef = useRef<VisibleRange | null>(null);
  const throttleTimerRef = useRef<number | null>(null);
  // Bump after layout/resize so scrollport metrics are re-read once the DOM has size.
  const [layoutTick, setLayoutTick] = useState(0);

  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimateSize,
    overscan,
  });

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

    // First real measurement: publish immediately so the label is not stuck on "… of N".
    if (prev.start < 1 && nextRange.start >= 1) {
      if (throttleTimerRef.current != null) {
        window.clearTimeout(throttleTimerRef.current);
        throttleTimerRef.current = null;
      }
      pendingRef.current = null;
      publishedRef.current = nextRange;
      onRangeRef.current?.(nextRange);
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
      publishedRef.current = pending;
      onRangeRef.current?.(pending);
    }, RANGE_REPORT_MS);
  }, [nextRange.start, nextRange.end]);

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
    const ro = new ResizeObserver(() => {
      virtualizer.measure();
      setLayoutTick((n) => n + 1);
    });
    ro.observe(el);
    // First paint often has clientHeight 0 until flex layout settles.
    setLayoutTick((n) => n + 1);
    return () => ro.disconnect();
  }, [virtualizer, count]);

  if (count === 0 && empty) {
    return (
      <div ref={parentRef} style={{ overflow: "auto", flex: 1, minHeight: 0, ...style }}>
        {empty}
      </div>
    );
  }

  return (
    <div ref={parentRef} style={{ overflow: "auto", flex: 1, minHeight: 0, ...style }}>
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
            ref={dynamicSize ? virtualizer.measureElement : undefined}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              height: dynamicSize ? undefined : `${estimateSize}px`,
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
