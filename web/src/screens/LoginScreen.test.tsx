/** @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

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
  });
});
