/** @vitest-environment jsdom */

// Pins the wiring the 2026-08-27 incident broke: useImportJob must pass
// pushResult.report into importOutcome, use that outcome as
// finalSummary.status, and send it as `status` in the /complete POST body.
// The verdict logic itself is covered exhaustively by importOutcome.test.ts;
// nothing there would have caught a revert of the three lines that connect
// that logic to the hook, because those tests call importOutcome directly.
//
// It also pins the two-gate flow added afterward: startImport now stops at
// Gate 1 instead of pushing straight through, approveGate runs the media
// pass (when there is one) and stops at Gate 2, and declineGate closes the
// session and deletes the staging folder. Every push assertion below goes
// through approveGate first, because there is no other way to reach it.

import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  FfmpegToolsProbe,
  PushFinishedReport,
  StagingSummary,
  TauriJobResult,
} from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";

const runMock = vi.fn<(fn: () => Promise<unknown>) => Promise<TauriJobResult>>();
const cancelMock = vi.fn();
const postMock = vi.fn();
const resolveImportStagingDirMock = vi.fn();
const invokePathStatMock = vi.fn();
const invokePushMock = vi.fn();
const invokeExtractMock = vi.fn();
const invokeSummarizeStagingMock = vi.fn();
const invokeTranscodeStagingMock = vi.fn();
const invokeDeleteStagingMock = vi.fn();
const probeFfmpegToolsMock = vi.fn<(dir: string | null) => Promise<FfmpegToolsProbe>>();
const setImportStageMock = vi.fn();
const discardImportSessionMock = vi.fn();

