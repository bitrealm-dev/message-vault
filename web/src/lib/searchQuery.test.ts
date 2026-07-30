import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  composeSearchQuery,
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
      'from:alice with:bob has:attachment after:2020-01-01 before:2021 source:imessage is:group label:Family in:trash',
    );
    assert.equal(q.from, "alice");
    assert.equal(q.to, "bob");
    assert.equal(q.hasAttachment, true);
    assert.equal(q.after, "2020-01-01");
    assert.equal(q.before, "2021-01-01");
    assert.equal(q.source, "imessage");
    assert.equal(q.conversationType, "group");
    assert.equal(q.label, "Family");
    assert.equal(q.includeTrash, true);
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

  it("parses last-contact: and first-contact: durations", () => {
    assert.equal(parseSearchQuery("last-contact:1y").lastContactDays, 365);
    assert.equal(parseSearchQuery("last-contact:6m").lastContactDays, 180);
    assert.equal(parseSearchQuery("last-contact:400d").lastContactDays, 400);
    assert.equal(parseSearchQuery("last-contact:45").lastContactDays, 45);
    assert.equal(parseSearchQuery("last-contact:abc").lastContactDays, null);
    assert.equal(parseSearchQuery("first-contact:5y").firstContactDays, 1825);
    assert.equal(parseSearchQuery("first-contact:30d").firstContactDays, 30);
    assert.equal(hasSearchCriteria(parseSearchQuery("last-contact:1y")), true);
    assert.equal(hasSearchCriteria(parseSearchQuery("first-contact:5y")), true);
  });
});

describe("composeSearchQuery", () => {
  it("round-trips advanced form fields into operators", () => {
    const s = composeSearchQuery({
      withPerson: "Ann Lee",
      hasWords: "birthday",
      doesntHave: "spam",
      conversationType: "group",
      hasAttachment: true,
      includeTrash: true,
      lastContact: "1y",
      firstContact: "5y",
    });
    assert.match(s, /with:"Ann Lee"/);
    assert.match(s, /birthday/);
    assert.match(s, /-spam/);
    assert.match(s, /is:group/);
    assert.match(s, /has:attachment/);
    assert.match(s, /in:trash/);
    assert.match(s, /last-contact:1y/);
    assert.match(s, /first-contact:5y/);
    assert.doesNotMatch(s, /subject:/);
    assert.doesNotMatch(s, /from:/);
    const parsed = parseSearchQuery(s);
    assert.equal(parsed.to, "Ann Lee");
    assert.deepEqual(parsed.terms, ["birthday"]);
    assert.deepEqual(parsed.exclude, ["spam"]);
    assert.equal(parsed.conversationType, "group");
    assert.equal(parsed.hasAttachment, true);
    assert.equal(parsed.includeTrash, true);
    assert.equal(parsed.lastContactDays, 365);
    assert.equal(parsed.firstContactDays, 1825);
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
