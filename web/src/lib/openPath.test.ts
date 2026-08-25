import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const isTauri = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("./tauri-check", () => ({
  isTauri: () => isTauri(),
}));

describe("openPathInExplorer", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReset();
    isTauri.mockReturnValue(true);
    invoke.mockResolvedValue(undefined);
  });

  it("invokes open_path in the desktop app", async () => {
    const { openPathInExplorer } = await import("./openPath");
    await openPathInExplorer("/home/sam/message-vault/staging");
    expect(invoke).toHaveBeenCalledWith("open_path", {
      path: "/home/sam/message-vault/staging",
    });
  });

  it("no-ops on blank paths", async () => {
    const { openPathInExplorer } = await import("./openPath");
    await openPathInExplorer("   ");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("rejects when not running in Tauri", async () => {
    isTauri.mockReturnValue(false);
    const { openPathInExplorer } = await import("./openPath");
    await expect(openPathInExplorer("/home/sam/message-vault/staging")).rejects.toThrow(
      "desktop app",
    );
  });
});
