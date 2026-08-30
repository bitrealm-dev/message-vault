/** @vitest-environment jsdom */

// Pins the wiring the 2026-08-27 incident broke: useImportJob must pass
// pushResult.report into importOutcome, use that outcome as
// finalSummary.status, and send it as `status` in the /complete POST body.
// The verdict logic itself is covered exhaustively by importOutcome.test.ts;
// nothing there would have caught a revert of the three lines that connect
// that logic to the hook, because those tests call importOutcome directly.

import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PushFinishedReport, TauriJobResult } from "../../lib/tauri";

const runMock = vi.fn<() => Promise<TauriJobResult>>();
const cancelMock = vi.fn();
const postMock = vi.fn();
const resolveImportStagingDirMock = vi.fn();

vi.mock("../../hooks/useTauriJob", () => ({
  useTauriJob: () => ({
    run: runMock,
    cancel: cancelMock,
    running: false,
    finished: true,
    log: [],
  }),
}));

vi.mock("../../lib/api", () => ({
  apiClient: {
    post: (...args: unknown[]) => postMock(...args),
  },
  getBaseUrl: () => "http://127.0.0.1:8080",
}));

vi.mock("../../lib/auth", () => ({
  useAuth: () => ({ token: "test-token" }),
}));

vi.mock("../../lib/system-settings", () => ({
  resolveImportStagingDir: (...args: unknown[]) => resolveImportStagingDirMock(...args),
}));

vi.mock("../../lib/tauri-check", () => ({
  isTauri: () => true,
}));

// Imported after the mocks above so useImportJob picks up the mocked modules.
const { useImportJob } = await import("./useImportJob");

function failedReport(): PushFinishedReport {
  return {
    ok: false,
    messages: 8_000,
    messages_attempted: 8_000,
    messages_inserted: 0,
    messages_deduped: 0,
    messages_failed: 8_000,
    assets_uploaded: 0,
    assets_bytes: 0,
    conversations_ok: 0,
    conversations_total: 681,
    conversations_failed: 681,
    conversations_skipped: 0,
    results: [],
  };
}

const baseForm = {
  source: "imessage-ios",
  backupPath: "/backups/iphone.tar",
  backupPassword: "",
  attachmentMedia: "copy" as const,
  maxResolution: "",
  maxFps: "",
  minSizeMb: "",
  contactNameMode: "fill_missing" as const,
  ownerPhones: [],
  force: false,
  obfuscate: false,
  isSbr: false,
  attachmentRoot: "",
  appleContacts: "",
  whatsappKey: "",
  whatsappWa: "",
  whatsappMedia: "",
  whatsappDb: "",
  whatsappBusiness: false,
};

describe("useImportJob wiring", () => {
  beforeEach(() => {
    runMock.mockReset();
    cancelMock.mockReset();
    postMock.mockReset();
    resolveImportStagingDirMock.mockReset();
    resolveImportStagingDirMock.mockResolvedValue("/home/sam/message-vault/staging-iphone");
    postMock.mockImplementation(async (path: string) => {
      if (path === "/v1/imports") return { id: 42 };
      return {};
    });
    // First run() call is extract, second is push. Neither invokeFn is
    // actually called by this mock, so the real Tauri commands never fire.
    runMock
      .mockResolvedValueOnce({
        summary: "Extracted 8000 messages.",
        extraction: { files_parsed: 681, messages_parsed: 8_000 },
      })
      .mockResolvedValueOnce({
        summary: "Push finished.",
        report: failedReport(),
      });
  });

  it("carries a report where every conversation failed through to a failed summary and /complete body", async () => {
    const { result } = renderHook(() => useImportJob());

    await act(async () => {
      await result.current.startImport(baseForm);
    });

    // The wiring under test: the hook's own verdict, not importOutcome's.
    expect(result.current.summaryView?.status).toBe("failed");
    expect(result.current.phase).toBe("done");

    const completeCall = postMock.mock.calls.find(([path]) => path === "/v1/imports/42/complete");
    expect(completeCall).toBeDefined();
    const [, body] = completeCall as [string, Record<string, unknown>];
    expect(body.status).toBe("failed");
    expect(body.ok).toBe(false);
  });
});
