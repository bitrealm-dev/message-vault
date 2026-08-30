/** Shared Tailwind class strings that follow the theme colors in theme.css. */

/**
 * Inset hairline under a list row.
 *
 * One line per row, never a matching line on top: in a virtualized list the
 * rows are absolutely positioned, so a top line and the previous row's bottom
 * line only land on the same pixel while every measured height is exact. When
 * one is off they separate and the list shows doubled rules.
 *
 * The inset is a fixed 8px, never a percentage: a percentage is resolved
 * against the row's own width, so dragging the column width slid both ends of
 * every rule horizontally.
 */
export const listRowDivider =
  "relative after:pointer-events-none after:absolute after:inset-x-2 after:bottom-0 after:h-px after:bg-border";

/** Lighter/thinner hairline under each contact row (one line between neighbors). */
export const listRowDividersThin =
  "relative after:pointer-events-none after:absolute after:inset-x-2 after:bottom-0 after:h-px after:origin-center after:scale-y-50 after:bg-border/40";

/**
 * Right gutter on a list's scroll element, matching the column resize handle.
 *
 * The handle is an invisible strip pinned to the column's right edge. Without
 * this gutter it lands on top of the scrollbar, which then cannot be grabbed
 * and shows the col-resize cursor. Insetting the scroller moves the scrollbar
 * clear of the handle so both stay usable. Keep in step with the handle width
 * in ColumnResizeHandle.
 */
export const listScrollGutter = "mr-2";
