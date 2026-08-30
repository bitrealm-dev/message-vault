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

const runMock = vi.fn<(fn: () => Promise<unknown>) => Promise<TauriJobResult>>();
const cancelMock = vi.fn();
const postMock = vi.fn();
const resolveImportStagingDirMock = vi.fn();
const invokePathStatMock = vi.fn();
const invokePushMock = vi.fn();

vi.mock("../../lib/tauri", () => ({
  invokeExtract: vi.fn(),
  invokePush: (...args: unknown[]) => invokePushMock(...args),
  invokePathStat: (...args: unknown[]) => invokePathStatMock(...args),
}));

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
const { useImportJob, restoreFormFromSnapshot } = await import("./useImportJob");

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
    invokePathStatMock.mockReset();
    invokePathStatMock.mockResolvedValue(null);
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

  it("records the staging folder and device on the session it creates", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/home/u/message-vault/staging-260830");
    invokePathStatMock.mockResolvedValue({
      exists: true,
      isFile: false,
      isDirectory: true,
      sizeBytes: 4096,
      modifiedUnixMs: 1_756_512_000_000,
    });
    postMock.mockResolvedValue({ id: 42 });
    runMock.mockResolvedValue({ summary: "ok", report: failedReport() });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(baseForm);
    });

    const createCall = postMock.mock.calls.find(([path]) => path === "/v1/imports");
    expect(createCall).toBeDefined();
    const body = createCall?.[1] as Record<string, unknown>;
    expect(body.stage).toBe("parse");
    expect(body.device_id).toEqual(expect.any(String));
    expect(body.staging_dir).toBe("/home/u/message-vault/staging-260830");
    expect(body.form).toMatchObject({ source: "imessage-ios" });
  });

  it("keeps the backup password out of the stored form snapshot", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    invokePathStatMock.mockResolvedValue(null);
    postMock.mockResolvedValue({ id: 43 });
    runMock.mockResolvedValue({ summary: "ok", report: failedReport() });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport({ ...baseForm, backupPassword: "hunter2" });
    });

    const body = postMock.mock.calls.find(([path]) => path === "/v1/imports")?.[1] as Record<
      string,
      unknown
    >;
    expect(JSON.stringify(body.form)).not.toContain("hunter2");
  });

  it("moves the session to pushing before the upload starts", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    invokePathStatMock.mockResolvedValue(null);
    postMock.mockResolvedValue({ id: 44 });
    runMock.mockResolvedValue({ summary: "ok", report: failedReport() });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(baseForm);
    });

    const stageCall = postMock.mock.calls.find(([path]) => String(path).endsWith("/stage"));
    expect(stageCall?.[1]).toEqual({ stage: "pushing" });
  });
});

describe("useImportJob resume path", () => {
  beforeEach(() => {
    runMock.mockReset();
    cancelMock.mockReset();
    postMock.mockReset();
    resolveImportStagingDirMock.mockReset();
    invokePathStatMock.mockReset();
    invokePathStatMock.mockResolvedValue(null);
    invokePushMock.mockReset();
    postMock.mockImplementation(async () => ({}));
    // A resumed run only ever calls run() once, for the push.
    runMock.mockResolvedValue({ summary: "Push finished.", report: failedReport() });
  });

  it("passes the resumed session id and staging dir through to invokePush", async () => {
    // Unlike the other resume-path tests, run() here calls through to the
    // job function it was given, so invokePush actually runs (against the
    // mocked Tauri command) and its arguments can be inspected.
    runMock.mockImplementation(async (fn) => {
      await fn();
      return { summary: "Push finished.", report: failedReport() };
    });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(baseForm, {
        sessionId: 99,
        stagingDir: "/home/u/message-vault/staging-260830",
      });
    });

    expect(invokePushMock).toHaveBeenCalledTimes(1);
    expect(invokePushMock).toHaveBeenCalledWith(
      expect.objectContaining({
        import_id: 99,
        input_dir: "/home/u/message-vault/staging-260830",
      }),
    );
  });

  it("skips staging resolve, session create, and extract when resuming a push", async () => {
    const { result } = renderHook(() => useImportJob());

    await act(async () => {
      await result.current.startImport(baseForm, {
        sessionId: 99,
        stagingDir: "/home/u/message-vault/staging-260830",
      });
    });

    expect(resolveImportStagingDirMock).not.toHaveBeenCalled();
    expect(invokePathStatMock).not.toHaveBeenCalled();
    expect(postMock.mock.calls.some(([path]) => path === "/v1/imports")).toBe(false);
    expect(runMock).toHaveBeenCalledTimes(1); // push only, no extract
    expect(result.current.stagingDir).toBe("/home/u/message-vault/staging-260830");
    expect(result.current.importSessionId).toBe(99);
  });

  it("marks the first three steps already staged and moves the session to pushing", async () => {
    const { result } = renderHook(() => useImportJob());

    await act(async () => {
      await result.current.startImport(baseForm, {
        sessionId: 99,
        stagingDir: "/home/u/message-vault/staging-260830",
      });
    });

    const stageCall = postMock.mock.calls.find(([path]) => String(path).endsWith("/stage"));
    expect(stageCall).toEqual(["/v1/imports/99/stage", { stage: "pushing" }]);

    for (const step of result.current.steps.slice(0, 3)) {
      expect(step.status).toBe("done");
      expect(step.detail).toBe("Already staged");
      expect(step.durationMs).toBeUndefined();
    }
    expect(result.current.steps[3]).toMatchObject({ label: "Upload to vault" });
  });

  it("still posts /complete against the resumed session id", async () => {
    const { result } = renderHook(() => useImportJob());

    await act(async () => {
      await result.current.startImport(baseForm, {
        sessionId: 99,
        stagingDir: "/home/u/message-vault/staging-260830",
      });
    });

    expect(result.current.phase).toBe("done");
    const completeCall = postMock.mock.calls.find(([path]) => path === "/v1/imports/99/complete");
    expect(completeCall).toBeDefined();
  });
});

const validSnapshot = {
  source: "imessage-ios",
  backupPath: "/backups/iphone.tar",
  attachmentMedia: "copy",
  maxResolution: "720p",
  maxFps: "30",
  minSizeMb: "20",
  contactNameMode: "fill_missing",
  ownerPhones: ["+15551234567"],
  force: false,
  obfuscate: false,
  isSbr: false,
  attachmentRoot: "",
  appleContacts: "",
  whatsappWa: "",
  whatsappMedia: "",
  whatsappDb: "",
  whatsappBusiness: false,
};

describe("restoreFormFromSnapshot", () => {
  it("rebuilds form values from a stored snapshot, defaulting the omitted secrets", () => {
    expect(restoreFormFromSnapshot(validSnapshot)).toEqual({
      ...validSnapshot,
      backupPassword: "",
      whatsappKey: "",
    });
  });

  it.each([
    ["null", null],
    ["undefined", undefined],
    ["a string", "not an object"],
    ["an empty object", {}],
    ["a snapshot missing most fields", { source: "imessage-ios" }],
    ["an invalid attachmentMedia", { ...validSnapshot, attachmentMedia: "not-a-real-mode" }],
    ["a non-array ownerPhones", { ...validSnapshot, ownerPhones: "+15551234567" }],
    ["a non-boolean force", { ...validSnapshot, force: "yes" }],
  ])("returns null for a malformed snapshot (%s)", (_label, raw) => {
    expect(restoreFormFromSnapshot(raw)).toBeNull();
  });
});
