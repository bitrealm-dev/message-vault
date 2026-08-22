import { describe, expect, it } from "vitest";
import {
  buildContactsQuery,
  buildMessagesQuery,
  canSubmitContacts,
  canSubmitMessages,
  composeCountComparison,
  EMPTY_COUNT,
  EMPTY_DATE_BOUND,
  pushDateBoundTokens,
} from "./buildAdvancedQuery.ts";

describe("composeCountComparison", () => {
  it("returns null for any or non-numeric", () => {
    expect(composeCountComparison({ comparator: "any", value: "3" })).toBeNull();
    expect(composeCountComparison({ comparator: "=", value: "" })).toBeNull();
    expect(composeCountComparison({ comparator: ">", value: "x" })).toBeNull();
  });

  it("formats comparator and digits", () => {
    expect(composeCountComparison({ comparator: "=", value: "2" })).toBe("=2");
    expect(composeCountComparison({ comparator: "<", value: " 10 " })).toBe("<10");
  });
});

describe("pushDateBoundTokens", () => {
  it("emits nothing for any", () => {
    const parts: string[] = [];
    pushDateBoundTokens((s) => parts.push(s), "first-contact", EMPTY_DATE_BOUND);
    expect(parts).toEqual([]);
  });

  it("emits after/before/between tokens", () => {
    const after: string[] = [];
    pushDateBoundTokens((s) => after.push(s), "first-contact", {
      op: "after",
      start: "2020-01-01",
      end: "",
    });
    expect(after).toEqual(["first-contact:>=2020-01-01"]);

    const before: string[] = [];
    pushDateBoundTokens((s) => before.push(s), "last-contact", {
      op: "before",
      start: "2021-06-01",
      end: "",
    });
    expect(before).toEqual(["last-contact:<2021-06-01"]);

    const between: string[] = [];
    pushDateBoundTokens((s) => between.push(s), "first-contact", {
      op: "between",
      start: "2020-01-01",
      end: "2021-01-01",
    });
    expect(between).toEqual(["first-contact:>=2020-01-01", "first-contact:<2021-01-01"]);
  });
});

describe("buildMessagesQuery", () => {
  it("returns empty for blank fields", () => {
    expect(
      buildMessagesQuery({
        nameOrHandle: "",
        handle: "",
        msgType: "all",
        participants: EMPTY_COUNT,
      }),
    ).toBe("");
    expect(
      canSubmitMessages({
        nameOrHandle: "",
        handle: "",
        msgType: "all",
        participants: EMPTY_COUNT,
      }),
    ).toBe(false);
  });

  it("assembles name, handle, type, and participants tokens", () => {
    expect(
      buildMessagesQuery({
        nameOrHandle: " Pat ",
        handle: " +1555 ",
        msgType: "group",
        participants: { comparator: ">", value: "3" },
      }),
    ).toBe("Pat handle:+1555 is:group participants:>3");
  });
});

describe("buildContactsQuery", () => {
  it("always includes search:contacts even when other fields empty", () => {
    expect(
      buildContactsQuery({
        contactName: "",
        handle: "",
        firstMsgBound: EMPTY_DATE_BOUND,
        lastMsgBound: EMPTY_DATE_BOUND,
        activity: "any",
        noPreferredName: false,
        noHandle: false,
        services: [],
      }),
    ).toBe("search:contacts");
    expect(
      canSubmitContacts({
        contactName: "",
        handle: "",
        firstMsgBound: EMPTY_DATE_BOUND,
        lastMsgBound: EMPTY_DATE_BOUND,
        activity: "any",
        noPreferredName: false,
        noHandle: false,
        services: [],
      }),
    ).toBe(false);
  });

  it("assembles contact tokens", () => {
    expect(
      buildContactsQuery({
        contactName: "Lee",
        handle: "+1",
        firstMsgBound: { op: "after", start: "2020-01-01", end: "" },
        lastMsgBound: EMPTY_DATE_BOUND,
        activity: "messages",
        noPreferredName: true,
        noHandle: false,
        services: ["phone"],
      }),
    ).toBe(
      'Lee handle:"+1" first-contact:>=2020-01-01 has:messages has:no-name service:phone search:contacts',
    );
  });
});
