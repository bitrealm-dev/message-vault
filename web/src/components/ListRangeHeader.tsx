import type { ReactNode } from "react";

/** Shared “N–M of total” header chrome for conversation and contact lists. */
export default function ListRangeHeader({
  rangeLabel,
  refreshing = false,
  filling = false,
  actions,
  letter,
}: {
  rangeLabel: string;
  refreshing?: boolean;
  filling?: boolean;
  /** Right side of the range row (sort control on the contact list). */
  actions?: ReactNode;
  /** Current name-section letter, aligned with the contact avatar column. */
  letter?: string | null;
}) {
  let activitySuffix = "";
  if (refreshing) activitySuffix = " · updating…";
  else if (filling) activitySuffix = " · loading more…";

  return (
    <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-1">
      <div className="flex min-w-0 items-center gap-2.5">
        {letter ? (
          <span className="flex h-7 w-7 shrink-0 items-center justify-center text-[0.75rem] font-semibold text-muted">
            {letter}
          </span>
        ) : null}
        <span className="min-w-0 truncate text-[0.688rem] text-muted">
          {rangeLabel}
          {activitySuffix}
        </span>
      </div>
      {actions ? <div className="shrink-0">{actions}</div> : null}
    </div>
  );
}
