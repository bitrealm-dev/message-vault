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

  it("logs in without a vault-selection step", async () => {
    stubVault();
    renderScreen();

    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Username" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Log in" })).toBeInTheDocument();

    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Server URL" })).not.toBeInTheDocument();
  });

  it("names the product and reports the connection as one word", async () => {
    stubVault();
    renderScreen();

    expect(await screen.findByText("Connected")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Message Vault" })).toBeInTheDocument();
    expect(setServer).toHaveBeenCalledWith("");
  });

  it("never shows the vault's host address", async () => {
    stubVault();
    renderScreen();

    await screen.findByText("Connected");
    expect(screen.queryByText(/127\.0\.0\.1/)).not.toBeInTheDocument();
    expect(screen.queryByText(/localhost/)).not.toBeInTheDocument();
  });

  it("probes /health rather than the auth mode endpoint", async () => {
    const fetchMock = stubVault();
    renderScreen();

    await screen.findByText("Connected");

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

  it("drops the password-length claim the server does not enforce", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Create Account" });
    await user.click(screen.getByRole("tab", { name: "Create Account" }));

    expect(screen.queryByText(/At least 8 characters/i)).not.toBeInTheDocument();
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

  it("says Disconnected when nothing answers", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    renderScreen();

    expect(await screen.findByText("Disconnected")).toBeInTheDocument();
    // The address field belongs to the settings screen now, not the card.
    expect(screen.queryByRole("textbox", { name: "Address" })).not.toBeInTheDocument();
  });

  it("keeps the way out of a red card live", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    renderScreen();

    await screen.findByText("Disconnected");
    expect(screen.getByRole("button", { name: "Change vault settings" })).toBeEnabled();
  });

  it("disables Log in while the vault is unreachable", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByText("Connected");
    await user.click(screen.getByRole("button", { name: "Change vault settings" }));

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const field = screen.getByRole("textbox", { name: "Address" });
    await user.clear(field);
    await user.type(field, "http://127.0.0.1:9999");
    await user.click(screen.getByRole("button", { name: "Change vault address" }));

    expect(await screen.findByText("Disconnected")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Login" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Log in" })).toBeDisabled();
  });

  it("opens Message Vault Settings from the link and comes back on Cancel", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    await user.click(screen.getByRole("button", { name: "Change vault settings" }));

    expect(screen.getByRole("heading", { name: "Message Vault Settings" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Address" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test" })).toBeInTheDocument();
    // The settings screen replaces the card body rather than opening beside it.
    expect(screen.queryByRole("tab", { name: "Login" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
  });

  it("reports what Test found for the typed address", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    await user.click(screen.getByRole("button", { name: "Change vault settings" }));

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const field = screen.getByRole("textbox", { name: "Address" });
    await user.clear(field);
    await user.type(field, "http://127.0.0.1:9999");
    await user.click(screen.getByRole("button", { name: "Test" }));

    expect(await screen.findByText("Disconnected")).toBeInTheDocument();
    // Testing does not commit the address: the card is still connected behind.
    expect(setServer).not.toHaveBeenCalledWith("http://127.0.0.1:9999");
  });

  it("applies a typed address and reconnects", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const user = userEvent.setup();
    renderScreen();

    await screen.findByText("Disconnected");
    await user.click(screen.getByRole("button", { name: "Change vault settings" }));

    stubVault();
    const field = screen.getByRole("textbox", { name: "Address" });
    await user.clear(field);
    await user.type(field, "http://127.0.0.1:8080");
    await user.click(screen.getByRole("button", { name: "Change vault address" }));

    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
    await waitFor(() => {
      expect(setServer).toHaveBeenCalledWith("http://127.0.0.1:8080");
    });
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

    await screen.findByText("Disconnected");

    healthy = true;

    expect(
      await screen.findByRole("tab", { name: "Login" }, { timeout: 3000 }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Connected")).toBeInTheDocument();
  });

  it("carries an abort signal on the health probe", async () => {
    const fetchMock = stubVault();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    const healthCall = fetchMock.mock.calls.find(([url]) => String(url).endsWith("/health"));
    expect(healthCall).toBeDefined();
    expect(healthCall?.[1]).toMatchObject({ signal: expect.any(AbortSignal) });
  });
});
