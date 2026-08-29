import { describe, expect, it } from "vitest";
import { permissionsLabel } from "./apiTokensUtils";

describe("permissionsLabel", () => {
  it("lists what the token may do", () => {
    expect(permissionsLabel({ can_import: true, can_export: true, can_delete: false })).toBe(
      "Import / Export",
    );
    expect(permissionsLabel({ can_import: true, can_export: true, can_delete: true })).toBe(
      "Import / Export / Delete",
    );
    expect(permissionsLabel({ can_import: false, can_export: true, can_delete: false })).toBe(
      "Export",
    );
  });

  it("says so when a token may do nothing", () => {
    expect(permissionsLabel({ can_import: false, can_export: false, can_delete: false })).toBe(
      "None",
    );
  });
});
