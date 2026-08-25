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
      className="absolute top-0 right-0 z-[60] h-full w-3 translate-x-full touch-none cursor-col-resize bg-transparent"
    >
      <div
        aria-hidden
        className={`pointer-events-none absolute top-0 bottom-0 left-0 w-px ${
          dragging || handleHover ? "bg-accent" : "bg-transparent"
        }`}
      />
    </div>
  );
}
