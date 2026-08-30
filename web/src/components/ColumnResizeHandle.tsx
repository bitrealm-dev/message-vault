import { Z_RESIZE_HANDLE } from "../lib/zLayers";
import type { ColumnResizeHandleProps } from "./useColumnResize";

/** Vertical grip on the right edge of a resizable column. */
export default function ColumnResizeHandle({
  ariaLabel,
  width,
  minWidth,
  maxWidth,
  dragging,
  handleHover,
  handleProps,
}: {
  ariaLabel: string;
  width: number;
  minWidth: number;
  maxWidth: number;
  dragging: boolean;
  handleHover: boolean;
  handleProps: ColumnResizeHandleProps;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: interactive column resize grip cannot use native hr
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={ariaLabel}
      aria-valuenow={width}
      aria-valuemin={minWidth}
      aria-valuemax={maxWidth}
      tabIndex={0}
      {...handleProps}
      // w-2 matches `listScrollGutter`, the inset that keeps the list scrollbar clear of this strip.
      className={`absolute top-0 right-0 h-full w-2 touch-none cursor-col-resize bg-transparent ${Z_RESIZE_HANDLE}`}
    >
      <div
        aria-hidden
        className={`pointer-events-none absolute top-0 right-0 bottom-0 w-px ${
          dragging || handleHover ? "bg-accent" : "bg-transparent"
        }`}
      />
    </div>
  );
}
