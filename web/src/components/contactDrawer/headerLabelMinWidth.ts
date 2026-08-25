/** Left pad (`pl-1`) only — resizer is absolute, sort glyph does not reserve width. */
const HEADER_CHROME_PX = 4;

/** Body cell horizontal padding (`px-1` both sides). */
const CELL_PADDING_PX = 8;

const HEADER_FONT = '600 11px ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';
const BODY_FONT = '400 13px ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';

function measureText(text: string, font: string, uppercase: boolean): number {
  const display = uppercase ? text.toUpperCase() : text;
  if (typeof document !== "undefined") {
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d");
    if (ctx) {
      ctx.font = font;
      return ctx.measureText(display).width;
    }
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
  if (!text) return HEADER_CHROME_PX + 24;
  const tracking = text.length * HEADER_TRACKING_PX;
  return Math.ceil(measureText(text, HEADER_FONT, true) + tracking + HEADER_CHROME_PX);
}

/**
 * Initial column width: at least the header min, otherwise the widest body cell.
 */
export function columnInitialWidth(headerMinPx: number, cellTexts: string[]): number {
  let widest = 0;
  for (const raw of cellTexts) {
    const text = raw.replace(/\s+/g, " ").trim();
    if (!text) continue;
    widest = Math.max(widest, measureText(text, BODY_FONT, false));
  }
  return Math.max(headerMinPx, Math.ceil(widest + CELL_PADDING_PX));
}
