import { describe, expect, it } from "vitest";
import {
  contactPreviewFromListRow,
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
