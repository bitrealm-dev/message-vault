import { useCallback, useRef, useState, type ReactNode, type UIEvent } from "react";
import {
  ListBox,
  ListBoxItem,
  ListLayout,
  Virtualizer,
  type Selection,
} from "react-aria-components";
import { formatVisibleRange } from "../lib/usePagedList";
import { isTauri } from "../lib/tauri-check";
import { listRowDividers } from "../lib/tw";
import VirtualList, { type VisibleRange } from "./VirtualList";

const NEAR_END_THRESHOLD = 10;

type InfiniteOffsetListProps<T> = {
  items: T[];
  total: number;
  loading: boolean;
  refreshing?: boolean;
  filling: boolean;
  error: string;
  hasMore: boolean;
  requestMore: () => void;
  estimateSize: number;
  /** Variable row heights (filter subtitles). Maps to estimatedRowSize on RAC. */
  dynamicSize?: boolean;
  selectedId?: string | null;
  onSelect: (item: T) => void;
  getId: (item: T) => string;
  /** Accessible name for ListBoxItem (Tauri path). */
  getTextValue?: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  empty?: ReactNode;
  ariaLabel: string;
  /** Override the “N of total” denominator (e.g. filtered client count). */
  rangeTotal?: number;
  errorPrefix?: string;
};

function rowClass(selected: boolean): string {
  return `box-border flex w-full cursor-pointer items-center gap-2.5 border-none p-2 px-3 text-left text-text outline-none ${listRowDividers} ${
    selected ? "bg-hover" : "bg-transparent"
  }`;
}

function rangeFromScroll(
  scrollTop: number,
  clientHeight: number,
  estimateSize: number,
  count: number,
): VisibleRange {
  if (count === 0 || clientHeight <= 0 || estimateSize <= 0) {
    return { start: 0, end: 0 };
  }
  const startIdx = Math.floor(scrollTop / estimateSize);
  const endIdx = Math.min(
    count - 1,
    Math.ceil((scrollTop + clientHeight) / estimateSize) - 1,
  );
  return { start: startIdx + 1, end: Math.max(startIdx, endIdx) + 1 };
}

function RacVirtualList<T extends object>({
  items,
  estimateSize,
  dynamicSize,
  selectedId,
  onSelect,
  getId,
  getTextValue,
  renderRow,
  requestMore,
  hasMore,
  onVisibleRangeChange,
  empty,
  ariaLabel,
}: {
  items: T[];
  estimateSize: number;
  dynamicSize: boolean;
  selectedId: string | null;
  onSelect: (item: T) => void;
  getId: (item: T) => string;
  getTextValue?: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  requestMore: () => void;
  hasMore: boolean;
  onVisibleRangeChange: (range: VisibleRange) => void;
  empty?: ReactNode;
  ariaLabel: string;
}) {
  const itemById = useRef(new Map<string, T>());
  itemById.current = new Map(items.map((item) => [getId(item), item]));

  const maybeRequestMore = useCallback(
    (end1Based: number) => {
      if (!hasMore || items.length === 0) return;
      if (end1Based >= items.length - NEAR_END_THRESHOLD) {
        requestMore();
      }
    },
    [hasMore, items.length, requestMore],
  );

  const onScroll = (e: UIEvent<HTMLElement>) => {
    const el = e.currentTarget;
    const range = rangeFromScroll(
      el.scrollTop,
      el.clientHeight,
      estimateSize,
      items.length,
    );
    onVisibleRangeChange(range);
    maybeRequestMore(range.end);
  };

  const onSelectionChange = (keys: Selection) => {
    if (keys === "all") return;
    const id = [...keys][0];
    if (id == null) return;
    const item = itemById.current.get(String(id));
    if (item) onSelect(item);
  };

  if (items.length === 0 && empty) {
    return <div className="min-h-0 flex-1 overflow-auto">{empty}</div>;
  }

  const layoutOptions = dynamicSize
    ? { estimatedRowSize: estimateSize }
    : { rowSize: estimateSize };

  return (
    <Virtualizer layout={ListLayout} layoutOptions={layoutOptions}>
      <ListBox
        aria-label={ariaLabel}
        items={items}
        selectionMode="single"
        selectedKeys={selectedId ? new Set([selectedId]) : new Set()}
        onSelectionChange={onSelectionChange}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-auto outline-none"
        style={{ display: "block", padding: 0 }}
      >
        {(item) => {
          const id = getId(item);
          const selected = id === selectedId;
          return (
            <ListBoxItem
              id={id}
              textValue={getTextValue?.(item) ?? id}
              className={rowClass(selected)}
              style={
                dynamicSize
                  ? { minHeight: estimateSize }
                  : { height: "100%", minHeight: 0 }
              }
            >
              {renderRow(item)}
            </ListBoxItem>
          );
        }}
      </ListBox>
    </Virtualizer>
  );
}

