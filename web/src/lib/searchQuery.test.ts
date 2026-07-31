import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  composeSearchQuery,
  formFromSearchQuery,
  hasSearchCriteria,
  parseSearchQuery,
  toFtsMatch,
} from "./searchQuery";

describe("parseSearchQuery", () => {
  it("parses free-text terms and phrases", () => {
    const q = parseSearchQuery('hello "exact phrase" world');
    assert.deepEqual(q.terms, ["hello", "world"]);
    assert.deepEqual(q.phrases, ["exact phrase"]);
  });

  it("parses operators", () => {
    const q = parseSearchQuery(
      "from:alice with:bob has:attachment after:2020-01-01 before:2021 source:imessage is:group",
    );
    assert.equal(q.from, "alice");
    assert.equal(q.to, "bob");
    assert.equal(q.hasAttachment, true);
    assert.equal(q.after, "2020-01-01");
    assert.equal(q.before, "2021-01-01");
    assert.equal(q.source, "imessage");
    assert.equal(q.conversationType, "group");
    assert.equal(q.mode, "messages");
  });

  it("treats label: as an alias for within: and ignores in:trash", () => {
    assert.equal(parseSearchQuery("label:Work").within, "Work");
    const q = parseSearchQuery("in:trash hello");
    assert.deepEqual(q.terms, ["hello"]);
  });

  it("treats with: and to: as the same participant filter", () => {
    assert.equal(parseSearchQuery("with:sam").to, "sam");
    assert.equal(parseSearchQuery("to:sam").to, "sam");
  });

  it("parses negation", () => {
    const q = parseSearchQuery('party -cake -"bad word"');
    assert.deepEqual(q.terms, ["party"]);
    assert.deepEqual(q.exclude, ["cake", "bad word"]);
  });

  it("treats is:direct as individual", () => {
    const q = parseSearchQuery("is:direct");
    assert.equal(q.conversationType, "individual");
  });

  it("reads the three first-contact / last-contact bound forms", () => {
    assert.deepEqual(parseSearchQuery("first-contact:>=2020-01-01").firstContact, {
      from: "2020-01-01",
      to: null,
    });
    assert.deepEqual(parseSearchQuery("first-contact:<2020-01-01").firstContact, {
      from: null,
      to: "2020-01-01",
    });
    assert.deepEqual(
      parseSearchQuery("first-contact:2020-01-01..2020-06-30").firstContact,
      { from: "2020-01-01", to: "2020-06-30" },
    );
    assert.deepEqual(parseSearchQuery("last-contact:>=2024-01-15").lastContact, {
      from: "2024-01-15",
      to: null,
    });
  });

  it("keeps bare dates meaning 'before' for older URLs", () => {
    assert.deepEqual(parseSearchQuery("last-contact:2024-01-15").lastContact, {
      from: null,
      to: "2024-01-15",
    });
    assert.deepEqual(parseSearchQuery("first-contact:2015").firstContact, {
      from: null,
      to: "2015-01-01",
    });
    assert.equal(
      hasSearchCriteria(parseSearchQuery("last-contact:2024-01-01")),
      true,
    );
    assert.equal(
      hasSearchCriteria(parseSearchQuery("first-contact:2019-06-01")),
      true,
    );
  });

  it("defaults legacy queries to messages and retires show:contact", () => {
    assert.equal(parseSearchQuery("hello").mode, "messages");
    assert.equal(parseSearchQuery("show:contact").mode, "messages");
    assert.equal(parseSearchQuery("show:contact").showContact, false);
    assert.equal(hasSearchCriteria(parseSearchQuery("show:contact")), false);
  });

  it("parses explicit contact mode and contact operators", () => {
    const q = parseSearchQuery(
      'search:contacts within:"Close Friends" handle:"Ann Lee" group-count:>=2 message-count:<100',
    );
    assert.equal(q.mode, "contacts");
    assert.equal(q.within, "Close Friends");
    assert.equal(q.handle, "Ann Lee");
    assert.deepEqual(q.groupCount, { comparator: ">=", value: 2 });
    assert.deepEqual(q.messageCount, { comparator: "<", value: 100 });
    assert.equal(hasSearchCriteria(q), true);
  });

  it("ignores invalid count comparisons", () => {
    assert.equal(parseSearchQuery("group-count:2").groupCount, null);
    assert.equal(parseSearchQuery("message-count:>=-1").messageCount, null);
    assert.equal(parseSearchQuery("message-count:>1.5").messageCount, null);
  });
});

