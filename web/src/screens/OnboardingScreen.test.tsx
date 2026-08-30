/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const logout = vi.fn();
const apiPost = vi.fn(async () => ({}));

vi.mock("../lib/auth", () => ({
  useAuth: () => ({
    login: vi.fn(),
    logout,
    token: "t",
    serverUrl: "",
    accountId: "acct",
  }),
}));

vi.mock("../lib/api", () => ({
  apiClient: {
    post: (...args: unknown[]) => apiPost(...(args as [])),
  },
}));

import OnboardingScreen from "./OnboardingScreen";

const rowValue = (n: number) => screen.getByRole("textbox", { name: `Account ${n} value` });

describe("OnboardingScreen", () => {
  beforeEach(() => {
    logout.mockReset();
    apiPost.mockReset();
  });

  afterEach(cleanup);

  it("names the section Your Accounts and explains nothing further", () => {
    render(<OnboardingScreen />);

    expect(screen.getByText("Your Accounts")).toBeInTheDocument();
    expect(screen.queryByText(/How you show up/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Source Accounts/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Welcome to the Message Vault/i)).not.toBeInTheDocument();
  });

  it("shows an example in the empty value field", () => {
    render(<OnboardingScreen />);
    expect(rowValue(1)).toHaveAttribute("placeholder", "+1 555-123-4567");
  });

  it("changes the placeholder when the service picker changes", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.click(screen.getByRole("button", { name: "Text message Account 1 type" }));
    await user.click(screen.getByRole("option", { name: "Email" }));

    expect(rowValue(1)).toHaveAttribute("placeholder", "you@example.com");
  });

  it("hides the remove control until there is more than one row", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    expect(screen.queryByRole("button", { name: "Remove account 1" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.getByRole("button", { name: "Remove account 1" })).toBeInTheDocument();
    expect(rowValue(2)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove account 2" }));
    expect(screen.queryByRole("button", { name: "Remove account 1" })).not.toBeInTheDocument();
  });

  it("stops at five accounts and points at Settings", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    for (let i = 0; i < 4; i++) {
      await user.click(screen.getByRole("button", { name: "+ Add account" }));
    }

    expect(rowValue(5)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "+ Add account" })).not.toBeInTheDocument();
    expect(screen.getByText("Add the rest in Settings after setup.")).toBeInTheDocument();
  });

  it("will not add a row on top of a value that is not an account", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "notaphone");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));

    // The row is refused, marked, and said out loud — all three, or the person
    // is left guessing why nothing happened.
    expect(screen.queryByRole("textbox", { name: "Account 2 value" })).not.toBeInTheDocument();
    expect(rowValue(1)).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Enter a phone number like +1 555-123-4567.")).toBeInTheDocument();
  });

  it("adds the row once the value is corrected", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "notaphone");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.queryByRole("textbox", { name: "Account 2 value" })).not.toBeInTheDocument();

    await user.clear(rowValue(1));
    await user.type(rowValue(1), "+1 555-123-4567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));

    expect(screen.getByRole("textbox", { name: "Account 2 value" })).toBeInTheDocument();
    expect(rowValue(1)).not.toHaveAttribute("aria-invalid", "true");
  });

  it("checks a value when the field is left", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "notaphone");
    await user.click(screen.getByRole("textbox", { name: "Display Name" }));

    expect(rowValue(1)).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Enter a phone number like +1 555-123-4567.")).toBeInTheDocument();
  });

  it("keeps the mark on the row that earned it when another is removed", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "+1 555-123-4567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    await user.type(rowValue(2), "notaphone");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(rowValue(2)).toHaveAttribute("aria-invalid", "true");

    // Row 1 goes, so the bad value is row 1 now. The mark has to move with the
    // value, not stay on the position it was first found at.
    await user.click(screen.getByRole("button", { name: "Remove account 1" }));

    expect(rowValue(1)).toHaveValue("notaphone");
    expect(rowValue(1)).toHaveAttribute("aria-invalid", "true");
  });

  it("holds back Continue to vault until the value is an account", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(screen.getByRole("textbox", { name: "Display Name" }), "Matt");
    await user.type(rowValue(1), "notaphone");
    await user.click(screen.getByRole("button", { name: "Continue to vault" }));

    expect(apiPost).not.toHaveBeenCalled();
    expect(rowValue(1)).toHaveAttribute("aria-invalid", "true");
  });

  it("keeps Continue to vault disabled until there is a name and an account", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    const submit = screen.getByRole("button", { name: "Continue to vault" });
    expect(submit).toBeDisabled();

    await user.type(screen.getByRole("textbox", { name: "Display Name" }), "Matt");
    expect(submit).toBeDisabled();

    await user.type(rowValue(1), "+1 555-123-4567");
    expect(submit).toBeEnabled();
  });

  it("goes back one screen, to login", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.click(screen.getByRole("button", { name: "Back to login" }));
    expect(logout).toHaveBeenCalledOnce();
  });
});
