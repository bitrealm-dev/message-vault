/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import LeftPanel from "./LeftPanel";

vi.mock("../lib/useAccountProfile", () => ({
  useAccountProfile: () => ({ profile: null }),
}));

vi.mock("../lib/useContactGroups", () => ({
  useContactGroups: () => ({ groups: [] }),
}));

vi.mock("../lib/useThreadTags", () => ({
  useThreadTags: () => ({ tags: [] }),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  localStorage.clear();
});

function renderPanel() {
  return render(
    <MemoryRouter>
      <LeftPanel onSearchChange={() => {}} onSearch={() => {}} />
    </MemoryRouter>,
  );
}

describe("LeftPanel", () => {
  it("puts browse icons in the shared 15px leading slot", () => {
    renderPanel();
    const messages = screen.getByRole("button", { name: "Messages" });
    expect(messages.querySelector('[class*="size-[15px]"]')).not.toBeNull();
    expect(messages.className).not.toContain("pl-[calc(15px+0.5rem)]");
  });

  it("lines empty saved-search copy up with the heading title slot", () => {
    renderPanel();
    const empty = screen.getByText("No saved searches");
    const row = empty.parentElement;
    expect(row?.querySelector('[class*="size-[15px]"]')).not.toBeNull();
    expect(row?.className).not.toContain("pl-[calc(15px+0.5rem)]");
  });

  it("indents named saved searches like nested group rows", () => {
    localStorage.setItem(
      "mv-saved-groups",
      JSON.stringify([{ id: "g1", name: "From Alice", query: "from:alice" }]),
    );
    renderPanel();
    const alice = screen.getByRole("button", { name: "From Alice" });
    expect(alice.className).toContain("pl-[calc(15px+0.5rem)]");
    expect(alice.className).toContain("self-stretch");
    expect(alice.querySelector('[class*="size-[15px]"]')).not.toBeNull();
  });

  it("closes the saved-search options menu on Escape", async () => {
    localStorage.setItem(
      "mv-saved-groups",
      JSON.stringify([{ id: "g1", name: "From Alice", query: "from:alice" }]),
    );
    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole("button", { name: "Saved search options for From Alice" }));
    expect(screen.getByRole("button", { name: "Rename…" })).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "Rename…" })).toBeNull();
  });
});
