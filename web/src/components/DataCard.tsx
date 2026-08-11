import type { ReactNode } from "react";

/** Header row band — distinct from the card body. */
export const dataCardHeaderRowClass = "border-b border-border bg-elevated";

/** Sortable / static header cells on the header band. */
export const dataCardHeaderCellClass =
  "px-2 py-2 text-center text-[0.688rem] font-semibold uppercase tracking-[0.04em] text-text outline-none cursor-pointer hover:text-accent data-hovered:text-accent";

/** Primary body cells. */
export const dataCardBodyCellClass =
  "px-3 py-2.5 align-middle text-center text-[0.813rem] leading-snug text-text";

/**
 * Bordered data card shell: panel surface, optional title/toolbar, scroll region.
 * Callers supply table markup (e.g. React Aria Table) as children.
 */
export default function DataCard({
  children,
  title,
  toolbar,
  className = "",
  maxWidthClass = "max-w-4xl",
}: {
  children: ReactNode;
  title?: ReactNode;
  toolbar?: ReactNode;
  className?: string;
  maxWidthClass?: string;
}) {
  const hasHeader = title != null || toolbar != null;
  return (
    <div
      className={`w-full ${maxWidthClass} rounded-lg border border-border bg-panel p-4 ${className}`.trim()}
    >
      {hasHeader ? (
        <div className="mb-3 flex items-center justify-between gap-3 pr-5">
          {title != null ? (
            <h3 className="m-0 min-w-0 text-[0.938rem] font-semibold text-text">{title}</h3>
          ) : (
            <span />
          )}
          {toolbar ? <div className="shrink-0">{toolbar}</div> : null}
        </div>
      ) : null}
      <div className="overflow-x-auto">{children}</div>
    </div>
  );
}
