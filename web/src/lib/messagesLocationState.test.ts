import { describe, expect, it } from "vitest";
import { asMessagesLocationState } from "./messagesLocationState.ts";

const conversation = {
  id: "42",
  participants: [],
  message_count: 1,
  last_message_at: "2026-01-01T00:00:00Z",
  date_range_start: null,
  date_range_end: null,
  service: "imessage",
  is_group: false,
  label: null,
};

describe("asMessagesLocationState", () => {
  it("accepts conversation and openContactId", () => {
    expect(
      asMessagesLocationState({
        conversation,
        openContactId: "c1",
      }),
    ).toEqual({ conversation, openContactId: "c1" });
  });

  it("accepts openContactId alone", () => {
    expect(asMessagesLocationState({ openContactId: "c1" })).toEqual({
      openContactId: "c1",
    });
  });

  it("rejects non-objects and invalid shapes", () => {
    expect(asMessagesLocationState(null)).toBeNull();
    expect(asMessagesLocationState("x")).toBeNull();
    expect(asMessagesLocationState({ conversation: { id: 1 } })).toBeNull();
    expect(asMessagesLocationState({ openContactId: 5 })).toBeNull();
    expect(asMessagesLocationState({})).toBeNull();
  });
});
