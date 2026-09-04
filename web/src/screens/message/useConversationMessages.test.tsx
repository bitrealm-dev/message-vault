/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message } from "../../lib/types";
import { listConversationMessages } from "../../lib/vaultApi";
import { mockedAuth, VaultProviders } from "../../test/vaultProviders";
import {
  buildFooterLabel,
  conversationYears,
  useConversationMessages,
} from "./useConversationMessages";

vi.mock("../../lib/auth", () => ({ useAuth: () => mockedAuth }));

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  listConversationMessages: vi.fn(),
}));

const getMessages = vi.mocked(listConversationMessages);

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

type MessagePage = { items: Message[]; total: number; limit: number; offset: number };
function page(items: Message[]): MessagePage {
  return { items, total: items.length, limit: 50, offset: 0 };
}

/** Route the mocked calls: conversation 1 hangs on `slow`, conversation 2 answers at once. */
function routeGets(slow: Promise<MessagePage>) {
  getMessages.mockImplementation(((id: number) =>
    id === 1
      ? slow
      : Promise.resolve(page([message(2)]))) as unknown as typeof listConversationMessages);
}

describe("useConversationMessages", () => {
  beforeEach(() => {
    getMessages.mockReset();
  });

  it("ignores a slow response from the conversation the user navigated away from", async () => {
    const slow = deferred<MessagePage>();
    routeGets(slow.promise);

    const { result, rerender } = renderHook(
      ({ id }: { id: number }) => useConversationMessages(id),
      { initialProps: { id: 1 }, wrapper: VaultProviders },
    );

    rerender({ id: 2 });
    await waitFor(() => expect(result.current.messages.map((m) => m.id)).toEqual([2]));

    slow.resolve(page([message(1)]));
    await slow.promise;

    // Conversation 1's own cache entry now holds its answer, but the hook is
    // reading conversation 2's entry, so its messages are untouched.
    expect(result.current.messages.map((m) => m.id)).toEqual([2]);
    expect(result.current.loading).toBe(false);
  });

  it("keeps the current page when a request rejects", async () => {
    const slow = deferred<MessagePage>();
    routeGets(slow.promise);

    const { result, rerender } = renderHook(
      ({ id }: { id: number }) => useConversationMessages(id),
      { initialProps: { id: 1 }, wrapper: VaultProviders },
    );

    rerender({ id: 2 });
    await waitFor(() => expect(result.current.messages.map((m) => m.id)).toEqual([2]));

    // Conversation 1's own entry fails; it must not touch conversation 2's.
    slow.reject(new Error("boom"));
    await slow.promise.catch(() => {});

    expect(result.current.messages.map((m) => m.id)).toEqual([2]);
    expect(result.current.loading).toBe(false);
  });

  it("asks for a year with year=, and walks its pages until the total is covered", async () => {
    // Page one of the unfiltered view, then a two-page year.
    getMessages
      .mockResolvedValueOnce(page([message(9)]))
      .mockResolvedValueOnce({ items: [message(1), message(2)], total: 3, limit: 500, offset: 0 })
      .mockResolvedValueOnce({ items: [message(3)], total: 3, limit: 500, offset: 2 });

    const { result } = renderHook(({ id }: { id: number }) => useConversationMessages(id), {
      initialProps: { id: 7 },
      wrapper: VaultProviders,
    });
    await waitFor(() => expect(result.current.loading).toBe(false));

    // Browsing all years carries no year at all.
    expect(getMessages).toHaveBeenNthCalledWith(
      1,
      7,
      { offset: 0, limit: 50 },
      expect.objectContaining({ signal: expect.anything() }),
    );

    act(() => result.current.selectYear(2020));
    await waitFor(() => expect(result.current.messages.map((m) => m.id)).toEqual([1, 2, 3]));

    // The year is a `year=` parameter on the conversation's own messages
    // route, asked for a whole page at a time, and paged until the vault's
    // total is covered — not a `date:2020` term in a search query.
    expect(getMessages).toHaveBeenNthCalledWith(
      2,
      7,
      { offset: 0, limit: 500, year: 2020 },
      expect.objectContaining({ signal: expect.anything() }),
    );
    expect(getMessages).toHaveBeenNthCalledWith(
      3,
      7,
      { offset: 2, limit: 500, year: 2020 },
      expect.objectContaining({ signal: expect.anything() }),
    );
    expect(getMessages).toHaveBeenCalledTimes(3);
    expect(result.current.total).toBe(3);
  });

  it("resets offset, activeYear, findTerm and activeMatch when the conversation changes", async () => {
    getMessages.mockResolvedValue(page([message(1)]));

    const { result, rerender } = renderHook(
      ({ id }: { id: number }) => useConversationMessages(id),
      { initialProps: { id: 1 }, wrapper: VaultProviders },
    );
    await waitFor(() => expect(result.current.loading).toBe(false));

    // Drive every reset-target away from its default before switching.
    act(() => result.current.selectYear(2020));
    act(() => result.current.fetchConversationPage(50));
    act(() => result.current.setFindTerm("hello"));
    act(() => result.current.setActiveMatch(3));

    expect(result.current.activeYear).toBe(2020);
    expect(result.current.offset).toBe(50);
    expect(result.current.findTerm).toBe("hello");
    expect(result.current.activeMatch).toBe(3);

    rerender({ id: 2 });

    expect(result.current.activeYear).toBeNull();
    expect(result.current.offset).toBe(0);
    expect(result.current.findTerm).toBe("");
    expect(result.current.activeMatch).toBe(0);
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
