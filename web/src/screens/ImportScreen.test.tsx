/** @vitest-environment jsdom */

// Entering Import must ask the vault whether a session is already live
// before showing anything: neither the blank form nor the resume panel
// may flash on screen while that check is in flight, and a vault that
// can't answer falls through to the form rather than blocking it.

import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActiveImportSession } from "../lib/importSession";
import type { ResumeDecision } from "./import/resumeDecision";

const hookState = vi.hoisted(() => ({ phase: "form" as "form" | "progress" | "done" }));
const startImportMock = vi.hoisted(() => vi.fn());
const cancelMock = vi.hoisted(() => vi.fn());
const returnToFormMock = vi.hoisted(() => vi.fn());
const getActiveImportSessionMock = vi.hoisted(() => vi.fn());
const discardImportSessionMock = vi.hoisted(() => vi.fn());
const invokePathStatMock = vi.hoisted(() => vi.fn());

vi.mock("./import/useImportJob", async (importOriginal) => {
  // restoreFormFromSnapshot is real: it's pure, already unit-tested on its
  // own, and using it here exercises the same validation the screen relies
  // on. Only useImportJob itself is replaced.
  const actual = await importOriginal<typeof import("./import/useImportJob")>();
  return {
    ...actual,
    useImportJob: () => ({
      phase: hookState.phase,
      steps: [],
      running: false,
      summaryView: null,
      stagingDir: null,
      completionText: undefined,
      startImport: startImportMock,
      cancel: cancelMock,
      returnToForm: returnToFormMock,
    }),
  };
});

vi.mock("../lib/importSession", () => ({
  getActiveImportSession: (...args: unknown[]) => getActiveImportSessionMock(...args),
  discardImportSession: (...args: unknown[]) => discardImportSessionMock(...args),
}));

vi.mock("../lib/deviceId", () => ({
  getDeviceId: () => "this-device",
}));

