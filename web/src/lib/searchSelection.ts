import type { SearchConversationHit } from "@/lib/search";

/**
 * Ordered unique contact IDs from displayed search hits.
 * Skips group conversations, unassigned handles, and duplicate contacts.
 */
export function orderedSearchContactIds(
  hits: readonly SearchConversationHit[],
): number[] {
  const seen = new Set<number>();
  const out: number[] = [];
  for (const hit of hits) {
    if (hit.conversationType !== "individual") continue;
    if (hit.contactId == null) continue;
    if (seen.has(hit.contactId)) continue;
    seen.add(hit.contactId);
    out.push(hit.contactId);
  }
  return out;
}

/**
 * Contiguous range selection from an anchor contact to a clicked contact,
 * using the displayed search-result order.
 */
export function applySearchRangeSelect(
  orderedIds: readonly number[],
  clickId: number,
  anchorId: number | null,
): Set<number> {
  const clickIndex = orderedIds.indexOf(clickId);
  if (clickIndex < 0) return new Set();

  const anchorIndex =
    anchorId != null ? orderedIds.indexOf(anchorId) : -1;
  if (anchorIndex < 0) return new Set([clickId]);

  const from = Math.min(anchorIndex, clickIndex);
  const to = Math.max(anchorIndex, clickIndex);
  const next = new Set<number>();
  for (let i = from; i <= to; i++) {
    const id = orderedIds[i];
    if (id !== undefined) next.add(id);
  }
  return next;
}
