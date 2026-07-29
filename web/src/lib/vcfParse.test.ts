import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  cardToDraft,
  parseVcfText,
  splitCategories,
} from "./vcfParse";

describe("splitCategories", () => {
  it("splits on commas and unescapes", () => {
    assert.deepEqual(splitCategories(String.raw`Family,Work\,Inc,Friends`), [
      "Family",
      "Work,Inc",
      "Friends",
    ]);
  });

  it("trims empty tokens", () => {
    assert.deepEqual(splitCategories("  Family  ,  "), ["Family"]);
    assert.deepEqual(splitCategories(""), []);
  });
});

describe("parseVcfText CATEGORIES", () => {
  it("collects repeated CATEGORIES and dedupes case-insensitively", () => {
    const cards = parseVcfText(`BEGIN:VCARD
VERSION:3.0
FN:Ada Lovelace
N:Lovelace;Ada;;;
TEL:+15551234567
CATEGORIES:Family,Friends
CATEGORIES:Work
CATEGORIES:family
END:VCARD
`);
    assert.equal(cards.length, 1);
    assert.deepEqual(cards[0]!.categories, ["Family", "Friends", "Work"]);
  });

  it("merges bracket FN tags with CATEGORIES in cardToDraft", () => {
    const cards = parseVcfText(`BEGIN:VCARD
VERSION:3.0
FN:Mom [Kin]
N:;;;;
TEL:+15557654321
CATEGORIES:Family,People
END:VCARD
`);
    const draft = cardToDraft(cards[0]!);
    assert.equal(draft.firstName, "Mom");
    assert.deepEqual(draft.labels, ["Kin", "Family"]);
  });
});
