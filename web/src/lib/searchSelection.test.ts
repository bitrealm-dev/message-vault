import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { SearchConversationHit } from "@/lib/search";
import {
  applySearchRangeSelect,
  orderedSearchContactIds,
} from "./searchSelection";

function hit(
  partial: Partial<SearchConversationHit> &
    Pick<SearchConversationHit, "conversationId">,
): SearchConversationHit {
  return {
    conversationType: "individual",
    contactId: null,
    title: "t",
    chatIdentifier: "c",
    matchCount: 1,
    dateStart: null,
    dateEnd: null,
    topMatch: null,
    ...partial,
  };
}

describe("orderedSearchContactIds", () => {
  it("keeps first occurrence of each contact and skips groups/unassigned", () => {
    const hits = [
      hit({ conversationId: 1, contactId: 10 }),
      hit({ conversationId: 2, conversationType: "group", contactId: null }),
      hit({ conversationId: 3, contactId: null }),
      hit({ conversationId: 4, contactId: 20 }),
      hit({ conversationId: 5, contactId: 10 }),
    ];
    assert.deepEqual(orderedSearchContactIds(hits), [10, 20]);
  });
});

describe("applySearchRangeSelect", () => {
  const ordered = [10, 20, 30, 40];

  it("selects only the clicked contact when there is no anchor", () => {
    assert.deepEqual([...applySearchRangeSelect(ordered, 30, null)], [30]);
  });

  it("selects the contiguous range from anchor to click", () => {
    assert.deepEqual(
      [...applySearchRangeSelect(ordered, 40, 20)].sort((a, b) => a - b),
      [20, 30, 40],
    );
  });

  it("works when click is above the anchor", () => {
    assert.deepEqual(
      [...applySearchRangeSelect(ordered, 10, 30)].sort((a, b) => a - b),
      [10, 20, 30],
    );
  });

  it("returns empty when the click is not in the ordered list", () => {
    assert.deepEqual([...applySearchRangeSelect(ordered, 99, 20)], []);
  });
});
