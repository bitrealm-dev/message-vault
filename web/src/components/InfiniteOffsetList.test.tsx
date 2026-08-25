/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import InfiniteOffsetList from "./InfiniteOffsetList";

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

afterEach(() => {
  cleanup();
});

type Item = { id: string; name: string };

function renderList(items: Item[]) {
  return render(
    <div style={{ height: 400 }}>
      <InfiniteOffsetList
        items={items}
        total={items.length}
        loading={false}
        filling={false}
        error=""
        hasMore={false}
        requestMore={() => {}}
        estimateSize={49}
        getId={(c) => c.id}
        onSelect={() => {}}
        renderRow={(c) => <span>{c.name}</span>}
        ariaLabel="Contacts"
        getSectionLetter={(c) => c.name.charAt(0).toUpperCase()}
      />
    </div>,
  );
}

describe("InfiniteOffsetList range pill", () => {
  it("shows the floating range pill when contacts are loaded", () => {
    renderList([
      { id: "1", name: "Alice" },
      { id: "2", name: "Bob" },
    ]);
    expect(screen.getByText(/of 2/)).toBeInTheDocument();
  });

  it("hides the range pill when the list is empty", () => {
    renderList([]);
    expect(screen.queryByText(/of /)).not.toBeInTheDocument();
  });
});
