/** Shared “N–M of total” header chrome for conversation and contact lists. */
export default function ListRangeHeader({
  rangeLabel,
  refreshing = false,
  filling = false,
}: {
  rangeLabel: string;
  refreshing?: boolean;
  filling?: boolean;
}) {
  let activitySuffix = "";
  if (refreshing) activitySuffix = " · updating…";
  else if (filling) activitySuffix = " · loading more…";

  return (
    <div className="shrink-0 border-b border-border px-3 py-1.5 text-[0.688rem] text-muted">
      {rangeLabel}
      {activitySuffix}
    </div>
  );
}
