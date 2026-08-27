/** Symmetric gutter so a centered label clears the edge-pinned sort caret. */
const HEADER_CHROME_PX = 32;

/** Locale-formatted body maxima used to size count and date columns. */
export const HANDLE_TABLE_THREADS_MAX = "9,999";
export const HANDLE_TABLE_MESSAGES_MAX = "999,999";
export const HANDLE_TABLE_DATE_SAMPLE = "2020-12-31";

/** Body cell horizontal padding (`px-1` both sides) for Service / Identity / Alias. */
const CELL_PADDING_PX = 8;

/** Date and count cells use DataCard `px-3` (12px each side). */
export const HANDLE_TABLE_COUNT_CELL_PADDING_PX = 24;

/** Group Messages: `px-3` left plus `!pr-9` right for the hover trash control. */
export const HANDLE_TABLE_GROUP_CELL_PADDING_PX = 48;

const HEADER_FONT = '600 11px ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';
const BODY_FONT = '400 13px ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';

let measureContext: CanvasRenderingContext2D | null | undefined;
const headerMinCache = new Map<string, number>();

function getMeasureContext(): CanvasRenderingContext2D | null {
  if (measureContext !== undefined) {
    return measureContext;
  }
  if (typeof document === "undefined") {
    measureContext = null;
    return null;
  }
  const canvas = document.createElement("canvas");
  measureContext = canvas.getContext("2d");
  return measureContext;
}

function measureText(text: string, font: string, uppercase: boolean): number {
  const display = uppercase ? text.toUpperCase() : text;
  const ctx = getMeasureContext();
  if (ctx) {
    ctx.font = font;
    return ctx.measureText(display).width;
  }
  return display.length * (uppercase ? 7.5 : 7);
}

/** Match header `tracking-[0.04em]` at 11px so measured width matches paint. */
const HEADER_TRACKING_PX = 11 * 0.04;

/**
 * Minimum column width so the header label stays fully visible.
 * Uses canvas measureText when available; otherwise a character estimate.
 */
export function headerLabelMinWidth(label: string): number {
  const text = label.replace(/\s+/g, " ").trim();
  const cached = headerMinCache.get(text);
  if (cached !== undefined) {
    return cached;
  }
  if (!text) {
    const empty = HEADER_CHROME_PX + 24;
    headerMinCache.set(text, empty);
    return empty;
  }
  // letter-spacing applies between characters, so n glyphs have n − 1 gaps.
  const tracking = Math.max(0, text.length - 1) * HEADER_TRACKING_PX;
  const width = Math.ceil(measureText(text, HEADER_FONT, true) + tracking + HEADER_CHROME_PX);
  headerMinCache.set(text, width);
  return width;
}

/**
 * Initial column width: at least the header min, otherwise the widest body cell.
 */
export function columnInitialWidth(
  headerMinPx: number,
  cellTexts: string[],
  cellPaddingPx = CELL_PADDING_PX,
): number {
  let widest = 0;
  for (const raw of cellTexts) {
    const text = raw.replace(/\s+/g, " ").trim();
    if (!text) continue;
    widest = Math.max(widest, measureText(text, BODY_FONT, false));
  }
  return Math.max(headerMinPx, Math.ceil(widest + cellPaddingPx));
}
