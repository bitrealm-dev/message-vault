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
    assert.equal(q.with, "bob");
    assert.equal(q.to, null);
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
    assert.equal(q.inConversation, null);
  });

  it("splits with: and to:; parses from:me and has:noattachment", () => {
    assert.equal(parseSearchQuery("with:sam").with, "sam");
    assert.equal(parseSearchQuery("to:sam").to, "sam");
    assert.equal(parseSearchQuery("from:me").from, "me");
    assert.equal(parseSearchQuery("has:noattachment").hasAttachment, false);
    assert.equal(parseSearchQuery("has:noatt").hasAttachment, false);
  });

  it("parses filename, filetype, text, in:, and relative dates", () => {
    const q = parseSearchQuery(
      'filename:invoice filetype:pdf text:hello in:"Family Chat" after:7d',
    );
    assert.equal(q.filename, "invoice");
    assert.equal(q.filetype, "document");
    assert.equal(q.text, "hello");
    assert.equal(q.inConversation, "Family Chat");
    assert.match(q.after ?? "", /^\d{4}-\d{2}-\d{2}$/);
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

  it("parses first/last/phone and is:nofirst / is:nolast", () => {
    const q = parseSearchQuery(
      'search:contacts first:Ann last:Lee phone:"+1555" is:nofirst is:nolast',
    );
    assert.equal(q.firstName, "Ann");
    assert.equal(q.lastName, "Lee");
    assert.equal(q.phone, "+1555");
    assert.equal(q.noFirstName, true);
    assert.equal(q.noLastName, true);
    assert.equal(hasSearchCriteria(q), true);
  });

  it("maps legacy is:nameless to nofirst and nolast", () => {
    const q = parseSearchQuery("search:contacts is:nameless");
    assert.equal(q.noFirstName, true);
    assert.equal(q.noLastName, true);
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
      fromPerson: "me",
      toPerson: "Sam",
      hasWords: "birthday",
      doesntHave: "spam",
      conversationType: "group",
      attachmentFilter: "yes",
      filetype: "image",
      filename: "photo",
    });
    assert.match(s, /with:"Ann Lee"/);
    assert.match(s, /from:me/);
    assert.match(s, /to:Sam/);
    assert.match(s, /within:"Close Friends"/);
    assert.match(s, /birthday/);
    assert.match(s, /-spam/);
    assert.match(s, /is:group/);
    assert.match(s, /has:attachment/);
    assert.match(s, /filetype:image/);
    assert.match(s, /filename:photo/);
    assert.doesNotMatch(s, /in:trash/);

    const parsed = parseSearchQuery(s);
    assert.equal(parsed.with, "Ann Lee");
    assert.equal(parsed.from, "me");
    assert.equal(parsed.to, "Sam");
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
      firstName: "Ann",
      lastName: "Lee",
      phone: "+1555",
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
      'search:contacts within:"Close Friends" first:Ann last:Lee phone:+1555 first-contact:>=2019-06-01 last-contact:<2024-01-15 group-count:>=2 message-count:<100',
    );
  });

  it("composes is:nofirst / is:nolast without first/last text", () => {
    assert.equal(
      composeSearchQuery({
        mode: "contacts",
        noFirstName: true,
        noLastName: true,
        firstName: "ignored",
        lastName: "ignored",
        phone: "555",
      }),
      "search:contacts is:nofirst is:nolast phone:555",
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

  it("composes with-person name fields in message mode", () => {
    assert.equal(
      composeSearchQuery({
        mode: "messages",
        firstName: "Ann",
        lastName: "Lee",
        phone: "555",
        hasWords: "hello",
      }),
      "first:Ann last:Lee phone:555 hello",
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
      fromPerson: "me",
      toPerson: "Sam",
      hasWords: 'birthday "exact phrase"',
      doesntHave: "spam",
      conversationType: "group",
      attachmentFilter: "yes",
      source: "imessage",
      date: { mode: "between", from: "2020-01-01", to: "2020-03-01" },
    });
    const form = formFromSearchQuery(composed);
    assert.equal(form.mode, "messages");
    assert.equal(form.withPerson, "Ann Lee");
    assert.equal(form.fromPerson, "me");
    assert.equal(form.toPerson, "Sam");
    // Exclusions stay in Has the words when boolean AST is used.
    assert.equal(form.hasWords, 'birthday "exact phrase" -spam');
    assert.equal(form.doesntHave, undefined);
    assert.equal(form.conversationType, "group");
    assert.equal(form.attachmentFilter, "yes");
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
    assert.equal(form.hasWords, 'hello -"bad word"');
    assert.equal(form.doesntHave, undefined);
    assert.equal(form.conversationType, "individual");
  });

  it("hydrates contact form mode and count fields", () => {
    const form = formFromSearchQuery(
      'search:contacts first:Ann last:Lee phone:555 group-count:>=2 message-count:=12',
    );
    assert.equal(form.mode, "contacts");
    assert.equal(form.firstName, "Ann");
    assert.equal(form.lastName, "Lee");
    assert.equal(form.phone, "555");
    assert.equal(form.handle, undefined);
    assert.deepEqual(form.groupCount, { comparator: ">=", value: "2" });
    assert.deepEqual(form.messageCount, { comparator: "=", value: "12" });
  });

  it("keeps handle: on the combined Handle field", () => {
    const name = formFromSearchQuery('search:contacts handle:"Ann Lee"');
    assert.equal(name.handle, "Ann Lee");
    assert.equal(name.firstName, undefined);
    assert.equal(name.phone, undefined);
    const phone = formFromSearchQuery("search:contacts handle:+15551212");
    assert.equal(phone.handle, "+15551212");
    assert.equal(phone.phone, undefined);
  });

  it("hydrates is:nofirst / is:nolast and legacy is:nameless", () => {
    const form = formFromSearchQuery("search:contacts is:nofirst is:nolast");
    assert.equal(form.noFirstName, true);
    assert.equal(form.noLastName, true);
    const legacy = formFromSearchQuery("search:contacts is:nameless");
    assert.equal(legacy.noFirstName, true);
    assert.equal(legacy.noLastName, true);
  });

  it("hydrates from:, to:, subject:, and with:", () => {
    const form = formFromSearchQuery(
      'from:alice to:me subject:hi with:bob has:noattachment',
    );
    assert.equal(form.fromPerson, "alice");
    assert.equal(form.toPerson, "me");
    assert.equal(form.subject, "hi");
    assert.equal(form.withPerson, "bob");
    assert.equal(form.attachmentFilter, "no");
    assert.equal(form.hasWords, undefined);
  });
});

