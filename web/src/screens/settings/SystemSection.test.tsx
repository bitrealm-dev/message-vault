/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SystemSection } from "./SystemSection";

const tauriState = vi.hoisted(() => ({ isTauri: true }));
const probeFfmpegTools = vi.hoisted(() => vi.fn());
const setFfmpegToolsDir = vi.hoisted(() => vi.fn());
const getHomeDir = vi.hoisted(() => vi.fn());

vi.mock("../../lib/tauri-check", () => ({
  isTauri: () => tauriState.isTauri,
}));

vi.mock("../../lib/tauri", () => ({
  probeFfmpegTools: (...args: unknown[]) => probeFfmpegTools(...args),
  setFfmpegToolsDir: (...args: unknown[]) => setFfmpegToolsDir(...args),
}));

vi.mock("../../lib/system-settings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/system-settings")>();
  return {
    ...actual,
    getHomeDir: () => getHomeDir(),
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  localStorage.clear();
  tauriState.isTauri = true;
  getHomeDir.mockResolvedValue("/home/demo");
  probeFfmpegTools.mockResolvedValue({
    ok: true,
    ffmpeg_path: "/usr/bin/ffmpeg",
    ffprobe_path: "/usr/bin/ffprobe",
    error: null,
  });
  setFfmpegToolsDir.mockResolvedValue({
    ok: true,
    ffmpeg_path: "/usr/bin/ffmpeg",
    ffprobe_path: "/usr/bin/ffprobe",
    error: null,
  });
});

describe("SystemSection", () => {
  it("shows the desktop-only stub when not in Tauri", () => {
    tauriState.isTauri = false;
    render(<SystemSection />);
    expect(screen.getByText(/available in the desktop app/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  });

  it("has no Save button and uses the new labels", async () => {
    render(<SystemSection />);
    await waitFor(() => {
      expect(screen.getByText("Import staging directory")).toBeTruthy();
    });
    expect(screen.getByText("ffmpeg directory")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Saving…" })).toBeNull();
  });

  it("persists import staging directory on change", async () => {
    const user = userEvent.setup();
    render(<SystemSection />);
    await waitFor(() => {
      expect(screen.getByDisplayValue("/home/demo/message-vault")).toBeTruthy();
    });

    const stagingInput = screen.getByDisplayValue("/home/demo/message-vault");
    await user.clear(stagingInput);
    await user.type(stagingInput, "/tmp/my-staging");

    expect(localStorage.getItem("mv-vault-working-dir")).toBe("/tmp/my-staging");
  });

  it("shows Found lines when both tools are present", async () => {
    render(<SystemSection />);
    await waitFor(() => {
      expect(screen.getByLabelText(/Found ffmpeg/i)).toBeTruthy();
    });
    expect(screen.getByLabelText(/Found ffprobe/i)).toBeTruthy();
    expect(screen.getByText("/usr/bin/ffmpeg")).toBeTruthy();
    expect(screen.getByText("/usr/bin/ffprobe")).toBeTruthy();
  });

  it("shows not-found when ffmpeg is missing", async () => {
    const missing = {
      ok: false,
      ffmpeg_path: null,
      ffprobe_path: "/usr/bin/ffprobe",
      error: "ffmpeg not found or failed -version",
    };
    probeFfmpegTools.mockResolvedValue(missing);
    setFfmpegToolsDir.mockResolvedValue(missing);
    render(<SystemSection />);
    await waitFor(() => {
      expect(screen.getByLabelText(/ffmpeg not found/i)).toBeTruthy();
    });
    expect(screen.getByLabelText(/Found ffprobe/i)).toBeTruthy();
  });
});
