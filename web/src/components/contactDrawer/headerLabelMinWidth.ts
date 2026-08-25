/** Horizontal padding + sort glyph gap + resizer hit area (px). */
const HEADER_CHROME_PX = 16 + 12 + 14;

/**
 * Minimum column width so the header label stays fully visible.
 * Uses canvas measureText when available; otherwise a character estimate.
 */
export function headerLabelMinWidth(label: string): number {
  const text = label.replace(/\s+/g, " ").trim();
  if (!text) return HEADER_CHROME_PX + 24;

  let textWidth: number;
  if (typeof document !== "undefined") {
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d");
    if (ctx) {
      ctx.font = '600 0.688rem ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';
      textWidth = ctx.measureText(text.toUpperCase()).width;
    } else {
      textWidth = text.length * 7.5;
    }
  } else {
    textWidth = text.length * 7.5;
  }

  return Math.ceil(textWidth + HEADER_CHROME_PX);
}
