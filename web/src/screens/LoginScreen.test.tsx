/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const login = vi.fn();

vi.mock("../lib/auth", () => ({
  useAuth: () => ({
    login,
    setServer: vi.fn(),
    serverUrl: "",
  }),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

import LoginScreen from "./LoginScreen";

/** Answer `/health` and `/v1/auth/mode` so Connect reaches the local auth card. */
function stubLocalModeServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (String(url).endsWith("/v1/auth/mode")) {
        return { ok: true, json: async () => ({ mode: "local" }) };
      }
      return { ok: true };
    }),
  );
}

async function connect(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Connect" }));
  await screen.findByRole("tab", { name: "Login" });
}

describe("LoginScreen", () => {
  beforeEach(() => {
    login.mockReset();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true }));
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("puts Connect first and offers no demo sign-in", () => {
    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    expect(screen.getByRole("heading", { name: "Message Vault" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connect" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try it" })).not.toBeInTheDocument();
    expect(screen.queryByText("OR")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Extract messages" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Format conversion" })).not.toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Connecting…" })).toBeInTheDocument();
  });

  it("shows Login and Create Account tabs after connecting, with Login first", async () => {
    stubLocalModeServer();
    const user = userEvent.setup();

    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    await connect(user);

    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Login", "Create Account"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");

    expect(screen.getByRole("textbox", { name: "Username" })).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Confirm Password")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try it" })).not.toBeInTheDocument();
  });

  it("asks for the password twice on the Create Account tab", async () => {
    stubLocalModeServer();
    const user = userEvent.setup();

    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    await connect(user);
    await user.click(screen.getByRole("tab", { name: "Create Account" }));

    expect(screen.getByRole("textbox", { name: "Username" })).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByLabelText("Confirm Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create account" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in" })).not.toBeInTheDocument();
  });

  it("rejects a new account when the two passwords disagree", async () => {
    stubLocalModeServer();
    const user = userEvent.setup();

    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    await connect(user);
    await user.click(screen.getByRole("tab", { name: "Create Account" }));

    await user.type(screen.getByRole("textbox", { name: "Username" }), "ada");
    await user.type(screen.getByLabelText("Password"), "hunter2");
    await user.type(screen.getByLabelText("Confirm Password"), "hunter3");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByText("Passwords do not match.")).toBeInTheDocument();
    expect(login).not.toHaveBeenCalled();
  });

  it("turns the Server URL light green for a blank URL when this origin is up", async () => {
    render(
      <MemoryRouter>
        <LoginScreen />
      </MemoryRouter>,
    );

    await waitFor(
      () => {
        expect(screen.getByRole("status", { name: "Connected" })).toBeInTheDocument();
      },
      { timeout: 2000 },
    );

    expect(fetch).toHaveBeenCalledWith(
      "/health",
      expect.objectContaining({ method: "GET", cache: "no-store" }),
    );
  });

  it("turns the Server URL light green when /health succeeds", async () => {
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
        expect(screen.getByRole("status", { name: "Connected" })).toBeInTheDocument();
      },
      { timeout: 2000 },
    );

    expect(fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8080/health",
      expect.objectContaining({ method: "GET", cache: "no-store" }),
    );
  });

  it("turns the Server URL light red when /health fails", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 503 }));
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
        expect(screen.getByRole("status", { name: "Disconnected" })).toBeInTheDocument();
      },
      { timeout: 2000 },
    );
  });
});
