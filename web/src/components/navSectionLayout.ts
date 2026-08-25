/**
 * Shared left-nav section layout: label column + trailing glyph column.
 * The trailing column is 1.5rem so it matches NavGlyphButton `size-6` (24px).
 */
export const NAV_SECTION_GRID_CLASS = "grid w-full grid-cols-[minmax(0,1fr)_1.5rem] items-center";

/** Fixed 15px slot for heading chevrons and row leading icons. */
export const NAV_LEADING_GLYPH_CLASS = "flex size-[15px] shrink-0 items-center justify-center";

/** Group / tag row shell: shared grid + hover/active fill. */
export function navGlyphRowClass(active: boolean): string {
  return `group relative ${NAV_SECTION_GRID_CLASS} rounded border-none py-0.5 text-left text-[0.875rem] text-text hover:bg-hover ${
    active ? "bg-hover font-semibold" : "bg-transparent font-normal"
  }`;
}
