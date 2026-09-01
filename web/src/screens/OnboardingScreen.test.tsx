/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
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

vi.mock("../lib/vaultApi", () => ({
  updateAccountProfile: (...args: unknown[]) => apiPost(...(args as [])),
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

    await user.type(rowValue(1), "+1 555-123-4567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.getByRole("button", { name: "Remove account 1" })).toBeInTheDocument();
    expect(rowValue(2)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove account 2" }));
    expect(screen.queryByRole("button", { name: "Remove account 1" })).not.toBeInTheDocument();
  });

  it("stops at five accounts and points at Settings", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    // Each row has to hold a distinct account before the next one can be added.
    for (let i = 0; i < 4; i++) {
      await user.type(rowValue(i + 1), `+1 555-123-45${60 + i}`);
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

  it("will not offer to add a row while the one above it is empty", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    const add = screen.getByRole("button", { name: "+ Add account" });
    expect(add).toBeDisabled();

    await user.type(rowValue(1), "+1 555-123-4567");
    expect(add).toBeEnabled();

    await user.clear(rowValue(1));
    expect(add).toBeDisabled();
  });

  it("refuses an account already in the list and blames the later row", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "+1 555-123-4567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    // The same number, typed the other way — still the same account.
    await user.type(rowValue(2), "+15551234567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));

    expect(screen.getByText("This account is already in the list.")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Account 3 value" })).not.toBeInTheDocument();
    // The first row to carry the number is not the mistake; the repeat is.
    expect(rowValue(2)).toHaveAttribute("aria-invalid", "true");
    expect(rowValue(1)).not.toHaveAttribute("aria-invalid", "true");
  });

  it("allows the same number on two different services", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "+1 555-123-4567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    await user.click(screen.getByRole("button", { name: "Text message Account 2 type" }));
    await user.click(screen.getByRole("option", { name: "WhatsApp" }));
    await user.type(rowValue(2), "+1 555-123-4567");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));

    expect(screen.queryByText("This account is already in the list.")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Account 3 value" })).toBeInTheDocument();
  });

  it("clears a repeated message before showing it again, so the recheck is visible", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    const message = "Enter a phone number like +1 555-123-4567.";
    await user.type(rowValue(1), "notaphone");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.getByText(message)).toBeInTheDocument();

    // Far enough after the first click to be a second look rather than the
    // blur-then-press pair that a single click produces.
    await new Promise((resolve) => setTimeout(resolve, 200));

    // The same words landing again would otherwise look like nothing happened.
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.queryByText(message)).not.toBeInTheDocument();

    await waitFor(() => expect(screen.getByText(message)).toBeInTheDocument());
  });

  it("swaps straight to a different message without blanking first", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.type(rowValue(1), "notaphone");
    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.getByText("Enter a phone number like +1 555-123-4567.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Text message Account 1 type" }));
    await user.click(screen.getByRole("option", { name: "Email" }));
    await user.click(screen.getByRole("button", { name: "+ Add account" }));

    expect(screen.getByText("Enter an email address like you@example.com.")).toBeInTheDocument();
  });

  it("goes back one screen, to login", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.click(screen.getByRole("button", { name: "Back to login" }));
    expect(logout).toHaveBeenCalledOnce();
  });
});
