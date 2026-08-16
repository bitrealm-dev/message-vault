import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type UIEvent,
} from "react";
import {
  ListBox,
  ListBoxItem,
  ListLayout,
  Virtualizer,
} from "react-aria-components";
import { groupByLetter } from "../lib/contactSort";
import { formatVisibleRange } from "../lib/usePagedList";
import { isTauri } from "../lib/tauri-check";
import { listRowDividersThin } from "../lib/tw";
import ListRangeHeader from "./ListRangeHeader";
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
  /** When set, drives the highlighted row instead of `selectedId`. */
  isRowHighlighted?: (item: T) => boolean;
  /** Spacer before the A–Z letter so it lines up with the initials column. */
  sectionLead?: ReactNode;
  getId: (item: T) => string;
  /** Accessible name for ListBoxItem (Tauri path). */
  getTextValue?: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  empty?: ReactNode;
  ariaLabel: string;
  /** Override the “N of total” denominator (e.g. filtered client count). */
  rangeTotal?: number;
  errorPrefix?: string;
  /** Control on the right of the “N–M of total” row. */
  headerActions?: ReactNode;
  selectAllChecked?: boolean;
  selectAllIndeterminate?: boolean;
  onSelectAllChange?: (checked: boolean) => void;
  selectAllLabel?: string;
  /** Letter for in-list section headers. Omit while searching. */
  getSectionLetter?: (item: T) => string;
};

