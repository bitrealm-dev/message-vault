/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const login = vi.fn();
const setServer = vi.fn();

vi.mock("../lib/auth", () => ({
  useAuth: () => ({ login, setServer, serverUrl: "" }),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

import LoginScreen from "./LoginScreen";

/** Answer `/health` as a healthy vault. Returns the underlying fetch mock. */
function stubVault() {
  const fetchMock = vi.fn(async (_url: string, _init?: RequestInit) => ({
    ok: true,
    text: async () => "",
  }));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function renderScreen() {
  render(
    <MemoryRouter>
      <LoginScreen />
    </MemoryRouter>,
  );
}

describe("LoginScreen", () => {
  beforeEach(() => {
    login.mockReset();
    setServer.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("signs in without a vault-selection step", async () => {
    stubVault();
    renderScreen();

    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Username" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();

    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Server URL" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Back to Vault Selection" }),
    ).not.toBeInTheDocument();
  });

  it("names the vault it connected to", async () => {
    stubVault();
    renderScreen();

    expect(await screen.findByText("connected")).toBeInTheDocument();
    expect(setServer).toHaveBeenCalledWith("");
  });

  it("probes /health rather than the auth mode endpoint", async () => {
    const fetchMock = stubVault();
    renderScreen();

    await waitFor(() => {
      expect(screen.getByText(/connected/i)).toBeInTheDocument();
    });

    const calls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(calls.some((url) => url.endsWith("/health"))).toBe(true);
    expect(calls.some((url) => url.endsWith("/v1/auth/mode"))).toBe(false);
  });

  it("keeps both tabs, Login first", async () => {
    stubVault();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Login", "Create Account"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
  });

  it("still asks for the password twice on Create Account", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Create Account" });
    await user.click(screen.getByRole("tab", { name: "Create Account" }));

    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByLabelText("Confirm Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create account" })).toBeInTheDocument();
  });

  it("rejects a new account when the two passwords disagree", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Create Account" });
    await user.click(screen.getByRole("tab", { name: "Create Account" }));
    await user.type(screen.getByRole("textbox", { name: "Username" }), "ada");
    await user.type(screen.getByLabelText("Password"), "hunter22");
    await user.type(screen.getByLabelText("Confirm Password"), "hunter23");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByText("Passwords do not match.")).toBeInTheDocument();
    expect(login).not.toHaveBeenCalled();
  });

  it("offers the address field when nothing answers", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    renderScreen();

    expect(await screen.findByText("disconnected")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Vault address" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in" })).not.toBeInTheDocument();
  });

  it("retries against a typed address", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const user = userEvent.setup();
    renderScreen();

    await screen.findByText("disconnected");

    stubVault();
    const field = screen.getByRole("textbox", { name: "Vault address" });
    await user.clear(field);
    await user.type(field, "http://127.0.0.1:8080");
    await user.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
    await waitFor(() => {
      expect(setServer).toHaveBeenCalledWith("http://127.0.0.1:8080");
    });
  });

  it("opens the address field from Change without losing the form", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    await user.click(screen.getByRole("button", { name: "Change" }));

    expect(screen.getByRole("textbox", { name: "Vault address" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Login" })).toBeInTheDocument();
  });

  it("reconnects on its own once a probe finds the vault healthy again", async () => {
    let healthy = false;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        return healthy ? { ok: true, text: async () => "" } : { ok: false, status: 503 };
      }),
    );
    renderScreen();

    await screen.findByText("disconnected");
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();

    healthy = true;

    expect(
      await screen.findByRole("tab", { name: "Login" }, { timeout: 3000 }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("carries an abort signal on the health probe", async () => {
    const fetchMock = stubVault();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    const healthCall = fetchMock.mock.calls.find(([url]) => String(url).endsWith("/health"));
    expect(healthCall).toBeDefined();
    expect(healthCall?.[1]).toMatchObject({ signal: expect.any(AbortSignal) });
  });

  it("keeps the tabs rendered, dimmed, when a connected vault drops", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByText("connected");
    await user.click(screen.getByRole("button", { name: "Change" }));

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const field = screen.getByRole("textbox", { name: "Vault address" });
    await user.clear(field);
    await user.type(field, "http://127.0.0.1:9999");
    await user.click(screen.getByRole("button", { name: "Use" }));

    expect(await screen.findByText("disconnected")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Login" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Username" })).toBeInTheDocument();
  });
});
