import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  decodeMessageCursor,
  encodeMessageCursor,
  mergeMessagePages,
  messagesCoverIds,
} from "./messageCursor";

describe("messageCursor", () => {
  it("round-trips opaque cursors", () => {
    const cursor = {
      timestamp: "2024-07-17T14:05:09",
      sortOrder: 12,
      id: 99,
    };
    assert.deepEqual(decodeMessageCursor(encodeMessageCursor(cursor)), cursor);
  });

  it("rejects malformed cursors", () => {
    assert.equal(decodeMessageCursor("not-a-cursor"), null);
    assert.equal(decodeMessageCursor(""), null);
  });

  it("merges pages without duplicates and keeps chronological order", () => {
    const older = [
      { id: 1, timestamp: "2020-01-01T00:00:00" },
      { id: 2, timestamp: "2020-01-02T00:00:00" },
    ];
    const newer = [
      { id: 2, timestamp: "2020-01-02T00:00:00" },
      { id: 3, timestamp: "2021-01-01T00:00:00" },
    ];
    assert.deepEqual(
      mergeMessagePages(newer, older).map((m) => m.id),
      [1, 2, 3],
    );
  });

  it("orders equal timestamps by id", () => {
    const a = [
      { id: 2, timestamp: "2020-01-01T00:00:00" },
      { id: 1, timestamp: "2020-01-01T00:00:00" },
    ];
    assert.deepEqual(
      mergeMessagePages([], a).map((m) => m.id),
      [1, 2],
    );
  });

  it("checks whether required message ids are already loaded", () => {
    assert.equal(messagesCoverIds([{ id: 1 }, { id: 2 }], [2]), true);
    assert.equal(messagesCoverIds([{ id: 1 }], [1, 2]), false);
  });
});
