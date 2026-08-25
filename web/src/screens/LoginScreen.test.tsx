/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/auth", () => ({
  useAuth: () => ({
    login: vi.fn(),
    setServer: vi.fn(),
    serverUrl: "",
  }),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

import LoginScreen from "./LoginScreen";

describe("LoginScreen", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("puts Connect first, keeps Try it disabled, and hides extract/format tools", () => {
    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    expect(screen.getByRole("heading", { name: "Message Vault" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connect" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try it" })).toBeDisabled();
    expect(screen.getByText("Sample sign-in is temporarily unavailable.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Extract messages" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Format conversion" })).not.toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Server status unknown" })).toBeInTheDocument();
  });

  it("turns the Server URL light green when /health succeeds", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    const input = screen.getByRole("textbox", { name: "Server URL" });
    await user.clear(input);
    await user.type(input, "http://127.0.0.1:8080");

    await waitFor(
      () => {
        expect(screen.getByRole("status", { name: "Server reachable" })).toBeInTheDocument();
      },
      { timeout: 2000 },
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:8080/health",
      expect.objectContaining({ method: "GET", cache: "no-store" }),
    );
  });

  it("turns the Server URL light red when /health fails", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 503 });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    const input = screen.getByRole("textbox", { name: "Server URL" });
    await user.type(input, "http://127.0.0.1:9999");

    await waitFor(
      () => {
        expect(screen.getByRole("status", { name: "Server unreachable" })).toBeInTheDocument();
      },
      { timeout: 2000 },
    );
  });
});
