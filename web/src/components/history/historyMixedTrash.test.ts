import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";

import { redoCommand, undoCommand } from "./historyRunner";
import {
  redoToastTextForCommand,
  toastTextForCommand,
  trashMessageThreadsLabel,
  undoToastTextForCommand,
  type HistoryCommand,
} from "./historyTypes";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

function command(
  handles: string[],
  conversationIds: number[],
  subjects: string[],
): Extract<HistoryCommand, { type: "trashMessageThreads" }> {
  return {
    type: "trashMessageThreads",
    handles,
    conversationIds,
    subjects,
    label: trashMessageThreadsLabel(subjects),
  };
}

describe("mixed message trash history", () => {
  it("uses accurate singular and plural toast copy", () => {
    const one = command(["+15555550123"], [], ["Alex"]);
    assert.equal(toastTextForCommand(one), "Deleted message Alex");
    assert.equal(undoToastTextForCommand(one), "Undeleted message Alex");
    assert.equal(redoToastTextForCommand(one), "Redid — Deleted message Alex");
    assert.equal(one.label, "Delete message Alex");

    const mixed = command(
      ["+15555550123"],
      [42],
      ["Alex", "Weekend plans"],
    );
    assert.equal(
      toastTextForCommand(mixed),
      "Deleted 2 messages Alex, Weekend plans",
    );
    assert.equal(
      undoToastTextForCommand(mixed),
      "Undeleted 2 messages Alex, Weekend plans",
    );
    assert.equal(mixed.label, "Delete 2 messages");
  });

  it("undoes and redoes a mixed command with one batch request", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    globalThis.fetch = (async (input, init) => {
      requests.push({ url: String(input), init });
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    }) as typeof fetch;
    const mixed = command(
      ["+15555550123"],
      [42],
      ["Alex", "Weekend plans"],
    );

    await undoCommand(mixed);
    await redoCommand(mixed);

    assert.equal(requests.length, 2);
    assert.deepEqual(
      requests.map(({ url, init }) => ({
        url,
        method: init?.method,
        body: JSON.parse(String(init?.body)),
      })),
      [
        {
          url: "/api/messages/trash",
          method: "DELETE",
          body: { handles: ["+15555550123"], conversationIds: [42] },
        },
        {
          url: "/api/messages/trash",
          method: "POST",
          body: { handles: ["+15555550123"], conversationIds: [42] },
        },
      ],
    );
  });
});
