import { describe, expect, it } from "vitest";
import { isReservedTagName, reservedTagError, tagListQuery, tagSlug } from "./messageTags";

describe("tagSlug", () => {
  it("turns spaces into dashes and keeps letter case", () => {
    expect(tagSlug("Work Friends")).toBe("Work-Friends");
  });
});

describe("reserved tags", () => {
  it("blocks Threads and Tags", () => {
    expect(isReservedTagName("Threads")).toBe(true);
    expect(isReservedTagName("tag")).toBe(true);
    expect(reservedTagError("Trash")).toBe('"Trash" is a reserved tag');
    expect(isReservedTagName("Holiday")).toBe(false);
  });
});

describe("tagListQuery", () => {
  it("quotes names that contain spaces", () => {
    expect(tagListQuery("Holiday", "")).toBe("tag:Holiday");
    expect(tagListQuery("none", "")).toBe("tag:none");
    expect(tagListQuery("Work Friends", "ada")).toBe('tag:"Work Friends" ada');
  });
});
