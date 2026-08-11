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
 * Bordered data card shell: panel surface, optional toolbar, scroll region.
 * Callers supply table markup (e.g. React Aria Table) as children.
 */
export default function DataCard({
  children,
  toolbar,
  className = "",
  maxWidthClass = "max-w-4xl",
}: {
  children: ReactNode;
  toolbar?: ReactNode;
  className?: string;
  maxWidthClass?: string;
}) {
  return (
    <div
      className={`w-full ${maxWidthClass} rounded-lg border border-border bg-panel p-4 ${className}`.trim()}
    >
      {toolbar ? <div className="mb-3 flex justify-end pr-5">{toolbar}</div> : null}
      <div className="overflow-x-auto">{children}</div>
    </div>
  );
}