vi.mock("../../lib/tauri", () => ({
  invokeExtract: (...args: unknown[]) => invokeExtractMock(...args),
  invokePush: (...args: unknown[]) => invokePushMock(...args),
  invokePathStat: (...args: unknown[]) => invokePathStatMock(...args),
  invokeSummarizeStaging: (...args: unknown[]) => invokeSummarizeStagingMock(...args),
  invokeTranscodeStaging: (...args: unknown[]) => invokeTranscodeStagingMock(...args),
  invokeDeleteStaging: (...args: unknown[]) => invokeDeleteStagingMock(...args),
  probeFfmpegTools: (...args: [string | null]) => probeFfmpegToolsMock(...args),
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

vi.mock("../../lib/importSession", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/importSession")>();
  return {
    ...actual,
    setImportStage: (...args: unknown[]) => setImportStageMock(...args),
    discardImportSession: (...args: unknown[]) => discardImportSessionMock(...args),
  };
});

// Imported after the mocks above so useImportJob picks up the mocked modules.
const { useImportJob, restoreFormFromSnapshot } = await import("./useImportJob");

/**
 * `runMock` stands in for `useTauriJob().run`, which always calls the
 * invoke function it is given before resolving. Tests that assert on
 * `invokeExtract`/`invokeTranscodeStaging`/`invokePush` args need that same
 * behaviour, so every canned result below goes through this instead of
 * `mockResolvedValueOnce` (which would never call the function at all).
 */
function runResult(result: TauriJobResult) {
  return async (fn: () => Promise<unknown>) => {
    await fn();
    return result;
  };
}

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

function okReport(overrides: Partial<PushFinishedReport> = {}): PushFinishedReport {
  return {
    ok: true,
    messages: 10,
    messages_attempted: 10,
    messages_inserted: 10,
    messages_deduped: 0,
    messages_failed: 0,
    assets_uploaded: 0,
    assets_bytes: 0,
    conversations_ok: 1,
    conversations_total: 1,
    conversations_failed: 0,
    conversations_skipped: 0,
    results: [],
    ...overrides,
  };
}

function stagingSummary(overrides: Partial<StagingSummary> = {}): StagingSummary {
  return {
    conversations: 1,
    messages: 10,
    contactIdentifiers: [],
    attachments: 0,
    attachmentBytes: 0,
    verdictCounts: {
      fitsAsIs: 0,
      likelyFits: 0,
      mayGrow: 0,
      probablyTooBig: 0,
      cannotProcess: 0,
    },
    forecasts: [],
    ...overrides,
  };
}

function okProbe(): FfmpegToolsProbe {
  return {
    ok: true,
    ffmpeg_path: "/usr/bin/ffmpeg",
    ffprobe_path: "/usr/bin/ffprobe",
    error: null,
  };
}

const EXTRACT_RESULT: TauriJobResult = {
  summary: "Extracted 8000 messages.",
  extraction: { files_parsed: 681, messages_parsed: 8_000 },
};

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

function form(overrides: { attachmentMedia?: AttachmentMediaMode } = {}) {
  return { ...baseForm, ...overrides };
}

describe("useImportJob wiring", () => {
  beforeEach(() => {
    runMock.mockReset();
    // Every test's first run() call is extract; a test that also calls
    // approveGate() chains a second implementation for the media pass or
    // the push on top of this one.
    runMock.mockImplementationOnce(runResult(EXTRACT_RESULT));
    cancelMock.mockReset();
    postMock.mockReset();
    resolveImportStagingDirMock.mockReset();
    resolveImportStagingDirMock.mockResolvedValue("/home/sam/message-vault/staging-iphone");
    invokePathStatMock.mockReset();
    invokePathStatMock.mockResolvedValue(null);
    invokeExtractMock.mockReset();
    invokePushMock.mockReset();
    invokeSummarizeStagingMock.mockReset();
    invokeSummarizeStagingMock.mockResolvedValue(stagingSummary());
    invokeTranscodeStagingMock.mockReset();
    invokeDeleteStagingMock.mockReset();
    invokeDeleteStagingMock.mockResolvedValue(undefined);
    probeFfmpegToolsMock.mockReset();
    probeFfmpegToolsMock.mockResolvedValue(okProbe());
    setImportStageMock.mockReset();
    setImportStageMock.mockResolvedValue(undefined);
    discardImportSessionMock.mockReset();
    discardImportSessionMock.mockResolvedValue(undefined);
    postMock.mockImplementation(async (path: string) => {
      if (path === "/v1/imports") return { id: 1 };
      return {};
    });
  });

  it("stops at the first gate instead of uploading", async () => {
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    expect(result.current.phase).toBe("gate_1");
    expect(invokePushMock).not.toHaveBeenCalled();
    expect(invokeTranscodeStagingMock).not.toHaveBeenCalled();
  });

  it("asks the exporter to stage originals under convert", async () => {
    // The desktop runs the media pass itself, after the gate.
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    expect(invokeExtractMock).toHaveBeenCalledWith(
      expect.objectContaining({ attachment_media: "copy" }),
    );
  });

  it("records the stage as it goes", async () => {
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    // No plan exists yet at either call — `setImportStage` still receives a
    // (harmlessly `undefined`) third argument; see `moveStage`.
    expect(setImportStageMock).toHaveBeenCalledWith(1, "write", undefined);
    expect(setImportStageMock).toHaveBeenCalledWith(1, "awaiting_gate_1", undefined);
  });

  it("runs the media pass then stops at the second gate", async () => {
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());
    expect(invokeTranscodeStagingMock).toHaveBeenCalled();
    expect(result.current.phase).toBe("gate_2");
    expect(invokePushMock).not.toHaveBeenCalled();
  });

  it("uploads straight from the first gate under copy, because there is no second one", async () => {
    runMock.mockImplementationOnce(runResult({ summary: "Push finished.", report: okReport() }));
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "copy" })));
    await act(() => result.current.approveGate());
    expect(invokeTranscodeStagingMock).not.toHaveBeenCalled();
    expect(invokePushMock).toHaveBeenCalled();
  });

  it("carries the plan approved at Gate 1 into the pushing stage call under copy", async () => {
    runMock.mockImplementationOnce(runResult({ summary: "Push finished.", report: okReport() }));
    const approved = stagingSummary({ conversations: 3 });
    invokeSummarizeStagingMock.mockResolvedValueOnce(approved);
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "copy" })));
    await act(() => result.current.approveGate());

    expect(setImportStageMock).toHaveBeenCalledWith(1, "pushing", approved);
  });

  it("recomputes the summary after the media pass rather than adjusting the old one", async () => {
    // Decision 39: the folder is the truth.
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    invokeSummarizeStagingMock.mockClear();
    await act(() => result.current.approveGate());
    expect(invokeSummarizeStagingMock).toHaveBeenCalledTimes(1);
  });

  it("writes transcode carrying the Gate-1 plan, then awaiting_gate_2 carrying it too", async () => {
    // Important 4: a crash mid-pass must not leave summary_json null with
    // no baseline for a later resume — so the plan rides the "transcode"
    // stage call too, not only the one after it.
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    const approved = stagingSummary({ conversations: 5 });
    invokeSummarizeStagingMock.mockResolvedValueOnce(approved);
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());

    expect(setImportStageMock).toHaveBeenCalledWith(1, "transcode", approved);
    expect(setImportStageMock).toHaveBeenCalledWith(1, "awaiting_gate_2", approved);
  });

  it("declining closes the session and deletes the folder", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/staging/run-1");
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.declineGate());
    expect(discardImportSessionMock).toHaveBeenCalledWith(1);
    expect(invokeDeleteStagingMock).toHaveBeenCalledWith({ staging_dir: "/staging/run-1" });
    expect(result.current.phase).toBe("form");
  });

  it("deletes the folder even when discarding the session fails", async () => {
    // Either half failing must not leave the other undone: a live session with
    // no folder blocks the next import, and a folder with no session is litter
    // nothing will ever clean up.
    discardImportSessionMock.mockRejectedValueOnce(new Error("offline"));
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.declineGate());
    expect(invokeDeleteStagingMock).toHaveBeenCalled();
  });

  it("still discards the session, and still returns to the form, even when deleting the folder fails", async () => {
    // The other direction of the same guarantee: a regression to sequential
    // discard-then-delete (each awaited without independent handling) would
    // let a rejected delete propagate out of declineGate and skip
    // returnToForm — leaving the screen stuck on Gate 1 with a session the
    // vault already considers discarded.
    invokeDeleteStagingMock.mockRejectedValueOnce(new Error("disk full"));
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.declineGate());
    expect(discardImportSessionMock).toHaveBeenCalledWith(1);
    expect(result.current.phase).toBe("form");
  });

  it("declines from Gate 2 the same way — closes the session and deletes the folder", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/staging/run-2");
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());
    expect(result.current.phase).toBe("gate_2");

    await act(() => result.current.declineGate());

    expect(discardImportSessionMock).toHaveBeenCalledWith(1);
    expect(invokeDeleteStagingMock).toHaveBeenCalledWith({ staging_dir: "/staging/run-2" });
    expect(result.current.phase).toBe("form");
  });

  it("approving at Gate 2 writes pushing carrying the recomputed summary, not Gate 1's", async () => {
    // Decision 15: the diff at Gate 2 is against what was approved at Gate
    // 1, but what gets approved when Gate 2 itself is approved is the
    // summary Gate 2 is showing — the recomputed one, not the original.
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    runMock.mockImplementationOnce(runResult({ summary: "Push finished.", report: okReport() }));
    const gate1Approved = stagingSummary({ conversations: 1 });
    const recomputed = stagingSummary({ conversations: 1, attachments: 4 });
    invokeSummarizeStagingMock.mockResolvedValueOnce(gate1Approved);
    invokeSummarizeStagingMock.mockResolvedValueOnce(recomputed);

    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate()); // Gate 1 -> media pass -> Gate 2
    expect(result.current.phase).toBe("gate_2");

    await act(() => result.current.approveGate()); // Gate 2 -> pushing

    expect(setImportStageMock).toHaveBeenCalledWith(1, "pushing", recomputed);
    expect(setImportStageMock).not.toHaveBeenCalledWith(1, "pushing", gate1Approved);
  });

  it("reaches a failed push through Gate 2 the same way copy mode does through Gate 1", async () => {
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    runMock.mockImplementationOnce(
      runResult({ summary: "Push finished.", report: failedReport() }),
    );
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());
    expect(result.current.phase).toBe("gate_2");

    await act(() => result.current.approveGate());

    expect(result.current.phase).toBe("done");
    expect(result.current.summaryView?.status).toBe("failed");
  });

  it("unwedges on a cancelled media pass instead of freezing the screen", async () => {
    // Critical: transcode_staging used to end a cancelled pass quietly (an
    // extract:log line, Ok(())) with no extract:finished and no
    // extract:error, so awaitTauriJob's promise never settled and the
    // screen was stuck. It now reports through extract:error like any other
    // failure, so run() rejects here exactly as it would for a real error.
    runMock.mockImplementationOnce(async (fn: () => Promise<unknown>) => {
      await fn();
      throw new Error("canceled");
    });
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());

    expect(invokePushMock).not.toHaveBeenCalled();
    expect(result.current.phase).toBe("done");
    expect(result.current.summaryView?.status).toBe("canceled");
    // The session stays wherever the run actually got to — "transcode" —
    // never advanced to a stage the cancelled run never reached.
    expect(setImportStageMock).not.toHaveBeenCalledWith(1, "awaiting_gate_2", expect.anything());
    expect(setImportStageMock).not.toHaveBeenCalledWith(1, "pushing", expect.anything());
  });

  it("does not complete the session on a cancelled media pass, so it stays resumable", async () => {
    // Decision 36 routes a cancellation mid-transcode to the same recovery
    // as a crash at that stage; decision 37 says only an explicit discard
    // ends a waiting session. Posting /complete would free the one-active-
    // session slot and drop the session out of GET /v1/imports/active,
    // stranding the staged folder with no session left to resume it
    // through — even though the "canceled" outcome is still shown locally.
    runMock.mockImplementationOnce(async (fn: () => Promise<unknown>) => {
      await fn();
      throw new Error("canceled");
    });
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());

    expect(result.current.summaryView?.status).toBe("canceled");
    expect(postMock.mock.calls.some(([path]) => path === "/v1/imports/1/complete")).toBe(false);
  });

  it("still completes the session as failed when the media pass genuinely fails", async () => {
    // Unlike a cancellation, a broken ffmpeg (or any other real failure)
    // must not lock the account out of importing — the run still completes
    // and frees the slot, same as before.
    runMock.mockRejectedValueOnce(new Error("ffmpeg exited with status 1"));
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());

    expect(result.current.summaryView?.status).toBe("failed");
    const completeCall = postMock.mock.calls.find(([path]) => path === "/v1/imports/1/complete");
    expect(completeCall).toBeDefined();
    const [, body] = completeCall as [string, Record<string, unknown>];
    expect(body.status).toBe("failed");
  });

  it("still completes the session on a cancelled extract — nothing was approved yet to protect", async () => {
    // Only a cancellation *after* Gate 1 (mid-transcode, with an approved
    // plan and a staged folder worth protecting) skips /complete. A
    // cancelled extract has nothing approved yet, so the spec sends it to
    // restart regardless, same as before this fix.
    runMock.mockReset();
    runMock.mockImplementationOnce(async (fn: () => Promise<unknown>) => {
      await fn();
      throw new Error("cancelled");
    });
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));

    expect(result.current.phase).toBe("done");
    const completeCall = postMock.mock.calls.find(([path]) => path === "/v1/imports/1/complete");
    expect(completeCall).toBeDefined();
  });

  it("a failed recompute after a successful media pass is a failed import, not an unhandled rejection", async () => {
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );
    invokeSummarizeStagingMock.mockRejectedValueOnce(new Error("disk full"));
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());

    expect(invokePushMock).not.toHaveBeenCalled();
    expect(result.current.phase).toBe("done");
    expect(result.current.summaryView?.status).toBe("failed");
    expect(setImportStageMock).not.toHaveBeenCalledWith(1, "awaiting_gate_2", expect.anything());
  });

  it("does not run the media pass twice on a double click", async () => {
    let resolveTranscode!: (value: TauriJobResult) => void;
    const pending = new Promise<TauriJobResult>((resolve) => {
      resolveTranscode = resolve;
    });
    // Deliberately left unresolved: lets the two approveGate() calls below
    // race while the pass is still "running".
    runMock.mockImplementationOnce(async (fn: () => Promise<unknown>) => {
      await fn();
      return pending;
    });
    // A fallback that keeps calling through, so if the double-click guard
    // were ever removed, a genuine second (or third) run() call would show
    // up as a genuine second call to invokeTranscodeStagingMock below —
    // without this, the mock's one-time queue would just exhaust and return
    // `undefined` without invoking anything, and the guard could be deleted
    // without this test noticing.
    runMock.mockImplementation(runResult({ summary: "Transcode finished.", transcode: undefined }));

    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(async () => {
      void result.current.approveGate();
      void result.current.approveGate();
      resolveTranscode({ summary: "Transcode finished.", transcode: undefined });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(invokeTranscodeStagingMock).toHaveBeenCalledTimes(1);
    expect(runMock).toHaveBeenCalledTimes(2); // extract, then exactly one media pass
  });

  it("a failed media pass is a failed import, not a silent skip to upload", async () => {
    runMock.mockImplementationOnce(async (fn: () => Promise<unknown>) => {
      await fn();
      throw new Error("ffmpeg missing");
    });
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    await act(() => result.current.approveGate());
    expect(invokePushMock).not.toHaveBeenCalled();
    expect(result.current.phase).toBe("done");
    expect(result.current.summaryView?.status).toBe("failed");
  });

  it("carries a report where every conversation failed through to a failed summary and /complete body", async () => {
    runMock.mockImplementationOnce(
      runResult({ summary: "Push finished.", report: failedReport() }),
    );
    const { result } = renderHook(() => useImportJob());

    await act(() => result.current.startImport(baseForm));
    await act(() => result.current.approveGate());

    // The wiring under test: the hook's own verdict, not importOutcome's.
    expect(result.current.summaryView?.status).toBe("failed");
    expect(result.current.phase).toBe("done");

    const completeCall = postMock.mock.calls.find(([path]) => path === "/v1/imports/1/complete");
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

    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(baseForm));

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
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport({ ...baseForm, backupPassword: "hunter2" }));

    const body = postMock.mock.calls.find(([path]) => path === "/v1/imports")?.[1] as Record<
      string,
      unknown
    >;
    expect(JSON.stringify(body.form)).not.toContain("hunter2");
  });

  it("moves the session to pushing before the upload starts", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    runMock.mockImplementationOnce(
      runResult({ summary: "Push finished.", report: failedReport() }),
    );

    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(baseForm));
    await act(() => result.current.approveGate());

    const stageCall = setImportStageMock.mock.calls.find(([, stage]) => stage === "pushing");
    expect(stageCall).toBeDefined();
    expect(stageCall?.[0]).toBe(1);
  });

  it("assembles a 4-row step list in convert mode, stopping at the gate with the media row still pending", async () => {
    // Pins the mode-dependent assembly stepsFor/stepIndexFor exist for: this
    // hook does not run the media pass until Gate 1 is approved, so the row
    // must sit pending, not silently vanish or get marked done.
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");

    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport({ ...baseForm, attachmentMedia: "convert" }));

    expect(result.current.phase).toBe("gate_1");
    expect(result.current.steps.map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Convert media",
      "Upload to vault",
    ]);
    expect(result.current.steps[2]?.status).toBe("pending");
    expect(result.current.steps[3]?.status).toBe("pending");
  });

  it("continues the convert-mode step list through the media pass into Gate 2", async () => {
    // Task 7 pinned the media row sitting pending after extract; this
    // continues the same run through approval: active while the pass runs,
    // done once it finishes.
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    runMock.mockImplementationOnce(
      runResult({ summary: "Transcode finished.", transcode: undefined }),
    );

    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport({ ...baseForm, attachmentMedia: "convert" }));
    await act(() => result.current.approveGate());

    expect(result.current.phase).toBe("gate_2");
    expect(result.current.steps[2]?.status).toBe("done");
  });

  it("says the staging row was Copied under convert, since extract only stages originals now", async () => {
    // Important 5: extract stages originals under convert/compress too
    // (ruling 3) — the staging row must say what extract actually did, not
    // what the user ultimately asked for. The media row (index 2) still
    // tells the convert/compress story once the pass itself runs.
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport({ ...baseForm, attachmentMedia: "convert" }));

    expect(result.current.steps[1]?.detail).toMatch(/^Copied /);
    expect(result.current.steps[1]?.detail).not.toMatch(/^Converted /);
  });

  it("never probes ffmpeg tools under copy mode, which never needs them", async () => {
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "copy" })));
    expect(probeFfmpegToolsMock).not.toHaveBeenCalled();
    expect(result.current.mediaToolsMissing).toBe(false);
  });

  it("flags missing ffmpeg tools at Gate 1 under convert", async () => {
    probeFfmpegToolsMock.mockResolvedValue({
      ok: false,
      ffmpeg_path: null,
      ffprobe_path: null,
      error: "ffmpeg not found",
    });
    const { result } = renderHook(() => useImportJob());
    await act(() => result.current.startImport(form({ attachmentMedia: "convert" })));
    expect(result.current.mediaToolsMissing).toBe(true);
  });
});