describe("toFtsMatch / hasSearchCriteria", () => {
  it("builds an FTS expression from terms and exclusions", () => {
    const q = parseSearchQuery('hello "exact" -nope');
    assert.equal(toFtsMatch(q), '"hello" AND "exact" AND NOT "nope"');
    assert.equal(hasSearchCriteria(q), true);
  });

  it("compiles OR, grouping, and prefix*", () => {
    assert.equal(toFtsMatch(parseSearchQuery("cat OR dog")), '("cat" OR "dog")');
    assert.equal(
      toFtsMatch(parseSearchQuery("(hello OR world) party")),
      '("hello" OR "world") AND "party"',
    );
    assert.equal(toFtsMatch(parseSearchQuery("avoc*")), "avoc*");
    assert.equal(
      toFtsMatch(parseSearchQuery("avoc* OR pine*")),
      "(avoc* OR pine*)",
    );
    // AND binds tighter than OR
    assert.equal(
      toFtsMatch(parseSearchQuery("hello OR world party")),
      '("hello" OR ("world" AND "party"))',
    );
  });

  it("keeps operators outside the FTS expression", () => {
    const q = parseSearchQuery("from:me (cat OR dog) has:attachment");
    assert.equal(q.from, "me");
    assert.equal(q.hasAttachment, true);
    assert.equal(toFtsMatch(q), '("cat" OR "dog")');
  });

  it("returns null when only metadata filters are set", () => {
    const q = parseSearchQuery("has:attachment is:group with:bob");
    assert.equal(toFtsMatch(q), null);
    assert.equal(hasSearchCriteria(q), true);
  });
});
