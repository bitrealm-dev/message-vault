/** @vitest-environment jsdom */
import { beforeEach, describe, expect, it } from "vitest";
import {
  bubbleBody,
  formatMessageTime,
  isGroupConversation,
  senderName,
} from "../components/messages/chatBubbleShared";
import { USE_NAME_ALIASES_KEY } from "./nameAliases";
import type { Message } from "./types";

function message(partial: Partial<Message> & Pick<Message, "conversation">): Message {
  return {
    id: "1",
    source: "imessage",
    service: "iMessage",
    guid: null,
    timestamp: "2026-08-11T15:04:00Z",
    timestamp_utc: "2026-08-11T15:04:00Z",
    is_from_me: false,
    sender: null,
    subject: null,
    text: "hi",
    attachments: [],
    tapbacks: [],
    ...partial,
  };
}

describe("formatMessageTime", () => {
  it("returns a non-empty locale string", () => {
    const s = formatMessageTime("2026-08-11T15:04:00Z");
    expect(s.length).toBeGreaterThan(0);
    expect(s).not.toBe("Invalid Date");
  });
});

describe("bubbleBody", () => {
  it("returns undefined for empty body", () => {
    expect(bubbleBody("", undefined)).toBeUndefined();
  });

  it("returns plain text without highlight", () => {
    expect(bubbleBody("hello", undefined)).toBe("hello");
  });

  it("returns highlighted nodes when a term is set", () => {
    const nodes = bubbleBody("hello world", "hello");
    expect(Array.isArray(nodes)).toBe(true);
    expect(nodes!.length).toBeGreaterThan(0);
  });
});

describe("senderName / isGroupConversation", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("labels outbound messages as Me", () => {
    const m = message({
      is_from_me: true,
      conversation: {
        id: "c1",
        chat_identifier: "x",
        conversation_type: "individual",
        group_title: null,
        participants: [{ handle: "+1", name_alias: null, preferred_name: "Ada", contact_id: null }],
      },
    });
    expect(senderName(m)).toBe("Me");
  });

  it("uses preferred name, then alias when aliases enabled", () => {
    const conversation = {
      id: "c1",
      chat_identifier: "x",
      conversation_type: "individual",
      group_title: null,
      participants: [
        {
          handle: "+1555",
          name_alias: "A.L.",
          preferred_name: "Ada",
          contact_id: null,
        },
      ],
    };
    const m = message({ sender: "+1555", conversation });
    expect(senderName(m)).toBe("Ada");
    window.localStorage.setItem(USE_NAME_ALIASES_KEY, "1");
    expect(senderName(m)).toBe("A.L.");
  });

  it("detects groups from type or participant count", () => {
    const one = message({
      conversation: {
        id: "c1",
        chat_identifier: "x",
        conversation_type: "individual",
        group_title: null,
        participants: [{ handle: "a", name_alias: null, preferred_name: null, contact_id: null }],
      },
    });
    expect(isGroupConversation(one)).toBe(false);

    const typed = message({
      conversation: {
        ...one.conversation,
        conversation_type: "group",
      },
    });
    expect(isGroupConversation(typed)).toBe(true);

    const many = message({
      conversation: {
        id: "c1",
        chat_identifier: "x",
        conversation_type: "individual",
        group_title: null,
        participants: [
          { handle: "a", name_alias: null, preferred_name: null, contact_id: null },
          { handle: "b", name_alias: null, preferred_name: null, contact_id: null },
        ],
      },
    });
    expect(isGroupConversation(many)).toBe(true);
  });
});