function TanStackVirtualList<T>({
  items,
  estimateSize,
  dynamicSize,
  selectedId,
  onSelect,
  getId,
  renderRow,
  requestMore,
  hasMore,
  onVisibleRangeChange,
  empty,
}: {
  items: T[];
  estimateSize: number;
  dynamicSize: boolean;
  selectedId: string | null;
  onSelect: (item: T) => void;
  getId: (item: T) => string;
  getTextValue?: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  requestMore: () => void;
  hasMore: boolean;
  onVisibleRangeChange: (range: VisibleRange) => void;
  empty?: ReactNode;
}) {
  return (
    <VirtualList
      count={items.length}
      estimateSize={estimateSize}
      dynamicSize={dynamicSize}
      nearEndThreshold={NEAR_END_THRESHOLD}
      onVisibleRangeChange={onVisibleRangeChange}
      onNearEnd={() => {
        if (hasMore) requestMore();
      }}
      empty={empty}
      renderItem={(index) => {
        const item = items[index];
        if (!item) return null;
        const id = getId(item);
        const selected = id === selectedId;
        return (
          <button
            type="button"
            onClick={() => onSelect(item)}
            style={{
              height: dynamicSize ? "auto" : "100%",
              minHeight: dynamicSize ? estimateSize : undefined,
            }}
            className={rowClass(selected)}
          >
            {renderRow(item)}
          </button>
        );
      }}
    />
  );
}

export default function InfiniteOffsetList<T extends object>({
  items,
  total,
  loading,
  refreshing = false,
  filling,
  error,
  hasMore,
  requestMore,
  estimateSize,
  dynamicSize = false,
  selectedId = null,
  onSelect,
  getId,
  getTextValue,
  renderRow,
  empty,
  ariaLabel,
  rangeTotal,
  errorPrefix = "Could not load list",
}: InfiniteOffsetListProps<T>) {
  const [visibleRange, setVisibleRange] = useState<VisibleRange>({
    start: 0,
    end: 0,
  });
  const denom = rangeTotal ?? total;

  const rangeLabel =
    loading && items.length === 0
      ? "Loading…"
      : formatVisibleRange(
          visibleRange.start,
          visibleRange.end,
          denom,
          items.length,
        );

  if (error && items.length === 0) {
    return (
      <div className="p-4 text-[0.813rem] text-danger">
        {errorPrefix}: {error}
      </div>
    );
  }

  const listProps = {
    items,
    estimateSize,
    dynamicSize,
    selectedId,
    onSelect,
    getId,
    getTextValue,
    renderRow,
    requestMore,
    hasMore,
    onVisibleRangeChange: setVisibleRange,
    empty,
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border px-3 py-1.5 text-[0.688rem] text-muted">
        {rangeLabel}
        {refreshing ? " · updating…" : filling ? " · loading more…" : null}
      </div>
      {isTauri() ? (
        <RacVirtualList {...listProps} ariaLabel={ariaLabel} />
      ) : (
        <TanStackVirtualList {...listProps} />
      )}
    </div>
  );
}
