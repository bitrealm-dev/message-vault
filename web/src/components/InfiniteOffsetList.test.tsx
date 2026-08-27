/** @vitest-environment jsdom */

import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import InfiniteOffsetList from "./InfiniteOffsetList";

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

afterEach(() => {
  cleanup();
});

type Item = { id: string; name: string };

function renderList(items: Item[], extra?: { loading?: boolean; filling?: boolean }) {
  return render(
    <div style={{ height: 400 }}>
      <InfiniteOffsetList
        items={items}
        total={items.length}
        loading={extra?.loading ?? false}
        filling={extra?.filling ?? false}
        error=""
        hasMore={false}
        requestMore={() => {}}
        estimateSize={49}
        getId={(c) => c.id}
        onSelect={() => {}}
        onSelectAllChange={() => {}}
        selectAllLabel="Select all contacts"
        renderRow={(c) => <span>{c.name}</span>}
        ariaLabel="Contacts"
        getSectionLetter={(c) => c.name.charAt(0).toUpperCase()}
      />
    </div>,
  );
}

describe("InfiniteOffsetList range pill", () => {
  it("shows the floating range pill outside the toolbar", () => {
    renderList([
      { id: "1", name: "Alice" },
      { id: "2", name: "Bob" },
    ]);
    const pill = screen.getByTestId("contact-list-range-pill");
    expect(pill).toHaveTextContent("of 2");
    expect(pill).not.toHaveAttribute("aria-live");

    const toolbar = screen.getByRole("checkbox", { name: "Select all contacts" }).closest("div");
    expect(toolbar).toBeTruthy();
    expect(within(toolbar as HTMLElement).queryByText(/of 2/)).not.toBeInTheDocument();
  });

  it("appends loading-more on the pill, not the toolbar", () => {
    renderList(
      [
        { id: "1", name: "Alice" },
        { id: "2", name: "Bob" },
      ],
      { filling: true },
    );
    const pill = screen.getByTestId("contact-list-range-pill");
    expect(pill).toHaveTextContent(/loading more/);
    const toolbar = screen.getByRole("checkbox", { name: "Select all contacts" }).closest("div");
    expect(within(toolbar as HTMLElement).queryByText(/loading more/)).not.toBeInTheDocument();
  });

  it("keeps Loading… in the toolbar when the list is still empty", () => {
    renderList([], { loading: true });
    expect(screen.queryByTestId("contact-list-range-pill")).not.toBeInTheDocument();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("hides the range pill when the list is empty", () => {
    renderList([]);
    expect(screen.queryByTestId("contact-list-range-pill")).not.toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
  });
});
