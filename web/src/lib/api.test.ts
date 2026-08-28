import { afterEach, describe, expect, it, vi } from "vitest";
import { apiClient, errorMessageFromBody, setBaseUrl, VaultApiError } from "./api";

afterEach(() => {
  vi.unstubAllGlobals();
  setBaseUrl("");
});

describe("errorMessageFromBody", () => {
  it("pulls the sentence out of the vault's error envelope", () => {
    expect(
      errorMessageFromBody(401, '{"ok":false,"error":"invalid username or password"}'),
    ).toBe("invalid username or password");
  });

  it("falls back to the raw body when it is not an envelope", () => {
    expect(errorMessageFromBody(502, "<html>Bad Gateway</html>")).toBe("<html>Bad Gateway</html>");
  });

  it("falls back to a generic sentence for an empty body", () => {
    expect(errorMessageFromBody(500, "   ")).toBe("Request failed (500)");
  });

  it("ignores an envelope whose error is blank", () => {
    expect(errorMessageFromBody(400, '{"ok":false,"error":"  "}')).toBe('{"ok":false,"error":"  "}');
  });
});

describe("apiClient errors", () => {
  it("throws a VaultApiError carrying the status and the server's message", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 409,
        text: async () => '{"ok":false,"error":"username already taken: matt"}',
      }),
    );

    await expect(apiClient.post("/v1/auth/register", {})).rejects.toMatchObject({
      name: "VaultApiError",
      status: 409,
      message: "username already taken: matt",
    });
  });

  it("is an Error, so existing catch blocks keep working", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        text: async () => '{"ok":false,"error":"invalid username or password"}',
      }),
    );

    const caught = await apiClient.get("/v1/whoami").catch((e: unknown) => e);
    expect(caught).toBeInstanceOf(Error);
    expect(caught).toBeInstanceOf(VaultApiError);
  });
});
