import { describe, expect, it } from "vitest";
import type { SearchField } from "./searchFields";
import { applySuggestionToQuery, buildSearchSuggestions } from "./useSearchSuggestions";

const fields: SearchField[] = [
  { word: "with", value_type: "person", values: [], help: "", example: "" },
  { word: "tag", value_type: "name", values: ["none"], help: "", example: "" },
  { word: "kind", value_type: "choice", values: ["direct", "group"], help: "", example: "" },
];

describe("buildSearchSuggestions", () => {
  it("completes a bare prefix to the words the list has", () => {
    const out = buildSearchSuggestions({
      completingValue: false,
      personOp: false,
      lastToken: "t",
      fields: [...fields],
      contacts: [],
    });
    expect(out.map((s) => s.insert)).toEqual(["tag:"]);
  });
  it("offers a choice word's values after the colon", () => {
    const out = buildSearchSuggestions({
      completingValue: true,
      personOp: false,
      lastToken: "kind:",
      fields: [...fields],
      contacts: [],
    });
    expect(out.map((s) => s.insert)).toEqual(["kind:direct ", "kind:group "]);
  });
  it("offers contacts by id for a person word", () => {
    const out = buildSearchSuggestions({
      completingValue: true,
      personOp: true,
      lastToken: "with:ja",
      fields: [...fields],
      contacts: [{ id: "42", name: "Jane Doe" }],
    });
    expect(out[0].insert).toBe("with:#42 ");
    expect(out[0].label).toBe("Jane Doe");
  });
});

describe("applySuggestionToQuery", () => {
  it("replaces the token being typed", () => {
    expect(applySuggestionToQuery("hello ta", { id: "tag", label: "tag:", insert: "tag:" })).toBe(
      "hello tag:",
    );
  });
});