describe("composeSearchQuery", () => {
  it("round-trips message form fields into operators", () => {
    const s = composeSearchQuery({
      mode: "messages",
      within: "Close Friends",
      withPerson: "Ann Lee",
      hasWords: "birthday",
      doesntHave: "spam",
      conversationType: "group",
      hasAttachment: true,
    });
    assert.match(s, /with:"Ann Lee"/);
    assert.match(s, /within:"Close Friends"/);
    assert.match(s, /birthday/);
    assert.match(s, /-spam/);
    assert.match(s, /is:group/);
    assert.match(s, /has:attachment/);
    assert.doesNotMatch(s, /in:trash/);
    assert.doesNotMatch(s, /subject:/);
    assert.doesNotMatch(s, /from:/);

    const parsed = parseSearchQuery(s);
    assert.equal(parsed.to, "Ann Lee");
    assert.equal(parsed.within, "Close Friends");
    assert.deepEqual(parsed.terms, ["birthday"]);
    assert.deepEqual(parsed.exclude, ["spam"]);
    assert.equal(parsed.conversationType, "group");
    assert.equal(parsed.hasAttachment, true);
  });

  it("turns a Date range into after: plus before:", () => {
    const s = composeSearchQuery({
      date: { mode: "between", from: "2020-01-01", to: "2020-03-01" },
    });
    assert.equal(s, "after:2020-01-01 before:2020-03-01");
    const parsed = parseSearchQuery(s);
    assert.equal(parsed.after, "2020-01-01");
    assert.equal(parsed.before, "2020-03-01");
  });

  it("emits only the filled side of a date range", () => {
    assert.equal(
      composeSearchQuery({ date: { mode: "on-or-after", from: "2020-01-01" } }),
      "after:2020-01-01",
    );
    assert.equal(
      composeSearchQuery({ date: { mode: "before", to: "2020-01-01" } }),
      "before:2020-01-01",
    );
    assert.equal(
      composeSearchQuery({
        mode: "contacts",
        firstContact: { mode: "between", from: "2020-01-01" },
      }),
      "search:contacts first-contact:>=2020-01-01",
    );
  });

  it("omits date fields left on Any time", () => {
    assert.equal(
      composeSearchQuery({
        date: { mode: "any", from: "2020-01-01", to: "2020-03-01" },
        firstContact: { mode: "any" },
        lastContact: { mode: "any" },
      }),
      "",
    );
  });

  it("composes only contact fields in contact mode", () => {
    const s = composeSearchQuery({
      mode: "contacts",
      within: "Close Friends",
      handle: "Ann Lee",
      firstContact: { mode: "on-or-after", from: "2019-06-01" },
      lastContact: { mode: "before", to: "2024-01-15" },
      groupCount: { comparator: ">=", value: "2" },
      messageCount: { comparator: "<", value: "100" },
      withPerson: "ignored",
      hasWords: "ignored",
      hasAttachment: true,
    });
    assert.equal(
      s,
      'search:contacts within:"Close Friends" handle:"Ann Lee" first-contact:>=2019-06-01 last-contact:<2024-01-15 group-count:>=2 message-count:<100',
    );
  });

  it("emits shared Within but not contact-only fields in message mode", () => {
    assert.equal(
      composeSearchQuery({
        mode: "messages",
        within: "Family",
        handle: "Ann",
        groupCount: { comparator: ">", value: "2" },
        hasWords: "hello",
      }),
      "within:Family hello",
    );
  });
});

describe("formFromSearchQuery", () => {
  it("hydrates Date and First contact from the query bar", () => {
    const form = formFromSearchQuery(
      "after:2000-01-01 first-contact:<2020-10-10",
    );
    assert.deepEqual(form.date, {
      mode: "on-or-after",
      from: "2000-01-01",
      to: "",
    });
    assert.deepEqual(form.firstContact, {
      mode: "before",
      from: "",
      to: "2020-10-10",
    });
    assert.deepEqual(form.lastContact, { mode: "any", from: "", to: "" });
  });

  it("round-trips compose → form for message fields", () => {
    const composed = composeSearchQuery({
      mode: "messages",
      withPerson: "Ann Lee",
      hasWords: 'birthday "exact phrase"',
      doesntHave: "spam",
      conversationType: "group",
      hasAttachment: true,
      source: "imessage",
      date: { mode: "between", from: "2020-01-01", to: "2020-03-01" },
    });
    const form = formFromSearchQuery(composed);
    assert.equal(form.mode, "messages");
    assert.equal(form.withPerson, "Ann Lee");
    assert.equal(form.hasWords, 'birthday "exact phrase"');
    assert.equal(form.doesntHave, "spam");
    assert.equal(form.conversationType, "group");
    assert.equal(form.hasAttachment, true);
    assert.equal(form.source, "imessage");
    assert.deepEqual(form.date, {
      mode: "between",
      from: "2020-01-01",
      to: "2020-03-01",
    });
  });

  it("maps is:direct and plain free text", () => {
    const form = formFromSearchQuery('hello is:direct -"bad word"');
    assert.equal(form.hasWords, "hello");
    assert.equal(form.doesntHave, '"bad word"');
    assert.equal(form.conversationType, "individual");
  });

  it("hydrates contact form mode and count fields", () => {
    const form = formFromSearchQuery(
      'search:contacts handle:"Ann Lee" group-count:>=2 message-count:=12',
    );
    assert.equal(form.mode, "contacts");
    assert.equal(form.handle, "Ann Lee");
    assert.deepEqual(form.groupCount, { comparator: ">=", value: "2" });
    assert.deepEqual(form.messageCount, { comparator: "=", value: "12" });
  });

  it("ignores from: and subject:", () => {
    const form = formFromSearchQuery("from:alice subject:hi with:bob");
    assert.equal(form.withPerson, "bob");
    assert.equal(form.hasWords, undefined);
  });
});

describe("toFtsMatch / hasSearchCriteria", () => {
  it("builds an FTS expression from terms and exclusions", () => {
    const q = parseSearchQuery('hello "exact" -nope');
    assert.equal(toFtsMatch(q), '"hello" AND "exact" AND NOT "nope"');
    assert.equal(hasSearchCriteria(q), true);
  });

  it("returns null when only metadata filters are set", () => {
    const q = parseSearchQuery("has:attachment is:group with:bob");
    assert.equal(toFtsMatch(q), null);
    assert.equal(hasSearchCriteria(q), true);
  });
});
