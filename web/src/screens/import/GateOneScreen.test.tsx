/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StagingSummary } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";
import GateOneScreen from "./GateOneScreen";

afterEach(() => {
  cleanup();
});

function summary(overrides: Partial<StagingSummary> = {}): StagingSummary {
  return {
    conversations: 1,
    messages: 1,
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

function props(
  overrides: {
    summary?: Partial<StagingSummary>;
    unknownContacts?: number;
    mode?: AttachmentMediaMode;
    onApprove?: () => void;
    onDecline?: () => void;
    busy?: boolean;
  } = {},
) {
  return {
    summary: summary(overrides.summary),
    unknownContacts: overrides.unknownContacts ?? 0,
    mode: overrides.mode ?? "convert",
    onApprove: overrides.onApprove ?? vi.fn(),
    onDecline: overrides.onDecline ?? vi.fn(),
    busy: overrides.busy ?? false,
  };
}

describe("GateOneScreen", () => {
  it("names the stage in its heading", () => {
    render(<GateOneScreen {...props()} />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Review what was copied");
  });

  it("shows the measured counts", () => {
    render(
      <GateOneScreen
        {...props({
          summary: {
            conversations: 12,
            messages: 4310,
            attachments: 88,
            attachmentBytes: 1024 * 1024 * 512,
          },
        })}
      />,
    );
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.getByText("4,310")).toBeInTheDocument();
  });

  it("says how many contacts are new to the vault", () => {
    render(<GateOneScreen {...props({ unknownContacts: 7 })} />);
    expect(screen.getByText(/7 new to your vault/)).toBeInTheDocument();
  });

  it("says the size numbers are estimates", () => {
    // The screen says throughout that these are estimates (decision 13).
    render(<GateOneScreen {...props({ mode: "convert" })} />);
    expect(screen.getByText(/estimate/i)).toBeInTheDocument();
  });

  it("offers to start the media step under convert", () => {
    render(<GateOneScreen {...props({ mode: "convert" })} />);
    expect(screen.getByRole("button", { name: "Convert media" })).toBeInTheDocument();
  });

  it("offers to upload directly under copy, because there is no media step", () => {
    render(<GateOneScreen {...props({ mode: "copy" })} />);
    expect(screen.getByRole("button", { name: "Upload to vault" })).toBeInTheDocument();
    expect(screen.queryByText(/estimate/i)).not.toBeInTheDocument();
  });

  it("does not act twice on a double click", () => {
    const onApprove = vi.fn();
    render(<GateOneScreen {...props({ onApprove, busy: true })} />);
    fireEvent.click(screen.getByRole("button", { name: /Convert media|Upload to vault/ }));
    expect(onApprove).not.toHaveBeenCalled();
  });

  it("does not act twice on a double click of the decline button either", () => {
    // The brief only pins this for approve; busy gates both buttons, so a
    // double-click on cancel must be a no-op too (decline callback never runs).
    const onDecline = vi.fn();
    render(<GateOneScreen {...props({ onDecline, busy: true })} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel this import" }));
    expect(onDecline).not.toHaveBeenCalled();
  });

  it("offers to cancel the import", () => {
    render(<GateOneScreen {...props()} />);
    expect(screen.getByRole("button", { name: "Cancel this import" })).toBeInTheDocument();
  });
});
