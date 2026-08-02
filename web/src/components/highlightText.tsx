import type { ReactNode } from "react";

/** Wraps case-insensitive occurrences of `terms` in `<mark>`. */
export function highlightText(text: string, terms: string[]): ReactNode {
  const cleaned = terms.map((t) => t.trim()).filter(Boolean);
  if (cleaned.length === 0) return text;
  const pattern = cleaned
    .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");
  if (!pattern) return text;
  const re = new RegExp(`(${pattern})`, "gi");
  const parts = text.split(re);
  return parts.map((part, i) =>
    i % 2 === 1 ? (
      <mark
        key={i}
        className="rounded-sm bg-amber-300 px-0.5 font-medium text-black dark:bg-amber-300/90"
      >
        {part}
      </mark>
    ) : (
      part
    ),
  );
}
