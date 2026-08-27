import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const isTauri = vi.fn();
const resolveImportStagingParent = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("./tauri-check", () => ({
  isTauri: () => isTauri(),
}));

vi.mock("./system-settings", () => ({
  resolveImportStagingParent: (...args: unknown[]) => resolveImportStagingParent(...args),
}));

describe("openPathInExplorer", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReset();
    resolveImportStagingParent.mockReset();
    isTauri.mockReturnValue(true);
    invoke.mockResolvedValue(undefined);
    resolveImportStagingParent.mockResolvedValue("/home/sam/message-vault");
  });

  it("invokes open_path with the staging parent in the desktop app", async () => {
    const { openPathInExplorer } = await import("./openPath");
    await openPathInExplorer("/home/sam/message-vault/staging");
    expect(invoke).toHaveBeenCalledWith("open_path", {
      path: "/home/sam/message-vault/staging",
      stagingRoot: "/home/sam/message-vault",
    });
  });

  it("no-ops on blank paths", async () => {
    const { openPathInExplorer } = await import("./openPath");
    await openPathInExplorer("   ");
    expect(invoke).not.toHaveBeenCalled();
    expect(resolveImportStagingParent).not.toHaveBeenCalled();
  });

  it("rejects when not running in Tauri", async () => {
    isTauri.mockReturnValue(false);
    const { openPathInExplorer } = await import("./openPath");
    await expect(openPathInExplorer("/home/sam/message-vault/staging")).rejects.toThrow(
      "desktop app",
    );
  });

  it("rejects when the staging parent cannot be resolved", async () => {
    resolveImportStagingParent.mockResolvedValue("");
    const { openPathInExplorer } = await import("./openPath");
    await expect(openPathInExplorer("/home/sam/message-vault/staging")).rejects.toThrow(
      /import staging directory/i,
    );
  });
});
