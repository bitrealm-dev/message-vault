/** Compact rounded count chip (contact / group list chrome). */
export function CountBadge({
  count,
  title,
}: {
  count: number;
  title?: string;
}) {
  return (
    <span
      title={title}
      className="inline-flex h-[1.125rem] min-w-[1.125rem] items-center justify-center rounded px-1 text-[11px] leading-none font-medium text-text tabular-nums bg-elevated"
    >
      {count.toLocaleString()}
    </span>
  );
}
