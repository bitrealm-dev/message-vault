import type { CachedContactDetail, CachedContactHandle } from "../../lib/contactDetailCache";
import { formatIsoDateOnly } from "../../lib/formatDate";

export type {
  ContactIdentityService,
  HandleService,
} from "../../lib/handleService";
export {
  CONTACT_IDENTITY_SERVICE_OPTIONS as HANDLE_SERVICE_OPTIONS,
  CONTACT_IDENTITY_SERVICES,
  formatHandleServiceLabel,
  handleServiceSelectValue,
  inferService,
} from "../../lib/handleService";

/** Lightweight row data so the drawer can paint before the detail API returns. */
export type ContactPreview = {
  id: string;
  name: string;
  handles?: string[];
  /**
   * True linked-identity count from the list API (`handle_count`).
   * List `handles` may include both raw and normalized forms of one identity;
   * stub rows while loading should match this count, not `handles.length`.
   */
  handleCount?: number;
  groups?: string[];
};

export type ContactBrowseKind = "all" | "direct" | "group";

/** Format an API ISO timestamp as YYYY-MM-DD for the handles table. */
export function formatHandleDate(iso: string | null | undefined): string | null {
  return formatIsoDateOnly(iso);
}

export function emptyHandleRow(handle: string): CachedContactHandle {
  return {
    handle,
    service: null,
    name_alias: null,
    start_date: null,
    end_date: null,
    individual_conversations: 0,
    group_conversations: 0,
    individual_message_count: 0,
    group_message_count: 0,
  };
}

/**
 * Build loading stub rows for the handles table.
 * Prefer `handleCount` (one row per linked identity) over the full preview
 * string list, which can list both raw and normalized forms of the same phone.
 */
export function previewHandleStubRows(
  handles: string[] | undefined,
  handleCount: number | undefined,
): CachedContactHandle[] {
  const list = handles ?? [];
  const count =
    handleCount != null && Number.isFinite(handleCount)
      ? Math.max(0, Math.floor(handleCount))
      : list.length;
  const rows: CachedContactHandle[] = [];
  for (let i = 0; i < count; i++) {
    rows.push(emptyHandleRow(list[i] ?? ""));
  }
  return rows;
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
