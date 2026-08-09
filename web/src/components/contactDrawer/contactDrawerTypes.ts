import type { CachedContactDetail } from "../../lib/contactDetailCache";

/** Lightweight row data so the drawer can paint before the detail API returns. */
export type ContactPreview = {
  id: string;
  name: string;
  handles?: string[];
};

export type ContactBrowseKind = "all" | "direct" | "group";

export const SERVICES = ["phone", "email", "discord", "instagram", "telegram", "signal"];

export function inferService(handle: string, service: string | null | undefined): string {
  if (service && service.trim()) return service.trim().toLowerCase();
  const h = handle.trim();
  if (h.includes("@") && !h.startsWith("@")) return "email";
  if (/^\+?\d[\d\s().-]{6,}$/.test(h)) return "phone";
  return "unknown";
}

export function yearRangeLabel(
  handles: CachedContactDetail["handles"],
): string | null {
  let minY: number | null = null;
  let maxY: number | null = null;
  for (const h of handles) {
    if (h.start_date) {
      const y = new Date(h.start_date).getFullYear();
      if (!Number.isNaN(y)) minY = minY === null ? y : Math.min(minY, y);
    }
    if (h.end_date) {
      const y = new Date(h.end_date).getFullYear();
      if (!Number.isNaN(y)) maxY = maxY === null ? y : Math.max(maxY, y);
    }
  }
  if (minY === null && maxY === null) return null;
  if (minY === null) return String(maxY);
  if (maxY === null) return String(minY);
  return minY === maxY ? String(minY) : `${minY}–${maxY}`;
}
