import { describe, expect, it } from "vitest";
import {
  contactPreviewFromListRow,
  contactPreviewFromThreadParticipants,
  HANDLE_STUB_PLACEHOLDER,
  previewHandleStubRows,
  sameContactPreviews,
} from "./contactDrawerTypes";

describe("contactPreviewFromListRow", () => {
  it("maps list-API handle_count onto preview handleCount", () => {
    expect(
      contactPreviewFromListRow({
        id: "1",
        name: "Ada",
        handles: ["+15550001", "15550001"],
        handle_count: 1,
        groups: ["Family"],
      }),
    ).toEqual({
      id: "1",
      name: "Ada",
      handles: ["+15550001", "15550001"],
      handleCount: 1,
      groups: ["Family"],
    });
  });
});

describe("previewHandleStubRows", () => {
  it("stubs one row when preview lists raw and normalized forms of one identity", () => {
    const rows = previewHandleStubRows(["+1555000b", "1555000b"], 1);
    expect(rows.map((r) => r.handle)).toEqual(["+1555000b"]);
  });

  it("uses unique identities when handleCount is missing", () => {
    const rows = previewHandleStubRows(["+1555000b", "1555000b"], undefined);
    expect(rows.map((r) => r.handle)).toEqual(["+1555000b"]);
  });

  it("pads with a placeholder when handleCount is larger than the unique list", () => {
    const rows = previewHandleStubRows(["+1555000b"], 3);
    expect(rows.map((r) => r.handle)).toEqual([
      "+1555000b",
      HANDLE_STUB_PLACEHOLDER,
      HANDLE_STUB_PLACEHOLDER,
    ]);
  });

  it("returns no rows when handleCount is 0", () => {
    expect(previewHandleStubRows(["+1555000b"], 0)).toEqual([]);
  });

  it("keeps two distinct phones when handleCount is 2", () => {
    const rows = previewHandleStubRows(["+15550001", "15550001", "+15550002", "15550002"], 2);
    expect(rows.map((r) => r.handle)).toEqual(["+15550001", "+15550002"]);
  });
});

describe("contactPreviewFromThreadParticipants", () => {
  it("builds a preview from matching conversation participants", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        { contact_id: "c1", handle: "+15550001", name: "Ada" },
        { contact_id: "c2", handle: "+15550002", name: "Bob" },
      ]),
    ).toEqual({
      id: "c1",
      name: "Ada",
      handles: ["+15550001"],
      handleCount: 1,
    });
  });

  it("uses preferred_name when name is missing (message-header participants)", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        { contact_id: "c1", handle: "+15550001", preferred_name: "Ada" },
      ])?.name,
    ).toBe("Ada");
  });

  it("counts two distinct phones for the same contact as two identities", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        { contact_id: "c1", handle: "+15550001", name: "Ada" },
        { contact_id: "c1", handle: "+15550002", name: "Ada" },
      ]),
    ).toMatchObject({
      handles: ["+15550001", "+15550002"],
      handleCount: 2,
    });
  });

  it("collapses raw and normalized forms of the same phone to handleCount 1", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        { contact_id: "c1", handle: "+15550001", name: "Ada" },
        { contact_id: "c1", handle: "15550001", name: "Ada" },
      ])?.handleCount,
    ).toBe(1);
  });

  it("falls back to the handle when no display name is set", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        { contact_id: "c1", handle: "+15550001", name: null },
      ])?.name,
    ).toBe("+15550001");
  });

  it("uses name_alias when preferred name is missing (thread chip fallback)", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        { contact_id: "c1", handle: "+15550001", name: null, name_alias: "Mom" },
      ])?.name,
    ).toBe("Mom");
  });

  it("prefers preferred name over name_alias so the stub heading matches the drawer", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [
        {
          contact_id: "c1",
          handle: "+15550001",
          name: "Ada",
          name_alias: "Mom",
        },
      ])?.name,
    ).toBe("Ada");
  });

  it("stubs at least one identity when matched handles are empty", () => {
    expect(
      contactPreviewFromThreadParticipants("c1", [{ contact_id: "c1", handle: "", name: "Ada" }]),
    ).toMatchObject({
      name: "Ada",
      handles: [],
      handleCount: 1,
    });
  });

  it("returns null when no participant matches the contact id", () => {
    expect(
      contactPreviewFromThreadParticipants("missing", [
        { contact_id: "c1", handle: "+15550001", name: "Ada" },
      ]),
    ).toBeNull();
  });
});

describe("sameContactPreviews", () => {
  const ada = { id: "1", name: "Ada", handles: ["+15550001"], handleCount: 1, groups: ["Family"] };

  it("treats a re-mapped but equal list as unchanged", () => {
    expect(
      sameContactPreviews([ada], [{ ...ada, handles: ["+15550001"], groups: ["Family"] }]),
    ).toBe(true);
  });

  it("reports a changed name", () => {
    expect(sameContactPreviews([ada], [{ ...ada, name: "Grace" }])).toBe(false);
  });

  it("reports changed group membership", () => {
    expect(sameContactPreviews([ada], [{ ...ada, groups: ["Work"] }])).toBe(false);
  });

  it("reports a changed length", () => {
    expect(sameContactPreviews([ada], [])).toBe(false);
    expect(sameContactPreviews([], [ada])).toBe(false);
  });

  it("treats two empty lists as unchanged", () => {
    expect(sameContactPreviews([], [])).toBe(true);
  });

  it("distinguishes a missing handles list from an empty one", () => {
    expect(
      sameContactPreviews([{ id: "1", name: "Ada" }], [{ id: "1", name: "Ada", handles: [] }]),
    ).toBe(false);
  });
});
