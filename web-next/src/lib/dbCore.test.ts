import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { displayName, sortFields, splitNameParts } from "./dbCore";

describe("splitNameParts", () => {
  it("splits on the first space", () => {
    assert.deepEqual(splitNameParts("Ada Lovelace"), {
      first: "Ada",
      last: "Lovelace",
    });
    assert.deepEqual(splitNameParts("Mary Ann Smith"), {
      first: "Mary",
      last: "Ann Smith",
    });
  });

  it("uses the whole string for both parts when there is no space", () => {
    assert.deepEqual(splitNameParts("Madonna"), {
      first: "Madonna",
      last: "Madonna",
    });
  });

  it("handles empty input", () => {
    assert.deepEqual(splitNameParts(""), { first: "", last: "" });
    assert.deepEqual(splitNameParts(null), { first: "", last: "" });
  });
});

describe("sortFields", () => {
  it("derives first/last sort keys from preferred_name", () => {
    const sorts = sortFields({
      preferred_name: "Ann Lee",
      preferred_handle: "+15555550100",
    });
    assert.equal(sorts.sortFirst, "Ann");
    assert.equal(sorts.sortLast, "Lee");
    assert.equal(sorts.letter, "L");
  });

  it("falls back to handle when preferred is empty", () => {
    const sorts = sortFields({
      preferred_name: null,
      preferred_handle: "+15555550100",
    });
    assert.equal(sorts.sortFirst, "+15555550100");
    assert.equal(sorts.sortLast, "+15555550100");
  });
});

describe("displayName", () => {
  it("prefers preferred_name then handle", () => {
    assert.equal(
      displayName({
        preferred_name: "Ann Lee",
        preferred_handle: "+15555550100",
      }),
      "Ann Lee",
    );
    const byHandle = displayName({
      preferred_name: null,
      preferred_handle: "+15555550100",
    });
    assert.ok(byHandle.includes("555"));
    assert.notEqual(byHandle, "Unknown");
  });
});
