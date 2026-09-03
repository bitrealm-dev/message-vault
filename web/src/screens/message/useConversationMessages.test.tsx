/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message } from "../../lib/types";
import { countExportMessages, exportMessages } from "../../lib/vaultApi";
import {
  buildFooterLabel,
  conversationYears,
  useConversationMessages,
} from "./useConversationMessages";

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  exportMessages: vi.fn(),
  countExportMessages: vi.fn(),
}));

const getMessages = vi.mocked(exportMessages);
const getCount = vi.mocked(countExportMessages);

function message(id: number): Message {
  return {
    id,
    source: "test",
    service: "sms",
    guid: null,
    timestamp: "2024-01-01T00:00:00Z",
    timestamp_utc: null,
    is_from_me: false,
    sender: "someone",
    subject: null,
    text: `from-${id}`,
    is_announcement: false,
    is_reply: false,
    num_replies: 0,
    sort_order: id,
    conversation: {
      id: 1,
      chat_identifier: "c",
      conversation_type: "direct",
      group_title: null,
      participants: [],
    },
    attachments: [],
    tapbacks: [],
  };
}

/** A promise plus the handles that settle it, so a test can control landing order. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  // Nothing else awaits this promise once the request is superseded.
  promise.catch(() => {});
  return { promise, resolve, reject };
}

/** Route the mocked calls: conversation A hangs on `slow`, conversation B answers at once. */
function routeGets(slow: Promise<{ messages: Message[] }>) {
  getCount.mockResolvedValue({ messages: 1 } as never);
  getMessages.mockImplementation(((params: { q: string }) =>
    params.q.includes("in:#A")
      ? slow
      : Promise.resolve({ messages: [message(2)] })) as unknown as typeof exportMessages);
}

describe("useConversationMessages", () => {
  beforeEach(() => {
    getMessages.mockReset();
    getCount.mockReset();
  });

  it("ignores a slow response from the conversation the user navigated away from", async () => {
    const slow = deferred<{ messages: Message[] }>();
    routeGets(slow.promise);

    const { result, rerender } = renderHook(
      ({ id }: { id: string }) => useConversationMessages(id),
      { initialProps: { id: "A" } },
    );

    rerender({ id: "B" });
    await waitFor(() => expect(result.current.messages.map((m) => m.id)).toEqual([2]));

    await act(async () => {
      slow.resolve({ messages: [message(1)] });
      await slow.promise;
    });

    expect(result.current.messages.map((m) => m.id)).toEqual([2]);
    expect(result.current.loading).toBe(false);
  });

  it("keeps the current page when a superseded request rejects", async () => {
    const slow = deferred<{ messages: Message[] }>();
    routeGets(slow.promise);

    const { result, rerender } = renderHook(
      ({ id }: { id: string }) => useConversationMessages(id),
      { initialProps: { id: "A" } },
    );

    rerender({ id: "B" });
    await waitFor(() => expect(result.current.messages.map((m) => m.id)).toEqual([2]));

    // The abort surfaces as a rejection; it must not blank B's messages.
    await act(async () => {
      slow.reject(new DOMException("aborted", "AbortError"));
      await slow.promise.catch(() => {});
    });

    expect(result.current.messages.map((m) => m.id)).toEqual([2]);
    expect(result.current.loading).toBe(false);
  });
});

describe("conversationYears", () => {
  it("covers both endpoint years", () => {
    expect(conversationYears("2020-05-01T00:00:00Z", "2022-02-01T00:00:00Z")).toEqual([
      2020, 2021, 2022,
    ]);
  });

  it("returns nothing without both endpoints", () => {
    expect(conversationYears(null, "2022-02-01T00:00:00Z")).toEqual([]);
  });
});

describe("buildFooterLabel", () => {
  it("shows the whole range for a year filter", () => {
    expect(buildFooterLabel(2021, 120, 0)).toBe("2021: 1–120 of 120");
  });

  it("shows the page window when browsing all years", () => {
    expect(buildFooterLabel(null, 120, 50)).toBe("Messages 51–100 of 120");
  });
});
