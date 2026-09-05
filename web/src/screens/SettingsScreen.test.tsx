/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockedAuth, VaultProviders } from "../test/vaultProviders";
import SettingsScreen from "./SettingsScreen";

/**
 * The Users tab is the highest-risk piece of Task 10: it exists only for
 * administrators, and `?tab=users` must not let a non-admin land on a panel
 * that will 403. This is the regression net for that gate — if a future edit
 * ever drops the `isAdmin` check from either the tab list or
 * `tabFromSearchParam`'s allowed-tabs argument, these tests go red.
 */

const profileState = vi.hoisted(() => ({
  profile: null as { username: string; is_admin?: boolean } | null,
}));

const tauriState = vi.hoisted(() => ({ isTauri: false }));

vi.mock("../lib/useAccountProfile", () => ({
  useAccountProfile: () => ({ profile: profileState.profile }),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => tauriState.isTauri,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../lib/auth", () => ({
  useAuth: () => ({ ...mockedAuth, updateToken: vi.fn() }),
}));

const apiGet = vi.hoisted(() => vi.fn());
vi.mock("../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/vaultApi")>()),
  listUsers: (...args: unknown[]) => apiGet(...args),
}));

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  apiGet.mockReset();
  apiGet.mockResolvedValue({ items: [] });
  tauriState.isTauri = false;
});

function baseProfile(overrides: Partial<{ is_admin: boolean }>) {
  return {
    account_id: "a1",
    username: "bob",
    preferred_name: null,
    phones: [],
    emails: [],
    can_import: true,
    can_export: true,
    can_delete: false,
    ...overrides,
  };
}

function renderSettings(initialEntries: string[]) {
  return render(
    <VaultProviders>
      <MemoryRouter initialEntries={initialEntries}>
        <SettingsScreen />
      </MemoryRouter>
    </VaultProviders>,
  );
}

describe("SettingsScreen admin gate", () => {
  it("hides the Users tab and panel from a non-administrator", () => {
    profileState.profile = baseProfile({ is_admin: false });
    renderSettings(["/settings"]);

    expect(screen.queryByRole("tab", { name: "Users" })).not.toBeInTheDocument();
    expect(screen.queryByText(/Everyone with an account on this vault/)).not.toBeInTheDocument();
  });

  it("falls a non-admin arriving at ?tab=users back to the account tab", () => {
    profileState.profile = baseProfile({ is_admin: false });
    renderSettings(["/settings?tab=users"]);

    // The tab itself must not exist for a non-admin...
    expect(screen.queryByRole("tab", { name: "Users" })).not.toBeInTheDocument();
    // ...and the selection must have fallen back to Account, not simply
    // failed to render anything.
    expect(screen.getByRole("tab", { name: "Account" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByText(/Everyone with an account on this vault/)).not.toBeInTheDocument();
    // Never called — a 403 was avoided, not just hidden after the fact.
    expect(apiGet).not.toHaveBeenCalled();
  });

  it("shows the Users tab and panel to an administrator arriving at ?tab=users", async () => {
    profileState.profile = baseProfile({ is_admin: true });
    renderSettings(["/settings?tab=users"]);

    expect(screen.getByRole("tab", { name: "Users" })).toHaveAttribute("aria-selected", "true");
    // Wait for the panel itself, not merely for the request to go out.
    expect(await screen.findByText(/Everyone with an account on this vault/)).toBeInTheDocument();
    expect(apiGet).toHaveBeenCalled();
  });
});

/**
 * Convert runs `message-reexport` inside the desktop process, so the tab is a
 * desktop-only tool. In a browser the tab must not exist and `?tab=convert`
 * must fall back to Account, the same way the admin gate treats Users.
 */
describe("SettingsScreen convert gate", () => {
  it("hides the Convert tab in the browser and falls ?tab=convert back to Account", () => {
    profileState.profile = baseProfile({ is_admin: false });
    renderSettings(["/settings?tab=convert"]);

    expect(screen.queryByRole("tab", { name: "Convert" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Account" })).toHaveAttribute("aria-selected", "true");
    expect(
      screen.getByText(/Manage your account, profile, storage, system, and appearance\./),
    ).toBeInTheDocument();
  });

  it("shows the Convert tab and tool in the desktop app", () => {
    tauriState.isTauri = true;
    profileState.profile = baseProfile({ is_admin: false });
    renderSettings(["/settings?tab=convert"]);

    expect(screen.getByRole("tab", { name: "Convert" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByLabelText("Input folder")).toBeInTheDocument();
    expect(screen.getByLabelText("Output folder")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();
    expect(
      screen.getByText(/Manage your account, profile, storage, system, convert, and appearance\./),
    ).toBeInTheDocument();
  });
});
