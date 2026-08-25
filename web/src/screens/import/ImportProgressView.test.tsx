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

  it("shows staging directory and vault-push.log links above Parse backup", () => {
    const staging = "/home/sam/message-vault/staging-iphone-ios-260824-180509";
    render(
      <ImportProgressView
        phase="progress"
        steps={[
          { label: "Parse backup", status: "active", detail: "Extracting…" },
          { label: "Copy attachments", status: "pending" },
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
    const parseBackup = screen.getByText("Parse backup");
    expect(
      stagingLink.compareDocumentPosition(parseBackup) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("opens staging folder and log when the links are clicked", async () => {
    const user = userEvent.setup();
    const staging = "/home/sam/message-vault/staging-test";
    render(
      <ImportProgressView
        phase="progress"
        steps={[{ label: "Parse backup", status: "active" }]}
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

  it("keeps Import Errors heading and table when issues exist", () => {
    render(
      <ImportProgressView
        phase="done"
        steps={[
          { label: "Parse backup", status: "done", durationMs: 1000 },
          { label: "Copy attachments", status: "done", durationMs: 500 },
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
