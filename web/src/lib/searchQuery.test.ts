import { describe, expect, it } from "vitest";
import {
  advancedContacts,
  advancedMessages,
  forContact,
  forGroup,
  forHandle,
  forTag,
  quote,
  suggestion,
  trashed,
  withKind,
} from "./searchQuery";

describe("quote", () => {
  it("returns a plain word bare", () => {
    expect(quote("Family")).toBe("Family");
  });

  it("quotes a value with a space", () => {
    expect(quote("Book Club")).toBe('"Book Club"');
  });

  it("quotes a value with parentheses, since the language reads them as grouping", () => {
    expect(quote("Family (close)")).toBe('"Family (close)"');
    expect(quote("(x")).toBe('"(x"');
    expect(quote("x)")).toBe('"x)"');
  });

  it("quotes a value with an embedded quote and escapes it by doubling", () => {
    expect(quote('say "hi"')).toBe('"say ""hi"""');
  });

  it("round-trips an empty string to something the language accepts", () => {
    expect(quote("")).toBe('""');
  });
});

describe("forGroup", () => {
  it("builds a bare group term", () => {
    expect(forGroup("Family")).toBe("group:Family");
  });

  it("quotes a group name that needs it", () => {
    expect(forGroup("Family (close)")).toBe('group:"Family (close)"');
  });
});

describe("forTag", () => {
  it("builds a bare tag term", () => {
    expect(forTag("Work")).toBe("tag:Work");
  });

  it("quotes a tag name that needs it", () => {
    expect(forTag("Book Club")).toBe('tag:"Book Club"');
  });
});

describe("forHandle", () => {
  it("trims and builds a bare handle term", () => {
    expect(forHandle("  ann@example.com  ")).toBe("handle:ann@example.com");
  });

  it("quotes a handle with a space", () => {
    expect(forHandle("Ann Lee")).toBe('handle:"Ann Lee"');
  });

  it("produces the same text on a messages screen and a contacts screen", () => {
    const messages = advancedMessages({
      nameOrHandle: "",
      handle: "Ann Lee",
      msgType: "all",
      participants: { comparator: "any", value: "" },
    });
    const contacts = advancedContacts({
      contactName: "",
      handle: "Ann Lee",
      firstMsgBound: { op: "any", start: "", end: "" },
      lastMsgBound: { op: "any", start: "", end: "" },
      activity: "any",
      noPreferredName: false,
      noHandle: false,
      services: [],
    });
    expect(messages).toBe(forHandle("Ann Lee"));
    expect(contacts).toBe(forHandle("Ann Lee"));
  });
});

describe("forContact", () => {
  it("builds a with:#id term", () => {
    expect(forContact("42")).toBe("with:#42");
  });
});

describe("withKind", () => {
  it("returns the query unchanged for all, rather than appending an empty term", () => {
    expect(withKind("q", "all")).toBe("q");
    expect(withKind("", "all")).toBe("");
  });

  it("appends kind:direct or kind:group to a non-empty query", () => {
    expect(withKind("q", "direct")).toBe("q kind:direct");
    expect(withKind("q", "group")).toBe("q kind:group");
  });

  it("builds a bare kind term when the query is empty", () => {
    expect(withKind("", "group")).toBe("kind:group");
  });
});

describe("trashed", () => {
  it("is bare trashed:yes with no search", () => {
    expect(trashed("")).toBe("trashed:yes");
    expect(trashed("   ")).toBe("trashed:yes");
  });

  it("appends a trimmed search term", () => {
    expect(trashed("  ada  ")).toBe("trashed:yes ada");
  });
});

describe("suggestion", () => {
  it("builds a bare word:value term", () => {
    expect(suggestion("tag", "Work")).toBe("tag:Work");
  });

  it("quotes a value that needs it", () => {
    expect(suggestion("tag", "Book Club")).toBe('tag:"Book Club"');
  });
});

describe("advancedMessages", () => {
  it("joins only the terms the person filled in", () => {
    expect(
      advancedMessages({
        nameOrHandle: "",
        handle: "",
        msgType: "all",
        participants: { comparator: "any", value: "" },
      }),
    ).toBe("");
  });

  it("builds one term per filled field, in order, quoting the handle", () => {
    expect(
      advancedMessages({
        nameOrHandle: "ada",
        handle: "Ann Lee",
        msgType: "direct",
        participants: { comparator: ">", value: "3" },
      }),
    ).toBe('ada handle:"Ann Lee" kind:direct participants:>3');
  });

  it("drops a participants comparison that is not a whole number", () => {
    expect(
      advancedMessages({
        nameOrHandle: "",
        handle: "",
        msgType: "all",
        participants: { comparator: ">", value: "abc" },
      }),
    ).toBe("");
  });
});

describe("advancedContacts", () => {
  it("joins only the terms the person filled in", () => {
    expect(
      advancedContacts({
        contactName: "",
        handle: "",
        firstMsgBound: { op: "any", start: "", end: "" },
        lastMsgBound: { op: "any", start: "", end: "" },
        activity: "any",
        noPreferredName: false,
        noHandle: false,
        services: [],
      }),
    ).toBe("");
  });

  it("builds date-bound, activity, and none terms", () => {
    expect(
      advancedContacts({
        contactName: "ada",
        handle: "",
        firstMsgBound: { op: "after", start: "2020-01-01", end: "" },
        lastMsgBound: { op: "between", start: "2021-01-01", end: "2021-06-01" },
        activity: "messages",
        noPreferredName: true,
        noHandle: false,
        services: ["imessage", "sms"],
      }),
    ).toBe(
      "ada first-message:>=2020-01-01 last-message:2021-01-01..2021-06-01 messages:>0 name:none service:imessage,sms",
    );
  });

  it("quotes a handle with a space instead of always quoting it", () => {
    expect(
      advancedContacts({
        contactName: "",
        handle: "ann@example.com",
        firstMsgBound: { op: "any", start: "", end: "" },
        lastMsgBound: { op: "any", start: "", end: "" },
        activity: "any",
        noPreferredName: false,
        noHandle: false,
        services: [],
      }),
    ).toBe("handle:ann@example.com");
  });
});
