/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import ExportScreen from "./ExportScreen";

const invokePull = vi.hoisted(() => vi.fn());
const invokeFormat = vi.hoisted(() => vi.fn());
const invokeDeleteStaging = vi.hoisted(() => vi.fn());
const invokeCancel = vi.hoisted(() => vi.fn());
const resolveExportStagingDir = vi.hoisted(() => vi.fn());
const awaitTauriJob = vi.hoisted(() => vi.fn());

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => true,
}));

vi.mock("../lib/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/tauri")>();
  return {
    EXPORT_FORMATS: actual.EXPORT_FORMATS,
    invokePull: (...args: unknown[]) => invokePull(...args),
    invokeFormat: (...args: unknown[]) => invokeFormat(...args),
    invokeDeleteStaging: (...args: unknown[]) => invokeDeleteStaging(...args),
    invokeCancel: (...args: unknown[]) => invokeCancel(...args),
    awaitTauriJob: (...args: unknown[]) => awaitTauriJob(...args),
    onExtractEvents: vi.fn(async () => () => {}),
  };
});

vi.mock("../lib/system-settings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/system-settings")>();
  return {
    ...actual,
    resolveExportStagingDir: (...args: unknown[]) => resolveExportStagingDir(...args),
  };
});

vi.mock("../lib/api", () => ({
  getBaseUrl: () => "http://127.0.0.1:8080",
}));

vi.mock("../lib/auth", () => ({
  useAuth: () => ({ token: "test-token" }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

beforeEach(() => {
  resolveExportStagingDir.mockResolvedValue(
    "/home/demo/message-vault/staging-export-260831-120000",
  );
  // The hook's `run` goes through awaitTauriJob: call the invoke and resolve.
  awaitTauriJob.mockImplementation(async (invokeFn: () => Promise<void>) => {
    await invokeFn();
    return { summary: "done" };
  });
});

/** Fill the save folder and press Export. */
async function exportTo(folder: string) {
  const user = userEvent.setup();
  render(<ExportScreen />);
  await user.type(screen.getByPlaceholderText("Choose folder…"), folder);
  await user.click(screen.getByRole("button", { name: "Export" }));
  return user;
}

/** Pick a format from the Format select, then press Export. */
async function exportAs(folder: string, formatLabel: string) {
  const user = userEvent.setup();
  render(<ExportScreen />);
  await user.type(screen.getByPlaceholderText("Choose folder…"), folder);
  await user.click(screen.getByRole("button", { name: /Format/ }));
  await user.click(await screen.findByRole("option", { name: formatLabel }));
  await user.click(screen.getByRole("button", { name: "Export" }));
  return user;
}

describe("ExportScreen", () => {
  it("pulls straight into the chosen folder for JSON Lines", async () => {
    await exportTo("/home/demo/out");

    await waitFor(() => expect(invokePull).toHaveBeenCalledTimes(1));
    expect(invokePull.mock.calls[0][0]).toMatchObject({ out_dir: "/home/demo/out" });
    // JSONL is what pull already writes, so there is nothing to convert and
    // no staging folder to make or remove.
    expect(resolveExportStagingDir).not.toHaveBeenCalled();
    expect(invokeFormat).not.toHaveBeenCalled();
    expect(invokeDeleteStaging).not.toHaveBeenCalled();
  });

  it("pulls into staging and converts into the chosen folder for CSV", async () => {
    const staging = "/home/demo/message-vault/staging-export-260831-120000";
    await exportAs("/home/demo/out", "CSV (.csv)");

    await waitFor(() => expect(invokeFormat).toHaveBeenCalledTimes(1));
    expect(invokePull.mock.calls[0][0]).toMatchObject({ out_dir: staging });
    expect(invokeFormat.mock.calls[0][0]).toEqual({
      input_dir: staging,
      output_dir: "/home/demo/out",
      output_format: "csv",
    });
  });

  it("removes the staging folder once the conversion finishes", async () => {
    const staging = "/home/demo/message-vault/staging-export-260831-120000";
    await exportAs("/home/demo/out", "CSV (.csv)");

    await waitFor(() => expect(invokeDeleteStaging).toHaveBeenCalledWith({ staging_dir: staging }));
  });

  it("removes the staging folder even when the conversion fails", async () => {
    // Otherwise a failed export silently leaves a whole copy of the vault on
    // disk, in a folder the person never chose and will not think to look in.
    const staging = "/home/demo/message-vault/staging-export-260831-120000";
    awaitTauriJob.mockImplementationOnce(async (invokeFn: () => Promise<void>) => {
      await invokeFn();
      return { summary: "pulled" };
    });
    awaitTauriJob.mockImplementationOnce(async () => {
      throw new Error("unsupported output format");
    });

    await exportAs("/home/demo/out", "CSV (.csv)");

    await waitFor(() => expect(invokeDeleteStaging).toHaveBeenCalledWith({ staging_dir: staging }));
    expect(await screen.findByText("unsupported output format")).toBeTruthy();
  });

  it("reports the failure rather than claiming the export finished", async () => {
    awaitTauriJob.mockImplementation(async () => {
      throw new Error("vault key is required");
    });

    await exportTo("/home/demo/out");

    expect(await screen.findByText("vault key is required")).toBeTruthy();
    expect(screen.queryByText(/Export complete/)).toBeNull();
  });
});
