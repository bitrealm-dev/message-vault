import type { CachedContactDetail, CachedContactHandle } from "../../lib/contactDetailCache";

/** Lightweight row data so the drawer can paint before the detail API returns. */
export type ContactPreview = {
  id: string;
  name: string;
  handles?: string[];
};

export type ContactBrowseKind = "all" | "direct" | "group";

/** Messaging service choices for the handles table Add/Edit controls. */
export const HANDLE_SERVICE_OPTIONS = [
  { value: "phone", label: "Text message" },
  { value: "whatsapp", label: "WhatsApp" },
] as const;

export function inferService(handle: string, service: string | null | undefined): string {
  if (service && service.trim()) return service.trim().toLowerCase();
  const h = handle.trim();
  if (h.includes("@") && !h.startsWith("@")) return "email";
  if (/^\+?\d[\d\s().-]{6,}$/.test(h)) return "phone";
  return "unknown";
}

/** User-facing service label for the handles table Service column. */
export function formatHandleServiceLabel(
  handle: string,
  service: string | null | undefined,
): string {
  const lower = inferService(handle, service);
  if (lower === "whatsapp") return "WhatsApp";
  if (
    lower === "phone" ||
    lower === "sms" ||
    lower === "mms" ||
    lower === "sms/mms" ||
    lower === "imessage" ||
    lower === "ios" ||
    lower === "rcs"
  ) {
    return "Text message";
  }
  if (lower === "email") return "Email";
  if (lower === "unknown") return "—";
  return lower.charAt(0).toUpperCase() + lower.slice(1);
}

/** Map an API/inferred service id onto a HANDLE_SERVICE_OPTIONS value. */
export function handleServiceSelectValue(
  handle: string,
  service: string | null | undefined,
): "phone" | "whatsapp" {
  return inferService(handle, service) === "whatsapp" ? "whatsapp" : "phone";
}

/** Format an API ISO timestamp as YYYY-MM-DD for the handles table. */
export function formatHandleDate(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) {
    // Already a date-only string, or unparseable — take the leading YYYY-MM-DD if present.
    const m = iso.match(/^(\d{4}-\d{2}-\d{2})/);
    return m ? m[1] : null;
  }
  const y = d.getUTCFullYear();
  const mo = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${mo}-${day}`;
}

export function handleDateRangeLabel(h: CachedContactHandle): string {
  const start = formatHandleDate(h.start_date);
  const end = formatHandleDate(h.end_date);
  if (!start && !end) return "—";
  if (start && end) return start === end ? start : `${start} – ${end}`;
  return start ?? end ?? "—";
}

export function emptyHandleRow(handle: string): CachedContactHandle {
  return {
    handle,
    service: null,
    start_date: null,
    end_date: null,
    individual_conversations: 0,
    group_conversations: 0,
    individual_message_count: 0,
    group_message_count: 0,
  };
}

export function sumHandleTotals(handles: CachedContactDetail["handles"]): {
  individual_conversations: number;
  group_conversations: number;
  individual_message_count: number;
  group_message_count: number;
  start_date: string | null;
  end_date: string | null;
} {
  let individual_conversations = 0;
  let group_conversations = 0;
  let individual_message_count = 0;
  let group_message_count = 0;
  let start_date: string | null = null;
  let end_date: string | null = null;
  for (const h of handles) {
    individual_conversations += h.individual_conversations;
    group_conversations += h.group_conversations;
    individual_message_count += h.individual_message_count;
    group_message_count += h.group_message_count;
    if (h.start_date && (!start_date || h.start_date < start_date)) start_date = h.start_date;
    if (h.end_date && (!end_date || h.end_date > end_date)) end_date = h.end_date;
  }
  return {
    individual_conversations,
    group_conversations,
    individual_message_count,
    group_message_count,
    start_date,
    end_date,
  };
}