function rowClass(selected: boolean, hovered = false): string {
  const fill = selected
    ? "bg-hover-strong"
    : hovered
      ? "bg-hover"
      : "bg-transparent hover:bg-hover";
  return `box-border flex w-full cursor-pointer items-center gap-2.5 border-none p-2 px-3 text-left text-text outline-none ${listRowDividersThin} ${fill}`;
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
  isRowHighlighted,
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
  isRowHighlighted?: (item: T) => boolean;
  getId: (item: T) => string;
  getTextValue?: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  requestMore: () => void;
  hasMore: boolean;
  onVisibleRangeChange: (range: VisibleRange) => void;
  empty?: ReactNode;
  ariaLabel: string;
}) {
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
        selectionBehavior="replace"
        selectedKeys={selectedId ? new Set([selectedId]) : new Set()}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-auto outline-none"
        style={{ display: "block", padding: 0 }}
      >
        {(item) => {
          const id = getId(item);
          return (
            <ListBoxItem
              id={id}
              textValue={getTextValue?.(item) ?? id}
              onAction={() => onSelect(item)}
              className={({ isSelected, isHovered }) =>
                rowClass(isRowHighlighted?.(item) ?? isSelected, isHovered)
              }
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
  isRowHighlighted,
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
  isRowHighlighted?: (item: T) => boolean;
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
        const selected = isRowHighlighted?.(item) ?? id === selectedId;
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

const LETTER_DIVIDER =
  "flex items-center border-b border-border bg-panel px-3 py-1";

function SectionedLetterList<T>({
  items,
  selectedId,
  onSelect,
  isRowHighlighted,
  sectionLead,
  getId,
  renderRow,
  requestMore,
  hasMore,
  getSectionLetter,
  currentLetter,
  onVisibleRangeChange,
  empty,
}: {
  items: T[];
  selectedId: string | null;
  onSelect: (item: T) => void;
  isRowHighlighted?: (item: T) => boolean;
  sectionLead?: ReactNode;
  getId: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  requestMore: () => void;
  hasMore: boolean;
  getSectionLetter: (item: T) => string;
  currentLetter: string | null;
  onVisibleRangeChange: (range: VisibleRange) => void;
  empty?: ReactNode;
}) {
  const groups = groupByLetter(items, getSectionLetter);
  const indexById = new Map(items.map((item, i) => [getId(item), i]));
  const scrollerRef = useRef<HTMLDivElement>(null);
  const onRangeRef = useRef(onVisibleRangeChange);
  onRangeRef.current = onVisibleRangeChange;
  const requestMoreRef = useRef(requestMore);
  requestMoreRef.current = requestMore;
  const hasMoreRef = useRef(hasMore);
  hasMoreRef.current = hasMore;

  const publishVisibleRange = (root: HTMLElement) => {
    const rootRect = root.getBoundingClientRect();
    const rows = root.querySelectorAll("[data-contact-index]");
    let start = 0;
    let end = 0;
    for (const row of rows) {
      const rect = row.getBoundingClientRect();
      if (rect.bottom <= rootRect.top || rect.top >= rootRect.bottom) continue;
      const raw = row.getAttribute("data-contact-index");
      const idx = raw == null ? Number.NaN : Number(raw);
      if (!Number.isFinite(idx)) continue;
      const oneBased = idx + 1;
      if (start === 0) start = oneBased;
      end = oneBased;
    }
    onRangeRef.current({ start, end });
    if (
      hasMoreRef.current &&
      items.length > 0 &&
      end >= items.length - NEAR_END_THRESHOLD
    ) {
      requestMoreRef.current();
    }
  };

  useLayoutEffect(() => {
    const root = scrollerRef.current;
    if (root) publishVisibleRange(root);
  }, [items]);

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    publishVisibleRange(e.currentTarget);
  };

  if (items.length === 0 && empty) {
    return <div className="min-h-0 flex-1 overflow-auto">{empty}</div>;
  }

  return (
    <div
      ref={scrollerRef}
      className="min-h-0 flex-1 overflow-auto"
      onScroll={onScroll}
    >
      {groups.map(([letter, groupItems], groupIndex) => (
        <section key={`${letter}-${groupIndex}`} aria-label={`Names starting with ${letter}`}>
          {letter !== currentLetter ? (
            <div className={`${LETTER_DIVIDER} gap-2.5`}>
              {sectionLead}
              <span className="flex h-7 w-7 shrink-0 items-center justify-center text-[0.75rem] font-semibold text-muted">
                {letter}
              </span>
            </div>
          ) : null}
          {groupItems.map((item) => {
            const id = getId(item);
            const selected = isRowHighlighted?.(item) ?? id === selectedId;
            const index = indexById.get(id) ?? 0;
            return (
              <button
                key={id}
                type="button"
                data-contact-index={index}
                onClick={() => onSelect(item)}
                className={rowClass(selected)}
              >
                {renderRow(item)}
              </button>
            );
          })}
        </section>
      ))}
    </div>
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
  isRowHighlighted,
  sectionLead,
  getId,
  getTextValue,
  renderRow,
  empty,
  ariaLabel,
  rangeTotal,
  errorPrefix = "Could not load list",
  headerActions,
  selectAllChecked = false,
  selectAllIndeterminate = false,
  onSelectAllChange,
  selectAllLabel,
  getSectionLetter,
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

  const firstVisibleIndex =
    visibleRange.start > 0 ? visibleRange.start - 1 : 0;
  const firstVisible = items[firstVisibleIndex];
  const headerLetter =
    getSectionLetter && firstVisible
      ? getSectionLetter(firstVisible)
      : null;

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
    isRowHighlighted,
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
      <ListRangeHeader
        rangeLabel={rangeLabel}
        refreshing={refreshing}
        filling={filling}
        actions={headerActions}
        selectAllChecked={selectAllChecked}
        selectAllIndeterminate={selectAllIndeterminate}
        onSelectAllChange={onSelectAllChange}
        selectAllLabel={selectAllLabel}
        selectAllDisabled={items.length === 0}
      />
      {headerLetter ? (
        <div className="flex shrink-0 items-center border-b border-border bg-panel px-3 py-1">
          <span className="flex h-7 w-7 shrink-0 items-center justify-center text-[0.75rem] font-semibold text-muted">
            {headerLetter}
          </span>
        </div>
      ) : null}
      {getSectionLetter ? (
        <SectionedLetterList
          items={items}
          selectedId={selectedId}
          onSelect={onSelect}
          isRowHighlighted={isRowHighlighted}
          sectionLead={sectionLead}
          getId={getId}
          renderRow={renderRow}
          requestMore={requestMore}
          hasMore={hasMore}
          getSectionLetter={getSectionLetter}
          currentLetter={headerLetter}
          onVisibleRangeChange={setVisibleRange}
          empty={empty}
        />
      ) : isTauri() ? (
        <RacVirtualList {...listProps} ariaLabel={ariaLabel} />
      ) : (
        <TanStackVirtualList {...listProps} />
      )}
    </div>
  );
}
