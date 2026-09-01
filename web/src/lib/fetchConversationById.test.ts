import { beforeEach, describe, expect, it, vi } from "vitest";
import { fetchConversationById } from "./fetchConversationById";
import type { Conversation } from "./types";
import { listConversations } from "./vaultApi";

vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  listConversations: vi.fn(),
}));

const get = vi.mocked(listConversations);

function conv(id: string): Conversation {
  return {
    id,
    participants: [],
    message_count: 1,
    last_message_at: "",
    date_range_start: null,
    date_range_end: null,
    service: "sms",
    is_group: false,
    label: `Chat ${id}`,
  };
}

describe("fetchConversationById", () => {
  beforeEach(() => {
    get.mockReset();
  });

  it("returns a match on the first page", async () => {
    get.mockResolvedValueOnce({
      conversations: [conv("1"), conv("2")],
      total: 2,
      limit: 100,
      offset: 0,
    });
    await expect(fetchConversationById("2")).resolves.toEqual(conv("2"));
    expect(get).toHaveBeenCalledTimes(1);
  });

  it("scans a later page when not on the first", async () => {
    get
      .mockResolvedValueOnce({
        conversations: [conv("1")],
        total: 101,
        limit: 100,
        offset: 0,
      })
      .mockResolvedValueOnce({
        conversations: [conv("99")],
        total: 101,
        limit: 100,
        offset: 100,
      });
    await expect(fetchConversationById("99")).resolves.toEqual(conv("99"));
    expect(get).toHaveBeenCalledTimes(2);
  });

  it("returns null when the id is missing", async () => {
    get.mockResolvedValueOnce({
      conversations: [conv("1")],
      total: 1,
      limit: 100,
      offset: 0,
    });
    await expect(fetchConversationById("missing")).resolves.toBeNull();
  });

  it("forwards AbortSignal to listConversations", async () => {
    const controller = new AbortController();
    get.mockResolvedValueOnce({
      conversations: [conv("1")],
      total: 1,
      limit: 100,
      offset: 0,
    });
    await fetchConversationById("1", controller.signal);
    expect(get.mock.calls[0]?.[1]).toEqual({ signal: controller.signal });
  });
});
