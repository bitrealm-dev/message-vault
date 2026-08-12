/** Shared theme-aware Tailwind class strings (tokens from theme.css). */

/** Inset (~95% width) hairlines above and below a list row; adjacent rows share one line. */
export const listRowDividers =
  "relative before:pointer-events-none before:absolute before:inset-x-[2.5%] before:top-0 before:h-px before:bg-border after:pointer-events-none after:absolute after:inset-x-[2.5%] after:bottom-0 after:h-px after:bg-border";

/** Lighter/thinner hairline under each contact row (one line between neighbors). */
export const listRowDividersThin =
  "relative after:pointer-events-none after:absolute after:inset-x-[2.5%] after:bottom-0 after:h-px after:origin-center after:scale-y-50 after:bg-border/40";
