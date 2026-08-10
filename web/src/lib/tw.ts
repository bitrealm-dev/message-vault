/** Shared theme-aware Tailwind class strings (tokens from theme.css). */

export const input =
  "box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.875rem] text-text outline-none focus:border-accent";
export const inputOnBg =
  "box-border w-full rounded border border-border bg-bg px-2 py-1.5 text-[0.875rem] text-text outline-none focus:border-accent";
export const sectionLabel =
  "text-[0.688rem] font-semibold uppercase tracking-[0.05em] text-muted";
export const emptyState = "p-4 text-[0.813rem] text-muted";
export const pill =
  "inline-flex items-center rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent";
export const closeButton =
  "cursor-pointer border-none bg-transparent p-1 text-[1.25rem] leading-none text-muted hover:text-text";
export const divider = "my-6 border-t border-border";

/** Inset (~95% width) hairlines above and below a list row; adjacent rows share one line. */
export const listRowDividers =
  "relative before:pointer-events-none before:absolute before:inset-x-[2.5%] before:top-0 before:h-px before:bg-border after:pointer-events-none after:absolute after:inset-x-[2.5%] after:bottom-0 after:h-px after:bg-border";