describe("useImportJob resume path", () => {
  beforeEach(() => {
    runMock.mockReset();
    // A resumed run only ever calls run() once, for the push — and, like
    // the wiring tests above, must actually call the invoke function so
    // invokePush's args can be inspected.
    runMock.mockImplementation(runResult({ summary: "Push finished.", report: failedReport() }));
    cancelMock.mockReset();
    postMock.mockReset();
    resolveImportStagingDirMock.mockReset();
    invokePathStatMock.mockReset();
    invokePathStatMock.mockResolvedValue(null);
    invokePushMock.mockReset();
    setImportStageMock.mockReset();
    setImportStageMock.mockResolvedValue(undefined);
    discardImportSessionMock.mockReset();
    postMock.mockImplementation(async () => ({}));
  });

  it("passes the resumed session id and staging dir through to invokePush", async () => {
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

  it("marks the staging steps already staged and moves the session to pushing without a plan", async () => {
    // baseForm uses attachmentMedia "copy", which has no media step: Read
    // backup, Copy to staging, Upload to vault — three rows, not four.
    // Nothing was ever gated on a resumed run, so there is no approved plan
    // to carry — `setImportStage` still receives a third argument, but it's
    // `undefined`, which reaches the server identically to omitting it
    // entirely (JSON.stringify drops it).
    const { result } = renderHook(() => useImportJob());

    await act(async () => {
      await result.current.startImport(baseForm, {
        sessionId: 99,
        stagingDir: "/home/u/message-vault/staging-260830",
      });
    });

    expect(setImportStageMock).toHaveBeenCalledWith(99, "pushing", undefined);

    expect(result.current.steps).toHaveLength(3);
    for (const step of result.current.steps.slice(0, 2)) {
      expect(step.status).toBe("done");
      expect(step.detail).toBe("Already staged");
      expect(step.durationMs).toBeUndefined();
    }
    expect(result.current.steps[2]).toMatchObject({ label: "Upload to vault" });
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
