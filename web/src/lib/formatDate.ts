/** Calendar date: year, month, and day (e.g. "Sep 9, 2024"). */
export function formatDay(iso: string): string {
  return new Date(iso).toLocaleDateString([], {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** Month and year only (e.g. "Sep 2024"). */
export function formatMonthYear(iso: string): string {
  return new Date(iso).toLocaleDateString([], {
    month: "short",
    year: "numeric",
  });
}

/** Locale default date (browser short date). */
export function formatLocaleDate(iso: string): string {
  return new Date(iso).toLocaleDateString();
}

/** Locale date + time for import history rows. */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * First and last message dates as a span.
 * Returns a single day when start and end format identically.
 */
export function formatDateSpan(
  start: string | null,
  end: string | null,
): string | null {
  if (start && end) {
    const a = formatDay(start);
    const b = formatDay(end);
    return a === b ? a : `${a} – ${b}`;
  }
  if (end) return formatDay(end);
  if (start) return formatDay(start);
  return null;
}

/** Unix seconds string → locale date, or "Never" when missing/invalid. */
export function formatUnixDate(secs: string | null | undefined): string {
  if (secs == null || secs === "") return "Never";
  const n = Number(secs);
  if (!Number.isFinite(n) || n <= 0) return "Never";
  try {
    return new Date(n * 1000).toLocaleDateString();
  } catch {
    return "Never";
  }
}

/** API ISO timestamp → YYYY-MM-DD (UTC), or null when unparseable. */
export function formatIsoDateOnly(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) {
    const m = iso.match(/^(\d{4}-\d{2}-\d{2})/);
    return m ? m[1] : null;
  }
  const y = d.getUTCFullYear();
  const mo = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${mo}-${day}`;
}
