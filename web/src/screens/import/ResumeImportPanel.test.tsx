/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ActiveImportSession } from "../../lib/importSession";
import ResumeImportPanel from "./ResumeImportPanel";
import type { ResumeDecision } from "./resumeDecision";

afterEach(() => {
  cleanup();
});

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

describe("ResumeImportPanel", () => {
  it("renders nothing when there is no session to decide about", () => {
    const decision: ResumeDecision = { kind: "none", canResume: false, session: null };
    const { container } = render(
      <ResumeImportPanel decision={decision} onResume={vi.fn()} onDiscard={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("offers to resume the upload and calls back on the user's choice", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    const onDiscard = vi.fn();
    const decision: ResumeDecision = { kind: "resume_push", canResume: true, session: session() };
    render(<ResumeImportPanel decision={decision} onResume={onResume} onDiscard={onDiscard} />);

    expect(screen.getByText("Finish your last import")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Your messages are staged and ready to upload. Picking up where you left off skips the extract.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Upload to vault" }));
    expect(onResume).toHaveBeenCalledTimes(1);
    expect(onDiscard).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Discard this import" }));
    expect(onDiscard).toHaveBeenCalledTimes(1);
  });

  it("offers to start over when the extract never finished", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    const onDiscard = vi.fn();
    const decision: ResumeDecision = {
      kind: "restart",
      canResume: false,
      session: session({ stage: "write" }),
    };
    render(<ResumeImportPanel decision={decision} onResume={onResume} onDiscard={onDiscard} />);

    expect(screen.getByText("Pick up your last import")).toBeInTheDocument();
    expect(
      screen.getByText(
        "The extract did not finish. Starting again reuses your settings and reads the backup from the beginning.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Start over" }));
    expect(onResume).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Discard this import" }));
    expect(onDiscard).toHaveBeenCalledTimes(1);
  });

  it("offers discard alone, naming the path, when the staged files are gone", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    const onDiscard = vi.fn();
    const decision: ResumeDecision = {
      kind: "folder_missing",
      canResume: false,
      session: session({ staging_dir: "/home/u/message-vault/staging-260830" }),
    };
    render(<ResumeImportPanel decision={decision} onResume={onResume} onDiscard={onDiscard} />);

    expect(screen.getByText("The staged files are gone")).toBeInTheDocument();
    expect(
      screen.getByText(
        "This import's folder is no longer at /home/u/message-vault/staging-260830. Discarding it lets you start a new one.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Upload to vault" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start over" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Discard this import" }));
    expect(onDiscard).toHaveBeenCalledTimes(1);
    expect(onResume).not.toHaveBeenCalled();
  });

  it("offers discard alone when the session belongs to another install", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    const onDiscard = vi.fn();
    const decision: ResumeDecision = {
      kind: "other_device",
      canResume: false,
      session: session({ device_id: "other-device" }),
    };
    render(<ResumeImportPanel decision={decision} onResume={onResume} onDiscard={onDiscard} />);

    expect(screen.getByText("This import belongs to another computer")).toBeInTheDocument();
    expect(
      screen.getByText(
        "It was started on a different install and its files are staged there. Discarding it lets you start a new import here.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Discard this import" }));
    expect(onDiscard).toHaveBeenCalledTimes(1);
    expect(onResume).not.toHaveBeenCalled();
  });
});
