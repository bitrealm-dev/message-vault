/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import VaultStatus from "./VaultStatus";

describe("VaultStatus", () => {
  afterEach(cleanup);

  it("says the state in one word", () => {
    const { rerender } = render(<VaultStatus state="connecting" />);
    expect(screen.getByText("Connecting")).toBeInTheDocument();

    rerender(<VaultStatus state="connected" />);
    expect(screen.getByText("Connected")).toBeInTheDocument();

    rerender(<VaultStatus state="disconnected" />);
    expect(screen.getByText("Disconnected")).toBeInTheDocument();

    rerender(<VaultStatus state="untested" />);
    expect(screen.getByText("Not tested")).toBeInTheDocument();
  });

  it("keeps an untested address out of the answered colours", () => {
    render(<VaultStatus state="untested" />);
    const status = screen.getByRole("status");
    expect(status).toHaveClass("text-muted");
    expect(status).not.toHaveClass("text-ok");
    expect(status).not.toHaveClass("text-danger");
    expect(status).not.toHaveClass("motion-safe:animate-pulse");
  });

  it("colours the word to agree with what it says", () => {
    const { rerender } = render(<VaultStatus state="connected" />);
    expect(screen.getByRole("status")).toHaveClass("text-ok");

    rerender(<VaultStatus state="disconnected" />);
    expect(screen.getByRole("status")).toHaveClass("text-danger");
  });

  it("flashes only while connecting", () => {
    const { rerender } = render(<VaultStatus state="connecting" />);
    expect(screen.getByRole("status")).toHaveClass("motion-safe:animate-pulse");

    rerender(<VaultStatus state="connected" />);
    expect(screen.getByRole("status")).not.toHaveClass("motion-safe:animate-pulse");
  });

  it("announces changes to a screen reader", () => {
    render(<VaultStatus state="connecting" />);
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("carries a caller's own placement classes", () => {
    render(<VaultStatus state="connected" className="pl-[13px]" />);
    expect(screen.getByRole("status")).toHaveClass("pl-[13px]");
  });
});
