/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import VaultLine, { type VaultLineProps } from "./VaultLine";

afterEach(cleanup);

function renderLine(overrides: Partial<VaultLineProps> = {}) {
  const props: VaultLineProps = {
    state: "connected",
    host: "vault.bitrealm.io",
    draft: "https://vault.bitrealm.io",
    health: "ok",
    onDraftChange: vi.fn(),
    onEdit: vi.fn(),
    onCancel: vi.fn(),
    onSubmit: vi.fn(),
    ...overrides,
  };
  render(<VaultLine {...props} />);
  return props;
}

describe("VaultLine", () => {
  it("names the host and says it is connected", () => {
    renderLine();
    expect(screen.getByText("vault.bitrealm.io")).toBeInTheDocument();
    expect(screen.getByText("connected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Change" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Vault address" })).not.toBeInTheDocument();
  });

  it("says connecting while a probe is in flight, with no way to change yet", () => {
    renderLine({ state: "connecting", health: "checking" });
    expect(screen.getByText("connecting…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change" })).not.toBeInTheDocument();
  });

  it("announces the connection status word politely", () => {
    renderLine();
    expect(screen.getByText("connected")).toHaveAttribute("aria-live", "polite");
  });

  it("opens the address field when Change is pressed", async () => {
    const user = userEvent.setup();
    const props = renderLine();
    await user.click(screen.getByRole("button", { name: "Change" }));
    expect(props.onEdit).toHaveBeenCalledOnce();
  });

  it("offers the address and Use while editing", async () => {
    const user = userEvent.setup();
    const props = renderLine({ state: "editing", health: "checking" });

    expect(screen.getByRole("textbox", { name: "Vault address" })).toHaveValue(
      "https://vault.bitrealm.io",
    );
    await user.click(screen.getByRole("button", { name: "Use" }));
    expect(props.onSubmit).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(props.onCancel).toHaveBeenCalledOnce();
  });

  it("says disconnected and offers Retry with a way forward", () => {
    renderLine({ state: "disconnected", host: "127.0.0.1:8080", health: "fail" });
    expect(screen.getByText("127.0.0.1:8080")).toBeInTheDocument();
    expect(screen.getByText("disconnected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.getByText("Start your vault, or enter another address.")).toBeInTheDocument();
  });

  it("tracks the live health value while disconnected, not the card state", () => {
    renderLine({ state: "disconnected", health: "ok" });
    expect(screen.getByText("connected")).toBeInTheDocument();
    expect(screen.queryByText("disconnected")).not.toBeInTheDocument();
  });

  it("tracks the live health value while editing, not a hard-coded connecting", () => {
    renderLine({ state: "editing", health: "ok" });
    expect(screen.getByText("connected")).toBeInTheDocument();
    cleanup();

    renderLine({ state: "editing", health: "fail" });
    expect(screen.getByText("disconnected")).toBeInTheDocument();
  });
});
