import { describe, expect, it } from "vitest";
import {
  groupFromSlug,
  groupListQuery,
  groupSlug,
  isReservedGroupName,
  reservedGroupError,
} from "./contactGroups";

describe("groupSlug", () => {
  it("turns spaces and punctuation into dashes and keeps letter case", () => {
    expect(groupSlug("Work Friends")).toBe("Work-Friends");
    expect(groupSlug("  Family  ")).toBe("Family");
    expect(groupSlug("reGroup")).toBe("reGroup");
  });
});

describe("groupFromSlug", () => {
  it("prefers an exact slug match, then ignores case", () => {
    expect(groupFromSlug("Work-Friends", ["Work Friends", "Family"])).toBe(
      "Work Friends",
    );
    expect(groupFromSlug("family", ["Family"])).toBe("Family");
    expect(groupFromSlug("missing", ["Family"])).toBeNull();
  });
});

describe("reserved groups", () => {
  it("blocks Contacts, Trash, and No group", () => {
    expect(isReservedGroupName("Contacts")).toBe(true);
    expect(isReservedGroupName("no group")).toBe(true);
    expect(reservedGroupError("Trash")).toBe("Trash is a reserved group");
    expect(isReservedGroupName("Family")).toBe(false);
  });
});

describe("groupListQuery", () => {
  it("quotes names that contain spaces and keeps typed search", () => {
    expect(groupListQuery("Family", "")).toBe("group:Family");
    expect(groupListQuery("Work Friends", "ada")).toBe('group:"Work Friends" ada');
    expect(groupListQuery("none", "bob")).toBe("group:none bob");
    expect(groupListQuery(null, "ada")).toBe("ada");
  });
});
