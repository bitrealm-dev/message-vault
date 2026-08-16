/** @vitest-environment jsdom */
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ListRangeHeader from "./ListRangeHeader";

describe("ListRangeHeader", () => {
  it("shows the range label", () => {
    render(<ListRangeHeader rangeLabel="1–20 of 100" />);
    expect(screen.getByText("1–20 of 100")).toBeInTheDocument();
  });

  it("appends updating and loading suffixes", () => {
    const { rerender } = render(
      <ListRangeHeader rangeLabel="1–20 of 100" refreshing />,
    );
    expect(screen.getByText(/updating…/)).toBeInTheDocument();

    rerender(<ListRangeHeader rangeLabel="1–20 of 100" filling />);
    expect(screen.getByText(/loading more…/)).toBeInTheDocument();
  });

  it("toggles select-all from the header checkbox", async () => {
    const onSelectAllChange = vi.fn();
    const user = userEvent.setup();
    render(
      <ListRangeHeader
        rangeLabel="1–20 of 100"
        onSelectAllChange={onSelectAllChange}
        selectAllLabel="Select all contacts"
      />,
    );
    await user.click(screen.getByRole("checkbox", { name: "Select all contacts" }));
    expect(onSelectAllChange).toHaveBeenCalledWith(true);
  });
});
