import { describe, expect, it } from "vitest";
import {
  contactPreviewFromListRow,
  contactPreviewFromThreadParticipants,
  HANDLE_STUB_PLACEHOLDER,
  previewHandleStubRows,
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

  it("returns null when no participant matches the contact id", () => {
    expect(
      contactPreviewFromThreadParticipants("missing", [
        { contact_id: "c1", handle: "+15550001", name: "Ada" },
      ]),
    ).toBeNull();
  });
});
