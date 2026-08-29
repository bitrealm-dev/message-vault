/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

vi.mock("../lib/useAccountProfile", () => ({
  useAccountProfile: () => ({ profile: profileState.profile }),
}));

vi.mock("../lib/auth", () => ({
  useAuth: () => ({ updateToken: vi.fn() }),
}));

const apiGet = vi.hoisted(() => vi.fn());
vi.mock("../lib/api", () => ({
  apiClient: {
    get: (...args: unknown[]) => apiGet(...args),
    post: vi.fn(),
    patch: vi.fn(),
    delete: vi.fn(),
  },
}));

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  apiGet.mockReset();
  apiGet.mockResolvedValue({ items: [] });
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
    <MemoryRouter initialEntries={initialEntries}>
      <SettingsScreen />
    </MemoryRouter>,
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
    expect(apiGet).not.toHaveBeenCalledWith("/v1/admin/users", expect.anything());
  });

  it("shows the Users tab and panel to an administrator arriving at ?tab=users", async () => {
    profileState.profile = baseProfile({ is_admin: true });
    renderSettings(["/settings?tab=users"]);

    expect(screen.getByRole("tab", { name: "Users" })).toHaveAttribute("aria-selected", "true");
    await waitFor(() => {
      expect(apiGet).toHaveBeenCalledWith("/v1/admin/users", expect.anything());
    });
    expect(screen.getByText(/Everyone with an account on this vault/)).toBeInTheDocument();
  });
});