vi.mock("../lib/tauri", () => ({
  invokeHomeDir: vi.fn().mockResolvedValue({ path: "/home/u", os: "linux" }),
  invokeIosBackupEncrypted: vi.fn().mockResolvedValue(null),
  invokePathStat: (...args: unknown[]) => invokePathStatMock(...args),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

vi.mock("./import/ImportFormFields", () => ({
  default: () => <div data-testid="import-form" />,
}));

vi.mock("./import/ImportProgressView", () => ({
  default: () => <div data-testid="import-progress" />,
}));

vi.mock("./import/ResumeImportPanel", () => ({
  default: (props: { decision: ResumeDecision; onResume: () => void; onDiscard: () => void }) => (
    <div data-testid="resume-panel">
      <span data-testid="resume-kind">{props.decision.kind}</span>
      <button type="button" onClick={props.onResume}>
        resume-action
      </button>
      <button type="button" onClick={props.onDiscard}>
        discard-action
      </button>
    </div>
  ),
}));

const { default: ImportScreen } = await import("./ImportScreen");

function session(overrides: Partial<ActiveImportSession> = {}): ActiveImportSession {
  return {
    id: 7,
    source: "imessage",
    mode: "append",
    status: "running",
    started_at: "2026-08-30T00:00:00Z",
    stage: "pushing",
    staging_dir: "/home/u/message-vault/staging-260830",
    device_id: "this-device",
    form: { source: "imessage-ios" },
    source_fingerprint: null,
    ...overrides,
  };
}

/** A promise plus the functions to settle it later, for controlling when the check resolves. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("ImportScreen entering Import", () => {
  beforeEach(() => {
    hookState.phase = "form";
    startImportMock.mockReset();
    cancelMock.mockReset();
    returnToFormMock.mockReset();
    getActiveImportSessionMock.mockReset();
    discardImportSessionMock.mockReset();
    discardImportSessionMock.mockResolvedValue(undefined);
    invokePathStatMock.mockReset();
    invokePathStatMock.mockResolvedValue({ exists: true, isFile: false, isDirectory: true });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders neither the form nor a panel while the active-session check is in flight", async () => {
    const pending = deferred<ActiveImportSession | null>();
    getActiveImportSessionMock.mockReturnValue(pending.promise);

    render(<ImportScreen />);

    expect(screen.queryByTestId("import-form")).not.toBeInTheDocument();
    expect(screen.queryByTestId("resume-panel")).not.toBeInTheDocument();

    await act(async () => {
      pending.resolve(null);
      await pending.promise;
    });

    expect(screen.getByTestId("import-form")).toBeInTheDocument();
  });

  it("shows the form when there is no active session", async () => {
    getActiveImportSessionMock.mockResolvedValue(null);
    render(<ImportScreen />);

    expect(await screen.findByTestId("import-form")).toBeInTheDocument();
    expect(screen.queryByTestId("resume-panel")).not.toBeInTheDocument();
  });

  it("falls through to the form when the vault cannot answer", async () => {
    getActiveImportSessionMock.mockRejectedValue(new Error("network down"));
    render(<ImportScreen />);

    expect(await screen.findByTestId("import-form")).toBeInTheDocument();
    expect(screen.queryByTestId("resume-panel")).not.toBeInTheDocument();
  });

  it("shows the resume panel instead of the form for a resumable session", async () => {
    getActiveImportSessionMock.mockResolvedValue(session({ stage: "pushing" }));
    render(<ImportScreen />);

    expect(await screen.findByTestId("resume-panel")).toBeInTheDocument();
    expect(screen.getByTestId("resume-kind")).toHaveTextContent("resume_push");
    expect(screen.queryByTestId("import-form")).not.toBeInTheDocument();
  });

  it("discards the session and drops through to the form", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(session({ stage: "pushing" }));
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    await user.click(screen.getByText("discard-action"));

    expect(discardImportSessionMock).toHaveBeenCalledWith(7);
    expect(await screen.findByTestId("import-form")).toBeInTheDocument();
  });

  it("resumes the push against the existing session without creating a new one", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(
      session({
        stage: "pushing",
        staging_dir: "/home/u/message-vault/staging-260830",
        form: {
          source: "imessage-ios",
          backupPath: "/backups/iphone.tar",
          attachmentMedia: "copy",
          maxResolution: "720p",
          maxFps: "30",
          minSizeMb: "20",
          contactNameMode: "fill_missing",
          ownerPhones: [],
          force: false,
          obfuscate: false,
          isSbr: false,
          attachmentRoot: "",
          appleContacts: "",
          whatsappWa: "",
          whatsappMedia: "",
          whatsappDb: "",
          whatsappBusiness: false,
        },
      }),
    );
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    await user.click(screen.getByText("resume-action"));

    expect(startImportMock).toHaveBeenCalledTimes(1);
    const [form, resume] = startImportMock.mock.calls[0] as [unknown, unknown];
    expect(form).toMatchObject({ source: "imessage-ios", backupPath: "/backups/iphone.tar" });
    expect(resume).toEqual({ sessionId: 7, stagingDir: "/home/u/message-vault/staging-260830" });
    expect(discardImportSessionMock).not.toHaveBeenCalled();
  });

  it("discards the old session before restarting when the extract never finished", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(
      session({
        stage: "write",
        form: {
          source: "imessage-ios",
          backupPath: "/backups/iphone.tar",
          attachmentMedia: "copy",
          maxResolution: "720p",
          maxFps: "30",
          minSizeMb: "20",
          contactNameMode: "fill_missing",
          ownerPhones: [],
          force: false,
          obfuscate: false,
          isSbr: false,
          attachmentRoot: "",
          appleContacts: "",
          whatsappWa: "",
          whatsappMedia: "",
          whatsappDb: "",
          whatsappBusiness: false,
        },
      }),
    );
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    expect(screen.getByTestId("resume-kind")).toHaveTextContent("restart");
    await user.click(screen.getByText("resume-action"));

    expect(discardImportSessionMock).toHaveBeenCalledWith(7);
    expect(startImportMock).toHaveBeenCalledTimes(1);
    const [form, resume] = startImportMock.mock.calls[0] as [unknown, unknown];
    expect(form).toMatchObject({ source: "imessage-ios", backupPath: "/backups/iphone.tar" });
    expect(resume).toBeUndefined();
  });

  it("re-checks for an open session when the screen returns to the form", async () => {
    // A swallowed final /complete, or a restart whose discard failed,
    // leaves a session open server-side that the screen has forgotten. If
    // Back never re-checks, the user gets a form whose Import button 409s.
    getActiveImportSessionMock.mockResolvedValue(null);
    const { rerender } = render(<ImportScreen />);

    expect(await screen.findByTestId("import-form")).toBeInTheDocument();
    expect(getActiveImportSessionMock).toHaveBeenCalledTimes(1);

    hookState.phase = "progress";
    await act(async () => {
      rerender(<ImportScreen />);
    });
    expect(getActiveImportSessionMock).toHaveBeenCalledTimes(1);

    hookState.phase = "done";
    await act(async () => {
      rerender(<ImportScreen />);
    });
    expect(getActiveImportSessionMock).toHaveBeenCalledTimes(1);

    getActiveImportSessionMock.mockResolvedValue(session({ stage: "pushing" }));
    hookState.phase = "form";
    await act(async () => {
      rerender(<ImportScreen />);
    });

    expect(getActiveImportSessionMock).toHaveBeenCalledTimes(2);
    expect(await screen.findByTestId("resume-panel")).toBeInTheDocument();
    expect(screen.getByTestId("resume-kind")).toHaveTextContent("resume_push");
  });

  it("runs one restart when the resume action is double-clicked", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(
      session({
        stage: "write",
        form: {
          source: "imessage-ios",
          backupPath: "/backups/iphone.tar",
          attachmentMedia: "copy",
          maxResolution: "720p",
          maxFps: "30",
          minSizeMb: "20",
          contactNameMode: "fill_missing",
          ownerPhones: [],
          force: false,
          obfuscate: false,
          isSbr: false,
          attachmentRoot: "",
          appleContacts: "",
          whatsappWa: "",
          whatsappMedia: "",
          whatsappDb: "",
          whatsappBusiness: false,
        },
      }),
    );
    // The panel stays mounted across this round trip by design, so the
    // second click lands on a live button.
    const pendingDiscard = deferred<void>();
    discardImportSessionMock.mockReturnValue(pendingDiscard.promise);
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    await user.click(screen.getByText("resume-action"));
    await user.click(screen.getByText("resume-action"));
    await user.click(screen.getByText("discard-action"));

    expect(discardImportSessionMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      pendingDiscard.resolve();
      await pendingDiscard.promise;
    });

    expect(discardImportSessionMock).toHaveBeenCalledTimes(1);
    expect(startImportMock).toHaveBeenCalledTimes(1);
  });

  it("falls back to a settings-unreadable panel when the stored form snapshot is malformed", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(
      session({ stage: "pushing", form: { nonsense: true } }),
    );
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    expect(screen.getByTestId("resume-kind")).toHaveTextContent("resume_push");
    await user.click(screen.getByText("resume-action"));

    expect(startImportMock).not.toHaveBeenCalled();
    expect(await screen.findByTestId("resume-kind")).toHaveTextContent("settings_unreadable");
  });

  it("still drops to the form when discarding from the panel fails server-side", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(session({ stage: "pushing" }));
    discardImportSessionMock.mockRejectedValue(new Error("network down"));
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    await user.click(screen.getByText("discard-action"));

    expect(discardImportSessionMock).toHaveBeenCalledWith(7);
    expect(await screen.findByTestId("import-form")).toBeInTheDocument();
  });

  it("still restarts when discarding the old session before a restart fails server-side", async () => {
    const user = userEvent.setup();
    getActiveImportSessionMock.mockResolvedValue(
      session({
        stage: "write",
        form: {
          source: "imessage-ios",
          backupPath: "/backups/iphone.tar",
          attachmentMedia: "copy",
          maxResolution: "720p",
          maxFps: "30",
          minSizeMb: "20",
          contactNameMode: "fill_missing",
          ownerPhones: [],
          force: false,
          obfuscate: false,
          isSbr: false,
          attachmentRoot: "",
          appleContacts: "",
          whatsappWa: "",
          whatsappMedia: "",
          whatsappDb: "",
          whatsappBusiness: false,
        },
      }),
    );
    discardImportSessionMock.mockRejectedValue(new Error("network down"));
    render(<ImportScreen />);

    await screen.findByTestId("resume-panel");
    await user.click(screen.getByText("resume-action"));

    expect(discardImportSessionMock).toHaveBeenCalledWith(7);
    expect(startImportMock).toHaveBeenCalledTimes(1);
    const [form, resume] = startImportMock.mock.calls[0] as [unknown, unknown];
    expect(form).toMatchObject({ source: "imessage-ios", backupPath: "/backups/iphone.tar" });
    expect(resume).toBeUndefined();
  });
});
