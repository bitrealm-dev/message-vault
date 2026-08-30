/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import ImportProgressView from "./ImportProgressView";

const openPathInExplorer = vi.fn();

vi.mock("../../lib/openPath", () => ({
  openPathInExplorer: (...args: unknown[]) => openPathInExplorer(...args),
}));

describe("ImportProgressView", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    openPathInExplorer.mockReset();
    openPathInExplorer.mockResolvedValue(undefined);
  });

  it("shows staging directory and vault-push.log links above Read backup", () => {
    const staging = "/home/sam/message-vault/staging-iphone-ios-260824-180509";
    render(
      <ImportProgressView
        phase="progress"
        steps={[
          { label: "Read backup", status: "active", detail: "Reading backup…" },
          { label: "Copy to staging", status: "pending" },
          { label: "Upload to vault", status: "pending" },
        ]}
        running
        summaryView={null}
        stagingDir={staging}
        onCancel={() => {}}
        onBack={() => {}}
      />,
    );

    expect(screen.getByText("Staging directory")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: staging })).toBeInTheDocument();
    expect(screen.getByText("Import log")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "vault-push.log" })).toBeInTheDocument();
    expect(screen.queryByText("Open import log")).not.toBeInTheDocument();

    const stagingLink = screen.getByRole("button", { name: staging });
    const readBackup = screen.getByText("Read backup");
    expect(
      stagingLink.compareDocumentPosition(readBackup) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("titles the screen by the active step, not a fixed heading", () => {
    render(
      <ImportProgressView
        phase="progress"
        steps={[
          { label: "Read backup", status: "done" },
          { label: "Copy to staging", status: "active" },
          { label: "Upload to vault", status: "pending" },
        ]}
        running
        summaryView={null}
        stagingDir={null}
        onCancel={() => {}}
        onBack={() => {}}
      />,
    );

    expect(screen.getByRole("heading", { name: "Copying to staging" })).toBeInTheDocument();
  });

  it("titles the finished screen by its outcome, not a step", () => {
    render(
      <ImportProgressView
        phase="done"
        steps={[
          { label: "Read backup", status: "done" },
          { label: "Copy to staging", status: "done" },
          { label: "Upload to vault", status: "done" },
        ]}
        running={false}
        summaryView={null}
        stagingDir={null}
        completionText="Import complete"
        onCancel={() => {}}
        onBack={() => {}}
      />,
    );

    expect(screen.getByRole("heading", { name: "Import finished" })).toBeInTheDocument();
  });

  it("opens staging folder and log when the links are clicked", async () => {
    const user = userEvent.setup();
    const staging = "/home/sam/message-vault/staging-test";
    render(
      <ImportProgressView
        phase="progress"
        steps={[{ label: "Read backup", status: "active" }]}
        running
        summaryView={null}
        stagingDir={staging}
        onCancel={() => {}}
        onBack={() => {}}
      />,
    );

    await user.click(screen.getByRole("button", { name: staging }));
    expect(openPathInExplorer).toHaveBeenCalledWith(staging);

    await user.click(screen.getByRole("button", { name: "vault-push.log" }));
    expect(openPathInExplorer).toHaveBeenCalledWith(`${staging}/vault-push.log`);
  });

  it("disables Cancel, without hiding it, while a not-cancellable step runs", () => {
    render(
      <ImportProgressView
        phase="progress"
        steps={[{ label: "Copy to staging", status: "done" }]}
        running
        summaryView={null}
        stagingDir={null}
        onCancel={() => {}}
        onBack={() => {}}
        cancelDisabled
      />,
    );

    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("keeps Import Errors heading and table when issues exist", () => {
    render(
      <ImportProgressView
        phase="done"
        steps={[
          { label: "Read backup", status: "done", durationMs: 1000 },
          { label: "Copy to staging", status: "done", durationMs: 550 },
          { label: "Upload to vault", status: "done", durationMs: 2000 },
        ]}
        running={false}
        summaryView={{
          status: "completed",
          messagesParsed: 10,
          messagesAttempted: 10,
          messagesInserted: 9,
          messagesDeduped: 1,
          messagesFailed: 0,
          durationMs: 3500,
          issues: [
            {
              kind: "warn",
              step: "upload",
              item: "chat.jsonl",
              reason: "Skipped one attachment",
            },
          ],
        }}
        stagingDir="/home/sam/message-vault/staging-test"
        completionText="Import complete"
        onCancel={() => {}}
        onBack={() => {}}
      />,
    );

    expect(screen.getByRole("heading", { name: "Import Errors" })).toBeInTheDocument();
    expect(screen.getByLabelText("Import errors")).toBeInTheDocument();
    expect(screen.queryByText("Open import log")).not.toBeInTheDocument();
  });
});
